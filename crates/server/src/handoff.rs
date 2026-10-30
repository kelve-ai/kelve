//! Work shown as it's handed over: when the person's agent commits, pushes or
//! opens a pull request for work kelve hasn't shown the person, its hook says
//! to show it in kelve, once for each commit. The person reviews work before
//! it's merged, so the agent shows it as it hands it over, not as it merges.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use kelve_core::hook::{Handover, handing_over};

use crate::error::ApiResult;
use crate::state::Shared;

/// How many things opened lately are looked through for the work.
const LATELY: i64 = 50;

/// The commits the agent has been told to show, since kelve started: it's
/// told once.
static TOLD: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// The folder a command hands work over in: where the agent works, or where
/// the command moves to first.
fn folder(cwd: &Path, handover: &Handover) -> PathBuf {
    match handover.dir.as_deref() {
        Some(d) if d == "~" || d.starts_with("~/") => std::env::home_dir()
            .map(|h| h.join(d.trim_start_matches('~').trim_start_matches('/')))
            .unwrap_or_else(|| cwd.join(d)),
        Some(d) => cwd.join(d),
        None => cwd.to_path_buf(),
    }
}

/// What kelve says after the agent ran `command` in `cwd`: to show the work
/// in kelve, when the command handed it over and kelve hasn't shown its
/// commit since it was made, and the agent hasn't been told already; else
/// nothing.
pub async fn after(app: &Shared, cwd: &str, command: &str) -> ApiResult<String> {
    let Some(handover) = handing_over(command) else {
        return Ok(String::new());
    };
    let at = folder(Path::new(cwd), &handover);
    let found = tokio::task::spawn_blocking(move || kelve_engine::git::head(&at, "HEAD"))
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
    Ok("You've handed this work over: show the person it in kelve now, with kelve's open tool, once, and a line on what you changed and why. Only what's new since you last showed them: what: worktree from a worktree, or what: change with the branch it'll merge into as base. Then carry on.".to_string())
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
