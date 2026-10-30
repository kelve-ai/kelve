//! Setting kelve up, as the installer does once kelve is in place: connecting
//! the person's agent, adding kelve's hook so it picks up their notes and
//! lessons, and how much kelve marks for them. Run by their agent, which they
//! asked to install kelve, it does all three, and leaves the agent the one word
//! to choose: how well the person knows their work. Run by the person in a
//! terminal, it asks.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, bail};
use kelve_core::hook::Agent;

use crate::client::Kelve;
use crate::style;

/// The levels, as the person and their agent name them, and what each marks.
const LEVELS: [(&str, &str); 4] = [
    ("new", "every idea, the basics too"),
    ("learning", "every idea but the basics"),
    ("working", "advanced and specialist ideas"),
    ("deep", "specialist ideas only"),
];

/// What a level marks, in words, or why the word isn't one.
fn level(word: &str) -> anyhow::Result<(&'static str, &'static str)> {
    let word = word.trim().to_lowercase();
    LEVELS
        .into_iter()
        .find(|(w, _)| *w == word)
        .with_context(|| format!("“{word}” isn't a level. Say new, learning, working or deep."))
}

/// `kelve marks set <level>`: how much kelve marks for the person, keeping
/// whatever else they said (areas, always and never, their own words).
pub async fn marks_set(word: &str) -> anyhow::Result<()> {
    let (word, marks) = level(word)?;
    crate::open::started().await?;
    let kelve = Kelve::new(Duration::from_secs(10))?;
    let now = kelve.send(kelve.get("/api/agent/marks")).await?;
    let mut spec = now["spec"].clone();
    if !spec.is_object() {
        spec = serde_json::json!({});
    }
    spec["mark"] = serde_json::json!(word);
    kelve
        .send(
            kelve
                .post("/api/agent/marks")
                .json(&serde_json::json!({ "spec": spec })),
        )
        .await?;
    style::head("marks");
    style::line(&format!(
        "kelve marks {marks}. Change it in kelve’s settings, under What kelve marks."
    ));
    Ok(())
}

/// An agent kelve can connect itself to, and how it's named to people.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Connectable {
    agent: Agent,
    shown: &'static str,
    /// Its own command, on the PATH when it's installed.
    command: &'static str,
}

const CONNECTABLE: [Connectable; 4] = [
    Connectable {
        agent: Agent::Claude,
        shown: "Claude Code",
        command: "claude",
    },
    Connectable {
        agent: Agent::Codex,
        shown: "Codex",
        command: "codex",
    },
    Connectable {
        agent: Agent::Cursor,
        shown: "Cursor",
        command: "cursor-agent",
    },
    Connectable {
        agent: Agent::Gemini,
        shown: "Gemini CLI",
        command: "gemini",
    },
];

/// The agent running this, from what each sets in the commands it runs.
fn running_in(var: impl Fn(&str) -> Option<String>) -> Option<Connectable> {
    let set = |name: &str| var(name).is_some_and(|v| !v.is_empty() && v != "0");
    let agent = if set("CLAUDECODE") {
        Agent::Claude
    } else if set("GEMINI_CLI") {
        Agent::Gemini
    } else if set("CODEX_SANDBOX") || set("CODEX_HOME") || set("CODEX_MANAGED_BY_NPM") {
        Agent::Codex
    } else if set("CURSOR_AGENT") {
        Agent::Cursor
    } else {
        return None;
    };
    CONNECTABLE.into_iter().find(|c| c.agent == agent)
}

fn on_path(command: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| {
        std::env::split_paths(&p)
            .any(|d| d.join(command).is_file() || d.join(format!("{command}.exe")).is_file())
    })
}

/// Asks the person, in the terminal: the answer, or the default when they
/// just press return.
fn ask(question: &str, choices: &[&str], default: usize) -> anyhow::Result<usize> {
    let shown: Vec<String> = choices
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i == default {
                format!("[{c}]")
            } else {
                (*c).to_string()
            }
        })
        .collect();
    loop {
        anstream::print!("  {question} {} ", shown.join(" / "));
        std::io::stdout().flush().ok();
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        let said = line.trim().to_lowercase();
        if said.is_empty() {
            return Ok(default);
        }
        if let Some(i) = choices
            .iter()
            .position(|c| c.to_lowercase().starts_with(&said))
        {
            return Ok(i);
        }
    }
}

