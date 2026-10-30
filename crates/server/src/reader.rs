//! The reader's work on the server: what's open, its files and changes, the
//! marks on them as they come, what a folder or a change is built on, and the
//! notes beside them. The routes are `api::reader`; the work the person's agent
//! does for them runs as jobs, here.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use kelve_core::fence::fence;
use kelve_core::ideas::IdeaKind;
use kelve_core::marks::Mark;
use kelve_core::notes::{Front, Passage};
use kelve_core::views::{
    BuiltOn, IdeaTag, LinesIn, Marked, Marking, Note, NoteIn, NoteOn, NoteRef, NoteState, OpenKind,
    Opened, Place, Reading, ServerEvent,
};
use kelve_engine::git::{Change, FileChange, LineKind, Stamp, Which};
use kelve_store::entries::Idea;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::agent::{self, Priority};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::mark::{self, Lines, Waiting};
use crate::pipeline::{self, Failed, Then, entries};
use crate::state::Shared;

/// Marking a file, as a job: `{opened, path}`.
pub const READ_FILE: &str = "read.file";
/// Marking a change's file, on its added lines: `{opened, path}`.
pub const READ_DIFF: &str = "read.diff";
/// Choosing what a folder is built on: `{opened}`.
pub const BUILT_ON: &str = "read.builtOn";
/// Reading a folder's files in the background, most central first, a few
/// at a time: `{opened}`.
pub const READ_FOLDER: &str = "read.folder";

/// The most of a change's files marked as soon as it's opened.
pub const MARK_AHEAD: usize = 20;

/// Something opened, and the folder it's in.
#[derive(Debug, Clone)]
pub struct Open {
    pub row: kelve_store::reader::Opened,
    pub root: PathBuf,
}

impl Open {
    pub fn kind(&self) -> OpenKind {
        match self.row.kind.as_str() {
            "file" => OpenKind::File,
            "change" => OpenKind::Change,
            _ => OpenKind::Folder,
        }
    }

    pub fn view(&self) -> Opened {
        Opened {
            id: self.row.id,
            kind: self.kind(),
            root: shown(&self.root),
            path: (!self.row.path.is_empty()).then(|| self.row.path.clone()),
            change: (!self.row.change.is_empty()).then(|| self.row.change.clone()),
            title: self.row.title.clone(),
            opened_at: kelve_core::Timestamp(kelve_store::at(self.row.opened_at)),
            said: self.row.said.clone(),
            agent: (!self.row.agent.is_empty()).then(|| self.row.agent.clone()),
            only: self.only(),
            way: None,
            ready: None,
        }
    }

    /// When only some files of the folder were opened: which.
    pub fn only(&self) -> Option<Vec<String>> {
        self.row
            .only
            .as_deref()
            .and_then(|o| serde_json::from_str(o).ok())
    }

    /// Which change it is, if it's one.
    pub fn which(&self) -> Option<Which> {
        which(&self.row.change)
    }
}

/// A folder as the person would write it: `~/projects/kelve`.
pub fn shown(path: &Path) -> String {
    let text = path.display().to_string();
    match std::env::home_dir().map(|h| h.display().to_string()) {
        Some(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text,
    }
}

/// A path as the person wrote it, `~/projects/kelve` too, as a path.
pub fn unshown(text: &str) -> PathBuf {
    match (
        text.strip_prefix('~'),
        std::env::home_dir().map(|h| h.display().to_string()),
    ) {
        (Some(rest), Some(home))
            if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) =>
        {
            PathBuf::from(format!("{home}{rest}"))
        }
        _ => PathBuf::from(text),
    }
}

/// A change by its words: `uncommitted`, `against main`, `since main`,
/// `commit 3f2a9c1`.
pub fn which(words: &str) -> Option<Which> {
    let words = words.trim();
    if words == "uncommitted" {
        return Some(Which::Uncommitted);
    }
    if let Some(base) = words.strip_prefix("since ") {
        return Some(Which::Since(base.trim().to_string()));
    }
    if let Some(base) = words.strip_prefix("against ") {
        return Some(Which::Against(base.trim().to_string()));
    }
    words
        .strip_prefix("commit ")
        .map(|rev| Which::Commit(rev.trim().to_string()))
}

/// Something opened, by its id.
pub async fn opened(app: &Shared, id: i64) -> ApiResult<Open> {
    let mut conn = app.db.acquire().await?;
    let row = kelve_store::reader::get(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("That isn't open in kelve any more. Open it again."))?;
    let root = PathBuf::from(&row.root);
    if !root.is_dir() {
        return Err(ApiError::not_found(format!(
            "{} isn't there any more.",
            shown(&root)
        )));
    }
    Ok(Open { row, root })
}

/// Whether a path is kelve's own, in a project: its notes, shown in the
/// notes window rather than among the files.
pub fn is_kelves(rel: &str) -> bool {
    rel == ".kelve" || rel.starts_with(".kelve/")
}

/// The key of the idea a mark leads to: the graph's ID, or a new idea's.
pub fn key_of(m: &Mark) -> String {
    m.idea
        .as_ref()
        .map_or_else(|| entries::new_key(&m.name), |i| i.id.clone())
}

pub fn marked(marks: Vec<Mark>) -> Vec<Marked> {
    marks
        .into_iter()
        .map(|mark| Marked {
            key: key_of(&mark),
            mark,
            new: false,
        })
        .collect()
}

/// The lines a change added to a file, as runs.
pub fn added(fc: &FileChange) -> Vec<Lines> {
    let mut runs: Vec<Lines> = Vec::new();
    for line in fc.hunks.iter().flat_map(|h| &h.lines) {
        let (LineKind::Added, Some(n)) = (line.kind, line.new) else {
            continue;
        };
        match runs.last_mut() {
            Some((_, to)) if *to + 1 == n => *to = n,
            _ => runs.push((n, n)),
        }
    }
    runs
}

/// A change as last read, and how to tell it still holds.
type Kept = Arc<Mutex<Option<(Stamp, Arc<Change>)>>>;

/// The changes lately read, by folder and which change, the latest last:
/// each is read again only once its repository says it's moved on.
static CHANGES: LazyLock<Mutex<Vec<(String, Kept)>>> = LazyLock::new(Default::default);

/// How many changes are kept read.
const KEEP_CHANGES: usize = 8;

/// A change, read from git's own files: as read before, while nothing it
/// was read from has moved on.
pub fn change(open: &Open) -> ApiResult<Arc<Change>> {
    let which = open
        .which()
        .ok_or_else(|| ApiError::bad_request("That isn't a change kelve can read."))?;
    let key = format!("{}\n{}", open.root.display(), open.row.change);
    let slot = {
        let mut all = CHANGES.lock().unwrap_or_else(|p| p.into_inner());
        let slot = match all.iter().position(|(k, _)| *k == key) {
            Some(at) => all.remove(at).1,
            None => Arc::default(),
        };
        all.push((key, slot.clone()));
        let over = all.len().saturating_sub(KEEP_CHANGES);
        all.drain(..over);
        slot
    };
    // One reading at a time for each change: asked for at once, it's read once.
    let mut kept = slot.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((stamp, change)) = kept.as_ref()
        && stamp.holds()
    {
        return Ok(change.clone());
    }
    // kelve's own folder, what's been read, the way in and notes, is
    // kept beside the work, not part of it.
    let (change, stamp) = kelve_engine::git::change_stamped(&open.root, &which, &is_kelves)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let change = Arc::new(change);
    *kept = Some((stamp, change.clone()));
    Ok(change)
}

/// Bumped whenever marks come in: what a change is built on, worked out
/// from them, is used again until they do.
static MARKED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Says marks came in, or a file was read.
pub fn marked_again() {
    MARKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

type BuiltOnEach = (Vec<BuiltOn>, BTreeMap<String, Vec<IdeaTag>>);
/// What a change was built on: from which marks, and which text.
type Built = (u64, u64, BuiltOnEach);

/// What changes are built on, lately worked out: by what was opened, with
/// the marks they were worked out from and the change's own text.
static BUILT: LazyLock<Mutex<HashMap<i64, Built>>> = LazyLock::new(Default::default);

/// A change's text, as a number, to tell when it's moved on.
fn print(change: &Change) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in &change.files {
        f.path.hash(&mut h);
        f.before.hash(&mut h);
        f.after.hash(&mut h);
    }
    h.finish()
}

/// How marking stands, from what's kept: done, or with the agent.
pub fn marking(whole: bool) -> Marking {
    if whole {
        Marking::Done
    } else {
        Marking::Marking
    }
}

