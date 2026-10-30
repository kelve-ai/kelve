//! The hook that tells the person's coding agent what kelve has for it, at the
//! moment it helps: the notes they left it, as they send a message; lessons
//! from the shared graph, as it reads or edits a file and as something fails;
//! and, when it hands work over that it hasn't shown the person (a commit, a
//! push, a pull request), to show it in kelve. One command, `kelve moment`, naming the agent and the event, on each
//! agent's own events: installed and taken out together, only with the person's
//! yes. kelve adds only its own entries, and takes out only those, leaving
//! everything else in the file as it was.
//!
//! Each agent writes its hooks, and its events, its own way. Here, for each:
//! where its hooks go, kelve's entries, how its event reads as one of kelve's
//! moments, and how kelve answers it. No IO: the caller reads and writes the
//! files, and runs the command.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// The command every entry runs, before its agent and event.
pub const COMMAND: &str = "kelve moment";

/// What the hook ran before lessons, taken out as kelve's own.
const BEFORE: &str = "kelve notes --waiting";

/// An agent with hooks kelve can add to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Agent {
    Claude,
    Codex,
    Cursor,
    Copilot,
    Gemini,
    Antigravity,
    Kiro,
    Droid,
    Qwen,
    Qoder,
    Augment,
}

/// How an agent keeps its hooks.
enum Layout {
    /// Under `hooks` in a shared file, or at its top, by event: entries of
    /// a matcher and a list of commands.
    Nested { under: Option<&'static str> },
    /// Cursor's: under `hooks`, by event, each entry a command itself.
    Flat,
    /// A file of kelve's own.
    Owned,
    /// Antigravity's: hook sets by name, kelve's under `kelve`.
    Named,
}

impl Agent {
    pub const ALL: [Agent; 11] = [
        Agent::Claude,
        Agent::Codex,
        Agent::Cursor,
        Agent::Copilot,
        Agent::Gemini,
        Agent::Antigravity,
        Agent::Kiro,
        Agent::Droid,
        Agent::Qwen,
        Agent::Qoder,
        Agent::Augment,
    ];

    /// How the command line names it: `kelve hook install codex`.
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Cursor => "cursor",
            Agent::Copilot => "copilot",
            Agent::Gemini => "gemini",
            Agent::Antigravity => "antigravity",
            Agent::Kiro => "kiro",
            Agent::Droid => "droid",
            Agent::Qwen => "qwen",
            Agent::Qoder => "qoder",
            Agent::Augment => "augment",
        }
    }

    /// As people call it.
    pub fn shown(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
            Agent::Cursor => "Cursor",
            Agent::Copilot => "GitHub Copilot",
            Agent::Gemini => "Gemini CLI",
            Agent::Antigravity => "Antigravity",
            Agent::Kiro => "Kiro",
            Agent::Droid => "Factory Droid",
            Agent::Qwen => "Qwen Code",
            Agent::Qoder => "Qoder",
            Agent::Augment => "Augment",
        }
    }

    pub fn parse(name: &str) -> Option<Agent> {
        let name = name.trim().to_lowercase();
        let name = match name.as_str() {
            "claude-code" | "claude code" | "claudecode" => "claude",
            "factory" => "droid",
            "gemini-cli" => "gemini",
            "agy" => "antigravity",
            "auggie" => "augment",
            other => other,
        };
        Agent::ALL.into_iter().find(|a| a.name() == name)
    }

    /// Where its hooks go, from the home folder. Claude Code's folder may be
    /// elsewhere: the caller knows it.
    pub fn file(self) -> &'static str {
        match self {
            Agent::Claude => ".claude/settings.json",
            Agent::Codex => ".codex/hooks.json",
            Agent::Cursor => ".cursor/hooks.json",
            Agent::Copilot => ".copilot/hooks/kelve.json",
            Agent::Gemini => ".gemini/settings.json",
            Agent::Antigravity => ".gemini/config/hooks.json",
            Agent::Kiro => ".kiro/hooks/kelve.json",
            Agent::Droid => ".factory/hooks.json",
            Agent::Qwen => ".qwen/settings.json",
            Agent::Qoder => ".qoder/settings.json",
            Agent::Augment => ".augment/settings.json",
        }
    }

    /// A script the hook runs, for an agent that runs only scripts: its path
    /// from the home folder, and what it says.
    pub fn script(self) -> Option<(&'static str, String)> {
        match self {
            Agent::Augment => Some((
                ".augment/hooks/kelve-moment.sh",
                format!("#!/bin/sh\nexec {COMMAND} --agent augment --on \"$1\"\n"),
            )),
            _ => None,
        }
    }

    /// What the person does once it's in, if anything: a sentence.
    pub fn then(self) -> Option<&'static str> {
        match self {
            Agent::Codex => Some(
                "In Codex, trust kelve's hooks in /hooks: Codex runs a hook only once you have.",
            ),
            Agent::Droid => {
                Some("In Factory Droid, look over kelve's hooks in /hooks, or restart it.")
            }
            Agent::Qwen => Some("Restart Qwen Code, or open /hooks, so it reads kelve's hooks."),
            Agent::Gemini | Agent::Antigravity => Some("Restart it, so it reads kelve's hooks."),
            _ => None,
        }
    }

    fn layout(self) -> Layout {
        match self {
            Agent::Claude
            | Agent::Codex
            | Agent::Gemini
            | Agent::Qwen
            | Agent::Qoder
            | Agent::Augment => Layout::Nested {
                under: Some("hooks"),
            },
            Agent::Droid => Layout::Nested { under: None },
            Agent::Cursor => Layout::Flat,
            Agent::Copilot | Agent::Kiro => Layout::Owned,
            Agent::Antigravity => Layout::Named,
        }
    }

    /// The events kelve's hook runs on, each with the tools it's for, if
    /// only some.
    fn events(self) -> Vec<(&'static str, Option<&'static str>)> {
        match self {
            Agent::Claude | Agent::Qoder => vec![
                ("UserPromptSubmit", None),
                (
                    "PostToolUse",
                    Some("Read|Edit|Write|MultiEdit|NotebookEdit|Bash"),
                ),
                ("PostToolUseFailure", None),
                // A turn ends: what a hard stretch taught, asked once.
                ("Stop", None),
            ],
            Agent::Qwen => vec![
                ("UserPromptSubmit", None),
                (
                    "PostToolUse",
                    Some("read_file|write_file|edit|run_shell_command"),
                ),
                ("PostToolUseFailure", None),
            ],
            // Codex reads with shell commands, and edits with apply_patch,
            // which matches Edit and Write.
            Agent::Codex => vec![
                ("UserPromptSubmit", None),
                ("PostToolUse", Some("Bash|Edit|Write")),
            ],
            Agent::Droid => vec![
                ("UserPromptSubmit", None),
                ("PostToolUse", Some("Read|Edit|Create|ApplyPatch|Execute")),
            ],
            Agent::Gemini => vec![
                ("BeforeAgent", None),
                (
                    "AfterTool",
                    Some("read_file|write_file|replace|run_shell_command"),
                ),
            ],
            Agent::Augment => vec![(
                "PostToolUse",
                Some("view|str-replace-editor|save-file|launch-process"),
            )],
            Agent::Cursor => vec![
                ("postToolUse", Some("Read|Write|Shell")),
                ("postToolUseFailure", None),
            ],
            Agent::Copilot => vec![("postToolUse", None), ("postToolUseFailure", None)],
            Agent::Kiro => vec![
                ("UserPromptSubmit", None),
                ("PostToolUse", Some("read|write|shell")),
            ],
            Agent::Antigravity => vec![
                (
                    "PostToolUse",
                    Some(
                        "view_file|replace_file_content|multi_replace_file_content|write_to_file|run_command",
                    ),
                ),
                ("PreInvocation", None),
            ],
        }
    }

    /// The command for one event: the agent and the event named, so neither
    /// has to be guessed from what the agent sends.
    pub fn command(self, event: &str) -> String {
        format!("{COMMAND} --agent {} --on {event}", self.name())
    }

    /// The command an entry runs: Augment's script, or the command.
    fn runs(self, event: &str, home: &str) -> String {
        match self.script() {
            Some((path, _)) => format!("{}/{path} {event}", home.trim_end_matches('/')),
            None => self.command(event),
        }
    }

    /// kelve's entry for one event, as it goes in the agent's file.
    fn entry(self, event: &str, matcher: Option<&str>, home: &str) -> Value {
        let command = self.runs(event, home);
        let mut entry = match self.layout() {
            Layout::Flat => json!({ "command": command }),
            Layout::Nested { .. } | Layout::Named => {
                let mut hook = json!({ "type": "command", "command": command });
                if self == Agent::Gemini {
                    hook["name"] = json!("kelve");
                }
                if self == Agent::Antigravity && event == "PreInvocation" {
                    return hook;
                }
                json!({ "hooks": [hook] })
            }
            Layout::Owned => json!({}),
        };
        if let Some(m) = matcher {
            entry["matcher"] = json!(m);
        }
        entry
    }

    /// kelve's own file, for an agent that reads a folder of hook files.
    fn owned(self, home: &str) -> Value {
        match self {
            Agent::Copilot => {
                let mut hooks = Map::new();
                for (event, _) in self.events() {
                    let command = self.runs(event, home);
                    hooks.insert(
                        event.into(),
                        json!([{ "type": "command", "bash": command, "powershell": command, "timeoutSec": 10 }]),
                    );
                }
                json!({ "version": 1, "hooks": hooks })
            }
            _ => {
                let hooks: Vec<Value> = self
                    .events()
                    .into_iter()
                    .map(|(event, matcher)| {
                        let mut h = json!({
                            "name": format!("kelve {event}"),
                            "trigger": event,
                            "action": { "type": "command", "command": self.runs(event, home) },
                            "timeout": 10,
                            "enabled": true,
                        });
                        if let Some(m) = matcher {
                            h["matcher"] = json!(m);
                        }
                        h
                    })
                    .collect();
                json!({ "version": "v1", "hooks": hooks })
            }
        }
    }
}

