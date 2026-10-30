//! What changed in a repo, read from git itself, never by running it: the
//! repo's own configuration can name programs git would run (hooks, filters,
//! pagers, diff drivers), and kelve runs nothing from what it reads. Opened
//! isolated, so nothing from the environment or the person's global git
//! setup is read either.
//!
//! The four kinds of change:
//! - what isn't committed yet, new files included, as an agent leaves them;
//! - a branch, against where it left another, as a pull request shows it;
//! - one commit, against its first parent;
//! - a pull request, read from its code host (`crate::pulls`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::objs::tree::EntryKind;
use serde::Serialize;
use similar::{ChangeTag, TextDiff};

use crate::read::text_of;
use crate::walk::{MAX_BYTES, Skip, walk_with_folders};

/// The most files one change lists.
pub const MAX_FILES: usize = 500;

/// Lines of context around each change, as git shows them.
const CONTEXT: usize = 3;

/// Which change to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Which {
    /// What isn't committed yet, against the last commit.
    Uncommitted,
    /// The checked-out branch, against where it left this one.
    Against(String),
    /// One commit, against its first parent.
    Commit(String),
    /// Everything since the checked-out branch left `base`, committed or
    /// not: what an agent working in a worktree has done so far.
    Since(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Added,
    Modified,
    Deleted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

/// One line of a hunk, with its number on each side it's on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Line {
    pub kind: LineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

/// A run of changed lines, with their context.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Hunk {
    pub lines: Vec<Line>,
}

/// One file's change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub path: String,
    pub status: Status,
    /// Why it isn't shown, when it isn't: a secret, a lockfile, not text.
    pub skip: Option<Skip>,
    pub hunks: Vec<Hunk>,
    pub added: u32,
    pub removed: u32,
    /// The file as it is after the change, secrets masked: what the changed
    /// lines are marked in. None when it's gone, or isn't read.
    #[serde(skip)]
    pub after: Option<String>,
    /// The file as it was before, secrets masked: for showing it whole,
    /// with what was taken out where it was. None when it's new.
    #[serde(skip)]
    pub before: Option<String>,
    /// Where it was, when the change moved it: a file taken out and one
    /// added that are mostly the same, as git finds renames. Its lines are
    /// then against the file it was.
    pub moved_from: Option<String>,
}

/// A change: what it is, and its files in path order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Change {
    /// What the change is, as people would say it: "Uncommitted changes",
    /// "first-version against main", "Fix the ledger (3f2a9c1)".
    pub title: String,
    pub files: Vec<FileChange>,
    /// Whether it had more than `MAX_FILES` files.
    pub more: bool,
}

/// Why a change couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    #[error("That folder isn’t in a git repository.")]
    NotARepo,
    #[error("kelve couldn’t find {0:?} in this repository. Check the name, or fetch it first.")]
    Unknown(String),
    #[error("{0:?} doesn’t share any history with what’s checked out.")]
    Unrelated(String),
    #[error("kelve couldn’t read this repository: {0}")]
    Read(String),
}

/// A commit, as a reason for something may be found in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Commit {
    /// Short, as git shows it: `3f2a9c1`.
    pub id: String,
    pub subject: String,
    pub body: String,
}

/// How far back `history` looks, on the first-parent line.
const LOOK_BACK: usize = 2_000;

/// The latest commits on the checked-out line that changed `rel`, a file or
/// a folder from `dir` ("" for all of it), newest first, at most `most`.
pub fn history(dir: &Path, rel: &str, most: usize) -> Result<Vec<Commit>, GitError> {
    let (repo, root) = open(dir)?;
    let Ok(head) = repo.head_commit() else {
        // No commits yet.
        return Ok(Vec::new());
    };
    // From the repository's root, as its trees name things.
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let under: Vec<String> = dir
        .strip_prefix(&root)
        .map(|p| {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let path = under
        .into_iter()
        .chain(rel.split('/').filter(|p| !p.is_empty()).map(String::from))
        .collect::<Vec<_>>()
        .join("/");
    let at = |id: ObjectId| -> Option<ObjectId> {
        let tree = repo.find_commit(id).ok()?.tree().ok()?;
        if path.is_empty() {
            return Some(tree.id);
        }
        tree.lookup_entry_by_path(&path)
            .ok()
            .flatten()
            .map(|e| e.object_id())
    };
    let walk = head
        .ancestors()
        .first_parent_only()
        .all()
        .map_err(read_err)?;
    let mut found = Vec::new();
    for info in walk.take(LOOK_BACK) {
        let Ok(info) = info else { break };
        let now = at(info.id);
        let before = info.parent_ids.first().and_then(|p| at(*p));
        if now == before {
            continue;
        }
        let Ok(commit) = repo.find_commit(info.id) else {
            continue;
        };
        let message = commit.message_raw_sloppy().to_str_lossy().into_owned();
        let (subject, body) = message.split_once('\n').unwrap_or((&message, ""));
        found.push(Commit {
            id: short(info.id),
            subject: subject.trim().to_string(),
            body: body.trim().to_string(),
        });
        if found.len() == most {
            break;
        }
    }
    Ok(found)
}

/// The most files `written_since` walks: past it, what's further on in
/// the walk isn't looked at.
pub const WRITTEN_WALK: usize = 20_000;

/// Files written in a folder since a moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Written {
    /// Absolute paths, in path order.
    pub files: Vec<PathBuf>,
    /// Whether the walk stopped at `WRITTEN_WALK` before the end.
    pub more: bool,
}

/// The files in `dir` written after `since`, in milliseconds since the
/// epoch: what an agent wrote through its shell, however it wrote it. In a
/// git repository, only those that aren't as last committed; in a folder
/// that isn't one, every file written since. Ignored files are left out, as
/// kelve's walk leaves them, and files kelve won't read by their name
/// (lockfiles, vendored or generated ones, secrets). Only files written
/// since are read, and the walk stops at `WRITTEN_WALK`, so it stays fast.
pub fn written_since(dir: &Path, since: i64) -> Result<Written, GitError> {
    let repo = open(dir).ok();
    // Where `dir` is in the repository, as its trees name things.
    let under = repo
        .as_ref()
        .and_then(|(_, root)| {
            let dir = std::fs::canonicalize(dir).ok()?;
            let root = std::fs::canonicalize(root).ok()?;
            dir.strip_prefix(&root).ok().map(|p| {
                p.components()
                    .map(|c| format!("{}/", c.as_os_str().to_string_lossy()))
                    .collect::<String>()
            })
        })
        .unwrap_or_default();
    let tree = repo
        .as_ref()
        .and_then(|(r, _)| r.head_commit().ok())
        .and_then(|c| c.tree().ok());
    // `dir` itself, so paths come back as the caller named it.
    let walked = crate::walk::walk_at_most(dir, WRITTEN_WALK).map_err(read_err)?;
    let mut out = Written {
        files: Vec::new(),
        more: walked.more,
    };
    for file in walked.files {
        if file.skip.is_some() || file.bytes > MAX_BYTES {
            continue;
        }
        let at = dir.join(&file.rel);
        let written = std::fs::metadata(&at)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64);
        if !written.is_some_and(|w| w > since) {
            continue;
        }
        if let Some((repo, _)) = &repo {
            let committed = tree.as_ref().and_then(|t| {
                t.lookup_entry_by_path(format!("{under}{}", file.rel))
                    .ok()
                    .flatten()
                    .map(|e| e.object_id())
            });
            let now = std::fs::read(&at).ok().and_then(|bytes| {
                gix::objs::compute_hash(repo.object_hash(), gix::objs::Kind::Blob, &bytes).ok()
            });
            if now.is_none() || now == committed {
                continue;
            }
        }
        out.files.push(at);
    }
    Ok(out)
}

