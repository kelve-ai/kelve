//! Notes as files: markdown with a small front matter saying the ideas a note
//! is about, by ID, and the passage it's on, as a path, a line range and a
//! fingerprint of the text, so it's found again after the file changes. kelve
//! writes each front-matter value as JSON on one line, which YAML reads too,
//! and reads back only its own keys: anything else a person or a tool put there
//! is kept as it was.
//!
//! A note for the agent is done for one version of its words: a hash of
//! its body (`version`), kept when the agent says it's done. Changed since,
//! in kelve or anywhere else, it waits for the agent again, with what was
//! done before and the lines added since.

use serde::{Deserialize, Serialize};

use crate::text::hash;

/// The passage a note is on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Passage {
    /// The file, in the folder it was opened in, with forward slashes.
    pub path: String,
    /// Its first and last lines, from 1, as "5-9".
    pub lines: String,
    /// A fingerprint of its text (`fingerprint`).
    pub fingerprint: String,
}

impl Passage {
    /// A passage of a file's lines, `start` to `end` from 1, fingerprinted.
    /// None when the lines aren't in the file.
    pub fn of(path: &str, file: &[&str], start: u32, end: u32) -> Option<Passage> {
        let lines = file.get(start.checked_sub(1)? as usize..end as usize)?;
        (start <= end).then(|| Passage {
            path: path.to_string(),
            lines: format!("{start}-{end}"),
            fingerprint: fingerprint(lines),
        })
    }

    /// Its first and last lines, as written.
    pub fn range(&self) -> Option<(u32, u32)> {
        let (a, b) = self
            .lines
            .split_once('-')
            .unwrap_or((&self.lines, &self.lines));
        let (a, b) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
        (a >= 1 && a <= b).then_some((a, b))
    }

    /// Where the passage is in the file now: where it was, if its text is
    /// unchanged there, else the nearest run of the same length with the
    /// same text. None when it's gone.
    pub fn find(&self, file: &[&str]) -> Option<(u32, u32)> {
        let (a, b) = self.range()?;
        let len = (b - a + 1) as usize;
        if len > file.len() {
            return None;
        }
        let was = a as usize - 1;
        let mut nearest: Option<usize> = None;
        for from in 0..=file.len() - len {
            if fingerprint(&file[from..from + len]) == self.fingerprint
                && nearest.is_none_or(|n| from.abs_diff(was) < n.abs_diff(was))
            {
                nearest = Some(from);
            }
        }
        nearest.map(|from| (from as u32 + 1, (from + len) as u32))
    }
}

/// A fingerprint of lines of text: the same words in the same order give
/// the same fingerprint, whatever the indentation or spacing.
pub fn fingerprint(lines: &[&str]) -> String {
    let words: Vec<&str> = lines.iter().flat_map(|l| l.split_whitespace()).collect();
    hash(&words.join(" "))
}

/// What kelve keeps in a note's front matter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Front {
    /// The ideas it's about, by ID.
    pub ideas: Vec<String>,
    /// The passage it's on, if it's on one.
    pub passage: Option<Passage>,
    /// For the person's agent to pick up (`for: agent`).
    pub for_agent: bool,
    /// For the agent: done, once it's said so; waiting until then. As
    /// kelve last wrote it: `done_hash` says for which words.
    pub done: bool,
    /// The version of its body the agent last did (`version`).
    pub done_hash: Option<String>,
    /// The words it was done for, so what's added since can be told.
    pub done_for: Option<String>,
    /// What the agent did, in its own line, once done.
    pub did: Option<String>,
    /// When it was done, as RFC 3339.
    pub done_at: Option<String>,
    /// What the agent opened when it was done, by its id.
    pub opened: Option<i64>,
    /// Front-matter lines that aren't kelve's, kept as they were.
    pub other: Vec<String>,
}

/// The version of a note's words: a hash of its body, without the blank
/// lines around it.
pub fn version(body: &str) -> String {
    hash(body.trim())
}

impl Front {
    /// Whether the agent has done this body: the words it last did are
    /// these. A note done before kelve kept which words counts as done.
    pub fn done_now(&self, body: &str) -> bool {
        match &self.done_hash {
            Some(h) => *h == version(body),
            None => self.done,
        }
    }

    /// Whether it's waiting for the agent: for it, with words, and not done
    /// as they are now.
    pub fn waiting(&self, body: &str) -> bool {
        self.for_agent && !body.trim().is_empty() && !self.done_now(body)
    }

