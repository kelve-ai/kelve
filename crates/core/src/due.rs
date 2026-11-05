//! Whether a piece of work is worth a debrief, decided by kelve from the
//! change itself, never by the agent. A debrief is due when the work changes
//! more than about 20 lines, adds a file, a dependency or a command, or its
//! handover names a decision. None is due when it's small and mechanical (a
//! typo, a rename, formatting, a version bump, files a tool wrote), when the
//! person's messages already said exactly what to write, or when it changed
//! nothing. Each answer comes with its reason, in plain words, for home.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::skills::work::made_by_tools;
use crate::text::re;

/// More lines changed than this, and a debrief is due: a first guess, to
/// tune on real work.
pub const LINES: u32 = 20;

/// How often the person wants a debrief, from settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DebriefsWhen {
    /// For work worth one.
    #[default]
    WorthOne,
    /// For every handover.
    Every,
    /// Only when they ask.
    OnlyAsked,
}

/// What happened to a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// One file of a change, as `due` needs it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub status: Status,
    pub added: u32,
    pub removed: u32,
    /// Whether all that changed in it is spaces and line breaks.
    pub whitespace_only: bool,
    /// The lines it adds and takes out, as far as kelve kept them: what's
    /// new in manifests and Makefiles, and what the person's messages
    /// already said.
    pub added_lines: Vec<String>,
    pub removed_lines: Vec<String>,
}

impl File {
    /// A file with only its counts.
    pub fn new(path: &str, status: Status, added: u32, removed: u32) -> File {
        File {
            path: path.to_string(),
            status,
            added,
            removed,
            whitespace_only: false,
            added_lines: Vec::new(),
            removed_lines: Vec::new(),
        }
    }
}

/// What the agent said as it handed over, and what the person said.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handover {
    /// What the agent said of its work.
    pub said: String,
    /// The person's own messages to their agent in this piece of work: when
    /// they already said exactly what to write, the agent decided nothing.
    pub messages: Vec<String>,
}

/// Whether a debrief is due, and why, in a sentence for home.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename = "DebriefDue")]
pub struct Due {
    pub due: bool,
    pub reason: String,
}

impl Due {
    fn yes(reason: impl Into<String>) -> Due {
        Due {
            due: true,
            reason: reason.into(),
        }
    }

    fn no(reason: impl Into<String>) -> Due {
        Due {
            due: false,
            reason: reason.into(),
        }
    }
}

static DECIDED: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"(?i)\b(?:decided|decide to|chose|chosen|choosing|opted|went with|instead of|rather than|trade-?offs?|picked|over the alternative)\b",
    )
});

/// Manifests, by name, and what they list.
const MANIFESTS: &[&str] = &[
    "cargo.toml",
    "package.json",
    "pyproject.toml",
    "requirements.txt",
    "requirements-dev.txt",
    "pipfile",
    "setup.py",
    "setup.cfg",
    "go.mod",
    "gemfile",
    "composer.json",
    "build.gradle",
    "build.gradle.kts",
    "pom.xml",
    "mix.exs",
    "pubspec.yaml",
    "deno.json",
    "chart.yaml",
    "version",
    "version.txt",
];

/// Files that run commands, by name.
const COMMANDS: &[&str] = &["makefile", "justfile", "taskfile.yml", "taskfile.yaml"];

/// Keys in a manifest that aren't dependencies.
const NOT_DEPENDENCIES: &[&str] = &[
    "version",
    "name",
    "edition",
    "description",
    "license",
    "authors",
    "author",
    "rust-version",
    "repository",
    "readme",
    "homepage",
    "documentation",
    "keywords",
    "categories",
    "publish",
    "resolver",
    "members",
    "exclude",
    "include",
    "main",
    "module",
    "types",
    "type",
    "private",
    "workspaces",
    "engines",
    "node",
    "npm",
    "bun",
    "packagemanager",
    "requires-python",
    "python",
    "go",
    "toolchain",
    "path",
    "workspace",
    "default",
    "features",
    "optional",
    "lints",
    "build",
    "test",
    "dev",
    "start",
    "lint",
    "format",
    "typecheck",
];