/// Asks for a changed file to be read.
pub async fn ask_diff(app: &Shared, id: i64, path: &str) -> ApiResult<()> {
    ask(
        app,
        READ_DIFF,
        &format!("{READ_DIFF}:{id}:{path}"),
        json!({ "opened": id, "path": path, "priority": Priority::Now as i64 }),
    )
    .await
}

/// Asks for a job, under a key, so asking again while it waits does nothing.
pub async fn ask(app: &Shared, kind: &str, key: &str, payload: serde_json::Value) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::jobs::add(&mut conn, kind, Some(key), &payload, Utc::now()).await?;
    app.wake.notify_one();
    Ok(())
}

/// Marks a file, or a change's file, as a job, telling the reader as its
/// marks come in, and keeping which ideas the file is built on. The
/// agent's answer runs it again.
pub async fn read_job(
    app: &Shared,
    payload: &serde_json::Value,
    diff: bool,
) -> pipeline::Result<Then> {
    let (Some(id), Some(path)) = (payload["opened"].as_i64(), payload["path"].as_str()) else {
        return Ok(Then::Done);
    };
    let Ok(open) = opened(app, id).await else {
        return Ok(Then::Done);
    };
    let (text, only) = if diff {
        let Ok(change) = change(&open) else {
            return Ok(Then::Done);
        };
        let Some(fc) = change.files.iter().find(|f| f.path == path) else {
            return Ok(Then::Done);
        };
        // A changed file is read whole, so what it already rested on shows
        // beside what the change added.
        match &fc.after {
            Some(after) => (after.clone(), None::<Vec<Lines>>),
            None => return Ok(Then::Done),
        }
    } else {
        match kelve_engine::read::read(&open.root, path) {
            Ok(t) => (t.text, None),
            Err(_) => return Ok(Then::Done),
        }
    };
    let priority = match payload["priority"].as_i64() {
        Some(0) => Priority::Now,
        Some(1) => Priority::Kept,
        _ => Priority::Rest,
    };
    // Asked for with the rest of a folder, before only part of it was read
    // now: not wanted, unless the person opens it.
    if !diff
        && priority == Priority::Rest
        && crate::way::to_read_now(app, &open)
            .await
            .is_some_and(|n| !n.contains(path))
    {
        return Ok(Then::Done);
    }
    let kind = if diff { READ_DIFF } else { READ_FILE };
    let key = format!("{kind}:{id}:{path}");
    let resume = agent::Resume::job(kind, key.clone(), payload.clone());
    let waiting = mark_and_keep(app, &open, path, &text, only.as_deref(), priority, resume).await?;
    // Waiting for the agent: at its place on the path, when it's on one.
    if waiting.is_some()
        && let Some(rank) = crate::way::rank(app, &open, path).await
    {
        let mut conn = app.db.acquire().await?;
        kelve_store::asks::place_reading(&mut conn, &key, rank).await?;
    }
    // Read, it's read plainly next, as soon as wanted as its reading was;
    // and once the way in is read, the rest of the path is asked for.
    if waiting.is_none() {
        crate::plainly::after_reading(app, id, path, priority as i64).await?;
    }
    Ok(Then::Done)
}

/// Marks a file's text, or a change's added lines in it, telling the
/// reader when its marks come in, and keeps the ideas they lead to: what's
/// still waiting, if anything. A file not wholly marked isn't kept as read.
async fn mark_and_keep(
    app: &Shared,
    open: &Open,
    path: &str,
    text: &str,
    only: Option<&[Lines]>,
    priority: Priority,
    resume: agent::Resume,
) -> pipeline::Result<Option<Waiting>> {
    let id = open.row.id;
    let done = mark::text(
        app,
        path,
        text,
        only,
        &mark::For {
            opened: Some(id),
            priority,
            resume,
        },
    )
    .await?;
    let whole = (only.is_none() && done.waiting.is_none()).then_some(text);
    keep_ideas(app, open, path, &done.marks, whole).await?;
    if done.waiting.is_none() {
        crate::read::keep_file(app, open, path, text).await;
        // The file read plainly, waiting for this file to be read, goes
        // now.
        let mut conn = app.db.acquire().await?;
        let plainly = format!("{}:{id}:{path}", crate::plainly::JOB);
        kelve_store::jobs::due_now(&mut conn, crate::plainly::JOB, &plainly).await?;
        drop(conn);
        app.wake.notify_one();
    }
    // Only when marks came in: the reader asks again for what it hears
    // changed, and asking again for what's still with the agent is how it
    // got here.
    if done.waiting.is_none() {
        app.publish(ServerEvent::Marks {
            opened: id,
            path: path.to_string(),
        });
        app.publish(ServerEvent::Read { opened: id });
    }
    Ok(done.waiting)
}

/// How far reading a folder has got, asking for the rest of it to be read
/// by the person's agent: `read` of its `of` files, then its folders'
/// pages read plainly, written from them.
pub async fn reading(app: &Shared, open: &Open, read: u32, of: u32) -> ApiResult<Reading> {
    let (read, of, pages) = with_page(app, open, None, read, of, true).await?;
    let marking = if read >= of {
        Marking::Done
    } else {
        read_ahead(app, open).await?;
        Marking::Marking
    };
    Ok(Reading {
        read,
        of,
        checking: checking(app, open.row.id, of - read.min(of)).await?,
        marking,
        pages: pages.of,
        pages_left: pages.left,
    })
}

/// Files read, of how many, and what's open's own pages after them read
/// plainly (`plainly::pages`): each of its folders' and a change's, one
/// more each, read once it's written; and the pages alone. Any never
/// asked for is asked for now, when `ask`.
pub async fn with_page(
    app: &Shared,
    open: &Open,
    change: Option<&Change>,
    read: u32,
    of: u32,
    ask: bool,
) -> ApiResult<(u32, u32, crate::plainly::Pages)> {
    let pages = crate::plainly::pages(app, open, change, ask).await?;
    Ok((read + pages.of - pages.left, of + pages.of, pages))
}

/// How many of the files not read yet are read, with their ideas being
/// checked or the file being read plainly: at most `left`.
pub async fn checking(app: &Shared, opened: i64, left: u32) -> ApiResult<u32> {
    if left == 0 {
        return Ok(0);
    }
    let mut conn = app.db.acquire().await?;
    let n = kelve_store::asks::checking(&mut conn, opened).await?;
    Ok(u32::try_from(n).unwrap_or(0).min(left))
}

/// Reads on along the way in, now there's one, or more of it was asked
/// for: a change's files as it changed them, else a folder's, and the pages
/// of either.
pub async fn read_on(app: &Shared, open: &Open) -> ApiResult<()> {
    match open.kind() {
        OpenKind::Change => {
            let change = change(open)?;
            mark_change(app, open, &change).await?;
            crate::plainly::ask_pages(app, open).await
        }
        _ => read_ahead(app, open).await,
    }
}

/// Asks for a folder to be read: once, so one already waiting isn't
/// hurried.
pub async fn read_ahead(app: &Shared, open: &Open) -> ApiResult<()> {
    // A lot, and no way in: it's found first, with the folders' lines.
    crate::way::ask_way_first(app, open).await?;
    crate::way::ask_folders(app, open.row.id).await?;
    let key = format!("{READ_FOLDER}:{}", open.row.root);
    let mut conn = app.db.acquire().await?;
    kelve_store::jobs::add_once(
        &mut conn,
        READ_FOLDER,
        &key,
        &json!({ "opened": open.row.id }),
        Utc::now(),
    )
    .await?;
    app.wake.notify_one();
    Ok(())
}