    /// Forgets what the agent did with it: it's for someone else now, or
    /// cleared for fresh notes.
    pub fn forget_done(&mut self) {
        self.done = false;
        self.done_hash = None;
        self.done_for = None;
        self.did = None;
        self.done_at = None;
        self.opened = None;
    }
}

/// The lines of `now` with words that `before` doesn't have, in order:
/// what was added to a note since it was done.
pub fn added(before: &str, now: &str) -> Vec<String> {
    let mut had: Vec<&str> = before.lines().map(str::trim).collect();
    let mut out = Vec::new();
    for line in now.lines().map(str::trim).filter(|l| !l.is_empty()) {
        match had.iter().position(|h| *h == line) {
            Some(i) => {
                had.swap_remove(i);
            }
            None => out.push(line.to_string()),
        }
    }
    out
}

/// A note's front matter and its body. A note without front matter is all
/// body.
pub fn split(text: &str) -> (Front, &str) {
    let mut front = Front::default();
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (front, text);
    };
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        at += line.len();
        let line = line.trim_end_matches(['\n', '\r']);
        if line == "---" {
            let body = rest[at..].strip_prefix('\n').unwrap_or(&rest[at..]);
            return (front, body);
        }
        match line.split_once(':') {
            Some(("ideas", value)) => match serde_json::from_str::<Vec<String>>(value.trim()) {
                Ok(ideas) => front.ideas = ideas,
                Err(_) => front.other.push(line.to_string()),
            },
            Some(("passage", value)) => match serde_json::from_str::<Passage>(value.trim()) {
                Ok(p) => front.passage = Some(p),
                Err(_) => front.other.push(line.to_string()),
            },
            Some(("for", value)) if value.trim() == "agent" => front.for_agent = true,
            Some(("state", value)) if matches!(value.trim(), "waiting" | "done") => {
                front.done = value.trim() == "done";
            }
            Some(("did", value)) => match serde_json::from_str::<String>(value.trim()) {
                Ok(did) => front.did = Some(did),
                Err(_) => front.other.push(line.to_string()),
            },
            Some(("done_hash", value)) if !value.trim().is_empty() => {
                front.done_hash = Some(value.trim().to_string());
            }
            Some(("done_for", value)) => match serde_json::from_str::<String>(value.trim()) {
                Ok(text) => front.done_for = Some(text),
                Err(_) => front.other.push(line.to_string()),
            },
            Some(("done", value)) => front.done_at = Some(value.trim().to_string()),
            Some(("opened", value)) => match value.trim().parse() {
                Ok(id) => front.opened = Some(id),
                Err(_) => front.other.push(line.to_string()),
            },
            _ => front.other.push(line.to_string()),
        }
    }
    // No closing line: not front matter after all.
    (Front::default(), text)
}