/// Connects the agent to kelve with its own command, as `kelve mcp` from
/// wherever this kelve is: what it said, or the line to run instead.
fn connect(c: Connectable, me: &Path, home: &Path) -> Result<String, String> {
    let me = me.display().to_string();
    let quietly = |cmd: &mut Command| cmd.output().ok().filter(|o| o.status.success()).is_some();
    match c.agent {
        Agent::Claude => {
            if quietly(Command::new("claude").args(["mcp", "get", "kelve"])) {
                return Ok("Claude Code was connected already".into());
            }
            quietly(
                Command::new("claude")
                    .args(["mcp", "add", "--scope", "user", "kelve", "--", &me, "mcp"]),
            )
            .then(|| "Connected to Claude Code".to_string())
            .ok_or_else(|| "claude mcp add --scope user kelve -- kelve mcp".to_string())
        }
        Agent::Codex => {
            if quietly(Command::new("codex").args(["mcp", "get", "kelve"])) {
                return Ok("Codex was connected already".into());
            }
            quietly(Command::new("codex").args(["mcp", "add", "kelve", "--", &me, "mcp"]))
                .then(|| "Connected to Codex".to_string())
                .ok_or_else(|| "codex mcp add kelve -- kelve mcp".to_string())
        }
        Agent::Gemini => quietly(
            Command::new("gemini").args(["mcp", "add", "--scope", "user", "kelve", &me, "mcp"]),
        )
        .then(|| "Connected to Gemini CLI".to_string())
        .ok_or_else(|| "gemini mcp add --scope user kelve kelve mcp".to_string()),
        Agent::Cursor => {
            let path = home.join(".cursor/mcp.json");
            let line = r#"{ "mcpServers": { "kelve": { "command": "kelve", "args": ["mcp"] } } } in ~/.cursor/mcp.json"#;
            let mut json: serde_json::Value = match std::fs::read_to_string(&path) {
                Ok(text) => serde_json::from_str(&text).map_err(|_| line.to_string())?,
                Err(_) => serde_json::json!({}),
            };
            if json["mcpServers"]["kelve"].is_object() {
                return Ok("Cursor was connected already".into());
            }
            if !json.is_object() {
                return Err(line.into());
            }
            json["mcpServers"]["kelve"] = serde_json::json!({ "command": me, "args": ["mcp"] });
            std::fs::create_dir_all(path.parent().unwrap_or(home)).map_err(|_| line.to_string())?;
            std::fs::write(
                &path,
                serde_json::to_string_pretty(&json).unwrap_or_default(),
            )
            .map_err(|_| line.to_string())?;
            Ok("Connected to Cursor".into())
        }
        _ => Err("kelve mcp".into()),
    }
}

