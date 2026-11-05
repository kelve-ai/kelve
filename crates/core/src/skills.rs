//! Skills: the `SKILL.md` files kelve keeps of what the person knows, in
//! the Agent Skills format (agentskills.io), so every agent that reads
//! skills can use them.
//!
//! A skill has front matter (`name`, `description`), a `#` title, a line or
//! two on what it's for, and its parts under at most three `##` sections.
//! **A part is a `###` heading and everything under it, up to the next
//! heading of any level.** Anything that isn't under a `###` heading isn't a
//! part: it's shown, but never checked. A part is known by its skill and its
//! heading, normalised, so a changed heading is a new part.
//!
//! The file is the source of truth, and people edit it, so kelve reads it
//! without losing a byte: writing back what was read gives the same text,
//! and an edit changes only the part it's about. Where each part came from
//! and what its checks found are kept in kelve's store, never in the file.

pub mod compare;
pub mod work;

use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::md;
use crate::redact::redact;
use crate::text::re;

pub use work::WorkKind;

/// The most lines a `SKILL.md` has: short beats complete.
pub const MOST_LINES: usize = 150;
/// The longest name, in characters, as the format has it.
pub const MOST_NAME: usize = 64;
/// The longest description, in characters, as the format has it.
pub const MOST_DESCRIPTION: usize = 1024;
/// The most sections parts sit under.
pub const MOST_SECTIONS: usize = 3;
/// How close two descriptions must be, by `closeness`, for kelve to suggest
/// adding to the skill that's there rather than making a new one: at least
/// half their words in common, once common words are left out. A first
/// guess, to tune on real skills.
pub const CLOSE: f64 = 0.5;

/// The kinds of skill, by how they reach the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SkillKind {
    /// How you like it: the person's defaults for a kind of work, handed
    /// over by kelve's hook whenever the agent touches that kind.
    Like,
    /// How you do something: a procedure the agent loads when its
    /// description matches the task.
    Do,
    /// What only matters in one repository, kept in the project.
    Project,
}

impl SkillKind {
    /// What the skills page calls the kind.
    pub fn label(self) -> &'static str {
        match self {
            SkillKind::Like => "How you like it",
            SkillKind::Do => "How you do something",
            SkillKind::Project => "One project",
        }
    }
}

/// The sections parts sit under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SectionKind {
    /// The person's defaults.
    HowYouLikeIt,
    /// A procedure, in order.
    Steps,
    /// Mistakes the agent made before, so it doesn't make them again.
    WatchOutFor,
}

impl SectionKind {
    /// The three, in the order a skill has them.
    pub const ALL: [SectionKind; 3] = [
        SectionKind::HowYouLikeIt,
        SectionKind::Steps,
        SectionKind::WatchOutFor,
    ];

    /// Its `##` heading.
    pub fn title(self) -> &'static str {
        match self {
            SectionKind::HowYouLikeIt => "How you like it",
            SectionKind::Steps => "Steps",
            SectionKind::WatchOutFor => "Watch out for",
        }
    }

    /// The section a `##` heading names, however it's cased or punctuated.
    pub fn of(heading: &str) -> Option<SectionKind> {
        let heading = normalise(heading);
        SectionKind::ALL
            .into_iter()
            .find(|k| normalise(k.title()) == heading)
    }
}

/// A part's identity: its skill, and its heading normalised.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PartKey {
    pub skill: String,
    pub heading: String,
}

impl PartKey {
    pub fn new(skill: &str, heading: &str) -> PartKey {
        PartKey {
            skill: skill.trim().to_string(),
            heading: normalise(heading),
        }
    }
}

impl fmt::Display for PartKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.skill, self.heading)
    }
}

static STEP_NUMBER: LazyLock<Regex> = LazyLock::new(|| re(r"^\d{1,3}[.)]\s+"));
static MARKS: LazyLock<Regex> = LazyLock::new(|| re(r"[*_`~]"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));

/// A heading as kelve knows a part by: lower case, curly quotes made
/// straight, without markdown's marks, a step's number or a full stop at
/// the end, and with its spaces made single. So renumbering steps, or
/// putting a word in bold, leaves the part the same part.
pub fn normalise(heading: &str) -> String {
    let h = heading.trim();
    let h = STEP_NUMBER.replace(h, "");
    let h = MARKS.replace_all(&h, "");
    let h = h.replace(['‘', '’'], "'").replace(['“', '”'], "\"");
    let h = SPACES.replace_all(h.trim(), " ");
    h.trim_end_matches(['.', ':', ';', ','])
        .trim()
        .to_lowercase()
}

/// One piece of a skill's text. Every byte of the text is in exactly one.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    /// The front matter, both `---` lines included.
    Front { raw: String },
    /// A `#` heading, its line alone.
    Title { raw: String, text: String },
    /// A `##` heading, its line alone.
    Section { raw: String, title: String },
    /// A `###` heading and everything under it, up to the next heading.
    Part { raw: String, heading: String },
    /// Anything else: an intro, a section's lead, deeper headings and what's
    /// under them, blank lines.
    Other { raw: String },
}

impl Piece {
    fn raw(&self) -> &str {
        match self {
            Piece::Front { raw }
            | Piece::Title { raw, .. }
            | Piece::Section { raw, .. }
            | Piece::Part { raw, .. }
            | Piece::Other { raw } => raw,
        }
    }

