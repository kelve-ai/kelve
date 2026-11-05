//! Checking a part of a skill on the person's past work (docs/skills.md, "Each
//! part is checked"), and Fits you. The person's agent redoes a past task
//! twice, as two subagents: once with the part and once without, or, for
//! Fits you, once with every skill and once with none. It hands in both
//! changes, and, for a part, whether each run did what the part says.
//!
//! kelve, never the agent, says what that means: the verdict here is a pure
//! function of the two runs and what the person kept, and it says why in
//! words a person can follow. How close a run came to what was kept is the
//! changed lines they share, file by file, so "82% like what you kept" means
//! 82% of the lines added or taken out were the same.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::views::PartCheck;

/// The most of a diff an answer may hand in, in characters.
pub const MOST_DIFF: usize = 200_000;
/// The most of what a subagent said, or of what shows it followed the part,
/// in characters.
pub const MOST_SAID: usize = 400;
/// The longest clearer wording, in characters.
pub const MOST_WORDING: usize = 1_200;
/// How alike two runs' changes must be to be the same work, whatever the
/// agent says each did: at least nine lines in ten the same.
pub const SAME: f64 = 0.9;

/// A line a change added or took out, in a file.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Line {
    pub path: String,
    pub added: bool,
    /// The line's words, its spaces made single and trimmed: indenting it
    /// again doesn't make it another line.
    pub text: String,
}

impl Line {
    pub fn new(path: &str, added: bool, text: &str) -> Line {
        Line {
            path: path.trim().to_string(),
            added,
            text: text.split_whitespace().collect::<Vec<_>>().join(" "),
        }
    }
}

/// The lines a unified diff, as `git diff` prints it, adds and takes out,
/// each with its file. Blank lines are left out: they say nothing about
/// the work.
pub fn lines_of(diff: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut path = String::new();
    let mut old = String::new();
    for raw in diff.lines() {
        if let Some(rest) = raw.strip_prefix("diff --git ") {
            // `a/x b/x`: the new side, until +++ says otherwise.
            path = rest.rsplit_once(" b/").map_or(rest, |(_, b)| b).to_string();
            old = path.clone();
            continue;
        }
        if let Some(rest) = raw.strip_prefix("--- ") {
            old = side(rest);
            continue;
        }
        if let Some(rest) = raw.strip_prefix("+++ ") {
            let new = side(rest);
            // A file taken out is known by its old name.
            path = if new == "/dev/null" { old.clone() } else { new };
            continue;
        }
        if raw.starts_with("@@") || raw.starts_with('\\') {
            continue;
        }
        let (added, text) = match raw.as_bytes().first() {
            Some(b'+') => (true, &raw[1..]),
            Some(b'-') => (false, &raw[1..]),
            _ => continue,
        };
        if text.trim().is_empty() {
            continue;
        }
        out.push(Line::new(&path, added, text));
    }
    out
}

/// A side of a diff's file header, without its `a/` or `b/`.
fn side(header: &str) -> String {
    let name = header.split('\t').next().unwrap_or(header).trim();
    name.strip_prefix("a/")
        .or_else(|| name.strip_prefix("b/"))
        .unwrap_or(name)
        .to_string()
}

/// How alike two changes are: the lines they share, of all their lines.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alike {
    /// From 0 to 1: twice the lines they share over the lines in both.
    /// Two changes with no lines at all are alike.
    pub score: f64,
    /// The lines they share, a line counted as often as both have it.
    pub same: usize,
    /// The lines in each.
    pub a: usize,
    pub b: usize,
}

impl Alike {
    /// As a whole percentage.
    pub fn percent(&self) -> u32 {
        (self.score * 100.0).round().clamp(0.0, 100.0) as u32
    }
}

/// How alike two changes are, by their lines.
pub fn alike(a: &[Line], b: &[Line]) -> Alike {
    let mut counts: HashMap<&Line, (usize, usize)> = HashMap::new();
    for l in a {
        counts.entry(l).or_default().0 += 1;
    }
    for l in b {
        counts.entry(l).or_default().1 += 1;
    }
    let same: usize = counts.values().map(|(x, y)| (*x).min(*y)).sum();
    let total = a.len() + b.len();
    let score = if total == 0 {
        1.0
    } else {
        2.0 * same as f64 / total as f64
    };
    Alike {
        score,
        same,
        a: a.len(),
        b: b.len(),
    }
}