/// `kelve setup`: connects the person's agent, adds kelve's hook to it, and
/// sets how much kelve marks: asking in a terminal, or, run by their agent,
/// leaving it the one word to choose.
pub async fn setup(agent: Option<&str>, word: Option<&str>) -> anyhow::Result<()> {
    let person = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let var = |n: &str| std::env::var(n).ok();
    let (home, claude) = crate::notes::homes()?;
    let me = std::env::current_exe().context("kelve can't tell where it's installed.")?;
    style::head("setup");

    // Which agent: the one named, the one running this, the only one
    // installed, or the person's choice.
    let found: Vec<Connectable> = CONNECTABLE
        .into_iter()
        .filter(|c| on_path(c.command))
        .collect();
    let by_agent = running_in(var);
    let chosen = match agent {
        Some(name) => {
            let a = Agent::parse(name).with_context(|| {
                format!("kelve doesn't know an agent called “{name}”. Say claude, codex, cursor or gemini.")
            })?;
            match CONNECTABLE.into_iter().find(|c| c.agent == a) {
                Some(c) => Some(c),
                None => bail!(
                    "kelve can't connect {name} itself: give it kelve as an MCP server that runs `kelve mcp`."
                ),
            }
        }
        None => match (by_agent, found.as_slice()) {
            (Some(c), _) => Some(c),
            (None, [one]) => Some(*one),
            (None, []) => None,
            (None, many) if person => {
                let names: Vec<&str> = many.iter().map(|c| c.shown).collect();
                Some(many[ask("Which agent should kelve work with?", &names, 0)?])
            }
            (None, many) => Some(many[0]),
        },
    };
    let Some(chosen) = chosen else {
        style::line(
            "No agent kelve knows is installed here. Give yours kelve as an MCP server that runs `kelve mcp`; then run `kelve setup` again.",
        );
        return Ok(());
    };
    // Newly connected, the agent has kelve's tools from its next session.
    let fresh = match connect(chosen, &me, &home) {
        Ok(said) => {
            style::line(&format!("✓ {said}."));
            !said.ends_with("already")
        }
        Err(line) => {
            style::line(&format!(
                "kelve couldn't connect {} itself. Run this where you use it: {line}",
                chosen.shown
            ));
            false
        }
    };

    // The hook: asked of the person; added for the agent they asked to
    // install kelve, which is their yes.
    let hook = !person
        || ask(
            &format!(
                "Let {} pick up your notes, and lessons as it works? It adds one hook to its settings.",
                chosen.shown
            ),
            &["Yes", "No"],
            0,
        )? == 0;
    if hook {
        match kelve_server::hooks::set(chosen.agent, true, &home, &claude) {
            Ok(_) => {
                crate::notes::hook_changed().await;
                style::line(&format!(
                    "✓ Notes and lessons: one hook in {}’s settings. `kelve hook remove {}` takes it out.",
                    chosen.shown,
                    chosen.agent.name()
                ));
            }
            Err(e) => style::line(&e),
        }
    }

    // How much kelve marks: the word given, the person's answer, or the
    // agent's to choose.
    let word = match word {
        Some(w) => Some(level(w)?.0),
        None if person => {
            let words: Vec<&str> = ["New", "Learning", "Working", "Deep"].to_vec();
            Some(
                LEVELS[ask(
                    "How well do you know your work? kelve marks what’s new to you.",
                    &words,
                    1,
                )?]
                .0,
            )
        }
        None => None,
    };
    match word {
        Some(w) => marks_set(w).await?,
        None => style::line(&format!(
            "Last, for {}: how well does this person know their work? From what you know of them, run `kelve marks set new`, `learning`, `working` or `deep` (new marks every idea; deep, specialist ideas only), then tell them what you chose.",
            chosen.shown
        )),
    }
    let then = if fresh {
        format!(
            "{} has kelve’s tools from its next session; then ask it:",
            chosen.shown
        )
    } else {
        format!("Ask {}:", chosen.shown)
    };
    style::line(&format!(
        "kelve is set up. {then} “Show me what you did in kelve.”"
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_is_one_of_four_words() {
        assert_eq!(level("Working").unwrap().0, "working");
        assert!(
            level("expert")
                .unwrap_err()
                .to_string()
                .contains("new, learning, working or deep")
        );
    }

    #[test]
    fn the_agent_running_this_is_known_by_what_it_sets() {
        let only = |name: &'static str| move |n: &str| (n == name).then(|| "1".to_string());
        assert_eq!(
            running_in(only("CLAUDECODE")).map(|c| c.agent),
            Some(Agent::Claude)
        );
        assert_eq!(
            running_in(only("GEMINI_CLI")).map(|c| c.agent),
            Some(Agent::Gemini)
        );
        assert_eq!(running_in(|_| None), None, "a person in a terminal");
    }

    #[test]
    fn cursor_is_connected_in_its_own_file_beside_what_was_there() {
        let home = std::env::temp_dir().join(format!("kelve-setup-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".cursor")).unwrap();
        std::fs::write(
            home.join(".cursor/mcp.json"),
            r#"{"mcpServers":{"other":{"command":"x"}}}"#,
        )
        .unwrap();
        let c = CONNECTABLE[2];
        assert_eq!(
            connect(c, Path::new("/opt/kelve"), &home).unwrap(),
            "Connected to Cursor"
        );
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(home.join(".cursor/mcp.json")).unwrap())
                .unwrap();
        assert_eq!(json["mcpServers"]["kelve"]["command"], "/opt/kelve");
        assert_eq!(json["mcpServers"]["other"]["command"], "x");
        assert_eq!(
            connect(c, Path::new("/opt/kelve"), &home).unwrap(),
            "Cursor was connected already"
        );
        std::fs::remove_dir_all(&home).ok();
    }
}
