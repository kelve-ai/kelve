//! A way into what's open: the person's agent gives one when it thinks the
//! person needs it, opening something new to them or a lot of it. Its files in
//! an order to read them, each with why to read it now, and a line on what each
//! folder is for. kelve checks each is there before it keeps it.
//!
//! The order goes with what was opened. A repo's own order, and every
//! folder's line, are kept in the project, in `.kelve/way.json` beside
//! `.kelve/read/`, so anyone with the repo has them; outside git, or when
//! the person keeps what's read to kelve (settings, Reading), in kelve's
//! own store. None of it goes to the shared graph.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use kelve_engine::central::Parts;

use chrono::Utc;
use kelve_core::fence::fence;
use kelve_core::views::{
    FolderLine, FolderPart, PartView, ServerEvent, StepAt, Way, WayPart, WayStep, WorkStage,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::agent::{self, Priority};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{self, Then};
use crate::reader::{self, Open};
use crate::state::Shared;

/// The job that has the person's agent say what each folder is for.
pub const FOLDERS: &str = "way.folders";
/// The job that has the person's agent find a way in, when there's a lot
/// to read and it gave none.
pub const FIND: &str = "way.find";
/// The job that has the person's agent give the rest of the path, in
/// parts, once the way in is read.
pub const REST: &str = "way.rest";
/// The most steps a way in found for the person has.
pub const MOST_FOUND: usize = 40;
/// The most files listed for the agent to place in one job.
const MOST_PLACED: usize = 1_500;
/// The longest name of a part of the path.
pub const NAME: usize = 40;
/// The most parts the rest of the path has, and the most files in each:
/// what someone needs to understand it, not every file. A big project, of
/// more than `BIG` text files, may have more parts, never bigger ones.
pub const MOST_PARTS: usize = 8;
pub const MOST_PARTS_BIG: usize = 12;
pub const BIG: usize = 200;
pub const MOST_IN_PART: usize = 15;

/// The most parts the rest of the path through so many text files has.
fn most_parts(files: usize) -> usize {
    if files > BIG {
        MOST_PARTS_BIG
    } else {
        MOST_PARTS
    }
}
/// How much of what a project says about itself goes with finding a way in.
const ABOUT: usize = 6_000;
/// The most folders one job asks about; the rest come in the next.
const MOST_FOLDERS: usize = 40;
/// The most of a folder's files and folders a job lists for it.
const MOST_LISTED: usize = 25;

/// The project's own way in, in the project.
pub const FILE: &str = ".kelve/way.json";

/// The most steps kept from an order. A way in is as long as it needs to
/// be, every file worth reading; this only keeps a runaway one in bounds.
pub const MOST_STEPS: usize = 200;
/// The longest line on a step or a folder.
pub const LINE: usize = 90;

/// A project's way in, as kept: its order, and its folders' lines.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    #[serde(default)]
    pub order: Vec<WayStep>,
    /// The rest of the path, in parts, after the way in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<WayPart>,
    /// What each folder is for, by its path.
    #[serde(default)]
    pub folders: BTreeMap<String, String>,
    /// The project's files as its path was found, as a print: opened again
    /// with other files, added or gone, the rest of the path is found again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<String>,
}

/// The way given with one thing opened, as kept with it: its order, the
/// rest of its path once there is one, and the parts the person asked to
/// have read. Kept once as only the order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Given {
    #[serde(default)]
    order: Vec<WayStep>,
    #[serde(default)]
    parts: Vec<WayPart>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    queued: Vec<String>,
}

fn given_of(way: Option<&str>) -> Option<Given> {
    let way = way?;
    serde_json::from_str::<Given>(way).ok().or_else(|| {
        serde_json::from_str::<Vec<WayStep>>(way)
            .ok()
            .map(|order| Given {
                order,
                ..Given::default()
            })
    })
}

/// Whether what's open is the whole project, whose way in is its own.
fn whole(open: &Open) -> bool {
    open.row.kind == "folder" && open.row.path.is_empty() && open.only().is_none()
}

/// A step or a folder line as the agent gave it.
#[derive(Debug, Clone, Deserialize)]
pub struct StepIn {
    pub path: String,
    #[serde(alias = "line")]
    pub why: String,
}

/// Whether the project keeps its way in itself.
fn in_project(app: &Shared, root: &Path) -> bool {
    app.settings().keep_read_in_repo && kelve_engine::git::root_of(root).is_some()
}

/// A line as kelve keeps it: plain, one line, not too long.
fn plain(text: &str) -> String {
    let one: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    kelve_core::text::clip(
        one.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .as_str(),
        LINE,
    )
}

/// A path from the root as kelve keeps it: `/`s, no leading `./` or `/`,
/// and nothing that climbs out.
fn clean(path: &str) -> Option<String> {
    let parts: Vec<&str> = path
        .trim()
        .trim_start_matches("./")
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    (!parts.is_empty() && !parts.contains(&"..")).then(|| parts.join("/"))
}

/// The project's way in, as kept.
pub async fn kept(app: &Shared, root: &Path) -> Kept {
    let file = root.join(FILE);
    if let Ok(text) = tokio::fs::read_to_string(&file).await
        && let Ok(kept) = serde_json::from_str::<Kept>(&text)
    {
        return kept;
    }
    let Ok(mut conn) = app.db.acquire().await else {
        return Kept::default();
    };
    kelve_store::reader::kept_way(&mut conn, &root.display().to_string())
        .await
        .ok()
        .flatten()
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default()
}

async fn keep(app: &Shared, root: &Path, kept: &Kept) -> ApiResult<()> {
    let json = serde_json::to_string_pretty(kept).unwrap_or_default();
    if in_project(app, root) {
        let file = root.join(FILE);
        if let Some(dir) = file.parent() {
            tokio::fs::create_dir_all(dir)
                .await
                .map_err(|e| ApiError::internal(format!("couldn't make .kelve: {e}")))?;
        }
        tokio::fs::write(&file, format!("{json}\n"))
            .await
            .map_err(|e| ApiError::internal(format!("couldn't keep the way in: {e}")))?;
        return Ok(());
    }
    let mut conn = app.db.acquire().await?;
    kelve_store::reader::keep_way(&mut conn, &root.display().to_string(), &json).await?;
    Ok(())
}

/// Takes the way in an agent gave with what it opened: the order, kept with
/// it (and as the project's own, for the whole project), and each folder's
/// line, kept with the project. Steps and folders that aren't there are
/// left out; an order with none left is no order, and opening again with
/// none lets the last one go: the agent thought none was needed.
pub async fn given(
    app: &Shared,
    open: &Open,
    order: &[StepIn],
    folders: &[StepIn],
) -> ApiResult<()> {
    let root = open.root.clone();
    let mut steps: Vec<WayStep> = Vec::new();
    for s in order {
        let Some(path) = clean(&s.path) else { continue };
        if steps.len() == MOST_STEPS
            || steps.iter().any(|x| x.path == path)
            || !root.join(&path).is_file()
        {
            continue;
        }
        steps.push(WayStep {
            path,
            why: plain(&s.why),
        });
    }
    let mut kept = kept(app, &root).await;
    let mut changed = false;
    for f in folders {
        let Some(path) = clean(&f.path) else { continue };
        let line = plain(&f.why);
        if line.is_empty() || !root.join(&path).is_dir() {
            continue;
        }
        if kept.folders.get(&path) != Some(&line) {
            kept.folders.insert(path, line);
            changed = true;
        }
    }
    // An order for the whole project is the project's own, and the rest
    // of the path after it is found again.
    if !steps.is_empty() && whole(open) {
        kept.order = steps.clone();
        kept.parts.clear();
        kept.files = Some(files_print(open).await);
        changed = true;
    }
    if changed {
        keep(app, &root, &kept).await?;
    }
    if open.row.way.is_none() && steps.is_empty() {
        return Ok(());
    }
    let mut conn = app.db.acquire().await?;
    let json = (!steps.is_empty()).then(|| {
        serde_json::to_string(&Given {
            order: steps,
            ..Given::default()
        })
        .unwrap_or_default()
    });
    kelve_store::reader::opened_way(&mut conn, open.row.id, json.as_deref()).await?;
    Ok(())
}

