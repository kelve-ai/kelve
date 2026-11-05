//! What the person's agent handed over, and what the person changed in it
//! after (docs/skills.md). As the agent shows its work in kelve, as a
//! change or a worktree, kelve keeps the change as it read it then: where,
//! which commit, whether some wasn't committed, and each file with its
//! hunks and its text, secrets masked. When a commit is made there later,
//! as the agent's hook sees it, kelve compares each file the commit holds
//! with the same file as it was handed over: the lines that differ are what
//! the person changed in the agent's work, kept as evidence for their
//! skills. Nothing here leaves.

use std::path::{Path, PathBuf};

use kelve_engine::git::{Change, Hunk, LineKind, Status, Which};
use kelve_store::handovers::{self, HandedFile, NewHandover};

use crate::error::{ApiError, ApiResult};
use crate::reader::{self, Open};
use crate::state::Shared;

/// The most of one file's text kept with a handover, in bytes: past it, its
/// hunks are kept, but it can't be compared.
pub const FILE_TEXT: usize = 256 * 1024;
/// The most text kept with one handover, in bytes, files and hunks
/// together: past it, the rest of its files are listed without them.
pub const ALL_TEXT: usize = 4 * 1024 * 1024;
/// How long after a handover a commit is compared with it.
const COMPARED_FOR: i64 = 7 * 24 * 60 * 60 * 1000;

/// A handover's files as kept, from a change as kelve read it: each file's
/// status, hunks as JSON and text, while there's room (`FILE_TEXT`,
/// `ALL_TEXT`). Whether anything was left out.
pub fn files_of(change: &Change) -> (Vec<HandedFile>, bool) {
    let mut room = ALL_TEXT;
    let mut cut = change.more;
    let files = change
        .files
        .iter()
        .map(|f| {
            let status = match f.status {
                Status::Added => "added",
                Status::Modified => "modified",
                Status::Deleted => "deleted",
            };
            let skip = f.skip.and_then(|s| {
                serde_json::to_value(s)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
            });
            let hunks = serde_json::to_string(&f.hunks).unwrap_or_else(|_| "[]".into());
            let hunks = if hunks.len() <= room {
                room -= hunks.len();
                hunks
            } else {
                cut = true;
                "[]".into()
            };
            let text = match f.after.as_deref() {
                Some(t) if t.len() <= FILE_TEXT && t.len() <= room => {
                    room -= t.len();
                    Some(t.to_string())
                }
                Some(_) => {
                    cut = true;
                    None
                }
                None => None,
            };
            HandedFile {
                path: f.path.clone(),
                status: status.into(),
                moved_from: f.moved_from.clone(),
                skip,
                added: i64::from(f.added),
                removed: i64::from(f.removed),
                hunks,
                text,
            }
        })
        .collect();
    (files, cut)
}

/// The branch a change was read on, from its title: `fix since main`,
/// `fix against main`. '' when it doesn't say.
fn branch_in(title: &str, which: &Which) -> String {
    let joint = match which {
        Which::Since(base) => format!(" since {base}"),
        Which::Against(base) => format!(" against {base}"),
        Which::Uncommitted | Which::Commit(_) => return String::new(),
    };
    title
        .strip_suffix(&joint)
        .filter(|b| !b.contains(' '))
        .unwrap_or_default()
        .to_string()
}

/// Keeps what the agent handed over as it showed `open` in kelve, when it's
/// a change or a worktree: its id, or none for anything else, or a change
/// kelve couldn't read.
pub async fn record(
    app: &Shared,
    open: &Open,
    agent: &str,
    title: &str,
    said: &str,
) -> ApiResult<Option<i64>> {
    let Some(which) = open.which() else {
        return Ok(None);
    };
    let change = match reader::change(open) {
        Ok(change) => change,
        Err(e) => {
            tracing::debug!(error = %e.message, "couldn't read a change handed over");
            return Ok(None);
        }
    };
    let root = open.root.clone();
    let looked = {
        let which = which.clone();
        tokio::task::spawn_blocking(move || {
            let head = kelve_engine::git::head(&root, "HEAD").map(|(id, _)| id);
            let uncommitted = match which {
                Which::Commit(_) | Which::Against(_) => false,
                Which::Uncommitted | Which::Since(_) => kelve_engine::git::change_stamped(
                    &root,
                    &Which::Uncommitted,
                    &reader::is_kelves,
                )
                .is_ok_and(|(c, _)| !c.files.is_empty()),
            };
            (head, uncommitted)
        })
        .await
        .map_err(ApiError::internal)?
    };
    let (head, uncommitted) = looked;
    let base = match &which {
        Which::Since(b) | Which::Against(b) => b.clone(),
        Which::Uncommitted | Which::Commit(_) => String::new(),
    };
    let (files, cut) = files_of(&change);
    let root = open.root.display().to_string();
    let branch = branch_in(&change.title, &which);
    let mut conn = app.db.acquire().await?;
    let id = handovers::add(
        &mut conn,
        &NewHandover {
            opened: Some(open.row.id),
            root: &root,
            branch: &branch,
            base: &base,
            change: &open.row.change,
            head: head.as_deref().unwrap_or_default(),
            uncommitted,
            agent,
            session: "",
            title,
            said,
            cut,
        },
        &files,
    )
    .await?;
    Ok(Some(id))
}