    fn raw_mut(&mut self) -> &mut String {
        match self {
            Piece::Front { raw }
            | Piece::Title { raw, .. }
            | Piece::Section { raw, .. }
            | Piece::Part { raw, .. }
            | Piece::Other { raw } => raw,
        }
    }
}

/// A `SKILL.md`, read so that writing it back gives the same text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pieces: Vec<Piece>,
}

/// A section of a skill, as the skills page shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// Its `##` heading's words; empty for parts before any section.
    pub title: String,
    /// Which of the three it is, if it's one.
    pub kind: Option<SectionKind>,
    pub parts: Vec<Part>,
}

/// A part of a skill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// Its `###` heading's words, as written.
    pub heading: String,
    /// Its heading normalised: with the skill's name, its key.
    pub key: String,
    /// The part as written, heading and all, without the blank lines after it.
    pub markdown: String,
    /// What's under its heading, without the blank lines around it.
    pub body: String,
    /// The line its heading is on, from 1.
    pub line: usize,
    /// The section it's under, if any.
    pub section: Option<String>,
}

/// Where a new part goes in its section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// After the section's last part.
    End,
    /// Right after the part with this heading: next to what it's about.
    After(String),
}

/// Why a skill couldn't be changed as asked: sentences the agent, or the
/// person, can act on.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("A part needs a heading: the rule itself, in a few words.")]
    NoHeading,
    #[error(
        "The skill already has a part “{0}”. Change that part, or give this one a heading of its own."
    )]
    Exists(String),
    #[error("The skill has no part “{0}”.")]
    NotFound(String),
    #[error(
        "What goes under a part can't have headings of its own: they'd end the part. Use bold, or a list."
    )]
    HeadingInBody,
}

impl Skill {
    /// A skill's text, read. Anything reads: what doesn't fit the format is
    /// kept as it is, and `check` says what's wrong with it.
    pub fn parse(text: &str) -> Skill {
        let mut pieces = Vec::new();
        let front = md::front_matter(text);
        let start = front.as_ref().map_or(0, |f| f.len);
        if start > 0 {
            pieces.push(Piece::Front {
                raw: text[..start].to_string(),
            });
        }
        let mut open: Option<Piece> = None;
        for line in md::lines(&text[start..]) {
            match (!line.code).then(|| md::heading(line.text)).flatten() {
                Some((level, words)) => {
                    pieces.extend(open.take());
                    let raw = line.raw.to_string();
                    let words = words.to_string();
                    match level {
                        1 => pieces.push(Piece::Title { raw, text: words }),
                        2 => pieces.push(Piece::Section { raw, title: words }),
                        3 => {
                            open = Some(Piece::Part {
                                raw,
                                heading: words,
                            })
                        }
                        _ => open = Some(Piece::Other { raw }),
                    }
                }
                None => match &mut open {
                    Some(piece) => piece.raw_mut().push_str(line.raw),
                    None => {
                        open = Some(Piece::Other {
                            raw: line.raw.to_string(),
                        })
                    }
                },
            }
        }
        pieces.extend(open);
        Skill { pieces }
    }

    /// A new skill, with its front matter, title and what it's for, and no
    /// parts yet.
    pub fn new(name: &str, description: &str, title: &str, intro: &str) -> Skill {
        let mut text = format!(
            "---\nname: {}\ndescription: {}\n---\n\n# {}\n",
            md::quote(name.trim()),
            md::quote(description.trim()),
            title.trim()
        );
        if !intro.trim().is_empty() {
            text.push('\n');
            text.push_str(intro.trim());
            text.push('\n');
        }
        Skill::parse(&text)
    }

    /// The text again: exactly what was read, with any edits.
    pub fn write(&self) -> String {
        self.pieces.iter().map(Piece::raw).collect()
    }

    fn front(&self) -> Option<md::Front> {
        match self.pieces.first() {
            Some(Piece::Front { raw }) => md::front_matter(raw),
            _ => None,
        }
    }

    /// The `name` its front matter gives.
    pub fn name(&self) -> Option<String> {
        self.front()?.get("name").map(str::to_string)
    }

    /// The `description` its front matter gives.
    pub fn description(&self) -> Option<String> {
        self.front()?.get("description").map(str::to_string)
    }

    /// Its `#` title, the first if there's more than one.
    pub fn title(&self) -> Option<String> {
        self.pieces.iter().find_map(|p| match p {
            Piece::Title { text, .. } => Some(text.clone()),
            _ => None,
        })
    }

    /// What it's for: what's under the title, before its first section or
    /// part.
    pub fn intro(&self) -> String {
        let Some(at) = self
            .pieces
            .iter()
            .position(|p| matches!(p, Piece::Title { .. }))
        else {
            return String::new();
        };
        self.pieces[at + 1..]
            .iter()
            .take_while(|p| matches!(p, Piece::Other { .. }))
            .map(Piece::raw)
            .collect::<String>()
            .trim()
            .to_string()
    }

    /// How many lines it has.
    pub fn lines(&self) -> usize {
        self.write().lines().count()
    }

    /// Its parts, in order, each with the line it starts on and its section.
    pub fn parts(&self) -> Vec<Part> {
        let mut out = Vec::new();
        let mut line = 1;
        let mut section: Option<String> = None;
        for piece in &self.pieces {
            match piece {
                Piece::Section { title, .. } => section = Some(title.clone()),
                Piece::Title { .. } => section = None,
                Piece::Part { raw, heading } => {
                    let body = raw.split_once('\n').map_or("", |(_, b)| b);
                    out.push(Part {
                        heading: heading.clone(),
                        key: normalise(heading),
                        markdown: raw.trim_end().to_string(),
                        body: body.trim_matches('\n').trim_end().to_string(),
                        line,
                        section: section.clone(),
                    });
                }
                _ => {}
            }
            line += piece.raw().matches('\n').count();
        }
        out
    }

