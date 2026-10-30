//! Matching lessons to the moment an agent is in. A lesson that fits loosely
//! does harm: agents copy what they're shown. So kelve gives nothing unless a
//! lesson passes two gates.
//!
//! - **Fit:** nothing in its "holds when" fails against the work's context
//!   (versions from lockfiles, a citation style, a language); what can't be
//!   told passes, with a check first.
//! - **Relevance:** as something fails, its codes or its words match the
//!   problem's; as a file is edited, one of its cues is in what was just
//!   written; as one is read, one is in the file.
//!
//! What passes is ranked by how relevant it is, then by a draw from how
//! often it has held, so a new lesson can earn its evidence. Every lesson
//! comes with a match card, made from its own words only (by the graph, or
//! here when it sends none): nothing about anyone's work goes into it, and
//! matching it is plain text matching, on the person's computer.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::lesson::{Lesson, Record, Standing};
use crate::text::re;

/// What a lesson is matched by.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Card {
    /// Words or names that show it's relevant where they appear: `thread_rng`,
    /// `\cite{`, "et al.".
    pub cues: Vec<String>,
    /// Codes and rule ids of the problems it answers: `E0425`, `ValueError`.
    pub codes: Vec<String>,
    /// The problem it answers, as different tools and versions print it,
    /// normalised (`lesson::seen_as`).
    pub variants: Vec<String>,
    /// The kinds of file it's for: `bibliography`, `test`, `data file`.
    pub roles: Vec<String>,
    /// The commands or tools it's for: `cargo`, `pandoc`, `pytest`.
    pub tools: Vec<String>,
    /// "Holds when", as conditions kelve can check against the work.
    pub holds: Vec<Facet>,
}

/// One condition a lesson holds under.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Facet {
    pub axis: Axis,
    /// What it's about on that axis: a package, a style guide, a setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// On an ordered axis (a version, an edition, a date): from this one on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub introduced: Option<String>,
    /// And before this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fixed: Option<String>,
    /// On a list axis (a place, a language, a platform, a setting): one of these.
    #[serde(default, rename = "in", skip_serializing_if = "Vec::is_empty")]
    pub any_of: Vec<String>,
}

/// What a condition is about.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Axis {
    Version,
    Edition,
    Jurisdiction,
    Period,
    Locale,
    Platform,
    Setting,
}

/// What kelve knows of the work's context, read from its files on the
/// person's computer: never sent anywhere.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// Versions of what it uses, by name in lower case: packages, toolchains.
    pub versions: BTreeMap<String, String>,
    /// Its settings, by key: `citation-style` (`apa`), `locale` (`en-GB`),
    /// `prose-style` (`microsoft`).
    pub settings: BTreeMap<String, String>,
}

/// Whether a lesson holds for the work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fit {
    Holds,
    /// Nothing says it doesn't, and not everything can be told: check first.
    Unknown,
    /// It doesn't, and why.
    Not(String),
}

static CODE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"\b(?:[A-Z]{1,4}\d{3,5}|[A-Z][A-Za-z]+\.[A-Z][A-Za-z]+|[A-Z]\w*(?:Error|Exception|Warning))\b",
    )
});
static SPAN: LazyLock<Regex> = LazyLock::new(|| re(r"`([^`]{2,80})`"));
static LATER: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b([a-z][\w.+-]*)\s+v?(\d+(?:\.\d+){0,2})\s*(?:and later|or later|and up|and newer|onwards|or newer|\+)",
    )
});
static BEFORE: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b([a-z][\w.+-]*)\s+(?:before|until|earlier than|older than|below)\s+v?(\d+(?:\.\d+){0,2})",
    )
});
static EDITION: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(APA|MLA|Chicago|Harvard|IEEE|Vancouver|OSCOLA|Bluebook)\s*(?:style\s*)?,?\s*(\d{1,2})(?:st|nd|rd|th)?(?:\s*edition)?\b",
    )
});
static LOCALE: LazyLock<Regex> = LazyLock::new(|| re(r"\b([a-z]{2}-[A-Z]{2})\b"));