/// The id git gives a file's contents as a blob, in hex: the same text has
/// the same id in every repository.
pub fn blob_id(bytes: &[u8]) -> Option<String> {
    gix::objs::compute_hash(gix::hash::Kind::Sha1, gix::objs::Kind::Blob, bytes)
        .ok()
        .map(|id| id.to_hex().to_string())
}

fn read_err(e: impl std::fmt::Display) -> GitError {
    GitError::Read(e.to_string())
}

/// The repository a folder is in: its root, where `.git` is.
pub fn root_of(dir: &Path) -> Option<PathBuf> {
    let dir = std::fs::canonicalize(dir).ok()?;
    dir.ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
}

fn open(dir: &Path) -> Result<(gix::Repository, PathBuf), GitError> {
    let root = root_of(dir).ok_or(GitError::NotARepo)?;
    let repo =
        gix::open_opts(&root, gix::open::Options::isolated()).map_err(|_| GitError::NotARepo)?;
    Ok((repo, root))
}

/// Files in a tree, by path: the blob each is, as git records it.
fn files_in(
    repo: &gix::Repository,
    tree: Option<ObjectId>,
) -> Result<BTreeMap<String, ObjectId>, GitError> {
    let Some(tree) = tree else {
        return Ok(BTreeMap::new());
    };
    let tree = repo.find_tree(tree).map_err(read_err)?;
    let mut recorder = gix::traverse::tree::Recorder::default();
    tree.traverse()
        .breadthfirst(&mut recorder)
        .map_err(read_err)?;
    Ok(recorder
        .records
        .into_iter()
        .filter(|e| matches!(e.mode.kind(), EntryKind::Blob | EntryKind::BlobExecutable))
        .map(|e| (e.filepath.to_str_lossy().into_owned(), e.oid))
        .collect())
}

fn commit<'r>(repo: &'r gix::Repository, spec: &str) -> Result<gix::Commit<'r>, GitError> {
    repo.rev_parse_single(spec)
        .map_err(|_| GitError::Unknown(spec.to_string()))?
        .object()
        .map_err(read_err)?
        .peel_to_commit()
        .map_err(|_| GitError::Unknown(spec.to_string()))
}

/// The commit a name points at, `HEAD` or a branch, from a folder in a
/// repository: its id, and when it was made, in milliseconds since 1970.
pub fn head(dir: &Path, spec: &str) -> Option<(String, i64)> {
    let (repo, _) = open(dir).ok()?;
    let c = commit(&repo, spec).ok()?;
    let at = c.time().ok()?.seconds.saturating_mul(1000);
    Some((c.id.to_string(), at))
}

fn short(id: ObjectId) -> String {
    id.to_hex_with_len(7).to_string()
}

fn blob(repo: &gix::Repository, id: ObjectId) -> Result<Vec<u8>, GitError> {
    Ok(repo.find_object(id).map_err(read_err)?.data.clone())
}

/// Where the checked-out branch left `base`.
fn merge_base(repo: &gix::Repository, head: ObjectId, base: &str) -> Result<ObjectId, GitError> {
    let base_commit = commit(repo, base)?;
    Ok(repo
        .merge_base(head, base_commit.id)
        .map_err(read_err)?
        .ok_or_else(|| GitError::Unrelated(base.to_string()))?
        .detach())
}

/// The checked-out branch's name, or the commit's when there's no branch.
fn branch_of(repo: &gix::Repository, head: ObjectId) -> String {
    repo.head_name()
        .ok()
        .flatten()
        .map(|n| n.shorten().to_str_lossy().into_owned())
        .unwrap_or_else(|| short(head))
}

/// Reads a change in the repo `dir` is in.
pub fn change(dir: &Path, which: &Which) -> Result<Change, GitError> {
    let (repo, root) = open(dir)?;
    read_change(&repo, &root, which, &|_| false, None)
}