/// One run of a past task, as the agent hands it in.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Run {
    /// Its change against the commit the task started from, as `git diff`
    /// prints it.
    pub diff: String,
    /// What its subagent said as it finished, in a line.
    pub said: String,
    /// Whether the work does what the part says. None for Fits you, which
    /// checks no part.
    #[serde(default)]
    pub followed: Option<bool>,
    /// The lines that show it, in a line.
    #[serde(default)]
    pub shows: String,
}

/// The agent's answer to a `compare` job: both runs, and clearer words for
/// the part when the run with it still didn't do what it says.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Answer {
    pub with: Run,
    pub without: Run,
    #[serde(default)]
    pub wording: Option<String>,
}

/// The shape a `compare` answer must have: for a part's check, whether each
/// run followed it; for Fits you, only the two changes.
pub fn schema(part: bool) -> Value {
    let mut run = json!({
        "type": "object",
        "properties": {
            "diff": { "type": "string", "description": "The run's change against the starting commit, as `git diff` prints it. Empty if it changed nothing." },
            "said": { "type": "string", "description": "What its subagent said as it finished, in a line." }
        },
        "required": ["diff", "said"]
    });
    if part {
        run["properties"]["followed"] = json!({ "type": "boolean", "description": "Whether the work does what the part says." });
        run["properties"]["shows"] =
            json!({ "type": "string", "description": "The lines that show it, in a line." });
        run["required"] = json!(["diff", "said", "followed", "shows"]);
    }
    let mut shape = json!({
        "type": "object",
        "properties": { "with": run.clone(), "without": run },
        "required": ["with", "without"]
    });
    if part {
        shape["properties"]["wording"] = json!({ "type": "string", "description": "Only when the run with the part still didn't do what it says: a clearer way to say the part, as a `###` heading that is the rule itself, then a sentence or two on why." });
    }
    shape
}

/// The answer, read and checked: why not, in a sentence the agent can act
/// on. `part` is whether it's a part's check, rather than Fits you.
pub fn read(value: &Value, part: bool) -> Result<Answer, String> {
    let answer: Answer = serde_json::from_value(value.clone()).map_err(|e| {
        format!("The answer isn't in the shape the job gives: {e}. Give with and without, each with diff and said.")
    })?;
    for (name, run) in [("with", &answer.with), ("without", &answer.without)] {
        if run.diff.chars().count() > MOST_DIFF {
            return Err(format!(
                "{name}'s diff is longer than {MOST_DIFF} characters. Give the change against the starting commit only, without files the run made by building."
            ));
        }
        if !run.diff.trim().is_empty() && !looks_like_diff(&run.diff) {
            return Err(format!(
                "{name}'s diff isn't a diff. Give `git diff` against the starting commit, as it printed it, or nothing if the run changed nothing."
            ));
        }
        if run.said.trim().is_empty() {
            return Err(format!(
                "{name} has no said. Give what its subagent said as it finished, in a line."
            ));
        }
        if run.said.chars().count() > MOST_SAID || run.shows.chars().count() > MOST_SAID {
            return Err(format!(
                "{name}'s said and shows are a line each: at most {MOST_SAID} characters."
            ));
        }
        if part && run.followed.is_none() {
            return Err(format!(
                "{name} doesn't say whether the run followed the part. Give followed, true or false, with shows."
            ));
        }
    }
    if part && answer.with.followed == Some(false) {
        let Some(wording) = answer.wording.as_deref().filter(|w| !w.trim().is_empty()) else {
            return Err("The run with the part didn't follow it, so give wording: a clearer way to say the part, as a `###` heading that is the rule itself, then a sentence or two on why.".into());
        };
        wording_problem(wording)?;
    }
    Ok(answer)
}

/// Whether text reads as a unified diff.
fn looks_like_diff(text: &str) -> bool {
    text.lines()
        .any(|l| l.starts_with("diff --git ") || l.starts_with("+++ ") || l.starts_with("@@ "))
}

/// What's wrong with a clearer wording for a part, if anything.
pub fn wording_problem(wording: &str) -> Result<(), String> {
    let wording = wording.trim();
    if wording.chars().count() > MOST_WORDING {
        return Err(format!(
            "The wording is longer than {MOST_WORDING} characters. Say the rule in its heading, and why in a sentence or two."
        ));
    }
    let Some((heading, body)) = split_part(wording) else {
        return Err("The wording must start with a `###` heading that is the rule itself.".into());
    };
    if heading.is_empty() {
        return Err(
            "The wording's `###` heading is empty. Write the rule itself, in a few words.".into(),
        );
    }
    if crate::md::lines(body)
        .iter()
        .any(|l| !l.code && crate::md::heading(l.text).is_some())
    {
        return Err(
            "The wording can only have its one `###` heading. Use bold, or a list, for the rest."
                .into(),
        );
    }
    Ok(())
}