/// Asks for every file of a folder not read, or changed since, to be read
/// by the person's agent, the files the rest lean on most first: a job for
/// each, which keeps a job for the agent. A file the person opens is moved
/// up the agent's list.
pub async fn read_folder_job(app: &Shared, payload: &serde_json::Value) -> pipeline::Result<Then> {
    let Some(id) = payload["opened"].as_i64() else {
        return Ok(Then::Done);
    };
    let Ok(open) = opened(app, id).await else {
        return Ok(Then::Done);
    };
    crate::read::load(app, &open).await;
    let only = open.only();
    // Some files, or one, are read as they are: ranking the whole of a large
    // repository first takes minutes.
    let named: Option<Vec<String>> = only
        .clone()
        .or_else(|| (!open.row.path.is_empty()).then(|| vec![open.row.path.clone()]));
    let order = match named {
        Some(paths) => hashed(&open.root, paths).await?,
        None => ranked(&open.root).await?,
    };
    // Its path, when there is one, is read in its order, ahead of the rest.
    let order: Ranked = match crate::way::view(app, &open).await {
        Some(way) if !way.order.is_empty() => {
            let path = crate::way::path_of(&way);
            let at = |p: &str| path.iter().position(|x| x == p).unwrap_or(usize::MAX);
            let mut ordered: Vec<(String, String)> = order.as_ref().clone();
            ordered.sort_by_key(|(p, _)| at(p));
            Arc::new(ordered)
        }
        _ => order,
    };
    // When there's a lot, only the way in and the parts the person asked
    // for; nothing while the way in is found.
    let now = crate::way::to_read_now(app, &open).await;
    if let Some(now) = &now
        && !now.is_empty()
    {
        crate::way::keep_to(app, &open, now).await?;
    }
    let read = read_files(app, &open.row.root).await?;
    let version = mark::read_version(app).await;
    let (wanted, _) = mark::lens_for(app);
    let written = crate::plainly::read_so_far(app).await?;
    let mut conn = app.db.acquire().await?;
    // Asked for again as the reader looks, so only a file found not to be
    // text is news: hearing it ran would ask again.
    let mut skipped = 0;
    let mut plain = Vec::new();
    let mut asking = Vec::new();
    for (path, hash) in order.iter() {
        if is_kelves(path)
            || only.as_ref().is_some_and(|o| !o.contains(path))
            || now.as_ref().is_some_and(|n| !n.contains(path))
        {
            continue;
        }
        match read.get(path) {
            Some(r) if r.skip.is_some() => continue,
            // Read already: read plainly, if it isn't, as part of reading it.
            Some(r) if r.hash.as_ref() == Some(&read_as(&version, &wanted, hash)) => {
                if !written.read(path, hash) {
                    plain.push(path.clone());
                }
                continue;
            }
            Some(r) if r.hash.is_none() && hash.is_empty() => continue,
            _ => {}
        }
        if hash.is_empty() {
            // Not text, say: found out only by reading it.
            if let Err(kelve_engine::read::ReadError::Skipped(skip)) =
                kelve_engine::read::read(&open.root, path)
            {
                kelve_store::reader::skipped_file(&mut conn, &open.row.root, path, skip.says())
                    .await?;
                skipped += 1;
            }
            continue;
        }
        asking.push(path);
    }
    // A lot to read and no way in: the agent finds one first, and only
    // that is read, until the person asks for more. Without one, as when the
    // person took it off the list, only what they open is read, never
    // everything.
    if now
        .as_ref()
        .is_some_and(std::collections::HashSet::is_empty)
    {
        let finding = crate::way::view(app, &open)
            .await
            .is_some_and(|w| w.finding);
        if !finding && !kelve_store::asks::dropped_of(&mut conn, agent::WAY, id).await? {
            drop(conn);
            crate::way::ask_way(app, id)
                .await
                .map_err(|e| Failed::Other(format!("couldn't ask for a way in: {e:?}")))?;
        }
        return Ok(Then::Done);
    }
    for path in asking {
        kelve_store::jobs::add(
            &mut conn,
            READ_FILE,
            Some(&format!("{READ_FILE}:{id}:{path}")),
            &json!({ "opened": id, "path": path, "priority": Priority::Rest as i64 }),
            Utc::now(),
        )
        .await?;
    }
    drop(conn);
    for path in plain {
        crate::plainly::already_read(app, id, &path).await?;
    }
    // Its folders' pages, read plainly once their files are.
    crate::plainly::ask_pages(app, &open)
        .await
        .map_err(|e| Failed::Other(format!("couldn't ask for its folders' pages: {e:?}")))?;
    app.wake.notify_one();
    if skipped > 0 {
        app.publish(ServerEvent::Read { opened: id });
    }
    Ok(Then::Done)
}

/// How long a folder's ranking is kept before it's ranked again: ranking
/// reads every file.
const RANKED_FOR: Duration = Duration::from_secs(10 * 60);

/// Each folder's files, most central first, each with a hash of its text
/// ("" when it can't be read), as ranked lately.
type Ranked = Arc<Vec<(String, String)>>;
static RANKINGS: LazyLock<Mutex<HashMap<PathBuf, (Instant, Ranked)>>> =
    LazyLock::new(Default::default);

/// Forgets a folder's ranking, so the next reading ranks it again and sees
/// what changed at once.
pub fn forget_ranking(root: &Path) {
    RANKINGS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(root);
}

/// Some files of a folder, with their texts' hashes ("" when one can't be
/// read), in the order given.
async fn hashed(root: &Path, paths: Vec<String>) -> pipeline::Result<Ranked> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        Arc::new(
            paths
                .into_iter()
                .map(|path| {
                    let hash = kelve_engine::read::read(&root, &path)
                        .map(|t| text_hash(&t.text))
                        .unwrap_or_default();
                    (path, hash)
                })
                .collect(),
        )
    })
    .await
    .map_err(|e| Failed::Other(e.to_string()))
}

/// A folder's files, most central first, with their texts' hashes: ranked
/// again only when the last ranking is old.
pub(crate) async fn ranked(root: &Path) -> pipeline::Result<Ranked> {
    if let Some((at, kept)) = RANKINGS.lock().unwrap_or_else(|p| p.into_inner()).get(root)
        && at.elapsed() < RANKED_FOR
    {
        return Ok(kept.clone());
    }
    let at = root.to_path_buf();
    let ranked: Ranked = tokio::task::spawn_blocking(move || {
        let Ok(walked) = kelve_engine::walk::walk(&at) else {
            return Arc::new(Vec::new());
        };
        let order = kelve_engine::central::order(&at, &walked.files);
        Arc::new(
            order
                .into_iter()
                .map(|path| {
                    let hash = kelve_engine::read::read(&at, &path)
                        .map(|t| text_hash(&t.text))
                        .unwrap_or_default();
                    (path, hash)
                })
                .collect(),
        )
    })
    .await
    .map_err(|e| Failed::Other(e.to_string()))?;
    RANKINGS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(root.to_path_buf(), (Instant::now(), ranked.clone()));
    Ok(ranked)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReadIdea {
    key: String,
    name: String,
    /// Its area and depth, as marked: what decides whom it's shown to.
    #[serde(default)]
    area: String,
    #[serde(default)]
    depth: kelve_core::lens::Depth,
}

/// Keeps the ideas a file's marks lead to, and, when the `whole` text was
/// marked, which ones the file is built on, and a hash of that text: a
/// change's lines alone aren't what the file is built on.
pub async fn keep_ideas(
    app: &Shared,
    open: &Open,
    path: &str,
    marks: &[Mark],
    whole: Option<&str>,
) -> pipeline::Result<()> {
    let mut conn = app.db.acquire().await?;
    let mut read = Vec::new();
    for m in marks {
        let key = key_of(m);
        if kelve_store::entries::idea(&mut conn, &key).await?.is_none() {
            kelve_store::entries::keep_idea(
                &mut conn,
                &Idea {
                    key: key.clone(),
                    name: m
                        .idea
                        .as_ref()
                        .map_or_else(|| m.name.clone(), |i| i.name.clone()),
                    definition: m.definition.clone(),
                    kind: IdeaKind::Concept.as_str().into(),
                    became: None,
                },
            )
            .await?;
        }
        read.push(ReadIdea {
            key,
            name: m
                .idea
                .as_ref()
                .map_or_else(|| m.name.clone(), |i| i.name.clone()),
            area: m.area.clone(),
            depth: m.depth,
        });
    }
    let Some(text) = whole else {
        return Ok(());
    };
    crate::lessons::refresh_soon(app, read.iter().map(|i| i.key.clone()).collect());
    let version = mark::read_version(app).await;
    let (wanted, _) = mark::lens_for(app);
    kelve_store::reader::read_file(
        &mut conn,
        &open.row.root,
        path,
        &serde_json::to_string(&read).unwrap_or_else(|_| "[]".into()),
        &read_as(&version, &wanted, &text_hash(text)),
    )
    .await?;
    // The libraries it imports: what lessons about a library are found by.
    let used = crate::lessons::not_own(&open.root, kelve_engine::candidates::libraries(path, text));
    kelve_store::lessons::keep_uses(&mut conn, &open.row.root, path, &used).await?;
    drop(conn);
    crate::lessons::look_up_soon(app, used);
    Ok(())
}

/// What a file was read as: its text's hash, under the instructions it was
/// read with and what was looked for, so a file read under earlier ones, or
/// for someone asking for less, is read again.
fn read_as(version: &str, lens: &kelve_core::lens::Lens, hash: &str) -> String {
    format!("{}:{hash}", read_under(version, lens))
}

/// How a file read under these instructions, looking for this, is recorded,
/// before its text's hash.
pub fn read_under(version: &str, lens: &kelve_core::lens::Lens) -> String {
    format!("{version}|{}", lens.print())
}

/// The hash of the text a file was read in, from how it's recorded.
pub fn read_text(hash: &str) -> Option<&str> {
    hash.rsplit_once(':').map(|(_, text)| text)
}

/// Whether a file of a folder, read, is done reading: read plainly too, or not
/// to be. One whose text isn't known counts as it's recorded.
pub fn read_through(read: &FileRead, path: &str, written: &crate::plainly::Written) -> bool {
    read.skip.is_some()
        || read
            .hash
            .as_deref()
            .and_then(read_text)
            .is_none_or(|text| written.read(path, text))
}

/// A hash of a file's text, to tell whether it changed since it was read.
pub fn text_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A file read in a folder: the ideas it's built on, or why it couldn't be
/// read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRead {
    pub ideas: Vec<IdeaTag>,
    pub skip: Option<String>,
    /// A hash of the text it held when it was read.
    pub hash: Option<String>,
}