/// Keeps folders' lines with the project, as the agent gave them,
/// leaving the order as it is.
pub async fn folders(app: &Shared, open: &Open, folders: &[StepIn]) -> ApiResult<()> {
    let mut kept = kept(app, &open.root).await;
    let mut changed = false;
    for f in folders {
        let Some(path) = clean(&f.path) else { continue };
        let line = plain(&f.why);
        if !line.is_empty()
            && open.root.join(&path).is_dir()
            && kept.folders.get(&path) != Some(&line)
        {
            kept.folders.insert(path, line);
            changed = true;
        }
    }
    if changed {
        keep(app, &open.root, &kept).await?;
    }
    Ok(())
}

/// The way into what's open: the order the agent gave with it, else, for
/// the whole project, the project's own; and the lines of the folders
/// still there. None when there's neither.
pub async fn view(app: &Shared, open: &Open) -> Option<Way> {
    let kept = kept(app, &open.root).await;
    let Given {
        order,
        parts,
        queued,
    } = given_of(open.row.way.as_deref()).unwrap_or_else(|| {
        if whole(open) {
            Given {
                order: kept.order.clone(),
                parts: kept.parts.clone(),
                queued: Vec::new(),
            }
        } else {
            Given::default()
        }
    });
    let there = |steps: Vec<WayStep>| -> Vec<WayStep> {
        steps
            .into_iter()
            .filter(|s| open.root.join(&s.path).is_file())
            .collect()
    };
    let order = there(order);
    let parts: Vec<WayPart> = parts
        .into_iter()
        .map(|p| WayPart {
            steps: there(p.steps),
            queued: queued.contains(&p.name),
            ..p
        })
        .filter(|p| !p.steps.is_empty())
        .collect();
    let folders: Vec<FolderLine> = kept
        .folders
        .into_iter()
        .filter(|(path, _)| open.root.join(path).is_dir())
        .map(|(path, line)| FolderLine { path, line })
        .collect();
    let finding = order.is_empty() && finding(app, open.row.id).await;
    (!order.is_empty() || !folders.is_empty() || finding).then_some(Way {
        order,
        parts,
        folders,
        finding,
        ..Way::default()
    })
}

/// The way through what's open as the reader shows it: with the files the
/// person's agent is working through, the steps the person has read, and
/// how many files the path leaves off.
pub async fn shown(app: &Shared, open: &Open) -> Option<Way> {
    let mut way = view(app, open).await?;
    if way.order.is_empty() {
        return Some(way);
    }
    way.reading = at_work(app, open).await;
    if let Ok(mut conn) = app.db.acquire().await {
        way.read_steps = kelve_store::reader::steps_read(&mut conn, open.row.id)
            .await
            .unwrap_or_default();
    }
    if !way.parts.is_empty() && open.row.kind != "change" {
        let on: std::collections::HashSet<String> = path_of(&way).into_iter().collect();
        // Known lately, or found in the background and shown when it is:
        // finding it reads every file, and the page doesn't wait for that.
        if let Some(files) = readable_lately(app, open) {
            let left = files.iter().filter(|f| !on.contains(*f)).count();
            way.left_off = u32::try_from(left).unwrap_or(u32::MAX);
        }
    }
    Some(way)
}

/// The files of what's open the person's agent has started and not
/// finished, each with how far it is: reading it, then checking its ideas,
/// then writing it plainly, until it reads plainly.
async fn at_work(app: &Shared, open: &Open) -> Vec<StepAt> {
    let Ok(mut conn) = app.db.acquire().await else {
        return Vec::new();
    };
    let id = open.row.id;
    let read = format!("{}:{id}:", read_kind(open));
    let plainly = format!("{}:{id}:", crate::plainly::JOB);
    let asks = kelve_store::asks::reading_of(&mut conn, id)
        .await
        .unwrap_or_default();
    let mut due = kelve_store::jobs::due_like(&mut conn, &read)
        .await
        .unwrap_or_default();
    due.extend(
        kelve_store::jobs::due_like(&mut conn, &plainly)
            .await
            .unwrap_or_default(),
    );
    #[derive(Default)]
    struct File {
        taken: bool,
        read: bool,
        checking: bool,
        writing: bool,
        due: bool,
    }
    let mut files: BTreeMap<String, File> = BTreeMap::new();
    // The overview, by the path "": the change's own page, or the top
    // folder's, while the agent writes it.
    let overview = [
        format!("{}:{id}", crate::plainly::CHANGE_JOB),
        format!("{}:{id}:", crate::plainly::FOLDER_JOB),
    ];
    for (kind, state, key) in &asks {
        if state == "taken" && overview.contains(key) {
            files.entry(String::new()).or_default().writing = true;
            files.entry(String::new()).or_default().read = true;
            continue;
        }
        let path = key
            .strip_prefix(&read)
            .or_else(|| key.strip_prefix(&plainly));
        let Some(path) = path else { continue };
        let f = files.entry(path.to_string()).or_default();
        match (kind.as_str(), state.as_str()) {
            ("read", "taken") => f.taken = true,
            ("read", "answered") => f.read = true,
            ("choose", _) => f.checking = true,
            ("plainly", _) => f.writing = true,
            _ => {}
        }
    }
    for key in &due {
        if let Some(path) = key.strip_prefix(&plainly) {
            files.entry(path.to_string()).or_default().writing = true;
        } else if let Some(path) = key.strip_prefix(&read) {
            files.entry(path.to_string()).or_default().due = true;
        }
    }
    files
        .into_iter()
        .filter_map(|(path, f)| {
            let stage = if f.taken {
                WorkStage::Reading
            } else if !f.read {
                return None;
            } else if f.writing {
                WorkStage::Writing
            } else if f.checking || f.due {
                WorkStage::Checking
            } else {
                return None;
            };
            Some(StepAt { path, stage })
        })
        .collect()
}

/// How what's open has its files read: a change's, as changed files.
fn read_kind(open: &Open) -> &'static str {
    if open.row.kind == "change" {
        reader::READ_DIFF
    } else {
        reader::READ_FILE
    }
}

/// The files of what's open to read now, when there's a lot of it: the way
/// in, and each part the person asked for; none while the way in is found,
/// or when there's none, so only what the person opens is read. None when
/// there's little enough to read it all.
pub async fn to_read_now(app: &Shared, open: &Open) -> Option<std::collections::HashSet<String>> {
    if !has_way(open).await {
        return None;
    }
    let way = view(app, open).await.unwrap_or_default();
    Some(
        way.order
            .iter()
            .chain(
                way.parts
                    .iter()
                    .filter(|p| p.queued)
                    .flat_map(|p| p.steps.iter()),
            )
            .map(|s| s.path.clone())
            .collect(),
    )
}

