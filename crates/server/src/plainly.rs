//! Plainly: a file, a folder and a change, each read in one shape, in plain
//! words, as the person's agent writes it, and checked in code
//! (`core::plainly`) before kelve keeps it.
//!
//! - **A file** is read plainly as soon as its reading is handed in, for
//!   whatever was opened, straight after it: part of reading it, so it
//!   counts as read only then (`Written`). One shown before it is, is
//!   asked for then, ahead of the rest.
//! - **A folder** is read as part of reading what's open: for a change,
//!   each folder holding a file it reads, up to the top; for a folder,
//!   each folder in it with files kelve reads (`folders_of`). Each is
//!   written from its parts as they were read plainly, bottom up: after
//!   its files, and after the folders in it. One shown is asked for then.
//! - **A change** is read when it's opened, written from its files, the
//!   last of reading it.
//!
//! It's the person's work, so none of it goes to the shared graph. It's
//! kept in kelve's store, by what it's about and what it was written from,
//! under the instructions' version: written once for each, and reused
//! wherever that is opened.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use kelve_core::fence::fence;
use kelve_core::plainly::{self as core, ChangeGiven, Diff, FileGiven, FolderGiven, Marked};
use kelve_core::spec::ReadAs;
use kelve_core::views::{
    ChangeKind, ChangePlainly, FolderPlainly, PlainChange, PlainFile, PlainWaiting, PlainlyView,
    ServerEvent,
};
use kelve_engine::git::{Change, FileChange, LineKind, Status};
use serde_json::{Value, json};

use crate::agent::{self, Priority};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{self, Failed, Then};
use crate::reader::{self, Open};
use crate::state::Shared;

/// Reading a file plainly, as a job: `{opened, path, since, priority}`.
pub const JOB: &str = "plainly.file";
/// Reading a folder plainly: `{opened, path, since, priority}`.
pub const FOLDER_JOB: &str = "plainly.folder";
/// Reading a change plainly: `{opened, since, priority}`.
pub const CHANGE_JOB: &str = "plainly.change";

/// The most of a file's lines the agent is given.
const MOST_LINES: usize = 3_000;
/// How long a file waits to be read, or a folder or a change for its files
/// to be read plainly, before it's written from what there is: the agent
/// may not be working.
const WAIT: i64 = 10 * 60 * 1000;
/// The most of a change's lines given for a file not read plainly yet.
const DIFF_START: usize = 24;

/// How soon a job's payload says it's wanted.
fn priority(payload: &Value) -> Priority {
    match payload["priority"].as_i64() {
        Some(0) => Priority::Now,
        Some(1) => Priority::Kept,
        Some(3) => Priority::Later,
        _ => Priority::Rest,
    }
}

/// What a folder's or a change's job was asked with, when it was: the
/// print of its material then, so the agent's answer to it is kept for
/// that; else what it's given now.
fn asked_with(payload: &Value, now: &str) -> String {
    payload["print"].as_str().unwrap_or(now).to_string()
}

/// Whether what a job waits for has waited long enough.
fn waited(payload: &Value) -> bool {
    Utc::now().timestamp_millis() - payload["since"].as_i64().unwrap_or(0) >= WAIT
}

fn print(text: &str) -> String {
    kelve_core::text::hash(text)
}

/// Where a page's job is on the agent's list, by its key: with it now,
/// or how many files are read before it; none on it yet while what it's
/// written from is still read.
async fn waiting(app: &Shared, key: &str) -> ApiResult<PlainWaiting> {
    let mut conn = app.db.acquire().await?;
    Ok(
        match kelve_store::asks::files_before(&mut conn, key).await? {
            Some((ask, writing, before)) => PlainWaiting {
                writing,
                reading: false,
                before: Some(before),
                ask: Some(ask),
            },
            None => PlainWaiting {
                writing: false,
                reading: false,
                before: None,
                ask: None,
            },
        },
    )
}

async fn kept<T: serde::de::DeserializeOwned>(
    app: &Shared,
    kind: &str,
    at: &str,
    print: &str,
    version: &str,
) -> pipeline::Result<Option<T>> {
    let mut conn = app.db.acquire().await?;
    Ok(
        kelve_store::reader::kept_plainly(&mut conn, kind, at, print, version)
            .await?
            .and_then(|j| serde_json::from_str(&j).ok()),
    )
}

async fn latest<T: serde::de::DeserializeOwned>(
    app: &Shared,
    kind: &str,
    at: &str,
    version: &str,
) -> pipeline::Result<Option<T>> {
    let mut conn = app.db.acquire().await?;
    Ok(
        kelve_store::reader::latest_plainly(&mut conn, kind, at, version)
            .await?
            .and_then(|j| serde_json::from_str(&j).ok()),
    )
}

async fn keep<T: serde::Serialize>(
    app: &Shared,
    kind: &str,
    at: &str,
    print: &str,
    version: &str,
    kept: &T,
) -> pipeline::Result<()> {
    let json = serde_json::to_string(kept).unwrap_or_default();
    let mut conn = app.db.acquire().await?;
    kelve_store::reader::keep_plainly(&mut conn, kind, at, print, version, &json).await?;
    Ok(())
}

/// Asks for a job, under a key, once: one already waiting keeps when it
/// was asked, so how long it has waited stands.
async fn queue(app: &Shared, kind: &str, key: &str, payload: Value) -> pipeline::Result<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::jobs::add_once(&mut conn, kind, key, &payload, Utc::now()).await?;
    drop(conn);
    app.wake.notify_one();
    Ok(())
}

// ---------------------------------------------------------------- a file

/// A file as it's read plainly: as it is, or, in a change, as the change
/// left it, with what it was.
#[derive(Debug, Clone)]
struct FileText {
    text: String,
    lang: Option<&'static str>,
    /// In a change: what it was, its file's change, and what it did.
    change: Option<(Option<String>, FileChange, Diff)>,
}

impl FileText {
    fn hash(&self) -> String {
        reader::text_hash(&self.text)
    }

    /// The hash of what it was in a change, or "".
    fn base(&self) -> String {
        match &self.change {
            Some((Some(before), _, _)) => reader::text_hash(before),
            Some((None, _, _)) => "new".into(),
            None => String::new(),
        }
    }

    fn code(&self) -> bool {
        self.lang.is_some_and(|l| l != "markdown")
    }
}

/// What a change did to a file, by its lines after it.
fn diff_of(before: &str, after: &str) -> Diff {
    let mut diff = Diff::default();
    let mut last = 0u32;
    for line in kelve_engine::git::whole(before, after).lines {
        match (line.kind, line.new) {
            (LineKind::Added, Some(n)) => {
                diff.added.insert(n);
                last = n;
            }
            (LineKind::Context, Some(n)) => last = n,
            (LineKind::Removed, _) => {
                diff.out_after.insert(last);
            }
            _ => {}
        }
    }
    diff
}

/// A file of what's open, as it's read plainly.
fn file_text(open: &Open, change: Option<&Change>, path: &str) -> ApiResult<FileText> {
    if reader::is_kelves(path) {
        return Err(ApiError::not_found("That isn't in what's open."));
    }
    if let Some(fc) = change.and_then(|c| c.files.iter().find(|f| f.path == path)) {
        let Some(after) = fc.after.clone() else {
            return Err(ApiError::bad_request(match fc.skip {
                Some(skip) => skip.says().to_string(),
                None => "The change took that file out: there's nothing of it to read.".into(),
            }));
        };
        let diff = diff_of(fc.before.as_deref().unwrap_or(""), &after);
        return Ok(FileText {
            text: after,
            lang: kelve_core::code::language(path),
            change: Some((fc.before.clone(), fc.clone(), diff)),
        });
    }
    match kelve_engine::read::read(&open.root, path) {
        Ok(read) => Ok(FileText {
            text: read.text,
            lang: read.lang,
            change: None,
        }),
        Err(kelve_engine::read::ReadError::Skipped(skip)) => {
            Err(ApiError::bad_request(skip.says()))
        }
        Err(_) => Err(ApiError::not_found("That isn't there any more.")),
    }
}

/// What's open's change, when it's one.
fn change_of(open: &Open) -> Option<Arc<Change>> {
    (open.kind() == kelve_core::views::OpenKind::Change)
        .then(|| reader::change(open).ok())
        .flatten()
}

/// What a file of what's open is for, once it's read plainly: for a
/// folder's page while its own plainly isn't written.
pub async fn what_of(app: &Shared, open: &Open, path: &str) -> ApiResult<Option<String>> {
    let Ok(ft) = file_text(open, change_of(open).as_deref(), path) else {
        return Ok(None);
    };
    let version = agent::version(app, agent::PLAINLY).await;
    Ok(
        kept::<PlainFile>(app, "file", &ft.hash(), &ft.base(), &version)
            .await?
            .map(|f| f.what),
    )
}