/// Every file read in a folder, by path.
pub(crate) async fn read_files(
    app: &Shared,
    root: &str,
) -> kelve_store::Result<BTreeMap<String, FileRead>> {
    let mut conn = app.db.acquire().await?;
    let rows = kelve_store::reader::read_files(&mut conn, root).await?;
    // Only the ideas the person wants marked.
    let spec = crate::spec::read(app);
    Ok(rows
        .into_iter()
        .map(|r| {
            let ideas: Vec<ReadIdea> = serde_json::from_str(&r.ideas).unwrap_or_default();
            let ideas = ideas
                .into_iter()
                .filter(|i| spec.shows(&i.area, i.depth, &i.name))
                .map(|i| IdeaTag {
                    key: i.key,
                    name: i.name,
                })
                .collect();
            (
                r.path,
                FileRead {
                    ideas,
                    skip: r.skip,
                    hash: r.hash,
                },
            )
        })
        .collect())
}

/// The files read in a folder that are built on an idea.
pub async fn files_with(app: &Shared, open: &Open, key: &str) -> ApiResult<Vec<String>> {
    Ok(read_files(app, &open.row.root)
        .await?
        .into_iter()
        .filter(|(_, read)| read.ideas.iter().any(|i| i.key == key))
        .map(|(path, _)| path)
        .collect())
}

/// Every file read in a folder, by path.
pub async fn files_read(app: &Shared, open: &Open) -> ApiResult<BTreeMap<String, FileRead>> {
    Ok(read_files(app, &open.row.root).await?)
}

/// Whether a mark is on lines a change added.
pub fn on_added(mark: &Mark, added: &[Lines]) -> bool {
    added
        .iter()
        .any(|(a, b)| *a <= mark.end && *b >= mark.start)
}

/// What a change is built on: the ideas in its changed files, those new in the
/// change first; and each file's, by its path, once it's all marked. An idea is
/// new in the change when it's marked on a line the change added, and nowhere
/// else: not on a line it left as it was, and not in any file kelve has read
/// outside it.
pub async fn change_built_on(
    app: &Shared,
    open: &Open,
    change: &Change,
) -> ApiResult<(Vec<BuiltOn>, BTreeMap<String, Vec<IdeaTag>>)> {
    let marked = MARKED.load(std::sync::atomic::Ordering::Relaxed);
    let text = print(change);
    if let Ok(kept) = BUILT.lock()
        && let Some((m, t, built)) = kept.get(&open.row.id)
        && (*m, *t) == (marked, text)
    {
        return Ok(built.clone());
    }
    let built = built_on_now(app, open, change).await?;
    if let Ok(mut kept) = BUILT.lock() {
        kept.insert(open.row.id, (marked, text, built.clone()));
    }
    Ok(built)
}

async fn built_on_now(
    app: &Shared,
    open: &Open,
    change: &Change,
) -> ApiResult<(Vec<BuiltOn>, BTreeMap<String, Vec<IdeaTag>>)> {
    let read = read_files(app, &open.row.root).await?;
    let changed: BTreeSet<&str> = change.files.iter().map(|f| f.path.as_str()).collect();
    let mut before: BTreeSet<String> = read
        .iter()
        .filter(|(path, _)| !changed.contains(path.as_str()))
        .flat_map(|(_, read)| read.ideas.iter().map(|i| i.key.clone()))
        .collect();
    let mut added_here: BTreeSet<String> = BTreeSet::new();
    let mut files = Vec::new();
    for fc in &change.files {
        let Some(after) = &fc.after else {
            files.push((fc, Vec::new(), true));
            continue;
        };
        let only = added(fc);
        let (marks, whole) = mark::kept(app, &fc.path, after, None).await?;
        for m in &marks {
            if on_added(m, &only) {
                added_here.insert(key_of(m));
            } else {
                before.insert(key_of(m));
            }
        }
        files.push((fc, marks, whole));
    }
    let mut out: Vec<BuiltOn> = Vec::new();
    let mut each: BTreeMap<String, Vec<IdeaTag>> = BTreeMap::new();
    for (fc, marks, whole) in files {
        let mut tags = Vec::new();
        // What's new is told from every mark; what's listed is what the
        // person wants marked.
        for m in mark::shown(app, marks) {
            let key = key_of(&m);
            let name = m
                .idea
                .as_ref()
                .map_or_else(|| m.name.clone(), |i| i.name.clone());
            tags.push(IdeaTag {
                key: key.clone(),
                name: name.clone(),
            });
            if out.iter().any(|b| b.key == key) {
                continue;
            }
            out.push(BuiltOn {
                new: added_here.contains(&key) && !before.contains(&key),
                why: format!("In {}", fc.path),
                name,
                key,
            });
        }
        if whole {
            each.insert(fc.path.clone(), tags);
        }
    }
    // New to the project first, each group in the order it comes.
    out.sort_by_key(|b| !b.new);
    Ok((out, each))
}

/// Asks for a change's files to be marked, the first ones first, and read
/// plainly: how far reading them has got. A file is read once its ideas are
/// checked and it's read plainly.
pub async fn mark_change(app: &Shared, open: &Open, change: &Change) -> ApiResult<Reading> {
    // No way in: it's found first, with the folders' lines, and the reading
    // follows it: the way in, then each part the person asks for, in the
    // path's order; nothing while it's found.
    crate::way::ask_way_first(app, open).await?;
    crate::way::ask_folders(app, open.row.id).await?;
    let written = crate::plainly::read_so_far(app).await?;
    let plain = written.in_change(change);
    let (mut read, mut of) = (0, 0);
    let now = crate::way::to_read_now(app, open).await;
    let path: Vec<String> = match &now {
        Some(_) => crate::way::view(app, open)
            .await
            .map(|w| crate::way::path_of(&w))
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let at = |p: &str| path.iter().position(|x| x == p).unwrap_or(usize::MAX);
    let mut files: Vec<&FileChange> = change
        .files
        .iter()
        .filter(|f| f.after.is_some())
        .filter(|f| now.as_ref().is_none_or(|n| n.contains(&f.path)))
        .collect();
    files.sort_by_key(|f| at(&f.path));
    let most = if now.is_some() {
        usize::MAX
    } else {
        MARK_AHEAD
    };
    for fc in files.into_iter().take(most) {
        let Some(after) = &fc.after else { continue };
        of += 1;
        if !mark::kept(app, &fc.path, after, None).await?.1 {
            ask_diff(app, open.row.id, &fc.path).await?;
            continue;
        }
        if !plain.contains(&fc.path) || written.changed(fc) {
            read += 1;
            continue;
        }
        // Read before, as the change left it: read plainly now.
        crate::plainly::already_read(app, open.row.id, &fc.path).await?;
    }
    Ok(Reading {
        read,
        of,
        checking: checking(app, open.row.id, of - read).await?,
        marking: marking(read >= of),
        pages: 0,
        pages_left: 0,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Chosen {
    key: String,
    name: String,
    why: String,
}

/// The ideas marked in a folder's files, a fingerprint of them, and how
/// many files were read.
pub(crate) async fn folder_ideas(
    app: &Shared,
    open: &Open,
) -> kelve_store::Result<(Vec<(String, String, usize)>, String, usize)> {
    let read = read_files(app, &open.row.root).await?;
    // Each idea once, however many names it was marked under ("Idempotency
    // key", "Idempotency keys"): counted by key, named as it's marked most.
    let mut counts: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for file in read.values() {
        for i in &file.ideas {
            *counts
                .entry(i.key.clone())
                .or_default()
                .entry(i.name.clone())
                .or_default() += 1;
        }
    }
    let files = read.values().filter(|f| f.skip.is_none()).count();
    let ideas: Vec<(String, String, usize)> = counts
        .into_iter()
        .filter_map(|(key, names)| {
            let total = names.values().sum();
            let name = names
                .into_iter()
                .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))?
                .0;
            Some((key, name, total))
        })
        .collect();
    let mut hash = Sha256::new();
    hash.update(open.row.root.as_bytes());
    for (k, _, c) in &ideas {
        hash.update(k.as_bytes());
        hash.update(c.to_le_bytes());
    }
    let fingerprint: String = hash
        .finalize()
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((ideas, fingerprint, files))
}

fn chosen(json: &str) -> Vec<BuiltOn> {
    let chosen: Vec<Chosen> = serde_json::from_str(json).unwrap_or_default();
    chosen
        .into_iter()
        .map(|c| BuiltOn {
            key: c.key,
            name: c.name,
            why: c.why,
            new: false,
        })
        .collect()
}

/// What a folder is built on, as chosen: kept, or asked for. The ideas marked
/// in its files, chosen rather than counted. While the folder is `reading` in
/// the background, the last choice stands until the files read have doubled, so
/// it isn't chosen again after every file.
pub async fn folder_built_on(app: &Shared, open: &Open, reading: bool) -> ApiResult<Vec<BuiltOn>> {
    let (ideas, fingerprint, files) = folder_ideas(app, open).await?;
    if ideas.is_empty() {
        return Ok(Vec::new());
    }
    let mut conn = app.db.acquire().await?;
    if let Some(now) = kelve_store::reader::built_on(&mut conn, &fingerprint).await? {
        return Ok(chosen(&now));
    }
    let last = kelve_store::reader::last_built_on(&mut conn, &open.row.root).await?;
    drop(conn);
    let again = match &last {
        None => true,
        Some((_, then)) => !reading || files as i64 >= then * 2,
    };
    if again {
        ask(
            app,
            BUILT_ON,
            &format!("{BUILT_ON}:{}", open.row.id),
            json!({ "opened": open.row.id }),
        )
        .await?;
    }
    Ok(last.map(|(json, _)| chosen(&json)).unwrap_or_default())
}

#[derive(Deserialize)]
struct Choice {
    n: usize,
    why: String,
}

/// The shape of a built-on job's answer.
fn built_on_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "chosen": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "n": { "type": "integer" }, "why": { "type": "string" } },
                    "required": ["n", "why"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["chosen"],
        "additionalProperties": false,
    })
}

