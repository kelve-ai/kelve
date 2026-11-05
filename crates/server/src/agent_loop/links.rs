//! The person's skills in each agent's own skills folder, so they work where
//! kelve isn't running and in any agent that reads skills. It's on for a new
//! install: `kelve setup` asks as it connects an agent, and anything else that
//! connects one gets the links as kelve next starts or next changes a skill,
//! until the person says no, in setup or in settings. Each skill folder is
//! linked, not copied, so an edit is everywhere at once; kelve keeps the links
//! in step as skills come and go, and touches nothing in those folders that
//! isn't its own link.
//!
//! Where each agent reads personal skills, from its docs (November 2026):
//! - Claude Code: `~/.claude/skills/<name>/SKILL.md` (its own folder,
//!   which may be elsewhere), following links;
//! - Codex: `~/.agents/skills`, following links;
//! - Gemini CLI: `~/.gemini/skills` (it reads `~/.agents/skills` too);
//! - Cursor: `~/.cursor/skills` (it reads Claude Code's and Codex's too).
//!
//! The others kelve connects to have no skills folder kelve knows of.

use std::io;
use std::path::{Path, PathBuf};

use kelve_core::hook::Agent;

use crate::skills_home;
use crate::state::Shared;

/// The file in kelve's own skills folder naming the agents whose skills
/// folders the person said yes to, one a line. Empty, they said no. Not
/// there, they've said nothing yet, and the setting stands.
const LINKED: &str = ".linked";

/// Where an agent reads the person's own skills, when it has such a folder.
pub fn folder(agent: Agent, home: &Path, claude: &Path) -> Option<PathBuf> {
    match agent {
        Agent::Claude => Some(claude.join("skills")),
        Agent::Codex => Some(home.join(".agents").join("skills")),
        Agent::Gemini => Some(home.join(".gemini").join("skills")),
        Agent::Cursor => Some(home.join(".cursor").join("skills")),
        _ => None,
    }
}

/// The agents the person said yes to.
pub fn linked(skills: &Path) -> Vec<Agent> {
    std::fs::read_to_string(skills.join(LINKED))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| Agent::parse(l.trim()))
        .collect()
}

/// Whether the person has said: yes to an agent's folder, or no to them
/// all. None until they do.
fn said(skills: &Path) -> Option<bool> {
    skills
        .join(LINKED)
        .is_file()
        .then(|| !linked(skills).is_empty())
}

fn set_linked(skills: &Path, agents: &[Agent]) -> io::Result<()> {
    std::fs::create_dir_all(skills)?;
    let text: String = agents.iter().map(|a| format!("{}\n", a.name())).collect();
    std::fs::write(skills.join(LINKED), text)
}

/// Whether `entry` is kelve's own link: one into kelve's skills folder.
fn ours(entry: &Path, skills: &Path) -> bool {
    std::fs::read_link(entry).is_ok_and(|to| to.starts_with(skills))
}

#[cfg(unix)]
fn symlink(to: &Path, at: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(to, at)
}

#[cfg(windows)]
fn symlink(to: &Path, at: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(to, at)
}

#[cfg(not(any(unix, windows)))]
fn symlink(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::other("links aren't made here"))
}

/// Puts a link to each of the person's skills in an agent's folder, takes
/// out its links to skills that are gone, and leaves everything else as it
/// was. How many it links, and the names it couldn't, because the agent
/// has a skill of its own by that name.
fn in_step(skills: &Path, to: &Path) -> io::Result<(usize, Vec<String>)> {
    std::fs::create_dir_all(to)?;
    let mut clashes = Vec::new();
    let mut count = 0;
    for found in skills_home::list(skills)? {
        let at = to.join(&found.name);
        if ours(&at, skills) {
            count += 1;
            continue;
        }
        if at.symlink_metadata().is_ok() {
            clashes.push(found.name);
            continue;
        }
        symlink(&found.dir, &at)?;
        count += 1;
    }
    for entry in std::fs::read_dir(to)? {
        let path = entry?.path();
        if ours(&path, skills) && !path.join(skills_home::FILE).is_file() {
            remove_link(&path)?;
        }
    }
    Ok((count, clashes))
}

#[cfg(windows)]
fn remove_link(path: &Path) -> io::Result<()> {
    std::fs::remove_dir(path)
}