/// Whether a piece of an agent's file is kelve's: it runs kelve's command,
/// or the one before it, or Augment's script.
fn is_kelves(v: &Value) -> bool {
    match v {
        Value::String(s) => s.starts_with(COMMAND) || s == BEFORE || s.contains("kelve-moment.sh"),
        Value::Array(a) => a.iter().any(is_kelves),
        Value::Object(o) => o.values().any(is_kelves),
        _ => false,
    }
}

/// A file, parsed: none for an empty one, an error for one that isn't a
/// JSON object, which kelve then leaves alone.
fn parse(agent: Agent, text: &str) -> Result<Map<String, Value>, String> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        _ => Err(format!(
            "{}'s hooks file isn't one kelve can read, so it's left as it is. Add the hook by hand, or fix the file and try again.",
            agent.shown()
        )),
    }
}

fn write(settings: &Map<String, Value>) -> String {
    let mut text = serde_json::to_string_pretty(settings).unwrap_or_else(|_| "{}".into());
    text.push('\n');
    text
}

fn unreadable(agent: Agent) -> String {
    format!(
        "{}'s hooks are in a shape kelve can't read, so they're left as they are.",
        agent.shown()
    )
}

/// Where an agent's events are, in its file, if anywhere.
fn events_of(agent: Agent, settings: &Map<String, Value>) -> Option<&Map<String, Value>> {
    match agent.layout() {
        Layout::Nested { under: None } => Some(settings),
        Layout::Nested { under: Some(key) } => settings.get(key).and_then(Value::as_object),
        Layout::Flat => settings.get("hooks").and_then(Value::as_object),
        Layout::Named => settings.get("kelve").and_then(Value::as_object),
        Layout::Owned => None,
    }
}

/// Whether an agent's file holds any of kelve's hook: then the person said
/// yes to it, and it's kept up to date with the events kelve runs on.
pub fn partly(agent: Agent, text: &str) -> bool {
    parse(agent, text).is_ok_and(|s| is_kelves(&Value::Object(s)))
}

/// Whether an agent's file holds kelve's hook, on every event it runs on.
pub fn installed(agent: Agent, text: &str) -> bool {
    let Ok(settings) = parse(agent, text) else {
        return false;
    };
    if let Layout::Owned = agent.layout() {
        return is_kelves(&Value::Object(settings));
    }
    let Some(events) = events_of(agent, &settings) else {
        return false;
    };
    agent.events().iter().all(|(event, _)| {
        events
            .get(*event)
            .and_then(Value::as_array)
            .is_some_and(|e| {
                e.iter().any(|x| {
                    let x = x.to_string();
                    x.contains(&format!("--on {event}"))
                        || x.contains(&format!("kelve-moment.sh {event}"))
                })
            })
    })
}

/// The file without kelve's entries, and nothing else changed.
fn without(agent: Agent, mut settings: Map<String, Value>) -> Result<Map<String, Value>, String> {
    if let Layout::Named = agent.layout() {
        settings.remove("kelve");
        return Ok(settings);
    }
    let key = match agent.layout() {
        Layout::Nested { under } => under,
        _ => Some("hooks"),
    };
    let container = match key {
        Some(k) => match settings.get_mut(k) {
            None => return Ok(settings),
            Some(Value::Object(o)) => o,
            Some(_) => return Err(unreadable(agent)),
        },
        None => &mut settings,
    };
    let events: Vec<String> = container.keys().cloned().collect();
    for event in events {
        if let Some(on) = container.get_mut(&event).and_then(Value::as_array_mut) {
            let had = on.len();
            on.retain(|e| !is_kelves(e));
            if on.is_empty() && had > 0 {
                container.remove(&event);
            }
        }
    }
    if let Some(k) = key
        && settings
            .get(k)
            .and_then(Value::as_object)
            .is_some_and(Map::is_empty)
    {
        settings.remove(k);
        if agent == Agent::Cursor && settings.len() == 1 && settings.contains_key("version") {
            settings.remove("version");
        }
    }
    Ok(settings)
}