/// Whether a built-on job's answer is in its shape.
pub fn built_on_shape(value: &serde_json::Value) -> std::result::Result<(), String> {
    serde_json::from_value::<Vec<Choice>>(value["chosen"].clone())
        .map(|_| ())
        .map_err(|e| {
            format!("It isn't in the shape asked for: {e}. Give chosen, each with n and why.")
        })
}

/// The ideas a built-on job listed, as they were when it asked.
#[derive(Serialize, Deserialize)]
struct Asked {
    ideas: Vec<(String, String, usize)>,
    fingerprint: String,
    files: usize,
}

/// Chooses what a folder is built on, as a job, with the person's agent.
/// When the answer comes back, it's read against the ideas as they were
/// when it asked: files read since mustn't ask again, or shift the numbers
/// the answer chose by.
pub async fn built_on_job(app: &Shared, payload: &serde_json::Value) -> pipeline::Result<Then> {
    let Some(id) = payload["opened"].as_i64() else {
        return Ok(Then::Done);
    };
    let Ok(open) = opened(app, id).await else {
        return Ok(Then::Done);
    };
    let asked = match serde_json::from_value::<Asked>(payload["asked"].clone()) {
        Ok(asked) => asked,
        Err(_) => match folder_ideas(app, &open).await {
            Ok((ideas, fingerprint, files)) => Asked {
                ideas,
                fingerprint,
                files,
            },
            Err(_) => return Ok(Then::Done),
        },
    };
    let resume = json!({ "opened": id, "asked": &asked });
    let Asked {
        ideas,
        fingerprint,
        files,
    } = asked;
    if ideas.is_empty() {
        return Ok(Then::Done);
    }
    let listed: String = ideas
        .iter()
        .enumerate()
        .map(|(i, (_, name, count))| {
            format!(
                "{}. {name} (in {count} file{})\n",
                i + 1,
                if *count == 1 { "" } else { "s" }
            )
        })
        .collect();
    let folder = open
        .root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let version = agent::version(app, agent::BUILT_ON).await;
    let Some(answer) = agent::ask(
        app,
        &format!("builtOn:{version}:{fingerprint}"),
        agent::Ask {
            kind: agent::BUILT_ON,
            about: format!("Choosing what {folder} is built on"),
            idea: None,
            opened: Some(id),
            material: json!({
                "folder": folder,
                "ideas": fence("said", &listed),
            }),
            schema: built_on_schema(),
            priority: Priority::Now,
            follows: None,
        },
        agent::Resume::job(BUILT_ON, format!("{BUILT_ON}:{id}"), resume),
    )
    .await?
    else {
        return Ok(Then::Done);
    };
    let choices: Vec<Choice> = serde_json::from_value(answer.value["chosen"].clone())
        .map_err(|e| Failed::Other(format!("a built-on answer doesn't read: {e}")))?;
    // Only ideas it was given, each once, at most seven.
    let mut chosen: Vec<Chosen> = Vec::new();
    for c in choices {
        let Some((key, name, _)) = c.n.checked_sub(1).and_then(|i| ideas.get(i)) else {
            continue;
        };
        if chosen.len() == 7 || chosen.iter().any(|x| &x.key == key) {
            continue;
        }
        chosen.push(Chosen {
            key: key.clone(),
            name: name.clone(),
            why: kelve_core::text::clip(c.why.trim(), 140),
        });
    }
    let mut conn = app.db.acquire().await?;
    kelve_store::reader::keep_built_on(
        &mut conn,
        &fingerprint,
        &open.row.root,
        files as i64,
        &serde_json::to_string(&chosen).unwrap_or_else(|_| "[]".into()),
    )
    .await?;
    drop(conn);
    app.publish(ServerEvent::Read { opened: id });
    // What defines a folder is what's opened first: its entries taken
    // ahead from the shared graph, where it has them.
    take_ahead(app, chosen.iter().map(|c| c.key.as_str())).await?;
    Ok(Then::Done)
}