/// The job's key: one for each text, what it was in a change, and version
/// of the instructions, whatever it was opened in.
fn file_key(version: &str, ft: &FileText) -> String {
    format!("plainly:{version}:{}:{}", ft.hash(), ft.base())
}

/// The ideas marked on a text, as `[[term]]` may lead to them.
fn marked_of(marks: &[kelve_core::marks::Mark]) -> Vec<Marked> {
    marks
        .iter()
        .map(|m| {
            let name = m
                .idea
                .as_ref()
                .map_or_else(|| m.name.clone(), |i| i.name.clone());
            Marked {
                names: vec![m.name.clone(), name.clone()],
                key: reader::key_of(m),
                name,
                from: m.start,
                to: m.end,
            }
        })
        .collect()
}

/// Whether a file is read plainly ahead of being shown: not when What
/// kelve marks says its area, or files overall, read as code. Shown
/// plainly anyway, it's asked for then.
fn read_ahead(spec: &kelve_core::spec::Spec, path: &str) -> bool {
    spec.reads(kelve_core::code::language(path)) == ReadAs::Plainly
}

/// Queues a file's plainly, once its reading is handed in: what was read,
/// read plainly too, as soon as wanted as its reading was.
pub async fn after_reading(app: &Shared, id: i64, path: &str, wanted: i64) -> pipeline::Result<()> {
    if !read_ahead(&crate::spec::read(app), path) {
        return Ok(());
    }
    let mut conn = app.db.acquire().await?;
    kelve_store::jobs::add(
        &mut conn,
        JOB,
        Some(&format!("{JOB}:{id}:{path}")),
        &json!({ "opened": id, "path": path, "since": Utc::now().timestamp_millis(), "priority": wanted }),
        Utc::now(),
    )
    .await?;
    drop(conn);
    app.wake.notify_one();
    Ok(())
}

/// Queues the plainly of a file already read, while what it's in is read:
/// once, so asking again never puts it off.
pub async fn already_read(app: &Shared, id: i64, path: &str) -> pipeline::Result<()> {
    if !read_ahead(&crate::spec::read(app), path) {
        return Ok(());
    }
    queue(
        app,
        JOB,
        &format!("{JOB}:{id}:{path}"),
        json!({ "opened": id, "path": path, "since": Utc::now().timestamp_millis(), "priority": Priority::Rest as i64 }),
    )
    .await
}

/// What's read plainly under the instructions used now: to count a file of
/// what's open as read only once it's read plainly too.
#[derive(Debug)]
pub struct Written {
    spec: kelve_core::spec::Spec,
    version: String,
    /// Each text written for, by its hash and, in a change, what it was.
    kept: BTreeSet<(String, String)>,
    /// The jobs put aside after their tries: not asked again for now.
    dropped: BTreeSet<String>,
}

impl Written {
    /// Whether a file is read plainly as part of reading it: not when What
    /// kelve marks reads it as code.
    pub fn wanted(&self, path: &str) -> bool {
        read_ahead(&self.spec, path)
    }

    /// Whether a text, by its hash and, in a change, the hash of what it
    /// was (`base`, "" outside one), is read plainly: written, or put
    /// aside after its tries.
    pub fn has(&self, hash: &str, base: &str) -> bool {
        self.kept.contains(&(hash.to_string(), base.to_string()))
            || self
                .dropped
                .contains(&format!("plainly:{}:{hash}:{base}", self.version))
    }

    /// Whether a file of a folder, read with the text whose hash is
    /// `hash`, is done reading: read plainly, or not to be.
    pub fn read(&self, path: &str, hash: &str) -> bool {
        !self.wanted(path) || self.has(hash, "")
    }

    /// Whether a change's file is read plainly, as the change left it.
    pub fn changed(&self, fc: &FileChange) -> bool {
        let Some(after) = &fc.after else {
            return true;
        };
        let base = match &fc.before {
            Some(before) => reader::text_hash(before),
            None => "new".into(),
        };
        self.has(&reader::text_hash(after), &base)
    }

    /// The files of a change it reads plainly as it's read.
    pub fn in_change(&self, change: &Change) -> BTreeSet<String> {
        read_ahead_in(&self.spec, change)
    }
}

/// What's read plainly so far.
pub async fn read_so_far(app: &Shared) -> pipeline::Result<Written> {
    let version = agent::version(app, agent::PLAINLY).await;
    let mut conn = app.db.acquire().await?;
    let kept = kelve_store::reader::plainly_kept(&mut conn, "file", &version)
        .await?
        .into_iter()
        .collect();
    let dropped = kelve_store::asks::dropped_like(&mut conn, &format!("plainly:{version}:"))
        .await?
        .into_iter()
        .collect();
    Ok(Written {
        spec: crate::spec::read(app),
        version,
        kept,
        dropped,
    })
}

// ---------------------------------------------------------------- its pages

/// How deep a folder is: the top is 0.
fn depth(path: &str) -> usize {
    if path.is_empty() {
        0
    } else {
        path.matches('/').count() + 1
    }
}

/// The folder each of `files` is in, and each folder above it, up to the
/// top, the deepest first, so each comes after the folders in it. Not a
/// dot-folder, which is tooling, nor one that holds only one folder: its
/// page is that folder's, as the folder above it lists it. The top always.
fn folders_holding(root: &Path, files: &[String]) -> Vec<String> {
    let mut folders = BTreeSet::from([String::new()]);
    for f in files {
        let mut at = f.as_str();
        while let Some((up, _)) = at.rsplit_once('/') {
            folders.insert(up.to_string());
            at = up;
        }
    }
    let mut out: Vec<String> = folders
        .into_iter()
        .filter(|d| {
            d.is_empty()
                || (!d.split('/').any(|s| s.starts_with('.'))
                    && crate::way::through(root, d.clone()) == *d)
        })
        .collect();
    out.sort_by(|a, b| depth(b).cmp(&depth(a)).then(a.cmp(b)));
    out
}

/// The folders of what's open read plainly as part of reading it, the deepest
/// first: for a change, each folder holding a file it reads, and each above it,
/// up to the top; for anything else opened as a folder, each folder in it with
/// a file kelve reads, but what tools made; for a file opened alone, none.
pub async fn folders_of(app: &Shared, open: &Open, change: Option<&Change>) -> Vec<String> {
    let root = open.root.clone();
    // The folders of what's read now, along the way in; none while it's
    // found, so no page is written before there's anything to write it from.
    let now = crate::way::to_read_now(app, open).await;
    if now
        .as_ref()
        .is_some_and(std::collections::HashSet::is_empty)
    {
        return Vec::new();
    }
    let files: Option<Vec<String>> = match open.kind() {
        kelve_core::views::OpenKind::File => return Vec::new(),
        kelve_core::views::OpenKind::Change => {
            let Some(change) = change else {
                return Vec::new();
            };
            let tools = tools_in(change);
            Some(
                change
                    .files
                    .iter()
                    .filter(|f| f.after.is_some() && f.skip.is_none() && !tools.contains(&f.path))
                    .filter(|f| now.as_ref().is_none_or(|n| n.contains(&f.path)))
                    .map(|f| f.path.clone())
                    .collect(),
            )
        }
        kelve_core::views::OpenKind::Folder => None,
    };
    if files.as_ref().is_some_and(Vec::is_empty) {
        return Vec::new();
    }
    let only = open.only();
    tokio::task::spawn_blocking(move || {
        let files = match files {
            Some(files) => files,
            None => {
                let Ok(walked) = kelve_engine::walk::walk(&root) else {
                    return Vec::new();
                };
                walked
                    .files
                    .into_iter()
                    .filter(|f| {
                        f.skip.is_none()
                            && !reader::is_kelves(&f.rel)
                            && !core::made_by_tools(&f.rel, None)
                            && only.as_ref().is_none_or(|o| o.contains(&f.rel))
                            && now.as_ref().is_none_or(|n| n.contains(&f.rel))
                    })
                    .map(|f| f.rel)
                    .collect()
            }
        };
        folders_holding(&root, &files)
    })
    .await
    .unwrap_or_default()
}

/// What's open's own pages, read plainly as part of reading it: its
/// folders, and a change's own page, the last.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pages {
    /// How many there are.
    pub of: u32,
    /// How many are still to read.
    pub left: u32,
    /// How many of them are folders'.
    pub folders: u32,
}

/// Whether a folder's or a change's job, by its key, is still to run, or
/// what it asked is with the agent, or waits for it.
async fn pending(app: &Shared, key: &str) -> kelve_store::Result<bool> {
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::asks::waiting_resuming(&mut conn, key).await?
        || kelve_store::jobs::due(&mut conn, key).await?.is_some())
}