/// When only part of what's open is read now: lets go of the reading still
/// waiting for the agent outside it, files and folders' pages, asked for
/// before, when everything was read.
pub async fn keep_to(
    app: &Shared,
    open: &Open,
    now: &std::collections::HashSet<String>,
) -> pipeline::Result<()> {
    let id = open.row.id;
    let folders: std::collections::HashSet<String> = crate::plainly::folders_of(app, open, None)
        .await
        .into_iter()
        .collect();
    let gone = {
        let mut conn = app.db.acquire().await?;
        let mut gone = 0;
        for (kind, job, keep) in [
            (agent::READ, reader::READ_FILE, now),
            (agent::PLAINLY, crate::plainly::JOB, now),
            (agent::PLAINLY_FOLDER, crate::plainly::FOLDER_JOB, &folders),
        ] {
            gone += kelve_store::asks::let_go_waiting(
                &mut conn,
                id,
                kind,
                &format!("{job}:{id}:"),
                keep,
            )
            .await?;
        }
        gone
    };
    if gone > 0 {
        tracing::info!(opened = id, gone, "let go of reading outside the way in");
        app.publish(ServerEvent::Asks);
    }
    Ok(())
}

/// The person asks for a part of the path to be read: it's kept with what's
/// open, and its files are read along the path.
pub async fn read_part(app: &Shared, open: &Open, name: &str) -> ApiResult<()> {
    let Some(way) = view(app, open).await else {
        return Err(ApiError::not_found("There's no path through what's open."));
    };
    if !way.parts.iter().any(|p| p.name == name) {
        return Err(ApiError::not_found("That part isn't on the path any more."));
    }
    let mut given = given_of(open.row.way.as_deref()).unwrap_or_else(|| Given {
        order: way.order.clone(),
        parts: way.parts.clone(),
        queued: Vec::new(),
    });
    if !given.queued.iter().any(|q| q == name) {
        given.queued.push(name.to_string());
    }
    let json = serde_json::to_string(&given).unwrap_or_default();
    {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::opened_way(&mut conn, open.row.id, Some(&json)).await?;
    }
    let open = reader::opened(app, open.row.id).await?;
    reader::read_on(app, &open).await?;
    // More to read: its count, and the way, show it.
    app.publish(ServerEvent::Opened);
    app.publish(ServerEvent::Read {
        opened: open.row.id,
    });
    Ok(())
}

/// Whether the person's agent is finding a way into what's open: asked
/// for, and not yet given.
async fn finding(app: &Shared, id: i64) -> bool {
    let Ok(mut conn) = app.db.acquire().await else {
        return false;
    };
    let job = kelve_store::jobs::due(&mut conn, &format!("{FIND}:{id}"))
        .await
        .ok()
        .flatten()
        .is_some();
    job || kelve_store::asks::asking(&mut conn, agent::WAY, id)
        .await
        .unwrap_or(false)
}

/// Where a file is on the path through what's open, if it's on it.
pub async fn rank(app: &Shared, open: &Open, path: &str) -> Option<usize> {
    let way = view(app, open).await?;
    path_of(&way).iter().position(|p| p == path)
}

/// The whole path through what's open, in order: the way in, then each
/// part of the rest.
pub fn path_of(way: &Way) -> Vec<String> {
    way.order
        .iter()
        .chain(way.parts.iter().flat_map(|p| p.steps.iter()))
        .map(|s| s.path.clone())
        .collect()
}

/// A folder of what's open, as its page shows it while its plainly isn't
/// written: its line, and its parts, each folder with its line and each
/// file with what it's for, once it's read plainly.
pub async fn part(app: &Shared, open: &Open, path: &str) -> ApiResult<PartView> {
    let path = if path.trim().is_empty() {
        String::new()
    } else {
        clean(path).ok_or_else(|| ApiError::not_found("That folder isn't in what's open."))?
    };
    if !open.root.join(&path).is_dir() || reader::is_kelves(&path) {
        return Err(ApiError::not_found("That folder isn't in what's open."));
    }
    let kept = kept(app, &open.root).await;
    if !kept.folders.contains_key(&path) {
        ask_folders(app, open.row.id).await?;
    }
    let root: PathBuf = open.root.clone();
    let at = path.clone();
    let found = tokio::task::spawn_blocking(move || parts_of(&root, &at))
        .await
        .map_err(|e| ApiError::internal(format!("couldn't read a folder's parts: {e}")))?;
    // A folder holding only one folder is that folder, as the tree shows it:
    // `src/api/v1`, not `src/api`.
    let mut parts = Vec::with_capacity(found.parts.len());
    for (p, folder) in found.parts {
        if folder {
            let p = through(&open.root, p);
            parts.push(FolderPart {
                line: kept.folders.get(&p).cloned(),
                path: p,
                folder,
            });
        } else {
            parts.push(FolderPart {
                line: crate::plainly::what_of(app, open, &p).await?,
                path: p,
                folder,
            });
        }
    }
    Ok(PartView {
        line: kept.folders.get(&path).cloned(),
        path,
        parts,
    })
}

/// Folders' parts lately worked out, by the project and folder, with what
/// its files were then: their paths, sizes and when each last changed.
static PARTS: LazyLock<Mutex<HashMap<Folder, (u64, Parts)>>> = LazyLock::new(Default::default);

/// A folder of a project: the project's root, and the folder from it.
type Folder = (PathBuf, String);

/// Each folder's parts being worked out now.
static WORKING: LazyLock<Mutex<HashMap<Folder, Arc<Mutex<()>>>>> = LazyLock::new(Default::default);

/// A folder's parts, from its files: read only as far as it takes, and
/// kept until a file changes, so a folder opens at once the next time.
pub fn parts_of(root: &Path, at: &str) -> Parts {
    let Ok(walked) = kelve_engine::walk::walk(root) else {
        return Parts::default();
    };
    let files: Vec<&kelve_engine::walk::Found> = walked
        .files
        .iter()
        .filter(|f| f.skip.is_none() && !reader::is_kelves(&f.rel))
        .collect();
    let print = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for f in &files {
            f.rel.hash(&mut h);
            f.bytes.hash(&mut h);
            std::fs::metadata(root.join(&f.rel))
                .and_then(|m| m.modified())
                .ok()
                .hash(&mut h);
        }
        h.finish()
    };
    let key = (root.to_path_buf(), at.to_string());
    // One at a time for each folder: asked while it's being worked out, it
    // waits for that, rather than working it out again beside it.
    let working = WORKING
        .lock()
        .map(|mut w| w.entry(key.clone()).or_default().clone())
        .unwrap_or_default();
    let _working = working.lock();
    if let Ok(kept) = PARTS.lock()
        && let Some((p, parts)) = kept.get(&key)
        && *p == print
    {
        return parts.clone();
    }
    let read = |rel: &str| kelve_engine::read::read(root, rel).ok().map(|t| t.text);
    let inside = |rel: &str| at.is_empty() || rel.starts_with(&format!("{at}/"));
    let texts: Vec<(String, Option<String>)> = files
        .iter()
        .map(|f| {
            (
                f.rel.clone(),
                inside(&f.rel).then(|| read(&f.rel)).flatten(),
            )
        })
        .collect();
    let found = kelve_engine::central::parts(at, &texts);
    if let Ok(mut kept) = PARTS.lock() {
        kept.insert(key, (print, found.clone()));
    }
    found
}

