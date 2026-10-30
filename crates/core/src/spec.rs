//! What kelve marks for the person: a short note, `marks.md` in their own notes
//! folder, written by hand, in settings or by their agent. A few lines kelve
//! reads itself, and their own words, which go only to their agent:
//!
//! ```text
//! ---
//! mark: working
//! read: as code
//! areas:
//!   rust: new, plainly
//!   distributed systems: new
//! always: Idempotency keys
//! never: Async functions, Pattern matching
//! by: Claude Code
//! at: 2026-10-22
//! ---
//! I came to backend from frontend work. Mark anything about how money moves.
//! ```
//!
//! How files read, as code or plainly, is in it too, overall and by area: it
//! changes how a file opens, never what's read or marked, so it's no part of
//! the lens.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use crate::ideas::norm;
use crate::lens::{self, Depth, Lens, Level};

/// How a code file opens: plainly, each part as a sentence on what it
/// does, unless the person said as code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ReadAs {
    Code,
    #[default]
    Plainly,
}

/// An area and how well the person knows it; and, when they said, how its
/// files read. An area is a file's when it's the file's language, as kelve
/// names it (`core::code::language`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AreaLevel {
    pub area: String,
    pub level: Level,
    /// How its files read; none for as files read overall.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub read: Option<ReadAs>,
}

/// What kelve marks for the person.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, rename = "MarksSpec")]
pub struct Spec {
    /// How much to mark, anywhere not named in `areas`.
    pub mark: Level,
    /// How code files read, in any area that doesn't say.
    pub read: ReadAs,
    /// How well they know each area, as they listed them.
    pub areas: Vec<AreaLevel>,
    /// Ideas always looked for and marked, by name.
    pub always: Vec<String>,
    /// Ideas never marked, by name.
    pub never: Vec<String>,
    /// Their own words on what they want marked: to their agent, as they
    /// wrote them.
    pub words: String,
    /// Who last wrote it: "you", or "agent", their agent.
    #[ts(optional)]
    pub by: Option<String>,
    /// When, as a date: 2026-10-22.
    #[ts(optional)]
    pub at: Option<String>,
}

fn level(word: &str) -> Option<Level> {
    match word.trim().to_lowercase().as_str() {
        "new" => Some(Level::New),
        "learning" => Some(Level::Learning),
        "working" => Some(Level::Working),
        "deep" => Some(Level::Deep),
        _ => None,
    }
}

fn level_word(l: Level) -> &'static str {
    match l {
        Level::New => "new",
        Level::Learning => "learning",
        Level::Working => "working",
        Level::Deep => "deep",
    }
}

fn read_as(word: &str) -> Option<ReadAs> {
    match word
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .as_str()
    {
        "as code" | "code" => Some(ReadAs::Code),
        "plainly" => Some(ReadAs::Plainly),
        _ => None,
    }
}

fn read_word(r: ReadAs) -> &'static str {
    match r {
        ReadAs::Code => "as code",
        ReadAs::Plainly => "plainly",
    }
}