/// Whether a page is still to read: its job is still to run or with the
/// agent, with nothing written for it yet; or it was never asked for, and
/// is asked for now, when `ask`. One written, even while it's written
/// again, or put aside after its tries, is read.
async fn page_left(
    app: &Shared,
    key: &str,
    kind: &str,
    at: &str,
    asked: &str,
    version: &str,
) -> ApiResult<Option<bool>> {
    let mut conn = app.db.acquire().await?;
    if kelve_store::reader::latest_plainly(&mut conn, kind, at, version)
        .await?
        .is_some()
    {
        return Ok(Some(false));
    }
    drop(conn);
    if pending(app, key).await? {
        return Ok(Some(true));
    }
    let mut conn = app.db.acquire().await?;
    if !kelve_store::asks::dropped_like(&mut conn, asked)
        .await?
        .is_empty()
    {
        return Ok(Some(false));
    }
    // Never asked for.
    Ok(None)
}

/// How far what's open's own pages are read (`Pages`), each folder's
/// (`folders_of`) and a change's, asking for any never asked for when
/// `ask`. A file opened alone has none.
pub async fn pages(
    app: &Shared,
    open: &Open,
    change: Option<&Change>,
    ask: bool,
) -> ApiResult<Pages> {
    let id = open.row.id;
    let read_now;
    let change = match (change, open.kind()) {
        (Some(c), _) => Some(c),
        (None, kelve_core::views::OpenKind::Change) => {
            read_now = reader::change(open).ok();
            read_now.as_deref()
        }
        _ => None,
    };
    let folders = folders_of(app, open, change).await;
    let mut pages = Pages::default();
    let version = agent::version(app, agent::PLAINLY_FOLDER).await;
    for path in &folders {
        let at = folder_at(open, path);
        let asked = format!("plainlyFolder:{version}:{}:", print(&at));
        let key = format!("{FOLDER_JOB}:{id}:{path}");
        pages.of += 1;
        pages.folders += 1;
        let left = match page_left(app, &key, "folder", &at, &asked, &version).await? {
            Some(left) => left,
            None => {
                if ask {
                    ask_folder(app, id, path, Priority::Rest).await?;
                }
                true
            }
        };
        pages.left += u32::from(left);
    }
    if change.is_some_and(|c| !c.files.is_empty()) {
        let version = change_version(app).await;
        let at = change_at(open);
        let asked = format!("plainlyChange:{version}:{}:", print(&at));
        let key = format!("{CHANGE_JOB}:{id}");
        pages.of += 1;
        let left = match page_left(app, &key, "change", &at, &asked, &version).await? {
            Some(left) => left,
            None => {
                if ask {
                    ask_change(app, id, Priority::Rest).await?;
                }
                true
            }
        };
        pages.left += u32::from(left);
    }
    Ok(pages)
}

/// Asks for every page of what's open to be read plainly, as it's opened:
/// its folders, the deepest first, then a change's own. Each written for
/// what it is now is let be when its job runs.
pub async fn ask_pages(app: &Shared, open: &Open) -> ApiResult<()> {
    let change = match open.kind() {
        kelve_core::views::OpenKind::File => return Ok(()),
        kelve_core::views::OpenKind::Change => match reader::change(open) {
            Ok(change) => Some(change),
            // Said when it's shown.
            Err(_) => return Ok(()),
        },
        kelve_core::views::OpenKind::Folder => None,
    };
    ask_overview(app, open).await?;
    for path in folders_of(app, open, change.as_deref()).await {
        ask_folder(app, open.row.id, &path, Priority::Rest).await?;
    }
    if change.is_some() {
        ask_change(app, open.row.id, Priority::Rest).await?;
    }
    // Once there's a way in, every other folder's page, after it, so the
    // person can find their own way through the rest.
    for path in rest_folders(app, open).await {
        ask_folder(app, open.row.id, &path, Priority::Later).await?;
    }
    Ok(())
}

/// Asks for what's open's overview, once there's a way in: the change's own
/// page, or a folder's top page. It's the first on the way in, and the
/// first job after it, written from what there is, without waiting for its
/// files: the whole of it before any part.
pub async fn ask_overview(app: &Shared, open: &Open) -> ApiResult<()> {
    if crate::way::to_read_now(app, open)
        .await
        .is_none_or(|n| n.is_empty())
    {
        return Ok(());
    }
    let id = open.row.id;
    // Only until it's first written: after that it's brought up to date as
    // any page is, once what it's written from is.
    let (kind, at, version) = match open.kind() {
        kelve_core::views::OpenKind::Change => {
            ("change", change_at(open), change_version(app).await)
        }
        _ => (
            "folder",
            folder_at(open, ""),
            agent::version(app, agent::PLAINLY_FOLDER).await,
        ),
    };
    {
        let mut conn = app.db.acquire().await?;
        if kelve_store::reader::latest_plainly(&mut conn, kind, &at, &version)
            .await?
            .is_some()
        {
            return Ok(());
        }
    }
    let mut first = json!({ "opened": id, "since": Utc::now().timestamp_millis(), "priority": Priority::Now as i64, "first": true });
    // In place of its page's job, if one waits: it comes first now.
    let (job, key) = match open.kind() {
        kelve_core::views::OpenKind::Change => (CHANGE_JOB, format!("{CHANGE_JOB}:{id}")),
        _ => {
            first["path"] = json!("");
            (FOLDER_JOB, format!("{FOLDER_JOB}:{id}:"))
        }
    };
    let mut conn = app.db.acquire().await?;
    kelve_store::jobs::add(&mut conn, job, Some(&key), &first, Utc::now()).await?;
    drop(conn);
    app.wake.notify_one();
    Ok(())
}

/// Once the overview is written, its page is asked for again as any page
/// is, so it's brought up to date once what it's written from is read.
async fn after_overview(
    app: &Shared,
    job: &str,
    key: &str,
    payload: &Value,
) -> pipeline::Result<()> {
    let mut again = payload.clone();
    if let Some(o) = again.as_object_mut() {
        o.remove("first");
        o.remove("print");
        o.insert("priority".into(), json!(Priority::Rest as i64));
        o.insert("since".into(), json!(Utc::now().timestamp_millis()));
    }
    let mut conn = app.db.acquire().await?;
    kelve_store::jobs::add(&mut conn, job, Some(key), &again, Utc::now()).await?;
    Ok(())
}

/// Whether a page's job is what's open's overview, written first.
fn first(payload: &Value) -> bool {
    payload["first"].as_bool().unwrap_or(false)
}

/// The folders of what's open that aren't on the way in, each with a file
/// kelve reads in it or below it, the shallowest first: their pages come
/// after the way in's, so the person can find their own way through the
/// rest. None without a way in.
pub async fn rest_folders(app: &Shared, open: &Open) -> Vec<String> {
    let Some(now) = crate::way::to_read_now(app, open).await else {
        return Vec::new();
    };
    if now.is_empty() {
        return Vec::new();
    }
    let root = open.root.clone();
    let open = open.clone();
    tokio::task::spawn_blocking(move || {
        // Its files, a folder's or a change's, but what tools made.
        let files: Vec<String> = crate::way::files_of(&open)
            .into_iter()
            .filter(|f| !core::made_by_tools(f, None))
            .collect();
        let on_way: Vec<String> = files.iter().filter(|f| now.contains(*f)).cloned().collect();
        let way: BTreeSet<String> = folders_holding(&root, &on_way).into_iter().collect();
        let mut rest: Vec<String> = folders_holding(&root, &files)
            .into_iter()
            .filter(|f| !way.contains(f))
            .collect();
        rest.sort_by(|a, b| depth(a).cmp(&depth(b)).then(a.cmp(b)));
        rest
    })
    .await
    .unwrap_or_default()
}