    /// Its sections, each with its parts, in order. Parts before any
    /// section come first, under a section with no title.
    pub fn sections(&self) -> Vec<Section> {
        let mut out: Vec<Section> = Vec::new();
        let titles: Vec<&str> = self
            .pieces
            .iter()
            .filter_map(|p| match p {
                Piece::Section { title, .. } => Some(title.as_str()),
                _ => None,
            })
            .collect();
        let parts = self.parts();
        if parts.iter().any(|p| p.section.is_none()) {
            out.push(Section {
                title: String::new(),
                kind: None,
                parts: parts
                    .iter()
                    .filter(|p| p.section.is_none())
                    .cloned()
                    .collect(),
            });
        }
        for title in titles {
            if out.iter().any(|s| s.title == title) {
                continue;
            }
            out.push(Section {
                title: title.to_string(),
                kind: SectionKind::of(title),
                parts: parts
                    .iter()
                    .filter(|p| p.section.as_deref() == Some(title))
                    .cloned()
                    .collect(),
            });
        }
        out
    }

    /// The part with this heading, however it's cased or numbered.
    pub fn part(&self, heading: &str) -> Option<Part> {
        let key = normalise(heading);
        self.parts().into_iter().find(|p| p.key == key)
    }

    /// Where the part with this heading is among the pieces.
    fn find(&self, heading: &str) -> Option<usize> {
        let key = normalise(heading);
        self.pieces
            .iter()
            .position(|p| matches!(p, Piece::Part { heading, .. } if normalise(heading) == key))
    }

    /// Adds a part to a section, making the section if the skill hasn't got
    /// it, in the order the three sections go in. The heading is the rule
    /// itself; the body says why, with an example only where it's needed.
    pub fn add_part(
        &mut self,
        section: SectionKind,
        place: &Place,
        heading: &str,
        body: &str,
    ) -> Result<(), EditError> {
        let heading = one_line(heading);
        if normalise(&heading).is_empty() {
            return Err(EditError::NoHeading);
        }
        if has_heading(body) {
            return Err(EditError::HeadingInBody);
        }
        if self.find(&heading).is_some() {
            return Err(EditError::Exists(heading));
        }
        let piece = Piece::Part {
            raw: part_text(&heading, body),
            heading: heading.clone(),
        };
        let at = match place {
            Place::After(after) => {
                self.find(after)
                    .ok_or_else(|| EditError::NotFound(one_line(after)))?
                    + 1
            }
            Place::End => match self.section_at(section) {
                Some(head) => {
                    let end = self.pieces[head + 1..]
                        .iter()
                        .position(|p| matches!(p, Piece::Section { .. } | Piece::Title { .. }))
                        .map_or(self.pieces.len(), |n| head + 1 + n);
                    // After the last part, or else after the section's lead.
                    self.pieces[head + 1..end]
                        .iter()
                        .rposition(|p| matches!(p, Piece::Part { .. }))
                        .map_or(end, |n| head + 2 + n)
                }
                None => {
                    let at = self.new_section_at(section);
                    self.insert(
                        at,
                        Piece::Section {
                            raw: format!("## {}\n", section.title()),
                            title: section.title().to_string(),
                        },
                    );
                    at + 1
                }
            },
        };
        self.insert(at, piece);
        Ok(())
    }

    /// Puts another wording in place of a part, where it is: a new heading
    /// is a new part, as far as checks go.
    pub fn replace_part(
        &mut self,
        heading: &str,
        new_heading: &str,
        body: &str,
    ) -> Result<(), EditError> {
        let at = self
            .find(heading)
            .ok_or_else(|| EditError::NotFound(one_line(heading)))?;
        let new_heading = one_line(new_heading);
        if normalise(&new_heading).is_empty() {
            return Err(EditError::NoHeading);
        }
        if has_heading(body) {
            return Err(EditError::HeadingInBody);
        }
        if let Some(other) = self.find(&new_heading)
            && other != at
        {
            return Err(EditError::Exists(new_heading));
        }
        let old = self.pieces[at].raw();
        let after = &old[old.trim_end_matches(['\n', '\r']).len()..];
        let mut raw = part_text(&new_heading, body);
        if after.len() > 1 {
            raw.push_str(&after[1..]);
        } else if after.is_empty() {
            raw.pop();
        }
        self.pieces[at] = Piece::Part {
            raw,
            heading: new_heading,
        };
        Ok(())
    }

    /// Takes a part out, with the blank lines after it.
    pub fn remove_part(&mut self, heading: &str) -> Result<(), EditError> {
        let at = self
            .find(heading)
            .ok_or_else(|| EditError::NotFound(one_line(heading)))?;
        self.pieces.remove(at);
        Ok(())
    }

    /// Where the section's heading is, if the skill has it.
    fn section_at(&self, kind: SectionKind) -> Option<usize> {
        self.pieces.iter().position(
            |p| matches!(p, Piece::Section { title, .. } if SectionKind::of(title) == Some(kind)),
        )
    }

