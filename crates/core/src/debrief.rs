//! The debrief: what the person's agent did, how it works and what it
//! decided, written by the agent as one markdown file (`kelve: debrief/1`),
//! because markdown is what agents write best. kelve reads it into its
//! parts, checks it before it shows it, and adds the actions on top.
//!
//! Also the answers to the two jobs a debrief leads to: research on a
//! decision, and an explanation of an idea, both markdown in a fixed shape.
//!
//! Every problem is a sentence the agent can act on: a debrief that fails
//! goes back to it once, with them.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::md;
use crate::text::re;

/// What the front matter's `kelve:` says of a debrief in this format.
pub const FORMAT: &str = "debrief/1";
/// The longest title, in characters.
pub const MOST_TITLE: usize = 100;
/// The most words in research's short answer, before its table.
pub const MOST_ANSWER_WORDS: usize = 80;
/// The most words in research as a whole, outside code.
pub const MOST_RESEARCH_WORDS: usize = 300;
/// The most ways research compares.
pub const MOST_OPTIONS: usize = 6;
/// The most words in an explanation, outside code: about 200, with room
/// for a sentence over.
pub const MOST_EXPLAIN_WORDS: usize = 230;

/// Where a decision came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Origin {
    /// The agent's own call: nothing told it how.
    Own,
    /// What the person said while it worked.
    Said,
    /// The person's edit after it handed over.
    Edit,
}

impl Origin {
    pub const ALL: [Origin; 3] = [Origin::Own, Origin::Said, Origin::Edit];

    /// How the debrief writes it, after `from:`.
    pub fn words(self) -> &'static str {
        match self {
            Origin::Own => "its own call",
            Origin::Said => "what you said",
            Origin::Edit => "your edit",
        }
    }

    /// The origin `from:` names, written as `words` gives it, in any case.
    pub fn of(words: &str) -> Option<Origin> {
        let words = words.trim().trim_end_matches('.').trim().to_lowercase();
        Origin::ALL.into_iter().find(|o| o.words() == words)
    }
}

/// A run of lines, both ends included, from 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "LineRange")]
pub struct Range {
    pub from: u32,
    pub to: u32,
}

impl Range {
    pub fn new(from: u32, to: u32) -> Range {
        Range { from, to }
    }

    fn overlaps(self, other: Range) -> bool {
        self.from <= other.to && other.from <= self.to
    }
}

/// Lines in one file: where a decision is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lines {
    pub path: String,
    /// Its runs of lines; none means the file as a whole.
    pub ranges: Vec<Range>,
}

/// A file the change touched, with the lines it added or changed, on the
/// file's new side: what a decision's lines are checked against. A file
/// with none, such as one deleted, takes any lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changed {
    pub path: String,
    pub ranges: Vec<Range>,
}

/// One decision in the work, as the agent wrote it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// What it chose, as a sentence: its `###` heading.
    pub title: String,
    pub from: Origin,
    /// Where it is in the change.
    pub lines: Vec<Lines>,
    /// The other ways it could have gone.
    pub could_have: Vec<String>,
    /// The ideas it rests on, by name.
    pub rests_on: Vec<String>,
    /// On a large change, the part of the change it belongs to.
    pub part: Option<String>,
    /// Why, in markdown.
    pub why: String,
    /// The line its heading is on, from 1.
    pub line: usize,
}

/// Whether the agent followed a part of the person's skills.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Followed {
    Followed,
    NotFollowed,
    /// It didn't come up in this work.
    DidntApply,
}

impl Followed {
    fn of(words: &str) -> Option<Followed> {
        let w = words
            .trim()
            .trim_end_matches('.')
            .trim()
            .to_lowercase()
            .replace('’', "'");
        match w.as_str() {
            "followed" | "followed it" | "held" => Some(Followed::Followed),
            "not followed" | "didn't follow" | "did not follow" | "didn't hold" => {
                Some(Followed::NotFollowed)
            }
            "didn't apply" | "did not apply" | "not applicable" => Some(Followed::DidntApply),
            _ => None,
        }
    }
}

/// A part of the person's skills the work used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Used {
    /// The skill, by its name.
    pub skill: String,
    /// The part, by its heading.
    pub part: String,
    pub followed: Followed,
}

/// An idea the work rests on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Idea {
    pub name: String,
    /// What it is, in a line.
    pub what: String,
}

/// A debrief, read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Debrief {
    pub title: String,
    /// What it did, in markdown.
    pub did: String,
    /// How it works, in markdown.
    pub how: String,
    pub decisions: Vec<Decision>,
    pub used: Vec<Used>,
    pub rests_on: Vec<Idea>,
}

/// The sections a debrief has, in order.
const DID: &str = "What it did";
const HOW: &str = "How it works";
const DECISIONS: &str = "Decisions";
const USED: &str = "What it used from your skills";
const RESTS_ON: &str = "What it rests on";
const SECTIONS: [&str; 5] = [DID, HOW, DECISIONS, USED, RESTS_ON];

/// A section's heading, as kelve compares them.
fn plain(heading: &str) -> String {
    heading
        .trim()
        .trim_end_matches(['.', ':'])
        .to_lowercase()
        .replace('’', "'")
}

