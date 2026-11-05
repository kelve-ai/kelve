//! What the person changed after their agent handed over, however they
//! committed it: the agent's own commits are seen by its hook as they're
//! made (`handover::on_commit`); commits the person makes in their own
//! terminal, or anywhere else, are seen lazily, the next time kelve hears
//! from the agent or the person, by comparing what's checked out with the
//! latest handover. What they changed is kept as evidence and learned from,
//! and the commit is what they kept of the work, for checks.

use std::path::{Path, PathBuf};

use kelve_engine::git::Which;
use kelve_store::candidates::NewEvidence;
use kelve_store::{agent_loop as store, candidates, handovers};

use crate::error::{ApiError, ApiResult};
use crate::handover::{Edited, as_diff, edited_after};
use crate::state::Shared;

/// How long after a handover a commit is compared with it.
const FOR: i64 = 7 * 24 * 60 * 60 * 1000;
/// Looks for commits made since the latest handover in the repository `dir`
/// is in, the agent's or the person's, and compares them with what was
/// handed over, once each: how many files the person changed. Cheap when
/// nothing was committed: a look at what's checked out, and one query.
pub async fn look(app: &Shared, dir: &str) -> ApiResult<usize> {
    let at = PathBuf::from(dir);
    let Some(root) = tokio::task::spawn_blocking(move || kelve_engine::git::root_of(&at))
        .await
        .map_err(ApiError::internal)?
    else {
        return Ok(0);
    };
    let root_s = root.display().to_string();
    let h = {
        let mut conn = app.db.acquire().await?;
        handovers::latest_in(&mut conn, &root_s, kelve_store::now() - FOR).await?
    };
    let Some(h) = h.filter(|h| !h.head.is_empty()) else {
        return Ok(0);
    };
    let dir = root.clone();
    let head = tokio::task::spawn_blocking(move || kelve_engine::git::head(&dir, "HEAD"))
        .await
        .map_err(ApiError::internal)?
        .map(|(id, _)| id)
        .unwrap_or_default();
    let compared = {
        let mut conn = app.db.acquire().await?;
        store::compared_of(&mut conn, h.id).await?
    };
    if head.is_empty() || head == h.head || head == compared {
        return Ok(0);
    }
    compare(app, &h, &root, &head).await
}

/// Compares what's committed now with a handover: the files the person
/// changed, each kept as evidence, and learned from.
async fn compare(
    app: &Shared,
    h: &handovers::Handover,
    root: &Path,
    head: &str,
) -> ApiResult<usize> {
    let (handed, by_agent) = {
        let mut conn = app.db.acquire().await?;
        let handed = handovers::files(&mut conn, h.id).await?;
        let mine = if h.session.is_empty() {
            Vec::new()
        } else {
            handovers::edited_since(&mut conn, &h.session, h.at).await?
        };
        (handed, mine)
    };
    let dir = root.to_path_buf();
    let since = h.head.clone();
    let committed = tokio::task::spawn_blocking(move || {
        kelve_engine::git::change_stamped(&dir, &Which::Against(since), &crate::reader::is_kelves)
    })
    .await
    .map_err(ApiError::internal)?;
    let committed = match committed {
        Ok((c, _)) => c,
        Err(e) => {
            tracing::debug!(error = %e, "couldn't read what was committed since a handover");
            return Ok(0);
        }
    };
    let edits: Vec<Edited> = edited_after(&handed, &committed)
        .into_iter()
        .filter(|e| {
            !by_agent
                .iter()
                .any(|(r, p)| same_file(&Path::new(r).join(p), &root.join(&e.path)))
        })
        .collect();
    let mut ids = Vec::with_capacity(edits.len());
    {
        let mut conn = app.db.acquire().await?;
        for e in &edits {
            ids.push(
                candidates::add_evidence(
                    &mut conn,
                    &NewEvidence {
                        kind: "edit",
                        session: &h.session,
                        root: &h.root,
                        handover: Some(h.id),
                        path: &e.path,
                        text: &as_diff(e),
                        ..Default::default()
                    },
                )
                .await?,
            );
        }
    }
    compared(app, h.id, head, &ids).await?;
    Ok(edits.len())
}

/// A commit was compared with a handover: it's what the person kept of the
/// work, the same commit isn't compared again, and what they changed, as
/// evidence, is learned from.
pub async fn compared(app: &Shared, handover: i64, head: &str, edits: &[i64]) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    store::compared_head(&mut conn, handover, head).await?;
    store::kept_at(&mut conn, handover, head).await?;
    for id in edits {
        store::unlearn(&mut conn, *id).await?;
    }
    drop(conn);
    if !edits.is_empty() {
        super::learn::from_edits(app, handover).await?;
    }
    Ok(())
}

/// Whether two paths are one file, however each is named.
fn same_file(a: &Path, b: &Path) -> bool {
    let a = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
    let b = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
    a == b
}