/// A folder, or the folder at the end of its chain of folders that each
/// hold only one folder.
pub(crate) fn through(root: &Path, mut path: String) -> String {
    for _ in 0..16 {
        let Ok(entries) = std::fs::read_dir(root.join(&path)) else {
            break;
        };
        let inside: Vec<_> = entries
            .filter_map(Result::ok)
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        match inside.as_slice() {
            [only] if only.file_type().is_ok_and(|t| t.is_dir()) => {
                path = format!("{path}/{}", only.file_name().to_string_lossy());
            }
            _ => break,
        }
    }
    path
}

/// Asks for every folder of what's open to have a line on what it's for,
/// written by the person's agent while it reads: once, while one waits.
pub async fn ask_folders(app: &Shared, id: i64) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::jobs::add_once(
        &mut conn,
        FOLDERS,
        &format!("{FOLDERS}:{id}"),
        &json!({ "opened": id }),
        Utc::now(),
    )
    .await?;
    app.wake.notify_one();
    Ok(())
}

/// The folders of what's open with no line yet, the shallowest first, each
/// with what's in it: for a folder, every one in it; for a change, those
/// its files are in.
fn unsaid(open: &Open, kept: &Kept) -> Vec<(String, Vec<String>)> {
    let Ok(walked) = kelve_engine::walk::walk(&open.root) else {
        return Vec::new();
    };
    let within: Vec<String> = if open.row.kind == "folder" || open.row.kind == "file" {
        vec![open.row.path.trim_matches('/').to_string()]
    } else {
        reader::change(open)
            .map(|c| c.files.iter().map(|f| f.path.clone()).collect())
            .unwrap_or_default()
    };
    let is_change = !(open.row.kind == "folder" || open.row.kind == "file");
    let mut held: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for f in walked.files.iter().filter(|f| !reader::is_kelves(&f.rel)) {
        let mut child = f.rel.clone();
        while let Some((up, name)) = child.rsplit_once('/') {
            let shown = if child == f.rel {
                name.to_string()
            } else {
                format!("{name}/")
            };
            held.entry(up.to_string()).or_default().insert(shown);
            child = up.to_string();
        }
    }
    let wanted = |dir: &str| {
        if is_change {
            within.iter().any(|f| f.starts_with(&format!("{dir}/")))
        } else {
            within
                .iter()
                .any(|w| w.is_empty() || dir == w || dir.starts_with(&format!("{w}/")))
        }
    };
    let mut dirs: Vec<(String, Vec<String>)> = held
        .into_iter()
        .filter(|(dir, _)| !kept.folders.contains_key(dir) && wanted(dir))
        .map(|(dir, names)| (dir, names.into_iter().take(MOST_LISTED).collect()))
        .collect();
    dirs.sort_by(|a, b| {
        a.0.matches('/')
            .count()
            .cmp(&b.0.matches('/').count())
            .then(a.0.cmp(&b.0))
    });
    dirs.truncate(MOST_FOLDERS);
    dirs
}

/// Has the person's agent say what each folder with no line is for, and
/// keeps the lines with the project, as a way in's are. Asks again for the
/// next folders, while there are more.
pub async fn folders_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let Some(id) = payload["opened"].as_i64() else {
        return Ok(Then::Done);
    };
    let Ok(open) = reader::opened(app, id).await else {
        return Ok(Then::Done);
    };
    // The way in says what each folder is for: while it's found, or still
    // to be asked for when there's a lot, wait for it; asked again once it's
    // in, for any it left out. Without one, as when the person took it off
    // the list, ask here.
    let way = view(app, &open).await.unwrap_or_default();
    if way.finding
        || (way.order.is_empty() && has_way(&open).await && {
            let mut conn = app.db.acquire().await?;
            !kelve_store::asks::dropped_of(&mut conn, agent::WAY, id).await?
        })
    {
        return Ok(Then::Done);
    }
    let kept = kept(app, &open.root).await;
    let dirs = {
        let open = open.clone();
        let kept = kept.clone();
        tokio::task::spawn_blocking(move || unsaid(&open, &kept))
            .await
            .unwrap_or_default()
    };
    if dirs.is_empty() {
        return Ok(Then::Done);
    }
    let listed: String = dirs
        .iter()
        .map(|(dir, names)| format!("{dir}/: {}\n", names.join(", ")))
        .collect();
    let mut hash = Sha256::new();
    hash.update(open.row.root.as_bytes());
    hash.update(listed.as_bytes());
    let print: String = hash
        .finalize()
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect();
    let version = agent::version(app, agent::FOLDERS).await;
    let project = open
        .root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(answer) = agent::ask(
        app,
        &format!("folders:{version}:{print}"),
        agent::Ask {
            kind: agent::FOLDERS,
            about: format!("Saying what {project}’s folders are for"),
            idea: None,
            opened: Some(id),
            material: json!({
                "folders": dirs.iter().map(|(d, _)| d).collect::<Vec<_>>(),
                "listed": fence("folders", &listed),
            }),
            schema: folders_schema(),
            priority: Priority::Rest,
            follows: None,
        },
        agent::Resume::job(FOLDERS, format!("{FOLDERS}:{id}"), payload.clone()),
    )
    .await?
    else {
        return Ok(Then::Done);
    };
    let given: Vec<StepIn> =
        serde_json::from_value(answer.value["folders"].clone()).unwrap_or_default();
    let asked: Vec<&str> = dirs.iter().map(|(d, _)| d.as_str()).collect();
    let given: Vec<StepIn> = given
        .into_iter()
        .filter(|f| asked.contains(&f.path.trim_end_matches('/')))
        .map(|f| StepIn {
            path: f.path.trim_end_matches('/').to_string(),
            why: f.why,
        })
        .collect();
    folders(app, &open, &given)
        .await
        .map_err(|e| pipeline::Failed::Other(format!("couldn't keep folders' lines: {e:?}")))?;
    app.publish(ServerEvent::Opened);
    // The next folders, if there are more: those it left out aren't asked again.
    if dirs.len() == MOST_FOLDERS && !given.is_empty() {
        ask_folders(app, id).await.map_err(|e| {
            pipeline::Failed::Other(format!("couldn't ask for more folders: {e:?}"))
        })?;
    }
    Ok(Then::Done)
}

/// The shape of a folders job's answer.
fn folders_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "folders": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "path": { "type": "string" }, "line": { "type": "string" } },
                    "required": ["path", "line"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["folders"],
        "additionalProperties": false,
    })
}

