//! Putting kelve's hook into the person's agent's settings, and taking it out,
//! only with their yes (`core::hook`): the files, read and written here, for
//! settings and for `kelve hook`.

use std::path::{Path, PathBuf};

use kelve_core::hook::{Agent, install, installed as holds, partly, remove};

/// Where an agent's hooks go: Claude Code's in its own folder, which may be
/// elsewhere; the rest from the home folder.
pub fn file(agent: Agent, home: &Path, claude: &Path) -> PathBuf {
    match agent {
        Agent::Claude => claude.join("settings.json"),
        _ => home.join(agent.file()),
    }
}

/// Whether the agent's settings hold kelve's hook.
pub fn installed(agent: Agent, home: &Path, claude: &Path) -> bool {
    let text = std::fs::read_to_string(file(agent, home, claude)).unwrap_or_default();
    holds(agent, &text)
}

/// Brings kelve's hook up to date in each agent's settings that hold it:
/// the person said yes to it, and a newer kelve may run on more events.
/// Only kelve's own entries change; what it can't change is logged.
pub fn up_to_date(home: &Path, claude: &Path) {
    for agent in Agent::ALL {
        let path = file(agent, home, claude);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        if !partly(agent, &text) || holds(agent, &text) {
            continue;
        }
        match set(agent, true, home, claude) {
            Ok(_) => tracing::info!(agent = agent.name(), "brought kelve's hook up to date"),
            Err(e) => {
                tracing::warn!(agent = agent.name(), error = %e, "couldn't bring kelve's hook up to date")
            }
        }
    }
}

fn shown(path: &Path) -> String {
    crate::reader::shown(path)
}

/// Puts kelve's hook in, or takes it out, leaving the rest of the agent's
/// settings as they were: what was done, in a sentence or two, or why not.
pub fn set(agent: Agent, on: bool, home: &Path, claude: &Path) -> Result<String, String> {
    let path = file(agent, home, claude);
    let cant = |e: std::io::Error| {
        format!(
            "kelve couldn't change {}: {e}. Check you can write there, and try again.",
            shown(&path)
        )
    };
    let was = std::fs::read_to_string(&path).unwrap_or_default();
    let home_text = home.display().to_string();
    if on {
        let now = install(agent, &was, &home_text)?;
        if let Some((script, says)) = agent.script() {
            let script = home.join(script);
            if let Some(dir) = script.parent() {
                std::fs::create_dir_all(dir).map_err(cant)?;
            }
            std::fs::write(&script, says).map_err(cant)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                    .map_err(cant)?;
            }
        }
        if now != was {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(cant)?;
            }
            std::fs::write(&path, now).map_err(cant)?;
        }
        let mut said = format!(
            "{} is told of notes you leave it, and given lessons as it works. The hook is in {}.",
            agent.shown(),
            shown(&path)
        );
        if let Some(then) = agent.then() {
            said.push(' ');
            said.push_str(then);
        }
        Ok(said)
    } else {
        match remove(agent, &was)? {
            Some(now) if now != was => std::fs::write(&path, now).map_err(cant)?,
            Some(_) => {}
            None if path.exists() => std::fs::remove_file(&path).map_err(cant)?,
            None => {}
        }
        if let Some((script, _)) = agent.script() {
            let _ = std::fs::remove_file(home.join(script));
        }
        Ok(format!("kelve's hook is out of {}.", shown(&path)))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn an_older_hook_is_brought_up_to_date_and_no_one_elses_is_touched() {
        let home =
            std::env::temp_dir().join(format!("kelve-hooks-{}", uuid::Uuid::now_v7().simple()));
        let claude = home.join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        std::fs::create_dir_all(home.join(".codex")).unwrap();
        // Claude Code's, from an older kelve: on fewer events, and on one
        // kelve no longer uses, before a command.
        let older = r#"{"hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"kelve moment --agent claude --on UserPromptSubmit"}]}],"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"kelve moment --agent claude --on PreToolUse"}]}],"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#;
        std::fs::write(claude.join("settings.json"), older).unwrap();
        // Codex's, with no kelve hook: never asked for, so never added.
        let theirs = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]}}"#;
        std::fs::write(home.join(".codex/hooks.json"), theirs).unwrap();
        up_to_date(&home, &claude);
        assert!(installed(Agent::Claude, &home, &claude));
        let now = std::fs::read_to_string(claude.join("settings.json")).unwrap();
        assert!(
            now.contains("--on PostToolUseFailure")
                && !now.contains("--on PreToolUse")
                && now.contains("say done"),
            "{now}"
        );
        assert_eq!(
            std::fs::read_to_string(home.join(".codex/hooks.json")).unwrap(),
            theirs
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn each_agents_hook_goes_in_and_comes_out_leaving_theirs() {
        let home =
            std::env::temp_dir().join(format!("kelve-hooks-{}", uuid::Uuid::now_v7().simple()));
        let claude = home.join(".claude");
        std::fs::create_dir_all(home.join(".cursor")).unwrap();
        std::fs::write(
            home.join(".cursor/hooks.json"),
            r#"{"version":1,"hooks":{"stop":[{"command":"say done"}]}}"#,
        )
        .unwrap();
        for agent in Agent::ALL {
            set(agent, true, &home, &claude).unwrap();
            assert!(installed(agent, &home, &claude), "{}", agent.name());
            set(agent, false, &home, &claude).unwrap();
            assert!(!installed(agent, &home, &claude), "{}", agent.name());
        }
        assert!(
            !home.join(".copilot/hooks/kelve.json").exists(),
            "kelve's own file goes"
        );
        assert!(
            !home.join(".augment/hooks/kelve-moment.sh").exists(),
            "and its script"
        );
        let cursor = std::fs::read_to_string(home.join(".cursor/hooks.json")).unwrap();
        assert!(cursor.contains("say done") && !cursor.contains("kelve moment"));
        std::fs::remove_dir_all(&home).unwrap();
    }
}