/// Takes ahead, from the shared graph, the entries for ideas the person is
/// likely to open next.
pub async fn take_ahead<'a>(
    app: &Shared,
    keys: impl Iterator<Item = &'a str>,
) -> pipeline::Result<()> {
    for key in keys {
        let idea = {
            let mut conn = app.db.acquire().await?;
            kelve_store::entries::idea(&mut conn, key).await?
        };
        if let Some(idea) = idea {
            entries::take_ahead(app, &idea).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- notes

/// Where the notes for a place are kept, for a project: with the work in
/// the project; the person's own, and those for their agent, in their own
/// notes folder.
fn notes_dir(app: &Shared, open: &Open, place: Place) -> PathBuf {
    match place {
        Place::Work => open.root.join(".kelve").join("notes"),
        Place::You | Place::Agent => own_folder(&own_notes(app), &open.row.root),
    }
}

/// Where notes kept only for the person go: the folder they chose, or
/// kelve's own.
pub fn own_notes(app: &Shared) -> PathBuf {
    own_notes_in(app, &app.settings())
}

/// The same, with these settings.
pub fn own_notes_in(app: &Shared, settings: &crate::settings::Settings) -> PathBuf {
    settings
        .notes_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| app.config.data.join("notes"))
}

/// The file in a project's folder of your own notes that says which
/// project it's for, so folders can move, and two projects of one name
/// stay apart.
const PROJECT_MARK: &str = ".kelve-project";

/// The folder in `base` for a project's own notes: named as the project
/// is, then, when another project has that name, with the folder above it,
/// then a short code. One already the project's, or free, is it.
fn own_folder(base: &Path, root: &str) -> PathBuf {
    let path = Path::new(root);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "notes".into());
    let above = path
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let code: String = Sha256::digest(root.as_bytes())
        .iter()
        .take(3)
        .map(|b| format!("{b:02x}"))
        .collect();
    let names = [
        name.clone(),
        format!("{name} ({above})"),
        format!("{name} ({code})"),
    ];
    for n in &names {
        let dir = base.join(n);
        match std::fs::read_to_string(dir.join(PROJECT_MARK)) {
            Ok(said) if said.trim() == root => return dir,
            Ok(_) => continue,
            Err(_) if dir.exists() => continue,
            Err(_) => return dir,
        }
    }
    base.join(&names[2])
}

/// Moves every project's folder of notes kept only for the person from one
/// folder to another, into the folder each project has there.
pub fn move_own_notes(from: &Path, to: &Path) -> std::io::Result<()> {
    let Ok(projects) = std::fs::read_dir(from) else {
        return Ok(());
    };
    std::fs::create_dir_all(to)?;
    for project in projects.filter_map(|e| e.ok()) {
        let dir = project.path();
        let Ok(root) = std::fs::read_to_string(dir.join(PROJECT_MARK)) else {
            continue;
        };
        let target = own_folder(to, root.trim());
        std::fs::create_dir_all(&target)?;
        for file in std::fs::read_dir(&dir)?.filter_map(|e| e.ok()) {
            let name = file.file_name().to_string_lossy().into_owned();
            let mut at = target.join(&name);
            // A note of the same name there already: kept beside it.
            if at.exists() && name != PROJECT_MARK {
                let stem = name.strip_suffix(".md").unwrap_or(&name).to_string();
                let mut n = 2;
                while at.exists() {
                    at = target.join(format!("{stem}-{n}.md"));
                    n += 1;
                }
            }
            if std::fs::rename(file.path(), &at).is_err() {
                // Across disks: copied, then let go.
                std::fs::copy(file.path(), &at)?;
                std::fs::remove_file(file.path())?;
            }
        }
        std::fs::remove_dir(&dir).ok();
    }
    Ok(())
}

/// A file name for a note: lower-case words and hyphens.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 60 {
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// A note's title: its first line with words, without its markdown marks,
/// or its file's name.
fn title_of(text: &str, file: &str) -> String {
    text.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .map(|t| {
            let t: String = t.chars().take(80).collect();
            t.replace(['*', '`', '_'], "")
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| file.trim_end_matches(".md").replace('-', " "))
}

fn place_word(place: Place) -> &'static str {
    match place {
        Place::You => "you",
        Place::Work => "work",
        Place::Agent => "agent",
    }
}

/// The file a note's id names, if it's a note kelve keeps: `work/ledger.md`.
fn note_path(app: &Shared, open: &Open, id: &str) -> Option<(Place, PathBuf, String)> {
    let (place, file) = id.split_once('/')?;
    let place = match place {
        "you" => Place::You,
        "work" => Place::Work,
        "agent" => Place::Agent,
        _ => return None,
    };
    let stem = file.strip_suffix(".md")?;
    if stem.is_empty() || slug(stem) != stem {
        return None;
    }
    Some((
        place,
        notes_dir(app, open, place).join(file),
        file.to_string(),
    ))
}

/// A note, by where it's kept, from its file's text: its passage found
/// again in the file as it is now, when `root` is the project it's in.
fn note_ref(root: Option<&Path>, place: Place, file: &str, path: &Path, text: &str) -> NoteRef {
    let (front, body) = kelve_core::notes::split(text);
    let on = front.passage.as_ref().and_then(|p| {
        let (start, end) = p.range()?;
        let now = root
            .and_then(|root| kelve_engine::read::read(root, &p.path).ok())
            .and_then(|read| p.find(&read.text.lines().collect::<Vec<_>>()));
        Some(NoteOn {
            path: p.path.clone(),
            start: now.map_or(start, |n| n.0),
            end: now.map_or(end, |n| n.1),
            found: now.is_some(),
        })
    });
    // An empty note, as one cleared for fresh notes, waits for nothing.
    // Done for other words than it has now, it waits again.
    let state = (front.for_agent && (front.done || !body.trim().is_empty())).then_some(
        if front.waiting(body) {
            NoteState::Waiting
        } else {
            NoteState::Done
        },
    );
    NoteRef {
        id: format!("{}/{file}", place_word(place)),
        title: title_of(body, file),
        place,
        file: shown(path),
        ideas: front.ideas,
        on,
        state,
        did: front.did.unwrap_or_default(),
        done_at: front
            .done_at
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(&at).ok())
            .map(|at| at.with_timezone(&Utc).into()),
        opened: front.opened,
    }
}

/// The notes in a folder, each with the place its front matter says: one
/// for the agent in the person's own folder is the agent's.
fn notes_in(root: Option<&Path>, dir: &Path, own: bool) -> Vec<(NoteRef, String)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, PathBuf)> = read
        .filter_map(|e| e.ok())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .filter(|(name, path)| name.ends_with(".md") && path.is_file())
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|(name, path)| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let place = match (own, kelve_core::notes::split(&text).0.for_agent) {
                (false, _) => Place::Work,
                (true, true) => Place::Agent,
                (true, false) => Place::You,
            };
            (note_ref(root, place, &name, &path, &text), text)
        })
        .collect()
}

/// The notes for what's open: for the agent, with the work, and the
/// person's own.
pub fn notes(app: &Shared, open: &Open) -> Vec<NoteRef> {
    let mut out: Vec<NoteRef> =
        notes_in(Some(&open.root), &notes_dir(app, open, Place::Work), false)
            .into_iter()
            .chain(notes_in(
                Some(&open.root),
                &notes_dir(app, open, Place::You),
                true,
            ))
            .map(|(n, _)| n)
            .collect();
    let rank = |p: Place| match p {
        Place::Agent => 0,
        Place::Work => 1,
        Place::You => 2,
    };
    out.sort_by_key(|n| rank(n.place));
    out
}

pub fn note(app: &Shared, open: &Open, id: &str) -> ApiResult<Note> {
    let (place, path, file) =
        note_path(app, open, id).ok_or_else(|| ApiError::not_found("There's no such note."))?;
    let text = std::fs::read_to_string(&path)
        .map_err(|_| ApiError::not_found("There's no such note any more."))?;
    Ok(Note {
        note: note_ref(Some(&open.root), place, &file, &path, &text),
        text: kelve_core::notes::split(&text).1.to_string(),
    })
}

/// Deletes a note.
pub fn forget_note(app: &Shared, open: &Open, id: &str) -> ApiResult<()> {
    let (_, path, _) =
        note_path(app, open, id).ok_or_else(|| ApiError::not_found("There's no such note."))?;
    std::fs::remove_file(&path)
        .map_err(|_| ApiError::not_found("There's no such note any more."))?;
    app.publish(ServerEvent::Notes);
    Ok(())
}

/// The person's own notes on an idea, in every project: found by its id,
/// across every project's folder in their notes folder. Never sent anywhere.
pub fn notes_on_idea(app: &Shared, key: &str) -> Vec<Note> {
    let Ok(projects) = std::fs::read_dir(own_notes(app)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for project in projects.filter_map(|e| e.ok()) {
        let dir = project.path();
        if !dir.join(PROJECT_MARK).exists() {
            continue;
        }
        for (note, text) in notes_in(None, &dir, true) {
            if note.place == Place::You && note.ideas.iter().any(|i| i == key) {
                out.push(Note {
                    note,
                    text: kelve_core::notes::split(&text).1.to_string(),
                });
            }
        }
    }
    out
}

/// The project a folder is in, among those with notes in the person's
/// folder: its root, and its folder of notes. The longest root the folder
/// is in wins.
fn own_project_for(app: &Shared, folder: &Path) -> Option<(PathBuf, PathBuf)> {
    let projects = std::fs::read_dir(own_notes(app)).ok()?;
    projects
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let root = std::fs::read_to_string(e.path().join(PROJECT_MARK)).ok()?;
            let root = PathBuf::from(root.trim());
            // As Windows tells folders apart too: an agent's `C:\…` is
            // the `\\?\C:\…` kelve keeps.
            crate::lessons::within(folder, &root).then(|| (root, e.path()))
        })
        .max_by_key(|(root, _)| root.as_os_str().len())
}

/// A note left for the agent, as it reads it: fenced, as material.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForAgent {
    pub id: String,
    /// Its first line, as the list shows it: never sent.
    #[serde(skip)]
    pub title: String,
    /// What the person asked, fenced: do it only as part of their work.
    pub note: String,
    /// The version of its words: given back with `note_done`, so a note
    /// changed since it was read isn't marked done.
    pub version: String,
    /// The file it's on, and its lines as they are now; as they were when
    /// it was left, when they've changed since.
    pub file: Option<String>,
    pub lines: Option<(u32, u32)>,
    pub text_now: Option<String>,
    /// Said when the lines it's on have changed since it was left.
    pub lines_changed: Option<&'static str>,
    /// The other notes waiting for the agent on those lines, by id.
    pub on_the_same_lines: Vec<String>,
    /// The ideas it's about, by name.
    pub ideas: Vec<String>,
    /// Done before, for other words: what it said then, and what the agent
    /// did, so it does only what's new.
    pub done_before: Option<DoneBefore>,
    /// The lines added since it was done.
    pub added: Vec<String>,
    /// Its words, with secrets out, as the job was asked for: never sent.
    #[serde(skip)]
    pub body: String,
    /// When the agent last said it was done, in milliseconds: never sent.
    #[serde(skip)]
    pub done_at: Option<i64>,
    /// When it last changed: never sent.
    #[serde(skip)]
    pub changed: Option<std::time::SystemTime>,
}