/// The hook saw the agent open something in kelve, in its session: what it
/// handed over then is that session's.
pub async fn of_session(app: &Shared, opened: i64, session: &str) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    handovers::of_session(&mut conn, opened, session).await?;
    Ok(())
}

/// One file the person changed after it was handed over: the lines that
/// differ between the version handed over and the one committed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edited {
    pub path: String,
    pub hunks: Vec<Hunk>,
    pub added: u32,
    pub removed: u32,
}

/// What the person changed after a handover, file by file: for each file
/// a commit holds that was handed over, the lines that differ between it as
/// handed over and as committed. A file handed over without its text
/// (too big, a secret, not text) can't be compared, and one the commit
/// leaves as it was handed over changed nothing.
pub fn edited_after(handed: &[HandedFile], committed: &Change) -> Vec<Edited> {
    let mut out = Vec::new();
    for c in &committed.files {
        if c.skip.is_some() {
            continue;
        }
        let Some(h) = handed.iter().find(|h| h.path == c.path) else {
            continue;
        };
        let was = match (h.status.as_str(), h.text.as_deref()) {
            ("deleted", _) => "",
            (_, Some(text)) => text,
            (_, None) => continue,
        };
        let now = match (c.status, c.after.as_deref()) {
            (Status::Deleted, _) => "",
            (_, Some(text)) => text,
            (_, None) => continue,
        };
        if was == now {
            continue;
        }
        let (hunks, added, removed) = kelve_engine::git::hunks(was, now);
        if hunks.is_empty() {
            continue;
        }
        out.push(Edited {
            path: c.path.clone(),
            hunks,
            added,
            removed,
        });
    }
    out
}

/// An edit as it's kept for the person's skills: its hunks as a diff, each
/// headed by where it is in the file as committed.
pub fn as_diff(e: &Edited) -> String {
    let mut out = String::new();
    for h in &e.hunks {
        let at = h.lines.iter().find_map(|l| l.new.or(l.old)).unwrap_or(1);
        out.push_str(&format!("@@ line {at}\n"));
        for l in &h.lines {
            let mark = match l.kind {
                LineKind::Context => ' ',
                LineKind::Added => '+',
                LineKind::Removed => '-',
            };
            out.push(mark);
            out.push_str(&l.text);
            out.push('\n');
        }
    }
    out
}