static FIELD: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s{0,3}[-*+]\s+([A-Za-z][A-Za-z ]{0,20}):\s*(.*)$"));
static RANGE: LazyLock<Regex> = LazyLock::new(|| re(r"^(\d+)(?:\s*[-–—]\s*(\d+))?$"));
static IDEA: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s{0,3}[-*+]\s+(?:\*\*([^*]+?)[:.]?\*\*[:.]?|([^:*][^:]*?):)\s*(.*)$"));
static ITEM: LazyLock<Regex> = LazyLock::new(|| re(r"^\s{0,3}(?:[-*+]|\d+[.)])\s+(.*)$"));
static WEB_LINK: LazyLock<Regex> = LazyLock::new(|| re(r"\[([^\]]+)\]\((https?://[^)\s]+)\)"));

/// A debrief's lines in one file, read from `src/fetch.rs 59-66, 70`: a
/// path, then its runs of lines. `src/fetch.rs:59-66` reads too.
fn lines_in(text: &str) -> Option<Lines> {
    let text = text.trim().trim_end_matches('.');
    let mut tokens: Vec<&str> = text.split_whitespace().collect();
    let mut ranges = Vec::new();
    // Runs of lines, read back from the end: `59-66,` then `70`.
    while tokens.len() > 1 {
        let last = tokens[tokens.len() - 1];
        let found: Option<Vec<Range>> = last
            .split(',')
            .filter(|p| !p.is_empty())
            .map(range)
            .collect();
        match found {
            Some(found) if !found.is_empty() => {
                ranges.splice(0..0, found);
                tokens.pop();
            }
            _ => break,
        }
    }
    let mut path = tokens.join(" ");
    if ranges.is_empty()
        && let Some((p, r)) = path.clone().rsplit_once(':')
        && let Some(found) = r
            .split(',')
            .filter(|p| !p.is_empty())
            .map(range)
            .collect::<Option<Vec<Range>>>()
        && !found.is_empty()
    {
        ranges = found;
        path = p.to_string();
    }
    let path = path
        .trim()
        .trim_matches('`')
        .trim_start_matches("./")
        .to_string();
    (!path.is_empty() && !path.contains(char::is_whitespace)).then_some(Lines { path, ranges })
}

/// A run of lines, from `59-66`, `59–66` or `59`.
fn range(text: &str) -> Option<Range> {
    let c = RANGE.captures(text.trim())?;
    let from: u32 = c.get(1)?.as_str().parse().ok()?;
    let to: u32 = match c.get(2) {
        Some(m) => m.as_str().parse().ok()?,
        None => from,
    };
    Some(Range { from, to })
}

/// A list's items, from a field's value: split on semicolons, or commas.
fn list(value: &str, by: char) -> Vec<String> {
    value
        .split(by)
        .map(|v| v.trim().trim_end_matches('.').trim().to_string())
        .filter(|v| !v.is_empty())
        .collect()
}

/// A decision as it's read, before its fields are checked.
struct Draft {
    title: String,
    line: usize,
    fields: Vec<(String, String, usize)>,
    why: Vec<String>,
    in_fields: bool,
}

/// Reads a debrief and checks it against the change it's about: its
/// files, with the lines each added or changed. The debrief, when it
/// passes; else every problem, as sentences the agent can act on.
pub fn read(text: &str, change: &[Changed]) -> Result<Debrief, Vec<String>> {
    read_against(text, Some(change))
}

/// Reads a debrief kelve kept: it passed its checks as it was handed in, so
/// its lines aren't checked again, as the work may have moved on since.
pub fn read_kept(text: &str) -> Result<Debrief, Vec<String>> {
    read_against(text, None)
}

