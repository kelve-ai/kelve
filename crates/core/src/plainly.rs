//! Plainly: a file, a folder or a change read in one shape, in plain words, as
//! the person's agent writes it; and what an answer must be before kelve takes
//! it. Every rejection is a sentence the agent can act on. A file's marks are
//! worked out here from its diff, never taken from the agent.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;

use crate::entry::check::{code_in, plain_ok, terms_in, unlink};
use crate::views::{
    ChangeKind, ChangePlainPart, ChangePlainly, FolderPlainPart, FolderPlainly, FolderShape,
    PartMark, PlainAlso, PlainFile, PlainIdea, PlainKeep, PlainLine, PlainOut, PlainPart,
};

/// The most parts a file has.
pub const MOST_PARTS: usize = 60;
/// The most parts a folder or a change has.
pub const MOST_FEW: usize = 7;
/// The most a folder keeps true, or a change touches.
pub const MOST_KEEPS: usize = 3;
/// The most things a change took out of a file.
pub const MOST_OUT: usize = 12;
/// The most words in what something is, or where a change fits.
pub const WHAT_WORDS: usize = 35;
/// The most words in a goal.
pub const GOAL_WORDS: usize = 6;
/// The most words a part is said in, and in how many sentences.
pub const SAYS_WORDS: usize = 45;
pub const SAYS_SENTENCES: usize = 2;
/// The most words in before → now, what it did before, a keep or a touch.
pub const LINE_WORDS: usize = 30;

// ---------------------------------------------------------------- the words

/// Lines as people read them: "lines 12–14", or "line 12".
fn lines(a: u32, b: u32) -> String {
    if a == b {
        format!("line {a}")
    } else {
        format!("lines {a}–{b}")
    }
}

/// How many words text has.
pub fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// How many sentences text has: runs ended by `.`, `!` or `?` before a
/// space or the end, outside code.
pub fn sentences(text: &str) -> usize {
    let mut prose = String::new();
    let mut in_code = false;
    for c in text.chars() {
        if c == '`' {
            in_code = !in_code;
            prose.push('x');
        } else if !in_code {
            prose.push(c);
        }
    }
    let chars: Vec<char> = prose.trim().chars().collect();
    let mut n = 0;
    let mut since = false;
    for (i, c) in chars.iter().enumerate() {
        if matches!(c, '.' | '!' | '?') && chars.get(i + 1).is_none_or(|n| n.is_whitespace()) {
            if since {
                n += 1;
            }
            since = false;
        } else if !c.is_whitespace() {
            since = true;
        }
    }
    n + usize::from(since)
}

/// The most words taken for a limit of `most`: a little over is taken as
/// it is, rather than sent back to say again, which costs the agent a
/// whole turn for a word or two; well over is sent back.
fn over(most: usize) -> usize {
    most + most.div_ceil(2)
}

/// Every field of a page's answer over its length, said at once when there's
/// more than one, so the agent fixes them all in one go rather than one a
/// try: a page put aside after its tries isn't written. One is said where
/// it's checked, as any problem is.
fn lengths(value: &Value) -> Result<(), String> {
    let mut over: Vec<String> = Vec::new();
    let mut check = |what: String, text: Option<&str>, most: usize| {
        let n = text.map(words).unwrap_or(0);
        if n > self::over(most) {
            over.push(format!("{what} is {n} words, at most {most}"));
        }
    };
    for (field, most) in [
        ("what", WHAT_WORDS),
        ("fits", WHAT_WORDS),
        ("change", LINE_WORDS),
    ] {
        check(field.to_string(), value[field].as_str(), most);
    }
    for (i, p) in value["parts"].as_array().into_iter().flatten().enumerate() {
        for (field, most) in [
            ("goal", GOAL_WORDS),
            ("says", SAYS_WORDS),
            ("before", LINE_WORDS),
        ] {
            check(format!("Part {}'s {field}", i + 1), p[field].as_str(), most);
        }
    }
    check(
        "also's says".into(),
        value["also"]["says"].as_str(),
        LINE_WORDS,
    );
    for (i, k) in value["keeps"].as_array().into_iter().flatten().enumerate() {
        check(format!("Keep {}", i + 1), k["says"].as_str(), LINE_WORDS);
    }
    if over.len() < 2 {
        return Ok(());
    }
    Err(format!(
        "{} are too long: {}. Say each in fewer words, the point first, and hand them in together.",
        over.len(),
        over.join("; ")
    ))
}

/// Text said in a field, kept to its limits: its whitespace made single,
/// or why not, `what` naming where it is ("Part 3's says").
fn said(
    what: &str,
    text: &str,
    most: usize,
    most_sentences: Option<usize>,
) -> Result<String, String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err(format!("{what} is empty: say it in a few plain words."));
    }
    let n = words(&text);
    if n > over(most) {
        return Err(format!(
            "{what} is {n} words: say it in at most {most}, the point first."
        ));
    }
    if let Some(most_sentences) = most_sentences {
        let s = sentences(&text);
        if s > most_sentences + 1 {
            return Err(format!(
                "{what} is {s} sentences: say it in at most {most_sentences}."
            ));
        }
    }
    if let Some(term) = terms_in(&text).into_iter().find(|t| t.contains('|')) {
        return Err(format!(
            "{what} links [[{term}]]: write a term as [[term]] alone, the idea's own name, with no |."
        ));
    }
    if !plain_ok(&text) {
        return Err(format!(
            "{what} has markup it can't have: `code`, **the key sentence** and [[term]] for an idea are the only markup, with no links, sources, HTML or line breaks."
        ));
    }
    Ok(text)
}

/// An optional field: none when it's missing or empty.
fn maybe(text: Option<&String>) -> Option<&str> {
    text.map(|t| t.trim()).filter(|t| !t.is_empty())
}

/// An idea marked somewhere, as `[[term]]` may lead to it: its name and the
/// name it has in the graph, its key, and the lines it's on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marked {
    pub names: Vec<String>,
    pub key: String,
    pub name: String,
    pub from: u32,
    pub to: u32,
}