/// A part's markdown, split into its heading's words and what's under it.
pub fn split_part(markdown: &str) -> Option<(String, &str)> {
    let markdown = markdown.trim_start();
    let first = markdown.lines().next()?;
    let heading = first.strip_prefix("### ")?.trim().to_string();
    let body = markdown[first.len()..].trim_matches(['\n', '\r']);
    Some((heading, body))
}

/// What a check found on one past task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// With the part, the agent did what it says; without it, it didn't.
    Works,
    /// The agent did it either way.
    NoDifference,
    /// With the part, the agent still didn't do what it says.
    Failed,
}

impl Outcome {
    /// As the store keeps it.
    pub fn name(self) -> &'static str {
        match self {
            Outcome::Works => "works",
            Outcome::NoDifference => "no_difference",
            Outcome::Failed => "failed",
        }
    }

    pub fn of(name: &str) -> Option<Outcome> {
        match name {
            "works" => Some(Outcome::Works),
            "no_difference" => Some(Outcome::NoDifference),
            "failed" => Some(Outcome::Failed),
            _ => None,
        }
    }
}

/// A check's verdict on one past task, with why, and how close each run
/// came to what the person kept.
#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    pub outcome: Outcome,
    /// How close each run came to what the person kept, when what they
    /// kept could be read.
    pub with: Option<Alike>,
    pub without: Option<Alike>,
    /// How alike the two runs are.
    pub runs: Alike,
    /// What it found, in a sentence.
    pub what: String,
    /// Why, in full: what it found, and how close each run came.
    pub why: String,
}

/// The verdict on one past task, from the two runs and the lines the person
/// kept, when those could be read. In order: the run with the part didn't
/// follow it, so it failed; the run without followed it anyway, or both
/// runs changed the same lines, so it makes no difference; else it works.
pub fn verdict(kept: Option<&[Line]>, answer: &Answer) -> Verdict {
    let with_lines = lines_of(&answer.with.diff);
    let without_lines = lines_of(&answer.without.diff);
    let with = kept.map(|k| alike(&with_lines, k));
    let without = kept.map(|k| alike(&without_lines, k));
    let runs = alike(&with_lines, &without_lines);
    let (outcome, what) = if answer.with.followed == Some(false) {
        (
            Outcome::Failed,
            match answer.with.shows.trim() {
                "" => "With the part, your agent still didn't do what it says.".to_string(),
                shows => format!(
                    "With the part, your agent still didn't do what it says: {}",
                    sentence(shows)
                ),
            },
        )
    } else if answer.without.followed == Some(true) {
        (
            Outcome::NoDifference,
            "Your agent did what it says without the part too.".to_string(),
        )
    } else if runs.score >= SAME {
        (
            Outcome::NoDifference,
            format!(
                "Both runs changed the same lines, {}% alike, with the part or without it.",
                runs.percent()
            ),
        )
    } else {
        (
            Outcome::Works,
            "With the part, your agent did what it says. Without it, it didn't.".to_string(),
        )
    };
    let why = match with.zip(without) {
        Some((w, wo)) => format!(
            "{what} With it, the work was {}% like what you kept; without it, {}%.",
            w.percent(),
            wo.percent()
        ),
        None => what.clone(),
    };
    Verdict {
        outcome,
        with,
        without,
        runs,
        what,
        why,
    }
}

/// Text as a sentence: a capital, and a full stop at the end.
fn sentence(text: &str) -> String {
    let text = text.trim();
    let mut chars = text.chars();
    let mut out: String = match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => return String::new(),
    };
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// What a part's checks found, over every past task they redid: what the
/// skills page shows, and the line that says why. A failure counts first,
/// unless more tasks showed it works; then any task where it works; else
/// it makes no difference. None until a task is done.
pub fn overall(outcomes: &[Outcome]) -> Option<(PartCheck, String)> {
    let n = outcomes.len();
    if n == 0 {
        return None;
    }
    let count = |o: Outcome| outcomes.iter().filter(|x| **x == o).count();
    let (works, same, failed) = (
        count(Outcome::Works),
        count(Outcome::NoDifference),
        count(Outcome::Failed),
    );
    let of = |k: usize| format!("{k} of {n} past {}", if n == 1 { "task" } else { "tasks" });
    if failed > 0 && failed >= works {
        return Some((PartCheck::Failed, format!("Not followed on {}", of(failed))));
    }
    if works > 0 {
        let mut hint = format!("Better on {}", of(works));
        if same > 0 {
            hint.push_str(&format!(", the same on {same}"));
        }
        if failed > 0 {
            hint.push_str(&format!(", not followed on {failed}"));
        }
        return Some((PartCheck::Works, hint));
    }
    let hint = match n {
        1 => "The same on the past task, with or without it".to_string(),
        2 => "The same on both past tasks, with or without it".to_string(),
        n => format!("The same on all {n} past tasks, with or without it"),
    };
    Some((PartCheck::NoDifference, hint))
}

