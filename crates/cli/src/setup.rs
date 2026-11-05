//! Setting kelve up, as the installer does once kelve is in place: connecting
//! the person's agent, and adding kelve's hook so it gets their skills as it
//! works. Run by their agent, which they asked to install kelve, it does both.
//! Run by the person in a terminal, it asks.

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, bail};
use kelve_core::hook::Agent;

use crate::style;

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

/// Asks the person, in the terminal: the question, then its choices on a
/// line of their own, the one taken marked with kelve's square. ← and → move
/// it, return takes it, and a choice's first letter takes that one. Where the
/// terminal can't give keys one at a time, they type the choice instead.
fn ask(question: &str, choices: &[&str], default: usize) -> anyhow::Result<usize> {
    style::line(question);
    match pick(choices, default) {
        Ok(Some(i)) => Ok(i),
        Ok(None) => anyhow::bail!("Setup stopped. Run `kelve setup` to start it again."),
        Err(_) => {
            // In place of the choices, if they were drawn.
            crossterm::execute!(
                std::io::stderr(),
                crossterm::cursor::MoveToColumn(0),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::CurrentLine)
            )
            .ok();
            typed(choices, default)
        }
    }
}

/// The choice taken with the keys, or none when the person stops setup with
/// Ctrl-C or Esc.
fn pick(choices: &[&str], default: usize) -> std::io::Result<Option<usize>> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use crossterm::{cursor, queue, terminal};

    /// Puts the terminal back as it was, however this ends.
    struct Raw;
    impl Drop for Raw {
        fn drop(&mut self) {
            terminal::disable_raw_mode().ok();
            crossterm::execute!(std::io::stderr(), cursor::Show).ok();
        }
    }

    // The choices, the one at `at` marked; with the keys, while choosing.
    let row = |at: usize, keys: bool| {
        let shown: Vec<String> = choices
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == at {
                    format!("{} {}", style::mark(), style::bold(c))
                } else {
                    format!("  {}", style::soft(c))
                }
            })
            .collect();
        let keys = if keys {
            format!("    {}", style::soft("← → then return"))
        } else {
            String::new()
        };
        format!("  {}{keys}", shown.join("   "))
    };
    let draw = |text: String| -> std::io::Result<()> {
        let mut err = std::io::stderr();
        queue!(
            err,
            cursor::MoveToColumn(0),
            terminal::Clear(terminal::ClearType::CurrentLine)
        )?;
        err.flush()?;
        anstream::eprint!("{text}");
        std::io::stderr().flush()
    };

    terminal::enable_raw_mode()?;
    let _raw = Raw;
    crossterm::execute!(std::io::stderr(), cursor::Hide)?;
    let mut at = default;
    let taken = loop {
        draw(row(at, true))?;
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Left | KeyCode::Up | KeyCode::BackTab => {
                at = (at + choices.len() - 1) % choices.len();
            }
            KeyCode::Right | KeyCode::Down | KeyCode::Tab => at = (at + 1) % choices.len(),
            KeyCode::Enter => break Some(at),
            KeyCode::Esc => break None,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break None,
            KeyCode::Char(c) => {
                let c = c.to_lowercase().to_string();
                if let Some(i) = choices
                    .iter()
                    .position(|ch| ch.to_lowercase().starts_with(&c))
                {
                    at = i;
                    break Some(at);
                }
            }
            _ => {}
        }
    };
    draw(taken.map_or_else(String::new, |at| row(at, false)))?;
    anstream::eprint!("\r\n");
    Ok(taken)
}

/// The choice typed, or the default when they just press return.
fn typed(choices: &[&str], default: usize) -> anyhow::Result<usize> {
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
        anstream::print!(
            "  Type one, or return for the one in brackets: {} ",
            shown.join(" / ")
        );
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

/// What setup says when it connects an agent again, whose kelve was set to
/// read elsewhere.
const OVERRIDDEN: &str =
    " afresh, without the kelve settings it had, such as another hosted service to use";

/// Why an agent's connection to kelve, as `claude mcp get kelve` shows it,
/// should be made again: kelve's own settings in its environment, such as
/// `KELVE_GRAPH_URL` from developing kelve, which would keep a new kelve
/// using another hosted service; or a command that isn't there any more.
fn stale(shown: &str) -> Option<&'static str> {
    let mut env = false;
    for line in shown.lines() {
        let line = line.trim();
        if line.starts_with("Environment:") {
            env = true;
        } else if env && line.starts_with("KELVE_") {
            return Some(OVERRIDDEN);
        } else if let Some(command) = line.strip_prefix("Command:") {
            let command = Path::new(command.trim());
            if command.is_absolute() && !command.exists() {
                return Some(" again: the kelve it ran isn't there any more");
            }
        }
    }
    None
}