/// A note's file: its front matter, when it has any, then its body.
pub fn join(front: &Front, body: &str) -> String {
    let mut lines = Vec::new();
    if !front.ideas.is_empty() {
        lines.push(format!(
            "ideas: {}",
            serde_json::to_string(&front.ideas).unwrap_or_else(|_| "[]".into())
        ));
    }
    if let Some(p) = &front.passage {
        lines.push(format!(
            "passage: {}",
            serde_json::to_string(p).unwrap_or_else(|_| "{}".into())
        ));
    }
    if front.for_agent {
        lines.push("for: agent".into());
        lines.push(format!(
            "state: {}",
            if front.done { "done" } else { "waiting" }
        ));
    }
    if let Some(did) = &front.did {
        lines.push(format!(
            "did: {}",
            serde_json::to_string(did).unwrap_or_else(|_| "\"\"".into())
        ));
    }
    if let Some(at) = &front.done_at {
        lines.push(format!("done: {at}"));
    }
    if let Some(h) = &front.done_hash {
        lines.push(format!("done_hash: {h}"));
    }
    if let Some(text) = &front.done_for {
        lines.push(format!(
            "done_for: {}",
            serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
        ));
    }
    if let Some(id) = front.opened {
        lines.push(format!("opened: {id}"));
    }
    lines.extend(front.other.iter().cloned());
    if lines.is_empty() {
        return body.to_string();
    }
    format!("---\n{}\n---\n\n{body}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str =
        "use sqlx::PgPool;\n\npub fn reserve() {\n    take_lock();\n    spend();\n}\n";

    #[test]
    fn a_note_keeps_its_front_matter_and_body_apart() {
        let lines: Vec<&str> = FILE.lines().collect();
        let front = Front {
            ideas: vec!["kelve:i3k9xq2m7twa8".into()],
            passage: Passage::of("src/ledger.rs", &lines, 3, 6),
            other: vec!["tags: [ledger]".into()],
            ..Front::default()
        };
        let text = join(&front, "# Reserving\n\nThe lock comes first.\n");
        assert!(text.starts_with("---\nideas: [\"kelve:i3k9xq2m7twa8\"]\npassage: {\"path\":\"src/ledger.rs\",\"lines\":\"3-6\""));
        let (read, body) = split(&text);
        assert_eq!(read, front);
        assert_eq!(body, "# Reserving\n\nThe lock comes first.\n");
    }

    #[test]
    fn a_note_for_the_agent_says_whether_its_done_and_what_was_done() {
        let waiting = Front {
            for_agent: true,
            ..Front::default()
        };
        let text = join(&waiting, "Make the cap a setting.\n");
        assert!(text.contains("for: agent\nstate: waiting"));
        assert_eq!(split(&text).0, waiting);
        let done = Front {
            for_agent: true,
            done: true,
            did: Some("Moved the cap to `Config`, \"30s\" by default.".into()),
            done_at: Some("2026-10-20T10:00:00Z".into()),
            opened: Some(7),
            ..Front::default()
        };
        assert_eq!(split(&join(&done, "Make the cap a setting.\n")).0, done);
    }

    #[test]
    fn a_note_is_done_for_the_words_it_was_done_for() {
        let body = "Make the cap a setting.\n";
        let done = Front {
            for_agent: true,
            done: true,
            done_hash: Some(version(body)),
            done_for: Some(body.trim().into()),
            did: Some("Moved the cap to `Config`.".into()),
            ..Front::default()
        };
        let text = join(&done, body);
        assert!(text.contains(&format!("done_hash: {}", version(body))));
        let (read, read_body) = split(&text);
        assert_eq!(read, done);
        assert!(!read.waiting(read_body));
        // The same words, blank lines aside: still done.
        assert!(!read.waiting("\nMake the cap a setting.\n\n"));
        // Changed anywhere, with `state: done` still written: it waits.
        let changed = text.replace("a setting.", "a setting, and the floor too.");
        let (read, read_body) = split(&changed);
        assert!(read.done && read.waiting(read_body));
        // Changed back, it's done again.
        assert!(!read.waiting(body));
        // Done before kelve kept which words: done.
        let before = Front {
            for_agent: true,
            done: true,
            ..Front::default()
        };
        assert!(!before.waiting("Anything"));
        // Empty, it waits for nothing.
        let empty = Front {
            for_agent: true,
            ..Front::default()
        };
        assert!(!empty.waiting("  \n"));
    }

    #[test]
    fn what_was_added_since_is_the_new_lines() {
        let before = "Make the cap a setting.\n\nKeep 30s.";
        let now = "Make the cap a setting.\n\nKeep 30s.\nAnd the floor too.\n  Keep 30s.";
        assert_eq!(added(before, now), ["And the floor too.", "Keep 30s."]);
        assert!(added(now, now).is_empty());
        assert_eq!(added("", "One\nTwo"), ["One", "Two"]);
    }

    #[test]
    fn a_note_without_front_matter_is_all_body() {
        assert_eq!(
            split("# Just words\n"),
            (Front::default(), "# Just words\n")
        );
        assert_eq!(split("---\nno end"), (Front::default(), "---\nno end"));
        assert_eq!(join(&Front::default(), "words"), "words");
    }

    #[test]
    fn a_passage_is_found_again_after_the_file_changes() {
        let lines: Vec<&str> = FILE.lines().collect();
        let p = Passage::of("src/ledger.rs", &lines, 3, 6).unwrap();
        assert_eq!(p.find(&lines), Some((3, 6)));

        // Two lines added above, and the passage reindented.
        let moved = "// Spending.\n// Held first.\nuse sqlx::PgPool;\n\npub fn reserve() {\n  take_lock();\n  spend();\n}\n";
        let moved: Vec<&str> = moved.lines().collect();
        assert_eq!(p.find(&moved), Some((5, 8)));

        // Its text changed: gone.
        let changed = FILE.replace("spend();", "spend_more();");
        let changed: Vec<&str> = changed.lines().collect();
        assert_eq!(p.find(&changed), None);

        // Lines outside the file are no passage.
        assert_eq!(Passage::of("a", &lines, 5, 40), None);
        assert_eq!(Passage::of("a", &lines, 0, 2), None);
    }
}