/// A skill's text without one part: what the run without it is given.
pub fn without_part(text: &str, heading: &str) -> Option<String> {
    let mut skill = super::Skill::parse(text);
    skill.remove_part(heading).ok()?;
    Some(skill.write())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEPT: &str = "diff --git a/src/fetch.rs b/src/fetch.rs
index 1111111..2222222 100644
--- a/src/fetch.rs
+++ b/src/fetch.rs
@@ -1,3 +1,8 @@
-pub fn fetch(url: &str) -> Result<String, String> {
+pub enum FetchError {
+    Timeout,
+    Status(u16),
+}
+
+pub fn fetch(url: &str) -> Result<String, FetchError> {
     todo!()
 }
";

    fn run(diff: &str, followed: bool) -> Run {
        Run {
            diff: diff.into(),
            said: "Done.".into(),
            followed: Some(followed),
            shows: "pub enum FetchError".into(),
        }
    }

    #[test]
    fn a_diffs_lines_are_read_with_their_files() {
        let lines = lines_of(KEPT);
        assert_eq!(lines.len(), 6);
        assert_eq!(
            lines[0],
            Line::new(
                "src/fetch.rs",
                false,
                "pub fn fetch(url: &str) -> Result<String, String> {"
            )
        );
        assert!(
            lines
                .iter()
                .skip(1)
                .all(|l| l.added && l.path == "src/fetch.rs")
        );
        // Indenting a line again doesn't make it another line.
        assert_eq!(
            Line::new("a", true, "    Timeout,"),
            Line::new("a", true, "Timeout,")
        );
    }

    #[test]
    fn a_file_taken_out_is_known_by_its_old_name() {
        let lines = lines_of(
            "diff --git a/old.rs b/old.rs\ndeleted file mode 100644\n--- a/old.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-gone\n",
        );
        assert_eq!(lines, vec![Line::new("old.rs", false, "gone")]);
    }

    #[test]
    fn closeness_is_the_lines_two_changes_share() {
        let kept = lines_of(KEPT);
        assert_eq!(alike(&kept, &kept).score, 1.0);
        assert_eq!(alike(&[], &[]).score, 1.0);
        assert_eq!(alike(&kept, &[]).score, 0.0);
        let half: Vec<Line> = kept[..2].to_vec();
        let a = alike(&half, &kept);
        assert_eq!((a.same, a.a, a.b), (2, 2, 6));
        assert_eq!(a.percent(), 50);
        // The same line in another file isn't the same line.
        let elsewhere = vec![Line::new("src/other.rs", true, "Timeout,")];
        assert_eq!(alike(&elsewhere, &kept).same, 0);
    }

    #[test]
    fn a_part_followed_only_with_it_works() {
        let answer = Answer {
            with: run(KEPT, true),
            without: run(
                "diff --git a/src/fetch.rs b/src/fetch.rs\n--- a/src/fetch.rs\n+++ b/src/fetch.rs\n@@ -1 +1 @@\n-pub fn fetch(url: &str) -> Result<String, String> {\n+pub fn fetch(url: &str) -> Result<String, Box<dyn Error>> {\n",
                false,
            ),
            wording: None,
        };
        let v = verdict(Some(&lines_of(KEPT)), &answer);
        assert_eq!(v.outcome, Outcome::Works);
        assert_eq!(v.with.map(|a| a.percent()), Some(100));
        assert_eq!(v.without.map(|a| a.percent()), Some(25));
        assert_eq!(
            v.what,
            "With the part, your agent did what it says. Without it, it didn't."
        );
        // What the person kept couldn't be read: the verdict stands, without how close.
        assert_eq!(verdict(None, &answer).why, v.what);
        assert_eq!(
            v.why,
            "With the part, your agent did what it says. Without it, it didn't. With it, the work was 100% like what you kept; without it, 25%."
        );
    }

    #[test]
    fn a_part_the_agent_follows_anyway_makes_no_difference() {
        let answer = Answer {
            with: run(KEPT, true),
            without: run(KEPT, true),
            wording: None,
        };
        let v = verdict(Some(&lines_of(KEPT)), &answer);
        assert_eq!(v.outcome, Outcome::NoDifference);
        assert!(
            v.why
                .starts_with("Your agent did what it says without the part too.")
        );

        // The agent may say it didn't, but the same lines are the same work.
        let answer = Answer {
            with: run(KEPT, true),
            without: run(KEPT, false),
            wording: None,
        };
        let v = verdict(Some(&lines_of(KEPT)), &answer);
        assert_eq!(v.outcome, Outcome::NoDifference);
        assert!(
            v.why
                .starts_with("Both runs changed the same lines, 100% alike")
        );
    }

    #[test]
    fn a_part_not_followed_with_it_failed() {
        let mut with = run("", false);
        with.shows = "it kept the error a string".into();
        let answer = Answer {
            with,
            without: run("", false),
            wording: Some("### Every error is a variant\nSo callers can match on it.".into()),
        };
        let v = verdict(Some(&lines_of(KEPT)), &answer);
        assert_eq!(v.outcome, Outcome::Failed);
        assert!(v.why.starts_with(
            "With the part, your agent still didn't do what it says: It kept the error a string."
        ));
    }

    #[test]
    fn a_parts_checks_add_up_to_what_the_page_shows() {
        use Outcome::*;
        assert_eq!(overall(&[]), None);
        assert_eq!(
            overall(&[Works, Works]),
            Some((PartCheck::Works, "Better on 2 of 2 past tasks".into()))
        );
        assert_eq!(
            overall(&[Works, NoDifference]),
            Some((
                PartCheck::Works,
                "Better on 1 of 2 past tasks, the same on 1".into()
            ))
        );
        assert_eq!(
            overall(&[Failed, NoDifference]),
            Some((
                PartCheck::Failed,
                "Not followed on 1 of 2 past tasks".into()
            ))
        );
        assert_eq!(
            overall(&[Works, Works, Failed]).map(|o| o.0),
            Some(PartCheck::Works)
        );
        assert_eq!(
            overall(&[NoDifference, NoDifference]),
            Some((
                PartCheck::NoDifference,
                "The same on both past tasks, with or without it".into()
            ))
        );
    }

    #[test]
    fn an_answer_in_the_wrong_shape_says_what_to_change() {
        let good = json!({
            "with": { "diff": KEPT, "said": "Done.", "followed": true, "shows": "the enum" },
            "without": { "diff": "", "said": "Done.", "followed": false, "shows": "a string" }
        });
        assert!(read(&good, true).is_ok());
        // Fits you checks no part.
        let fits = json!({ "with": { "diff": KEPT, "said": "Done." }, "without": { "diff": "", "said": "Done." } });
        assert!(read(&fits, false).is_ok());
        assert!(read(&fits, true).unwrap_err().contains("followed"));

        let mut not_a_diff = good.clone();
        not_a_diff["with"]["diff"] = json!("I changed fetch.rs");
        assert!(
            read(&not_a_diff, true)
                .unwrap_err()
                .contains("isn't a diff")
        );

        let mut failed = good.clone();
        failed["with"]["followed"] = json!(false);
        assert!(read(&failed, true).unwrap_err().contains("give wording"));
        failed["wording"] = json!("Every error is a variant");
        assert!(read(&failed, true).unwrap_err().contains("`###` heading"));
        failed["wording"] = json!("### Every error is a variant\nSo callers can match on it.");
        assert_eq!(
            read(&failed, true).unwrap().wording.as_deref(),
            Some("### Every error is a variant\nSo callers can match on it.")
        );
    }

    #[test]
    fn the_run_without_a_part_is_given_the_skill_without_it() {
        let text = "---\nname: rust\n---\n\n# Rust\n\n## How you like it\n\n### One\nKeep.\n\n### Two\nGo.\n";
        let without = without_part(text, "two").unwrap();
        assert!(without.contains("### One"));
        assert!(!without.contains("### Two"));
        assert_eq!(without_part(text, "three"), None);
        assert_eq!(
            split_part("### Two\nGo.\n"),
            Some(("Two".to_string(), "Go."))
        );
    }
}