/// How a file or a folder stood on disk: what writing to it changes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stat {
    modified: Option<SystemTime>,
    len: u64,
    /// Which file it is, so one moved over it isn't taken for it.
    #[cfg(unix)]
    ino: u64,
}

/// How a path stands now, never following a link: none when it isn't there.
fn stat(path: &Path) -> Option<Stat> {
    let m = std::fs::symlink_metadata(path).ok()?;
    Some(Stat {
        modified: m.modified().ok(),
        len: m.len(),
        #[cfg(unix)]
        ino: std::os::unix::fs::MetadataExt::ino(&m),
    })
}

/// How close to reading a write can be and still not be told from one
/// just after it: some disks keep times to the second, or two.
const RACY: Duration = Duration::from_secs(2);

/// The commits a change is between, as they were when it was read.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Commits {
    head: Option<ObjectId>,
    /// The branch checked out, as titles name it.
    branch: Option<String>,
    /// What it's read against, or the one commit it is.
    base: Option<ObjectId>,
}

fn commits(repo: &gix::Repository, which: &Which) -> Result<Commits, GitError> {
    let base = match which {
        Which::Uncommitted => None,
        Which::Against(spec) | Which::Since(spec) | Which::Commit(spec) => {
            Some(commit(repo, spec)?.id)
        }
    };
    Ok(Commits {
        head: repo.head_id().ok().map(|id| id.detach()),
        branch: repo
            .head_name()
            .ok()
            .flatten()
            .map(|n| n.as_bstr().to_str_lossy().into_owned()),
        base,
    })
}

/// What a change was read from: to tell, far more cheaply than reading it
/// again, whether it would read the same. The commits it's between and,
/// for work not yet committed, every file and folder it was read from, as
/// each stood on disk. A file written, added or taken out, or a commit
/// made, and it no longer holds.
#[derive(Clone, Debug)]
pub struct Stamp {
    dir: PathBuf,
    which: Which,
    commits: Commits,
    disk: Vec<(PathBuf, Option<Stat>)>,
    /// Something was written so close to reading it that a write just after
    /// could leave it looking the same: it's read again next time.
    racy: bool,
}

impl Stamp {
    /// Whether the change, read again now, would be what it was.
    pub fn holds(&self) -> bool {
        if self.racy {
            return false;
        }
        let Ok((repo, _)) = open(&self.dir) else {
            return false;
        };
        if commits(&repo, &self.which).as_ref() != Ok(&self.commits) {
            return false;
        }
        self.disk.iter().all(|(path, was)| stat(path) == *was)
    }
}

/// How the disk stood as a change was read: filled in as it's read.
struct Disk {
    taken: SystemTime,
    paths: Vec<(PathBuf, Option<Stat>)>,
}

impl Disk {
    fn add(&mut self, path: PathBuf) {
        let now = stat(&path);
        self.paths.push((path, now));
    }
}

/// Reads a change, as `change` does, with its stamp. Paths, from the
/// repository's root, that `leave` says are left out are neither read
/// nor part of the stamp: writing to them changes nothing.
pub fn change_stamped(
    dir: &Path,
    which: &Which,
    leave: &dyn Fn(&str) -> bool,
) -> Result<(Change, Stamp), GitError> {
    let taken = SystemTime::now();
    let (repo, root) = open(dir)?;
    let commits = commits(&repo, which)?;
    let mut disk = Disk {
        taken,
        paths: Vec::new(),
    };
    let on_disk = matches!(which, Which::Uncommitted | Which::Since(_));
    let change = read_change(&repo, &root, which, leave, on_disk.then_some(&mut disk))?;
    let since = disk
        .taken
        .checked_sub(RACY)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let racy = disk.paths.iter().any(|(_, s)| {
        s.as_ref()
            .is_some_and(|s| s.modified.is_none_or(|m| m >= since))
    });
    Ok((
        change,
        Stamp {
            dir: dir.to_path_buf(),
            which: which.clone(),
            commits,
            disk: disk.paths,
            racy,
        },
    ))
}

fn read_change(
    repo: &gix::Repository,
    root: &Path,
    which: &Which,
    leave: &dyn Fn(&str) -> bool,
    disk: Option<&mut Disk>,
) -> Result<Change, GitError> {
    let head = repo.head_commit().ok();
    let head_tree = head
        .as_ref()
        .and_then(|c| c.tree_id().ok())
        .map(|id| id.detach());
    match which {
        Which::Uncommitted => uncommitted(repo, root, head_tree, leave, disk),
        Which::Since(base) => {
            let head = head.ok_or_else(|| GitError::Unknown("HEAD".into()))?;
            let from = merge_base(repo, head.id, base)?;
            let from_tree = repo
                .find_commit(from)
                .map_err(read_err)?
                .tree_id()
                .map_err(read_err)?
                .detach();
            let mut change = uncommitted(repo, root, Some(from_tree), leave, disk)?;
            change.title = format!("{} since {base}", branch_of(repo, head.id));
            Ok(change)
        }
        Which::Against(base) => {
            let head = head.ok_or_else(|| GitError::Unknown("HEAD".into()))?;
            let base_commit = commit(repo, base)?;
            let from = repo
                .merge_base(head.id, base_commit.id)
                .map_err(read_err)?
                .ok_or_else(|| GitError::Unrelated(base.clone()))?
                .detach();
            let from_tree = repo
                .find_commit(from)
                .map_err(read_err)?
                .tree_id()
                .map_err(read_err)?
                .detach();
            let branch = repo
                .head_name()
                .ok()
                .flatten()
                .map(|n| n.shorten().to_str_lossy().into_owned())
                .unwrap_or_else(|| short(head.id));
            between(
                repo,
                Some(from_tree),
                head_tree,
                format!("{branch} against {base}"),
                leave,
            )
        }
        Which::Commit(spec) => {
            let c = commit(repo, spec)?;
            let tree = c.tree_id().map_err(read_err)?.detach();
            let parent = match c.parent_ids().next() {
                Some(p) => Some(
                    repo.find_commit(p.detach())
                        .map_err(read_err)?
                        .tree_id()
                        .map_err(read_err)?
                        .detach(),
                ),
                None => None,
            };
            let summary = c
                .message_raw_sloppy()
                .lines()
                .next()
                .unwrap_or_default()
                .to_str_lossy()
                .trim()
                .to_string();
            let title = if summary.is_empty() {
                format!("Commit {}", short(c.id))
            } else {
                format!("{summary} ({})", short(c.id))
            };
            between(repo, parent, Some(tree), title, leave)
        }
    }
}