/// The `[[term]]`s in some texts that lead to one of `ideas`, matched by
/// name as `ideas::norm` matches them, each once; and the texts with every
/// other term as its plain words.
pub fn resolve(texts: &mut [&mut String], ideas: &[&Marked]) -> Vec<PlainIdea> {
    let find = |term: &str| {
        let want = crate::ideas::norm(term);
        ideas
            .iter()
            .find(|m| m.names.iter().any(|n| crate::ideas::norm(n) == want))
    };
    let mut out: Vec<PlainIdea> = Vec::new();
    for text in texts.iter_mut() {
        for term in terms_in(text) {
            if let Some(m) = find(&term)
                && !out.iter().any(|i| i.term.eq_ignore_ascii_case(&term))
            {
                out.push(PlainIdea {
                    term,
                    key: m.key.clone(),
                    name: m.name.clone(),
                });
            }
        }
        **text = unlink(text, |t| find(t).is_some());
    }
    out
}

/// Marks on lines `from` to `to`.
pub fn on_lines(ideas: &[Marked], from: u32, to: u32) -> Vec<&Marked> {
    ideas
        .iter()
        .filter(|m| m.from <= to && m.to >= from)
        .collect()
}

// ---------------------------------------------------------------- a file

/// What a change did to a file, by its lines after it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    /// The lines it added.
    pub added: BTreeSet<u32>,
    /// Where it took lines out: after which line, 0 for the top.
    pub out_after: BTreeSet<u32>,
}

/// What a file's job gave the agent, as its answer is checked against it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileGiven {
    /// How many lines it has.
    pub count: u32,
    pub blank: BTreeSet<u32>,
    /// Code, whose `code` spans must be on their part's lines; else text.
    pub code: bool,
    /// Its lines, as given, from line 1.
    pub lines: Vec<String>,
    /// In a change, what it did.
    pub diff: Option<Diff>,
}

#[derive(Debug, Deserialize)]
struct FileIn {
    what: String,
    #[serde(default)]
    change: Option<String>,
    parts: Vec<PartIn>,
    #[serde(default)]
    out: Vec<OutIn>,
}

#[derive(Debug, Deserialize)]
struct PartIn {
    from: i64,
    to: i64,
    goal: String,
    says: String,
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    also: bool,
}

#[derive(Debug, Deserialize)]
struct OutIn {
    goal: String,
    says: String,
}

/// Which of a text's lines are blank, counted from 1, and how many lines
/// it has, up to `most`.
pub fn blank_lines(text: &str, most: usize) -> (u32, BTreeSet<u32>) {
    let mut count = 0u32;
    let mut blank = BTreeSet::new();
    for (i, line) in text.lines().take(most).enumerate() {
        count = u32::try_from(i + 1).unwrap_or(u32::MAX);
        if line.trim().is_empty() {
            blank.insert(count);
        }
    }
    (count, blank)
}

/// A text's lines numbered as the agent reads them: ` 12  text`, the
/// numbers no wider than the last.
pub fn numbered(text: &str, most: usize) -> String {
    let w = text.lines().take(most).count().to_string().len();
    text.lines()
        .take(most)
        .enumerate()
        .map(|(i, l)| format!("{:>w$}  {l}\n", i + 1))
        .collect()
}

/// The lines of numbered text, fenced, as `numbered` wrote them: by number.
pub fn unnumbered(fenced: &str) -> Vec<String> {
    let inner: Vec<&str> = fenced.lines().collect();
    let inner = match inner.len() {
        0..=2 => &[][..],
        n => &inner[1..n - 1],
    };
    let mut out: Vec<String> = Vec::new();
    for line in inner {
        let t = line.trim_start();
        let (n, rest) = t.split_once("  ").unwrap_or((t, ""));
        if let Ok(n) = n.parse::<usize>()
            && n == out.len() + 1
        {
            out.push(rest.to_string());
        }
    }
    out
}

/// Whether the change touched lines `from` to `to`, or took out lines at
/// their edges: what lets a part say what it did before.
fn touched(diff: &Diff, from: u32, to: u32) -> bool {
    diff.added.range(from..=to).next().is_some()
        || diff
            .out_after
            .range(from.saturating_sub(1)..=to)
            .next()
            .is_some()
}

/// A part's mark, from the diff: New when every line of it that isn't
/// blank was added; Changed when any was, or lines were taken out inside
/// it, or at its edges and the agent said what it did before; else As
/// before.
pub fn mark_of(
    diff: Option<&Diff>,
    blank: &BTreeSet<u32>,
    from: u32,
    to: u32,
    said_before: bool,
) -> PartMark {
    let Some(diff) = diff else {
        return PartMark::AsBefore;
    };
    let full: Vec<u32> = (from..=to).filter(|l| !blank.contains(l)).collect();
    if !full.is_empty() && full.iter().all(|l| diff.added.contains(l)) {
        return PartMark::New;
    }
    let inside = to > from && diff.out_after.range(from..to).next().is_some();
    if diff.added.range(from..=to).next().is_some()
        || inside
        || (said_before && touched(diff, from, to))
    {
        PartMark::Changed
    } else {
        PartMark::AsBefore
    }
}

/// Whether `code` is on some lines: as written, or a call's name without
/// its brackets.
fn on(code: &str, text: &[&str]) -> bool {
    let bare = code.trim_end_matches("()").trim();
    text.iter()
        .any(|l| l.contains(code) || (!bare.is_empty() && l.contains(bare)))
}

