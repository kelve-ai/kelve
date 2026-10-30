//! Work shown before it's merged: when the person's agent is about to merge a
//! branch, and kelve hasn't shown the person that work, its hook says to show
//! it in kelve first, once for each commit. Merging again goes ahead: the
//! person may not want to see it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use kelve_core::hook::{Merge, merging};

use crate::error::ApiResult;
use crate::state::Shared;

/// How many things opened lately are looked through for the work.
const LATELY: i64 = 50;

/// The commits the agent has been told to show before merging, since kelve
/// started: it's told once.
static TOLD: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// The folder a merge runs in: where the agent works, or where the command
/// moves to first.
fn folder(cwd: &Path, merge: &Merge) -> PathBuf {
    match merge.dir.as_deref() {
        Some(d) if d == "~" || d.starts_with("~/") => std::env::home_dir()
            .map(|h| h.join(d.trim_start_matches('~').trim_start_matches('/')))
            .unwrap_or_else(|| cwd.join(d)),
        Some(d) => cwd.join(d),
        None => cwd.to_path_buf(),
    }
}

/// What kelve says before the agent runs `command` in `cwd`: to show the
/// work in kelve first, when it's a merge of a commit kelve hasn't shown
/// since it was made, and the agent hasn't been told already; else nothing.
pub async fn before(app: &Shared, cwd: &str, command: &str) -> ApiResult<String> {
    let Some(merge) = merging(command) else {
        return Ok(String::new());
    };
    let at = folder(Path::new(cwd), &merge);
    let spec = merge.branch.clone().unwrap_or_else(|| "HEAD".into());
    let found = {
        let (at, spec) = (at.clone(), spec.clone());
        tokio::task::spawn_blocking(move || kelve_engine::git::head(&at, &spec))
            .await
            .ok()
            .flatten()
    };
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
    let what = merge
        .branch
        .map(|b| format!("`{b}`"))
        .unwrap_or_else(|| "this branch".into());
    Ok(format!(
        "Before you merge {what}, show the person this work in kelve: use kelve's open tool once, with what: worktree from the worktree it was done in, or what: change with the branch it merges into as base. Then merge. Merging again without showing it goes ahead."
    ))
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