/// The codes and typed errors in some text.
pub fn codes_in(text: &str) -> BTreeSet<String> {
    CODE.find_iter(text)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// A lesson's card, made from its own words in code: the stand-in graph's,
/// and kelve's own when the graph sends none. The real graph's is fuller.
pub fn card(lesson: &Lesson) -> Card {
    let mut cues: Vec<String> = Vec::new();
    for text in [&lesson.says, &lesson.act] {
        for c in SPAN.captures_iter(text) {
            let span = c[1]
                .trim()
                .trim_end_matches("()")
                .trim_end_matches('(')
                .to_string();
            let last = span
                .rsplit("::")
                .next()
                .unwrap_or(&span)
                .rsplit('.')
                .next()
                .unwrap_or(&span)
                .to_string();
            for cue in [span, last] {
                if cue.chars().count() >= 3 && !cues.contains(&cue) {
                    cues.push(cue);
                }
            }
        }
    }
    let seen = lesson.seen_as.clone().unwrap_or_default();
    // A better way's `Seen as` is the usual way as an agent writes it: its
    // cue is found as the usual way is written (`lesson::cue`).
    if let Some(cue) = crate::lesson::cue(&seen)
        && !cues.contains(&cue)
    {
        cues.push(cue);
    }
    let mut holds = Vec::new();
    for c in LATER.captures_iter(&lesson.holds_when) {
        holds.push(Facet {
            axis: Axis::Version,
            name: Some(c[1].to_lowercase()),
            introduced: Some(c[2].to_string()),
            fixed: None,
            any_of: Vec::new(),
        });
    }
    for c in BEFORE.captures_iter(&lesson.holds_when) {
        holds.push(Facet {
            axis: Axis::Version,
            name: Some(c[1].to_lowercase()),
            introduced: None,
            fixed: Some(c[2].to_string()),
            any_of: Vec::new(),
        });
    }
    for c in EDITION.captures_iter(&lesson.holds_when) {
        let n: u32 = c[2].parse().unwrap_or(0);
        holds.push(Facet {
            axis: Axis::Edition,
            name: Some(c[1].to_lowercase()),
            introduced: Some(n.to_string()),
            fixed: Some((n + 1).to_string()),
            any_of: Vec::new(),
        });
    }
    let locales: Vec<String> = LOCALE
        .captures_iter(&lesson.holds_when)
        .map(|c| c[1].to_string())
        .collect();
    if !locales.is_empty() {
        holds.push(Facet {
            axis: Axis::Locale,
            name: None,
            introduced: None,
            fixed: None,
            any_of: locales,
        });
    }
    Card {
        cues,
        codes: codes_in(&seen).into_iter().collect(),
        variants: if seen.is_empty() {
            Vec::new()
        } else {
            vec![seen]
        },
        holds,
        ..Card::default()
    }
}

/// A version's numbers, for ordering: `0.9.2` before `0.10`.
fn numbers(v: &str) -> Vec<u64> {
    v.trim_start_matches(['v', 'V', '='])
        .split(['.', '-', '+'])
        .map_while(|p| p.parse::<u64>().ok())
        .collect()
}

fn order(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (numbers(a), numbers(b));
    for i in 0..a.len().max(b.len()) {
        let o = a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0));
        if o.is_ne() {
            return o;
        }
    }
    std::cmp::Ordering::Equal
}

/// Whether a value on an ordered axis is within a condition's range.
fn within(value: &str, f: &Facet) -> bool {
    f.introduced
        .as_deref()
        .is_none_or(|i| order(value, i).is_ge())
        && f.fixed.as_deref().is_none_or(|x| order(value, x).is_lt())
}

/// Whether a lesson holds for the work, by its card's conditions: it
/// doesn't if any one doesn't; it's unknown if any can't be told.
pub fn fits(card: &Card, ctx: &Context) -> Fit {
    let mut unknown = false;
    for f in &card.holds {
        let name = f.name.as_deref().map(str::to_lowercase);
        let known: Option<bool> = match f.axis {
            Axis::Version => name
                .as_deref()
                .and_then(|n| ctx.versions.get(n))
                .map(|v| within(v, f)),
            Axis::Edition => match (name.as_deref(), ctx.settings.get("citation-style")) {
                (Some(n), Some(style)) if !style.to_lowercase().starts_with(n) => Some(false),
                (Some(_), Some(style)) => {
                    // A style named with its edition says which: `apa-6th`.
                    let edition: String = style.chars().filter(char::is_ascii_digit).collect();
                    if edition.is_empty() {
                        None
                    } else {
                        Some(within(&edition, f))
                    }
                }
                _ => None,
            },
            Axis::Locale => ctx
                .settings
                .get("locale")
                .map(|l| f.any_of.iter().any(|x| x.eq_ignore_ascii_case(l))),
            Axis::Setting => name
                .as_deref()
                .and_then(|n| ctx.settings.get(n))
                .map(|v| f.any_of.iter().any(|x| x.eq_ignore_ascii_case(v))),
            _ => None,
        };
        match known {
            Some(false) => {
                let what = f
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", f.axis).to_lowercase());
                let has = name
                    .as_deref()
                    .and_then(|n| ctx.versions.get(n).or_else(|| ctx.settings.get(n)))
                    .or_else(|| {
                        ctx.settings.get(if f.axis == Axis::Locale {
                            "locale"
                        } else {
                            "citation-style"
                        })
                    })
                    .cloned()
                    .unwrap_or_default();
                return Fit::Not(format!("this work uses {what} {has}").trim().to_string());
            }
            None => unknown = true,
            Some(true) => {}
        }
    }
    if unknown { Fit::Unknown } else { Fit::Holds }
}