/// A file's answer, as kelve keeps it, or why it can't be taken, in a
/// sentence the agent can act on.
pub fn check_file(value: &Value, given: &FileGiven) -> Result<PlainFile, String> {
    lengths(value)?;
    let answer: FileIn = serde_json::from_value(value.clone())
        .map_err(|e| format!("It isn't in the shape asked for: {e}."))?;
    let in_change = given.diff.is_some();
    let what = said("what", &answer.what, WHAT_WORDS, Some(SAYS_SENTENCES))?;
    // What only a change has is left out of a file that isn't in one,
    // rather than sent back.
    let change = match maybe(answer.change.as_ref()) {
        Some(c) if in_change => Some(said("change", c, LINE_WORDS, Some(SAYS_SENTENCES))?),
        _ => None,
    };
    if answer.parts.len() > MOST_PARTS {
        return Err(format!(
            "Give at most {MOST_PARTS} parts: a part is a step toward what the file is for, often a few functions."
        ));
    }
    if given.count > 0 && answer.parts.is_empty() {
        return Err("Give its parts: every line that isn't blank is in one.".into());
    }
    // Each part's lines, set right rather than sent back: kept inside the
    // file, in order, each line in one part only, and a line no part has
    // joined to the part before it, or, before the first, to the first.
    let mut spans: Vec<(u32, u32, usize)> = answer
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let count = given.count.max(1);
            let from = u32::try_from(p.from.max(1)).unwrap_or(1).min(count);
            let to = u32::try_from(p.to.max(1)).unwrap_or(1).clamp(from, count);
            (from, to, i)
        })
        .collect();
    spans.sort();
    let mut kept: Vec<(u32, u32, usize)> = Vec::with_capacity(spans.len());
    for (from, to, i) in spans {
        let from = kept.last().map_or(from, |&(_, end, _)| from.max(end + 1));
        if from > to {
            // Inside the part before it: its lines are that part's.
            continue;
        }
        if let Some(last) = kept.last_mut() {
            // Lines between them that no part has go to the one before.
            if (last.1 + 1..from).any(|l| !given.blank.contains(&l)) {
                last.1 = from - 1;
            }
        }
        kept.push((from, to, i));
    }
    if let Some(first) = kept.first_mut() {
        first.0 = 1;
    }
    if let Some(last) = kept.last_mut()
        && (last.1 + 1..=given.count).any(|l| !given.blank.contains(&l))
    {
        last.1 = given.count;
    }
    // In the order the agent gave them, the order things happen.
    kept.sort_by_key(|&(_, _, i)| i);
    let mut parts = Vec::with_capacity(kept.len());
    for (n, &(from, to, i)) in kept.iter().enumerate() {
        let n = n + 1;
        let p = &answer.parts[i];
        let at = format!("Part {n}, {},", lines(from, to));
        let mut goal = said(&format!("{at} goal"), &p.goal, GOAL_WORDS, Some(1))?;
        let mut says = said(
            &format!("{at} says"),
            &p.says,
            SAYS_WORDS,
            Some(SAYS_SENTENCES),
        )?;
        // Before only where the change touched these lines; elsewhere it's
        // left out, rather than sent back.
        let before = match (maybe(p.before.as_ref()), &given.diff) {
            (Some(b), Some(diff)) if touched(diff, from, to) => Some(said(
                &format!("{at} before"),
                b,
                LINE_WORDS,
                Some(SAYS_SENTENCES),
            )?),
            _ => None,
        };
        if given.code {
            let text: Vec<&str> = given
                .lines
                .iter()
                .skip(from as usize - 1)
                .take((to - from + 1) as usize)
                .map(String::as_str)
                .collect();
            // A name that isn't on its lines is said as words, not code,
            // rather than sent back.
            for code in code_in(&goal).into_iter().chain(code_in(&says)) {
                if !on(&code, &text) {
                    let words = code.clone();
                    goal = goal.replace(&format!("`{code}`"), &words);
                    says = says.replace(&format!("`{code}`"), &words);
                }
            }
        }
        parts.push(PlainPart {
            from,
            to,
            mark: mark_of(
                given.diff.as_ref(),
                &given.blank,
                from,
                to,
                before.is_some(),
            ),
            goal,
            says,
            ideas: Vec::new(),
            before,
            also: p.also,
        });
    }
    let answer_out = if in_change { &answer.out[..] } else { &[] };
    if answer_out.len() > MOST_OUT {
        return Err(format!(
            "Give at most {MOST_OUT} things in out: what the change took out, by what it was for."
        ));
    }
    let mut out = Vec::with_capacity(answer_out.len());
    for (i, o) in answer_out.iter().enumerate() {
        let at = format!("Out {}", i + 1);
        out.push(PlainOut {
            goal: said(&format!("{at}'s goal"), &o.goal, GOAL_WORDS, Some(1))?,
            says: said(
                &format!("{at}'s says"),
                &o.says,
                SAYS_WORDS,
                Some(SAYS_SENTENCES),
            )?,
            ideas: Vec::new(),
        });
    }
    Ok(PlainFile {
        what,
        change,
        parts,
        out,
    })
}

/// A kept file's terms that lead to ideas marked where they're said: a
/// part's on its lines, what and change on any.
pub fn resolve_file(file: &mut PlainFile, ideas: &[Marked]) -> Vec<PlainIdea> {
    for p in &mut file.parts {
        let here = on_lines(ideas, p.from, p.to);
        let mut texts: Vec<&mut String> = vec![&mut p.goal, &mut p.says];
        if let Some(b) = p.before.as_mut() {
            texts.push(b);
        }
        p.ideas = resolve(&mut texts, &here);
    }
    let all: Vec<&Marked> = ideas.iter().collect();
    for o in &mut file.out {
        o.ideas = resolve(&mut [&mut o.goal, &mut o.says], &all);
    }
    let mut texts: Vec<&mut String> = vec![&mut file.what];
    if let Some(c) = file.change.as_mut() {
        texts.push(c);
    }
    resolve(&mut texts, &all)
}

// ---------------------------------------------------------------- a folder

/// What a folder's job gave the agent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderGiven {
    /// Its parts, from the root, each with whether it's a folder.
    pub parts: Vec<(String, bool)>,
    /// Which use which, as places in `parts`, the user first.
    pub uses: BTreeSet<(usize, usize)>,
    /// In a change, each part's lines added and taken out, and whether
    /// every file in it is new.
    pub changed: Option<Vec<(u32, u32, bool)>>,
}

#[derive(Debug, Deserialize)]
struct FolderIn {
    what: String,
    #[serde(default)]
    change: Option<String>,
    shape: String,
    parts: Vec<FolderPartIn>,
    #[serde(default)]
    also: Option<AlsoIn>,
    #[serde(default)]
    keeps: Vec<KeepIn>,
}