/// Connects the agent to kelve with its own command, as `kelve mcp` from
/// wherever this kelve is: what it said, or the line to run instead.
fn connect(c: Connectable, me: &Path, home: &Path) -> Result<String, String> {
    let me = me.display().to_string();
    let quietly = |cmd: &mut Command| cmd.output().ok().filter(|o| o.status.success()).is_some();
    match c.agent {
        Agent::Claude => {
            let had = Command::new("claude")
                .args(["mcp", "get", "kelve"])
                .output()
                .ok();
            let mut again = "";
            if let Some(had) = had.filter(|o| o.status.success()) {
                match stale(&String::from_utf8_lossy(&had.stdout)) {
                    None => return Ok("Claude Code was connected already".into()),
                    // Connected as it shouldn't be: taken out, and connected afresh.
                    Some(why) => {
                        quietly(
                            Command::new("claude").args(["mcp", "remove", "kelve", "-s", "user"]),
                        );
                        again = why;
                    }
                }
            }
            quietly(
                Command::new("claude")
                    .args(["mcp", "add", "--scope", "user", "kelve", "--", &me, "mcp"]),
            )
            .then(|| format!("Connected to Claude Code{again}"))
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
            let had = &json["mcpServers"]["kelve"];
            let setting = had["env"]
                .as_object()
                .is_some_and(|env| env.keys().any(|k| k.starts_with("KELVE_")));
            if had.is_object() && !setting {
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
            Ok(if setting {
                format!("Connected to Cursor{OVERRIDDEN}")
            } else {
                "Connected to Cursor".into()
            })
        }
        _ => Err("kelve mcp".into()),
    }
}

/// `kelve setup`: connects the person's agent, and adds kelve's hook to it:
/// asking in a terminal, or, run by their agent, which is their yes.
pub async fn setup(agent: Option<&str>) -> anyhow::Result<()> {
    let person = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let var = |n: &str| std::env::var(n).ok();
    let (home, claude) = crate::hook::homes()?;
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
                "Give {} your skills as it works? It adds one hook to its settings.",
                chosen.shown
            ),
            &["Yes", "No"],
            0,
        )? == 0;
    if hook {
        match kelve_server::hooks::set(chosen.agent, true, &home, &claude) {
            Ok(_) => {
                crate::hook::hook_changed().await;
                style::line(&format!(
                    "✓ Skills: one hook in {}’s settings. `kelve hook remove {}` takes it out.",
                    chosen.shown,
                    chosen.agent.name()
                ));
            }
            Err(e) => style::line(&e),
        }
    }

    // The skills folder: asked of the person; for the agent they asked to
    // install kelve, their yes.
    crate::skills_setup::offer(
        chosen.agent,
        chosen.shown,
        |question| Ok(!person || ask(question, &["Yes", "No"], 0)? == 0),
        &home,
        &claude,
    )?;
    // kelve's settings say so too.
    crate::hook::hook_changed().await;

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
    fn a_connection_that_sets_kelves_own_settings_is_made_again() {
        let dev = "kelve:\n  Scope: User config (available in all your projects)\n  Status: ✔ Connected\n  Type: stdio\n  Command: kelve\n  Args: mcp\n  Environment:\n    KELVE_GRAPH_URL=http://localhost:4620\n";
        assert_eq!(stale(dev), Some(OVERRIDDEN));
        // Absolute on every system: Windows's need a drive.
        let missing = std::env::temp_dir().join("kelve-that-was-taken-out");
        let gone = format!(
            "kelve:\n  Command: {}\n  Args: mcp\n  Environment:\n",
            missing.display()
        );
        assert!(stale(&gone).is_some(), "{gone}");
        let fine = "kelve:\n  Command: kelve\n  Args: mcp\n  Environment:\n\nTo remove this server, run: claude mcp remove kelve -s user\n";
        assert_eq!(stale(fine), None);
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