/// Whether the pages of `folders` are read plainly, as what's above them
/// is written from them: each written for what it is now, or with nothing
/// asked of it.
async fn written_below(app: &Shared, open: &Open, folders: &[String]) -> pipeline::Result<bool> {
    let version = agent::version(app, agent::PLAINLY_FOLDER).await;
    for path in folders {
        if !pending(app, &format!("{FOLDER_JOB}:{}:{path}", open.row.id)).await? {
            continue;
        }
        let Ok(m) = folder_material(app, open, path).await else {
            continue;
        };
        if kept::<FolderPlainly>(app, "folder", &folder_at(open, path), &m.print, &version)
            .await?
            .is_none()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether a folder or a change has waited long enough for what it's
/// written from, to be written from what there is: ten minutes, and
/// nothing of what's open is with the agent or waits for it, so nothing
/// more is coming.
async fn waited_out(app: &Shared, id: i64, payload: &Value) -> pipeline::Result<bool> {
    if !waited(payload) {
        return Ok(false);
    }
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::asks::reading(&mut conn, id).await? == 0)
}

/// The page above a folder's: the folder that lists it, past any that
/// hold only one folder. None above the top.
fn page_above(root: &Path, path: &str) -> Option<String> {
    let mut at = path;
    loop {
        if at.is_empty() {
            return None;
        }
        let up = at.rsplit_once('/').map(|(up, _)| up).unwrap_or_default();
        if up.is_empty() || crate::way::through(root, up.to_string()) == up {
            return Some(up.to_string());
        }
        at = up;
    }
}

/// The files and folders beside a file that use it, as kelve reads their
/// code: a folder's path ends in `/`.
async fn used_by(open: &Open, path: &str) -> Vec<String> {
    let dir = path
        .rsplit_once('/')
        .map(|(d, _)| d.to_string())
        .unwrap_or_default();
    let root = open.root.clone();
    let found = tokio::task::spawn_blocking(move || crate::way::parts_of(&root, &dir))
        .await
        .unwrap_or_default();
    let Some(at) = found.parts.iter().position(|(p, _)| p == path) else {
        return Vec::new();
    };
    found
        .uses
        .iter()
        .filter(|(_, used)| *used == at)
        .filter_map(|(user, _)| found.parts.get(*user))
        .map(|(p, folder)| if *folder { format!("{p}/") } else { p.clone() })
        .collect()
}

/// A file read plainly: as kept, or asked of the person's agent, ahead of
/// the rest, since the person is looking at it. Unless `ask`, nothing is
/// asked: it's only looked at, ahead of opening it.
pub async fn view(app: &Shared, open: &Open, path: &str, ask: bool) -> ApiResult<PlainlyView> {
    let path = path.trim().to_string();
    let change = change_of(open);
    let ft = file_text(open, change.as_deref(), &path)?;
    let version = agent::version(app, agent::PLAINLY).await;
    let in_change = match &ft.change {
        Some((before, fc, _)) => {
            let kind = if before.is_none() {
                Some(ChangeKind::Adds)
            } else {
                latest::<ChangePlainly>(app, "change", &change_at(open), &change_version(app).await)
                    .await?
                    .and_then(|c| c.kind)
            };
            Some(PlainChange {
                kind,
                says: None,
                moved_from: fc.moved_from.clone(),
            })
        }
        None => None,
    };
    let mut view = PlainlyView {
        code: ft.code(),
        what: None,
        ideas: Vec::new(),
        used_by: used_by(open, &path).await,
        change: in_change,
        parts: None,
        out: Vec::new(),
        waiting: None,
        path: path.clone(),
    };
    if let Some(mut file) = kept::<PlainFile>(app, "file", &ft.hash(), &ft.base(), &version).await?
    {
        let (marks, _) = pipeline::mark::kept(app, &path, &ft.text, None).await?;
        view.ideas = core::resolve_file(&mut file, &marked_of(&marks));
        view.what = Some(file.what);
        if let Some(c) = view.change.as_mut() {
            c.says = file.change;
        }
        view.parts = Some(file.parts);
        view.out = file.out;
        return Ok(view);
    }
    let id = open.row.id;
    if ask {
        reader::ask(
            app,
            JOB,
            &format!("{JOB}:{id}:{path}"),
            json!({ "opened": id, "path": path, "since": Utc::now().timestamp_millis(), "priority": Priority::Now as i64 }),
        )
        .await?;
        agent::hurry(app, None, Some(id)).await?;
    }
    let mut waiting = waiting(app, &file_key(&version, &ft)).await?;
    // Not asked for yet while the file itself is still read: where its
    // reading is, then, and whether the agent has it now.
    if waiting.before.is_none() {
        let read_as = if ft.change.is_some() {
            reader::READ_DIFF
        } else {
            reader::READ_FILE
        };
        let mut conn = app.db.acquire().await?;
        let read = kelve_store::asks::key_resuming(
            &mut conn,
            agent::READ,
            &format!("{read_as}:{id}:{path}"),
        )
        .await?;
        if let Some(key) = read
            && let Some((ask, taken, before)) =
                kelve_store::asks::files_before(&mut conn, &key).await?
        {
            waiting = PlainWaiting {
                writing: false,
                reading: taken,
                before: Some(before),
                ask: Some(ask),
            };
        }
    }
    view.waiting = Some(waiting);
    Ok(view)
}

/// What the job is called, for the agent and on its list.
fn about(path: &str) -> String {
    format!("Reading {path} plainly")
}

fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

/// The shape of a file's answer.
fn file_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "what": string("What the whole file is for, or for text what it says or decides: one or two sentences, at most 35 words."),
            "change": string("Only in a change: before → now, in a line, at most 30 words."),
            "parts": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "from": { "type": "integer", "description": "Its first line." },
                        "to": { "type": "integer", "description": "Its last line." },
                        "goal": string("What it's for, in two to six words."),
                        "says": string("What it does and what that achieves: at most two sentences, 45 words."),
                        "before": string("Only in a change, for lines it touched: what it did before, at most 30 words."),
                        "also": { "type": "boolean", "description": "Set-up: what it brings in, its settings." },
                    },
                    "required": ["from", "to", "goal", "says"],
                    "additionalProperties": false,
                },
            },
            "out": {
                "type": "array",
                "description": "Only in a change: what it took out.",
                "items": {
                    "type": "object",
                    "properties": { "goal": string("What it was for, in two to six words."), "says": string("What it did: at most two sentences, 45 words.") },
                    "required": ["goal", "says"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["what", "parts"],
        "additionalProperties": false,
    })
}

/// What a file's job gave the agent, as its answer is checked against it.
fn file_given(material: &Value) -> FileGiven {
    let nums = |v: &Value| -> BTreeSet<u32> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok()))
                    .collect()
            })
            .unwrap_or_default()
    };
    let diff = material["change"].is_object().then(|| {
        let mut added = BTreeSet::new();
        for run in material["change"]["added"].as_array().into_iter().flatten() {
            let (a, b) = (run[0].as_u64().unwrap_or(0), run[1].as_u64().unwrap_or(0));
            for n in a..=b.min(a + 100_000) {
                if let Ok(n) = u32::try_from(n) {
                    added.insert(n);
                }
            }
        }
        Diff {
            added,
            out_after: nums(&material["change"]["takenOutAfter"]),
        }
    });
    FileGiven {
        count: material["lineCount"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0),
        blank: nums(&material["blankLines"]),
        code: material["code"].as_bool().unwrap_or(false),
        lines: core::unnumbered(material["text"].as_str().unwrap_or_default()),
        diff,
    }
}

/// Whether a file's answer is in its shape and fits the file as it was
/// given: why not, in a sentence the agent can act on.
pub fn shape(material: &Value, value: &Value) -> Result<(), String> {
    core::check_file(value, &file_given(material)).map(|_| ())
}

/// The ideas marked in a file, with their lines, as the agent reads them.
fn ideas_listed(marks: &[kelve_core::marks::Mark]) -> String {
    let mut seen = BTreeSet::new();
    let mut out = String::new();
    for m in marks {
        let name = m
            .idea
            .as_ref()
            .map_or_else(|| m.name.clone(), |i| i.name.clone());
        if !seen.insert(name.clone()) {
            continue;
        }
        let (a, b) = (m.start, m.end);
        let lines = if a == b {
            format!("line {a}")
        } else {
            format!("lines {a}-{b}")
        };
        out.push_str(&format!("{name}, {lines}: {}\n", m.use_));
    }
    out
}

/// Runs of lines, as `[from, to]`.
fn runs(lines: &BTreeSet<u32>) -> Vec<[u32; 2]> {
    let mut out: Vec<[u32; 2]> = Vec::new();
    for &n in lines {
        match out.last_mut() {
            Some(run) if run[1] + 1 == n => run[1] = n,
            _ => out.push([n, n]),
        }
    }
    out
}

/// What a change did to a file, as the agent is given it: the lines it
/// added, and the text of those it took out, by where they were.
fn change_material(open: &Open, before: Option<&str>, fc: &FileChange, diff: &Diff) -> Value {
    let mut out = String::new();
    let mut last = 0u32;
    let mut in_run = false;
    let after = fc.after.as_deref().unwrap_or("");
    for line in kelve_engine::git::whole(before.unwrap_or(""), after).lines {
        match (line.kind, line.new) {
            (LineKind::Removed, _) => {
                if !in_run {
                    out.push_str(&if last == 0 {
                        "At the top:\n".to_string()
                    } else {
                        format!("After line {last}:\n")
                    });
                    in_run = true;
                }
                out.push_str(&format!("- {}\n", line.text));
            }
            (_, Some(n)) => {
                last = n;
                in_run = false;
            }
            _ => in_run = false,
        }
    }
    json!({
        "status": match fc.status { Status::Added => "added", Status::Modified => "modified", Status::Deleted => "deleted" },
        "movedFrom": fc.moved_from,
        "said": fence("said", &open.row.said),
        "added": runs(&diff.added),
        "takenOutAfter": diff.out_after,
        "takenOut": fence("said", &out),
    })
}

