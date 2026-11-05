//! Setup's offer to put the person's skills in their agent's own skills
//! folder, so they work where kelve isn't running and in any agent that
//! reads skills; and `kelve setup --remove`, which takes out what setup put
//! in: those links, and kelve's hook, from every agent that has them
//! (`server::agent_loop::links`, `server::hooks`).

use std::path::{Path, PathBuf};

use kelve_core::hook::Agent;
use kelve_server::agent_loop::links;

use crate::style;

/// Where the person's own skills are kept: `KELVE_SKILLS`, or
/// `~/.kelve/skills`, as kelve's server has them.
pub fn skills_dir(home: &Path) -> PathBuf {
    std::env::var("KELVE_SKILLS")
        .ok()
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::home_dir()
                .unwrap_or_else(|| home.to_path_buf())
                .join(".kelve")
                .join("skills")
        })
}

/// Offers to link the person's skills into the agent's skills folder: asked
/// of the person in a terminal; for the agent they asked to install kelve,
/// their yes.
pub fn offer(
    agent: Agent,
    shown: &str,
    asked: impl FnOnce(&str) -> anyhow::Result<bool>,
    home: &Path,
    claude: &Path,
) -> anyhow::Result<()> {
    let Some(folder) = links::folder(agent, home, claude) else {
        style::line(&format!(
            "{shown} has no skills folder kelve knows of: your skills reach it through kelve's hook, and its skills tool."
        ));
        return Ok(());
    };
    let yes = asked(&format!(
        "Put your skills in {shown}’s skills folder too, so they work where kelve isn’t running? kelve links each one there, in {}, and keeps them in step.",
        kelve_server::reader::shown(&folder)
    ))?;
    if !yes {
        // Kept, so kelve's setting says so too, and nothing's linked for them.
        if let Err(e) = links::unlink(agent, home, claude, &skills_dir(home)) {
            style::line(&e);
        }
        return Ok(());
    }
    match links::link(agent, home, claude, &skills_dir(home)) {
        Ok(said) => style::line(&format!("✓ {said} `kelve setup --remove` takes them out.")),
        Err(e) => style::line(&e),
    }
    Ok(())
}

/// `kelve setup --remove`: takes kelve's links to the person's skills out of
/// every agent's skills folder, and kelve's hook out of every agent's
/// settings, leaving the rest as it was. The skills themselves stay.
pub async fn remove() -> anyhow::Result<()> {
    let (home, claude) = crate::hook::homes()?;
    let skills = skills_dir(&home);
    style::head("setup --remove");
    let mut did = false;
    for agent in Agent::ALL {
        match links::unlink(agent, &home, &claude, &skills) {
            Ok(said) if !said.is_empty() => {
                style::line(&format!("✓ {said}"));
                did = true;
            }
            Ok(_) => {}
            Err(e) => style::line(&e),
        }
        if kelve_server::hooks::installed(agent, &home, &claude) {
            match kelve_server::hooks::set(agent, false, &home, &claude) {
                Ok(said) => {
                    style::line(&format!("✓ {said}"));
                    did = true;
                }
                Err(e) => style::line(&e),
            }
        }
    }
    if did {
        crate::hook::hook_changed().await;
    } else {
        style::line("kelve's hook and links to your skills weren't in any agent's settings.");
    }
    style::line(
        "kelve is still connected to your agent as an MCP server: take that out in the agent, as `claude mcp remove kelve`, say.",
    );
    Ok(())
}