#[cfg(not(windows))]
fn remove_link(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

/// Links the person's skills into an agent's skills folder, and keeps them
/// in step from now on: what was done, in a sentence, or why not.
pub fn link(agent: Agent, home: &Path, claude: &Path, skills: &Path) -> Result<String, String> {
    let Some(to) = folder(agent, home, claude) else {
        return Err(format!(
            "{} has no skills folder kelve knows of, so your skills reach it through kelve's hook and its skills tool.",
            agent.shown()
        ));
    };
    let cant = |e: io::Error| {
        format!(
            "kelve couldn't link your skills into {}: {e}. Check you can write there, and try again.",
            crate::reader::shown(&to)
        )
    };
    std::fs::create_dir_all(skills).map_err(cant)?;
    let (count, clashes) = in_step(skills, &to).map_err(cant)?;
    let mut agents = linked(skills);
    if !agents.contains(&agent) {
        agents.push(agent);
    }
    set_linked(skills, &agents).map_err(cant)?;
    let mut said = format!(
        "Your skills are in {}’s skills folder, {}: {count} now, and each new one as kelve makes it.",
        agent.shown(),
        crate::reader::shown(&to)
    );
    if !clashes.is_empty() {
        said.push_str(&format!(
            " It has skills of its own called {}, so kelve left those as they are.",
            clashes.join(", ")
        ));
    }
    Ok(said)
}

/// Takes kelve's links out of an agent's skills folder, and stops keeping
/// them in step: what was done, in a sentence.
pub fn unlink(agent: Agent, home: &Path, claude: &Path, skills: &Path) -> Result<String, String> {
    let Some(to) = folder(agent, home, claude) else {
        return Ok(String::new());
    };
    let mut gone = 0;
    if let Ok(entries) = std::fs::read_dir(&to) {
        for entry in entries.flatten() {
            let path = entry.path();
            if ours(&path, skills) {
                remove_link(&path).map_err(|e| {
                    format!(
                        "kelve couldn't take its links out of {}: {e}.",
                        crate::reader::shown(&to)
                    )
                })?;
                gone += 1;
            }
        }
    }
    let agents: Vec<Agent> = linked(skills).into_iter().filter(|a| *a != agent).collect();
    set_linked(skills, &agents).map_err(|e| format!("kelve couldn't change {LINKED}: {e}."))?;
    Ok(if gone == 0 {
        String::new()
    } else {
        format!(
            "Took your skills out of {}’s skills folder: {gone} {}. The skills themselves stay in {}.",
            agent.shown(),
            if gone == 1 { "link" } else { "links" },
            crate::reader::shown(skills)
        )
    })
}

/// The person turned "skills in each agent's own folder" on or off in
/// settings: linked into each agent whose hook kelve has, or taken out of
/// every one. What couldn't be done, logged.
pub fn follow_setting(app: &Shared, on: bool) {
    let (home, claude, skills) = (
        &app.config.agents_home,
        &app.config.claude_dir,
        &app.config.skills_dir,
    );
    let agents: Vec<Agent> = if on {
        Agent::ALL
            .into_iter()
            .filter(|a| folder(*a, home, claude).is_some())
            .filter(|a| crate::hooks::installed(*a, home, claude))
            .collect()
    } else {
        linked(skills)
    };
    for agent in agents {
        let done = if on {
            link(agent, home, claude, skills)
        } else {
            unlink(agent, home, claude, skills)
        };
        if let Err(e) = done {
            tracing::warn!(agent = agent.name(), error = %e, "couldn't change the skills links");
        }
    }
    // Turned on with no agent to link yet: a no from before no longer stands.
    if on
        && linked(skills).is_empty()
        && let Err(e) = std::fs::remove_file(skills.join(LINKED))
        && e.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(error = %e, "couldn't change the skills links");
    }
}

/// The setting as the person last said: on when setup linked an agent's
/// folder, off when they said no or took them all out. If they've said
/// nothing yet, it stands.
pub async fn sync_setting(app: &Shared) {
    let Some(on) = said(&app.config.skills_dir) else {
        return;
    };
    if app.settings().skills_folder == on {
        return;
    }
    let mut s = (*app.settings()).clone();
    s.skills_folder = on;
    if let Err(e) = app.save_settings(s).await {
        tracing::warn!(error = %e.message, "couldn't keep whether skills are linked");
    }
}

/// Keeps the links in each agent's folder the person said yes to in step
/// with their skills: new skills linked, gone ones taken out. With the
/// setting on and nothing said yet, that's each agent with kelve's hook.
pub fn keep_in_step(app: &Shared) {
    let skills = &app.config.skills_dir;
    if app.settings().skills_folder && said(skills).is_none() {
        follow_setting(app, true);
    }
    for agent in linked(skills) {
        let Some(to) = folder(agent, &app.config.agents_home, &app.config.claude_dir) else {
            continue;
        };
        if let Err(e) = in_step(skills, &to) {
            tracing::warn!(agent = agent.name(), error = %e, "couldn't keep the skills links in step");
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn skills_are_linked_kept_in_step_and_taken_out_leaving_the_agents_own() {
        let base = std::env::temp_dir().join(format!("kelve-links-{}", crate::crypto::token()));
        let (home, claude, skills) = (base.join("home"), base.join("claude"), base.join("skills"));
        skills_home::write(&skills, "rust", "---\nname: rust\n---\n").unwrap();
        // Claude has a skill of its own called rust.
        std::fs::create_dir_all(claude.join("skills/docs")).unwrap();
        skills_home::write(&skills, "docs", "---\nname: docs\n---\n").unwrap();
        let said = link(Agent::Claude, &home, &claude, &skills).unwrap();
        assert!(said.contains("1 now"), "{said}");
        assert!(said.contains("called docs"), "{said}");
        assert!(claude.join("skills/rust/SKILL.md").is_file());
        assert_eq!(linked(&skills), vec![Agent::Claude]);
        // A new skill is linked; a gone one's link goes.
        skills_home::write(&skills, "words", "---\nname: words\n---\n").unwrap();
        std::fs::remove_dir_all(skills.join("rust")).unwrap();
        in_step(&skills, &claude.join("skills")).unwrap();
        assert!(claude.join("skills/words/SKILL.md").is_file());
        assert!(claude.join("skills/rust").symlink_metadata().is_err());
        let said = unlink(Agent::Claude, &home, &claude, &skills).unwrap();
        assert!(said.contains("1 link."), "{said}");
        assert!(claude.join("skills/docs").is_dir(), "the agent's own stays");
        assert!(linked(&skills).is_empty());
        assert!(
            folder(Agent::Codex, &home, &claude)
                .unwrap()
                .ends_with(".agents/skills")
        );
        std::fs::remove_dir_all(&base).ok();
    }
}