fn read_against(text: &str, change: Option<&[Changed]>) -> Result<Debrief, Vec<String>> {
    let mut problems = Vec::new();
    let Some(front) = md::front_matter(text) else {
        return Err(vec![format!(
            "The debrief has no front matter. Start it with `---`, then `kelve: {FORMAT}` and `title:`, then `---`."
        )]);
    };
    match front.get("kelve") {
        Some(f) if f.trim() == FORMAT => {}
        Some(f) => problems.push(format!(
            "The front matter says `kelve: {f}`. A debrief says `kelve: {FORMAT}`."
        )),
        None => problems.push(format!(
            "The front matter has no `kelve: {FORMAT}`. Add it."
        )),
    }
    let title = front.get("title").unwrap_or_default().trim().to_string();
    if title.is_empty() {
        problems.push(
            "The front matter has no `title:`. Add one: what the work did, in a few words.".into(),
        );
    } else if title.chars().count() > MOST_TITLE {
        problems.push(format!(
            "The title is {} characters long, and the most is {MOST_TITLE}. Shorten it.",
            title.chars().count()
        ));
    }

    let body = &text[front.len..];
    let offset = front.lines;
    problems.extend(md::problems(body, offset + 1));

    let mut section: Option<&'static str> = None;
    let mut seen: HashSet<&'static str> = HashSet::new();
    let mut did = Vec::new();
    let mut how = Vec::new();
    let mut drafts: Vec<Draft> = Vec::new();
    let mut used = Vec::new();
    let mut ideas = Vec::new();

    for line in md::lines(body) {
        let n = line.number + offset;
        let heading = (!line.code).then(|| md::heading(line.text)).flatten();
        match heading {
            Some((1, _)) => {
                problems.push(format!(
                    "Line {n} is a `#` heading. The title goes in the front matter, and sections start with `##`."
                ));
                continue;
            }
            Some((2, words)) => {
                match SECTIONS.iter().find(|s| plain(s) == plain(words)) {
                    Some(s) => {
                        if !seen.insert(s) {
                            problems.push(format!(
                                "“{s}” is in the debrief twice, the second time on line {n}. Put it all under one heading."
                            ));
                        }
                        section = Some(s);
                    }
                    None => {
                        problems.push(format!(
                            "Line {n} starts a section kelve doesn't know, “{words}”. Use only: {}.",
                            SECTIONS.join(", ")
                        ));
                        // What's under it is said once, here.
                        section = Some("");
                    }
                }
                continue;
            }
            Some((3, words)) if section == Some(DECISIONS) => {
                drafts.push(Draft {
                    title: words.to_string(),
                    line: n,
                    fields: Vec::new(),
                    why: Vec::new(),
                    in_fields: true,
                });
                continue;
            }
            _ => {}
        }
        match section {
            None => {
                if !line.text.trim().is_empty() {
                    problems.push(format!(
                        "Line {n} isn't under a section. Start with `## {DID}`."
                    ));
                    // Say it once.
                    section = Some("");
                }
            }
            Some(DID) => did.push(line.text),
            Some(HOW) => how.push(line.text),
            Some(DECISIONS) => match drafts.last_mut() {
                None => {
                    if !line.text.trim().is_empty() {
                        problems.push(format!(
                            "Line {n}, under Decisions, isn't in a decision. Start each decision with a `###` heading: what it chose, as a sentence."
                        ));
                    }
                }
                Some(d) => {
                    let blank = line.text.trim().is_empty();
                    if d.in_fields && !line.code {
                        if let Some(c) = FIELD.captures(line.text) {
                            let key = c.get(1).map_or("", |m| m.as_str()).trim().to_lowercase();
                            let value = c.get(2).map_or("", |m| m.as_str()).trim().to_string();
                            d.fields.push((key, value, n));
                            continue;
                        }
                        if blank && d.fields.is_empty() {
                            continue;
                        }
                        d.in_fields = false;
                    }
                    d.why.push(line.text.to_string());
                }
            },
            Some(USED) => {
                if line.text.trim().is_empty() {
                    continue;
                }
                match used_item(line.text) {
                    Some(u) => used.push(u),
                    None => problems.push(format!(
                        "Line {n} doesn't say a skill, its part and whether it was followed. Write it as `- rust: Errors are one enum: followed`, or `not followed`, or `didn't apply`."
                    )),
                }
            }
            Some(RESTS_ON) => {
                if line.text.trim().is_empty() {
                    continue;
                }
                match idea_item(line.text) {
                    Some(i) => ideas.push(i),
                    None => problems.push(format!(
                        "Line {n} doesn't name an idea and say what it is. Write it as `- **Exponential backoff:** waiting longer after each failure.`"
                    )),
                }
            }
            Some(_) => {}
        }
    }

    let did = joined(&did);
    let how = joined(&how);
    if did.is_empty() {
        problems.push(format!(
            "The debrief doesn't say what the work did. Add `## {DID}`, with a few sentences."
        ));
    }
    if how.is_empty() {
        problems.push(format!(
            "The debrief doesn't say how it works. Add `## {HOW}`: one sentence for a small change, or a step for each part."
        ));
    }

    let mut decisions = Vec::new();
    for (i, d) in drafts.into_iter().enumerate() {
        let name = format!("Decision {}, “{}”,", i + 1, crate::text::clip(&d.title, 50));
        match decision(d, &name, change) {
            Ok(decision) => decisions.push(decision),
            Err(found) => problems.extend(found),
        }
    }

    if problems.is_empty() {
        Ok(Debrief {
            title,
            did,
            how,
            decisions,
            used,
            rests_on: ideas,
        })
    } else {
        Err(problems)
    }
}

/// Lines joined again, without the blank lines around them.
fn joined(lines: &[&str]) -> String {
    lines.join("\n").trim_matches('\n').trim_end().to_string()
}

