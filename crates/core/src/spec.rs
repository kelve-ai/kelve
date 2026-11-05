//! What the person knows already: a short file, `marks.md`, in kelve's own
//! folder, that kelve keeps for itself and "I know this" in a debrief adds to.
//! Nothing asks them to fill it in. A few lines, and their own words if they
//! wrote any by hand, which go only to their agent:
//!
//! ```text
//! ---
//! mark: working
//! areas:
//!   rust: new
//!   distributed systems: new
//! always: Idempotency keys
//! never: Async functions, Pattern matching
//! by: Claude Code
//! at: 2026-10-22
//! ---
//! I came to backend from frontend work. Mark anything about how money moves.
//! ```
//!
//! kelve gives it to the person's agent as it explains an idea or looks into a
//! decision, so it knows what's new to them. Only `never` decides anything in
//! kelve itself: those ideas aren't marked new to them in a debrief. The rest
//! goes to the agent as it's written.

use serde::{Deserialize, Serialize};

/// How well the person knows an area, as their agent reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Level {
    /// Every idea is new to them, the basics too.
    New,
    /// All but the basics.
    Learning,
    /// Advanced and specialist ideas: what kelve takes until it's told more.
    #[default]
    Working,
    /// Specialist ideas only.
    Deep,
}

/// An area's name as kelve compares it: lower case, single spaces.
fn area(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// An area and how well the person knows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaLevel {
    pub area: String,
    pub level: Level,
}

/// What the person knows already.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Spec {
    /// How well they know their work, anywhere not named in `areas`.
    pub mark: Level,
    /// How well they know each area, as they listed them.
    pub areas: Vec<AreaLevel>,
    /// Ideas to always treat as new to them, by name.
    pub always: Vec<String>,
    /// Ideas they know, by name: never marked new to them.
    pub never: Vec<String>,
    /// Their own words on it: to their agent, as they wrote them.
    pub words: String,
    /// Who last wrote it.
    pub by: Option<String>,
    /// When, as a date: 2026-10-22.
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
            // `rust: new`; anything after a comma, as how its files once
            // read (`rust: new, plainly`), is left out.
            let lv = value.split_once(',').map_or(value, |(lv, _)| lv);
            if let Some(l) = level(lv) {
                let area = area(key);
                if !area.is_empty() {
                    spec.areas.push(AreaLevel { area, level: l });
                }
            }
            continue;
        }
        in_areas = false;
        match key.trim() {
            "mark" => spec.mark = level(value).unwrap_or_default(),
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
    if !spec.areas.is_empty() {
        out.push_str("areas:\n");
        for a in &spec.areas {
            out.push_str(&format!("  {}: {}\n", area(&a.area), level_word(a.level)));
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
                },
                AreaLevel {
                    area: "distributed systems".into(),
                    level: Level::New,
                },
                AreaLevel {
                    area: "sql".into(),
                    level: Level::New,
                },
            ],
            "an area with no level kelve knows is left out"
        );
        assert_eq!(spec.always, ["Idempotency keys", "Exactly-once delivery"]);
        assert_eq!(spec.never, ["Async functions", "Pattern matching"]);
        assert_eq!(spec.by.as_deref(), Some("Claude Code"));
        assert_eq!(spec.words, "I came to backend from frontend work.");
        assert_eq!(parse(&write(&spec)), spec);
        // No front matter: all words, everything else as it starts.
        let plain = parse("Mark anything about money.");
        assert_eq!(
            (plain.mark, plain.words.as_str()),
            (Level::Working, "Mark anything about money.")
        );
    }

    #[test]
    fn with_nothing_said_the_person_is_taken_to_be_working() {
        assert_eq!(Spec::default().mark, Level::Working);
        assert!(write(&Spec::default()).contains("mark: working\n"));
    }
}