/// What a note said when the agent last did it, and what it did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneBefore {
    /// Fenced, as the note is.
    pub text: String,
    pub did: String,
}

/// Said when the lines a note is on have changed since it was left.
pub const LINES_CHANGED: &str = "These lines have changed since the note was left.";

/// Whether two notes are on lines of the same file that overlap.
fn same_lines(a: &NoteOn, b: &NoteOn) -> bool {
    a.path == b.path && a.start <= b.end && b.start <= a.end
}

/// The notes waiting for the agent in the project a folder is in.
pub async fn waiting_for_agent(app: &Shared, folder: &Path) -> ApiResult<Vec<ForAgent>> {
    let Some((root, dir)) = own_project_for(app, folder) else {
        return Ok(Vec::new());
    };
    let all = notes_in(Some(&root), &dir, true);
    let mut out = Vec::new();
    for (n, text) in &all {
        if n.state != Some(NoteState::Waiting) {
            continue;
        }
        let (front, body) = kelve_core::notes::split(text);
        let redacted = kelve_core::redact::redact(body);
        let mut ideas = Vec::new();
        for key in &n.ideas {
            let mut conn = app.db.acquire().await?;
            if let Some(i) = kelve_store::entries::idea(&mut conn, key).await? {
                ideas.push(i.name);
            }
        }
        let text_now = n.on.as_ref().filter(|on| on.found).and_then(|on| {
            let read = kelve_engine::read::read(&root, &on.path).ok()?;
            let lines: Vec<&str> = read.text.lines().collect();
            let (a, b) = (on.start as usize, on.end as usize);
            (a >= 1 && b <= lines.len()).then(|| lines[a - 1..b].join("\n"))
        });
        // Done before for other words: what it said then, and what's new.
        let before = front
            .done_for
            .as_deref()
            .filter(|_| front.did.is_some())
            .map(kelve_core::redact::redact);
        let added = before
            .as_deref()
            .map(|b| kelve_core::notes::added(b, &redacted))
            .unwrap_or_default();
        let done_before = before.map(|b| DoneBefore {
            text: kelve_core::fence::fence("note", &b),
            did: front.did.clone().unwrap_or_default(),
        });
        let on_the_same_lines =
            n.on.as_ref()
                .map(|on| {
                    all.iter()
                        .filter(|(o, _)| o.id != n.id && o.state == Some(NoteState::Waiting))
                        .filter(|(o, _)| o.on.as_ref().is_some_and(|x| same_lines(on, x)))
                        .map(|(o, _)| o.id.clone())
                        .collect()
                })
                .unwrap_or_default();
        out.push(ForAgent {
            id: n.id.clone(),
            title: n.title.clone(),
            note: kelve_core::fence::fence("note", &redacted),
            version: kelve_core::notes::version(body),
            file: n.on.as_ref().map(|on| on.path.clone()),
            lines: n.on.as_ref().map(|on| (on.start, on.end)),
            text_now: text_now.map(|t| kelve_core::fence::fence("lines", &t)),
            lines_changed: n
                .on
                .as_ref()
                .is_some_and(|on| !on.found)
                .then_some(LINES_CHANGED),
            on_the_same_lines,
            ideas,
            done_before,
            added,
            body: redacted.trim().to_string(),
            done_at: n.done_at.as_ref().map(|at| at.0.timestamp_millis()),
            changed: n
                .id
                .rsplit('/')
                .next()
                .and_then(|file| std::fs::metadata(dir.join(file)).ok())
                .and_then(|m| m.modified().ok()),
        });
    }
    Ok(out)
}

/// The notes waiting for an agent that it hasn't been told of as they are
/// now, by its hook: each is told of now.
pub async fn new_for_agent(app: &Shared, folder: &Path, agent: &str) -> ApiResult<usize> {
    let waiting = waiting_for_agent(app, folder).await?;
    let root = root_of_notes(app, folder).display().to_string();
    let mut conn = app.db.acquire().await?;
    let mut new = 0;
    for n in &waiting {
        if kelve_store::asks::tell(&mut conn, agent, &root, &n.id, &n.version).await? {
            new += 1;
        }
    }
    Ok(new)
}

/// The notes the agent did since a time, in the project a folder is in,
/// each with what it said it did.
pub fn done_for_agent(
    app: &Shared,
    folder: &Path,
    since: chrono::DateTime<Utc>,
) -> Vec<(NoteRef, String)> {
    let Some((root, dir)) = own_project_for(app, folder) else {
        return Vec::new();
    };
    notes_in(Some(&root), &dir, true)
        .into_iter()
        .filter(|(n, _)| {
            n.place == Place::Agent
                && n.state == Some(NoteState::Done)
                && n.done_at.as_ref().is_some_and(|at| at.0 > since)
        })
        .map(|(n, _)| {
            let did = n.did.clone();
            (n, did)
        })
        .collect()
}

/// What the agent opened, for the notes it said were done in the last
/// half hour that don't lead anywhere yet.
pub fn link_done_notes(app: &Shared, folder: &Path, opened: i64) {
    let Some((_, dir)) = own_project_for(app, folder) else {
        return;
    };
    let since = Utc::now() - chrono::Duration::minutes(30);
    for (n, text) in notes_in(None, &dir, true) {
        let recent = n.done_at.as_ref().is_some_and(|at| at.0 > since);
        if n.state != Some(NoteState::Done) || n.opened.is_some() || !recent {
            continue;
        }
        let (mut front, body) = kelve_core::notes::split(&text);
        front.opened = Some(opened);
        let file = n.id.trim_start_matches("agent/");
        if std::fs::write(dir.join(file), kelve_core::notes::join(&front, body)).is_err() {
            tracing::warn!("couldn't keep what the agent opened with a note it did");
        }
    }
    app.publish(ServerEvent::Notes);
}

/// Said when a note changed since the agent read it.
pub const NOTE_CHANGED: &str = "This note changed since you read it: read it again with notes.";

/// For which words the agent says a note is done.
#[derive(Debug, Clone, Copy)]
pub enum DoneFor<'a> {
    /// The version `notes` gave: refused when the note has changed since.
    Read(&'a str),
    /// A job's answer, for the version and words it was asked for: when the
    /// note has changed since, kept against those words, and the note
    /// waits for the agent again with what's new.
    Job { version: &'a str, text: &'a str },
}

/// The agent says a note is done, for the words it read, and what it did.
pub async fn note_done(
    app: &Shared,
    folder: &Path,
    id: &str,
    did: &str,
    opened: Option<i64>,
    done_for: DoneFor<'_>,
) -> ApiResult<()> {
    let (_, dir) = own_project_for(app, folder)
        .ok_or_else(|| ApiError::not_found("There are no notes for you in this project."))?;
    let file = id
        .strip_prefix("agent/")
        .filter(|f| {
            f.ends_with(".md") && slug(f.trim_end_matches(".md")) == f.trim_end_matches(".md")
        })
        .ok_or_else(|| {
            ApiError::not_found("There's no such note for you. Use the id the notes tool gave.")
        })?;
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path)
        .map_err(|_| ApiError::not_found("There's no such note any more."))?;
    let (mut front, body) = kelve_core::notes::split(&text);
    if !front.for_agent {
        return Err(ApiError::not_found(
            "There's no such note for you. Use the id the notes tool gave.",
        ));
    }
    let did = kelve_core::redact::redact(&did.split_whitespace().collect::<Vec<_>>().join(" "));
    if did.is_empty() || did.chars().count() > 300 {
        return Err(ApiError::bad_request(
            "Say what you did in one line, at most 300 characters.",
        ));
    }
    let now = kelve_core::notes::version(body);
    let (version, text) = match done_for {
        DoneFor::Read(v) if v.trim() != now => {
            return Err(ApiError::conflict(NOTE_CHANGED));
        }
        DoneFor::Read(_) => (now.clone(), kelve_core::redact::redact(body.trim())),
        // Done already as it is now: an answer for older words adds nothing.
        DoneFor::Job { version, .. } if version != now && front.done_now(body) => {
            return Ok(());
        }
        DoneFor::Job { version, text } => (version.to_string(), text.to_string()),
    };
    let current = version == now;
    front.done = current;
    front.done_hash = Some(version);
    front.done_for = Some(text);
    front.did = Some(did);
    front.done_at = Some(Utc::now().to_rfc3339());
    front.opened = opened;
    std::fs::write(&path, kelve_core::notes::join(&front, body)).map_err(ApiError::internal)?;
    // Done as it is now: its jobs, if they're still on the list, come off,
    // before it's said to be done: come off later, they could take the job
    // asked when its words change back. Done for older words, the job for
    // these stays.
    if current {
        let prefix = crate::list::note_prefix(&root_of_notes(app, folder), id);
        if let Ok(mut conn) = app.db.acquire().await {
            let _ = kelve_store::asks::drop_like(&mut conn, &prefix, None, true).await;
        }
        app.publish(ServerEvent::Asks);
    }
    app.publish(ServerEvent::Notes);
    Ok(())
}