/// A list as written by hand, `a, b`, or as `[a, b]`.
fn list(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Reads `marks.md`. Lines kelve doesn't know are left out; nothing in it
/// fails.
pub fn parse(text: &str) -> Spec {
    let mut spec = Spec::default();
    let (front, body) = match text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .and_then(|rest| {
            let end = rest.find("\n---")?;
            let after = &rest[end + 4..];
            Some((
                &rest[..end],
                after
                    .strip_prefix('\n')
                    .or_else(|| after.strip_prefix("\r\n"))
                    .unwrap_or(after),
            ))
        }) {
        Some(parts) => parts,
        None => ("", text),
    };
    let mut in_areas = false;
    for line in front.lines() {
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if in_areas && indented {
            // `rust: new`, or with how its files read, `rust: new, plainly`.
            let (lv, read) = value.split_once(',').unwrap_or((value, ""));
            if let Some(l) = level(lv) {
                let area = lens::area(key);
                if !area.is_empty() {
                    spec.areas.push(AreaLevel {
                        area,
                        level: l,
                        read: read_as(read),
                    });
                }
            }
            continue;
        }
        in_areas = false;
        match key.trim() {
            "mark" => spec.mark = level(value).unwrap_or_default(),
            "read" => spec.read = read_as(value).unwrap_or_default(),
            "areas" => in_areas = true,
            "always" => spec.always = list(value),
            "never" => spec.never = list(value),
            "by" => spec.by = Some(value.trim().to_string()).filter(|s| !s.is_empty()),
            "at" => spec.at = Some(value.trim().to_string()).filter(|s| !s.is_empty()),
            _ => {}
        }
    }
    spec.words = body.trim().to_string();
    spec
}

/// Writes `marks.md` from a spec.
pub fn write(spec: &Spec) -> String {
    let mut out = String::from("---\n");
    out.push_str(&format!("mark: {}\n", level_word(spec.mark)));
    out.push_str(&format!("read: {}\n", read_word(spec.read)));
    if !spec.areas.is_empty() {
        out.push_str("areas:\n");
        for a in &spec.areas {
            let read = a
                .read
                .map(|r| format!(", {}", read_word(r)))
                .unwrap_or_default();
            out.push_str(&format!(
                "  {}: {}{read}\n",
                lens::area(&a.area),
                level_word(a.level)
            ));
        }
    }
    if !spec.always.is_empty() {
        out.push_str(&format!("always: {}\n", spec.always.join(", ")));
    }
    if !spec.never.is_empty() {
        out.push_str(&format!("never: {}\n", spec.never.join(", ")));
    }
    if let Some(by) = &spec.by {
        out.push_str(&format!("by: {by}\n"));
    }
    if let Some(at) = &spec.at {
        out.push_str(&format!("at: {at}\n"));
    }
    out.push_str("---\n");
    if !spec.words.trim().is_empty() {
        out.push_str(spec.words.trim());
        out.push('\n');
    }
    out
}

/// A print of the person's own words: what a reading that looked for them
/// keeps, never the words.
pub fn words_print(words: &str) -> String {
    Sha256::digest(words.trim().as_bytes())
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Spec {
    /// What a reading for the person looks for.
    pub fn lens(&self) -> Lens {
        Lens {
            level: self.mark,
            areas: self
                .areas
                .iter()
                .map(|a| (lens::area(&a.area), a.level))
                .collect(),
            always: self.always.iter().cloned().collect(),
            words: if self.words.trim().is_empty() {
                Default::default()
            } else {
                std::iter::once(words_print(&self.words)).collect()
            },
        }
    }

    /// How a file of this language reads (`core::code::language`): as its
    /// area says, else as files read overall.
    pub fn reads(&self, language: Option<&str>) -> ReadAs {
        language
            .and_then(|l| {
                let l = lens::area(l);
                self.areas.iter().find(|a| lens::area(&a.area) == l)
            })
            .and_then(|a| a.read)
            .unwrap_or(self.read)
    }

    /// Whether an idea of this area and depth, by this name, is marked for
    /// the person: what they asked for, less what they never want.
    pub fn shows(&self, area: &str, depth: Depth, name: &str) -> bool {
        let n = norm(name);
        !self.never.iter().any(|x| norm(x) == n) && self.lens().wants(area, depth, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HAND: &str = "---\nmark: Working\nareas:\n  Rust: deep, As  Code\n  distributed  systems: new\n  sql: new, plainly\n  cooking: lots\nalways: Idempotency keys, Exactly-once delivery\nnever: [Async functions, \"Pattern matching\"]\nby: Claude Code\nat: 2026-10-22\nsomething: kelve doesn't know\n---\nI came to backend from frontend work.\n";

    #[test]
    fn a_spec_written_by_hand_reads_as_meant_and_writes_back_the_same() {
        let spec = parse(HAND);
        assert_eq!(spec.mark, Level::Working);
        assert_eq!(
            spec.areas,
            [
                AreaLevel {
                    area: "rust".into(),
                    level: Level::Deep,
                    read: Some(ReadAs::Code),
                },
                AreaLevel {
                    area: "distributed systems".into(),
                    level: Level::New,
                    read: None,
                },
                AreaLevel {
                    area: "sql".into(),
                    level: Level::New,
                    read: Some(ReadAs::Plainly),
                },
            ],
            "an area with no level kelve knows is left out"
        );
        assert_eq!(spec.read, ReadAs::Plainly, "plainly, unless it says");
        assert_eq!(parse("---\nread: as code\n---\n").read, ReadAs::Code);
        assert_eq!(spec.reads(Some("rust")), ReadAs::Code, "as its area says");
        assert_eq!(spec.reads(Some("python")), ReadAs::Plainly, "else overall");
        assert_eq!(spec.reads(None), ReadAs::Plainly);
        assert_eq!(spec.always, ["Idempotency keys", "Exactly-once delivery"]);
        assert_eq!(spec.never, ["Async functions", "Pattern matching"]);
        assert_eq!(spec.by.as_deref(), Some("Claude Code"));
        assert_eq!(spec.words, "I came to backend from frontend work.");
        assert_eq!(parse(&write(&spec)), spec);
        // No front matter: all words, everything else as it starts.
        let plain = parse("Mark anything about money.");
        assert_eq!(
            (plain.mark, plain.words.as_str()),
            (Level::Learning, "Mark anything about money.")
        );
    }

    #[test]
    fn the_spec_says_what_is_looked_for_and_what_is_shown() {
        let spec = parse(HAND);
        let lens = spec.lens();
        let code = Spec {
            read: ReadAs::Code,
            ..spec.clone()
        };
        assert_eq!(code.lens(), lens, "how files read is no part of the lens");
        assert_eq!(lens.level_of("Rust"), Level::Deep);
        assert_eq!(lens.level_of("payments"), Level::Working);
        assert_eq!(lens.words.len(), 1, "the words go in as a print");
        assert!(!lens.words.iter().any(|w| w.contains("frontend")));
        assert!(spec.shows("distributed systems", Depth::Basic, "Consensus"));
        assert!(!spec.shows("rust", Depth::Advanced, "Lifetimes"));
        assert!(
            spec.shows("payments", Depth::Common, "Idempotency keys"),
            "always"
        );
        assert!(
            !spec.shows("rust", Depth::Specialist, "Async functions"),
            "never, whatever else"
        );
    }
}