#[derive(Debug, Deserialize)]
struct FolderPartIn {
    part: String,
    goal: String,
    says: String,
    #[serde(default)]
    before: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AlsoIn {
    #[serde(default)]
    parts: Vec<String>,
    #[serde(default)]
    says: String,
}

#[derive(Debug, Deserialize)]
struct KeepIn {
    says: String,
    by: String,
}

/// A part of a folder as the agent names it, by its path as given; a
/// folder may end in `/`.
fn part_at(given: &FolderGiven, name: &str) -> Option<usize> {
    let name = name.trim().trim_end_matches('/');
    given.parts.iter().position(|(p, _)| p == name)
}

/// A folder's answer, as kelve keeps it, or why it can't be taken.
pub fn check_folder(value: &Value, given: &FolderGiven) -> Result<FolderPlainly, String> {
    lengths(value)?;
    let answer: FolderIn = serde_json::from_value(value.clone())
        .map_err(|e| format!("It isn't in the shape asked for: {e}."))?;
    let what = said("what", &answer.what, WHAT_WORDS, Some(SAYS_SENTENCES))?;
    // Left out of a folder that isn't in a change, rather than sent back.
    let change = match (maybe(answer.change.as_ref()), &given.changed) {
        (Some(c), Some(_)) => Some(said("change", c, LINE_WORDS, Some(SAYS_SENTENCES))?),
        _ => None,
    };
    let shape = match answer.shape.trim() {
        "steps" => FolderShape::Steps,
        "parts" => FolderShape::Parts,
        other => {
            return Err(format!(
                "shape is {other:?}: give steps, for the steps one thing takes through it, or parts."
            ));
        }
    };
    if answer.parts.is_empty() {
        return Err("Give its parts: at least one.".into());
    }
    if answer.parts.len() > MOST_FEW {
        return Err(format!(
            "Give at most {MOST_FEW} parts: the ones that matter most; put what most of them use in also."
        ));
    }
    let mut parts = Vec::with_capacity(answer.parts.len());
    let mut at_of = Vec::with_capacity(answer.parts.len());
    for (i, p) in answer.parts.iter().enumerate() {
        let n = i + 1;
        let Some(at) = part_at(given, &p.part) else {
            return Err(format!(
                "Part {n}, {:?}, isn't one of the folder's parts: name each by its path exactly as given.",
                p.part
            ));
        };
        if at_of.contains(&at) {
            return Err(format!(
                "Part {n}, {}, is named twice: give each part once.",
                given.parts[at].0
            ));
        }
        at_of.push(at);
        let name = &given.parts[at].0;
        let label = format!("Part {n}, {name},");
        let goal = said(&format!("{label} goal"), &p.goal, GOAL_WORDS, Some(1))?;
        let says = said(
            &format!("{label} says"),
            &p.says,
            SAYS_WORDS,
            Some(SAYS_SENTENCES),
        )?;
        let changed = given.changed.as_ref().and_then(|c| c.get(at)).copied();
        // Before only for a part the change touched; elsewhere it's left
        // out, rather than sent back.
        let before = match (maybe(p.before.as_ref()), changed) {
            (Some(b), Some(c)) if c != (0, 0, c.2) => Some(said(
                &format!("{label} before"),
                b,
                LINE_WORDS,
                Some(SAYS_SENTENCES),
            )?),
            _ => None,
        };
        parts.push(FolderPlainPart {
            part: name.clone(),
            folder: given.parts[at].1,
            goal,
            says,
            ideas: Vec::new(),
            before,
            mark: changed.map(|(a, r, new)| match (a, r, new) {
                (0, 0, _) => PartMark::AsBefore,
                (_, _, true) => PartMark::New,
                _ => PartMark::Changed,
            }),
        });
    }
    if shape == FolderShape::Steps {
        for (i, pair) in at_of.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            if !given.uses.contains(&(a, b)) && !given.uses.contains(&(b, a)) {
                return Err(format!(
                    "Steps {} and {}, {} and {}, don't lead on: neither uses the other, as kelve reads their code. Put the steps in an order where each uses the next or is used by it, or give shape parts.",
                    i + 1,
                    i + 2,
                    given.parts[a].0,
                    given.parts[b].0
                ));
            }
        }
    }
    let also = match answer.also {
        Some(a) if !a.parts.is_empty() || !a.says.trim().is_empty() => {
            let mut named = Vec::new();
            for p in &a.parts {
                let Some(at) = part_at(given, p) else {
                    return Err(format!(
                        "also names {p:?}, which isn't one of the folder's parts: name each by its path exactly as given."
                    ));
                };
                named.push(given.parts[at].0.clone());
            }
            Some(PlainAlso {
                parts: named,
                says: said("also's says", &a.says, LINE_WORDS, Some(1))?,
                ideas: Vec::new(),
            })
        }
        _ => None,
    };
    if answer.keeps.len() > MOST_KEEPS {
        return Err(format!(
            "Give at most {MOST_KEEPS} keeps: what it keeps true that matters most."
        ));
    }
    let mut keeps = Vec::with_capacity(answer.keeps.len());
    for (i, k) in answer.keeps.iter().enumerate() {
        let Some(at) = part_at(given, &k.by) else {
            return Err(format!(
                "Keep {} is kept by {:?}, which isn't one of the folder's parts: say which part keeps it, by its path as given.",
                i + 1,
                k.by
            ));
        };
        keeps.push(PlainKeep {
            says: said(&format!("Keep {}", i + 1), &k.says, LINE_WORDS, Some(1))?,
            by: given.parts[at].0.clone(),
            ideas: Vec::new(),
        });
    }
    Ok(FolderPlainly {
        path: String::new(),
        what: Some(what),
        change,
        ideas: Vec::new(),
        shape: Some(shape),
        parts,
        also,
        keeps,
        stale: false,
        waiting: None,
    })
}