static TOML_DEP: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"^\s*([A-Za-z0-9_.-]+)\s*=\s*(?:"[\^~=<>]?\d[^"]*"|\{.*\b(?:version|path|git|workspace)\b)"#,
    )
});
static JSON_DEP: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"^\s*"(@?[A-Za-z0-9_./-]+)"\s*:\s*"(?:[\^~=<>]*\s*\d|workspace:|npm:|file:|github:|git\+|latest)"#,
    )
});
static JSON_SCRIPT: LazyLock<Regex> =
    LazyLock::new(|| re(r#"^\s*"([A-Za-z0-9_:.-]+)"\s*:\s*"([^"]*)""#));
static PY_DEP: LazyLock<Regex> =
    LazyLock::new(|| re(r#"^\s*([A-Za-z0-9_.-]+)\s*(?:\[[^\]]*\])?\s*(?:[<>=!~]=?|@)"#));
static PY_QUOTED: LazyLock<Regex> =
    LazyLock::new(|| re(r#"^\s*"([A-Za-z0-9_.-]+)\s*(?:\[[^\]]*\])?\s*(?:[<>=!~]=?|@)"#));
static GO_DEP: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\s*(?:require\s+)?([a-z0-9.-]+\.[a-z]{2,}/[^\s]+)\s+v\d"));
static GEM: LazyLock<Regex> = LazyLock::new(|| re(r#"^\s*gem\s+['"]([^'"]+)['"]"#));
static TARGET: LazyLock<Regex> =
    LazyLock::new(|| re(r"^([A-Za-z0-9][A-Za-z0-9_.-]*)\s*:(?:[^=]|$)"));
static PY_SCRIPT: LazyLock<Regex> =
    LazyLock::new(|| re(r#"^\s*([A-Za-z0-9_-]+)\s*=\s*"[A-Za-z0-9_.]+:[A-Za-z0-9_.]+""#));
static BIN: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*\[\[bin\]\]"));

/// A file's name, lower case.
fn name_of(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// The dependencies a manifest's lines name.
fn dependencies(name: &str, lines: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in lines {
        let found = match name {
            "cargo.toml" | "pipfile" => TOML_DEP.captures(line).and_then(|c| c.get(1)),
            "pyproject.toml" => TOML_DEP
                .captures(line)
                .or_else(|| PY_QUOTED.captures(line))
                .and_then(|c| c.get(1)),
            "package.json" | "composer.json" | "deno.json" => {
                JSON_DEP.captures(line).and_then(|c| c.get(1))
            }
            "go.mod" => GO_DEP.captures(line).and_then(|c| c.get(1)),
            "gemfile" => GEM.captures(line).and_then(|c| c.get(1)),
            n if n.starts_with("requirements") => {
                let t = line.trim();
                (!t.is_empty() && !t.starts_with('#') && !t.starts_with('-'))
                    .then(|| PY_DEP.captures(t).and_then(|c| c.get(1)))
                    .flatten()
            }
            _ => None,
        };
        if let Some(m) = found {
            let dep = m.as_str().trim_matches('"').to_lowercase();
            if !NOT_DEPENDENCIES.contains(&dep.as_str()) {
                out.insert(dep);
            }
        }
        // A whole requirements line with no version is a dependency too.
        if name.starts_with("requirements") {
            let t = line.trim();
            if !t.is_empty()
                && !t.starts_with('#')
                && !t.starts_with('-')
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.[]".contains(c))
            {
                out.insert(t.to_lowercase());
            }
        }
    }
    out
}

/// The commands a file's lines name: Makefile targets, package scripts,
/// binaries and console scripts.
fn commands(name: &str, lines: &[String]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in lines {
        let found = match name {
            n if COMMANDS.contains(&n) => TARGET
                .captures(line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .filter(|t| !t.starts_with('.')),
            "package.json" => JSON_SCRIPT.captures(line).and_then(|c| {
                let key = c.get(1)?.as_str();
                let value = c.get(2)?.as_str();
                // A script runs something; a version or a path doesn't.
                let runs = value.contains(' ')
                    && !value.starts_with(|ch: char| ch.is_ascii_digit() || "^~<>=".contains(ch));
                (runs && !NOT_DEPENDENCIES.contains(&key.to_lowercase().as_str()))
                    .then(|| key.to_string())
            }),
            "cargo.toml" => BIN.is_match(line).then(|| "[[bin]]".to_string()),
            "pyproject.toml" | "setup.cfg" => PY_SCRIPT
                .captures(line)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string()),
            _ => None,
        };
        out.extend(found);
    }
    out
}

/// Whether a file is a manifest, by its name.
fn manifest(path: &str) -> bool {
    MANIFESTS.contains(&name_of(path).as_str())
}

/// Whether a file is a changelog.
fn changelog(path: &str) -> bool {
    let n = name_of(path);
    n.starts_with("changelog") || n.starts_with("changes") || n.starts_with("history")
}

/// A list, in words: "a, b and c".
fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Whether a debrief is due for a piece of work, from the change, what was
/// said, and how often the person wants one.
pub fn due(files: &[File], handover: &Handover, when: DebriefsWhen) -> Due {
    let changed: Vec<&File> = files
        .iter()
        .filter(|f| f.added + f.removed > 0 || f.status != Status::Modified)
        .collect();
    if changed.is_empty() {
        return Due::no("Nothing changed.");
    }
    match when {
        DebriefsWhen::OnlyAsked => {
            return Due::no("You asked for debriefs only when you ask for one.");
        }
        DebriefsWhen::Every => {
            return Due::yes("You asked for a debrief of every handover.");
        }
        DebriefsWhen::WorthOne => {}
    }

    // What a person wrote, as against what tools wrote or only moved.
    let written: Vec<&File> = changed
        .iter()
        .copied()
        .filter(|f| !made_by_tools(&f.path) && !f.whitespace_only)
        .filter(|f| !(f.status == Status::Renamed && f.added + f.removed == 0))
        .collect();

    if let Some(e) = said_exactly(&written, &handover.messages) {
        return Due::no(e.reason());
    }

    let decided = DECIDED.find(&handover.said).map(|m| m.as_str().to_string());
    if let Some(word) = &decided
        && !written.is_empty()
    {
        return Due::yes(format!(
            "Your agent said it {}, so it made a call you may want a say in.",
            match word.to_lowercase().as_str() {
                "decided" | "decide to" | "chose" | "chosen" | "choosing" | "opted" | "picked" => {
                    "decided something"
                }
                _ => "weighed one way against another",
            }
        ));
    }

    let mut deps: BTreeSet<String> = BTreeSet::new();
    let mut cmds: BTreeSet<String> = BTreeSet::new();
    for f in &written {
        let name = name_of(&f.path);
        let before = dependencies(&name, &f.removed_lines);
        deps.extend(
            dependencies(&name, &f.added_lines)
                .difference(&before)
                .cloned(),
        );
        let before = commands(&name, &f.removed_lines);
        cmds.extend(commands(&name, &f.added_lines).difference(&before).cloned());
    }
    if !deps.is_empty() {
        let deps: Vec<String> = deps.into_iter().collect();
        return Due::yes(format!(
            "It adds {} {}.",
            if deps.len() == 1 {
                "a dependency,"
            } else {
                "dependencies,"
            },
            and(&deps)
        ));
    }
    if !cmds.is_empty() {
        let cmds: Vec<String> = cmds.into_iter().filter(|c| c != "[[bin]]").collect();
        return Due::yes(if cmds.is_empty() {
            "It adds a command.".to_string()
        } else {
            format!(
                "It adds {} {}.",
                if cmds.len() == 1 {
                    "a command,"
                } else {
                    "commands,"
                },
                and(&cmds)
            )
        });
    }

    let added: Vec<String> = written
        .iter()
        .filter(|f| f.status == Status::Added)
        .map(|f| f.path.clone())
        .collect();
    if let Some(first) = added.first() {
        return Due::yes(if added.len() == 1 {
            format!("It adds a file, {first}.")
        } else {
            format!("It adds {} files, {first} among them.", added.len())
        });
    }

    let bump = !written.is_empty()
        && written
            .iter()
            .all(|f| (manifest(&f.path) && f.added <= 2 && f.removed <= 2) || changelog(&f.path));
    if bump && written.iter().any(|f| manifest(&f.path)) {
        return Due::no("It only changes a version.");
    }

    // Lines changed: for each file, the more of what it added and took out,
    // so a line rewritten counts once.
    let lines: u32 = written.iter().map(|f| f.added.max(f.removed)).sum();
    if lines > LINES {
        return Due::yes(format!(
            "It changes {lines} lines in {}.",
            files_count(written.len())
        ));
    }

    if written.is_empty() {
        let tools = changed.iter().any(|f| made_by_tools(&f.path));
        let moved = changed.iter().any(|f| f.status == Status::Renamed);
        return Due::no(if changed.iter().all(|f| f.whitespace_only) {
            "Only spacing changed."
        } else if tools && !moved {
            "Only files a tool wrote changed."
        } else if moved && !tools {
            "It only moves files."
        } else {
            "Only spacing, moves and files a tool wrote changed."
        });
    }
    Due::no(format!(
        "It's a small change: {lines} {} in {}.",
        if lines == 1 { "line" } else { "lines" },
        files_count(written.len())
    ))
}

/// Of the lines changed, at least this share must be ones the person's
/// messages spelled out for them to have said exactly what to write...
const SPELLED: usize = 80;
/// ...and no more than this many lines may be the agent's own.
const BEYOND: usize = 10;
/// Quoted text in a message, as a named edit gives it.
const QUOTED: &str = r#"`([^`\n]{1,80})`|"([^"\n]{1,80})"|“([^”\n]{1,80})”|'([^'\n]{1,80})'"#;

/// An edit a message names: "change `30` to `60`", "replace "foo" with
/// "bar"", "rename old_name to new_name", "s/foo/bar/".
static NAMED: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)\b(?:replace|change|rename|swap|turn)\s+(?:{QUOTED}|([A-Za-z0-9_.:-]+))\s+(?:with|to|into|for)\s+(?:{q2}|([A-Za-z0-9_.:-]+))",
        q2 = QUOTED
    ))
});
static SED: LazyLock<Regex> = LazyLock::new(|| re(r"\bs/([^/\n]{1,80})/([^/\n]{0,80})/g?"));
static ARROW: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?:{QUOTED})\s*(?:->|→|=>)\s*(?:{q2})",
        q2 = QUOTED
    ))
});

/// How the person's messages said exactly what to write: how many of the
/// lines that changed they spelled out, of how many, and the edit they
/// named, if one made some of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exactly {
    pub spelled: usize,
    pub of: usize,
    pub named: Option<(String, String)>,
}

