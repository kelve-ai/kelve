//! `kelve hook`, and what kelve's hook says of what's waiting. The agent is told
//! as the person sends it a message, through a hook in its settings that runs
//! `kelve moment`, added only with the person's yes: one line when decisions
//! from a debrief wait to be redone, in the project it works in, that it hasn't
//! been told of, and nothing otherwise.

use std::path::PathBuf;

use anyhow::{Context, bail};

use crate::client::Kelve;

/// The folder the agent works in.
pub fn here() -> String {
    std::env::current_dir()
        .map(|d| d.display().to_string())
        .unwrap_or_default()
}

/// The line the hook prints, when decisions wait to be redone.
pub fn redo_line(n: usize) -> Option<String> {
    match n {
        0 => None,
        1 => Some("kelve: the person wants a decision in your work done another way. Before you start what they asked, take it with kelve's next_job, do it, and hand it in with hand_in.".into()),
        n => Some(format!(
            "kelve: the person wants {n} decisions in your work done another way. Before you start what they asked, take them with kelve's next_job, do each, and hand each in with hand_in."
        )),
    }
}

/// How many decisions wait to be redone here that the hook hasn't told the
/// agent of: each is told of now.
pub async fn new_here(kelve: &Kelve, agent: &str) -> anyhow::Result<usize> {
    let new = kelve
        .send(
            kelve
                .post("/api/agent/redos/new")
                .json(&serde_json::json!({ "path": here(), "agent": agent })),
        )
        .await?;
    Ok(new["new"].as_u64().unwrap_or(0) as usize)
}

/// The home folder, and Claude Code's own folder, which may be elsewhere.
pub(crate) fn homes() -> anyhow::Result<(PathBuf, PathBuf)> {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    // The home folder as the system has it: `HOME`, or on Windows the
    // user's profile.
    let home = var("KELVE_AGENTS_HOME")
        .map(PathBuf::from)
        .or_else(std::env::home_dir)
        .context("kelve can't find your home folder, so it can't find your agent's settings.")?;
    let claude = var("KELVE_CLAUDE_DIR")
        .or_else(|| var("CLAUDE_CONFIG_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"));
    Ok((home, claude))
}

/// `kelve hook install|remove <agent>`: adds kelve's entries to the
/// agent's hooks, or takes them out, leaving the rest as it was
/// (`core::hook`).
pub async fn hook(install: bool, agent: &str) -> anyhow::Result<()> {
    let Some(agent) = kelve_core::hook::Agent::parse(agent) else {
        let names: Vec<&str> = kelve_core::hook::Agent::ALL
            .iter()
            .map(|a| a.name())
            .collect();
        bail!(
            "kelve has hooks for {}. Other agents read your skills as kelve's instructions ask.",
            names.join(", ")
        );
    };
    let (home, claude) = homes()?;
    let said =
        kelve_server::hooks::set(agent, install, &home, &claude).map_err(anyhow::Error::msg)?;
    anstream::println!("{said}");
    hook_changed().await;
    Ok(())
}

/// Tells a running kelve its hook changed, so a page that's open says so;
/// nothing when kelve isn't running.
pub(crate) async fn hook_changed() {
    if let Ok(kelve) = Kelve::new(std::time::Duration::from_secs(2)) {
        let _ = kelve.send(kelve.post("/api/agent/hook")).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hook_says_one_line_or_nothing() {
        assert_eq!(redo_line(0), None);
        assert!(redo_line(1).unwrap().contains("a decision in your work"));
        assert!(redo_line(2).unwrap().contains("2 decisions in your work"));
    }
}