/// Whether a folders job's answer is in its shape: each folder one it was
/// asked about, each line short and plain.
pub fn folders_shape(material: &Value, value: &Value) -> Result<(), String> {
    #[derive(Deserialize)]
    struct Given {
        path: String,
        line: String,
    }
    let given: Vec<Given> = serde_json::from_value(value["folders"].clone()).map_err(|e| {
        format!("It isn't in the shape asked for: {e}. Give folders, each with path and line.")
    })?;
    let asked: Vec<&str> = material["folders"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    for g in &given {
        if !asked.contains(&g.path.trim_end_matches('/')) {
            return Err(format!(
                "{} isn't one of the folders listed: give each path exactly as listed.",
                g.path
            ));
        }
        let line = g.line.trim();
        if line.is_empty() || line.chars().count() > LINE {
            return Err(format!(
                "The line for {} must say what it's for in at most {LINE} characters.",
                g.path
            ));
        }
    }
    Ok(())
}

/// How long the text files of what's open are known before they're found
/// again: finding them reads every file, and the reader asks as it works.
const READABLE_FOR: std::time::Duration = std::time::Duration::from_secs(60);
type Readable = Arc<Vec<String>>;
static READABLE: LazyLock<Mutex<HashMap<i64, (std::time::Instant, Readable)>>> =
    LazyLock::new(Default::default);

/// What's open whose text files are being found again.
static FINDING_READABLE: LazyLock<Mutex<std::collections::HashSet<i64>>> =
    LazyLock::new(Default::default);

/// The text files of what's open, as found lately, if they've been found:
/// when that was a while ago, or never, they're found again in the
/// background, and the reader told when they are.
fn readable_lately(app: &Shared, open: &Open) -> Option<Readable> {
    let id = open.row.id;
    let known = READABLE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&id)
        .cloned();
    if known
        .as_ref()
        .is_none_or(|(at, _)| at.elapsed() >= READABLE_FOR)
        && FINDING_READABLE
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id)
    {
        let (app, open) = (app.clone(), open.clone());
        tokio::spawn(async move {
            let files: Readable = Arc::new(readable(&open).await);
            READABLE
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(id, (std::time::Instant::now(), files));
            FINDING_READABLE
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
            app.publish(ServerEvent::Opened);
        });
    }
    known.map(|(_, files)| files)
}

/// The text files of what's open, by their paths from the root, as they're
/// listed for the agent to place: those kelve reads, never an image or a
/// font.
async fn readable(open: &Open) -> Vec<String> {
    let open = open.clone();
    tokio::task::spawn_blocking(move || readable_now(&open))
        .await
        .unwrap_or_default()
}

fn readable_now(open: &Open) -> Vec<String> {
    let files = files_of(open);
    if open.row.kind == "change" {
        return files;
    }
    files
        .into_iter()
        .filter(|rel| kelve_engine::read::image_kind(rel).is_none())
        .filter(|rel| kelve_engine::read::read(&open.root, rel).is_ok())
        .collect()
}

/// The files of what's open kelve would read, by their paths from the
/// root, before any is found not to be text.
pub(crate) fn files_of(open: &Open) -> Vec<String> {
    if open.row.kind == "change" {
        return reader::change(open)
            .map(|c| {
                c.files
                    .iter()
                    .filter(|f| {
                        f.skip.is_none() && f.after.is_some() && !reader::is_kelves(&f.path)
                    })
                    .map(|f| f.path.clone())
                    .collect()
            })
            .unwrap_or_default();
    }
    let Ok(walked) = kelve_engine::walk::walk(&open.root) else {
        return Vec::new();
    };
    let only = open.only();
    let within = open.row.path.trim_matches('/');
    walked
        .files
        .into_iter()
        .filter(|f| f.skip.is_none() && !reader::is_kelves(&f.rel))
        .filter(|f| match &only {
            Some(o) => o.contains(&f.rel),
            None => {
                within.is_empty() || f.rel == within || f.rel.starts_with(&format!("{within}/"))
            }
        })
        .map(|f| f.rel)
        .collect()
}

/// Whether what's open has a way in: anything with files in it, a folder,
/// some files or a change, however few, but a file opened alone (Nathan,
/// 29 October: the same route for everything). kelve finds the way in
/// first, with its job, and reads only along it.
pub async fn has_way(open: &Open) -> bool {
    if open.kind() == kelve_core::views::OpenKind::File {
        return false;
    }
    let open = open.clone();
    tokio::task::spawn_blocking(move || !files_of(&open).is_empty())
        .await
        .unwrap_or(false)
}