/// Two trees' differences.
fn between(
    repo: &gix::Repository,
    old: Option<ObjectId>,
    new: Option<ObjectId>,
    title: String,
    leave: &dyn Fn(&str) -> bool,
) -> Result<Change, GitError> {
    let before = files_in(repo, old)?;
    let after = files_in(repo, new)?;
    let paths: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    let mut files = Vec::new();
    let mut more = false;
    for path in paths.into_iter().filter(|p| !leave(p)) {
        let (a, b) = (before.get(path), after.get(path));
        if a == b {
            continue;
        }
        if files.len() == MAX_FILES {
            more = true;
            break;
        }
        let old_bytes = a.map(|id| blob(repo, *id)).transpose()?;
        let new_bytes = b.map(|id| blob(repo, *id)).transpose()?;
        files.push(file_change(
            path,
            old_bytes.as_deref(),
            new_bytes.as_deref(),
        ));
    }
    pair_moves(&mut files);
    Ok(Change { title, files, more })
}

/// What isn't committed: the files on disk against the last commit, new
/// files included and ignored ones left out. A file whose size and times
/// match git's index is what the index says, as git itself decides, so a
/// large repo isn't read whole.
fn uncommitted(
    repo: &gix::Repository,
    root: &Path,
    head: Option<ObjectId>,
    leave: &dyn Fn(&str) -> bool,
    disk: Option<&mut Disk>,
) -> Result<Change, GitError> {
    let mut committed = files_in(repo, head)?;
    committed.retain(|p, _| !leave(p));
    let index = repo.index_or_empty().map_err(read_err)?;
    let options = gix::index::entry::stat::Options::default();
    let stamp = index.timestamp();
    let mut known: BTreeMap<String, ObjectId> = BTreeMap::new();
    for entry in index.entries() {
        let path = entry.path(&index).to_str_lossy().into_owned();
        let on_disk = root.join(&path);
        let Ok(meta) = gix::index::fs::Metadata::from_path_no_follow(&on_disk) else {
            continue;
        };
        let Ok(stat) = gix::index::entry::Stat::from_fs(&meta) else {
            continue;
        };
        if entry.stat.matches(&stat, options) && !entry.stat.is_racy(stamp, options) {
            known.insert(path, entry.id);
        }
    }
    let (mut walked, mut folders) = walk_with_folders(root).map_err(read_err)?;
    walked.files.retain(|f| !leave(&f.rel));
    folders.retain(|f| !leave(f));
    let on_disk: BTreeSet<&str> = walked.files.iter().map(|f| f.rel.as_str()).collect();
    // How each file and folder stands, before any is read: one written
    // after this no longer matches.
    if let Some(disk) = disk {
        disk.add(repo.index_path());
        disk.add(repo.common_dir().join("info").join("exclude"));
        for folder in &folders {
            disk.add(root.join(folder));
        }
        for file in &walked.files {
            disk.add(root.join(&file.rel));
        }
        for path in committed.keys().filter(|p| !on_disk.contains(p.as_str())) {
            disk.add(root.join(path));
        }
    }
    let hash_kind = repo.object_hash();
    let mut now: BTreeMap<String, Option<ObjectId>> = BTreeMap::new();
    for file in &walked.files {
        let id = match known.get(&file.rel) {
            Some(id) => Some(*id),
            None if file.bytes > MAX_BYTES => None,
            None => std::fs::read(root.join(&file.rel)).ok().and_then(|bytes| {
                gix::objs::compute_hash(hash_kind, gix::objs::Kind::Blob, &bytes).ok()
            }),
        };
        now.insert(file.rel.clone(), id);
    }
    let mut files = Vec::new();
    let mut more = false;
    let paths: BTreeSet<&str> = committed
        .keys()
        .map(String::as_str)
        .chain(on_disk.iter().copied())
        .collect();
    for path in paths {
        let was = committed.get(path);
        let is = now.get(path);
        let changed = match (was, is) {
            (Some(a), Some(Some(b))) => a != b,
            (Some(_), Some(None)) => true,
            // Committed, but not on disk: deleted, unless it's only ignored now.
            (Some(_), None) => !root.join(path).exists(),
            (None, Some(_)) => true,
            (None, None) => false,
        };
        if !changed {
            continue;
        }
        if files.len() == MAX_FILES {
            more = true;
            break;
        }
        let old_bytes = was.map(|id| blob(repo, *id)).transpose()?;
        let new_bytes = if is.is_some() {
            std::fs::read(root.join(path)).ok()
        } else {
            None
        };
        let change = file_change(path, old_bytes.as_deref(), new_bytes.as_deref());
        // Only line endings differ, or nothing did: not a change worth showing.
        if change.skip.is_none() && change.hunks.is_empty() && change.status == Status::Modified {
            continue;
        }
        files.push(change);
    }
    pair_moves(&mut files);
    Ok(Change {
        title: "Uncommitted changes".into(),
        files,
        more,
    })
}

/// One file's change, from its bytes before and after.
fn file_change(path: &str, old: Option<&[u8]>, new: Option<&[u8]>) -> FileChange {
    let read = |bytes: Option<&[u8]>| bytes.map(|b| text_of(path, b)).transpose();
    texts_change(path, read(old), read(new))
}