    /// Where a new section goes: before the first section that comes after
    /// it, or that isn't one of the three (such as "More"), else at the end.
    fn new_section_at(&self, kind: SectionKind) -> usize {
        self.pieces
            .iter()
            .position(|p| match p {
                Piece::Section { title, .. } => SectionKind::of(title).is_none_or(|k| k > kind),
                _ => false,
            })
            .unwrap_or(self.pieces.len())
    }

    /// Puts a piece in, with a blank line before it and, when something
    /// follows, after it.
    fn insert(&mut self, at: usize, mut piece: Piece) {
        let so_far: String = self.pieces[..at].iter().map(Piece::raw).collect();
        if let Some(before) = at.checked_sub(1).and_then(|i| self.pieces.get_mut(i)) {
            let raw = before.raw_mut();
            if !so_far.ends_with('\n') {
                raw.push('\n');
            }
            if !so_far.ends_with("\n\n") {
                raw.push('\n');
            }
        }
        if at < self.pieces.len() {
            let raw = piece.raw_mut();
            if !raw.ends_with("\n\n") {
                raw.push('\n');
            }
        }
        self.pieces.insert(at, piece);
    }
}

/// A heading on one line.
fn one_line(text: &str) -> String {
    SPACES.replace_all(text.trim(), " ").to_string()
}

/// Whether a part's body has a heading in it, outside code.
fn has_heading(body: &str) -> bool {
    md::lines(body)
        .iter()
        .any(|l| !l.code && md::heading(l.text).is_some())
}

/// A part as kelve writes it: the heading, then what's under it.
fn part_text(heading: &str, body: &str) -> String {
    let body = body.trim_matches('\n').trim_end();
    if body.trim().is_empty() {
        format!("### {heading}\n")
    } else {
        format!("### {heading}\n{body}\n")
    }
}

/// What's wrong with a skill's name, by the format's rules: lowercase
/// letters, digits and single hyphens, at most 64 characters.
pub fn name_problem(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return Some(
            "The skill has no name. Add `name:` to its front matter, such as `name: rust`.".into(),
        );
    }
    if name.chars().count() > MOST_NAME {
        return Some(format!(
            "The name “{}” is {} characters long, and the most is {MOST_NAME}. Shorten it.",
            crate::text::clip(name, 40),
            name.chars().count()
        ));
    }
    let fits = name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if !fits {
        return Some(format!(
            "The name “{}” can only have lowercase letters, digits and single hyphens, and can't start or end with a hyphen, such as `retrying-network-calls`.",
            crate::text::clip(name, 40)
        ));
    }
    None
}

static WHEN: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(?:use|used|using|when|whenever)\b"));

/// What's wrong with a skill's description: it must say when to use the
/// skill, in at most 1024 characters.
pub fn description_problem(description: &str) -> Option<String> {
    let description = description.trim();
    if description.is_empty() {
        return Some("The skill has no description. Add `description:` to its front matter, saying what it's for and when to use it.".into());
    }
    if description.chars().count() > MOST_DESCRIPTION {
        return Some(format!(
            "The description is {} characters long, and the most is {MOST_DESCRIPTION}. Say what the skill is for and when to use it, and nothing else.",
            description.chars().count()
        ));
    }
    if !WHEN.is_match(description) {
        return Some("The description doesn't say when to use the skill. Add a sentence such as “Use when writing Rust.”".into());
    }
    None
}

/// Everything wrong with a skill's text, as sentences that say what to
/// change. Empty when it's sound.
pub fn check(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let skill = Skill::parse(text);
    match skill.front() {
        None => out.push("SKILL.md has no front matter. Start it with a line of `---`, then `name:` and `description:`, then `---` again.".into()),
        Some(front) => {
            out.extend(name_problem(front.get("name").unwrap_or_default()));
            out.extend(description_problem(front.get("description").unwrap_or_default()));
        }
    }
    if skill.title().is_none() {
        out.push(
            "SKILL.md has no title. Add a `#` heading after the front matter, such as `# Rust`."
                .into(),
        );
    }
    let lines = skill.lines();
    if lines > MOST_LINES {
        out.push(format!(
            "SKILL.md has {lines} lines, and the most is {MOST_LINES}. Take out parts that make no difference, or move long examples into `references/`."
        ));
    }
    let mut secret = false;
    for line in md::lines(text) {
        let n = line.number;
        if redact(line.text) != line.text {
            out.push(format!("Line {n} seems to hold a secret. Take it out."));
            secret = true;
        }
        if md::ABSOLUTE.is_match(line.text) {
            out.push(format!(
                "Line {n} has a path on someone's computer in it. Take it out, or write the path from the project's root."
            ));
        }
    }
    if !secret && redact(text) != text {
        out.push("SKILL.md seems to hold a secret. Take it out.".into());
    }
    let sections = skill.sections();
    let holding: Vec<&Section> = sections.iter().filter(|s| !s.parts.is_empty()).collect();
    if holding.len() > MOST_SECTIONS {
        out.push(format!(
            "Parts sit under {} sections, and the most is {MOST_SECTIONS}: How you like it, Steps and Watch out for. Move them into those.",
            holding.len()
        ));
    }
    for section in &holding {
        if section.title.is_empty() {
            for part in &section.parts {
                out.push(format!(
                    "The part “{}”, on line {}, isn't under a section. Put it under `## How you like it`, `## Steps` or `## Watch out for`.",
                    part.heading, part.line
                ));
            }
        } else if section.kind.is_none() {
            out.push(format!(
                "Parts sit under “{}”, which isn't one of the three sections. Put them under `## How you like it`, `## Steps` or `## Watch out for`.",
                section.title
            ));
        }
    }
    let mut seen: HashSet<String> = HashSet::new();
    for part in skill.parts() {
        if part.key.is_empty() {
            out.push(format!(
                "The `###` heading on line {} is empty. Write the rule itself, in a few words.",
                part.line
            ));
        } else if !seen.insert(part.key.clone()) {
            out.push(format!(
                "Two parts have the heading “{}”. Make them one part, or give each a heading that says how they differ.",
                part.heading
            ));
        }
    }
    out
}