/// Twelve hex digits of a hash of some text: a job's print.
fn print_of(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for p in parts {
        hash.update(p.as_bytes());
    }
    hash.finalize()
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn project(open: &Open) -> String {
    open.root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Asks the person's agent for a way into what's open, when there's a lot
/// to read and it gave none: at once, so it's the first job the agent
/// takes, before any reading, which then follows it. Its answer runs the
/// job again, which keeps it.
pub async fn ask_way(app: &Shared, id: i64) -> ApiResult<()> {
    find_job(app, &json!({ "opened": id }))
        .await
        .map_err(|e| ApiError::internal(format!("couldn't ask for a way in: {e:?}")))?;
    Ok(())
}

/// Asks the person's agent for a way into what's open, with its folders'
/// lines, when there's a lot of it and none was given: first, before any of
/// it is read, however it was opened, by the agent or in the reader. Not
/// while one is found, nor when the person took it off the list. Whether
/// there's a way in, or one being found.
pub async fn ask_way_first(app: &Shared, open: &Open) -> ApiResult<bool> {
    if !has_way(open).await {
        return Ok(false);
    }
    let way = view(app, open).await.unwrap_or_default();
    if !way.order.is_empty() {
        changed_since(app, open).await?;
        return Ok(true);
    }
    if way.finding {
        return Ok(true);
    }
    let dropped = {
        let mut conn = app.db.acquire().await?;
        kelve_store::asks::dropped_of(&mut conn, agent::WAY, open.row.id).await?
    };
    if dropped {
        return Ok(false);
    }
    // Boxed: finding the way in reads along it, which comes back here.
    Box::pin(ask_way(app, open.row.id)).await?;
    Ok(true)
}

/// The print of what's open's files, by their paths: what its path was
/// found for.
async fn files_print(open: &Open) -> String {
    let open = open.clone();
    tokio::task::spawn_blocking(move || {
        let mut files = files_of(&open);
        files.sort();
        print_of(&[&files.join("\n")])
    })
    .await
    .unwrap_or_default()
}

/// The openings whose files were checked against the path's print.
static CHECKED: LazyLock<Mutex<std::collections::HashSet<(i64, i64)>>> =
    LazyLock::new(Default::default);

/// Whether what's open is due to have its files checked: once each time
/// it's opened, though a folder of it shows many times.
fn check_due(open: &Open) -> bool {
    CHECKED
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert((open.row.id, open.row.opened_at))
}

/// The whole project, opened again with files added or gone since its path
/// was found: the way in stands, as what changed in a file is read again
/// by its text, but the rest of the path is found again, so files new to
/// it have their place. A way kept before kelve kept the print starts
/// keeping it now.
async fn changed_since(app: &Shared, open: &Open) -> ApiResult<()> {
    if !whole(open) || !check_due(open) {
        return Ok(());
    }
    let mut kept = kept(app, &open.root).await;
    if kept.order.is_empty() {
        return Ok(());
    }
    let now = files_print(open).await;
    if kept.files.as_deref() == Some(now.as_str()) {
        return Ok(());
    }
    let had = kept.files.replace(now).is_some();
    if had {
        kept.parts.clear();
    }
    keep(app, &open.root, &kept).await?;
    if !had {
        return Ok(());
    }
    if let Some(mut given) = given_of(open.row.way.as_deref()) {
        given.parts.clear();
        let json = serde_json::to_string(&given).unwrap_or_default();
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::opened_way(&mut conn, open.row.id, Some(&json)).await?;
    }
    ask_rest(app, open.row.id).await
}

/// Asks the person's agent for the rest of the path, when it gave a way in
/// with what it opened, and there's a lot to read: once, while one waits.
pub async fn ask_rest(app: &Shared, id: i64) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::jobs::add_once(
        &mut conn,
        REST,
        &format!("{REST}:{id}"),
        &json!({ "opened": id }),
        Utc::now(),
    )
    .await?;
    app.wake.notify_one();
    Ok(())
}

/// A part of the path as the agent gives it: its files by path alone.
#[derive(Deserialize)]
struct PartIn {
    name: String,
    why: String,
    paths: Vec<String>,
}

/// The parts the agent gave, as kelve keeps them: each file there, once,
/// not on the way in; a part with none is left out.
fn parts_from(open: &Open, given: Vec<PartIn>, order: &[WayStep]) -> Vec<WayPart> {
    let mut placed: std::collections::HashSet<String> =
        order.iter().map(|s| s.path.clone()).collect();
    let mut parts: Vec<WayPart> = Vec::new();
    for p in given {
        let mut steps = Vec::new();
        for path in p.paths {
            let Some(path) = clean(&path) else { continue };
            if !open.root.join(&path).is_file() || !placed.insert(path.clone()) {
                continue;
            }
            steps.push(WayStep {
                path,
                why: String::new(),
            });
        }
        let name = kelve_core::text::clip(plain(&p.name).as_str(), NAME);
        if !steps.is_empty() && !name.is_empty() && !parts.iter().any(|q: &WayPart| q.name == name)
        {
            parts.push(WayPart {
                name,
                why: plain(&p.why),
                steps,
                queued: false,
            });
        }
    }
    parts
}

/// Has the person's agent find a way into what's open: the files to read
/// first, in order, and the rest of the path in parts. kelve then reads
/// the way in, and each part when the person asks; the folders' lines come
/// from the folders job, as they're read.
pub async fn find_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let Some(id) = payload["opened"].as_i64() else {
        return Ok(Then::Done);
    };
    let Ok(open) = reader::opened(app, id).await else {
        return Ok(Then::Done);
    };
    if view(app, &open).await.is_some_and(|w| !w.order.is_empty()) {
        return Ok(Then::Done);
    }
    let mut files = readable(&open).await;
    if files.is_empty() {
        return Ok(Then::Done);
    }
    // A folder's files as kelve ranks them, the ones the rest lean on
    // first: where a way in usually starts. A change's, as it changed them.
    if open.row.kind != "change" {
        let ranked = reader::ranked(&open.root).await?;
        let at = |p: &str| {
            ranked
                .iter()
                .position(|(r, _)| r == p)
                .unwrap_or(usize::MAX)
        };
        files.sort_by_key(|f| at(f));
    }
    // Its folders with no line yet, the shallowest first, each with what's
    // in it: their lines come with the way in, so Files has them at once.
    let dirs = {
        let open = open.clone();
        let kept = kept(app, &open.root).await;
        tokio::task::spawn_blocking(move || unsaid(&open, &kept))
            .await
            .unwrap_or_default()
    };
    let folders: Vec<&String> = dirs.iter().map(|(d, _)| d).collect();
    let in_folders: String = dirs
        .iter()
        .map(|(dir, names)| format!("{dir}/: {}\n", names.join(", ")))
        .collect();
    // What it says about itself: its README and its instructions for agents.
    let within = open.row.path.trim_matches('/');
    let at = |name: &str| {
        if within.is_empty() {
            name.to_string()
        } else {
            format!("{within}/{name}")
        }
    };
    let mut about = String::new();
    for name in ["README.md", "readme.md", "README", "AGENTS.md", "CLAUDE.md"] {
        let rel = at(name);
        if !files.contains(&rel) {
            continue;
        }
        if let Ok(read) = kelve_engine::read::read(&open.root, &rel) {
            about.push_str(&format!(
                "{rel}:\n{}\n\n",
                kelve_core::text::clip(&read.text, ABOUT)
            ));
        }
    }
    let listed: Vec<&String> = files.iter().take(MOST_PLACED).collect();
    let version = agent::version(app, agent::WAY).await;
    let print = print_of(&[
        &open.row.root,
        &open.row.path,
        &listed
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    ]);
    let Some(answer) = agent::ask(
        app,
        &format!("way:{version}:{print}"),
        agent::Ask {
            kind: agent::WAY,
            about: format!("Finding a way into {}", project(&open)),
            idea: None,
            opened: Some(id),
            material: json!({
                "files": listed,
                "mostParts": most_parts(files.len()),
                "folders": folders,
                "inFolders": fence("folders", &in_folders),
                "about": fence("about", &about),
                // A change: what it is, as its agent said, and its files
                // are the ones it touched.
                "change": (open.row.kind == "change").then(|| fence("said", &format!("{}\n{}", open.row.title, open.row.said))),
            }),
            schema: way_schema(),
            priority: Priority::Now,
            follows: None,
        },
        agent::Resume::job(FIND, format!("{FIND}:{id}"), payload.clone()),
    )
    .await?
    else {
        return Ok(Then::Done);
    };
    let order: Vec<StepIn> =
        serde_json::from_value(answer.value["order"].clone()).unwrap_or_default();
    let order: Vec<StepIn> = order.into_iter().take(MOST_FOUND).collect();
    let lines: Vec<StepIn> =
        serde_json::from_value(answer.value["folders"].clone()).unwrap_or_default();
    let asked = self::listed(&answer.material, "folders");
    let lines: Vec<StepIn> = lines
        .into_iter()
        .map(|f| StepIn {
            path: f.path.trim_end_matches('/').to_string(),
            why: f.why,
        })
        .filter(|f| asked.contains(&f.path))
        .collect();
    given(app, &open, &order, &lines)
        .await
        .map_err(|e| pipeline::Failed::Other(format!("couldn't keep the way in: {e:?}")))?;
    let open = reopened(app, id).await?;
    let parts: Vec<PartIn> =
        serde_json::from_value(answer.value["parts"].clone()).unwrap_or_default();
    let order_now = view(app, &open).await.map(|w| w.order).unwrap_or_default();
    keep_parts(app, &open, parts_from(&open, parts, &order_now))
        .await
        .map_err(|e| {
            pipeline::Failed::Other(format!("couldn't keep the rest of the path: {e:?}"))
        })?;
    let open = reopened(app, id).await?;
    in_order(app, &open).await?;
    // Now there's a way in: read it.
    reader::read_on(app, &open).await.map_err(|e| {
        pipeline::Failed::Other(format!("couldn't ask for the way in to be read: {e:?}"))
    })?;
    app.publish(ServerEvent::Opened);
    Ok(Then::Done)
}

async fn reopened(app: &Shared, id: i64) -> pipeline::Result<Open> {
    reader::opened(app, id)
        .await
        .map_err(|e| pipeline::Failed::Other(format!("couldn't read what's open: {e:?}")))
}