/// The project a folder is in, as its notes go by.
pub(crate) fn root_of_notes(app: &Shared, folder: &Path) -> PathBuf {
    own_project_for(app, folder).map_or_else(|| folder.to_path_buf(), |(root, _)| root)
}

/// What a new note keeps in its front matter: the passage it's on, read
/// from the file now, and the ideas it's about.
fn front_for(open: &Open, on: Option<&LinesIn>, ideas: &[String]) -> ApiResult<Front> {
    let passage = match on {
        None => None,
        Some(on) => {
            let read = kelve_engine::read::read(&open.root, &on.path)
                .map_err(|_| ApiError::bad_request("kelve can't read the file that note is on."))?;
            let lines: Vec<&str> = read.text.lines().collect();
            Some(
                Passage::of(&on.path, &lines, on.start, on.end).ok_or_else(|| {
                    ApiError::bad_request("Those lines aren't in the file. Choose them again.")
                })?,
            )
        }
    };
    Ok(Front {
        ideas: ideas.iter().take(20).cloned().collect(),
        passage,
        ..Front::default()
    })
}

/// A free file name in `dir` for a note with this text.
fn free_name(dir: &Path, text: &str) -> String {
    let base = match slug(&title_of(text, "")) {
        s if s.is_empty() => "note".to_string(),
        s => s,
    };
    let mut name = format!("{base}.md");
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{base}-{n}.md");
        n += 1;
    }
    name
}

/// Keeps a note: a new one under a name from its title, with the passage
/// and ideas it's about, or one there is, keeping what its front matter
/// says. A note given a new place moves there. `text` is the note's body.
pub fn keep_note(app: &Shared, open: &Open, note: &NoteIn) -> ApiResult<Note> {
    let (id, place, text) = (note.id.as_deref(), note.place, note.text.as_str());
    if text.len() > 1_000_000 {
        return Err(ApiError::bad_request(
            "That note is too long to keep. Split it into two.",
        ));
    }
    let was = match id {
        Some(id) => Some(
            note_path(app, open, id).ok_or_else(|| ApiError::not_found("There's no such note."))?,
        ),
        None => None,
    };
    let mut front = match &was {
        Some((_, path, _)) => {
            let kept = std::fs::read_to_string(path).unwrap_or_default();
            let (mut front, body) = kelve_core::notes::split(&kept);
            if front.for_agent {
                // Done before kelve kept for which words: for these.
                if front.done && front.done_hash.is_none() {
                    front.done_hash = Some(kelve_core::notes::version(body));
                    front.done_for = Some(kelve_core::redact::redact(body.trim()));
                }
                if text.trim().is_empty() && !body.trim().is_empty() {
                    // Cleared for fresh notes: what was done goes.
                    front.forget_done();
                } else {
                    // Written in again, it waits for the agent, with what
                    // it did before kept, so it does only what's new;
                    // written back as it was done, it's done again.
                    front.done = front.done_now(text);
                }
            }
            front
        }
        None => front_for(
            open,
            note.on.as_ref(),
            note.ideas.as_deref().unwrap_or_default(),
        )?,
    };
    // For the agent, it waits for it again; not for it, it's the person's.
    if (place == Place::Agent) != front.for_agent {
        front.for_agent = place == Place::Agent;
        front.forget_done();
    }
    let dir = notes_dir(app, open, place);
    let (path, file) = match &was {
        Some((_, path, file)) if path.parent() == Some(dir.as_path()) => {
            (path.clone(), file.clone())
        }
        _ => {
            let file = free_name(&dir, text);
            (dir.join(&file), file)
        }
    };
    let whole = kelve_core::notes::join(&front, text);
    std::fs::create_dir_all(&dir).map_err(ApiError::internal)?;
    // A project's folder of your own notes says which project it's for.
    if place != Place::Work && !dir.join(PROJECT_MARK).exists() {
        std::fs::write(dir.join(PROJECT_MARK), &open.row.root).map_err(ApiError::internal)?;
    }
    std::fs::write(&path, &whole).map_err(ApiError::internal)?;
    // Moved: the old file goes.
    if let Some((_, old, _)) = &was
        && old != &path
    {
        std::fs::remove_file(old).ok();
    }
    app.publish(ServerEvent::Notes);
    // Once it's been left alone, it's on the agent's list: the reader's
    // told then.
    if place == Place::Agent {
        let app = app.clone();
        tokio::spawn(async move {
            tokio::time::sleep(crate::list::SETTLE + std::time::Duration::from_secs(1)).await;
            app.publish(ServerEvent::Notes);
        });
    }
    Ok(Note {
        note: note_ref(Some(&open.root), place, &file, &path, &whole),
        text: text.to_string(),
    })
}

/// A note left for the agent, made the person's own: it was taken off the
/// agent's list, so the agent isn't told of it. Its words, the lines it's
/// on and its ideas stay.
pub async fn note_to_you(app: &Shared, opened_id: i64, id: &str) -> ApiResult<()> {
    let open = opened(app, opened_id).await?;
    let Some((Place::Agent, path, _)) = note_path(app, &open, id) else {
        return Ok(());
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let body = kelve_core::notes::split(&text).1.to_string();
    keep_note(
        app,
        &open,
        &NoteIn {
            id: Some(id.to_string()),
            place: Place::You,
            text: body,
            on: None,
            ideas: None,
        },
    )?;
    Ok(())
}

/// How long a search waits for the shared graph before answering without it.
pub const GRAPH_WAIT: Duration = Duration::from_secs(2);

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(start: u32, end: u32) -> Mark {
        Mark {
            start,
            end,
            span: None,
            name: "Jitter".into(),
            definition: String::new(),
            idea: None,
            use_: String::new(),
            area: String::new(),
            depth: Default::default(),
        }
    }

    #[test]
    fn a_mark_is_on_added_lines_when_any_of_it_is() {
        let added = [(5, 7), (20, 20)];
        assert!(on_added(&mark(6, 6), &added));
        assert!(
            on_added(&mark(1, 5), &added),
            "touching the first added line"
        );
        assert!(on_added(&mark(18, 25), &added), "a run around one");
        assert!(!on_added(&mark(8, 19), &added), "between them");
        assert!(!on_added(&mark(1, 4), &[]), "nothing added");
    }

    #[test]
    fn changes_are_read_by_their_words() {
        assert_eq!(which("uncommitted"), Some(Which::Uncommitted));
        assert_eq!(which("against main"), Some(Which::Against("main".into())));
        assert_eq!(
            which("commit 3f2a9c1"),
            Some(Which::Commit("3f2a9c1".into()))
        );
        assert_eq!(which("anything else"), None);
    }

    #[test]
    fn note_names_are_plain() {
        assert_eq!(
            slug("Spending ledger: what to check!"),
            "spending-ledger-what-to-check"
        );
        assert_eq!(
            title_of("# Spending ledger\n\nText", "x.md"),
            "Spending ledger"
        );
        assert_eq!(
            title_of("Make the **cap** a setting.\nMore", "x.md"),
            "Make the cap a setting."
        );
        assert_eq!(title_of("\n\n", "rate-limits.md"), "rate limits");
    }

    #[test]
    fn paths_are_read_as_people_write_them() {
        assert_eq!(unshown("/srv/kelve"), PathBuf::from("/srv/kelve"));
        assert_eq!(unshown("~someone/kelve"), PathBuf::from("~someone/kelve"));
        if let Some(home) = std::env::home_dir().map(|h| h.display().to_string())
            && !home.is_empty()
        {
            assert_eq!(
                unshown("~/projects/kelve"),
                PathBuf::from(format!("{home}/projects/kelve"))
            );
            assert_eq!(unshown("~"), PathBuf::from(home));
        }
    }
}