/// One file's change, from its text before and after as kelve reads it:
/// none where it wasn't there, or why it wasn't read.
pub(crate) fn texts_change(
    path: &str,
    old: Result<Option<String>, Skip>,
    new: Result<Option<String>, Skip>,
) -> FileChange {
    let status = match (&old, &new) {
        (Ok(None), _) => Status::Added,
        (_, Ok(None)) => Status::Deleted,
        _ => Status::Modified,
    };
    let (before, after) = match (old, new) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(skip), _) | (_, Err(skip)) => {
            return FileChange {
                path: path.to_string(),
                status,
                skip: Some(skip),
                hunks: Vec::new(),
                added: 0,
                removed: 0,
                after: None,
                before: None,
                moved_from: None,
            };
        }
    };
    let (hunks, added, removed) = hunks(
        before.as_deref().unwrap_or(""),
        after.as_deref().unwrap_or(""),
    );
    FileChange {
        path: path.to_string(),
        status,
        skip: None,
        hunks,
        added,
        removed,
        after,
        before,
        moved_from: None,
    }
}

/// How alike a file taken out and one added must be to be one file moved,
/// as git's own default: half their lines the same.
const MOVED_ALIKE: f32 = 0.5;
/// The most pairs compared for files moved and changed: past that, only
/// files moved as they were are found.
const MOST_COMPARED: usize = 2_500;

/// Finds the files a change moved: each file taken out, paired with one
/// added that's the same, or mostly the same, the same name first. The
/// two become one file, at its new path, changed against the old.
fn pair_moves(files: &mut Vec<FileChange>) {
    let gone: Vec<usize> = (0..files.len())
        .filter(|&i| files[i].status == Status::Deleted && files[i].before.is_some())
        .collect();
    let new: Vec<usize> = (0..files.len())
        .filter(|&i| files[i].status == Status::Added && files[i].after.is_some())
        .collect();
    if gone.is_empty() || new.is_empty() {
        return;
    }
    let name = |p: &str| p.rsplit('/').next().unwrap_or(p).to_string();
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut taken: BTreeSet<usize> = BTreeSet::new();
    // The same text, moved as it was.
    for &g in &gone {
        if let Some(&n) = new
            .iter()
            .find(|&&n| !taken.contains(&n) && files[n].after == files[g].before)
        {
            pairs.push((g, n));
            taken.insert(n);
        }
    }
    // Mostly the same: the most alike first.
    let left: Vec<usize> = gone
        .iter()
        .copied()
        .filter(|g| !pairs.iter().any(|(p, _)| p == g))
        .collect();
    if left.len() * new.len() <= MOST_COMPARED {
        let mut alike: Vec<(f32, bool, usize, usize)> = Vec::new();
        for &g in &left {
            for &n in new.iter().filter(|n| !taken.contains(n)) {
                let (Some(a), Some(b)) = (&files[g].before, &files[n].after) else {
                    continue;
                };
                let ratio = TextDiff::from_lines(a.as_str(), b.as_str()).ratio();
                if ratio >= MOVED_ALIKE {
                    alike.push((ratio, name(&files[g].path) == name(&files[n].path), g, n));
                }
            }
        }
        alike.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.total_cmp(&a.0)));
        let mut done: BTreeSet<usize> = BTreeSet::new();
        for (_, _, g, n) in alike {
            if done.contains(&g) || taken.contains(&n) {
                continue;
            }
            pairs.push((g, n));
            done.insert(g);
            taken.insert(n);
        }
    }
    for &(g, n) in &pairs {
        let before = files[g].before.clone().unwrap_or_default();
        let after = files[n].after.clone().unwrap_or_default();
        let (hunks, added, removed) = hunks(&before, &after);
        let from = files[g].path.clone();
        let f = &mut files[n];
        f.status = Status::Modified;
        f.hunks = hunks;
        f.added = added;
        f.removed = removed;
        f.before = Some(before);
        f.moved_from = Some(from);
    }
    let gone: BTreeSet<usize> = pairs.iter().map(|(g, _)| *g).collect();
    let mut i = 0;
    files.retain(|_| {
        i += 1;
        !gone.contains(&(i - 1))
    });
}

/// Every line of a file through a change, in order: the lines it kept and
/// added, numbered as they are after it, and the lines it took out, where
/// they were. One hunk, with all of its context.
pub fn whole(old: &str, new: &str) -> Hunk {
    let old = old.replace("\r\n", "\n");
    let new = new.replace("\r\n", "\n");
    let diff = TextDiff::from_lines(&old, &new);
    let number = |n: Option<usize>| n.and_then(|n| u32::try_from(n + 1).ok());
    let lines = diff
        .iter_all_changes()
        .map(|change| Line {
            kind: match change.tag() {
                ChangeTag::Equal => LineKind::Context,
                ChangeTag::Insert => LineKind::Added,
                ChangeTag::Delete => LineKind::Removed,
            },
            old: number(change.old_index()),
            new: number(change.new_index()),
            text: change.value().trim_end_matches('\n').to_string(),
        })
        .collect();
    Hunk { lines }
}