/// Has the person's agent give the rest of the path through what's open,
/// in parts, each in its order, when it gave a way in with what it opened;
/// and keeps it, with the way in.
pub async fn rest_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let Some(id) = payload["opened"].as_i64() else {
        return Ok(Then::Done);
    };
    let Ok(open) = reader::opened(app, id).await else {
        return Ok(Then::Done);
    };
    let Some(way) = view(app, &open).await else {
        return Ok(Then::Done);
    };
    if way.order.is_empty() || !way.parts.is_empty() {
        return Ok(Then::Done);
    }
    let files = readable(&open).await;
    let on: std::collections::HashSet<&str> = way.order.iter().map(|s| s.path.as_str()).collect();
    let left: Vec<&String> = files
        .iter()
        .filter(|f| !on.contains(f.as_str()))
        .take(MOST_PLACED)
        .collect();
    if left.is_empty() {
        return Ok(Then::Done);
    }
    let way_in: String = way
        .order
        .iter()
        .map(|s| format!("{}: {}\n", s.path, s.why))
        .collect();
    let lines: String = way
        .folders
        .iter()
        .map(|f| format!("{}/: {}\n", f.path, f.line))
        .collect();
    let version = agent::version(app, agent::PATH).await;
    let print = print_of(&[
        &open.row.root,
        &way_in,
        &left
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    ]);
    let Some(answer) = agent::ask(
        app,
        &format!("path:{version}:{print}"),
        agent::Ask {
            kind: agent::PATH,
            about: format!("Ordering the rest of {}", project(&open)),
            idea: None,
            opened: Some(id),
            material: json!({
                "files": left,
                "mostParts": most_parts(files.len()),
                "wayIn": fence("way", &way_in),
                "folders": fence("folders", &lines),
            }),
            schema: rest_schema(),
            priority: Priority::Now,
            follows: None,
        },
        agent::Resume::job(REST, format!("{REST}:{id}"), payload.clone()),
    )
    .await?
    else {
        return Ok(Then::Done);
    };
    let given: Vec<PartIn> =
        serde_json::from_value(answer.value["parts"].clone()).unwrap_or_default();
    keep_parts(app, &open, parts_from(&open, given, &way.order))
        .await
        .map_err(|e| {
            pipeline::Failed::Other(format!("couldn't keep the rest of the path: {e:?}"))
        })?;
    let open = reopened(app, id).await?;
    in_order(app, &open).await?;
    app.publish(ServerEvent::Opened);
    Ok(Then::Done)
}

/// Keeps the rest of the path with the way in: the project's own, for the
/// whole project, and with what was opened.
async fn keep_parts(app: &Shared, open: &Open, parts: Vec<WayPart>) -> ApiResult<()> {
    if whole(open) {
        let mut kept = kept(app, &open.root).await;
        kept.parts = parts.clone();
        kept.files = Some(files_print(open).await);
        keep(app, &open.root, &kept).await?;
    }
    if let Some(mut given) = given_of(open.row.way.as_deref()) {
        given.parts = parts;
        let json = serde_json::to_string(&given).unwrap_or_default();
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::opened_way(&mut conn, open.row.id, Some(&json)).await?;
    }
    Ok(())
}

/// Puts the reading of what's open still waiting for the agent in the
/// order of its path, ahead of the files the path doesn't name.
pub async fn in_order(app: &Shared, open: &Open) -> pipeline::Result<()> {
    let Some(way) = view(app, open).await else {
        return Ok(());
    };
    let kind = read_kind(open);
    let keys: Vec<String> = path_of(&way)
        .into_iter()
        .map(|p| format!("{kind}:{}:{p}", open.row.id))
        .collect();
    let mut conn = app.db.acquire().await?;
    kelve_store::asks::in_order(&mut conn, open.row.id, &keys).await?;
    Ok(())
}

fn step_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "path": { "type": "string" }, "why": { "type": "string" } },
        "required": ["path", "why"],
        "additionalProperties": false,
    })
}

fn parts_schema() -> Value {
    json!({
        "type": "array",
        "items": {
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "why": { "type": "string" },
                "paths": { "type": "array", "items": { "type": "string" } },
            },
            "required": ["name", "why", "paths"],
            "additionalProperties": false,
        },
    })
}

/// The shape of a way-in job's answer.
fn way_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "order": { "type": "array", "items": step_schema() },
            "parts": parts_schema(),
            "folders": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": { "path": { "type": "string" }, "line": { "type": "string" } },
                    "required": ["path", "line"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["order", "parts"],
        "additionalProperties": false,
    })
}

/// The shape of the rest of the path's answer.
fn rest_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "parts": parts_schema() },
        "required": ["parts"],
        "additionalProperties": false,
    })
}