/// A decision's fields, read and checked.
fn decision(d: Draft, name: &str, change: Option<&[Changed]>) -> Result<Decision, Vec<String>> {
    let mut problems = Vec::new();
    if d.title.trim().is_empty() {
        problems.push(format!(
            "The decision on line {} has no heading. Say what it chose, as a sentence.",
            d.line
        ));
    }
    let mut keys: HashSet<String> = HashSet::new();
    let mut from = None;
    let mut lines = Vec::new();
    let mut could_have = Vec::new();
    let mut rests_on = Vec::new();
    let mut part = None;
    let mut has_lines = false;
    for (key, value, n) in &d.fields {
        if !keys.insert(key.clone()) {
            problems.push(format!("{name} gives `{key}` twice. Give it once."));
            continue;
        }
        match key.as_str() {
            "from" => match Origin::of(value) {
                Some(o) => from = Some(o),
                None => problems.push(format!(
                    "{name} says it's from “{value}”. `from` is one of: its own call, what you said, your edit."
                )),
            },
            "lines" => {
                has_lines = true;
                for piece in value.split(';').filter(|p| !p.trim().is_empty()) {
                    match lines_in(piece) {
                        Some(l) => lines.push(l),
                        None => problems.push(format!(
                            "{name} has lines on line {n} kelve can't read, “{}”. Write a path, then line ranges: `src/fetch.rs 59-66, 70`, with more files after a semicolon.",
                            piece.trim()
                        )),
                    }
                }
            }
            "could have" => could_have = list(value, ';'),
            "rests on" => rests_on = list(value, ','),
            "part" => part = Some(value.clone()).filter(|v| !v.is_empty()),
            other => problems.push(format!(
                "{name} has a field kelve doesn't know, `{other}`, on line {n}. Use only from, lines, could have, rests on and part."
            )),
        }
    }
    if from.is_none() && !keys.contains("from") {
        problems.push(format!(
            "{name} doesn't say where it came from. Add `- from:` with its own call, what you said or your edit."
        ));
    }
    if !has_lines {
        problems.push(format!(
            "{name} has no lines. Add `- lines:` with the path and the line ranges in the change, such as `src/fetch.rs 59-66`."
        ));
    }
    if let Some(change) = change {
        for l in &lines {
            problems.extend(in_change(l, name, change));
        }
    }
    let why = d.why.join("\n").trim_matches('\n').trim_end().to_string();
    if why.trim().is_empty() {
        problems.push(format!(
            "{name} doesn't say why. Add a sentence or two after its fields."
        ));
    }
    match (problems.is_empty(), from) {
        (true, Some(from)) => Ok(Decision {
            title: d.title,
            from,
            lines,
            could_have,
            rests_on,
            part,
            why,
            line: d.line,
        }),
        _ => Err(problems),
    }
}

/// What's wrong with a decision's lines, against the change.
fn in_change(lines: &Lines, name: &str, change: &[Changed]) -> Vec<String> {
    let Some(file) = change
        .iter()
        .find(|c| c.path.trim_start_matches("./") == lines.path)
    else {
        return vec![format!(
            "{name} has lines in {}, which this change didn't touch. Give lines in the files it changed.",
            lines.path
        )];
    };
    let mut problems = Vec::new();
    for r in &lines.ranges {
        if r.from == 0 || r.from > r.to {
            problems.push(format!(
                "{name} gives lines {}-{} in {}. Give the first line, then the last, counting from 1.",
                r.from, r.to, lines.path
            ));
            continue;
        }
        if !file.ranges.is_empty() && !file.ranges.iter().any(|c| r.overlaps(*c)) {
            let changed: Vec<String> = file.ranges.iter().take(5).map(|c| show(*c)).collect();
            problems.push(format!(
                "{name} gives lines {} in {}, which this change didn't add or change. Its changed lines there are {}.",
                show(*r),
                lines.path,
                changed.join(", ")
            ));
        }
    }
    problems
}

/// A run of lines as a debrief writes it.
fn show(r: Range) -> String {
    if r.from == r.to {
        r.from.to_string()
    } else {
        format!("{}-{}", r.from, r.to)
    }
}

/// A line of "What it used from your skills": `- rust: Errors are one enum: followed`.
fn used_item(line: &str) -> Option<Used> {
    let item = ITEM.captures(line)?.get(1)?.as_str().trim();
    let item = item.replace("**", "");
    let (skill, rest) = item.split_once(':')?;
    let (part, followed) = rest.rsplit_once(':')?;
    let skill = skill.trim().trim_matches('`').to_string();
    let part = part.trim().to_string();
    if skill.is_empty() || part.is_empty() {
        return None;
    }
    Some(Used {
        skill,
        part,
        followed: Followed::of(followed)?,
    })
}

/// A line of "What it rests on": `- **Exponential backoff:** waiting longer…`.
fn idea_item(line: &str) -> Option<Idea> {
    let c = IDEA.captures(line)?;
    let name = c.get(1).or_else(|| c.get(2))?.as_str().trim();
    let what = c.get(3).map_or("", |m| m.as_str()).trim();
    (!name.is_empty() && !what.is_empty()).then(|| Idea {
        name: name.to_string(),
        what: what.to_string(),
    })
}

/// One way research compared.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Way {
    pub name: String,
    /// One thing for it.
    pub for_it: String,
    /// One thing against it.
    pub against: String,
    /// Whether it's the one suggested: its name in bold.
    pub suggested: bool,
}

/// A page research looked at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub name: String,
    pub url: String,
}

/// Research on a decision, read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Research {
    /// The short answer, in markdown.
    pub answer: String,
    pub ways: Vec<Way>,
    /// The one it suggests, as its sentence says it, in markdown.
    pub suggests: String,
    pub sources: Vec<Source>,
    /// The research as written, to render.
    pub markdown: String,
}

const FOUND: &str = "#### What your agent found";

/// A table row's cells.
fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