/// The agent's file with kelve's hook in, once, on each of its events.
/// `home` is the home folder, for an entry that runs a script there.
pub fn install(agent: Agent, text: &str, home: &str) -> Result<String, String> {
    if let Layout::Owned = agent.layout() {
        return Ok(format!(
            "{}\n",
            serde_json::to_string_pretty(&agent.owned(home)).unwrap_or_default()
        ));
    }
    if installed(agent, text) {
        return Ok(text.to_string());
    }
    let mut settings = without(agent, parse(agent, text)?)?;
    if let Layout::Named = agent.layout() {
        let mut set = Map::new();
        for (event, matcher) in agent.events() {
            set.insert(event.into(), json!([agent.entry(event, matcher, home)]));
        }
        set.insert("enabled".into(), json!(true));
        settings.insert("kelve".into(), Value::Object(set));
        return Ok(write(&settings));
    }
    if agent == Agent::Cursor {
        settings.entry("version").or_insert(json!(1));
    }
    let container = match agent.layout() {
        Layout::Nested { under: None } => &mut settings,
        _ => {
            let hooks = settings.entry("hooks").or_insert_with(|| json!({}));
            hooks.as_object_mut().ok_or_else(|| unreadable(agent))?
        }
    };
    for (event, matcher) in agent.events() {
        let on = container.entry(event).or_insert_with(|| json!([]));
        on.as_array_mut()
            .ok_or_else(|| unreadable(agent))?
            .push(agent.entry(event, matcher, home));
    }
    Ok(write(&settings))
}

/// The agent's file with kelve's hook taken out, and nothing else; None
/// when the file is kelve's own, to be deleted.
pub fn remove(agent: Agent, text: &str) -> Result<Option<String>, String> {
    if let Layout::Owned = agent.layout() {
        return Ok(None);
    }
    let settings = parse(agent, text)?;
    if !is_kelves(&Value::Object(settings.clone())) {
        return Ok(Some(text.to_string()));
    }
    Ok(Some(write(&without(agent, settings)?)))
}

/// A moment in the agent's work, as kelve's server takes it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moment {
    /// `prompt`, `read`, `edit`, `ran`, `failed`, `next`: the agent is
    /// about to call its model, and can be given what was kept for it;
    /// or `end`: its turn ends.
    pub what: String,
    pub agent: String,
    pub session: String,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// For an edit: what was written, for matching what it's about. Kept on
    /// the person's computer, like everything here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written: Option<String>,
    /// For an edit: the text it replaced, when the agent's tool says, so
    /// kelve can tell the agent replaced its own earlier work.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced: Option<String>,
    /// The agent can't be given text now: keep it for its next `next`.
    pub hold: bool,
    /// Work the command handed over, read from the whole command, not its
    /// key: the folder, absolute but for a `~` kelve knows, and the branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handover: Option<Handover>,
}

const READS: [&str; 8] = [
    "read",
    "read_file",
    "view",
    "view_file",
    "fs_read",
    "readfile",
    "open",
    "cat",
];
const EDITS: [&str; 17] = [
    "edit",
    "write",
    "multiedit",
    "notebookedit",
    "apply_patch",
    "applypatch",
    "write_file",
    "replace",
    "create",
    "str-replace-editor",
    "save-file",
    "replace_file_content",
    "multi_replace_file_content",
    "write_to_file",
    "fs_write",
    "replace_in_file",
    "edit_file",
];
const SHELLS: [&str; 10] = [
    "bash",
    "shell",
    "run_shell_command",
    "powershell",
    "launch-process",
    "run_command",
    "execute_bash",
    "execute",
    "execute_command",
    "exec_command",
];

/// What a pipe's last stage can be that only shows some of what came
/// before: the command exits as it does, whatever came before did.
const FILTERS: [&str; 10] = [
    "tail", "head", "grep", "rg", "less", "more", "cat", "tee", "sed", "awk",
];

/// What a command can start with that only reads or searches: its output
/// may say "error" without anything having failed.
const LOOKS: [&str; 16] = [
    "cat", "grep", "rg", "find", "ls", "git", "echo", "printf", "sed", "awk", "head", "tail", "jq",
    "wc", "diff", "less",
];