/// The paths a job listed for the agent.
fn listed(material: &Value, field: &str) -> std::collections::HashSet<String> {
    material[field]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a step's line says why, in at most `LINE` characters.
fn why_fits(path: &str, why: &str) -> Result<(), String> {
    let why = why.trim();
    if why.is_empty() || why.chars().count() > LINE {
        return Err(format!(
            "The line for {path} must say why to read it there, in at most {LINE} characters."
        ));
    }
    Ok(())
}

/// Whether the parts given are in their shape: each named and with a line,
/// each path a file listed, placed once, not on the way in.
/// The most parts a job asked for: as its material says, else eight.
fn most_of(material: &Value) -> usize {
    material["mostParts"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(MOST_PARTS)
}

fn parts_shape(
    material: &Value,
    value: &Value,
    files: &std::collections::HashSet<String>,
    seen: &mut std::collections::HashSet<String>,
) -> Result<(), String> {
    let parts: Vec<PartIn> = serde_json::from_value(value["parts"].clone()).map_err(|e| {
        format!("It isn't in the shape asked for: {e}. Give parts, each with name, why and paths.")
    })?;
    let most = most_of(material);
    if parts.len() > most {
        return Err(format!(
            "Give at most {most} parts: what someone needs to understand it, not every file."
        ));
    }
    if let Some(p) = parts.iter().find(|p| p.paths.len() > MOST_IN_PART) {
        return Err(format!(
            "{} has {} files: give at most {MOST_IN_PART} in a part, those that show how it works, and leave the rest off the path.",
            p.name.trim(),
            p.paths.len()
        ));
    }
    for p in &parts {
        let name = p.name.trim();
        if name.is_empty() || name.chars().count() > NAME {
            return Err(format!(
                "Name each part in plain words, in at most {NAME} characters: {name:?} isn't."
            ));
        }
        let why = p.why.trim();
        if why.is_empty() || why.chars().count() > LINE {
            return Err(format!(
                "The line for {name} must say why it comes next, in at most {LINE} characters."
            ));
        }
        for path in &p.paths {
            if !files.contains(path) {
                return Err(format!(
                    "{path} isn't one of the files listed: give each path exactly as listed."
                ));
            }
            if !seen.insert(path.clone()) {
                return Err(format!("{path} is placed twice: place each file once."));
            }
        }
    }
    Ok(())
}

/// Whether a way-in job's answer is in its shape: each step a file listed,
/// once, with a short line; then the parts.
pub fn way_shape(material: &Value, value: &Value) -> Result<(), String> {
    let order: Vec<StepIn> = serde_json::from_value(value["order"].clone()).map_err(|e| {
        format!("It isn't in the shape asked for: {e}. Give order, each step with path and why.")
    })?;
    if order.len() > MOST_FOUND {
        return Err(format!(
            "Give at most {MOST_FOUND} steps in order: the rest goes in parts."
        ));
    }
    let files = listed(material, "files");
    let mut seen = std::collections::HashSet::new();
    for s in &order {
        if !files.contains(&s.path) {
            return Err(format!(
                "{} isn't one of the files listed: give each path exactly as listed.",
                s.path
            ));
        }
        if !seen.insert(s.path.clone()) {
            return Err(format!(
                "{} is in the order twice: give each file once.",
                s.path
            ));
        }
        why_fits(&s.path, &s.why)?;
    }
    parts_shape(material, value, &files, &mut seen)?;
    // Each folder's line, when it gave them: a folder listed, said briefly.
    let folders = listed(material, "folders");
    let lines: Vec<StepIn> = serde_json::from_value(value["folders"].clone()).unwrap_or_default();
    for f in &lines {
        let path = f.path.trim_end_matches('/');
        if !folders.contains(path) {
            return Err(format!(
                "{path} isn't one of the folders listed: give each folder's path exactly as listed."
            ));
        }
        why_fits(path, &f.why)?;
    }
    Ok(())
}

/// Whether the rest of the path's answer is in its shape: each part named
/// and with a line, each path a file listed, placed once.
pub fn rest_shape(material: &Value, value: &Value) -> Result<(), String> {
    let files = listed(material, "files");
    parts_shape(
        material,
        value,
        &files,
        &mut std::collections::HashSet::new(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_way_in_and_the_rest_of_the_path_name_only_files_listed_once() {
        let material =
            json!({ "files": ["README.md", "src/a.rs", "src/b.rs"], "folders": ["src"] });
        let step = |p: &str| json!({ "path": p, "why": "Where it starts." });
        let way = |order: Vec<Value>| json!({ "order": order, "parts": [] });
        assert!(way_shape(&material, &way(vec![step("README.md")])).is_ok());
        assert!(
            way_shape(&material, &way(vec![step("other.rs")]))
                .unwrap_err()
                .contains("isn't one of the files listed")
        );
        assert!(
            way_shape(&material, &way(vec![step("src/a.rs"), step("src/a.rs")]))
                .unwrap_err()
                .contains("twice")
        );
        assert!(
            way_shape(
                &material,
                &way(vec![
                    json!({ "path": "src/a.rs", "why": "x".repeat(LINE + 1) })
                ])
            )
            .is_err()
        );
        let many: Vec<Value> = (0..=MOST_FOUND).map(|_| step("src/a.rs")).collect();
        assert!(
            way_shape(&material, &way(many))
                .unwrap_err()
                .contains("at most")
        );
        let part = |name: &str, paths: &[&str]| json!({ "name": name, "why": "It comes next.", "paths": paths });
        // The way in, then the rest in parts, each file placed once.
        assert!(
            way_shape(
                &material,
                &json!({ "order": [step("README.md")], "parts": [part("The bank", &["src/a.rs"]), part("Tests", &["src/b.rs"])] })
            )
            .is_ok()
        );
        assert!(
            way_shape(
                &material,
                &json!({ "order": [step("src/a.rs")], "parts": [part("The bank", &["src/a.rs"])] })
            )
            .unwrap_err()
            .contains("placed twice")
        );
        assert!(way_shape(&material, &json!({ "order": [step("README.md")] })).is_err());
        // Each folder's line, for a folder listed.
        let with = |path: &str| json!({ "order": [step("README.md")], "parts": [], "folders": [{ "path": path, "line": "The code." }] });
        assert!(way_shape(&material, &with("src")).is_ok());
        assert!(way_shape(&material, &with("src/")).is_ok());
        assert!(
            way_shape(&material, &with("elsewhere"))
                .unwrap_err()
                .contains("folders listed")
        );
        assert!(
            rest_shape(
                &material,
                &json!({ "parts": [part("The bank", &["src/a.rs"]), part("Tests", &["src/b.rs"])] })
            )
            .is_ok()
        );
        assert!(
            rest_shape(
                &material,
                &json!({ "parts": [part("A", &["src/a.rs"]), part("B", &["src/a.rs"])] })
            )
            .unwrap_err()
            .contains("placed twice")
        );
        assert!(
            rest_shape(
                &material,
                &json!({ "parts": [part(&"n".repeat(NAME + 1), &["src/a.rs"])] })
            )
            .is_err()
        );
        assert!(rest_shape(&material, &json!({ "parts": [part("", &[])] })).is_err());
        // Parts for understanding, never every file.
        let many: Vec<Value> = (0..=MOST_PARTS)
            .map(|n| part(&format!("Part {n}"), &[]))
            .collect();
        assert!(
            rest_shape(&material, &json!({ "parts": many.clone() }))
                .unwrap_err()
                .contains("at most")
        );
        // A big project may have more parts, as its job says.
        let roomy = json!({ "files": ["README.md"], "mostParts": most_parts(BIG + 1) });
        assert!(rest_shape(&roomy, &json!({ "parts": many })).is_ok());
        assert_eq!(most_parts(BIG), MOST_PARTS);
        let files: Vec<String> = (0..=MOST_IN_PART).map(|n| format!("src/f{n}.rs")).collect();
        let material = json!({ "files": files });
        let big = json!({ "name": "Everything", "why": "All of it.", "paths": files });
        assert!(
            rest_shape(&material, &json!({ "parts": [big] }))
                .unwrap_err()
                .contains("at most")
        );
    }

    #[test]
    fn a_way_kept_once_as_only_its_order_reads_as_one() {
        let old = given_of(Some(r#"[{"path":"a.rs","why":"First."}]"#)).unwrap();
        assert_eq!(old.order.len(), 1);
        assert!(old.parts.is_empty());
        let new = given_of(Some(r#"{"order":[{"path":"a.rs","why":"First."}],"parts":[{"name":"Then","why":"Next.","steps":[{"path":"b.rs","why":"Next."}]}]}"#)).unwrap();
        assert_eq!(new.parts[0].steps[0].path, "b.rs");
        assert!(given_of(None).is_none());
    }

    #[test]
    fn paths_stay_inside_and_lines_stay_plain() {
        assert_eq!(clean("./src//bank/").as_deref(), Some("src/bank"));
        assert_eq!(clean("../etc"), None);
        assert_eq!(clean("src/../../x"), None);
        assert_eq!(clean("  "), None);
        assert_eq!(
            plain("Talking to the bank,\n and nothing else."),
            "Talking to the bank, and nothing else."
        );
        assert!(plain(&"word ".repeat(40)).chars().count() <= LINE + 1);
    }

    #[test]
    fn a_folders_parts_are_kept_until_a_file_changes() {
        let root = std::env::temp_dir().join(format!("kelve-parts-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/bank.rs"), "use crate::retry;\n").unwrap();
        std::fs::write(root.join("src/retry.rs"), "pub fn again() {}\n").unwrap();
        let first = parts_of(&root, "src");
        assert_eq!(first.uses, [(0, 1)]);
        assert_eq!(parts_of(&root, "src"), first, "kept");
        std::fs::write(
            root.join("src/retry.rs"),
            "use crate::bank;\npub fn again() {}\n",
        )
        .unwrap();
        assert_eq!(
            parts_of(&root, "src").uses,
            [(0, 1), (1, 0)],
            "worked out again"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