/// Reads a file plainly, as a job, with the person's agent; when its answer
/// comes back, checks it again and keeps it.
pub async fn job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let (Some(id), Some(path)) = (payload["opened"].as_i64(), payload["path"].as_str()) else {
        return Ok(Then::Done);
    };
    let Ok(open) = reader::opened(app, id).await else {
        return Ok(Then::Done);
    };
    let change = change_of(&open);
    let Ok(ft) = file_text(&open, change.as_deref(), path) else {
        return Ok(Then::Done);
    };
    let version = agent::version(app, agent::PLAINLY).await;
    // Written meanwhile, for this text, wherever it was opened.
    if kept::<PlainFile>(app, "file", &ft.hash(), &ft.base(), &version)
        .await?
        .is_some()
    {
        written(app, &open, path).await?;
        return Ok(Then::Done);
    }
    // It waits for the file to be read, so it's given the ideas marked in
    // it: the reading asked for now, ahead of the rest.
    let (marks, whole) = pipeline::mark::kept(app, path, &ft.text, None).await?;
    if !whole && !waited(payload) {
        let asked = if ft.change.is_some() {
            reader::ask_diff(app, id, path).await
        } else {
            reader::ask(
                app,
                reader::READ_FILE,
                &format!("{}:{id}:{path}", reader::READ_FILE),
                json!({ "opened": id, "path": path, "priority": payload["priority"].as_i64().unwrap_or(0) }),
            )
            .await
        };
        asked.map_err(|e| Failed::Other(format!("couldn't ask for a file to be read: {e:?}")))?;
        // Its reading, once handed in, brings this on again.
        return Ok(Then::Again(Utc::now() + chrono::Duration::minutes(1)));
    }
    let marks = pipeline::mark::shown(app, marks);
    // Part of reading the file: it goes where its reading was, before the
    // next file's.
    let read_as = if ft.change.is_some() {
        reader::READ_DIFF
    } else {
        reader::READ_FILE
    };
    let follows = {
        let mut conn = app.db.acquire().await?;
        kelve_store::asks::key_resuming(&mut conn, agent::READ, &format!("{read_as}:{id}:{path}"))
            .await?
    };
    let (count, blank) = core::blank_lines(&ft.text, MOST_LINES);
    let material = json!({
        "path": path,
        "language": ft.lang,
        "code": ft.code(),
        "lineCount": count,
        "blankLines": blank,
        "text": fence("said", &core::numbered(&ft.text, MOST_LINES)),
        "ideas": fence("said", &ideas_listed(&marks)),
        "change": ft.change.as_ref().map(|(before, fc, diff)| change_material(&open, before.as_deref(), fc, diff)),
    });
    let Some(answer) = agent::ask(
        app,
        &file_key(&version, &ft),
        agent::Ask {
            kind: agent::PLAINLY,
            about: about(path),
            idea: None,
            opened: Some(id),
            material,
            schema: file_schema(),
            priority: priority(payload),
            follows,
        },
        agent::Resume::job(JOB, format!("{JOB}:{id}:{path}"), payload.clone()),
    )
    .await?
    else {
        // Waiting for the agent: at its file's place on the path, as its
        // reading would be, though the file was read before and has none.
        if let Some(rank) = crate::way::rank(app, &open, path).await {
            let mut conn = app.db.acquire().await?;
            kelve_store::asks::place_reading(&mut conn, &format!("{JOB}:{id}:{path}"), rank)
                .await?;
        }
        return Ok(Then::Done);
    };
    // Checked as it was handed in; checked again against what it was
    // given, since nothing kept goes unchecked.
    let file = core::check_file(&answer.value, &file_given(&answer.material))
        .map_err(|e| Failed::Other(format!("a plainly answer doesn't read: {e}")))?;
    keep(app, "file", &ft.hash(), &ft.base(), &version, &file).await?;
    written(app, &open, path).await?;
    Ok(Then::Done)
}

/// A file was read plainly: the reader hears, and the folder it's in and
/// the change it's in, waiting for their files, look again.
async fn written(app: &Shared, open: &Open, path: &str) -> pipeline::Result<()> {
    let id = open.row.id;
    let mut conn = app.db.acquire().await?;
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or_default();
    kelve_store::jobs::due_now_at(&mut conn, &format!("{FOLDER_JOB}:{id}:{dir}")).await?;
    kelve_store::jobs::due_now_at(&mut conn, &format!("{CHANGE_JOB}:{id}")).await?;
    drop(conn);
    app.wake.notify_one();
    app.publish(ServerEvent::Plainly { opened: id });
    Ok(())
}

// ---------------------------------------------------------------- a folder

/// A folder as its job gives it: what's checked, what the agent reads, and
/// whether its files are read plainly yet.
struct FolderMaterial {
    material: Value,
    print: String,
    ready: bool,
}

/// A folder's plainly is kept at its project and path.
fn folder_at(open: &Open, path: &str) -> String {
    format!("{}:{path}", open.row.root)
}

/// The folder of what's open at `path`, "" for its top, from its parts and
/// how they were read.
async fn folder_material(app: &Shared, open: &Open, path: &str) -> ApiResult<FolderMaterial> {
    let path = path.trim().trim_matches('/').to_string();
    if !open.root.join(&path).is_dir() || reader::is_kelves(&path) {
        return Err(ApiError::not_found("That folder isn't in what's open."));
    }
    let root = open.root.clone();
    let at = path.clone();
    let found = tokio::task::spawn_blocking(move || {
        let mut found = crate::way::parts_of(&root, &at);
        for (p, folder) in &mut found.parts {
            if *folder {
                *p = crate::way::through(&root, p.clone());
            }
        }
        found
    })
    .await
    .map_err(|e| ApiError::internal(format!("couldn't read a folder's parts: {e}")))?;
    let change = change_of(open);
    let lines = crate::way::kept(app, &open.root).await.folders;
    let file_version = agent::version(app, agent::PLAINLY).await;
    let folder_version = agent::version(app, agent::PLAINLY_FOLDER).await;
    let mut listed = String::new();
    // It waits only for the files that will be read plainly as what's open
    // is read: in a change, those of its files it reads; else every file,
    // but those read as code. The rest are given as they are.
    let spec = crate::spec::read(app);
    let ahead = change.as_ref().map(|c| read_ahead_in(&spec, c));
    // When there's a lot, only what's read now is waited for.
    let now = crate::way::to_read_now(app, open).await;
    let will_read = |p: &str| match &ahead {
        Some(files) => files.contains(p),
        None => read_ahead(&spec, p) && now.as_ref().is_none_or(|n| n.contains(p)),
    };
    let mut ready = true;
    let mut changed: Vec<(u32, u32, bool)> = Vec::new();
    for (i, (p, folder)) in found.parts.iter().enumerate() {
        let what: Option<String> = if *folder {
            match latest::<FolderPlainly>(app, "folder", &folder_at(open, p), &folder_version)
                .await?
            {
                Some(f) => f.what,
                None => lines.get(p).cloned(),
            }
        } else {
            match file_text(open, change.as_deref(), p) {
                Ok(ft) => {
                    let kept =
                        kept::<PlainFile>(app, "file", &ft.hash(), &ft.base(), &file_version)
                            .await?;
                    ready &= kept.is_some() || !will_read(p);
                    kept.map(|f| f.what)
                }
                Err(_) => None,
            }
        };
        let kind = if *folder { "folder" } else { "file" };
        let mut line = format!("{}. {p}/ ({kind})", i + 1);
        if !*folder {
            line = format!("{}. {p} ({kind})", i + 1);
        }
        if let Some(c) = &change {
            let inside: Vec<&FileChange> = c
                .files
                .iter()
                .filter(|f| f.path == *p || f.path.starts_with(&format!("{p}/")))
                .collect();
            let (a, r) = inside
                .iter()
                .fold((0, 0), |(a, r), f| (a + f.added, r + f.removed));
            let new = !inside.is_empty() && inside.iter().all(|f| f.status == Status::Added);
            if a + r > 0 {
                line.push_str(&format!(", +{a} −{r} in the change"));
            }
            changed.push((a, r, new));
        }
        match what {
            Some(w) => line.push_str(&format!(": {w}\n")),
            None => line.push('\n'),
        }
        listed.push_str(&line);
    }
    let uses: Vec<[usize; 2]> = found.uses.iter().map(|(a, b)| [*a, *b]).collect();
    let uses_listed: String = found
        .uses
        .iter()
        .map(|(a, b)| format!("{} uses {}\n", found.parts[*a].0, found.parts[*b].0))
        .collect();
    let parts: Vec<Value> = found
        .parts
        .iter()
        .map(|(p, folder)| json!({ "path": p, "folder": folder }))
        .collect();
    let mut material = json!({
        "path": path,
        "parts": parts,
        "listed": fence("said", &listed),
        "uses": uses,
        "usesListed": fence("said", &uses_listed),
        "changed": change.as_ref().map(|_| changed.iter().map(|(a, r, n)| json!([a, r, n])).collect::<Vec<_>>()),
        "said": change.as_ref().map(|_| fence("said", &open.row.said)),
    });
    let print = print(&material.to_string());
    material["print"] = json!(print);
    Ok(FolderMaterial {
        material,
        print,
        ready,
    })
}