/// The agent committed in `cwd` (`moment`): the commit it made is compared
/// with the latest handover there, in the last week, and what the person
/// changed in the work handed over is kept as evidence, a piece for each
/// file. Files the agent edited itself since the handover are its own
/// work, not the person's, and are left out. How many files were kept.
pub async fn on_commit(app: &Shared, session: &str, cwd: &str) -> ApiResult<usize> {
    let at = PathBuf::from(cwd);
    let Some(root) = tokio::task::spawn_blocking(move || kelve_engine::git::root_of(&at))
        .await
        .map_err(ApiError::internal)?
    else {
        return Ok(0);
    };
    let root_s = root.display().to_string();
    let (handover, handed, edited_by_agent) = {
        let mut conn = app.db.acquire().await?;
        let Some(h) =
            handovers::latest_in(&mut conn, &root_s, kelve_store::now() - COMPARED_FOR).await?
        else {
            return Ok(0);
        };
        let handed = handovers::files(&mut conn, h.id).await?;
        let mine = handovers::edited_since(&mut conn, session, h.at).await?;
        (h, handed, mine)
    };
    let dir = root.clone();
    let committed = tokio::task::spawn_blocking(move || {
        kelve_engine::git::change_stamped(&dir, &Which::Commit("HEAD".into()), &reader::is_kelves)
    })
    .await
    .map_err(ApiError::internal)?;
    let committed = match committed {
        Ok((c, _)) => c,
        Err(e) => {
            tracing::debug!(error = %e, "couldn't read the commit to compare with a handover");
            return Ok(0);
        }
    };
    let by_agent = |path: &str| {
        edited_by_agent
            .iter()
            .any(|(r, p)| same_file(Path::new(r).join(p), root.join(path)))
    };
    let edits: Vec<Edited> = edited_after(&handed, &committed)
        .into_iter()
        .filter(|e| !by_agent(&e.path))
        .collect();
    let mut conn = app.db.acquire().await?;
    for e in &edits {
        kelve_store::candidates::add_evidence(
            &mut conn,
            &kelve_store::candidates::NewEvidence {
                kind: "edit",
                session: &handover.session,
                root: &root_s,
                handover: Some(handover.id),
                path: &e.path,
                text: &as_diff(e),
                ..Default::default()
            },
        )
        .await?;
    }
    handovers::compared(&mut conn, handover.id).await?;
    Ok(edits.len())
}

/// Whether two paths are one file, however each is named.
fn same_file(a: PathBuf, b: PathBuf) -> bool {
    let a = std::fs::canonicalize(&a).unwrap_or(a);
    let b = std::fs::canonicalize(&b).unwrap_or(b);
    a == b
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use kelve_engine::git::FileChange;

    fn handed(path: &str, status: &str, text: Option<&str>) -> HandedFile {
        HandedFile {
            path: path.into(),
            status: status.into(),
            moved_from: None,
            skip: None,
            added: 0,
            removed: 0,
            hunks: "[]".into(),
            text: text.map(str::to_string),
        }
    }

    fn committed(path: &str, status: Status, after: Option<&str>) -> FileChange {
        FileChange {
            path: path.into(),
            status,
            skip: None,
            hunks: Vec::new(),
            added: 0,
            removed: 0,
            after: after.map(str::to_string),
            before: None,
            moved_from: None,
        }
    }

    #[test]
    fn only_lines_changed_in_files_handed_over_count() {
        let handed = [
            handed("src/retry.rs", "modified", Some("a\nb\nc\n")),
            handed("src/same.rs", "added", Some("x\n")),
            handed("src/big.rs", "modified", None),
            handed("src/gone.rs", "deleted", None),
        ];
        let change = Change {
            title: "Commit".into(),
            more: false,
            files: vec![
                committed("src/retry.rs", Status::Modified, Some("a\nB\nc\n")),
                committed("src/same.rs", Status::Added, Some("x\n")),
                committed("src/big.rs", Status::Modified, Some("y\n")),
                committed("src/gone.rs", Status::Added, Some("back\n")),
                committed("src/other.rs", Status::Modified, Some("z\n")),
            ],
        };
        let edits = edited_after(&handed, &change);
        let paths: Vec<&str> = edits.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, vec!["src/retry.rs", "src/gone.rs"]);
        assert_eq!((edits[0].added, edits[0].removed), (1, 1));
        let diff = as_diff(&edits[0]);
        assert!(diff.starts_with("@@ line 1\n"), "{diff}");
        assert!(diff.contains("-b\n+B\n"), "{diff}");
        assert_eq!((edits[1].added, edits[1].removed), (1, 0));
    }

    #[test]
    fn a_handovers_text_is_kept_while_theres_room() {
        let big = "x".repeat(FILE_TEXT + 1);
        let change = Change {
            title: "fix since main".into(),
            more: false,
            files: vec![
                committed("a.rs", Status::Added, Some("fn a() {}\n")),
                committed("big.rs", Status::Added, Some(&big)),
            ],
        };
        let (files, cut) = files_of(&change);
        assert!(cut);
        assert_eq!(files[0].text.as_deref(), Some("fn a() {}\n"));
        assert_eq!(files[0].status, "added");
        assert_eq!(files[1].text, None);
        assert_eq!(
            branch_in("fix since main", &Which::Since("main".into())),
            "fix"
        );
        assert_eq!(branch_in("Uncommitted changes", &Which::Uncommitted), "");
    }
}