/// A problem, as kelve tells one from another: the tool, its codes and its
/// words, from the command and the problem as `seen_as` made it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fingerprint {
    pub tool: String,
    pub codes: BTreeSet<String>,
    pub template: String,
}

/// The tool a command runs: its first word, without its folder.
pub fn tool_of(command: &str) -> String {
    let first = command
        .split_whitespace()
        .find(|w| {
            !w.contains('=')
                && !["sudo", "npx", "bunx", "uv", "poetry", "pnpm", "yarn", "env"].contains(w)
        })
        .unwrap_or_default();
    first.rsplit('/').next().unwrap_or(first).to_lowercase()
}

/// The tools a shell command runs, each part of it (`a && b | c; d`) by its
/// program's name, as published: `cargo`, `pytest`, `playwright`, `pgrep`.
/// Never the shell's own words (`cd`, `echo`, `if`), never a path or a script
/// of the work's own (`./run.sh`, `dev/kelve.ts`), and never anything that
/// isn't a plain name. What kelve finds lessons about as the agent works, with
/// no file read.
pub fn tools_in(command: &str) -> Vec<String> {
    // What runs another program, and the shell's words before one: passed
    // over to reach the program.
    const LAUNCHERS: [&str; 26] = [
        "sudo", "npx", "bunx", "uv", "uvx", "poetry", "pnpm", "yarn", "env", "time", "nice",
        "nohup", "exec", "command", "run", "x", "dlx", "if", "elif", "then", "else", "while",
        "until", "do", "!", "{",
    ];
    // The shell's own commands and words: no program runs.
    const SHELL: [&str; 19] = [
        "cd", "echo", "printf", "export", "set", "unset", "source", ".", "true", "false", "fi",
        "for", "done", "case", "esac", "exit", "read", "test", "[",
    ];
    let mut out: Vec<String> = Vec::new();
    let parts = command.split(['\n', ';', '|', '&', '(', ')', '{', '}', '`']);
    for part in parts {
        let Some(word) = part
            .split_whitespace()
            .find(|w| !w.contains('=') && !LAUNCHERS.contains(&w.to_lowercase().as_str()))
        else {
            continue;
        };
        let name = word.to_lowercase();
        // A plain program name, as it's published: python3 is python.
        let plain = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-_".contains(c))
            && name.starts_with(|c: char| c.is_ascii_lowercase());
        if !plain || SHELL.contains(&name.as_str()) {
            continue;
        }
        let name = name.trim_end_matches(|c: char| c.is_ascii_digit());
        if name.len() >= 2 && !out.iter().any(|o| o == name) {
            out.push(name.to_string());
        }
    }
    out
}

pub fn fingerprint(command: &str, seen: &str) -> Fingerprint {
    Fingerprint {
        tool: tool_of(command),
        codes: codes_in(seen),
        template: seen.trim().to_lowercase(),
    }
}

fn grams(s: &str) -> BTreeSet<String> {
    let padded: Vec<char> = format!("  {s} ").chars().collect();
    padded.windows(3).map(|w| w.iter().collect()).collect()
}

/// How alike two texts are, by the three-letter runs they share: 0 to 1.
pub fn alike(a: &str, b: &str) -> f64 {
    let (ga, gb) = (grams(a), grams(b));
    let union = ga.union(&gb).count();
    if union == 0 {
        return 0.0;
    }
    ga.intersection(&gb).count() as f64 / union as f64
}