/// Reads research on a decision, in its fixed shape: a short answer, a
/// table of the ways it compared (`| Option | For | Against |`), the one it
/// suggests (`**It suggests** …`), and where it looked (`Sources: …`).
pub fn research(text: &str) -> Result<Research, Vec<String>> {
    let mut problems = md::problems(text, 1);
    let lines = md::lines(text);
    let body: Vec<&md::Line> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    match body.first() {
        Some(l) if l.text.trim() == FOUND => {}
        _ => problems.push(format!(
            "Research starts with `{FOUND}`. Add it as the first line."
        )),
    }
    let table_at = body
        .iter()
        .position(|l| !l.code && l.text.trim_start().starts_with('|'));
    let answer: Vec<&str> = body
        .iter()
        .skip(1)
        .take_while(|l| l.code || !l.text.trim_start().starts_with('|'))
        .take_while(|l| !l.text.trim_start().starts_with("**It suggests**"))
        .map(|l| l.text)
        .collect();
    let answer = answer.join("\n");
    if answer.trim().is_empty() {
        problems.push("Research gives no short answer. Say what you found, in a sentence or two, before the table.".into());
    } else if md::words(&answer) > MOST_ANSWER_WORDS {
        problems.push(format!(
            "The short answer has {} words, and the most is {MOST_ANSWER_WORDS}. Say what you found in a sentence or two; the table holds the rest.",
            md::words(&answer)
        ));
    }

    let mut ways = Vec::new();
    match table_at {
        None => problems.push("Research has no table of the ways it compared. Add `| Option | For | Against |`, with a row for each way.".into()),
        Some(at) => {
            let rows: Vec<Vec<String>> = body[at..]
                .iter()
                .take_while(|l| !l.code && l.text.trim_start().starts_with('|'))
                .map(|l| cells(l.text))
                .collect();
            let head: Vec<String> = rows.first().map(|r| r.iter().map(|c| plain(c)).collect()).unwrap_or_default();
            if head != ["option", "for", "against"] {
                problems.push("The table's heading row is `| Option | For | Against |`.".into());
            }
            for (i, row) in rows.iter().enumerate().skip(2) {
                let [name, for_it, against] = row.as_slice() else {
                    problems.push(format!(
                        "Row {} of the table has {} cells. Give each way its name, one thing for it and one against.",
                        i - 1,
                        row.len()
                    ));
                    continue;
                };
                if name.is_empty() || for_it.is_empty() || against.is_empty() {
                    problems.push(format!(
                        "Row {} of the table leaves a cell empty. Give each way its name, one thing for it and one against.",
                        i - 1
                    ));
                    continue;
                }
                let bold = name.starts_with("**") && name.ends_with("**") && name.len() > 4;
                ways.push(Way {
                    name: if bold { name[2..name.len() - 2].to_string() } else { name.clone() },
                    for_it: for_it.clone(),
                    against: against.clone(),
                    suggested: bold,
                });
            }
            if ways.len() < 2 && rows.len() >= 2 {
                problems.push("The table compares fewer than two ways. Give at least two, the one there now among them.".into());
            }
            if ways.len() > MOST_OPTIONS {
                problems.push(format!(
                    "The table compares {} ways, and the most is {MOST_OPTIONS}. Keep the ones that matter.",
                    ways.len()
                ));
            }
        }
    }

    let suggests = body
        .iter()
        .find(|l| !l.code && l.text.trim_start().starts_with("**It suggests**"))
        .map(|l| l.text.trim().to_string());
    if suggests.is_none() {
        problems.push(
            "Research doesn't say which way it suggests. Add a line starting `**It suggests**`."
                .into(),
        );
    }

    let sources_line = body
        .iter()
        .find(|l| !l.code && l.text.trim_start().to_lowercase().starts_with("sources:"));
    let sources: Vec<Source> = sources_line
        .map(|l| {
            WEB_LINK
                .captures_iter(l.text)
                .map(|c| Source {
                    name: c.get(1).map_or("", |m| m.as_str()).to_string(),
                    url: c.get(2).map_or("", |m| m.as_str()).to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    if sources.is_empty() {
        problems.push("Research doesn't say where it looked. End with `Sources:` and a link to each page you read, such as `[RFC 9110](https://www.rfc-editor.org/rfc/rfc9110)`.".into());
    }
    if md::words(text) > MOST_RESEARCH_WORDS {
        problems.push(format!(
            "Research has {} words, and the most is {MOST_RESEARCH_WORDS}. Keep to the answer, the table, the suggestion and the sources.",
            md::words(text)
        ));
    }
    match (problems.is_empty(), suggests) {
        (true, Some(suggests)) => Ok(Research {
            answer: answer.trim().to_string(),
            ways,
            suggests,
            sources,
            markdown: text.trim().to_string(),
        }),
        _ => Err(problems),
    }
}

/// Reads an explanation of an idea: markdown, at most about 200 words,
/// using the change as its example.
pub fn explanation(text: &str) -> Result<String, Vec<String>> {
    let mut problems = md::problems(text, 1);
    let words = md::words(text);
    if text.trim().is_empty() {
        problems.push("The explanation is empty. Explain the idea in a short paragraph or two, using this change as the example.".into());
    } else if words > MOST_EXPLAIN_WORDS {
        problems.push(format!(
            "The explanation has {words} words, and the most is about 200. Keep to what the idea is and how this change uses it."
        ));
    }
    for line in md::lines(text) {
        if !line.code && matches!(md::heading(line.text), Some((1..=3, _))) {
            problems.push(format!(
                "Line {} is a heading. An explanation sits under the idea, so leave headings out, or use `####`.",
                line.number
            ));
        }
    }
    if problems.is_empty() {
        Ok(text.trim().to_string())
    } else {
        Err(problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEBRIEF: &str = "---
kelve: debrief/1
title: Retry failed downloads with a growing wait
---

## What it did

When a download fails because of the network or the server, fetch now
tries again, up to 5 times, waiting longer each time.

## How it works

Every download now goes through `with_retries()`, in `src/retry.rs`.
When a request fails, it asks `should_retry()` whether the failure might pass:

- timeouts and dropped connections: **yes**
- anything that says the request is wrong, 4xx: **no**

## Decisions

### Retry only failures that might pass next time
- from: its own call
- lines: src/fetch.rs 59-66
- could have: retried every failure; retried 429 too
- rests on: HTTP status codes, Idempotence

Timeouts, dropped connections and server errors often clear up by
themselves. A 4xx means the request itself is wrong.

### Bad arguments exit with 1
- from: your edit
- lines: src/args.rs 15; src/retry.rs 8-9, 13
- part: The command line

- `1` says fetch was called wrong.
- `2` says fetch itself failed.

## What it used from your skills

- rust: Errors are one enum, with a message for each: followed
- rust: Fix clippy warnings, never quiet them: didn't apply

## What it rests on

- **Exponential backoff:** waiting longer after each failure: 1, 2, 4, 8 seconds.
- **Jitter**: a little randomness added to a wait.
";

    fn change() -> Vec<Changed> {
        vec![
            Changed {
                path: "src/fetch.rs".into(),
                ranges: vec![Range::new(55, 70)],
            },
            Changed {
                path: "src/args.rs".into(),
                ranges: vec![Range::new(15, 15)],
            },
            Changed {
                path: "src/retry.rs".into(),
                ranges: vec![Range::new(1, 69)],
            },
        ]
    }

    #[test]
    fn the_specs_debrief_reads() {
        let d = read(DEBRIEF, &change()).unwrap();
        assert_eq!(d.title, "Retry failed downloads with a growing wait");
        assert!(d.did.starts_with("When a download fails"));
        assert!(d.did.ends_with("waiting longer each time."));
        assert!(
            d.how
                .contains("- anything that says the request is wrong, 4xx: **no**")
        );
        assert_eq!(d.decisions.len(), 2);

        let first = &d.decisions[0];
        assert_eq!(first.title, "Retry only failures that might pass next time");
        assert_eq!(first.from, Origin::Own);
        assert_eq!(
            first.lines,
            vec![Lines {
                path: "src/fetch.rs".into(),
                ranges: vec![Range::new(59, 66)]
            }]
        );
        assert_eq!(
            first.could_have,
            vec!["retried every failure", "retried 429 too"]
        );
        assert_eq!(first.rests_on, vec!["HTTP status codes", "Idempotence"]);
        assert_eq!(first.part, None);
        assert!(first.why.starts_with("Timeouts, dropped"));
        assert!(first.why.ends_with("request itself is wrong."));
        assert_eq!(first.line, 21);

        let second = &d.decisions[1];
        assert_eq!(second.from, Origin::Edit);
        assert_eq!(second.part.as_deref(), Some("The command line"));
        assert_eq!(
            second.lines[1],
            Lines {
                path: "src/retry.rs".into(),
                ranges: vec![Range::new(8, 9), Range::new(13, 13)]
            }
        );
        // A why that's a list, after a blank line, isn't fields.
        assert!(second.why.starts_with("- `1` says"));

        assert_eq!(
            d.used,
            vec![
                Used {
                    skill: "rust".into(),
                    part: "Errors are one enum, with a message for each".into(),
                    followed: Followed::Followed
                },
                Used {
                    skill: "rust".into(),
                    part: "Fix clippy warnings, never quiet them".into(),
                    followed: Followed::DidntApply
                },
            ]
        );
        assert_eq!(
            d.rests_on,
            vec![
                Idea {
                    name: "Exponential backoff".into(),
                    what: "waiting longer after each failure: 1, 2, 4, 8 seconds.".into()
                },
                Idea {
                    name: "Jitter".into(),
                    what: "a little randomness added to a wait.".into()
                },
            ]
        );
    }

    #[test]
    fn a_short_debrief_needs_only_what_it_did_and_how() {
        let text = "---\nkelve: debrief/1\ntitle: Fix a typo\n---\n\n## What it did\n\nFixed a typo.\n\n## How it works\n\nOne word in the README.\n";
        let d = read(text, &[]).unwrap();
        assert!(d.decisions.is_empty() && d.used.is_empty() && d.rests_on.is_empty());
    }

    fn problems(text: &str) -> Vec<String> {
        read(text, &change()).unwrap_err()
    }

    #[test]
    fn decisions_must_be_in_the_change() {
        let moved = DEBRIEF.replace("src/fetch.rs 59-66", "src/fetch.rs 10-20");
        let found = problems(&moved);
        assert_eq!(
            found,
            vec![
                "Decision 1, “Retry only failures that might pass next time”, gives lines 10-20 in src/fetch.rs, which this change didn't add or change. Its changed lines there are 55-70."
            ]
        );
        let elsewhere = DEBRIEF.replace("src/fetch.rs 59-66", "src/http.rs 59-66");
        assert!(problems(&elsewhere)[0].contains("in src/http.rs, which this change didn't touch"));
        // Kept, it reads whatever lines it gives: the work may have moved on.
        assert!(read_kept(&elsewhere).is_ok());
        assert!(read_kept("no front matter").is_err());
        let backwards = DEBRIEF.replace("src/fetch.rs 59-66", "src/fetch.rs 66-59");
        assert!(problems(&backwards)[0].contains("Give the first line, then the last"));
        let none = DEBRIEF.replace("- lines: src/fetch.rs 59-66\n", "");
        assert!(problems(&none)[0].contains("has no lines"));
        let unreadable = DEBRIEF.replace("src/fetch.rs 59-66", "somewhere near the top");
        assert!(problems(&unreadable)[0].contains("kelve can't read"));
        // Other ways of writing lines read too.
        for fine in [
            "src/fetch.rs:59-66",
            "./src/fetch.rs 59–66",
            "`src/fetch.rs` 59, 60-61",
            "src/fetch.rs",
        ] {
            let text = DEBRIEF.replace("src/fetch.rs 59-66", fine);
            assert!(
                read(&text, &change()).is_ok(),
                "{fine}: {:?}",
                read(&text, &change())
            );
        }
        // A file with no changed lines, such as one deleted, takes any.
        let mut deleted = change();
        deleted[0].ranges.clear();
        assert!(read(&moved, &deleted).is_ok());
    }

    #[test]
    fn from_is_one_of_three() {
        let wrong = DEBRIEF.replace("from: its own call", "from: my own idea");
        assert_eq!(
            problems(&wrong),
            vec![
                "Decision 1, “Retry only failures that might pass next time”, says it's from “my own idea”. `from` is one of: its own call, what you said, your edit."
            ]
        );
        for fine in ["Its own call.", "WHAT YOU SAID", "your edit"] {
            let text = DEBRIEF.replace("from: its own call", &format!("from: {fine}"));
            assert!(read(&text, &change()).is_ok(), "{fine}");
        }
        let missing = DEBRIEF.replace("- from: its own call\n", "");
        assert!(problems(&missing)[0].contains("doesn't say where it came from"));
    }

    #[test]
    fn a_decision_has_known_fields_once_and_says_why() {
        let unknown = DEBRIEF.replace("- part: The command line", "- because: it had to");
        assert!(problems(&unknown)[0].contains("a field kelve doesn't know, `because`"));
        let twice = DEBRIEF.replace("- part: The command line", "- from: your edit");
        assert!(problems(&twice)[0].contains("gives `from` twice"));
        let no_why = DEBRIEF.replace(
            "Timeouts, dropped connections and server errors often clear up by\nthemselves. A 4xx means the request itself is wrong.\n",
            "",
        );
        assert!(problems(&no_why)[0].contains("doesn't say why"));
        let stray = DEBRIEF.replace("## Decisions\n", "## Decisions\n\nSome words first.\n");
        assert!(problems(&stray)[0].contains("isn't in a decision"));
    }

    #[test]
    fn its_shape_is_checked() {
        assert!(read("no front matter", &[]).unwrap_err()[0].contains("no front matter"));
        let wrong = DEBRIEF.replace("kelve: debrief/1", "kelve: debrief/2");
        assert!(problems(&wrong)[0].contains("A debrief says `kelve: debrief/1`"));
        let untitled = DEBRIEF.replace("title: Retry failed downloads with a growing wait\n", "");
        assert!(problems(&untitled)[0].contains("no `title:`"));
        let unknown = DEBRIEF.replace("## What it rests on", "## Notes");
        let found = problems(&unknown);
        assert!(
            found[0].contains("a section kelve doesn't know, “Notes”"),
            "{found:?}"
        );
        let h1 = DEBRIEF.replace("## What it did\n", "# Title\n\n## What it did\n");
        assert!(problems(&h1)[0].contains("is a `#` heading"));
        let empty = "---\nkelve: debrief/1\ntitle: x\n---\n";
        let found = read(empty, &[]).unwrap_err();
        assert_eq!(found.len(), 2, "{found:?}");
        let loose = DEBRIEF.replace("---\n\n## What it did", "---\n\nHello.\n\n## What it did");
        assert!(problems(&loose)[0].contains("isn't under a section"));
        let twice = format!("{DEBRIEF}\n## What it did\n\nMore.\n");
        assert!(problems(&twice)[0].contains("in the debrief twice"));
        let bad_item = DEBRIEF.replace(
            "- **Jitter**: a little randomness added to a wait.",
            "- Jitter",
        );
        assert!(problems(&bad_item)[0].contains("doesn't name an idea"));
        let bad_used = DEBRIEF.replace(": didn't apply", ": maybe");
        assert!(problems(&bad_used)[0].contains("doesn't say a skill"));
    }

    #[test]
    fn only_the_markdown_kelve_shows_passes() {
        let html = DEBRIEF.replace(
            "tries again, up to 5 times",
            "tries <b>again</b>, up to 5 times",
        );
        let found = problems(&html);
        assert_eq!(found.len(), 1);
        assert!(found[0].starts_with("Line 9 has HTML in it"), "{found:?}");
        let image = DEBRIEF.replace(
            "## How it works\n",
            "## How it works\n\n![chart](https://example.com/c.png)\n",
        );
        assert!(problems(&image)[0].contains("an image"));
        let link = DEBRIEF.replace("in `src/retry.rs`", "in [retry](src/retry.rs)");
        assert!(problems(&link)[0].contains("Link only to sources on the web"));
        let source = DEBRIEF.replace(
            "in `src/retry.rs`",
            "as [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110) says",
        );
        assert!(read(&source, &change()).is_ok());
    }

    const RESEARCH: &str = "#### What your agent found

Retry **429** too. It means the server is busy, not that the request
is wrong, and it usually says when to come back.

| Option | For | Against |
|---|---|---|
| Leave 429 alone | Nothing to change | A busy server stops the download |
| **Retry 429, as Retry-After says** | Does what the server asked | A few more lines |

**It suggests** retrying 429, waiting at least as long as `Retry-After`.

Sources: [RFC 6585](https://www.rfc-editor.org/rfc/rfc6585), [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110)
";

    #[test]
    fn the_specs_research_reads() {
        let r = research(RESEARCH).unwrap();
        assert!(r.answer.starts_with("Retry **429** too."));
        assert!(r.answer.ends_with("when to come back."));
        assert_eq!(r.ways.len(), 2);
        assert_eq!(r.ways[0].name, "Leave 429 alone");
        assert!(!r.ways[0].suggested);
        assert_eq!(r.ways[1].name, "Retry 429, as Retry-After says");
        assert!(r.ways[1].suggested);
        assert_eq!(r.ways[1].against, "A few more lines");
        assert!(r.suggests.starts_with("**It suggests** retrying 429"));
        assert_eq!(
            r.sources,
            vec![
                Source {
                    name: "RFC 6585".into(),
                    url: "https://www.rfc-editor.org/rfc/rfc6585".into()
                },
                Source {
                    name: "RFC 9110".into(),
                    url: "https://www.rfc-editor.org/rfc/rfc9110".into()
                },
            ]
        );
    }

    #[test]
    fn research_keeps_its_shape() {
        let no_head = RESEARCH.replace("#### What your agent found\n", "");
        assert!(
            research(&no_head)
                .unwrap_err()
                .iter()
                .any(|p| p.contains("starts with `#### What your agent found`"))
        );
        let no_table = RESEARCH
            .lines()
            .filter(|l| !l.starts_with('|'))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            research(&no_table)
                .unwrap_err()
                .iter()
                .any(|p| p.contains("no table"))
        );
        let one = RESEARCH.replace(
            "| Leave 429 alone | Nothing to change | A busy server stops the download |\n",
            "",
        );
        assert!(
            research(&one)
                .unwrap_err()
                .iter()
                .any(|p| p.contains("fewer than two ways"))
        );
        let short_row = RESEARCH.replace(
            "| Leave 429 alone | Nothing to change | A busy server stops the download |",
            "| Leave 429 alone | Nothing to change |",
        );
        assert!(
            research(&short_row)
                .unwrap_err()
                .iter()
                .any(|p| p.contains("has 2 cells"))
        );
        let heading = RESEARCH.replace("| Option | For | Against |", "| Way | Pro | Con |");
        assert!(
            research(&heading)
                .unwrap_err()
                .iter()
                .any(|p| p.contains("heading row"))
        );
        let no_pick = RESEARCH.replace("**It suggests** retrying", "Retrying");
        assert!(
            research(&no_pick)
                .unwrap_err()
                .iter()
                .any(|p| p.contains("which way it suggests"))
        );
        let no_sources = RESEARCH.replace("Sources: [RFC 6585](https://www.rfc-editor.org/rfc/rfc6585), [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110)", "Sources: the RFCs");
        assert!(
            research(&no_sources)
                .unwrap_err()
                .iter()
                .any(|p| p.contains("where it looked"))
        );
        let long = RESEARCH.replace("it usually says when to come back.", &"word ".repeat(90));
        assert!(
            research(&long)
                .unwrap_err()
                .iter()
                .any(|p| p.contains("short answer has"))
        );
        let html = RESEARCH.replace("Retry **429** too.", "Retry <em>429</em> too.");
        assert!(
            research(&html)
                .unwrap_err()
                .iter()
                .any(|p| p.contains("HTML"))
        );
    }

    #[test]
    fn an_explanation_is_short_markdown() {
        let text = "In this change, `wait_before()` adds up to half of each wait at random:\n\n```\nlet jitter = random_between(0, ceiling / 2);\n```\n\nWithout it, clients that failed together retry together.";
        assert_eq!(explanation(text).unwrap(), text);
        assert!(explanation("  ").unwrap_err()[0].contains("empty"));
        let long = "word ".repeat(260);
        assert!(explanation(&long).unwrap_err()[0].contains("260 words"));
        // Code doesn't count.
        let code = format!("Short.\n```\n{}\n```", "x ".repeat(400));
        assert!(explanation(&code).is_ok());
        assert!(explanation("## Jitter\nText.").unwrap_err()[0].contains("is a heading"));
        assert!(explanation("#### In this change\nText.").is_ok());
        assert!(explanation("See ![x](https://x.com/a.png)").is_err());
    }

    #[test]
    fn origins_are_written_as_the_debrief_has_them() {
        for o in Origin::ALL {
            assert_eq!(Origin::of(o.words()), Some(o));
        }
        assert_eq!(serde_json::to_value(Origin::Said).unwrap(), "said");
    }
}