/// A kept folder's terms that lead to ideas: a part's among those marked in
/// it, the rest among all of them. `in_part` says which ideas a part has.
pub fn resolve_folder(
    folder: &mut FolderPlainly,
    ideas: &[Marked],
    in_part: impl Fn(&str) -> Vec<String>,
) {
    let all: Vec<&Marked> = ideas.iter().collect();
    for p in &mut folder.parts {
        let keys = in_part(&p.part);
        let here: Vec<&Marked> = ideas.iter().filter(|m| keys.contains(&m.key)).collect();
        let mut texts: Vec<&mut String> = vec![&mut p.goal, &mut p.says];
        if let Some(b) = p.before.as_mut() {
            texts.push(b);
        }
        p.ideas = resolve(&mut texts, &here);
    }
    if let Some(a) = folder.also.as_mut() {
        a.ideas = resolve(&mut [&mut a.says], &all);
    }
    for k in &mut folder.keeps {
        let keys = in_part(&k.by);
        let here: Vec<&Marked> = ideas.iter().filter(|m| keys.contains(&m.key)).collect();
        k.ideas = resolve(&mut [&mut k.says], &here);
    }
    let mut texts: Vec<&mut String> = Vec::new();
    if let Some(w) = folder.what.as_mut() {
        texts.push(w);
    }
    if let Some(c) = folder.change.as_mut() {
        texts.push(c);
    }
    folder.ideas = resolve(&mut texts, &all);
}

// ---------------------------------------------------------------- a change

