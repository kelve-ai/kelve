//! The hook's router: each moment in the person's agent's work, as `kelve
//! moment` passes it on (`core::hook`), and what kelve has for the agent
//! then. As the person sends a message: what they said, kept short with
//! secrets stripped, to catch what they corrected. As the agent reads or
//! edits a file, or writes one through its shell: which files it touched,
//! so kelve knows what work it did. As it opens something in kelve: that
//! its session did, and the handover made then is its session's. As it
//! commits: what the person changed in work it handed over (`handover`).
//! As it hands work over, or its turn ends: to show the work in kelve, and
//! to finish reading what it opened there (`handoff`). For an agent that
//! can't be given text at the moment (Antigravity), what's kept for it.
//!
//! Lessons (`lessons`) hang off the same moments: each is one call below,
//! marked as lessons', so they can be taken out whole.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::auth::Agent;
use crate::error::{ApiError, ApiResult};
use crate::lessons;
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
    /// For an edit: what was written, to match what it's about.
    #[serde(default)]
    pub written: Option<String>,
    /// For an edit: the text it replaced, when the agent's tool says.
    #[serde(default)]
    pub replaced: Option<String>,
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
            crate::handoff::opened(&app, session, id).await?;
            crate::handover::of_session(&app, id, session).await?;
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
    // The folder the agent works in, as kelve knows it: one the person
    // opened, else one walked for what its files import, else one walked
    // now, in the background, the first time the hook meets it. Never one
    // around the repository it's in, as a worktree inside the project is:
    // what other agents write there isn't this one's work.
    let repo = kelve_engine::git::root_of(Path::new(&cwd));
    let read = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::root_of(&mut conn, &cwd)
            .await?
            .filter(|k| !holds_another(k, repo.as_deref()))
    };
    let read_root = match read {
        Some(root) => Some(root),
        // Lessons'.
        None => lessons::walked_root(&app, &cwd, repo.as_deref()).await?,
    };
    let root = read_root.clone().unwrap_or_else(|| cwd.clone());
    let agent = m.agent.as_deref().unwrap_or_default().trim();
    let before = {
        let mut conn = app.db.acquire().await?;
        kelve_store::lessons::moment_now(&mut conn, session).await?
    };
    let text = match m.what.as_str() {
        "read" | "edit" => match &m.file {
            Some(file) => {
                let edit = m.what == "edit";
                touch(&app, session, file, edit, read_root.as_deref()).await?;
                // Lessons'.
                lessons::on_file(
                    &app,
                    session,
                    agent,
                    file,
                    edit,
                    m.written.as_deref(),
                    m.replaced.as_deref(),
                    read_root.as_deref(),
                )
                .await?
            }
            None => String::new(),
        },
        "ran" => match &m.command {
            Some(command) => {
                // What the command wrote, first: a command that writes a file
                // and runs the tests, as agents' do, passes with it.
                let wrote =
                    shell_writes(&app, session, agent, read_root.as_deref(), before).await?;
                if command.contains("git commit") {
                    crate::handover::on_commit(&app, session, &cwd).await?;
                }
                // Lessons'.
                let given = lessons::on_ran(&app, session, agent, &root, command).await?;
                joined([wrote, given])
            }
            None => String::new(),
        },
        // The turn ends, or, for an agent that can't say so, the person
        // writes again: what the stretch taught, asked once (lessons'); work
        // it did, to show if it's done; and what it opened, to finish
        // reading.
        "end" | "prompt" => {
            if let Some(said) = m.prompt.as_deref().filter(|_| m.what == "prompt") {
                kept_message(&app, session, agent, &cwd, said).await?;
            }
            joined([
                lessons::at_turn_end(&app, session).await?,
                crate::handoff::done(&app, session).await?,
                crate::handoff::reading(&app, session).await?,
            ])
        }
        "failed" => {
            let command = m.command.clone().unwrap_or_default();
            let wrote = shell_writes(&app, session, agent, read_root.as_deref(), before).await?;
            // Lessons'.
            let failed = lessons::on_failed(
                &app,
                session,
                agent,
                &root,
                &command,
                m.problem.as_deref().unwrap_or_default(),
            )
            .await?;
            joined([wrote, failed])
        }
        // About to call its model: what was kept for it.
        "next" => {
            let mut conn = app.db.acquire().await?;
            kelve_store::lessons::take_held(&mut conn, session)
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
        kelve_store::lessons::hold_for(&mut conn, session, &text).await?;
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

/// Where a file is, as kelve knows it: in a folder it read, or a new one in
/// the folder the agent works in.
async fn place(
    app: &Shared,
    file: &str,
    in_root: Option<&str>,
) -> ApiResult<Option<(String, String)>> {
    let read = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::read_at(&mut conn, file).await?
    };
    Ok(read
        .map(|(root, path, _)| (root, path))
        .or_else(|| Some((in_root?.to_string(), new_in(in_root?, file)?))))
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
    if let Some((root, path)) = place(app, file, in_root).await? {
        let mut conn = app.db.acquire().await?;
        kelve_store::lessons::touch(&mut conn, session, &root, &path, edit).await?;
    }
    Ok(())
}

/// Folders whose walk for shell writes was cut short, logged once each.
static CUT_SHORT: LazyLock<Mutex<std::collections::HashSet<String>>> =
    LazyLock::new(Default::default);

/// What the agent wrote through its shell since its last moment, however it
/// wrote it: each file git sees changed, or, in a folder that isn't a
/// repository, each file kelve walks, written since. Each is a file the
/// agent wrote over whole, as an edit is, unless only its whitespace
/// changed, as a formatter changes it. The person writing in the same
/// moment can't be told apart.
async fn shell_writes(
    app: &Shared,
    session: &str,
    agent: &str,
    root: Option<&str>,
    since: Option<i64>,
) -> ApiResult<String> {
    // Only in a folder kelve has read: an agent working anywhere else costs
    // no walk.
    let (Some(root), Some(since)) = (root, since) else {
        return Ok(String::new());
    };
    let dir = std::path::PathBuf::from(root);
    let found = tokio::task::spawn_blocking(move || kelve_engine::git::written_since(&dir, since))
        .await
        .map_err(ApiError::internal)?;
    let found = match found {
        Ok(found) => found,
        Err(e) => {
            tracing::debug!(error = %e, "couldn't look for what the shell wrote");
            return Ok(String::new());
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
    let mut said = Vec::new();
    for file in found.files {
        let file = file.display().to_string();
        // Files kelve has read, and new ones the agent made in the folder;
        // never kelve's own, as what it keeps of what it read.
        let Some((at, path)) = place(app, &file, Some(root)).await? else {
            continue;
        };
        if crate::reader::is_kelves(&path) {
            continue;
        }
        let Ok(now) = kelve_engine::read::read(Path::new(&at), &path) else {
            continue;
        };
        // Lessons'.
        if lessons::reformatted(app, session, &at, &path, &now.text).await? {
            continue;
        }
        touch(app, session, &file, true, Some(root)).await?;
        // Lessons'.
        let text = lessons::on_file(
            app,
            session,
            agent,
            &file,
            true,
            Some(&now.text),
            None,
            Some(root),
        )
        .await?;
        if !text.is_empty() {
            said.push(text);
        }
    }
    Ok(said.join("\n"))
}