impl Exactly {
    /// Why no debrief is due, in a sentence for home.
    pub fn reason(&self) -> String {
        let lines = if self.spelled == self.of {
            if self.of == 1 {
                "the one line it changed".to_string()
            } else {
                format!("all {} lines it changed", self.of)
            }
        } else {
            format!("{} of the {} lines it changed", self.spelled, self.of)
        };
        match &self.named {
            Some((from, to)) => format!(
                "Your messages already said exactly what to write: change “{from}” to “{to}”, which is {lines}."
            ),
            None => format!(
                "Your messages already said exactly what to write: {lines} {} in them.",
                if self.spelled == 1 { "is" } else { "are" }
            ),
        }
    }
}

/// A line as it's compared: its words, with every run of spaces as one.
fn squeezed(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether a line says anything: not only brackets, punctuation or a word
/// of two letters.
fn says_something(line: &str) -> bool {
    line.chars().filter(|c| c.is_alphanumeric()).count() >= 3
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `text` holds `line` whole: not as part of a longer word.
fn holds(text: &str, line: &str) -> bool {
    text.match_indices(line).any(|(i, _)| {
        let before = text[..i].chars().next_back();
        let after = text[i + line.len()..].chars().next();
        let starts = line.chars().next().is_some_and(word_char);
        let ends = line.chars().next_back().is_some_and(word_char);
        !(starts && before.is_some_and(word_char)) && !(ends && after.is_some_and(word_char))
    })
}

/// The edits the messages name, each as what's replaced and with what.
fn named_edits(messages: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let pair = |c: regex::Captures<'_>| -> Option<(String, String)> {
        let found: Vec<&str> = c.iter().skip(1).flatten().map(|m| m.as_str()).collect();
        match found[..] {
            [from, to] if !from.is_empty() && from != to => Some((from.into(), to.into())),
            _ => None,
        }
    };
    for re in [&*NAMED, &*SED, &*ARROW] {
        out.extend(re.captures_iter(messages).filter_map(pair));
    }
    out.dedup();
    out
}

/// Whether the person's messages already said exactly what to write: the
/// work changed little beyond what they spelled out. A line changed is
/// spelled out when it's in their messages whole, spacing aside, or when
/// an edit they named ("change `30` to `60`") turns a line taken out into
/// a line put in. At least 80% of the lines that say something must be,
/// and no more than ten may not. Lines kelve didn't keep count against it.
pub fn said_exactly(written: &[&File], messages: &[String]) -> Option<Exactly> {
    let text = squeezed(&messages.join("\n"));
    if text.is_empty() || written.is_empty() {
        return None;
    }
    let edits = named_edits(&messages.join("\n"));
    let mut spelled = 0;
    let mut of = 0;
    let mut named = None;
    for f in written {
        let kept = |lines: &[String]| -> Vec<String> {
            lines
                .iter()
                .map(|l| squeezed(l))
                .filter(|l| says_something(l))
                .collect()
        };
        let added = kept(&f.added_lines);
        let removed = kept(&f.removed_lines);
        // Lines kelve didn't keep are the agent's, as far as it knows.
        let unkept = (f.added as usize).saturating_sub(f.added_lines.len())
            + (f.removed as usize).saturating_sub(f.removed_lines.len());
        of += added.len() + removed.len() + unkept;
        let mut used = vec![false; added.len()];
        for r in &removed {
            let made = edits.iter().find_map(|(from, to)| {
                if !r.contains(from.as_str()) {
                    return None;
                }
                let now = r.replace(from.as_str(), to);
                let i = (0..added.len()).find(|&i| !used[i] && added[i] == now)?;
                Some((i, (from.clone(), to.clone())))
            });
            if let Some((i, edit)) = made {
                used[i] = true;
                spelled += 2;
                named.get_or_insert(edit);
            } else if holds(&text, r) {
                spelled += 1;
            }
        }
        spelled += added
            .iter()
            .zip(&used)
            .filter(|(a, used)| !**used && holds(&text, a))
            .count();
    }
    let beyond = of.saturating_sub(spelled);
    (spelled > 0 && beyond * 100 <= of * (100 - SPELLED) && beyond <= BEYOND).then_some(Exactly {
        spelled,
        of,
        named,
    })
}

/// "1 file", "3 files".
fn files_count(n: usize) -> String {
    if n == 1 {
        "1 file".to_string()
    } else {
        format!("{n} files")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worth(files: &[File]) -> Due {
        due(files, &Handover::default(), DebriefsWhen::WorthOne)
    }

    fn with_lines(mut f: File, added: &[&str], removed: &[&str]) -> File {
        f.added_lines = added.iter().map(|s| s.to_string()).collect();
        f.removed_lines = removed.iter().map(|s| s.to_string()).collect();
        f
    }

    #[test]
    fn more_than_twenty_lines_is_worth_a_debrief() {
        let files = [
            File::new("src/retry.rs", Status::Modified, 18, 2),
            File::new("src/fetch.rs", Status::Modified, 5, 9),
        ];
        assert_eq!(worth(&files), Due::yes("It changes 27 lines in 2 files."));
        // A line rewritten counts once.
        let files = [File::new("src/retry.rs", Status::Modified, 15, 15)];
        assert_eq!(
            worth(&files),
            Due::no("It's a small change: 15 lines in 1 file.")
        );
        let files = [File::new("src/retry.rs", Status::Modified, 21, 0)];
        assert!(worth(&files).due);
        let files = [File::new("src/retry.rs", Status::Modified, 20, 0)];
        assert!(!worth(&files).due);
    }

    #[test]
    fn a_new_file_is_worth_a_debrief() {
        let files = [File::new("src/retry.rs", Status::Added, 4, 0)];
        assert_eq!(worth(&files), Due::yes("It adds a file, src/retry.rs."));
        let files = [
            File::new("src/retry.rs", Status::Added, 4, 0),
            File::new("tests/retry.rs", Status::Added, 4, 0),
        ];
        assert_eq!(
            worth(&files),
            Due::yes("It adds 2 files, src/retry.rs among them.")
        );
        // A lockfile a tool wrote isn't.
        let files = [File::new("web/bun.lock", Status::Added, 400, 0)];
        assert_eq!(worth(&files), Due::no("Only files a tool wrote changed."));
    }

    #[test]
    fn a_new_dependency_is_worth_a_debrief() {
        let cargo = with_lines(
            File::new("Cargo.toml", Status::Modified, 1, 0),
            &["rand = \"0.9\""],
            &[],
        );
        assert_eq!(worth(&[cargo]), Due::yes("It adds a dependency, rand."));
        let table = with_lines(
            File::new("crates/cli/Cargo.toml", Status::Modified, 2, 0),
            &[
                "tokio = { workspace = true }",
                "serde = { version = \"1\", features = [\"derive\"] }",
            ],
            &[],
        );
        assert_eq!(
            worth(&[table]),
            Due::yes("It adds dependencies, serde and tokio.")
        );
        let npm = with_lines(
            File::new("web/package.json", Status::Modified, 1, 0),
            &["    \"zod\": \"^3.23.8\","],
            &[],
        );
        assert_eq!(worth(&[npm]), Due::yes("It adds a dependency, zod."));
        let py = with_lines(
            File::new("requirements.txt", Status::Modified, 2, 0),
            &["httpx>=0.27", "# a comment"],
            &[],
        );
        assert_eq!(worth(&[py]), Due::yes("It adds a dependency, httpx."));
        let go = with_lines(
            File::new("go.mod", Status::Modified, 1, 0),
            &["\tgithub.com/google/uuid v1.6.0"],
            &[],
        );
        assert_eq!(
            worth(&[go]),
            Due::yes("It adds a dependency, github.com/google/uuid.")
        );
    }

    #[test]
    fn a_version_bump_is_not() {
        let bump = with_lines(
            File::new("Cargo.toml", Status::Modified, 1, 1),
            &["version = \"0.1.13\""],
            &["version = \"0.1.12\""],
        );
        let log = File::new("CHANGELOG.md", Status::Modified, 6, 0);
        assert_eq!(worth(&[bump, log]), Due::no("It only changes a version."));
        // A dependency's version moving isn't a new dependency.
        let moved = with_lines(
            File::new("Cargo.toml", Status::Modified, 1, 1),
            &["serde = \"1.0.200\""],
            &["serde = \"1.0.199\""],
        );
        assert_eq!(worth(&[moved]), Due::no("It only changes a version."));
        let npm = with_lines(
            File::new("package.json", Status::Modified, 1, 1),
            &["  \"version\": \"0.2.0\","],
            &["  \"version\": \"0.1.0\","],
        );
        assert_eq!(worth(&[npm]), Due::no("It only changes a version."));
    }

    #[test]
    fn a_new_command_is_worth_a_debrief() {
        let make = with_lines(
            File::new("Makefile", Status::Modified, 2, 0),
            &["fixtures:", "\t./scripts/fixtures.sh"],
            &[],
        );
        assert_eq!(worth(&[make]), Due::yes("It adds a command, fixtures."));
        let phony = with_lines(
            File::new("Makefile", Status::Modified, 1, 1),
            &[".PHONY: test fixtures", "X := 1"],
            &[".PHONY: test"],
        );
        assert!(!worth(&[phony]).due);
        let scripts = with_lines(
            File::new("web/package.json", Status::Modified, 1, 0),
            &["    \"looks\": \"playwright test --config looks.config.ts\","],
            &[],
        );
        assert_eq!(worth(&[scripts]), Due::yes("It adds a command, looks."));
        let bin = with_lines(
            File::new("Cargo.toml", Status::Modified, 3, 0),
            &["[[bin]]", "name = \"kelve-graph-stand-in\""],
            &[],
        );
        assert_eq!(worth(&[bin]), Due::yes("It adds a command."));
        let py = with_lines(
            File::new("pyproject.toml", Status::Modified, 1, 0),
            &["fetch = \"fetch.cli:main\""],
            &[],
        );
        assert_eq!(worth(&[py]), Due::yes("It adds a command, fetch."));
    }

    #[test]
    fn a_handover_that_names_a_decision_is_worth_a_debrief() {
        let files = [File::new("src/retry.rs", Status::Modified, 3, 1)];
        let said = Handover {
            said: "Capped the wait at 30 seconds. I chose a cap over a growing limit.".into(),
            messages: Vec::new(),
        };
        let got = due(&files, &said, DebriefsWhen::WorthOne);
        assert!(got.due);
        assert_eq!(
            got.reason,
            "Your agent said it decided something, so it made a call you may want a say in."
        );
        let weighed = Handover {
            said: "Used a loop rather than recursion.".into(),
            messages: Vec::new(),
        };
        assert!(
            due(&files, &weighed, DebriefsWhen::WorthOne)
                .reason
                .contains("weighed one way against another")
        );
        // "Decision" alone, or "decides", is not a call it made.
        let plain = Handover {
            said: "Fixed the decision table's typo.".into(),
            messages: Vec::new(),
        };
        assert!(!due(&files, &plain, DebriefsWhen::WorthOne).due);
    }

    #[test]
    fn small_and_mechanical_work_is_not() {
        assert_eq!(worth(&[]), Due::no("Nothing changed."));
        assert_eq!(
            worth(&[File::new("src/a.rs", Status::Modified, 0, 0)]),
            Due::no("Nothing changed.")
        );
        assert_eq!(
            worth(&[File::new("README.md", Status::Modified, 1, 1)]),
            Due::no("It's a small change: 1 line in 1 file.")
        );
        let mut spaces = File::new("src/a.rs", Status::Modified, 300, 280);
        spaces.whitespace_only = true;
        assert_eq!(worth(&[spaces]), Due::no("Only spacing changed."));
        let moved = File::new("src/old.rs", Status::Renamed, 0, 0);
        assert_eq!(worth(&[moved]), Due::no("It only moves files."));
        let built = [
            File::new("Cargo.lock", Status::Modified, 120, 40),
            File::new("web/dist/app.js", Status::Modified, 900, 880),
        ];
        assert_eq!(worth(&built), Due::no("Only files a tool wrote changed."));
    }

    fn said(messages: &[&str]) -> Handover {
        Handover {
            said: "I chose this.".into(),
            messages: messages.iter().map(|m| m.to_string()).collect(),
        }
    }

    const RETRY: &[&str] = &[
        "pub fn backoff(attempt: u32) -> Duration {",
        "    let wait = Duration::from_millis(100 * 2u64.pow(attempt));",
        "    wait.min(Duration::from_secs(30))",
        "}",
    ];

    #[test]
    fn what_the_person_said_and_chose_comes_first() {
        let big = [with_lines(
            File::new("src/retry.rs", Status::Added, 4, 0),
            RETRY,
            &[],
        )];
        let pasted = format!(
            "Add this to src/retry.rs:\n\n```rust\n{}\n```",
            RETRY.join("\n")
        );
        assert_eq!(
            due(&big, &said(&[&pasted]), DebriefsWhen::WorthOne),
            Due::no(
                "Your messages already said exactly what to write: all 3 lines it changed are in them."
            )
        );
        let small = [File::new("README.md", Status::Modified, 1, 1)];
        assert!(due(&small, &Handover::default(), DebriefsWhen::Every).due);
        assert!(!due(&big, &Handover::default(), DebriefsWhen::OnlyAsked).due);
        // Every handover still needs something to have changed.
        assert!(!due(&[], &Handover::default(), DebriefsWhen::Every).due);
    }

    #[test]
    fn lines_the_messages_spelled_out_are_no_call_of_the_agents() {
        let file = |added: &[&str], removed: &[&str]| {
            with_lines(
                File::new(
                    "src/retry.rs",
                    Status::Modified,
                    added.len() as u32,
                    removed.len() as u32,
                ),
                added,
                removed,
            )
        };
        let check = |files: &[File], messages: &[&str]| {
            let written: Vec<&File> = files.iter().collect();
            let messages: Vec<String> = messages.iter().map(|m| m.to_string()).collect();
            said_exactly(&written, &messages)
        };
        // Pasted with other spacing, the lines are still the same.
        let pasted = "please add\n  pub fn backoff(attempt: u32)   -> Duration {\n let wait = Duration::from_millis(100 * 2u64.pow(attempt));\nwait.min(Duration::from_secs(30))\n}";
        let got = check(&[file(RETRY, &[])], &[pasted]).expect("said exactly");
        assert_eq!((got.spelled, got.of, got.named), (3, 3, None));
        // Without the messages, or with ones about something else, it's the
        // agent's work.
        assert_eq!(check(&[file(RETRY, &[])], &[]), None);
        assert_eq!(
            check(&[file(RETRY, &[])], &["Make the retries back off."]),
            None
        );
        // Most, but not enough: the agent wrote the rest.
        let more: Vec<&str> = RETRY
            .iter()
            .copied()
            .chain([
                "    log::warn!(\"retrying\");",
                "    metrics::count(\"retry\");",
            ])
            .collect();
        assert_eq!(check(&[file(&more, &[])], &[pasted]), None);
        // A little beyond is still what they said: 4 of 5, over two
        // messages; 3 of 4 isn't.
        let one: Vec<&str> = RETRY.iter().copied().chain(["    // capped"]).collect();
        assert_eq!(check(&[file(&one, &[])], &[pasted]), None);
        let traced = "and `tracing::debug!(attempt, \"backing off\");` first";
        let two: Vec<&str> = one
            .iter()
            .copied()
            .chain(["    tracing::debug!(attempt, \"backing off\");"])
            .collect();
        let got = check(&[file(&two, &[])], &[pasted, traced]).expect("a little beyond");
        assert_eq!((got.spelled, got.of), (4, 5));
        assert_eq!(
            got.reason(),
            "Your messages already said exactly what to write: 4 of the 5 lines it changed are in them."
        );
        // A line inside a longer word isn't the line.
        assert_eq!(check(&[file(&["x = 1"], &[])], &["max = 10"]), None);
        // Lines kelve didn't keep count against it.
        let mut cut = file(RETRY, &[]);
        cut.added = 40;
        assert_eq!(check(&[cut], &[pasted]), None);
    }

    #[test]
    fn an_edit_the_messages_named_is_no_call_of_the_agents() {
        let check = |files: &[File], messages: &[&str]| {
            let written: Vec<&File> = files.iter().collect();
            let messages: Vec<String> = messages.iter().map(|m| m.to_string()).collect();
            said_exactly(&written, &messages)
        };
        let timeout = |n: u32| {
            with_lines(
                File::new("src/client.rs", Status::Modified, n, n),
                &vec!["    .timeout(Duration::from_secs(60))"; n as usize],
                &vec!["    .timeout(Duration::from_secs(30))"; n as usize],
            )
        };
        for asked in [
            "Change `30` to `60` in the client's timeout.",
            "replace \"from_secs(30)\" with \"from_secs(60)\"",
            "s/from_secs(30)/from_secs(60)/g",
            "`30` -> `60`",
        ] {
            let got = check(&[timeout(3)], &[asked]).unwrap_or_else(|| panic!("{asked}"));
            assert_eq!((got.spelled, got.of), (6, 6), "{asked}");
            assert!(got.named.is_some(), "{asked}");
        }
        let got = check(&[timeout(1)], &["rename 30 to 60"]).expect("named bare");
        assert_eq!(
            got.reason(),
            "Your messages already said exactly what to write: change “30” to “60”, which is all 2 lines it changed."
        );
        // The edit named, and more besides.
        let mut more = timeout(1);
        more.added_lines
            .extend(["    .retries(5)".into(), "    .backoff(true)".into()]);
        more.added += 2;
        assert_eq!(check(&[more], &["Change `30` to `60`."]), None);
        // An edit that doesn't make the line that's there isn't it.
        assert_eq!(check(&[timeout(2)], &["Change `30` to `90`."]), None);
        // Through due, it's said in a sentence, before the size.
        let files: Vec<File> = (0..3)
            .map(|i| {
                let mut f = timeout(10);
                f.path = format!("src/client{i}.rs");
                f
            })
            .collect();
        let got = due(
            &files,
            &said(&["Change `30` to `60` everywhere."]),
            DebriefsWhen::WorthOne,
        );
        assert!(!got.due);
        assert!(got.reason.contains("change “30” to “60”"), "{}", got.reason);
    }
}
