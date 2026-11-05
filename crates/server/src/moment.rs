//! The hook's router: each moment in the person's agent's work, as `kelve
//! moment` passes it on (`core::hook`), and what kelve has for the agent
//! then. As the person sends a message: what they said, kept short with
//! secrets stripped, to catch what they corrected. As the agent reads or
//! edits a file, or writes one through its shell: which files it touched,
//! so kelve knows what work it did. As it opens something in kelve: that
//! its session did, and the handover made then is its session's. As it
//! commits: what the person changed in work it handed over (`handover`).
//! As it hands work over, or its turn ends: to show the work in kelve
//! (`handoff`). Through it all, the person's skills, as they apply
//! (`agent_loop::deliver`). For an agent that can't be given text at the
//! moment (Antigravity), what's kept for it.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::auth::Agent;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

/// A moment in the agent's work, as its hook saw it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentIn {
    /// `read`, `edit`, `ran`, `failed`, `prompt`, `next`, `opened` or `end`.
    pub what: String,
    pub session: String,
    /// The folder it works in.
    pub cwd: String,
    /// The file it read or edited, absolute.
    #[serde(default)]
    pub file: Option<String>,
    /// The command it ran, or the tool and what it was given, as a key for
    /// knowing when the same thing passes.
    #[serde(default)]
    pub command: Option<String>,
    /// What failed, as the tool said it.
    #[serde(default)]
    pub problem: Option<String>,
    /// The agent, as its hook names it.
    #[serde(default)]
    pub agent: Option<String>,
    /// The agent can't be given text now: keep it until its next `next`.
    #[serde(default)]
    pub hold: bool,
    /// Work the command handed over, when it did.
    #[serde(default)]
    pub handover: Option<kelve_core::hook::Handover>,
    /// What it opened in kelve, by its number.
    #[serde(default)]
    pub opened: Option<i64>,
    /// For `prompt`: what the person sent, when the agent's event says.
    #[serde(default)]
    pub prompt: Option<String>,
}

/// What kelve has for the agent at a moment: text to put before it, or
/// nothing.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MomentOut {
    pub text: String,
}

/// Joins what each part of kelve has to say at a moment, leaving out what's
/// empty.
fn joined(parts: impl IntoIterator<Item = String>) -> String {
    parts
        .into_iter()
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A moment in the agent's work: what kelve keeps of it, and what it has
/// for the agent now.
pub async fn moment(
    State(app): State<Shared>,
    _agent: Agent,
    Json(mut m): Json<MomentIn>,
) -> ApiResult<Json<MomentOut>> {
    m.cwd = crate::handoff::at_home(&m.cwd).display().to_string();
    let session = m.session.trim();
    if session.is_empty() {
        return Ok(Json(MomentOut::default()));
    }
    // It opened something in kelve: kept for its session, and nothing more.
    if m.what == "opened" {
        if let Some(id) = m.opened {
            crate::handover::of_session(&app, id, session).await?;
            // The agent loop's: the debrief and the past task are its session's.
            if let Err(e) = crate::agent_loop::debrief::in_session(&app, id, session).await {
                tracing::warn!(error = %e.message, "couldn't keep which session handed work over");
            }
        }
        return Ok(Json(MomentOut::default()));
    }
    // Where it works: a file it reads or edits outside the folder it
    // started in, as in another worktree, is in that file's repository.
    let cwd = m
        .file
        .as_deref()
        .filter(|f| !Path::new(f).starts_with(&m.cwd))
        .and_then(|f| Path::new(f).parent())
        .and_then(kelve_engine::git::root_of)
        .map(|r| r.display().to_string())
        .unwrap_or_else(|| m.cwd.clone());
    // The folder the agent works in, as kelve knows it: one opened in
    // kelve, else the repository it's in. Never one around the repository
    // it's in, as a worktree inside the project is: what other agents
    // write there isn't this one's work.
    let repo = kelve_engine::git::root_of(Path::new(&cwd));
    let known = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::root_of(&mut conn, &cwd)
            .await?
            .filter(|k| !holds_another(k, repo.as_deref()))
    };
    let read_root = known.or_else(|| repo.as_ref().map(|r| r.display().to_string()));
    // Files it reads or edits anywhere else are its work in the folder it
    // works in.
    let root = read_root.clone().unwrap_or_else(|| cwd.clone());
    let agent = m.agent.as_deref().unwrap_or_default().trim();
    let before = {
        let mut conn = app.db.acquire().await?;
        kelve_store::moments::moment_now(&mut conn, session).await?
    };
    // The agent loop's: what was committed since the last handover here,
    // however it was, is compared with it.
    if let Err(e) = crate::agent_loop::after::look(&app, &cwd).await {
        tracing::warn!(error = %e.message, "couldn't compare what was committed with a handover");
    }
    let text = match m.what.as_str() {
        "read" | "edit" => match &m.file {
            Some(file) => {
                let edit = m.what == "edit";
                touch(&app, session, file, edit, Some(&root)).await?;
                // The skill for this kind of work, once.
                crate::agent_loop::deliver::on_file(&app, session, &cwd, file).await?
            }
            None => String::new(),
        },
        "ran" => match &m.command {
            Some(command) => {
                // What the command wrote, first: a command that writes a file
                // and runs the tests, as agents' do, passes with it.
                shell_writes(&app, session, read_root.as_deref(), before).await?;
                if command.contains("git commit") {
                    crate::handover::on_commit(&app, session, &cwd).await?;
                }
                String::new()
            }
            None => String::new(),
        },
        // The turn ends, or, for an agent that can't say so, the person
        // writes again: work it did, to show if it's done.
        "end" | "prompt" => {
            if let Some(said) = m.prompt.as_deref().filter(|_| m.what == "prompt") {
                kept_message(&app, session, agent, &cwd, said).await?;
                // The agent loop's: a correction is learned from.
                let root = crate::agent_loop::repo_of(&cwd).await;
                crate::agent_loop::learn::on_message(&app, session, &root, said).await?;
            }
            // The agent loop's: the skills the session starts with, as it
            // does; and, as its turn ends, the jobs waiting for it.
            let skills = match m.what.as_str() {
                "prompt" => crate::agent_loop::deliver::at_prompt(&app, session, &cwd).await?,
                _ => crate::agent_loop::deliver::waiting(&app, session).await?,
            };
            joined([skills, crate::handoff::done(&app, session).await?])
        }
        "failed" => {
            shell_writes(&app, session, read_root.as_deref(), before).await?;
            String::new()
        }
        // About to call its model: what was kept for it.
        "next" => {
            let mut conn = app.db.acquire().await?;
            kelve_store::moments::take_held(&mut conn, session)
                .await?
                .join("\n\n")
        }
        _ => String::new(),
    };
    // Work handed over: shown in kelve, if it hasn't been.
    let text = match &m.handover {
        Some(h) => joined([text, crate::handoff::after(&app, session, h).await?]),
        None => text,
    };
    if m.hold && !text.is_empty() {
        let mut conn = app.db.acquire().await?;
        kelve_store::moments::hold_for(&mut conn, session, &text).await?;
        return Ok(Json(MomentOut::default()));
    }
    Ok(Json(MomentOut { text }))
}