/// The hunks between two texts, ignoring how lines end, with the count of
/// lines added and removed.
pub fn hunks(old: &str, new: &str) -> (Vec<Hunk>, u32, u32) {
    let old = old.replace("\r\n", "\n");
    let new = new.replace("\r\n", "\n");
    let diff = TextDiff::from_lines(&old, &new);
    let (mut added, mut removed) = (0, 0);
    let mut out = Vec::new();
    for group in diff.grouped_ops(CONTEXT) {
        let mut lines = Vec::new();
        for op in group {
            for change in diff.iter_changes(&op) {
                let kind = match change.tag() {
                    ChangeTag::Equal => LineKind::Context,
                    ChangeTag::Insert => {
                        added += 1;
                        LineKind::Added
                    }
                    ChangeTag::Delete => {
                        removed += 1;
                        LineKind::Removed
                    }
                };
                let number = |n: Option<usize>| n.and_then(|n| u32::try_from(n + 1).ok());
                lines.push(Line {
                    kind,
                    old: number(change.old_index()),
                    new: number(change.new_index()),
                    text: change.value().trim_end_matches('\n').to_string(),
                });
            }
        }
        out.push(Hunk { lines });
    }
    (out, added, removed)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn hunks_number_both_sides_and_keep_context() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\n";
        let new = "a\nb\nc\nD\ne\nf\ng\nh\ni\nj\n";
        let (hunks, added, removed) = hunks(old, new);
        assert_eq!((added, removed), (2, 1));
        assert_eq!(
            hunks.len(),
            1,
            "changes this close share a hunk, as in git: {hunks:#?}"
        );
        let first = &hunks[0].lines;
        assert_eq!(
            first[0],
            Line {
                kind: LineKind::Context,
                old: Some(1),
                new: Some(1),
                text: "a".into()
            }
        );
        assert!(
            first
                .iter()
                .any(|l| l.kind == LineKind::Removed && l.text == "d" && l.old == Some(4))
        );
        assert!(
            first
                .iter()
                .any(|l| l.kind == LineKind::Added && l.text == "D" && l.new == Some(4))
        );
        // Line endings alone are no change.
        assert!(super::hunks("a\r\nb\r\n", "a\nb\n").0.is_empty());
    }

    #[test]
    fn a_secret_file_changes_without_being_read() {
        let c = file_change(".env", Some(b"KEY=a"), Some(b"KEY=b"));
        assert_eq!(c.skip, Some(Skip::Secret));
        assert!(c.hunks.is_empty());
        let c = file_change("a.rs", None, Some(b"fn a() {}\n"));
        assert_eq!((c.status, c.added), (Status::Added, 1));
        assert_eq!(c.after.as_deref(), Some("fn a() {}\n"));
    }

    /// Runs git for a test's fixture, never kelve: with no global or system
    /// configuration, and a fixed author.
    fn git(dir: &Path, args: &[&str]) {
        let ran = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
                // A commit starts git's maintenance in the background, which
                // makes and removes a lock in .git while a test reads it.
                "-c",
                "maintenance.auto=false",
            ])
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .expect("git runs");
        assert!(
            ran.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&ran.stderr)
        );
    }

    fn repo() -> PathBuf {
        let dir = crate::tests::folder(&[
            ("src/ledger.rs", "fn reserve() {}\n\nfn settle() {}\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("README.md", "# Ledger\n"),
            (".gitignore", "target/\n"),
        ]);
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "Start the ledger"]);
        dir
    }

    #[test]
    fn written_since_finds_files_written_after_a_moment_that_git_sees_changed() {
        let dir = repo();
        let ms = |t: std::time::SystemTime| {
            t.duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64
        };
        // Written before the moment: not since, whatever git says of it.
        std::fs::write(dir.join("README.md"), "# Ledger, changed before\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let moment = ms(std::time::SystemTime::now());
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("src/ledger.rs"), "fn reserve() { todo!() }\n").unwrap();
        std::fs::write(dir.join("src/new.rs"), "fn new() {}\n").unwrap();
        // Written again as it was committed, ignored, or a lockfile: none.
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/out.rs"), "fn out() {}\n").unwrap();
        std::fs::write(dir.join("Cargo.lock"), "# lock\n").unwrap();
        let found = written_since(&dir, moment).unwrap();
        assert!(!found.more);
        let root = std::fs::canonicalize(&dir).unwrap();
        let rel: Vec<String> = found
            .files
            .iter()
            .map(|p| {
                std::fs::canonicalize(p)
                    .unwrap()
                    .strip_prefix(&root)
                    .unwrap()
                    .display()
                    .to_string()
                    // As written on every platform: `src\new.rs` on Windows.
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(rel, ["src/ledger.rs", "src/new.rs"]);
        // From a folder inside it, as kelve may have opened it.
        let inside = written_since(&dir.join("src"), moment).unwrap();
        assert_eq!(
            inside.files,
            [dir.join("src/ledger.rs"), dir.join("src/new.rs")]
        );
    }

    #[test]
    fn written_since_in_a_folder_that_isnt_a_repository_is_every_file_written_since() {
        let dir = crate::tests::folder(&[
            ("notes.md", "# Notes\n"),
            ("drafts/a.py", "print(1)\n"),
            (".gitignore", "out/\n"),
        ]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let moment = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(dir.join("drafts/a.py"), "print(2)\n").unwrap();
        std::fs::create_dir_all(dir.join("out")).unwrap();
        std::fs::write(dir.join("out/b.py"), "print(3)\n").unwrap();
        let found = written_since(&dir, moment).unwrap();
        let names: Vec<String> = found
            .files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            names,
            ["a.py"],
            "what's ignored is left out, as kelve walks"
        );
        assert!(!found.more);
    }

    #[test]
    fn history_finds_the_commits_that_changed_a_file_or_a_folder() {
        let dir = repo();
        std::fs::write(dir.join("src/ledger.rs"), "fn reserve() {}\n").unwrap();
        git(
            &dir,
            &["commit", "-q", "-am", "Drop settle\n\nNothing calls it."],
        );
        std::fs::write(dir.join("README.md"), "# Ledger, again\n").unwrap();
        git(&dir, &["commit", "-q", "-am", "Say what it is"]);

        let file = history(&dir, "src/ledger.rs", 10).unwrap();
        let said: Vec<&str> = file.iter().map(|c| c.subject.as_str()).collect();
        assert_eq!(said, ["Drop settle", "Start the ledger"]);
        assert_eq!(file[0].body, "Nothing calls it.");
        assert_eq!(file[0].id.len(), 7);
        let folder = history(&dir, "src", 10).unwrap();
        assert_eq!(folder.len(), 2);
        assert_eq!(history(&dir, "", 1).unwrap()[0].subject, "Say what it is");
        // From a folder inside the repository, its paths are its own.
        assert_eq!(history(&dir.join("src"), "ledger.rs", 10).unwrap().len(), 2);
        assert!(history(&dir, "nowhere.rs", 10).unwrap().is_empty());
    }

    /// Every file under `.git`, with its bytes, to show nothing wrote there.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk_into(at: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in std::fs::read_dir(at).expect("readable").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk_into(&path, out);
                } else {
                    out.insert(path.clone(), std::fs::read(&path).expect("readable"));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk_into(&dir.join(".git"), &mut out);
        out
    }

    fn paths(c: &Change) -> Vec<(&str, Status)> {
        c.files
            .iter()
            .map(|f| (f.path.as_str(), f.status))
            .collect()
    }

    #[test]
    fn a_file_moved_is_one_file_changed_against_where_it_was() {
        let dir = repo();
        std::fs::create_dir_all(dir.join("src/books")).unwrap();
        // Moved as it was, and moved with a line changed.
        std::fs::rename(dir.join("src/main.rs"), dir.join("src/start.rs")).unwrap();
        std::fs::remove_file(dir.join("src/ledger.rs")).unwrap();
        std::fs::write(
            dir.join("src/books/ledger.rs"),
            "fn reserve() {}\n\nfn settle(cost: f64) {}\n",
        )
        .unwrap();
        git(&dir, &["add", "-A"]);
        git(&dir, &["commit", "-q", "-m", "Move the books"]);
        for which in [Which::Commit("HEAD".into()), Which::Uncommitted] {
            let c = change(&dir, &which).expect("read");
            if which == Which::Uncommitted {
                assert!(c.files.is_empty(), "nothing left uncommitted");
                continue;
            }
            assert_eq!(
                paths(&c),
                [
                    ("src/books/ledger.rs", Status::Modified),
                    ("src/start.rs", Status::Modified),
                ]
            );
            let ledger = &c.files[0];
            assert_eq!(ledger.moved_from.as_deref(), Some("src/ledger.rs"));
            assert_eq!((ledger.added, ledger.removed), (1, 1));
            assert_eq!(c.files[1].moved_from.as_deref(), Some("src/main.rs"));
            assert_eq!((c.files[1].added, c.files[1].removed), (0, 0));
        }
    }

    #[test]
    fn uncommitted_work_includes_new_files_and_leaves_ignored_ones_out() {
        let dir = repo();
        std::fs::write(
            dir.join("src/ledger.rs"),
            "fn reserve() {}\n\nfn settle(cost: f64) {}\n",
        )
        .unwrap();
        std::fs::write(dir.join("src/price.rs"), "fn most() {}\n").unwrap();
        std::fs::remove_file(dir.join("README.md")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/out.rs"), "fn built() {}\n").unwrap();
        std::fs::write(dir.join(".env"), "KEY=abc\n").unwrap();
        let before = snapshot(&dir);
        let c = change(&dir, &Which::Uncommitted).expect("read");
        assert_eq!(c.title, "Uncommitted changes");
        assert_eq!(
            paths(&c),
            [
                (".env", Status::Added),
                ("README.md", Status::Deleted),
                ("src/ledger.rs", Status::Modified),
                ("src/price.rs", Status::Added),
            ]
        );
        assert_eq!(c.files[0].skip, Some(Skip::Secret), "listed, never read");
        let ledger = &c.files[2];
        assert_eq!((ledger.added, ledger.removed), (1, 1));
        assert!(
            ledger
                .after
                .as_deref()
                .is_some_and(|t| t.contains("settle(cost"))
        );
        // Read from a subfolder, it's the same repo.
        assert_eq!(
            paths(&change(&dir.join("src"), &Which::Uncommitted).expect("read")),
            paths(&c)
        );
        assert_eq!(snapshot(&dir), before, "nothing in .git changed");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Every file and folder in `dir`, git's own too, as written a minute
    /// ago: nothing in it is too new to tell from a write after it.
    fn age(dir: &Path) {
        let then = SystemTime::now() - Duration::from_secs(60);
        let mut at = vec![dir.to_path_buf()];
        while let Some(path) = at.pop() {
            if path.is_dir() {
                at.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            }
            touchable(&path).set_modified(then).unwrap();
        }
    }

    /// A file or folder opened so its times can be set.
    #[cfg(not(windows))]
    fn touchable(path: &Path) -> std::fs::File {
        std::fs::File::open(path).unwrap()
    }

    /// A file or folder opened so its times can be set: on Windows, that
    /// takes asking for it, and a folder opens only with backup semantics.
    #[cfg(windows)]
    fn touchable(path: &Path) -> std::fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        std::fs::File::options()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)
            .unwrap()
    }

    /// A change read with its stamp, once nothing's too new: the stamp holds.
    fn stamped(dir: &Path, which: &Which) -> Stamp {
        age(dir);
        let (_, stamp) = change_stamped(dir, which, &|p| p.starts_with(".kelve/")).expect("read");
        assert!(stamp.holds(), "nothing changed");
        stamp
    }

    #[test]
    fn a_stamp_holds_until_work_not_committed_moves_on() {
        let dir = repo();
        git(&dir, &["checkout", "-q", "-b", "agent/fix"]);
        std::fs::write(dir.join("src/main.rs"), "fn main() { retry(); }\n").unwrap();
        std::fs::create_dir_all(dir.join("empty")).unwrap();
        for which in [Which::Uncommitted, Which::Since("main".into())] {
            // A file the change has, written again.
            let stamp = stamped(&dir, &which);
            std::fs::write(dir.join("src/main.rs"), "fn main() { again(); }\n").unwrap();
            assert!(!stamp.holds(), "{which:?}: a changed file written");
            // A file it doesn't have yet, changed.
            let stamp = stamped(&dir, &which);
            std::fs::write(dir.join("src/ledger.rs"), "fn reserve() {}\n").unwrap();
            assert!(!stamp.holds(), "{which:?}: an unchanged file written");
            // A file added, in a folder it has, one with nothing in it, and one new.
            for added in ["src/new.rs", "empty/new.rs", "brand/new/new.rs"] {
                let stamp = stamped(&dir, &which);
                std::fs::create_dir_all(dir.join(added).parent().unwrap()).unwrap();
                std::fs::write(dir.join(added), "fn new() {}\n").unwrap();
                assert!(!stamp.holds(), "{which:?}: {added} added");
            }
            // A file taken out.
            let stamp = stamped(&dir, &which);
            std::fs::remove_file(dir.join("README.md")).unwrap();
            assert!(!stamp.holds(), "{which:?}: a file taken out");
            // A commit.
            let stamp = stamped(&dir, &which);
            git(&dir, &["add", "."]);
            git(&dir, &["commit", "-q", "-m", "More"]);
            assert!(!stamp.holds(), "{which:?}: a commit made");
            // Ignored files, and paths it leaves out, aren't part of it.
            std::fs::create_dir_all(dir.join("target")).unwrap();
            std::fs::create_dir_all(dir.join(".kelve/read")).unwrap();
            let stamp = stamped(&dir, &which);
            std::fs::write(dir.join("target/out.rs"), "fn built() {}\n").unwrap();
            std::fs::write(dir.join(".kelve/read/a.json"), "{}\n").unwrap();
            assert!(stamp.holds(), "{which:?}: only what it leaves out written");
            std::fs::write(dir.join("README.md"), "# Ledger\n").unwrap();
        }
        // Written just now, a file could be written again unseen: it's read again.
        std::fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
        let (_, stamp) = change_stamped(&dir, &Which::Uncommitted, &|_| false).expect("read");
        assert!(!stamp.holds(), "too new to tell");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stamp_on_commits_holds_until_they_move_on() {
        let dir = repo();
        git(&dir, &["checkout", "-q", "-b", "agent/fix"]);
        std::fs::write(dir.join("src/retry.rs"), "fn retry() {}\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "Retry with jitter"]);
        let against = Which::Against("main".into());
        let one = Which::Commit("HEAD".into());
        // Work not committed isn't part of either.
        let stamps = [stamped(&dir, &against), stamped(&dir, &one)];
        std::fs::write(dir.join("src/main.rs"), "fn main() { retry(); }\n").unwrap();
        assert!(stamps.iter().all(Stamp::holds), "nothing committed");
        // A commit on the branch, or on what it's against.
        git(&dir, &["commit", "-q", "-am", "Call it"]);
        assert!(stamps.iter().all(|s| !s.holds()), "a commit on the branch");
        let stamp = stamped(&dir, &against);
        git(&dir, &["checkout", "-q", "main"]);
        git(&dir, &["checkout", "-q", "-b", "other"]);
        assert!(!stamp.holds(), "another branch checked out");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_worktree_is_read_since_its_base_committed_or_not_with_its_commits() {
        let dir = repo();
        git(&dir, &["checkout", "-q", "-b", "agent/fix"]);
        std::fs::write(dir.join("src/retry.rs"), "fn retry() {}\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "Retry with jitter"]);
        // Still working: a change not committed yet.
        std::fs::write(dir.join("src/main.rs"), "fn main() { retry(); }\n").unwrap();
        let c = change(&dir, &Which::Since("main".into())).expect("read");
        assert_eq!(c.title, "agent/fix since main");
        assert_eq!(
            paths(&c),
            [
                ("src/main.rs", Status::Modified),
                ("src/retry.rs", Status::Added)
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_is_read_against_where_it_left_and_a_commit_against_its_parent() {
        let dir = repo();
        git(&dir, &["checkout", "-q", "-b", "agent"]);
        std::fs::write(dir.join("src/price.rs"), "fn most() {}\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "Add prices"]);
        // main moves on too, which the branch didn't change.
        git(&dir, &["checkout", "-q", "main"]);
        std::fs::write(dir.join("src/main.rs"), "fn main() { run() }\n").unwrap();
        git(&dir, &["commit", "-q", "-am", "Run"]);
        git(&dir, &["checkout", "-q", "agent"]);
        let c = change(&dir, &Which::Against("main".into())).expect("read");
        assert_eq!(c.title, "agent against main");
        assert_eq!(paths(&c), [("src/price.rs", Status::Added)]);
        let c = change(&dir, &Which::Commit("HEAD".into())).expect("read");
        assert!(c.title.starts_with("Add prices ("), "{}", c.title);
        assert_eq!(paths(&c), [("src/price.rs", Status::Added)]);
        // The first commit has no parent: everything in it is new.
        let c = change(&dir, &Which::Commit("main~1".into())).expect("read");
        assert_eq!(c.files.len(), 4);
        assert!(matches!(
            change(&dir, &Which::Against("nowhere".into())),
            Err(GitError::Unknown(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn outside_a_repo_says_so() {
        let dir = crate::tests::folder(&[("a.rs", "fn a() {}\n")]);
        assert_eq!(change(&dir, &Which::Uncommitted), Err(GitError::NotARepo));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod blob_ids {
    #[test]
    fn a_files_blob_id_is_gits() {
        // `printf 'hello\n' | git hash-object --stdin`
        assert_eq!(
            super::blob_id(b"hello\n").as_deref(),
            Some("ce013625030ba8dba906f756967f9e9ca394464a")
        );
    }
}