/// What a change's job gave the agent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeGiven {
    /// Its files, from the root.
    pub files: Vec<String>,
    /// The files it moved, each with where it was.
    pub moved: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ChangeIn {
    what: String,
    #[serde(default)]
    fits: Option<String>,
    kind: String,
    #[serde(default)]
    change: Option<String>,
    parts: Vec<ChangePartIn>,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    touches: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ChangePartIn {
    mark: String,
    goal: String,
    says: String,
    #[serde(default)]
    before: Option<String>,
    files: Vec<String>,
}

/// A kind of change by its word.
pub fn kind_of(word: &str) -> Option<ChangeKind> {
    Some(match word.trim() {
        "adds" => ChangeKind::Adds,
        "fixes" => ChangeKind::Fixes,
        "reshapes" => ChangeKind::Reshapes,
        "takesOut" => ChangeKind::TakesOut,
        "updates" => ChangeKind::Updates,
        _ => return None,
    })
}

/// A change's answer, as kelve keeps it, or why it can't be taken. What
/// tools made, kelve adds itself, after.
pub fn check_change(value: &Value, given: &ChangeGiven) -> Result<ChangePlainly, String> {
    lengths(value)?;
    let answer: ChangeIn = serde_json::from_value(value.clone())
        .map_err(|e| format!("It isn't in the shape asked for: {e}."))?;
    let what = said("what", &answer.what, WHAT_WORDS, Some(SAYS_SENTENCES))?;
    let fits = maybe(answer.fits.as_ref())
        .map(|f| said("fits", f, WHAT_WORDS, Some(SAYS_SENTENCES)))
        .transpose()?;
    let kind = kind_of(&answer.kind).ok_or_else(|| {
        format!(
            "kind is {:?}: give one of adds, fixes, reshapes, takesOut or updates.",
            answer.kind
        )
    })?;
    let change = maybe(answer.change.as_ref())
        .map(|c| said("change", c, LINE_WORDS, Some(SAYS_SENTENCES)))
        .transpose()?;
    if answer.parts.is_empty() {
        return Err("Give its parts: at least one, by what each is for.".into());
    }
    if answer.parts.len() > MOST_FEW {
        return Err(format!(
            "Give at most {MOST_FEW} parts: by purpose, not by file, the one that carries the purpose first."
        ));
    }
    let file = |name: &str| {
        let name = name.trim();
        given.files.iter().find(|f| *f == name).cloned()
    };
    let mut parts = Vec::with_capacity(answer.parts.len());
    for (i, p) in answer.parts.iter().enumerate() {
        let n = i + 1;
        let label = format!("Part {n}");
        let mark = match p.mark.trim() {
            "new" => PartMark::New,
            "changed" => PartMark::Changed,
            "moved" => PartMark::Moved,
            "takenOut" => PartMark::TakenOut,
            other => {
                return Err(format!(
                    "{label}'s mark is {other:?}: give new, changed, moved or takenOut."
                ));
            }
        };
        if p.files.is_empty() {
            return Err(format!(
                "{label} names no files: give the files it's in, by their paths as given."
            ));
        }
        let mut files = Vec::with_capacity(p.files.len());
        for f in &p.files {
            let Some(f) = file(f) else {
                return Err(format!(
                    "{label} names {f:?}, which isn't in the change: name its files by their paths as given."
                ));
            };
            if mark == PartMark::Moved && !given.moved.contains_key(&f) {
                return Err(format!(
                    "{label} is marked moved, but {f} didn't move: mark moved only what only moved, and say what changed as changed."
                ));
            }
            if !files.contains(&f) {
                files.push(f);
            }
        }
        parts.push(ChangePlainPart {
            mark,
            goal: said(&format!("{label}'s goal"), &p.goal, GOAL_WORDS, Some(1))?,
            says: said(
                &format!("{label}'s says"),
                &p.says,
                SAYS_WORDS,
                Some(SAYS_SENTENCES),
            )?,
            ideas: Vec::new(),
            before: maybe(p.before.as_ref())
                .map(|b| {
                    said(
                        &format!("{label}'s before"),
                        b,
                        LINE_WORDS,
                        Some(SAYS_SENTENCES),
                    )
                })
                .transpose()?,
            files,
        });
    }
    let mut tools = Vec::new();
    for t in &answer.tools {
        let Some(t) = file(t) else {
            return Err(format!(
                "tools names {t:?}, which isn't in the change: name what tools made by its path as given."
            ));
        };
        if !tools.contains(&t) {
            tools.push(t);
        }
    }
    if answer.touches.len() > MOST_KEEPS {
        return Err(format!(
            "Give at most {MOST_KEEPS} touches: what else now behaves differently, that matters most."
        ));
    }
    let mut touches = Vec::new();
    for (i, t) in answer.touches.iter().enumerate() {
        touches.push(PlainLine {
            says: said(
                &format!("Touch {}", i + 1),
                t,
                LINE_WORDS,
                Some(SAYS_SENTENCES),
            )?,
            ideas: Vec::new(),
        });
    }
    Ok(ChangePlainly {
        what: Some(what),
        fits,
        kind: Some(kind),
        change,
        ideas: Vec::new(),
        parts,
        tools,
        touches,
        order: Vec::new(),
        stale: false,
        waiting: None,
    })
}

/// A kept change's terms that lead to ideas: a part's among those marked in
/// its files, the rest among all of them. `in_file` says which ideas a file
/// has.
pub fn resolve_change(
    change: &mut ChangePlainly,
    ideas: &[Marked],
    in_file: impl Fn(&str) -> Vec<String>,
) {
    let all: Vec<&Marked> = ideas.iter().collect();
    for p in &mut change.parts {
        let keys: Vec<String> = p.files.iter().flat_map(|f| in_file(f)).collect();
        let here: Vec<&Marked> = ideas.iter().filter(|m| keys.contains(&m.key)).collect();
        let mut texts: Vec<&mut String> = vec![&mut p.goal, &mut p.says];
        if let Some(b) = p.before.as_mut() {
            texts.push(b);
        }
        p.ideas = resolve(&mut texts, &here);
    }
    for t in &mut change.touches {
        t.ideas = resolve(&mut [&mut t.says], &all);
    }
    let mut texts: Vec<&mut String> = Vec::new();
    for t in [&mut change.what, &mut change.fits, &mut change.change]
        .into_iter()
        .flatten()
    {
        texts.push(t);
    }
    change.ideas = resolve(&mut texts, &all);
}

/// Whether a file of a change was made by a tool, from its path and why
/// kelve didn't read it: an image, a lockfile, or a file that says a tool
/// made it or sits where a tool's output goes.
pub fn made_by_tools(path: &str, skip: Option<&str>) -> bool {
    let lower = path.to_ascii_lowercase();
    let image = [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".avif", ".bmp", ".ico",
    ]
    .iter()
    .any(|e| lower.ends_with(e));
    image || matches!(skip, Some("lockfile" | "generated"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const RETRY: &str = "use std::time::Duration;\n\nconst WAIT: Duration = Duration::from_millis(500);\nconst MAX_ATTEMPTS: u32 = 3;\n\npub async fn with_retries() {\n    loop {}\n}\n";

    fn given(text: &str, code: bool, diff: Option<Diff>) -> FileGiven {
        let (count, blank) = blank_lines(text, 2000);
        let fenced = crate::fence::fence("said", &numbered(text, 2000));
        FileGiven {
            count,
            blank,
            code,
            lines: unnumbered(&fenced),
            diff,
        }
    }

    fn part(from: i64, to: i64, says: &str) -> Value {
        json!({ "from": from, "to": to, "goal": "Does its part", "says": says })
    }

    fn file(parts: Vec<Value>) -> Value {
        json!({ "what": "Tries a call again.", "parts": parts })
    }

    #[test]
    fn a_file_is_said_a_part_at_a_time_covering_every_line_in_any_order() {
        let g = given(RETRY, true, None);
        assert_eq!(g.count, 8);
        assert_eq!(g.blank, BTreeSet::from([2, 5]));
        assert_eq!(
            g.lines[2],
            "const WAIT: Duration = Duration::from_millis(500);"
        );
        let good = file(vec![
            part(6, 8, "Calls the bank   until it works: `with_retries`."),
            part(
                3,
                4,
                "Sets the limits: wait **half a second**, and try `MAX_ATTEMPTS` times.",
            ),
            json!({ "from": 1, "to": 1, "goal": "Brings in time", "says": "Lengths of time.", "also": true }),
        ]);
        let f = check_file(&good, &g).expect("taken");
        assert_eq!(f.parts.len(), 3);
        assert_eq!(
            f.parts[0].says,
            "Calls the bank until it works: `with_retries`."
        );
        assert_eq!(f.parts[0].from, 6, "in the order the agent gave");
        assert!(f.parts[2].also);
        assert!(f.parts.iter().all(|p| p.mark == PartMark::AsBefore));
        // A part may take in the blank lines around it.
        assert!(check_file(&file(vec![part(1, 5, "a"), part(6, 8, "b")]), &g).is_ok());
    }

    #[test]
    fn lines_that_miss_overlap_or_run_over_are_set_right_and_too_much_said_comes_back() {
        let g = given(RETRY, true, None);
        let lines = |parts: Vec<Value>| -> Vec<(u32, u32)> {
            check_file(&file(parts), &g)
                .expect("set right, not sent back")
                .parts
                .iter()
                .map(|p| (p.from, p.to))
                .collect()
        };
        // A line no part has joins the part before it; overlaps are
        // trimmed; past the end, kept to the file; a part inside another
        // goes; the first starts at 1; the last runs to the end.
        assert_eq!(
            lines(vec![part(1, 1, "a"), part(6, 8, "b")]),
            [(1, 5), (6, 8)]
        );
        assert_eq!(
            lines(vec![part(1, 4, "a"), part(4, 8, "b")]),
            [(1, 4), (5, 8)]
        );
        assert_eq!(
            lines(vec![part(1, 4, "a"), part(6, 9, "b")]),
            [(1, 4), (6, 8)]
        );
        assert_eq!(lines(vec![part(1, 4, "a")]), [(1, 8)]);
        assert_eq!(
            lines(vec![part(0, 4, "a"), part(6, 8, "b")]),
            [(1, 4), (6, 8)]
        );
        assert_eq!(lines(vec![part(1, 8, "a"), part(3, 4, "b")]), [(1, 8)]);
        assert_eq!(
            lines(vec![part(3, 4, "a"), part(6, 8, "b")]),
            [(1, 4), (6, 8)]
        );
        // What's said is still kept to its limits: a little over is taken,
        // well over comes back.
        let said = |parts: Vec<Value>| check_file(&file(parts), &g).unwrap_err();
        assert!(said(vec![part(1, 4, " "), part(6, 8, "b")]).contains("empty"));
        let long = "word ".repeat(SAYS_WORDS * 2);
        assert!(said(vec![part(1, 4, &long), part(6, 8, "b")]).contains("at most 45"));
        assert!(
            said(vec![part(1, 4, "One. Two. Three. Four."), part(6, 8, "b")])
                .contains("4 sentences")
        );
        let goal = json!({ "from": 1, "to": 4, "goal": "a goal that runs on and on far too long", "says": "a" });
        assert!(said(vec![goal, part(6, 8, "b")]).contains("at most 6"));
        let near = json!({ "from": 1, "to": 4, "goal": "a goal one word too long", "says": "a" });
        assert!(check_file(&file(vec![near, part(6, 8, "b")]), &g).is_ok());
        // More than one too long: said at once, to fix in one go.
        let long = "one two three four five six seven eight nine ten";
        let both =
            lengths(&json!({ "what": "Fine.", "parts": [{ "goal": long }, { "goal": long }] }))
                .unwrap_err();
        assert!(
            both.starts_with("2 are too long: Part 1's goal is 10 words, at most 6; Part 2's goal"),
            "{both}"
        );
        assert!(lengths(&json!({ "what": "Fine.", "parts": [{ "goal": long }] })).is_ok());
        assert!(
            said(vec![
                part(1, 4, "Uses [[Jitter|a random wait]]."),
                part(6, 8, "b")
            ])
            .contains("with no |")
        );
        // A power isn't a source.
        assert!(
            check_file(
                &file(vec![part(1, 4, "Up to 2^40 tries."), part(6, 8, "b")]),
                &g
            )
            .is_ok()
        );
        for markup in ["As it says^1.", "<b>Waits</b>.", "See [[ that."] {
            assert!(
                said(vec![part(1, 4, markup), part(6, 8, "b")]).contains("markup"),
                "passed: {markup:?}"
            );
        }
        // A name in code that isn't on the part's lines is said as words; in
        // text, anything goes.
        let words = check_file(
            &file(vec![part(1, 4, "Waits `forever`."), part(6, 8, "b")]),
            &g,
        )
        .expect("taken");
        assert_eq!(words.parts[0].says, "Waits forever.");
        let text = given("# Retries\n\nThey wait.\n", false, None);
        assert!(check_file(&file(vec![part(1, 3, "Says `forever`.")]), &text).is_ok());
        // Before, change and out are only for a change: elsewhere, left out.
        let before =
            json!({ "from": 1, "to": 4, "goal": "Sets it up", "says": "a", "before": "b" });
        let left = check_file(&file(vec![before, part(6, 8, "b")]), &g).expect("taken");
        assert_eq!(left.parts[0].before, None);
        let mut with_change = file(vec![part(1, 4, "a"), part(6, 8, "b")]);
        with_change["change"] = json!("Now it waits.");
        assert_eq!(check_file(&with_change, &g).expect("taken").change, None);
        assert!(
            check_file(&json!({ "parts": "all" }), &g)
                .unwrap_err()
                .contains("shape")
        );
    }

    #[test]
    fn in_a_change_kelve_marks_each_part_from_the_diff() {
        // Lines 3 and 4 are new; a line was taken out after line 7.
        let diff = Diff {
            added: BTreeSet::from([3, 4]),
            out_after: BTreeSet::from([7]),
        };
        let g = given(RETRY, true, Some(diff));
        let answer = json!({
            "what": "Tries a call again.",
            "change": "It used to try forever. Now it stops after three tries.",
            "parts": [
                { "from": 1, "to": 1, "goal": "Brings in time", "says": "Lengths of time." },
                { "from": 3, "to": 4, "goal": "Sets the limits", "says": "Three tries." },
                { "from": 6, "to": 8, "goal": "Tries again", "says": "Loops.", "before": "Logged each try." },
            ],
            "out": [{ "goal": "The log", "says": "Said each try out loud." }],
        });
        let f = check_file(&answer, &g).expect("taken");
        let marks: Vec<PartMark> = f.parts.iter().map(|p| p.mark).collect();
        assert_eq!(
            marks,
            [PartMark::AsBefore, PartMark::New, PartMark::Changed]
        );
        // Before only where the change touched: elsewhere it's left out.
        let mut wrong = answer.clone();
        wrong["parts"][0]["before"] = json!("Brought in more.");
        assert_eq!(check_file(&wrong, &g).expect("taken").parts[0].before, None);
        // A whole new file is all new.
        let all = Diff {
            added: (1..=8).collect(),
            out_after: BTreeSet::new(),
        };
        let f = check_file(
            &file(vec![part(1, 4, "a"), part(6, 8, "b")]),
            &given(RETRY, true, Some(all)),
        )
        .expect("taken");
        assert!(f.parts.iter().all(|p| p.mark == PartMark::New));
    }

    #[test]
    fn terms_lead_to_ideas_marked_on_the_parts_lines_or_read_as_words() {
        let g = given(RETRY, true, None);
        let answer = file(vec![
            part(1, 4, "Waits between tries, as [[exponential backoffs]] do."),
            part(
                6,
                8,
                "Goes round, with [[Jitter]] and [[Exponential backoff]].",
            ),
        ]);
        let mut f = check_file(&answer, &g).expect("taken");
        let ideas = vec![Marked {
            names: vec!["Exponential backoff".into()],
            key: "i_backoff".into(),
            name: "Exponential backoff".into(),
            from: 3,
            to: 3,
        }];
        resolve_file(&mut f, &ideas);
        assert_eq!(
            f.parts[0].ideas.len(),
            1,
            "matched as the graph's names are"
        );
        assert_eq!(f.parts[0].ideas[0].key, "i_backoff");
        assert!(f.parts[0].says.contains("[[exponential backoffs]]"));
        // Not marked on its lines, or not at all: plain words.
        assert!(f.parts[1].ideas.is_empty());
        assert_eq!(
            f.parts[1].says,
            "Goes round, with Jitter and Exponential backoff."
        );
    }

    #[test]
    fn a_folders_steps_must_lead_on() {
        let g = FolderGiven {
            parts: vec![
                ("api".into(), true),
                ("ledger.rs".into(), false),
                ("bank".into(), true),
                ("config.rs".into(), false),
            ],
            uses: BTreeSet::from([(0, 1), (1, 2)]),
            changed: None,
        };
        let p = |part: &str| json!({ "part": part, "goal": "Takes a step", "says": "It does." });
        let steps =
            |parts: Vec<Value>| json!({ "what": "Moves money.", "shape": "steps", "parts": parts });
        let f =
            check_folder(&steps(vec![p("api/"), p("ledger.rs"), p("bank")]), &g).expect("taken");
        assert_eq!(f.shape, Some(FolderShape::Steps));
        assert_eq!(f.parts[0].part, "api");
        assert!(f.parts[0].folder);
        assert_eq!(f.parts[0].mark, None);
        // Used by the next counts too.
        assert!(check_folder(&steps(vec![p("bank"), p("ledger.rs")]), &g).is_ok());
        let why = check_folder(&steps(vec![p("api"), p("bank")]), &g).unwrap_err();
        assert!(why.contains("Steps 1 and 2, api and bank"), "{why}");
        assert!(why.contains("give shape parts"));
        let parts = json!({ "what": "Moves money.", "shape": "parts", "parts": [p("api"), p("bank")],
            "also": { "parts": ["config.rs"], "says": "Its settings." },
            "keeps": [{ "says": "Money moves only in pairs that balance.", "by": "ledger.rs" }] });
        let f = check_folder(&parts, &g).expect("taken");
        assert_eq!(f.keeps[0].by, "ledger.rs");
        assert!(
            check_folder(&steps(vec![p("nowhere.rs")]), &g)
                .unwrap_err()
                .contains("exactly as given")
        );
        let eight: Vec<Value> = (0..8).map(|_| p("api")).collect();
        assert!(
            check_folder(&steps(eight), &g)
                .unwrap_err()
                .contains("at most 7")
        );
        let mut keep = parts.clone();
        keep["keeps"][0]["by"] = json!("elsewhere.rs");
        assert!(
            check_folder(&keep, &g)
                .unwrap_err()
                .contains("which part keeps it")
        );
        // Before, outside a change, is left out.
        let mut before = parts.clone();
        before["parts"][0]["before"] = json!("It did less.");
        assert_eq!(
            check_folder(&before, &g).expect("taken").parts[0].before,
            None
        );
        let changed = FolderGiven {
            changed: Some(vec![
                (4, 1, false),
                (0, 0, false),
                (3, 0, true),
                (0, 0, false),
            ]),
            ..g
        };
        let f = check_folder(&before, &changed).expect("taken in a change");
        assert_eq!(f.parts[0].mark, Some(PartMark::Changed));
        assert_eq!(f.parts[1].mark, Some(PartMark::New));
        // Before for a part the change didn't touch is left out.
        let mut untouched = parts.clone();
        untouched["parts"] =
            json!([{ "part": "ledger.rs", "goal": "Keeps books", "says": "a", "before": "b" }]);
        assert_eq!(
            check_folder(&untouched, &changed).expect("taken").parts[0].before,
            None
        );
    }

    #[test]
    fn a_change_is_told_by_purpose_and_moves_are_only_moves() {
        let g = ChangeGiven {
            files: vec![
                "src/retry.rs".into(),
                "src/backoff.rs".into(),
                "Cargo.lock".into(),
            ],
            moved: BTreeMap::from([("src/backoff.rs".into(), "src/wait.rs".into())]),
        };
        let answer = json!({
            "what": "Retries now wait longer, at a random point.",
            "kind": "fixes",
            "parts": [
                { "mark": "changed", "goal": "Waits longer", "says": "Doubles each wait.", "before": "Waited half a second.", "files": ["src/retry.rs"] },
                { "mark": "moved", "goal": "Its wait, moved", "says": "Only moved.", "files": ["src/backoff.rs"] },
            ],
            "tools": ["Cargo.lock"],
            "touches": ["A call can take 30 seconds longer."],
        });
        let c = check_change(&answer, &g).expect("taken");
        assert_eq!(c.kind, Some(ChangeKind::Fixes));
        assert_eq!(c.parts[1].mark, PartMark::Moved);
        assert_eq!(c.tools, ["Cargo.lock"]);
        let mut wrong = answer.clone();
        wrong["parts"][0]["mark"] = json!("moved");
        assert!(
            check_change(&wrong, &g)
                .unwrap_err()
                .contains("didn't move")
        );
        let mut wrong = answer.clone();
        wrong["kind"] = json!("improves");
        assert!(
            check_change(&wrong, &g)
                .unwrap_err()
                .contains("adds, fixes")
        );
        let mut wrong = answer.clone();
        wrong["parts"][0]["files"] = json!(["src/elsewhere.rs"]);
        assert!(
            check_change(&wrong, &g)
                .unwrap_err()
                .contains("isn't in the change")
        );
        let mut wrong = answer;
        wrong["touches"] = json!(["a", "b", "c", "d"]);
        assert!(check_change(&wrong, &g).unwrap_err().contains("at most 3"));
    }

    #[test]
    fn sentences_and_tools_are_counted_plainly() {
        assert_eq!(sentences("Waits 0.5 seconds. Then tries `a.b()` again."), 2);
        assert_eq!(sentences("No stop at the end"), 1);
        assert_eq!(sentences(""), 0);
        assert!(made_by_tools("web/e2e/looks/screenshots/a.png", None));
        assert!(made_by_tools("Cargo.lock", Some("lockfile")));
        assert!(made_by_tools("web/src/api/gen/X.ts", Some("generated")));
        assert!(!made_by_tools("src/retry.rs", None));
        assert_eq!(
            unnumbered("<said>\n    1  a\n    2  \n    3    b\n</said>"),
            ["a", "", "  b"]
        );
    }
}