/// What a folder's job gave the agent, as its answer is checked against it.
fn folder_given(material: &Value) -> FolderGiven {
    let parts = material["parts"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|p| {
                    (
                        p["path"].as_str().unwrap_or_default().to_string(),
                        p["folder"].as_bool().unwrap_or(false),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    let uses = material["uses"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|u| (n(&u[0]) as usize, n(&u[1]) as usize))
                .collect()
        })
        .unwrap_or_default();
    let changed = material["changed"].as_array().map(|a| {
        a.iter()
            .map(|c| {
                (
                    u32::try_from(n(&c[0])).unwrap_or(0),
                    u32::try_from(n(&c[1])).unwrap_or(0),
                    c[2].as_bool().unwrap_or(false),
                )
            })
            .collect()
    });
    FolderGiven {
        parts,
        uses,
        changed,
    }
}

pub fn folder_shape(material: &Value, value: &Value) -> Result<(), String> {
    core::check_folder(value, &folder_given(material)).map(|_| ())
}

fn folder_key(version: &str, open: &Open, path: &str, print: &str) -> String {
    format!(
        "plainlyFolder:{version}:{}:{print}",
        self::print(&folder_at(open, path))
    )
}

/// The shape of a folder's answer.
fn folder_schema() -> Value {
    let part = json!({
        "type": "object",
        "properties": {
            "part": string("Its path, exactly as given."),
            "goal": string("What it's for, in two to six words."),
            "says": string("What it does here and what for: at most two sentences, 45 words."),
            "before": string("Only in a change, for a part it changed: what it did before, at most 30 words."),
        },
        "required": ["part", "goal", "says"],
        "additionalProperties": false,
    });
    json!({
        "type": "object",
        "properties": {
            "what": string("What the folder is for: one or two sentences, at most 35 words."),
            "change": string("Only in a change: what it does here, before → now, at most 30 words."),
            "shape": { "type": "string", "enum": ["steps", "parts"] },
            "parts": { "type": "array", "items": part, "maxItems": 7 },
            "also": {
                "type": "object",
                "properties": {
                    "parts": { "type": "array", "items": { "type": "string" } },
                    "says": string("What they are, in a line of at most 30 words."),
                },
                "required": ["parts", "says"],
                "additionalProperties": false,
            },
            "keeps": {
                "type": "array",
                "maxItems": 3,
                "items": {
                    "type": "object",
                    "properties": { "says": string("What it keeps true, in a line of at most 30 words."), "by": string("The part that keeps it, by its path.") },
                    "required": ["says", "by"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["what", "shape", "parts"],
        "additionalProperties": false,
    })
}

/// Queues a folder's plainly: for its page, or the top of a folder opened.
pub async fn ask_folder(app: &Shared, id: i64, path: &str, wanted: Priority) -> ApiResult<()> {
    let path = path.trim().trim_matches('/');
    queue(
        app,
        FOLDER_JOB,
        &format!("{FOLDER_JOB}:{id}:{path}"),
        json!({ "opened": id, "path": path, "since": Utc::now().timestamp_millis(), "priority": wanted as i64 }),
    )
    .await?;
    Ok(())
}

/// The ideas marked in the files read, each once, and which are in each
/// file.
fn read_ideas(read: &BTreeMap<String, reader::FileRead>) -> Vec<Marked> {
    let mut out: Vec<Marked> = Vec::new();
    for r in read.values() {
        for i in &r.ideas {
            if !out.iter().any(|m| m.key == i.key) {
                out.push(Marked {
                    names: vec![i.name.clone()],
                    key: i.key.clone(),
                    name: i.name.clone(),
                    from: 0,
                    to: 0,
                });
            }
        }
    }
    out
}

/// The keys of the ideas in the files at or under `part`.
fn keys_under(read: &BTreeMap<String, reader::FileRead>, part: &str) -> Vec<String> {
    read.iter()
        .filter(|(p, _)| *p == part || p.starts_with(&format!("{part}/")))
        .flat_map(|(_, r)| r.ideas.iter().map(|i| i.key.clone()))
        .collect()
}

/// A folder read plainly: as kept for it as it is; else as it was, while
/// it's written again, or where it is on the agent's list.
pub async fn folder_view(app: &Shared, open: &Open, path: &str) -> ApiResult<FolderPlainly> {
    let m = folder_material(app, open, path).await?;
    let path = m.material["path"].as_str().unwrap_or_default().to_string();
    let version = agent::version(app, agent::PLAINLY_FOLDER).await;
    let at = folder_at(open, &path);
    let exact = kept::<FolderPlainly>(app, "folder", &at, &m.print, &version).await?;
    let stale = exact.is_none();
    let found = match exact {
        Some(f) => Some(f),
        None => latest::<FolderPlainly>(app, "folder", &at, &version).await?,
    };
    let mut view = found.unwrap_or(FolderPlainly {
        path: String::new(),
        what: None,
        change: None,
        ideas: Vec::new(),
        shape: None,
        parts: Vec::new(),
        also: None,
        keeps: Vec::new(),
        stale: false,
        waiting: None,
    });
    view.path = path.clone();
    if view.what.is_some() {
        let read = reader::files_read(app, open).await?;
        let ideas = read_ideas(&read);
        core::resolve_folder(&mut view, &ideas, |p| keys_under(&read, p));
    }
    if stale {
        view.stale = view.what.is_some();
        // Written from its files read plainly: they're read first.
        if !m.ready && open.kind() != kelve_core::views::OpenKind::Change {
            reader::read_ahead(app, open).await?;
        }
        ask_folder(app, open.row.id, &path, Priority::Now).await?;
        agent::hurry(app, None, Some(open.row.id)).await?;
        view.waiting = Some(waiting(app, &folder_key(&version, open, &path, &m.print)).await?);
    }
    Ok(view)
}

/// Reads a folder plainly, as a job, once its files are read plainly, or
/// it has waited long enough; keeps what the agent hands in.
pub async fn folder_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let (Some(id), Some(path)) = (payload["opened"].as_i64(), payload["path"].as_str()) else {
        return Ok(Then::Done);
    };
    let Ok(open) = reader::opened(app, id).await else {
        return Ok(Then::Done);
    };
    let Ok(m) = folder_material(app, &open, path).await else {
        return Ok(Then::Done);
    };
    let version = agent::version(app, agent::PLAINLY_FOLDER).await;
    let at = folder_at(&open, path);
    // Written from its parts as they were read plainly, its files' and
    // its folders', bottom up: a file or a folder in it read plainly
    // brings this on again. One of the rest's, after the way in, is
    // written from what there is, the shallowest first, waiting for
    // nothing; so is the overview, the first on the way in.
    let rest = priority(payload) == Priority::Later || first(payload);
    // When only the way in is read, only the folders on it below are
    // waited for: the rest's pages come after.
    let below: Vec<String> = match crate::way::to_read_now(app, &open).await {
        Some(now) => folders_in(&m.material)
            .into_iter()
            .filter(|f| now.iter().any(|p| p.starts_with(&format!("{f}/"))))
            .collect(),
        None => folders_in(&m.material),
    };
    if kept::<FolderPlainly>(app, "folder", &at, &m.print, &version)
        .await?
        .is_some()
    {
        // Kept for what it is now, as the overview is, while what it's
        // written from is still read: it'll change, so it waits for that.
        if !rest && !(m.ready && written_below(app, &open, &below).await?) {
            return Ok(Then::Again(Utc::now() + chrono::Duration::minutes(2)));
        }
        app.publish(ServerEvent::Plainly { opened: id });
        return Ok(Then::Done);
    }
    // An answer to what it was asked with stands, though its parts have
    // moved on since: it's kept for that, and asked again when it's shown.
    let asked = asked_with(payload, &m.print);
    let key = folder_key(&version, &open, path, &asked);
    if !agent::answered(app, &key).await?
        && !rest
        && !(m.ready && written_below(app, &open, &below).await?)
        && !waited_out(app, id, payload).await?
    {
        return Ok(Then::Again(Utc::now() + chrono::Duration::minutes(2)));
    }
    // The top by what was opened's title; in a change, that's the
    // change's, so by its project's folder.
    let shown = match (path.is_empty(), open.kind()) {
        (false, _) => path.to_string(),
        (true, kelve_core::views::OpenKind::Change) => open
            .root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        (true, _) => open.row.title.clone(),
    };
    let mut resume = payload.clone();
    resume["print"] = json!(asked);
    let Some(answer) = agent::ask(
        app,
        &key,
        agent::Ask {
            kind: agent::PLAINLY_FOLDER,
            about: format!("Reading {shown}/ plainly"),
            idea: None,
            opened: Some(id),
            material: m.material,
            schema: folder_schema(),
            priority: priority(payload),
            follows: None,
        },
        agent::Resume::job(FOLDER_JOB, format!("{FOLDER_JOB}:{id}:{path}"), resume),
    )
    .await?
    else {
        return Ok(Then::Done);
    };
    let mut folder = core::check_folder(&answer.value, &folder_given(&answer.material))
        .map_err(|e| Failed::Other(format!("a folder's plainly doesn't read: {e}")))?;
    folder.path = path.to_string();
    let print = answer.material["print"]
        .as_str()
        .unwrap_or(&m.print)
        .to_string();
    keep(app, "folder", &at, &print, &version, &folder).await?;
    if first(payload) {
        after_overview(
            app,
            FOLDER_JOB,
            &format!("{FOLDER_JOB}:{id}:{path}"),
            payload,
        )
        .await?;
    }
    // The page above it, and the change it's in, written from it, look
    // again.
    let above = {
        let root = open.root.clone();
        let path = path.to_string();
        tokio::task::spawn_blocking(move || page_above(&root, &path))
            .await
            .unwrap_or_default()
    };
    let mut conn = app.db.acquire().await?;
    if let Some(above) = above {
        kelve_store::jobs::due_now_at(&mut conn, &format!("{FOLDER_JOB}:{id}:{above}")).await?;
    }
    kelve_store::jobs::due_now_at(&mut conn, &format!("{CHANGE_JOB}:{id}")).await?;
    drop(conn);
    app.wake.notify_one();
    app.publish(ServerEvent::Plainly { opened: id });
    Ok(Then::Done)
}

/// The folders a folder's job lists as its parts.
fn folders_in(material: &Value) -> Vec<String> {
    material["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["folder"].as_bool().unwrap_or(false))
        .filter_map(|p| p["path"].as_str().map(str::to_string))
        .collect()
}

// ---------------------------------------------------------------- a change

/// A change's plainly is kept at its project and which change it is.
fn change_at(open: &Open) -> String {
    format!("{}:{}", open.row.root, open.row.change)
}

async fn change_version(app: &Shared) -> String {
    agent::version(app, agent::PLAINLY_CHANGE).await
}

/// What tools made in a change: images, lockfiles, and files that say a
/// tool made them or sit where a tool's output goes.
fn tools_in(change: &Change) -> Vec<String> {
    use kelve_engine::walk::Skip;
    change
        .files
        .iter()
        .filter(|f| {
            let skip = f.skip.map(|s| match s {
                Skip::Lockfile => "lockfile",
                Skip::Generated => "generated",
                _ => "other",
            });
            core::made_by_tools(&f.path, skip)
        })
        .map(|f| f.path.clone())
        .collect()
}

/// The files of a change read plainly as it's read: those it marks as
/// it's opened (`reader::mark_change`), but for what tools made, what can't
/// be read, and what What kelve marks reads as code. A folder or the change
/// waits for these, and only these, to be read plainly.
fn read_ahead_in(spec: &kelve_core::spec::Spec, change: &Change) -> BTreeSet<String> {
    let tools = tools_in(change);
    change
        .files
        .iter()
        .filter(|f| f.after.is_some())
        .take(reader::MARK_AHEAD)
        .filter(|f| f.skip.is_none() && !tools.contains(&f.path) && read_ahead(spec, &f.path))
        .map(|f| f.path.clone())
        .collect()
}

/// A change as its job gives it.
struct ChangeMaterial {
    change: Arc<Change>,
    material: Value,
    print: String,
    ready: bool,
}

async fn change_material_of(app: &Shared, open: &Open) -> ApiResult<ChangeMaterial> {
    if open.kind() != kelve_core::views::OpenKind::Change {
        return Err(ApiError::bad_request("That isn't a change."));
    }
    let change = reader::change(open)?;
    let tools = tools_in(&change);
    let version = agent::version(app, agent::PLAINLY).await;
    // Files read as code aren't read plainly ahead: it doesn't wait for them.
    let spec = crate::spec::read(app);
    let mut each = String::new();
    let mut ready = true;
    let ahead = read_ahead_in(&spec, &change);
    let mut files = Vec::new();
    for fc in &change.files {
        let status = match fc.status {
            Status::Added => "added",
            Status::Modified => "modified",
            Status::Deleted => "deleted",
        };
        files.push(json!({
            "path": fc.path, "status": status, "added": fc.added, "removed": fc.removed,
            "movedFrom": fc.moved_from,
        }));
        let mut head = format!("{} ({status}, +{} −{}", fc.path, fc.added, fc.removed);
        if let Some(from) = &fc.moved_from {
            head.push_str(&format!(", moved from {from}"));
        }
        head.push(')');
        if tools.contains(&fc.path) {
            each.push_str(&format!("{head}: made by a tool, not read.\n\n"));
            continue;
        }
        if let Some(skip) = fc.skip {
            each.push_str(&format!("{head}: {}\n\n", skip.says()));
            continue;
        }
        let written = match &fc.after {
            Some(after) => {
                let ft = FileText {
                    text: after.clone(),
                    lang: kelve_core::code::language(&fc.path),
                    change: Some((fc.before.clone(), fc.clone(), Diff::default())),
                };
                kept::<PlainFile>(app, "file", &ft.hash(), &ft.base(), &version).await?
            }
            None => None,
        };
        if ahead.contains(&fc.path) {
            ready &= written.is_some();
        }
        match written {
            Some(w) => {
                each.push_str(&format!("{head}\nWhat it is: {}\n", w.what));
                if let Some(c) = w.change {
                    each.push_str(&format!("What changed: {c}\n"));
                }
                each.push('\n');
            }
            None => {
                each.push_str(&format!("{head}\nThe start of what changed:\n"));
                for line in fc
                    .hunks
                    .iter()
                    .flat_map(|h| &h.lines)
                    .filter(|l| l.kind != LineKind::Context)
                    .take(DIFF_START)
                {
                    let sign = if line.kind == LineKind::Added {
                        '+'
                    } else {
                        '-'
                    };
                    each.push_str(&format!("{sign} {}\n", line.text));
                }
                each.push('\n');
            }
        }
    }
    let mut material = json!({
        "title": fence("said", &change.title),
        "said": fence("said", &open.row.said),
        "files": files,
        "tools": tools,
        "each": fence("said", &each),
        "more": change.more,
    });
    let print = print(&material.to_string());
    material["print"] = json!(print);
    Ok(ChangeMaterial {
        change,
        material,
        print,
        ready,
    })
}

fn change_given(material: &Value) -> ChangeGiven {
    let mut given = ChangeGiven::default();
    for f in material["files"].as_array().into_iter().flatten() {
        let path = f["path"].as_str().unwrap_or_default().to_string();
        if let Some(from) = f["movedFrom"].as_str() {
            given.moved.insert(path.clone(), from.to_string());
        }
        given.files.push(path);
    }
    given
}

pub fn change_shape(material: &Value, value: &Value) -> Result<(), String> {
    core::check_change(value, &change_given(material)).map(|_| ())
}

fn change_key(version: &str, open: &Open, print: &str) -> String {
    format!(
        "plainlyChange:{version}:{}:{print}",
        self::print(&change_at(open))
    )
}

/// The shape of a change's answer.
fn change_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "what": string("What it changes, and why, as the result: one or two sentences, at most 35 words."),
            "fits": string("Where it fits in the whole, in a sentence of at most 35 words."),
            "kind": { "type": "string", "enum": ["adds", "fixes", "reshapes", "takesOut", "updates"] },
            "change": string("Before → now, in a line, at most 30 words."),
            "parts": {
                "type": "array",
                "maxItems": 7,
                "items": {
                    "type": "object",
                    "properties": {
                        "mark": { "type": "string", "enum": ["new", "changed", "moved", "takenOut"] },
                        "goal": string("What it's for, in two to six words."),
                        "says": string("What it does: at most two sentences, 45 words."),
                        "before": string("What it did before, for what changed, at most 30 words."),
                        "files": { "type": "array", "items": { "type": "string" }, "description": "The files it's in, by path as given." },
                    },
                    "required": ["mark", "goal", "says", "files"],
                    "additionalProperties": false,
                },
            },
            "tools": { "type": "array", "items": { "type": "string" }, "description": "What tools made, by path." },
            "touches": { "type": "array", "maxItems": 3, "items": { "type": "string" }, "description": "What else now behaves differently, a line each." },
        },
        "required": ["what", "kind", "parts"],
        "additionalProperties": false,
    })
}

/// Queues a change's plainly, at its open.
pub async fn ask_change(app: &Shared, id: i64, wanted: Priority) -> ApiResult<()> {
    queue(
        app,
        CHANGE_JOB,
        &format!("{CHANGE_JOB}:{id}"),
        json!({ "opened": id, "since": Utc::now().timestamp_millis(), "priority": wanted as i64 }),
    )
    .await?;
    Ok(())
}

/// What kelve adds to what the agent said of a change: what it knows tools
/// made, and its files in the order to read them.
fn finished(mut c: ChangePlainly, change: &Change) -> ChangePlainly {
    for t in tools_in(change) {
        if !c.tools.contains(&t) {
            c.tools.push(t);
        }
    }
    let mut order: Vec<String> = Vec::new();
    let named = c.parts.iter().flat_map(|p| p.files.iter().cloned());
    for f in named.chain(change.files.iter().map(|f| f.path.clone())) {
        if !c.tools.contains(&f) && !order.contains(&f) {
            order.push(f);
        }
    }
    c.order = order;
    c
}

/// A change read plainly: as kept for it as it is; else as it was, while
/// it's written again, or where it is on the agent's list.
pub async fn change_view(app: &Shared, open: &Open) -> ApiResult<ChangePlainly> {
    let m = change_material_of(app, open).await?;
    let version = change_version(app).await;
    let at = change_at(open);
    let exact = kept::<ChangePlainly>(app, "change", &at, &m.print, &version).await?;
    let stale = exact.is_none();
    let found = match exact {
        Some(c) => Some(c),
        None => latest::<ChangePlainly>(app, "change", &at, &version).await?,
    };
    let mut view = match found {
        Some(c) => finished(c, &m.change),
        None => ChangePlainly {
            what: None,
            fits: None,
            kind: None,
            change: None,
            ideas: Vec::new(),
            parts: Vec::new(),
            tools: tools_in(&m.change),
            touches: Vec::new(),
            order: Vec::new(),
            stale: false,
            waiting: None,
        },
    };
    if view.what.is_some() {
        let read = reader::files_read(app, open).await?;
        let ideas = read_ideas(&read);
        core::resolve_change(&mut view, &ideas, |f| keys_under(&read, f));
    }
    if stale {
        view.stale = view.what.is_some();
        ask_change(app, open.row.id, Priority::Now).await?;
        agent::hurry(app, None, Some(open.row.id)).await?;
        view.waiting = Some(waiting(app, &change_key(&version, open, &m.print)).await?);
    }
    Ok(view)
}

/// Reads a change plainly, as a job, once its files are read plainly, or
/// it has waited long enough; keeps what the agent hands in.
pub async fn change_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let Some(id) = payload["opened"].as_i64() else {
        return Ok(Then::Done);
    };
    let Ok(open) = reader::opened(app, id).await else {
        return Ok(Then::Done);
    };
    let Ok(m) = change_material_of(app, &open).await else {
        return Ok(Then::Done);
    };
    let version = change_version(app).await;
    let at = change_at(&open);
    if kept::<ChangePlainly>(app, "change", &at, &m.print, &version)
        .await?
        .is_some()
    {
        // Kept for what it is now, as the overview is, while its files are
        // still read: it'll change, so it waits for that.
        if !first(payload)
            && !(m.ready
                && written_below(app, &open, &folders_of(app, &open, Some(&m.change)).await)
                    .await?)
            && !waited_out(app, id, payload).await?
        {
            return Ok(Then::Again(Utc::now() + chrono::Duration::minutes(2)));
        }
        app.publish(ServerEvent::Plainly { opened: id });
        return Ok(Then::Done);
    }
    if m.change.files.is_empty() {
        return Ok(Then::Done);
    }
    let asked = asked_with(payload, &m.print);
    let key = change_key(&version, &open, &asked);
    // The last of reading it: after its files, and its folders' pages.
    if !agent::answered(app, &key).await?
        && !first(payload)
        && !(m.ready
            && written_below(app, &open, &folders_of(app, &open, Some(&m.change)).await).await?)
        && !waited_out(app, id, payload).await?
    {
        return Ok(Then::Again(Utc::now() + chrono::Duration::minutes(2)));
    }
    let mut resume = payload.clone();
    resume["print"] = json!(asked);
    let Some(answer) = agent::ask(
        app,
        &key,
        agent::Ask {
            kind: agent::PLAINLY_CHANGE,
            about: format!("Reading {} plainly", open.row.title),
            idea: None,
            opened: Some(id),
            material: m.material,
            schema: change_schema(),
            priority: priority(payload),
            follows: None,
        },
        agent::Resume::job(CHANGE_JOB, format!("{CHANGE_JOB}:{id}"), resume),
    )
    .await?
    else {
        return Ok(Then::Done);
    };
    let checked = core::check_change(&answer.value, &change_given(&answer.material))
        .map_err(|e| Failed::Other(format!("a change's plainly doesn't read: {e}")))?;
    let print = answer.material["print"]
        .as_str()
        .unwrap_or(&m.print)
        .to_string();
    keep(
        app,
        "change",
        &at,
        &print,
        &version,
        &finished(checked, &m.change),
    )
    .await?;
    if first(payload) {
        after_overview(app, CHANGE_JOB, &format!("{CHANGE_JOB}:{id}"), payload).await?;
    }
    app.publish(ServerEvent::Plainly { opened: id });
    Ok(Then::Done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_files_answer_is_checked_against_what_it_was_given() {
        let material = json!({
            "lineCount": 4, "blankLines": [2], "code": true,
            "text": fence("said", &core::numbered("use a;\n\nfn retry() {}\nfn wait() {}\n", 10)),
        });
        let good = json!({ "what": "Tries again.", "parts": [
            { "from": 1, "to": 1, "goal": "Brings in a", "says": "What it needs.", "also": true },
            { "from": 3, "to": 4, "goal": "Tries again", "says": "Calls `retry`, then `wait`." },
        ]});
        assert_eq!(shape(&material, &good), Ok(()));
        // Lines left out, and a name that isn't on its lines, are set right
        // by kelve rather than sent back.
        let short = json!({ "what": "Tries again.", "parts": [{ "from": 1, "to": 1, "goal": "Brings in a", "says": "a" }] });
        assert_eq!(shape(&material, &short), Ok(()));
        let named = json!({ "what": "Tries again.", "parts": [
            { "from": 1, "to": 1, "goal": "Brings in a", "says": "a" },
            { "from": 3, "to": 4, "goal": "Tries again", "says": "Calls `backoff`." },
        ]});
        assert_eq!(shape(&material, &named), Ok(()));
        let long = json!({ "what": "Tries again.", "parts": [
            { "from": 1, "to": 4, "goal": "Brings in what it needs and then tries again and again", "says": "a" },
        ]});
        assert!(shape(&material, &long).unwrap_err().contains("at most 6"));
        // In a change: what it added, and where it took lines out.
        let mut changed = material.clone();
        changed["change"] = json!({ "added": [[4, 4]], "takenOutAfter": [3] });
        let given = file_given(&changed);
        assert_eq!(given.diff.as_ref().map(|d| d.added.len()), Some(1));
        // Before where the change didn't touch is left out, not sent back.
        let mut before = good.clone();
        before["parts"][0]["before"] = json!("Brought in more.");
        assert_eq!(shape(&changed, &before), Ok(()));
    }

    #[test]
    fn a_diff_is_told_by_the_lines_after_it() {
        let d = diff_of("a\nb\nc\n", "a\nB\nc\nd\n");
        assert_eq!(d.added, BTreeSet::from([2, 4]));
        assert_eq!(d.out_after, BTreeSet::from([1]));
        assert_eq!(runs(&BTreeSet::from([2, 3, 4, 7])), [[2, 4], [7, 7]]);
    }
}
