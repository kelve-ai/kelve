//! Work shown as it's done: as the person's agent finishes its turn, having
//! edited files kelve hasn't shown the person since, its hook says to show
//! that work in kelve, if it's done and worth showing, which the agent
//! decides; and when it commits, pushes or opens a pull request for work
//! kelve hasn't shown, to show it then, once for each commit. Work shown
//! since it was last edited is shown: a commit changes nothing in it. The
//! agent shows its work as its branch since its base, committed or not, so
//! showing it again brings the same thing up to date, never another.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use kelve_core::hook::Handover;

use crate::error::ApiResult;
use crate::state::Shared;

/// How many things opened lately are looked through for the work.
const LATELY: i64 = 50;

/// The commits the agent has been told to show, since kelve started: it's
/// told once.
static TOLD: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// The edit each session was last told, as a turn ended, to show the work
/// of: it's told once for each.
static TOLD_DONE: LazyLock<Mutex<HashMap<String, i64>>> = LazyLock::new(Default::default);

/// A folder as the hook gives it, with a `~` for the home folder put right.
pub fn at_home(path: &str) -> PathBuf {
    match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => std::env::home_dir().map_or_else(
            || PathBuf::from(path),
            |h| h.join(rest.trim_start_matches('/')),
        ),
        _ => PathBuf::from(path),
    }
}

/// How the agent is told to show its work: the same thing each time, as
/// it is now.
const SHOW: &str = "with kelve's open tool, once, and a line on what you changed and why: what: worktree, with the branch it'll merge into as base, whether you're in a worktree or not and whether it's committed or not. It's the same thing in kelve each time you show it, brought up to date.";

/// The same, for work in a folder that isn't in a git repository: its
/// files as they are now, since there's nothing to show them against.
const SHOW_FILES: &str = "with kelve's open tool, once, and a line on what you changed and why: what: files, with every file you've changed in this work, since the folder isn't in a git repository. It's the same thing in kelve each time you show it, brought up to date.";

/// What kelve says after the agent handed work over: to show it in kelve,
/// when kelve hasn't shown its commit, the branch named or else the one
/// checked out, since it was made, nor anything there since `session` last
/// edited it, and the agent hasn't been told already; else nothing.
pub async fn after(app: &Shared, session: &str, handover: &Handover) -> ApiResult<String> {
    let Some(at) = handover.dir.as_deref().map(at_home) else {
        return Ok(String::new());
    };
    // Shown since the agent last edited it there: committing it, pushing it
    // or asking for it to be merged changes nothing the person hasn't seen.
    let edit = {
        let mut conn = app.db.acquire().await?;
        kelve_store::lessons::last_edit(&mut conn, session).await?
    };
    if let Some((root, edited)) = edit
        && overlap(Path::new(&root), &at)
        && opened_since(app, &root, edited).await?
    {
        return Ok(String::new());
    }
    let spec = handover.branch.clone().unwrap_or_else(|| "HEAD".into());
    let found = tokio::task::spawn_blocking(move || kelve_engine::git::head(&at, &spec))
        .await
        .ok()
        .flatten();
    let Some((commit, made)) = found else {
        return Ok(String::new());
    };
    if shown(app, &commit, made).await? {
        return Ok(String::new());
    }
    if !TOLD
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(commit)
    {
        return Ok(String::new());
    }
    Ok(format!(
        "You've handed this work over: show the person it in kelve now, {SHOW} Then carry on."
    ))
}

/// What kelve says as the agent's turn ends, or, for an agent that can't say
/// so, as the person writes again: when it edited files and nothing has been
/// opened in kelve in that folder since, to show the work if it's done and
/// the person would want to see it, once for its last edit; else nothing.
pub async fn done(app: &Shared, session: &str) -> ApiResult<String> {
    let edit = {
        let mut conn = app.db.acquire().await?;
        kelve_store::lessons::last_edit(&mut conn, session).await?
    };
    let Some((root, edited)) = edit else {
        return Ok(String::new());
    };
    if opened_since(app, &root, edited).await? {
        return Ok(String::new());
    }
    if TOLD_DONE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(session.to_string(), edited)
        == Some(edited)
    {
        return Ok(String::new());
    }
    let at = PathBuf::from(&root);
    let show = match tokio::task::spawn_blocking(move || kelve_engine::git::root_of(&at)).await {
        Ok(Some(_)) => SHOW,
        _ => SHOW_FILES,
    };
    Ok(format!(
        "You've changed files and haven't shown them in kelve since. If you've finished what you were doing and the person would want to see it, show them it in kelve now, {show} If you haven't finished, or it isn't worth showing, carry on."
    ))
}

/// Whether anything was opened in kelve in `root`, or a folder around it,
/// since `at`.
async fn opened_since(app: &Shared, root: &str, at: i64) -> ApiResult<bool> {
    let lately = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::lately(&mut conn, LATELY).await?
    };
    let root = Path::new(root);
    Ok(lately
        .iter()
        .any(|o| o.opened_at >= at && overlap(Path::new(&o.root), root)))
}

/// Whether two folders are one, or one is in the other, however each is
/// named.
fn overlap(a: &Path, b: &Path) -> bool {
    let (a, b) = (
        std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf()),
        std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf()),
    );
    a.starts_with(&b) || b.starts_with(&a)
}

/// Whether kelve showed the work at `commit` since it was made: something
/// opened as a change or a worktree, whose head was that commit.
async fn shown(app: &Shared, commit: &str, made: i64) -> ApiResult<bool> {
    let lately = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::lately(&mut conn, LATELY).await?
    };
    let roots: Vec<PathBuf> = lately
        .into_iter()
        .filter(|o| o.kind == "change" && o.opened_at >= made)
        .map(|o| PathBuf::from(o.root))
        .collect();
    let commit = commit.to_string();
    Ok(tokio::task::spawn_blocking(move || {
        roots
            .iter()
            .any(|r| kelve_engine::git::head(r, "HEAD").is_some_and(|(id, _)| id == commit))
    })
    .await
    .unwrap_or(false))
}