/// How alike two descriptions are, in words a person can follow: the words
/// they share, and a score from 0 to 1.
#[derive(Clone, Debug, PartialEq)]
pub struct Closeness {
    /// Twice the words they share over the words in both (Dice's
    /// coefficient), counting each word once, by its stem, without common
    /// words.
    pub score: f64,
    /// The words they share, by their stems, in order.
    pub shared: Vec<String>,
}

impl Closeness {
    /// Whether they're close enough to add to the one that's there.
    pub fn close(&self) -> bool {
        self.score >= CLOSE
    }
}

/// Words every description might have, which say nothing about what it's
/// for: they're left out of closeness.
const COMMON: &[&str] = &[
    "a", "about", "after", "all", "always", "an", "and", "any", "are", "as", "at", "be", "before",
    "but", "by", "can", "do", "doe", "done", "each", "every", "follow", "for", "from", "her",
    "his", "how", "in", "into", "is", "it", "its", "like", "my", "never", "no", "not", "of", "on",
    "one", "or", "our", "out", "project", "read", "review", "skill", "so", "some", "step", "that",
    "the", "their", "them", "these", "they", "this", "those", "to", "up", "us", "use", "wa",
    "want", "what", "when", "whenever", "which", "who", "with", "work", "writ", "write", "written",
    "you", "your",
];

/// A word cut to its stem, so "retries", "retrying" and "retry" are one.
fn stem(word: &str) -> String {
    let w = word.trim_matches('\'');
    let w = w.strip_suffix("'s").unwrap_or(w);
    let n = w.chars().count();
    if n > 4 && w.ends_with("ies") {
        return format!("{}y", &w[..w.len() - 3]);
    }
    if n > 5 && w.ends_with("ing") {
        let s = &w[..w.len() - 3];
        return s
            .strip_suffix("y")
            .map_or(s.to_string(), |s| format!("{s}y"));
    }
    if n > 4 && w.ends_with("ed") {
        return w[..w.len() - 2].to_string();
    }
    if n > 3 && w.ends_with('s') && !w.ends_with("ss") {
        return w[..w.len() - 1].to_string();
    }
    w.to_string()
}

/// The words of a description that say what it's for, by their stems.
fn content(text: &str, ignore: &[String]) -> BTreeSet<String> {
    text.to_lowercase()
        .replace(['’', '‘'], "'")
        .split(|c: char| !(c.is_alphanumeric() || c == '\''))
        .map(stem)
        .filter(|w| w.chars().count() > 1)
        .filter(|w| !COMMON.contains(&w.as_str()) && !ignore.contains(w))
        .collect()
}

/// How close two skills' descriptions are. `ignore` names words to leave
/// out too, such as the person's own name, which their descriptions share.
pub fn closeness(a: &str, b: &str, ignore: &[&str]) -> Closeness {
    let ignore: Vec<String> = ignore.iter().map(|w| stem(&w.to_lowercase())).collect();
    let a = content(a, &ignore);
    let b = content(b, &ignore);
    let shared: Vec<String> = a.intersection(&b).cloned().collect();
    let total = a.len() + b.len();
    let score = if total == 0 {
        0.0
    } else {
        2.0 * shared.len() as f64 / total as f64
    };
    Closeness { score, shared }
}