/// Whether two problems are the same one: the same tool, and the same codes
/// when they have codes, else nearly the same words. What "the same error
/// came back" asks.
pub fn same_problem(a: &Fingerprint, b: &Fingerprint) -> bool {
    if !a.tool.is_empty() && !b.tool.is_empty() && a.tool != b.tool {
        return false;
    }
    if !a.codes.is_empty() && !b.codes.is_empty() {
        return a.codes == b.codes && alike(&a.template, &b.template) >= 0.35;
    }
    alike(&a.template, &b.template) >= 0.6
}

/// How closely a card answers a problem, from 0 to 1: a shared code, or how
/// alike its words are to the nearest way the card has it.
pub fn answers(card: &Card, problem: &Fingerprint) -> f64 {
    let codes: BTreeSet<&str> = card.codes.iter().map(String::as_str).collect();
    let shared = problem.codes.iter().any(|c| codes.contains(c.as_str()));
    let words = card
        .variants
        .iter()
        .map(|v| alike(&v.to_lowercase(), &problem.template))
        .fold(0.0_f64, f64::max);
    if shared { words.max(0.75) } else { words }
}

/// How alike a problem must be to a card's to be the one it answers.
pub const ANSWERS: f64 = 0.5;

/// Whether a cue is in some text: a name as it's written, a word as a word.
pub fn cue_in(cue: &str, text: &str) -> bool {
    let cue = cue.trim();
    if cue.chars().count() < 3 {
        return false;
    }
    let plain = cue.chars().all(|c| c.is_alphabetic() || c == ' ');
    if !plain {
        return text.contains(cue);
    }
    let lower = text.to_lowercase();
    let cue = cue.to_lowercase();
    lower.match_indices(&cue).any(|(at, _)| {
        let before = lower[..at].chars().next_back();
        let after = lower[at + cue.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// The moment a lesson is matched for.
#[derive(Clone, Copy, Debug)]
pub enum At<'a> {
    /// Something failed: the problem as kelve tells it.
    Failed(&'a Fingerprint),
    /// A file was edited: what was written, and the file as it is now.
    Edited { written: &'a str, file: &'a str },
    /// A file was read: the file.
    Read { file: &'a str },
}

/// A lesson to match.
#[derive(Clone, Debug)]
pub struct Candidate<'a> {
    pub id: &'a str,
    pub about: &'a [String],
    pub card: &'a Card,
    pub record: Record,
    pub standing: Standing,
    pub seen_as: bool,
}

/// A lesson chosen: how relevant (3, an exact fit, to 1), and whether it
/// holds or must be checked first.
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
    pub id: String,
    pub tier: u8,
    pub fit: Fit,
}

/// A draw from Beta(a, b), for whole a and b: the a-th smallest of a + b - 1
/// uniform draws. Kept to a hundred draws at most.
fn beta(a: u32, b: u32, draw: &mut impl FnMut() -> f64) -> f64 {
    let (a, b) = (a.clamp(1, 60), b.clamp(1, 60));
    let mut u: Vec<f64> = (0..a + b - 1).map(|_| draw()).collect();
    u.sort_by(f64::total_cmp);
    u[(a - 1) as usize]
}

/// The lessons to give at a moment, at most `most`: none that doesn't hold
/// or isn't relevant; the most relevant first, then by a draw from how often
/// each has held, so a new one can earn its evidence; one for each idea.
/// `draw` gives a uniform number in [0, 1).
pub fn choose(
    at: At,
    candidates: &[Candidate],
    ctx: &Context,
    most: usize,
    draw: &mut impl FnMut() -> f64,
) -> Vec<Chosen> {
    let mut scored: Vec<(u8, f64, &Candidate, Fit)> = Vec::new();
    for c in candidates {
        if c.standing == Standing::Doubted {
            continue;
        }
        let fit = fits(c.card, ctx);
        if matches!(fit, Fit::Not(_)) {
            continue;
        }
        let cue = |text: &str| c.card.cues.iter().any(|q| cue_in(q, text));
        let tier = match at {
            At::Failed(problem) => {
                let score = answers(c.card, problem);
                if score >= 0.95 {
                    3
                } else if score >= ANSWERS {
                    2
                } else {
                    0
                }
            }
            At::Edited { written, file } => {
                if cue(written) {
                    3
                } else if cue(file) && (!c.seen_as || c.standing == Standing::Confirmed) {
                    1
                } else {
                    0
                }
            }
            At::Read { file } => {
                if cue(file) && (!c.seen_as || c.standing == Standing::Confirmed) {
                    1
                } else {
                    0
                }
            }
        };
        // What can't be told to hold is given at a file only when it fits
        // exactly; at a failure, the match is itself a sign it holds.
        let enough = match (&fit, at) {
            (Fit::Unknown, At::Failed(_)) => tier >= 2,
            (Fit::Unknown, _) => tier >= 3,
            _ => tier >= 1,
        };
        if !enough {
            continue;
        }
        let r = c.record;
        let held = r.held + u32::from(r.backed);
        let sample = beta(1 + held, 1 + 2 * r.didnt, draw);
        scored.push((tier, sample, c, fit));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.total_cmp(&a.1)));
    let mut ideas: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for (tier, _, c, fit) in scored {
        if out.len() >= most {
            break;
        }
        if c.about.iter().any(|a| ideas.contains(a.as_str())) {
            continue;
        }
        ideas.extend(c.about.iter().map(String::as_str));
        out.push(Chosen {
            id: c.id.to_string(),
            tier,
            fit,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::lesson::seen_as;

    #[test]
    fn a_commands_tools_are_its_programs_never_the_shells_words_or_the_works_scripts() {
        assert_eq!(
            tools_in("cd web && npx playwright test 2>&1 | tail -40; echo done"),
            ["playwright", "tail"]
        );
        assert_eq!(
            tools_in("RUST_LOG=debug cargo test -p store && python3 -m pytest"),
            ["cargo", "python"]
        );
        assert_eq!(tools_in("uv run pytest -q"), ["pytest"]);
        assert_eq!(tools_in("pgrep -f 'playwright test'"), ["pgrep"]);
        assert!(
            tools_in("./run.sh && dev/kelve.ts up && bun dev/kelve.ts")
                .iter()
                .all(|t| t == "bun")
        );
        assert!(tools_in("if [ -f x ]; then true; fi").is_empty());
        assert_eq!(
            tools_in("until ! pgrep -f 'server --port'; do sleep 1; done"),
            ["pgrep", "sleep"]
        );
        assert_eq!(
            tools_in("cd web && if grep -q x a; then ls; fi"),
            ["grep", "ls"]
        );
        assert!(tools_in("test -f x && [ -d y ]").is_empty());
    }

    fn lesson(says: &str, act: &str, holds: &str, seen: Option<&str>) -> Lesson {
        Lesson {
            about: vec!["kelve:i1".into()],
            says: says.into(),
            act: act.into(),
            example: None,
            holds_when: holds.into(),
            backed_by: None,
            seen_as: seen.map(seen_as),
        }
    }

    #[test]
    fn a_card_is_made_from_the_lessons_own_words() {
        let l = lesson(
            "rand 0.9 renamed `thread_rng()` to `rng()`.",
            "Call `rand::rng()`.",
            "rand 0.9 and later.",
            Some("error[E0425]: cannot find function `thread_rng` in crate `rand`"),
        );
        let c = card(&l);
        assert!(c.cues.contains(&"thread_rng".to_string()), "{:?}", c.cues);
        assert!(c.cues.contains(&"rand::rng".to_string()));
        assert_eq!(c.codes, ["E0425"]);
        assert_eq!(c.holds[0].name.as_deref(), Some("rand"));
        assert_eq!(c.holds[0].introduced.as_deref(), Some("0.9"));
        let apa = card(&lesson(
            "APA 7 drops the publisher's location.",
            "Leave it out.",
            "APA 7th edition.",
            None,
        ));
        assert_eq!(apa.holds[0].axis, Axis::Edition);
    }

    #[test]
    fn a_lesson_that_doesnt_hold_for_the_work_is_never_given() {
        let c = card(&lesson("x", "y", "rand 0.9 and later.", None));
        let mut ctx = Context::default();
        assert_eq!(fits(&c, &ctx), Fit::Unknown);
        ctx.versions.insert("rand".into(), "0.8.5".into());
        assert_eq!(fits(&c, &ctx), Fit::Not("this work uses rand 0.8.5".into()));
        ctx.versions.insert("rand".into(), "0.10.1".into());
        assert_eq!(fits(&c, &ctx), Fit::Holds);
        let apa = card(&lesson("x", "y", "APA 7th edition.", None));
        ctx.settings
            .insert("citation-style".into(), "chicago-author-date".into());
        assert!(matches!(fits(&apa, &ctx), Fit::Not(_)));
        ctx.settings
            .insert("citation-style".into(), "apa-6th-edition".into());
        assert!(matches!(fits(&apa, &ctx), Fit::Not(_)));
        ctx.settings.insert("citation-style".into(), "apa".into());
        assert_eq!(fits(&apa, &ctx), Fit::Unknown);
    }

    #[test]
    fn the_same_problem_is_told_by_its_codes_and_words() {
        let a = fingerprint(
            "cargo test",
            &seen_as("error[E0425]: cannot find function `thread_rng`"),
        );
        let b = fingerprint(
            "cargo test --test retry",
            &seen_as("error[E0425]: cannot find function `thread_rng` in this scope"),
        );
        let c = fingerprint("cargo test", &seen_as("error[E0308]: mismatched types"));
        assert!(same_problem(&a, &b));
        assert!(!same_problem(&a, &c));
        let py1 = fingerprint(
            "pytest",
            &seen_as("ValueError: could not convert string to float: 'n/a'"),
        );
        let py2 = fingerprint("pytest", &seen_as("KeyError: 'amount'"));
        assert!(
            !same_problem(&py1, &py2),
            "two errors of one command aren't the same"
        );
    }

    #[test]
    fn nothing_is_given_unless_it_fits_and_matters_here() {
        let rename = lesson(
            "rand 0.9 renamed `thread_rng()` to `rng()`.",
            "Call `rand::rng()`.",
            "rand 0.9 and later.",
            Some("error[E0425]: cannot find function `thread_rng` in crate `rand`"),
        );
        let general = lesson(
            "Full jitter spreads callers out the most.",
            "Wait a random time below the ceiling.",
            "Retries from many callers.",
            None,
        );
        let (c1, c2) = (card(&rename), card(&general));
        let about = vec!["kelve:i1".to_string()];
        let about2 = vec!["kelve:i2".to_string()];
        let cands = [
            Candidate {
                id: "l1",
                about: &about,
                card: &c1,
                record: Record::default(),
                standing: Standing::New,
                seen_as: true,
            },
            Candidate {
                id: "l2",
                about: &about2,
                card: &c2,
                record: Record {
                    held: 30,
                    didnt: 0,
                    backed: false,
                },
                standing: Standing::Confirmed,
                seen_as: false,
            },
        ];
        let mut n = 0.0;
        let mut draw = || {
            n = (n + 0.37) % 1.0;
            n
        };
        let ctx = Context::default();
        // Read: the general lesson has no cue in the file, the rename can't be told to hold.
        assert!(
            choose(
                At::Read {
                    file: "let mut r = thread_rng();"
                },
                &cands,
                &ctx,
                2,
                &mut draw
            )
            .is_empty()
        );
        // Edited, writing the old name: given, with a check first.
        let got = choose(
            At::Edited {
                written: "let r = rand::thread_rng();",
                file: "",
            },
            &cands,
            &ctx,
            2,
            &mut draw,
        );
        assert_eq!(
            got.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["l1"]
        );
        assert_eq!(got[0].fit, Fit::Unknown);
        // Failed with its error: given.
        let p = fingerprint(
            "cargo build",
            &seen_as(
                "error[E0425]: cannot find function `thread_rng` in crate `rand`\n --> src/x.rs:3:9",
            ),
        );
        assert_eq!(
            choose(At::Failed(&p), &cands, &ctx, 2, &mut draw)[0].id,
            "l1"
        );
        // Failed with another: nothing.
        let q = fingerprint("cargo build", &seen_as("error[E0308]: mismatched types"));
        assert!(choose(At::Failed(&q), &cands, &ctx, 2, &mut draw).is_empty());
        // The work uses rand 0.8: never.
        let mut old = Context::default();
        old.versions.insert("rand".into(), "0.8.5".into());
        assert!(choose(At::Failed(&p), &cands, &old, 2, &mut draw).is_empty());
    }

    #[test]
    fn a_cue_is_a_name_as_written_or_a_word_as_a_word() {
        assert!(cue_in("thread_rng", "rand::thread_rng()"));
        assert!(cue_in("et al", "Smith et al. (2020)"));
        assert!(!cue_in("key", "monkeys"));
        assert!(!cue_in("ab", "ab"));
    }
}