/// A command's stages, split at each pipe outside quotes: `a | b` is two,
/// `a || b` one.
fn stages(command: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut quote) = (0, None);
    let bytes = command.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match (quote, b) {
            (None, b'\'' | b'"') => quote = Some(b),
            (Some(q), _) if b == q => quote = None,
            (None, b'|') if bytes.get(i + 1) != Some(&b'|') && (i == 0 || bytes[i - 1] != b'|') => {
                out.push(command[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(command[start..].trim());
    out
}

/// The commands run one after another (`a && b`, `a; b`), split outside
/// quotes, each trimmed.
fn commands(command: &str) -> Vec<&str> {
    let bytes = command.as_bytes();
    let (mut start, mut quote) = (0, None);
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match (quote, b) {
            (None, b'\'' | b'"') => quote = Some(b),
            (Some(q), _) if b == q => quote = None,
            (None, b';') => {
                out.push(command[start..i].trim());
                start = i + 1;
            }
            (None, b'&') if bytes.get(i + 1) == Some(&b'&') => {
                out.push(command[start..i].trim());
                start = i + 2;
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(command[start.min(command.len())..].trim());
    out.retain(|c| !c.is_empty());
    out
}

/// The command that matters of several run one after another: the last
/// that does something besides moving, reading or searching (`ls; cargo
/// test | tail; ls` is `cargo test | tail`), else the last. And whether
/// others follow it, so the whole's exit isn't its own.
fn main_command(command: &str) -> (&str, bool) {
    let all = commands(command);
    let looks = |c: &str| {
        let first = first_word(stages(c).first().copied().unwrap_or_default());
        first == "cd" || LOOKS.contains(&first)
    };
    match all.iter().rposition(|c| !looks(c)) {
        Some(at) => (all[at], at + 1 < all.len()),
        None => (all.last().copied().unwrap_or(command.trim()), false),
    }
}

fn first_word(stage: &str) -> &str {
    stage.split_whitespace().next().unwrap_or_default()
}

/// Whether a command's last stage is a filter: then its exit code is the
/// filter's, and says nothing of the rest.
fn filtered(command: &str) -> bool {
    let s = stages(command);
    s.len() > 1 && s.last().is_some_and(|l| FILTERS.contains(&first_word(l)))
}

static CD: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| crate::text::re(r"^cd\s+\S+\s*&&\s*"));
static REDIRECT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| crate::text::re(r"\s+\d?>&\d|\s+\d?>\s*/dev/null"));

/// A command as kelve knows it again, when the same thing is run once more:
/// the last of several run one after another, without moving into a folder
/// first, what only shows part of its output, or where its errors go.
/// `cd app && cargo test 2>&1 | tail -40` is `cargo test`, as `cargo test |
/// head` and `ls && cargo test` are.
pub fn command_key(command: &str) -> String {
    let mut c = main_command(command).0;
    while let Some(m) = CD.find(c) {
        c = &c[m.end()..];
    }
    let mut s = stages(c);
    while s.len() > 1 && s.last().is_some_and(|l| FILTERS.contains(&first_word(l))) {
        s.pop();
    }
    let c = REDIRECT.replace_all(&s.join(" | "), "").to_string();
    c.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A line that says something failed, as build and test tools print it.
static FAILURE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    crate::text::re(
        r"(?m)^\s*(?:error(?:\[[A-Z]*\d+\])?:|test result: FAILED|FAILED\b|FAIL\b|--- FAIL|Traceback \(most recent call last\)|npm ERR!|fatal:|[A-Z]\w*(?:Error|Exception):)|\berror TS\d+:",
    )
});

/// Whether what a command printed shows it failed, when its exit code
/// can't: it was piped into a filter, or other commands followed it, and it
/// didn't only read or search.
fn failed_behind_filter(command: &str, response: &Value) -> bool {
    let (main, followed) = main_command(command);
    let first = stages(main)
        .first()
        .map(|s| first_word(CD.replace(s, "").as_ref()).to_string())
        .unwrap_or_default();
    if !(filtered(main) || followed) || LOOKS.contains(&first.as_str()) {
        return false;
    }
    let text = ["stdout", "stderr", "output"]
        .iter()
        .filter_map(|k| response.get(*k).and_then(Value::as_str))
        .chain(response.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    FAILURE.is_match(&text)
}

/// The first of these fields that holds text, at the top of a value.
fn first_str(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().filter_map(|k| v.get(*k)).find_map(|x| {
        x.as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
    })
}

/// A value, or the JSON a string of it holds.
fn unstring(v: &Value) -> Value {
    match v {
        Value::String(s) => serde_json::from_str(s).unwrap_or_else(|_| v.clone()),
        other => other.clone(),
    }
}

/// The files a patch touches: Codex's `*** Update File: <path>`.
fn patched(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            ["*** Update File:", "*** Add File:"]
                .iter()
                .find_map(|p| l.strip_prefix(p))
                .map(|p| p.trim().to_string())
        })
        .collect()
}

/// The file a shell command only reads, when that's all it does:
/// `cat src/x.rs`, `sed -n '1,40p' src/x.rs`, `head src/x.rs`.
fn read_by(command: &str) -> Option<String> {
    let words: Vec<&str> = command.split_whitespace().collect();
    let (&first, rest) = words.split_first()?;
    if command.contains(['|', '>', ';', '&'])
        || !["cat", "head", "tail", "sed", "nl", "less", "bat"].contains(&first)
    {
        return None;
    }
    let files: Vec<&str> = rest
        .iter()
        .copied()
        .filter(|w| {
            !w.starts_with('-') && !w.starts_with('\'') && !w.starts_with('"') && w.contains('.')
        })
        .collect();
    match files.as_slice() {
        [one] => Some(one.trim_matches(['\'', '"']).to_string()),
        _ => None,
    }
}

/// The exit code a tool's response says, when it says one.
fn exit_code(response: &Value) -> Option<i64> {
    for k in ["exit_code", "exitCode", "exit", "returnCode", "code"] {
        if let Some(c) = response.get(k).and_then(Value::as_i64) {
            return Some(c);
        }
    }
    if let Some(c) = response
        .get("metadata")
        .and_then(|m| m.get("exit"))
        .and_then(Value::as_i64)
    {
        return Some(c);
    }
    let text = response.as_str().map(str::to_string).or_else(|| {
        first_str(
            response,
            &["output", "stdout", "textResultForLlm", "llmContent"],
        )
    })?;
    let lower = text.to_lowercase();
    for p in ["exit code:", "exit code", "exit status", "exited with code"] {
        if let Some(at) = lower.find(p) {
            let n: String = lower[at + p.len()..]
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = n.parse() {
                return Some(n);
            }
        }
    }
    None
}

/// What failed, from a tool's response.
fn failed_with(response: &Value) -> Option<String> {
    if let Some(t) = response.as_str() {
        return Some(t.to_string());
    }
    // Every stream it said anything on, the error first: a command whose
    // errors went to its output (`2>&1`) leaves only what the agent's own
    // tool adds on stderr, and the problem is in the output.
    let said: Vec<String> = [
        "error",
        "stderr",
        "textResultForLlm",
        "llmContent",
        "output",
        "stdout",
    ]
    .iter()
    .filter_map(|k| response.get(*k).and_then(Value::as_str))
    .map(str::trim)
    .filter(|t| !t.is_empty())
    .map(str::to_string)
    .collect();
    (!said.is_empty()).then(|| said.join("\n"))
}

/// A command's words, as the shell splits them: at spaces outside quotes,
/// the quotes taken off. `git -c user.name='Ada Lovelace' commit` is four.
fn words(command: &str) -> Vec<String> {
    let (mut out, mut word, mut quote, mut any) = (Vec::new(), String::new(), None, false);
    for c in command.chars() {
        match (quote, c) {
            (None, '\'' | '"') => (quote, any) = (Some(c), true),
            (Some(q), _) if c == q => quote = None,
            (None, c) if c.is_whitespace() => {
                if any {
                    out.push(std::mem::take(&mut word));
                }
                any = false;
            }
            _ => {
                word.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(word);
    }
    out
}

/// The folder a command moves to before it does anything else: `cd x &&
/// cargo test` works in `x`.
pub fn moved_to(command: &str) -> Option<String> {
    let first = commands(command).into_iter().next()?;
    match words(first).as_slice() {
        [cd, dir] if cd == "cd" => Some(dir.clone()),
        _ => None,
    }
}

/// Work a command hands over for review: the folder it runs in, when it
/// moves there first (`cd x && …`, `git -C x …`), and the branch, when it
/// names one that isn't the one checked out (`gh pr create --head x`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handover {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// Whether a command hands work over: `git commit`, `git push` or `gh pr
/// create`. Not a commit or push only tried out (`--dry-run`), and not what
/// only looks at work, like `git status` or `gh pr view`.
pub fn handing_over(command: &str) -> Option<Handover> {
    let mut dir: Option<String> = None;
    for c in commands(command) {
        let words = words(c);
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        match words.as_slice() {
            ["cd", to, ..] => {
                dir = Some(match &dir {
                    Some(d) => absolute(d, to),
                    None => (*to).to_string(),
                });
            }
            ["git", rest @ ..] => {
                let mut i = 0;
                let mut at = dir.clone();
                // Git's own options, before what it's asked to do.
                while let Some(w) = rest.get(i) {
                    match *w {
                        "-C" => {
                            at = rest.get(i + 1).map(|d| match &at {
                                Some(from) => absolute(from, d),
                                None => (*d).to_string(),
                            });
                            i += 2;
                        }
                        "-c" => i += 2,
                        w if w.starts_with('-') => i += 1,
                        _ => break,
                    }
                }
                let what = rest.get(i).copied();
                let args = rest.get(i + 1..).unwrap_or_default();
                // Only tried out: `--dry-run`, and for a push, `-n` too.
                let tried =
                    args.contains(&"--dry-run") || (what == Some("push") && args.contains(&"-n"));
                if matches!(what, Some("commit" | "push")) && !tried {
                    return Some(Handover {
                        dir: at,
                        branch: None,
                    });
                }
            }
            ["gh", "pr", "create", args @ ..] => {
                let branch = args.iter().enumerate().find_map(|(i, a)| match *a {
                    "--head" | "-H" => args.get(i + 1).map(|b| (*b).to_string()),
                    a => a.strip_prefix("--head=").map(str::to_string),
                });
                return Some(Handover {
                    dir: dir.clone(),
                    branch,
                });
            }
            _ => {}
        }
    }
    None
}

/// An agent's event, as one of kelve's moments: None for one kelve has
/// nothing for. `on` is the event, as the installed command names it.
pub fn moment(agent: Agent, on: &str, event: &Value) -> Option<Moment> {
    let session = first_str(
        event,
        &[
            "session_id",
            "sessionId",
            "conversation_id",
            "conversationId",
            "taskId",
        ],
    )
    .unwrap_or_default();
    let root = ["workspace_roots", "workspacePaths", "workspaceRoots"]
        .iter()
        .find_map(|k| {
            event
                .get(*k)
                .and_then(|v| v.get(0))
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let call = event.get("toolCall");
    let input = unstring(
        ["tool_input", "toolArgs"]
            .iter()
            .find_map(|k| event.get(*k))
            .or_else(|| call.and_then(|c| c.get("args")))
            .unwrap_or(&Value::Null),
    );
    let cwd = first_str(event, &["cwd"])
        .or_else(|| first_str(&input, &["Cwd", "cwd"]))
        .or(root)
        .unwrap_or_default();
    let mut m = Moment {
        agent: agent.name().into(),
        session,
        cwd,
        ..Moment::default()
    };
    let lower = on.to_lowercase();
    if ["userpromptsubmit", "beforeagent", "userpromptsubmitted"].contains(&lower.as_str()) {
        m.what = "prompt".into();
        return Some(m);
    }
    if lower == "preinvocation" {
        m.what = "next".into();
        return Some(m);
    }
    // A turn ends; but one kelve already held up goes on, never asked twice.
    if lower == "stop" {
        if event.get("stop_hook_active").and_then(Value::as_bool) == Some(true) {
            return None;
        }
        m.what = "end".into();
        return Some(m);
    }
    // Before a command: nothing, now kelve asks after work is handed over.
    // A hook from an older kelve still sends it.
    if lower == "pretooluse" {
        return None;
    }
    let tool = first_str(event, &["tool_name", "toolName"])
        .or_else(|| call.and_then(|c| first_str(c, &["name"])))
        .unwrap_or_default();
    let t = tool.to_lowercase();
    let t = t.rsplit("__").next().unwrap_or(&t).to_string();
    let response = unstring(
        ["tool_response", "tool_output", "toolResult", "tool_result"]
            .iter()
            .find_map(|k| event.get(*k))
            .unwrap_or(&Value::Null),
    );
    let file = first_str(
        &input,
        &[
            "file_path",
            "filePath",
            "path",
            "AbsolutePath",
            "TargetFile",
            "notebook_path",
            "target_file",
        ],
    )
    .or_else(|| first_str(event, &["file_path"]))
    .or_else(|| {
        ["command", "patch", "input"]
            .iter()
            .filter_map(|k| input.get(*k).and_then(Value::as_str))
            .chain(input.as_str())
            .find_map(|t| patched(t).into_iter().next())
    });
    // kelve's own tools: what goes wrong there is kelve's to say, not a
    // problem in the work.
    if tool.to_lowercase().starts_with("mcp__kelve") {
        return None;
    }
    let ran = first_str(&input, &["command", "CommandLine", "command_line", "cmd"])
        .filter(|c| !c.contains("*** Begin Patch"))
        .map(|c| c.trim().to_string());
    // Work handed over, from the whole command: its key may be another of
    // the commands run with it.
    let handover = ran.as_deref().and_then(handing_over).map(|h| Handover {
        dir: Some(
            h.dir
                .map_or_else(|| m.cwd.clone(), |d| absolute(&m.cwd, &d)),
        ),
        ..h
    });
    // A command that moves first (`cd x && …`) works there, as in another
    // worktree: that's the folder its moment is about.
    if let Some(dir) = ran.as_deref().and_then(moved_to) {
        m.cwd = absolute(&m.cwd, &dir);
    }
    let command = ran.as_deref().map(command_key);
    // Failed: the event says so, or an error came with it, or the command
    // exited badly.
    let error = first_str(event, &["error", "error_message", "tool_error"]).or_else(|| {
        response
            .get("error")
            .and_then(Value::as_str)
            .filter(|e| !e.is_empty())
            .map(str::to_string)
    });
    let bad_result = response
        .get("resultType")
        .or_else(|| response.get("result_type"))
        .and_then(Value::as_str)
        .is_some_and(|r| r != "success");
    let failed = lower.contains("failure")
        || error.is_some()
        || exit_code(&response).is_some_and(|c| c != 0)
        || bad_result
        || response.get("interrupted").and_then(Value::as_bool) == Some(true)
        || ran
            .as_deref()
            .is_some_and(|c| failed_behind_filter(c, &response));
    m.hold = agent == Agent::Antigravity;
    if failed {
        m.what = "failed".into();
        m.command = command.or_else(|| file.as_ref().map(|f| format!("{tool} {f}")));
        m.problem = error.or_else(|| failed_with(&response));
        return Some(m);
    }
    if SHELLS.contains(&t.as_str()) {
        let command = command?;
        match read_by(&command) {
            Some(f) => {
                m.what = "read".into();
                m.file = Some(absolute(&m.cwd, &f));
            }
            None => m.what = "ran".into(),
        }
        m.command = Some(command);
        m.handover = handover;
        return Some(m);
    }
    let what = if READS.contains(&t.as_str()) {
        "read"
    } else if EDITS.contains(&t.as_str()) {
        "edit"
    } else {
        return None;
    };
    m.what = what.into();
    if what == "edit" {
        m.written = written(&input);
        m.replaced = replaced(&input);
    }
    m.file = Some(absolute(&m.cwd, &file?));
    m.command = Some(format!("{tool} {}", m.file.as_deref().unwrap_or_default()));
    Some(m)
}

/// What an edit replaced, as each agent's tool gives it: the old text, each
/// of several edits' old text, or a patch's taken-out lines.
fn replaced(input: &Value) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = first_str(
        input,
        &["old_string", "old_str", "oldText", "TargetContent"],
    ) {
        parts.push(t);
    }
    for key in ["edits", "ReplacementChunks"] {
        if let Some(edits) = input.get(key).and_then(Value::as_array) {
            parts.extend(
                edits
                    .iter()
                    .filter_map(|e| first_str(e, &["old_string", "oldText", "TargetContent"])),
            );
        }
    }
    if parts.is_empty()
        && let Some(patch) = ["command", "patch", "input"]
            .iter()
            .filter_map(|k| input.get(*k).and_then(Value::as_str))
            .find(|t| t.contains("*** Begin Patch"))
    {
        parts.push(
            patch
                .lines()
                .filter(|l| l.starts_with('-') && !l.starts_with("---"))
                .map(|l| &l[1..])
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    let all = parts.join("\n");
    (!all.trim().is_empty()).then(|| crate::text::clip(&all, 20_000))
}

/// What an edit wrote, as each agent's tool gives it: the new text, a write's
/// content, each of several edits' new text, or a patch's added lines.
fn written(input: &Value) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(t) = first_str(
        input,
        &[
            "new_string",
            "content",
            "new_str",
            "file_text",
            "newText",
            "ReplacementContent",
            "CodeContent",
            "file_content",
            "text",
        ],
    ) {
        parts.push(t);
    }
    for key in ["edits", "ReplacementChunks"] {
        if let Some(edits) = input.get(key).and_then(Value::as_array) {
            parts.extend(
                edits
                    .iter()
                    .filter_map(|e| first_str(e, &["new_string", "newText", "ReplacementContent"])),
            );
        }
    }
    if parts.is_empty()
        && let Some(patch) = ["command", "patch", "input"]
            .iter()
            .filter_map(|k| input.get(*k).and_then(Value::as_str))
            .find(|t| t.contains("*** Begin Patch"))
    {
        parts.push(
            patch
                .lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .map(|l| &l[1..])
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    let all = parts.join("\n");
    (!all.trim().is_empty()).then(|| crate::text::clip(&all, 20_000))
}

/// A path, made absolute against the folder the agent works in. One from
/// the home folder (`~/x`) stays as it is: kelve knows where home is.
fn absolute(cwd: &str, path: &str) -> String {
    if path.starts_with('/')
        || path == "~"
        || path.starts_with("~/")
        || path.chars().nth(1) == Some(':')
        || cwd.is_empty()
    {
        path.to_string()
    } else {
        format!(
            "{}/{}",
            cwd.trim_end_matches('/'),
            path.trim_start_matches("./")
        )
    }
}

/// The most kelve says at one moment, under every agent's cap.
const MOST: usize = 6000;

/// What the hook prints for kelve's text, as the agent takes it; None for
/// nothing.
pub fn reply(agent: Agent, on: &str, text: &str) -> Option<String> {
    let text = crate::text::clip(text.trim(), MOST);
    if text.is_empty() {
        return None;
    }
    let lower = on.to_lowercase();
    let prompt = lower == "userpromptsubmit";
    // As a turn ends, they take kelve's word only as why to go on: the
    // stop is held up once, and the agent answers before it hands back.
    if lower == "stop" && matches!(agent, Agent::Claude | Agent::Qoder) {
        return Some(json!({ "decision": "block", "reason": text }).to_string());
    }
    Some(match agent {
        Agent::Kiro => text,
        Agent::Cursor => json!({ "additional_context": text }).to_string(),
        Agent::Copilot => json!({ "additionalContext": text }).to_string(),
        Agent::Antigravity if lower == "preinvocation" => {
            json!({ "injectSteps": [{ "ephemeralMessage": text }] }).to_string()
        }
        Agent::Antigravity => return None,
        Agent::Gemini => {
            json!({ "hookSpecificOutput": { "hookEventName": on, "additionalContext": text } })
                .to_string()
        }
        _ if prompt => text,
        _ => json!({ "hookSpecificOutput": { "hookEventName": on, "additionalContext": text } })
            .to_string(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const THEIRS: &str = r#"{
  "model": "opus",
  "hooks": {
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "echo hi" }] }],
    "Stop": [{ "hooks": [{ "type": "command", "command": "say done" }] }]
  }
}"#;

    const HOME: &str = "/Users/sam";

    #[test]
    fn work_handed_over_is_seen_and_nothing_else_is() {
        let at = |dir: Option<&str>| {
            Some(Handover {
                dir: dir.map(str::to_string),
                branch: None,
            })
        };
        assert_eq!(
            handing_over("git commit -qam 'Wait between tries'"),
            at(None)
        );
        assert_eq!(handing_over("git push -u origin fix"), at(None));
        assert_eq!(
            handing_over("gh pr create --title Retries --body x"),
            at(None)
        );
        assert_eq!(
            handing_over("cd ~/projects/ledger && git add -A && git commit -m Retries"),
            at(Some("~/projects/ledger"))
        );
        assert_eq!(handing_over("git -C ../ledger push"), at(Some("../ledger")));
        // Words in quotes are one word, as the shell has them.
        assert_eq!(
            handing_over(
                "git -c user.name='Ada Lovelace' -c user.email=ada@example.com commit -m \"Retries; and backoff\""
            ),
            at(None)
        );
        assert_eq!(
            handing_over("cd \"/w/my ledger\" && git commit -m x"),
            at(Some("/w/my ledger"))
        );
        assert_eq!(
            handing_over("cd ~/w && git -C ledger push"),
            at(Some("~/w/ledger"))
        );
        // A pull request for a branch named: that branch's work.
        for named in [
            "gh pr create --head fix --fill",
            "gh pr create -R ada/ledger -H fix --fill",
            "gh pr create --head=fix",
        ] {
            assert_eq!(
                handing_over(named),
                Some(Handover {
                    dir: None,
                    branch: Some("fix".into())
                }),
                "{named}"
            );
        }
        // Tried out, only looked at, or not git at all.
        for none in [
            "git commit --dry-run",
            "git push --dry-run",
            "git push -n",
            "git status",
            "git log --oneline",
            "gh pr view 3",
            "gh pr merge --squash",
            "cargo test",
            "echo git commit",
            "echo \"git commit\"",
        ] {
            assert_eq!(handing_over(none), None, "{none}");
        }
        // The hook reads it from the whole command, whatever its key: a
        // commit with others run after it, a push whose output is filtered.
        let handed = |cwd: &str, command: &str| {
            let event = json!({ "session_id": "s", "cwd": cwd, "tool_name": "Bash", "tool_input": { "command": command }, "tool_response": { "stdout": "", "stderr": "" } });
            moment(Agent::Claude, "PostToolUse", &event).unwrap()
        };
        let m = handed(
            "/w",
            "cd ~/ledger && cargo fmt --check && git add -A && git -c user.name='Ada Lovelace' commit -m 'Retries' && git log -1 --format='%h %s'",
        );
        assert_eq!(m.cwd, "~/ledger", "kelve puts home right");
        assert_eq!(m.handover, at(Some("~/ledger")));
        let m = handed(
            "/w",
            "git -C ledger push -u origin fix 2>&1 | grep -v '^remote:' ; echo pushed",
        );
        assert_eq!(m.command.as_deref(), Some("echo pushed"));
        assert_eq!(m.handover, at(Some("/w/ledger")));
        assert_eq!(
            handed("/w/ledger", "git push").handover,
            at(Some("/w/ledger"))
        );
        assert_eq!(handed("/w/ledger", "cargo test").handover, None);
        // Before a command is nothing to kelve now, even from an older hook.
        let event = json!({ "session_id": "s", "cwd": "/w/ledger", "tool_name": "Bash", "tool_input": { "command": "git push" } });
        assert_eq!(moment(Agent::Claude, "PreToolUse", &event), None);
        // And it's no longer installed.
        for agent in [Agent::Claude, Agent::Qoder, Agent::Codex] {
            assert!(
                !agent.events().iter().any(|(e, _)| *e == "PreToolUse"),
                "{}",
                agent.name()
            );
        }
    }

    #[test]
    fn a_hook_from_an_older_kelve_is_kelves_but_not_up_to_date() {
        let older = r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"kelve moment --agent claude --on UserPromptSubmit"}]}],"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#;
        assert!(partly(Agent::Claude, older));
        assert!(!installed(Agent::Claude, older));
        let now = install(Agent::Claude, older, HOME).unwrap();
        assert!(installed(Agent::Claude, &now));
        assert!(now.contains("say done"), "theirs kept");
        assert!(!partly(
            Agent::Claude,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#
        ));
    }

    #[test]
    fn it_adds_its_own_entries_once_and_leaves_the_rest() {
        let with = install(Agent::Claude, THEIRS, HOME).unwrap();
        assert!(installed(Agent::Claude, &with));
        let v: Value = serde_json::from_str(&with).unwrap();
        assert_eq!(v["model"], "opus");
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
        assert_eq!(v["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 2);
        assert_eq!(
            v["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            "kelve moment --agent claude --on PostToolUse"
        );
        assert!(
            v["hooks"]["PostToolUse"][0]["matcher"]
                .as_str()
                .unwrap()
                .contains("Read")
        );
        assert_eq!(install(Agent::Claude, &with, HOME).unwrap(), with, "once");
        let without = remove(Agent::Claude, &with).unwrap().unwrap();
        assert!(!installed(Agent::Claude, &without));
        let v: Value = serde_json::from_str(&without).unwrap();
        assert_eq!(
            v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
            "echo hi"
        );
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
    }

    #[test]
    fn the_hook_from_before_gives_way_to_this_one() {
        let before = r#"{ "hooks": { "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "kelve notes --waiting" }] }], "PostToolUse": [{ "hooks": [{ "type": "command", "command": "kelve moment" }] }] } }"#;
        assert!(!installed(Agent::Claude, before));
        let with = install(Agent::Claude, before, HOME).unwrap();
        let v: Value = serde_json::from_str(&with).unwrap();
        assert_eq!(v["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
        assert_eq!(v["hooks"]["PostToolUse"].as_array().unwrap().len(), 1);
        let gone = remove(Agent::Claude, before).unwrap().unwrap();
        assert_eq!(serde_json::from_str::<Value>(&gone).unwrap(), json!({}));
    }

    #[test]
    fn every_agent_gets_its_own_shape_and_loses_it_again() {
        for agent in Agent::ALL {
            let with = install(agent, "", HOME).unwrap();
            assert!(installed(agent, &with), "{}", agent.name());
            match remove(agent, &with).unwrap() {
                None => assert!(
                    matches!(agent, Agent::Copilot | Agent::Kiro),
                    "{}",
                    agent.name()
                ),
                Some(gone) => assert_eq!(
                    serde_json::from_str::<Value>(&gone).unwrap(),
                    json!({}),
                    "{}",
                    agent.name()
                ),
            }
        }
        let cursor: Value =
            serde_json::from_str(&install(Agent::Cursor, "", HOME).unwrap()).unwrap();
        assert_eq!(cursor["version"], 1);
        assert_eq!(
            cursor["hooks"]["postToolUseFailure"][0]["command"],
            "kelve moment --agent cursor --on postToolUseFailure"
        );
        let droid: Value = serde_json::from_str(&install(Agent::Droid, "", HOME).unwrap()).unwrap();
        assert!(
            droid["PostToolUse"][0]["matcher"]
                .as_str()
                .unwrap()
                .contains("Execute")
        );
        let gravity: Value = serde_json::from_str(
            &install(Agent::Antigravity, r#"{"theirs":{"Stop":[]}}"#, HOME).unwrap(),
        )
        .unwrap();
        assert_eq!(
            gravity["kelve"]["PreInvocation"][0]["command"],
            "kelve moment --agent antigravity --on PreInvocation"
        );
        assert!(gravity["theirs"].is_object(), "theirs stays");
        let augment: Value =
            serde_json::from_str(&install(Agent::Augment, "", HOME).unwrap()).unwrap();
        assert_eq!(
            augment["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
            "/Users/sam/.augment/hooks/kelve-moment.sh PostToolUse"
        );
        assert!(install(Agent::Codex, "{ not json", HOME).is_err());
    }

    #[test]
    fn a_turn_ending_is_a_moment_once_and_its_question_holds_up_the_stop() {
        let stop = json!({ "session_id": "s", "cwd": "/w", "stop_hook_active": false });
        let m = moment(Agent::Claude, "Stop", &stop).unwrap();
        assert_eq!(m.what, "end");
        // Held up once already: it goes on, never asked twice.
        let again = json!({ "session_id": "s", "cwd": "/w", "stop_hook_active": true });
        assert!(moment(Agent::Claude, "Stop", &again).is_none());
        let said = reply(Agent::Claude, "Stop", "kelve: write it as a lesson.").unwrap();
        let said: Value = serde_json::from_str(&said).unwrap();
        assert_eq!(said["decision"], "block");
        assert_eq!(said["reason"], "kelve: write it as a lesson.");
        assert!(
            reply(Agent::Claude, "Stop", "").is_none(),
            "nothing to ask: it stops"
        );
        assert!(
            install(Agent::Claude, "{}", HOME)
                .unwrap()
                .contains("--on Stop")
        );
    }

    #[test]
    fn each_agents_events_read_as_kelves_moments() {
        let claude = json!({ "session_id": "s", "cwd": "/w", "tool_name": "Read", "tool_input": { "file_path": "/w/src/a.rs" } });
        let m = moment(Agent::Claude, "PostToolUse", &claude).unwrap();
        assert_eq!(
            (m.what.as_str(), m.file.as_deref()),
            ("read", Some("/w/src/a.rs"))
        );
        let failed = json!({ "session_id": "s", "cwd": "/w", "tool_name": "Bash", "tool_input": { "command": "cargo test" }, "error": "Exit code 101\nerror[E0425]: cannot find" });
        let m = moment(Agent::Claude, "PostToolUseFailure", &failed).unwrap();
        assert_eq!(
            (m.what.as_str(), m.command.as_deref()),
            ("failed", Some("cargo test"))
        );
        // Piped into a filter, a command exits as the filter does: what it
        // printed says it failed, and run again with another filter it's
        // the same command.
        let piped = json!({ "session_id": "s", "cwd": "/w", "tool_name": "Bash", "tool_input": { "command": "cargo test 2>&1 | tail -40" }, "tool_response": { "stdout": "   Compiling dice v0.1.0\nerror: expected identifier, found reserved keyword `gen`\nerror: could not compile `dice`", "stderr": "" } });
        let m = moment(Agent::Claude, "PostToolUse", &piped).unwrap();
        assert_eq!(
            (m.what.as_str(), m.command.as_deref()),
            ("failed", Some("cargo test"))
        );
        assert!(m.problem.unwrap().contains("reserved keyword"));
        let passed = json!({ "session_id": "s", "cwd": "/w", "tool_name": "Bash", "tool_input": { "command": "cd /w && cargo test 2>&1 | tail -15" }, "tool_response": { "stdout": "test result: ok. 2 passed; 0 failed", "stderr": "" } });
        let m = moment(Agent::Claude, "PostToolUse", &passed).unwrap();
        assert_eq!(
            (m.what.as_str(), m.command.as_deref()),
            ("ran", Some("cargo test"))
        );
        // Run after something else, it's known by what it ran last.
        let after = json!({ "session_id": "s", "cwd": "/w", "tool_name": "Bash", "tool_input": { "command": "ls -R | head -50 && python3 -m unittest 2>&1 | tail -30" }, "tool_response": { "stdout": "a.py\nFAIL: test_x (tests.T.test_x)\nAssertionError: Lists differ: ['a'] != []\nFAILED (failures=1)", "stderr": "" } });
        let m = moment(Agent::Claude, "PostToolUse", &after).unwrap();
        assert_eq!(
            (m.what.as_str(), m.command.as_deref()),
            ("failed", Some("python3 -m unittest"))
        );
        assert_eq!(command_key("echo 'a && b'; cargo test"), "cargo test");
        // Followed by something else, its exit is that one's: what it printed says.
        let then = json!({ "session_id": "s", "cwd": "/w", "tool_name": "Bash", "tool_input": { "command": "python3 -m unittest 2>&1 | tail -40; ls" }, "tool_response": { "stdout": "AssertionError: Lists differ: ['a'] != []\nFAILED (failures=1)\nblog\nREADME.md", "stderr": "" } });
        let m = moment(Agent::Claude, "PostToolUse", &then).unwrap();
        assert_eq!(
            (m.what.as_str(), m.command.as_deref()),
            ("failed", Some("python3 -m unittest"))
        );
        // Searching for errors isn't failing.
        let search = json!({ "session_id": "s", "cwd": "/w", "tool_name": "Bash", "tool_input": { "command": "grep -rn 'error:' src | head" }, "tool_response": { "stdout": "src/a.rs:3: error: no", "stderr": "" } });
        assert_eq!(
            moment(Agent::Claude, "PostToolUse", &search).unwrap().what,
            "ran"
        );
        // An edit says what it replaced.
        let edit = json!({ "session_id": "s", "cwd": "/w", "tool_name": "Edit", "tool_input": { "file_path": "/w/a.rs", "old_string": "sleep(30)", "new_string": "advance(30)" } });
        let m = moment(Agent::Claude, "PostToolUse", &edit).unwrap();
        assert_eq!(m.replaced.as_deref(), Some("sleep(30)"));
        // kelve's own tools say for themselves what went wrong.
        let own = json!({ "session_id": "s", "cwd": "/w", "tool_name": "mcp__kelve__hand_in", "tool_input": { "job": 3 }, "error": "Not taken: say it in fewer words." });
        assert!(moment(Agent::Claude, "PostToolUseFailure", &own).is_none());
        // Codex: an edit is a patch; a read is a shell command; a failure an exit code.
        let patch = json!({ "session_id": "c", "cwd": "/w", "tool_name": "apply_patch", "tool_input": { "command": "*** Begin Patch\n*** Update File: src/b.rs\n@@\n-a\n+b\n*** End Patch" } });
        let m = moment(Agent::Codex, "PostToolUse", &patch).unwrap();
        assert_eq!(
            (m.what.as_str(), m.file.as_deref()),
            ("edit", Some("/w/src/b.rs"))
        );
        let cat = json!({ "session_id": "c", "cwd": "/w", "tool_name": "Bash", "tool_input": { "command": "sed -n '1,80p' src/b.rs" }, "tool_response": { "exit_code": 0, "output": "fn b() {}" } });
        assert_eq!(
            moment(Agent::Codex, "PostToolUse", &cat)
                .unwrap()
                .file
                .as_deref(),
            Some("/w/src/b.rs")
        );
        let bad = json!({ "session_id": "c", "cwd": "/w", "tool_name": "Bash", "tool_input": { "command": "pytest" }, "tool_response": "Exit code: 1\nValueError: bad" });
        assert_eq!(
            moment(Agent::Codex, "PostToolUse", &bad).unwrap().what,
            "failed"
        );
        // Cursor's output is a string of JSON; its failure has error_message.
        let cursor = json!({ "conversation_id": "k", "cwd": "/w", "tool_name": "Write", "tool_input": { "file_path": "/w/x.md" }, "tool_output": "{\"ok\":true}" });
        assert_eq!(
            moment(Agent::Cursor, "postToolUse", &cursor).unwrap().what,
            "edit"
        );
        let cf = json!({ "conversation_id": "k", "cwd": "/w", "tool_name": "Shell", "tool_input": { "command": "npm test" }, "error_message": "Tests failed" });
        assert_eq!(
            moment(Agent::Cursor, "postToolUseFailure", &cf)
                .unwrap()
                .problem
                .as_deref(),
            Some("Tests failed")
        );
        // Copilot: camelCase, its args a string of JSON.
        let copilot = json!({ "sessionId": "p", "cwd": "/w", "toolName": "view", "toolArgs": "{\"path\":\"src/c.ts\"}" });
        assert_eq!(
            moment(Agent::Copilot, "postToolUse", &copilot)
                .unwrap()
                .file
                .as_deref(),
            Some("/w/src/c.ts")
        );
        // Antigravity: no cwd, its call nested, and what it's given held for later.
        let gravity = json!({ "conversationId": "g", "workspacePaths": ["/w"], "toolCall": { "name": "run_command", "args": { "CommandLine": "go test", "Cwd": "/w" } }, "error": "exit status 1" });
        let m = moment(Agent::Antigravity, "PostToolUse", &gravity).unwrap();
        assert!(m.hold && m.what == "failed" && m.session == "g");
        assert_eq!(
            moment(
                Agent::Antigravity,
                "PreInvocation",
                &json!({ "conversationId": "g" })
            )
            .unwrap()
            .what,
            "next"
        );
        assert_eq!(
            moment(Agent::Gemini, "BeforeAgent", &json!({ "session_id": "s" }))
                .unwrap()
                .what,
            "prompt"
        );
        assert!(
            moment(
                Agent::Claude,
                "PostToolUse",
                &json!({ "tool_name": "WebSearch" })
            )
            .is_none()
        );
    }

    #[test]
    fn each_agent_is_answered_its_own_way() {
        assert_eq!(
            reply(Agent::Claude, "UserPromptSubmit", "a note").unwrap(),
            "a note"
        );
        let v: Value =
            serde_json::from_str(&reply(Agent::Claude, "PostToolUse", "a lesson").unwrap())
                .unwrap();
        assert_eq!(v["hookSpecificOutput"]["additionalContext"], "a lesson");
        let v: Value =
            serde_json::from_str(&reply(Agent::Cursor, "postToolUse", "a lesson").unwrap())
                .unwrap();
        assert_eq!(v["additional_context"], "a lesson");
        let v: Value =
            serde_json::from_str(&reply(Agent::Copilot, "postToolUse", "a lesson").unwrap())
                .unwrap();
        assert_eq!(v["additionalContext"], "a lesson");
        assert_eq!(
            reply(Agent::Antigravity, "PostToolUse", "a lesson"),
            None,
            "held for later"
        );
        let v: Value =
            serde_json::from_str(&reply(Agent::Antigravity, "PreInvocation", "a lesson").unwrap())
                .unwrap();
        assert_eq!(v["injectSteps"][0]["ephemeralMessage"], "a lesson");
        assert_eq!(
            reply(Agent::Kiro, "PostToolUse", "a lesson").unwrap(),
            "a lesson"
        );
        assert_eq!(reply(Agent::Claude, "PostToolUse", "  "), None);
    }

    #[test]
    fn a_failure_is_read_from_every_stream_and_where_its_command_moved() {
        // Claude Code's own note on stderr, the error in the output: the
        // error is what's kept.
        let response = json!({
            "stdout": "test retry ... FAILED\nthread 'retry' panicked at src/retry.rs:9:5:\nleft: 3, right: 5",
            "stderr": "Shell cwd was reset to /Users/n/kelve",
        });
        let said = failed_with(&response).unwrap();
        assert!(said.contains("FAILED") && said.contains("Shell cwd"));
        assert!(!crate::lesson::seen_as(&said).contains("Shell cwd"));
        assert_eq!(
            moved_to("cd /w/kelve-way2 && cargo test"),
            Some("/w/kelve-way2".into())
        );
        assert_eq!(moved_to("cargo test; cd /w"), None);
        assert_eq!(
            moved_to("cd '/w/my ledger' && cargo test"),
            Some("/w/my ledger".into())
        );
    }
}