/// The skill, of those given by name and description, whose description
/// is closest to a draft's, if any is close enough.
pub fn closest<'a>(
    draft: &str,
    skills: &[(&'a str, &str)],
    ignore: &[&str],
) -> Option<(&'a str, Closeness)> {
    skills
        .iter()
        .map(|(name, description)| (*name, closeness(draft, description, ignore)))
        .filter(|(_, c)| c.close())
        .max_by(|(_, a), (_, b)| a.score.total_cmp(&b.score))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST: &str = "---
name: rust
description: How Sam wants Rust written. Use when reading, writing or reviewing Rust.
---

# Rust

Rust that fails clearly and reads plainly, in any project.

## How you like it

### Errors are one enum, with a message for each
Every way a function can fail is a variant, and Display turns it into a
sentence a person can act on.

```rust
// ### not a heading, in code
pub enum FetchError { Timeout }
```

### Bad arguments exit 1, the tool’s own failures exit 2
So scripts can tell \"you called it wrong\" from \"it broke\".

## Watch out for

### Fix clippy warnings, never quiet them
Change the code itself. Never add #[allow] to make a warning go away.

## More

- [Error messages in full](references/errors.md)
";

    #[test]
    fn a_skill_is_written_back_exactly_as_it_was_read() {
        for text in [
            RUST,
            "",
            "no front matter at all\n",
            "---\nname: x\n---",
            "# Title only",
            "### A part with no newline at the end",
            "---\r\nname: x\r\n---\r\n\r\n# X\r\n\r\n## Steps\r\n\r\n### 1. One\r\nDo it.\r\n",
            "#### deep\n### part\n## section\n# title\ntext\n",
        ] {
            assert_eq!(Skill::parse(text).write(), text, "{text:?}");
        }
    }

    #[test]
    fn its_front_matter_title_and_intro_are_read() {
        let skill = Skill::parse(RUST);
        assert_eq!(skill.name().as_deref(), Some("rust"));
        assert_eq!(
            skill.description().as_deref(),
            Some("How Sam wants Rust written. Use when reading, writing or reviewing Rust.")
        );
        assert_eq!(skill.title().as_deref(), Some("Rust"));
        assert_eq!(
            skill.intro(),
            "Rust that fails clearly and reads plainly, in any project."
        );
        assert_eq!(skill.lines(), RUST.lines().count());
    }

    #[test]
    fn a_part_is_a_level_three_heading_and_what_is_under_it() {
        let skill = Skill::parse(RUST);
        let parts = skill.parts();
        let headings: Vec<&str> = parts.iter().map(|p| p.heading.as_str()).collect();
        assert_eq!(
            headings,
            vec![
                "Errors are one enum, with a message for each",
                "Bad arguments exit 1, the tool’s own failures exit 2",
                "Fix clippy warnings, never quiet them",
            ]
        );
        // Code in a part is the part's, and a heading inside code isn't one.
        assert!(parts[0].body.contains("pub enum FetchError"));
        assert!(parts[0].body.contains("// ### not a heading"));
        assert!(parts[0].body.starts_with("Every way"));
        assert!(parts[0].markdown.starts_with("### Errors are one enum"));
        assert!(parts[0].markdown.ends_with("```"));
        assert_eq!(parts[0].line, 12);
        assert_eq!(
            RUST.lines().nth(11),
            Some("### Errors are one enum, with a message for each")
        );
        assert_eq!(parts[2].line, 26);
        assert_eq!(parts[0].section.as_deref(), Some("How you like it"));
        assert_eq!(parts[2].section.as_deref(), Some("Watch out for"));
        assert_eq!(
            parts[1].key,
            "bad arguments exit 1, the tool's own failures exit 2"
        );

        let sections = skill.sections();
        let shape: Vec<(&str, Option<SectionKind>, usize)> = sections
            .iter()
            .map(|s| (s.title.as_str(), s.kind, s.parts.len()))
            .collect();
        assert_eq!(
            shape,
            vec![
                ("How you like it", Some(SectionKind::HowYouLikeIt), 2),
                ("Watch out for", Some(SectionKind::WatchOutFor), 1),
                ("More", None, 0),
            ]
        );
    }

    #[test]
    fn any_heading_ends_a_part() {
        let skill = Skill::parse(
            "## Steps\n\n### One\nDo this.\n#### Example\nshown, never checked\n### Two\nThen this.\n",
        );
        let parts = skill.parts();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].body, "Do this.");
        assert_eq!(parts[1].body, "Then this.");
    }

    #[test]
    fn headings_are_normalised_into_keys() {
        assert_eq!(
            normalise("  Errors are **one** enum.  "),
            "errors are one enum"
        );
        assert_eq!(
            normalise("1. Add it to the `Args` struct"),
            "add it to the args struct"
        );
        assert_eq!(
            normalise("2) Add it to the Args struct"),
            "add it to the args struct"
        );
        assert_eq!(
            normalise("Don’t   silence warnings:"),
            "don't silence warnings"
        );
        assert_eq!(normalise("Use 2.5 seconds"), "use 2.5 seconds");
        assert_eq!(
            PartKey::new("rust", "### is not stripped, only the words: Errors").to_string(),
            "rust#### is not stripped, only the words: errors"
        );
        assert_eq!(
            PartKey::new("rust", "Errors."),
            PartKey::new("rust", "errors")
        );
        assert_eq!(
            SectionKind::of("how YOU like it."),
            Some(SectionKind::HowYouLikeIt)
        );
        assert_eq!(SectionKind::of("More"), None);
    }

    #[test]
    fn a_part_is_added_after_the_last_in_its_section() {
        let mut skill = Skill::parse(RUST);
        skill
            .add_part(
                SectionKind::HowYouLikeIt,
                &Place::End,
                "A duration is a Duration, never a bare number",
                "A `u64` doesn't say whether it means seconds or milliseconds.",
            )
            .unwrap();
        let text = skill.write();
        assert!(
            text.contains("from \"it broke\".\n\n### A duration is a Duration, never a bare number\nA `u64` doesn't say whether it means seconds or milliseconds.\n\n## Watch out for"),
            "{text}"
        );
        // Everything else is as it was.
        let mut back = Skill::parse(&text);
        back.remove_part("a duration is a duration, never a bare number")
            .unwrap();
        assert_eq!(back.write(), RUST);
    }

    #[test]
    fn a_part_is_added_next_to_the_part_it_is_about() {
        let mut skill = Skill::parse(RUST);
        skill
            .add_part(
                SectionKind::HowYouLikeIt,
                &Place::After("Errors are one enum, with a message for each".into()),
                "Every error says what to do next",
                "So a person can act on it.",
            )
            .unwrap();
        let headings: Vec<String> = skill.parts().into_iter().map(|p| p.heading).collect();
        assert_eq!(headings[1], "Every error says what to do next");
        assert!(skill.write().contains("```\n\n### Every error says what to do next\nSo a person can act on it.\n\n### Bad arguments"));
        assert_eq!(
            skill.add_part(SectionKind::Steps, &Place::After("nothing".into()), "x", ""),
            Err(EditError::NotFound("nothing".into()))
        );
    }

    #[test]
    fn a_missing_section_is_made_where_it_belongs() {
        let mut skill = Skill::parse(RUST);
        skill
            .add_part(
                SectionKind::Steps,
                &Place::End,
                "1. Run the tests",
                "Then clippy.",
            )
            .unwrap();
        let text = skill.write();
        assert!(
            text.contains("from \"it broke\".\n\n## Steps\n\n### 1. Run the tests\nThen clippy.\n\n## Watch out for"),
            "{text}"
        );
        // A section that's none of the three, like More, stays last.
        let mut skill = Skill::parse("# X\n\n## More\n\n- a link\n");
        skill
            .add_part(
                SectionKind::WatchOutFor,
                &Place::End,
                "Never unwrap",
                "Return the error.",
            )
            .unwrap();
        assert_eq!(
            skill.write(),
            "# X\n\n## Watch out for\n\n### Never unwrap\nReturn the error.\n\n## More\n\n- a link\n"
        );
        // And at the end of a file with no sections.
        let mut skill = Skill::new("x", "Use when testing.", "X", "For tests.");
        skill
            .add_part(SectionKind::HowYouLikeIt, &Place::End, "Short names", "")
            .unwrap();
        assert_eq!(
            skill.write(),
            "---\nname: x\ndescription: Use when testing.\n---\n\n# X\n\nFor tests.\n\n## How you like it\n\n### Short names\n"
        );
        assert_eq!(check(&skill.write()), Vec::<String>::new());
    }

    #[test]
    fn a_section_with_a_lead_and_no_parts_gets_the_part_after_its_lead() {
        let mut skill = Skill::parse("## Watch out for\n\nMistakes made before.\n## More\n");
        skill
            .add_part(
                SectionKind::WatchOutFor,
                &Place::End,
                "Never unwrap",
                "Return the error.",
            )
            .unwrap();
        assert_eq!(
            skill.write(),
            "## Watch out for\n\nMistakes made before.\n\n### Never unwrap\nReturn the error.\n\n## More\n"
        );
    }

    #[test]
    fn edits_are_refused_with_a_reason() {
        let mut skill = Skill::parse(RUST);
        assert_eq!(
            skill.add_part(
                SectionKind::WatchOutFor,
                &Place::End,
                "fix CLIPPY warnings, never quiet them.",
                ""
            ),
            Err(EditError::Exists(
                "fix CLIPPY warnings, never quiet them.".into()
            ))
        );
        assert_eq!(
            skill.add_part(SectionKind::WatchOutFor, &Place::End, "  ", ""),
            Err(EditError::NoHeading)
        );
        assert_eq!(
            skill.add_part(
                SectionKind::WatchOutFor,
                &Place::End,
                "New",
                "## Sneaky\nsection"
            ),
            Err(EditError::HeadingInBody)
        );
        // A heading in code isn't one.
        assert!(
            skill
                .add_part(
                    SectionKind::WatchOutFor,
                    &Place::End,
                    "New",
                    "```\n# comment\n```"
                )
                .is_ok()
        );
        assert_eq!(
            skill.remove_part("not there"),
            Err(EditError::NotFound("not there".into()))
        );
        assert_eq!(
            skill.replace_part(
                "Errors are one enum, with a message for each",
                "Fix clippy warnings, never quiet them",
                "x"
            ),
            Err(EditError::Exists(
                "Fix clippy warnings, never quiet them".into()
            ))
        );
        assert!(EditError::NotFound("x".into()).to_string().ends_with('.'));
    }

    #[test]
    fn a_part_is_replaced_in_place() {
        let mut skill = Skill::parse(RUST);
        skill
            .replace_part(
                "Fix clippy warnings, never quiet them",
                "Fix every warning in the code itself",
                "Never add `#[allow]` to quiet one. If the lint is wrong, say why in a comment.",
            )
            .unwrap();
        let text = skill.write();
        assert!(text.contains("## Watch out for\n\n### Fix every warning in the code itself\nNever add `#[allow]` to quiet one. If the lint is wrong, say why in a comment.\n\n## More"), "{text}");
        // The same heading, reworded under it, keeps its key.
        let mut skill = Skill::parse(RUST);
        skill
            .replace_part(
                "Bad arguments exit 1, the tool’s own failures exit 2",
                "Bad arguments exit 1, the tool's own failures exit 2",
                "Scripts can tell.",
            )
            .unwrap();
        assert_eq!(skill.parts()[1].body, "Scripts can tell.");
        // The last part, with no blank line after it, stays that way.
        let mut skill = Skill::parse("## Steps\n### One\nold");
        skill.replace_part("One", "One", "new").unwrap();
        assert_eq!(skill.write(), "## Steps\n### One\nnew");
    }

    #[test]
    fn a_sound_skill_passes_its_checks() {
        assert_eq!(check(RUST), Vec::<String>::new());
    }

    #[test]
    fn names_keep_to_the_format() {
        assert_eq!(name_problem("retrying-network-calls"), None);
        assert_eq!(name_problem("rust2"), None);
        for bad in [
            "Rust",
            "rust_style",
            "-rust",
            "rust-",
            "a--b",
            "rust style",
            "",
        ] {
            assert!(name_problem(bad).is_some(), "{bad}");
        }
        assert!(
            name_problem(&"a".repeat(65))
                .unwrap()
                .contains("65 characters")
        );
        assert_eq!(name_problem(&"a".repeat(64)), None);
    }

    #[test]
    fn descriptions_say_when_to_use_the_skill() {
        assert_eq!(
            description_problem("How Sam wants Rust written. Use when writing Rust."),
            None
        );
        assert_eq!(
            description_problem("Steps for releasing, whenever a version goes out."),
            None
        );
        assert!(
            description_problem("How Sam wants Rust written.")
                .unwrap()
                .contains("when to use")
        );
        assert!(description_problem("").unwrap().contains("no description"));
        assert!(
            description_problem(&format!("Use when {}", "x".repeat(1100)))
                .unwrap()
                .contains("1024")
        );
    }

    #[test]
    fn checks_say_what_to_fix() {
        let long = format!("{RUST}{}", "\nmore".repeat(150));
        assert!(
            check(&long)
                .iter()
                .any(|p| p.contains("lines, and the most is 150"))
        );

        let secret = RUST.replace(
            "Change the code itself.",
            "Use token ghp_abcdefghijklmnopqrstuvwxyz0123456789AB.",
        );
        let found = check(&secret);
        assert!(
            found
                .iter()
                .any(|p| p.starts_with("Line 27 seems to hold a secret")),
            "{found:?}"
        );

        let path = RUST.replace("Change the code itself.", "See /Users/sam/acme/notes.md.");
        assert!(
            check(&path)
                .iter()
                .any(|p| p.contains("path on someone's computer"))
        );

        let no_front = "# Rust\n\n## Steps\n\n### One\n";
        assert!(check(no_front)[0].contains("no front matter"));

        let loose = "---\nname: x\ndescription: Use when testing.\n---\n\n# X\n\n### Loose part\n\n## Extra\n\n### Under extra\n";
        let found = check(loose);
        assert!(
            found
                .iter()
                .any(|p| p.contains("“Loose part”, on line 8, isn't under a section")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|p| p.contains("“Extra”, which isn't one of the three")),
            "{found:?}"
        );

        let four = "---\nname: x\ndescription: Use when testing.\n---\n\n# X\n\n## How you like it\n### a\n## Steps\n### b\n## Watch out for\n### c\n## Steps again\n### d\n";
        assert!(check(four).iter().any(|p| p.contains("under 4 sections")));

        let twice = "---\nname: x\ndescription: Use when testing.\n---\n\n# X\n\n## Steps\n### 1. Run it\n### 2. Run it.\n###\n";
        let found = check(twice);
        assert!(
            found
                .iter()
                .any(|p| p.contains("Two parts have the heading “2. Run it.”")),
            "{found:?}"
        );
        assert!(
            found
                .iter()
                .any(|p| p.contains("heading on line 11 is empty")),
            "{found:?}"
        );

        let untitled = "---\nname: x\ndescription: Use when testing.\n---\n";
        assert!(check(untitled).iter().any(|p| p.contains("no title")));
        for problem in check(&long).iter().chain(found.iter()) {
            assert!(
                problem.ends_with('.') || problem.ends_with('”'),
                "{problem}"
            );
        }
    }

    #[test]
    fn a_new_skill_quotes_what_must_be_quoted() {
        let skill = Skill::new(
            "retrying-network-calls",
            "Retrying calls: when a network call fails. Use when one can.",
            "Retrying network calls",
            "In any language.",
        );
        assert_eq!(skill.name().as_deref(), Some("retrying-network-calls"));
        assert_eq!(
            skill.description().as_deref(),
            Some("Retrying calls: when a network call fails. Use when one can.")
        );
        assert_eq!(skill.title().as_deref(), Some("Retrying network calls"));
        assert_eq!(skill.intro(), "In any language.");
        assert!(skill.parts().is_empty());
    }

    #[test]
    fn closeness_counts_the_words_two_descriptions_share() {
        let retry = "Retrying network calls that fail. Use when a network call can fail.";
        let again = "Retries failed network calls, with a growing wait. Use when calling a server.";
        let release = "The steps Sam follows to release a version. Use when releasing.";
        let close = closeness(retry, again, &[]);
        assert_eq!(close.shared, vec!["call", "fail", "network", "retry"]);
        assert!(close.close(), "{close:?}");
        let far = closeness(retry, release, &["Sam"]);
        assert!(far.shared.is_empty(), "{far:?}");
        assert!(!far.close());
        assert_eq!(closeness("", "", &[]).score, 0.0);
        assert_eq!(closeness(retry, retry, &[]).score, 1.0);

        // The person's name, in every description, is left out when given.
        let rust = "How Sam wants Rust written. Use when reading, writing or reviewing Rust.";
        let python = "How Sam wants Python written. Use when reading, writing or reviewing Python.";
        assert!(!closeness(rust, python, &["sam"]).close());

        let skills = [("release", release), ("retrying-network-calls", again)];
        let (name, found) = closest(retry, &skills, &["Sam"]).unwrap();
        assert_eq!(name, "retrying-network-calls");
        assert!(found.score >= CLOSE);
        assert!(
            closest(
                "Writing SQL migrations. Use when changing the schema.",
                &skills,
                &[]
            )
            .is_none()
        );
    }

    #[test]
    fn kinds_have_labels() {
        assert_eq!(SkillKind::Like.label(), "How you like it");
        assert_eq!(serde_json::to_value(SkillKind::Do).unwrap(), "do");
        assert_eq!(
            serde_json::to_value(SectionKind::WatchOutFor).unwrap(),
            "watchOutFor"
        );
    }
}