/// Keeps what the person sent their agent, in the repository it works in,
/// or the folder: short, with secrets stripped (`store::handovers`).
async fn kept_message(
    app: &Shared,
    session: &str,
    agent: &str,
    cwd: &str,
    said: &str,
) -> ApiResult<()> {
    let at = PathBuf::from(cwd);
    let root = tokio::task::spawn_blocking(move || kelve_engine::git::root_of(&at))
        .await
        .ok()
        .flatten()
        .map_or_else(|| cwd.to_string(), |r| r.display().to_string());
    let mut conn = app.db.acquire().await?;
    kelve_store::handovers::keep_message(&mut conn, session, agent, &root, said).await?;
    Ok(())
}

/// Whether `known`, a folder kelve knows, holds `repo`, a repository of its
/// own, rather than being it or in it.
pub(crate) fn holds_another(known: &str, repo: Option<&Path>) -> bool {
    let Some(repo) = repo else {
        return false;
    };
    let known = std::fs::canonicalize(known).unwrap_or_else(|_| PathBuf::from(known));
    let repo = std::fs::canonicalize(repo).unwrap_or_else(|_| repo.to_path_buf());
    repo != known && repo.starts_with(&known)
}

/// A file's path in a folder, `/` between its parts, when it's in it and
/// kelve would read it by its name.
pub(crate) fn new_in(root: &str, file: &str) -> Option<String> {
    let rel = Path::new(file).strip_prefix(root).ok()?;
    let rel: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let rel = rel.join("/");
    (!rel.is_empty() && kelve_engine::walk::by_name(&rel).is_none()).then_some(rel)
}

/// Where a file is: in the folder the agent works in, by its path there.
fn place(file: &str, in_root: Option<&str>) -> Option<(String, String)> {
    Some((in_root?.to_string(), new_in(in_root?, file)?))
}

/// The agent read or edited a file: kept for its session, a day, with when
/// it last edited it, so kelve knows what work it did (`handoff::done`).
async fn touch(
    app: &Shared,
    session: &str,
    file: &str,
    edit: bool,
    in_root: Option<&str>,
) -> ApiResult<()> {
    if let Some((root, path)) = place(file, in_root) {
        let mut conn = app.db.acquire().await?;
        kelve_store::moments::touch(&mut conn, session, &root, &path, edit).await?;
    }
    Ok(())
}

/// Folders whose walk for shell writes was cut short, logged once each.
static CUT_SHORT: LazyLock<Mutex<std::collections::HashSet<String>>> =
    LazyLock::new(Default::default);

/// What the agent wrote through its shell since its last moment, however it
/// wrote it: each file git sees changed, or, in a folder opened in kelve
/// that isn't a repository, each file kelve walks, written since. Each is
/// kept as edited. The person writing in the same moment can't be told
/// apart.
async fn shell_writes(
    app: &Shared,
    session: &str,
    root: Option<&str>,
    since: Option<i64>,
) -> ApiResult<()> {
    // Only in a folder kelve knows: an agent working anywhere else costs
    // no walk.
    let (Some(root), Some(since)) = (root, since) else {
        return Ok(());
    };
    let dir = std::path::PathBuf::from(root);
    let found = tokio::task::spawn_blocking(move || kelve_engine::git::written_since(&dir, since))
        .await
        .map_err(ApiError::internal)?;
    let found = match found {
        Ok(found) => found,
        Err(e) => {
            tracing::debug!(error = %e, "couldn't look for what the shell wrote");
            return Ok(());
        }
    };
    if found.more
        && CUT_SHORT
            .lock()
            .map(|mut cut| cut.insert(root.to_string()))
            .unwrap_or(false)
    {
        tracing::info!(
            most = kelve_engine::git::WRITTEN_WALK,
            "a folder has more files than kelve looks through for what the shell wrote; the rest aren't seen"
        );
    }
    for file in found.files {
        let file = file.display().to_string();
        // New files the agent made in the folder too; never kelve's own.
        let Some((_, path)) = place(&file, Some(root)) else {
            continue;
        };
        if crate::reader::is_kelves(&path) {
            continue;
        }
        touch(app, session, &file, true, Some(root)).await?;
    }
    Ok(())
}
