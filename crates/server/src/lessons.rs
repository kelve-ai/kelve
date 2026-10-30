//! Lessons: what people's agents learned using the ideas kelve marks, from the
//! shared graph. kelve fetches the lessons about the ideas in what it reads,
//! gives the best to the person's agent at the moment it helps (as it reads or
//! edits a file, as something fails), keeps what came of each, and, as a turn
//! ends after a hard stretch the agent got past, keeps a job for it to write
//! what it learned, held a day before it leaves so the person can stop it.
//!
//! Only lessons, what came of them and what showed it go to the graph:
//! never the problem, the command, the file or anything else from the work.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use std::time::Instant;

use axum::Json;
use axum::extract::{Path as UrlPath, Query, State};
use chrono::Utc;
use kelve_core::ideas::IdeaRef;
use kelve_core::lesson::{self, Given, Lesson, Outcome, Shown, Standing};
use kelve_core::matching::{self, At, Candidate, Card, Context, Fit};
use kelve_core::views::{
    LearnedState, LearnedView, LessonBrief, LessonUse, LessonView, LessonsSettings, LessonsView,
    ServerEvent,
};
use kelve_graph_api::{LessonPage, LessonState};
use kelve_store::lessons as store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agent::{self, Ask, Priority, Resume};
use crate::auth::{Agent, Me};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{self, Then};
use crate::state::Shared;

/// Writing what the agent learned, after a hard stretch it got past.
pub const WRITE: &str = "lesson.write";
/// Sending it, once it's waited.
pub const SEND: &str = "lesson.send";

/// How long lessons fetched about an idea are used before fetching again.
const FRESH: i64 = 24 * 60 * 60 * 1000;
/// How often what came of lessons goes to the graph.
const EVERY: Duration = Duration::from_secs(60 * 60);
/// How many lessons an opened file or change lists before "N more".
const LISTED: usize = 5;

/// The files an agent read or edited lately, by its session: what a lesson
/// it writes can be about, and the one it edited last, where a fix went.
/// Kept in the store a day, so a restart doesn't forget them.
#[derive(Debug, Default, Clone)]
struct Touched {
    paths: Vec<String>,
    edited: Option<String>,
}

async fn touch(app: &Shared, session: &str, root: &str, path: &str, edit: bool) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    store::touch(&mut conn, session, root, path, edit).await?;
    Ok(())
}

async fn touched(app: &Shared, session: &str) -> ApiResult<Touched> {
    let mut conn = app.db.acquire().await?;
    let (_, paths, edited) = store::touched(&mut conn, session).await?;
    Ok(Touched { paths, edited })
}

/// The files touched in a folder in the last day, by any session.
async fn touched_in(app: &Shared, root: &str) -> ApiResult<Vec<String>> {
    let mut conn = app.db.acquire().await?;
    Ok(store::touched_in(&mut conn, root, kelve_store::now() - FRESH).await?)
}

/// An idea marked in a file, as kept.
#[derive(Debug, Clone, Deserialize)]
struct ReadIdea {
    key: String,
    name: String,
}

fn ideas_of(json: &str) -> Vec<ReadIdea> {
    serde_json::from_str(json).unwrap_or_default()
}

/// The ideas marked in some files of a folder, each once, by key.
async fn ideas_in(app: &Shared, root: &str, paths: &[String]) -> ApiResult<Vec<ReadIdea>> {
    let mut conn = app.db.acquire().await?;
    let mut out: Vec<ReadIdea> = Vec::new();
    for path in paths {
        if let Some(json) = kelve_store::reader::ideas_in(&mut conn, root, path).await? {
            for i in ideas_of(&json) {
                if !out.iter().any(|o| o.key == i.key) {
                    out.push(i);
                }
            }
        }
    }
    Ok(out)
}

/// Looks up which of the graph's ideas some libraries are, those not looked
/// up in a day: only the one idea of exactly its name, found in the graph's
/// name index, by an anonymous bucket of names, so the graph never learns
/// which library was asked about, and a name it doesn't have is never
/// sent. Then fetches the lessons about them.
pub async fn look_up(app: &Shared, names: &[String]) -> ApiResult<()> {
    let Some(graph) = app.graph_reader() else {
        return Ok(());
    };
    let names = {
        let mut conn = app.db.acquire().await?;
        store::to_look_up(&mut conn, names, kelve_store::now() - FRESH).await?
    };
    let mut ideas = Vec::new();
    for name in names {
        let hash = kelve_graph_api::name_hash(&name);
        let bucket = match graph.bucket(&hash[..kelve_graph_api::BUCKET_CHARS]).await {
            Ok(bucket) => bucket,
            Err(e) => {
                tracing::info!(error = %e, "couldn't look up libraries in the shared graph; kelve will try again");
                return Ok(());
            }
        };
        // The one idea of exactly its name. A name too plain for the graph
        // to call a match in prose, such as `rand`, is one an import names.
        let exact: Vec<&kelve_graph_api::Candidate> = bucket
            .names
            .get(&hash)
            .map(|c| {
                c.iter()
                    .filter(|c| kelve_core::ideas::key(&c.name) == kelve_core::ideas::key(&name))
                    .collect()
            })
            .unwrap_or_default();
        let sure = match exact.as_slice() {
            [one] => Some(*one),
            _ => None,
        };
        let mut conn = app.db.acquire().await?;
        if let Some(c) = sure {
            kelve_store::entries::keep_idea(
                &mut conn,
                &kelve_store::entries::Idea {
                    key: c.id.clone(),
                    name: c.name.clone(),
                    definition: c.definition.clone(),
                    kind: kelve_core::ideas::IdeaKind::Tool.as_str().into(),
                    became: None,
                },
            )
            .await?;
            ideas.push(c.id.clone());
        }
        store::looked_up(&mut conn, &name, sure.map(|c| c.id.as_str())).await?;
    }
    refresh(app, &ideas).await
}

/// Of the libraries a file imports, those that aren't the work's own: not
/// its folder's name, and not a module or package in it (`from shop.store
/// import ...` in a project with `shop/`).
pub fn not_own(root: &Path, names: Vec<String>) -> Vec<String> {
    let folder = root
        .file_name()
        .map(|n| n.to_string_lossy().replace('-', "_"))
        .unwrap_or_default();
    names
        .into_iter()
        .filter(|n| {
            n.replace('-', "_") != folder
                && ![root.to_path_buf(), root.join("src"), root.join("lib")]
                    .iter()
                    .any(|d| d.join(n).exists() || d.join(format!("{n}.py")).exists())
        })
        .collect()
}

/// Looks up libraries in the background.
pub fn look_up_soon(app: &Shared, names: Vec<String>) {
    if names.is_empty() {
        return;
    }
    let app = app.clone();
    tokio::spawn(async move {
        if let Err(e) = look_up(&app, &names).await {
            tracing::warn!(error = %e.message, "couldn't look up the libraries a file uses");
        }
    });
}

/// The graph's ideas for the libraries some files import.
async fn used_in(app: &Shared, root: &str, paths: &[String]) -> ApiResult<Vec<ReadIdea>> {
    let mut conn = app.db.acquire().await?;
    Ok(store::used_in(&mut conn, root, paths)
        .await?
        .into_iter()
        .map(|(key, name)| ReadIdea { key, name })
        .collect())
}

/// The most ideas a lesson job offers.
const OFFERED: usize = 12;

/// What a lesson can be about: the shared graph's ideas marked in the files
/// the agent touched, the libraries they import, and the tools it runs; then those of the rest
/// of the folder, since what failed, a build or a test, is often about the
/// whole of it, as an edition set in `Cargo.toml` is. Only the graph's: a
/// lesson is about ideas it has.
async fn lesson_ideas(app: &Shared, root: &str, touched: &[String]) -> ApiResult<Vec<ReadIdea>> {
    let rest: Vec<String> = {
        let mut conn = app.db.acquire().await?;
        let mut rest: Vec<String> = kelve_store::reader::read_files(&mut conn, root)
            .await?
            .into_iter()
            .map(|f| f.path)
            .collect();
        // A folder walked for its imports: its files' libraries.
        for p in store::walked_files(&mut conn, root).await? {
            if !rest.contains(&p) {
                rest.push(p);
            }
        }
        rest.retain(|p| !touched.contains(p));
        rest
    };
    let mut out = ideas_in(app, root, touched).await?;
    let more = [
        used_in(app, root, touched).await?,
        used_in(app, root, &[COMMANDS.to_string()]).await?,
        ideas_in(app, root, &rest).await?,
        used_in(app, root, &rest).await?,
    ];
    for i in more.into_iter().flatten() {
        if !out.iter().any(|o| o.key == i.key) {
            out.push(i);
        }
    }
    out.retain(|i| kelve_core::ideas::is_id(&i.key));
    out.truncate(OFFERED);
    Ok(out)
}

/// Fetches the lessons about ideas from the graph, those not fetched in a
/// day: the graph's ideas only.
pub async fn refresh(app: &Shared, keys: &[String]) -> ApiResult<()> {
    let Some(graph) = app.graph_reader() else {
        return Ok(());
    };
    let ids: Vec<String> = keys
        .iter()
        .filter(|k| kelve_core::ideas::is_id(k))
        .cloned()
        .collect();
    let stale = {
        let mut conn = app.db.acquire().await?;
        store::stale(&mut conn, &ids, kelve_store::now() - FRESH).await?
    };
    if stale.is_empty() {
        return Ok(());
    }
    let got = match graph.lessons(&stale).await {
        Ok(got) => got,
        Err(e) => {
            tracing::info!(error = %e, "couldn't fetch lessons from the shared graph; kelve will try again");
            return Ok(());
        }
    };
    let kept: Vec<(String, Vec<String>, String)> = got
        .lessons
        .iter()
        .map(|p| {
            (
                p.id.clone(),
                p.lesson.about.clone(),
                serde_json::to_string(p).unwrap_or_default(),
            )
        })
        .collect();
    let mut conn = app.db.acquire().await?;
    store::keep_fetched(&mut conn, &stale, &kept).await?;
    Ok(())
}

/// Fetches lessons about these ideas in the background.
pub fn refresh_soon(app: &Shared, keys: Vec<String>) {
    let app = app.clone();
    tokio::spawn(async move {
        if let Err(e) = refresh(&app, &keys).await {
            tracing::warn!(error = %e.message, "couldn't keep the lessons fetched");
        }
    });
}

/// The lessons kept about ideas, live, best first: each with the key of
/// the idea it was found by.
async fn kept_about(app: &Shared, keys: &[String]) -> ApiResult<Vec<(LessonPage, String)>> {
    let mut conn = app.db.acquire().await?;
    let mut out: Vec<(LessonPage, String)> = store::about(&mut conn, keys)
        .await?
        .into_iter()
        .filter_map(|(_, by, page)| Some((serde_json::from_str::<LessonPage>(&page).ok()?, by)))
        .filter(|(p, _)| p.state == LessonState::Live)
        .collect();
    out.sort_by(|a, b| b.0.record.rank().total_cmp(&a.0.record.rank()));
    Ok(out)
}

/// The names of the ideas lessons are about, by key, as kelve knows them.
async fn names(app: &Shared, pages: &[&LessonPage]) -> ApiResult<BTreeMap<String, String>> {
    let mut conn = app.db.acquire().await?;
    let mut out = BTreeMap::new();
    for p in pages {
        for key in &p.lesson.about {
            if out.contains_key(key) {
                continue;
            }
            if let Some(i) = kelve_store::entries::idea(&mut conn, key).await? {
                out.insert(key.clone(), i.name);
            }
        }
    }
    Ok(out)
}

/// Lessons as the agent reads them, each with whether kelve couldn't tell
/// it holds for this work, so the agent checks first, and whether it was
/// given as the agent wrote the usual way it replaces.
async fn for_agent(app: &Shared, pages: &[(&LessonPage, bool, bool)]) -> ApiResult<String> {
    let refs: Vec<&LessonPage> = pages.iter().map(|(p, ..)| *p).collect();
    let names = names(app, &refs).await?;
    let given: Vec<Given> = pages
        .iter()
        .map(|(p, check, instead)| Given {
            id: &p.id,
            lesson: &p.lesson,
            about: p
                .lesson
                .about
                .iter()
                .filter_map(|k| names.get(k).map(String::as_str))
                .collect(),
            record: p.record,
            check: *check,
            instead: *instead,
        })
        .collect();
    Ok(lesson::for_agent(&given))
}

/// A lesson's card: the graph's, or, when it sent none, one made here from
/// the lesson's own words.
fn card_of(p: &LessonPage) -> Card {
    if p.card == Card::default() {
        matching::card(&p.lesson)
    } else {
        p.card.clone()
    }
}

/// How long the work's context is used before it's read again.
const CONTEXT_FOR: Duration = Duration::from_secs(60);

static CONTEXTS: LazyLock<Mutex<HashMap<String, (Instant, Context)>>> =
    LazyLock::new(Default::default);

/// The work's context, read from its folder (`engine::context`), kept a
/// minute: what tells whether a lesson holds for it.
fn context(root: &str) -> Context {
    if let Ok(kept) = CONTEXTS.lock()
        && let Some((at, ctx)) = kept.get(root)
        && at.elapsed() < CONTEXT_FOR
    {
        return ctx.clone();
    }
    let ctx = kelve_engine::context::read(Path::new(root));
    if let Ok(mut kept) = CONTEXTS.lock() {
        if kept.len() > 50 {
            kept.clear();
        }
        kept.insert(root.to_string(), (Instant::now(), ctx.clone()));
    }
    ctx
}

/// A uniform draw in [0, 1), from the operating system.
fn draw() -> f64 {
    let mut bytes = [0u8; 8];
    if getrandom::fill(&mut bytes).is_err() {
        return 0.5;
    }
    (u64::from_le_bytes(bytes) >> 11) as f64 / (1u64 << 53) as f64
}

/// A file's text, as kelve matches lessons against it: never sent anywhere.
fn file_text(file: &str) -> String {
    const MOST: u64 = 512 * 1024;
    match std::fs::metadata(file) {
        Ok(m) if m.len() <= MOST => std::fs::read_to_string(file).unwrap_or_default(),
        _ => String::new(),
    }
}

/// A moment in the agent's work, as its hook saw it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MomentIn {
    /// `read`, `edit`, `ran`, `failed`, `prompt`, `next` or `end`.
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
}

/// What kelve has for the agent at a moment: text to put before it, or
/// nothing.
#[derive(Debug, Clone, Default, Serialize)]
pub struct MomentOut {
    pub text: String,
}

/// A moment in the agent's work: the lesson that helps now, if one does;
/// what came of lessons given before it; when something that failed passes,
/// a job to write what it learned; and, when it hands work over, to show it
/// in kelve.
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
    // now, in the background, the first time the hook meets it.
    let read_root = {
        let mut conn = app.db.acquire().await?;
        match kelve_store::reader::root_of(&mut conn, &cwd).await? {
            Some(root) => Some(root),
            None => match store::walked_root_of(&mut conn, &cwd).await? {
                Some(root) => Some(root),
                None => match walk_folder(&cwd) {
                    Some(folder) => {
                        if store::start_walk(&mut conn, &folder).await? {
                            walk_soon(&app, folder.clone());
                        }
                        Some(folder)
                    }
                    None => None,
                },
            },
        }
    };
    let root = read_root.clone().unwrap_or_else(|| cwd.clone());
    let agent = m.agent.as_deref().unwrap_or_default().trim();
    let before = {
        let mut conn = app.db.acquire().await?;
        store::moment_now(&mut conn, session).await?
    };
    let text = match m.what.as_str() {
        "read" | "edit" => match &m.file {
            Some(file) => {
                let edit = m.what == "edit";
                on_file(
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
                {
                    let mut conn = app.db.acquire().await?;
                    store::passed_now(&mut conn, session).await?;
                }
                on_pass(&app, session, command).await?;
                if command.contains("git commit") {
                    on_commit(&app, session).await?;
                }
                notice_tools(&app, &root, command).await?;
                let given = on_command(&app, session, agent, &root, command).await?;
                [wrote, given]
                    .into_iter()
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            None => String::new(),
        },
        // The turn ends, or, for an agent that can't say so, the person
        // writes again: what the stretch taught, asked once.
        "end" | "prompt" => ask_stretch(&app, session).await?,
        "failed" => {
            let command = m.command.clone().unwrap_or_default();
            let problem = lesson::seen_as(m.problem.as_deref().unwrap_or_default());
            let wrote = shell_writes(&app, session, agent, read_root.as_deref(), before).await?;
            notice_tools(&app, &root, &command).await?;
            let failed = on_failure(&app, session, agent, &root, &command, &problem).await?;
            [wrote, failed]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }
        // About to call its model: what was kept for it.
        "next" => {
            let mut conn = app.db.acquire().await?;
            store::take_held(&mut conn, session).await?.join("\n\n")
        }
        _ => String::new(),
    };
    // Work handed over: shown in kelve, if it hasn't been.
    let text = match &m.handover {
        Some(h) => [text, crate::handoff::after(&app, h).await?]
            .into_iter()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        None => text,
    };
    if m.hold && !text.is_empty() {
        let mut conn = app.db.acquire().await?;
        store::hold_for(&mut conn, session, &text).await?;
        return Ok(Json(MomentOut::default()));
    }
    Ok(Json(MomentOut { text }))
}

/// How many lessons were given in a session, and which.
async fn given_so_far(app: &Shared, session: &str) -> ApiResult<Vec<store::Given>> {
    let mut conn = app.db.acquire().await?;
    Ok(store::given_in(&mut conn, session).await?)
}

/// The most files a walk for imports looks through.
pub const WALK_MOST: usize = kelve_engine::git::WRITTEN_WALK;

/// The folder to walk for an agent working in `cwd`: the git repository
/// around it, or, outside one, `cwd` itself; never the home folder, the
/// root, a folder that holds the home folder, or a system folder. In the
/// form the agent names it, so its files' paths are found as it names them.
pub fn walk_folder(cwd: &str) -> Option<String> {
    let dir = Path::new(cwd);
    if !dir.is_absolute() || !dir.is_dir() {
        return None;
    }
    let folder = dir
        .ancestors()
        .find(|d| d.join(".git").exists())
        .unwrap_or(dir);
    let home = std::env::home_dir();
    if home.as_deref().is_some_and(|h| within(h, folder)) || folder.parent().is_none() {
        return None;
    }
    // Windows's own folders and what's in them, wherever it keeps them.
    const WINDOWS: [&str; 4] = [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ];
    if cfg!(windows)
        && WINDOWS
            .iter()
            .filter_map(std::env::var_os)
            .any(|s| within(folder, Path::new(&s)))
    {
        return None;
    }
    // The system's own folders and what's in them; a folder of the
    // system's that holds people's work, such as /var or /opt, only itself.
    const INSIDE: [&str; 12] = [
        "/bin",
        "/boot",
        "/dev",
        "/etc",
        "/lib",
        "/proc",
        "/sbin",
        "/sys",
        "/usr",
        "/System",
        "/Library",
        "/private/etc",
    ];
    const ITSELF: [&str; 6] = [
        "/var",
        "/opt",
        "/private",
        "/Applications",
        "/Volumes",
        "/tmp",
    ];
    if INSIDE.iter().any(|s| folder.starts_with(s)) || ITSELF.iter().any(|s| folder == Path::new(s))
    {
        return None;
    }
    Some(folder.display().to_string())
}

/// Whether `path` is `folder` or in it, as the system tells folders apart:
/// on Windows, in any case, by either separator, and with or without the
/// `\\?\` a whole path may start with.
pub(crate) fn within(path: &Path, folder: &Path) -> bool {
    if !cfg!(windows) {
        return path.starts_with(folder);
    }
    let plain = |p: &Path| {
        let p = p.to_string_lossy().replace('\\', "/").to_lowercase();
        let p = p.strip_prefix("//?/").unwrap_or(&p);
        p.trim_end_matches('/').to_string()
    };
    let (path, folder) = (plain(path), plain(folder));
    path == folder || path.starts_with(&format!("{folder}/"))
}

/// Walks a folder kelve hasn't opened, in the background: once.
pub fn walk_soon(app: &Shared, root: String) {
    let app = app.clone();
    tokio::spawn(async move {
        if let Err(e) = walk(&app, &root).await {
            tracing::warn!(error = %e.message, "couldn't walk a folder for the libraries its files import");
        }
    });
}

/// Walks a folder for what its files import, as kelve's walk finds them
/// (its ignore rules, nothing kelve won't read by name), at most `WALK_MOST`
/// files: each one's libraries kept and looked up in the graph by their
/// names alone, so lessons about them reach the agent there. Nothing read,
/// marked or shown; nothing run.
pub async fn walk(app: &Shared, root: &str) -> ApiResult<()> {
    let dir = std::path::PathBuf::from(root);
    let (files, used, more) = tokio::task::spawn_blocking(move || {
        let Ok(walked) = kelve_engine::walk::walk_at_most(&dir, WALK_MOST) else {
            return (Vec::new(), Vec::new(), false);
        };
        let mut files = Vec::new();
        let mut used: Vec<(String, Vec<String>)> = Vec::new();
        for f in walked.files {
            if f.skip.is_some() {
                continue;
            }
            if kelve_core::code::language(&f.rel).is_some()
                && let Ok(text) = kelve_engine::read::read(&dir, &f.rel)
            {
                let names = not_own(
                    &dir,
                    kelve_engine::candidates::libraries(&f.rel, &text.text),
                );
                if !names.is_empty() {
                    used.push((f.rel.clone(), names));
                }
            }
            files.push(f.rel);
        }
        (files, used, walked.more)
    })
    .await
    .map_err(ApiError::internal)?;
    let mut names: Vec<String> = Vec::new();
    {
        let mut conn = app.db.acquire().await?;
        for (path, libs) in &used {
            store::keep_uses(&mut conn, root, path, libs).await?;
            for n in libs {
                if !names.contains(n) {
                    names.push(n.clone());
                }
            }
        }
        store::keep_walk(&mut conn, root, &files, more).await?;
    }
    if more {
        tracing::info!(
            most = WALK_MOST,
            "a folder has more files than kelve walks for their imports; the rest aren't seen"
        );
    }
    look_up(app, &names).await
}

/// A file's path in a folder, `/` between its parts, when it's in it and
/// kelve would read it by its name.
fn new_in(root: &str, file: &str) -> Option<String> {
    let rel = Path::new(file).strip_prefix(root).ok()?;
    let rel: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let rel = rel.join("/");
    (!rel.is_empty() && kelve_engine::walk::by_name(&rel).is_none()).then_some(rel)
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
        let read = {
            let mut conn = app.db.acquire().await?;
            kelve_store::reader::read_at(&mut conn, &file).await?
        };
        // Files kelve has read, and new ones the agent made in the folder;
        // never kelve's own, as what it keeps of what it read.
        let Some((at, path)) = read
            .map(|(at, path, _)| (at, path))
            .or_else(|| new_in(root, &file).map(|rel| (root.to_string(), rel)))
        else {
            continue;
        };
        if crate::reader::is_kelves(&path) {
            continue;
        }
        let Ok(now) = kelve_engine::read::read(Path::new(&at), &path) else {
            continue;
        };
        let before = {
            let mut conn = app.db.acquire().await?;
            store::seen_text(&mut conn, session, &at, &path).await?
        };
        if before.is_some_and(|b| lesson::only_whitespace(&b, &now.text)) {
            let mut conn = app.db.acquire().await?;
            store::keep_seen(&mut conn, session, &at, &path, &now.text).await?;
            continue;
        }
        let text = on_file(
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

/// The agent read or edited a file: a lesson about the ideas kelve marked in
/// it, only if one fits the work and matters here (`core::matching`):
/// most reads and edits give nothing.
#[allow(clippy::too_many_arguments)]
async fn on_file(
    app: &Shared,
    session: &str,
    agent: &str,
    file: &str,
    edit: bool,
    written: Option<&str>,
    replaced: Option<&str>,
    in_root: Option<&str>,
) -> ApiResult<String> {
    let read = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::read_at(&mut conn, file).await?
    };
    let unread = read.is_none();
    // A file kelve hasn't read, in a folder it knows: one the agent made, or
    // one in a folder walked for its imports, nothing marked in it.
    let found = read.or_else(|| {
        let rel = new_in(in_root?, file)?;
        Some((in_root?.to_string(), rel, "[]".to_string()))
    });
    let Some((root, path, ideas)) = found else {
        return Ok(String::new());
    };
    touch(app, session, &root, &path, edit).await?;
    // What a file kelve hasn't read imports, as it is now: so a file made
    // or changed since a walk is found by its libraries too.
    if edit && unread {
        let text = file_text(file);
        let used = not_own(
            Path::new(&root),
            kelve_engine::candidates::libraries(&path, &text),
        );
        {
            let mut conn = app.db.acquire().await?;
            store::keep_uses(&mut conn, &root, &path, &used).await?;
            store::keep_walked(&mut conn, &root, &path).await?;
        }
        look_up(app, &used).await?;
    }
    if edit {
        kept_writing(app, session, &root, &path, file, written, replaced).await?;
        if let Ok(seen) = kelve_engine::read::read(Path::new(&root), &path) {
            let mut conn = app.db.acquire().await?;
            store::keep_seen(&mut conn, session, &root, &path, &seen.text).await?;
        }
    }
    // The ideas marked in it, and the libraries it imports.
    let mut keys: Vec<String> = ideas_of(&ideas).into_iter().map(|i| i.key).collect();
    for i in used_in(app, &root, std::slice::from_ref(&path)).await? {
        if !keys.contains(&i.key) {
            keys.push(i.key);
        }
    }
    refresh_soon(app, keys.clone());
    let mut pages: Vec<LessonPage> = kept_about(app, &keys)
        .await?
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    // A better way about an idea elsewhere in the folder, as its usual way
    // is written here: a file's ideas are marked as it was read, before the
    // agent wrote the usual way in it.
    if let Some(wrote) = written.filter(|_| edit) {
        let elsewhere: Vec<String> = {
            let mut conn = app.db.acquire().await?;
            let mut out: Vec<String> = Vec::new();
            for f in kelve_store::reader::read_files(&mut conn, &root).await? {
                for i in ideas_of(&f.ideas) {
                    if !keys.contains(&i.key) && !out.contains(&i.key) {
                        out.push(i.key);
                    }
                }
            }
            out
        };
        for (p, _) in kept_about(app, &elsewhere).await? {
            let cued = p
                .lesson
                .seen_as
                .as_deref()
                .and_then(lesson::cue)
                .is_some_and(|c| matching::cue_in(&c, wrote));
            if cued && !pages.iter().any(|q| q.id == p.id) {
                pages.push(p);
            }
        }
        // A better way whose cue is only where it's already written its
        // way: nothing to give.
        pages.retain(|p| !lesson::only_better(&p.lesson, wrote));
    }
    if pages.is_empty() {
        return Ok(String::new());
    }
    let text = file_text(file);
    let at = if edit {
        At::Edited {
            written: written.unwrap_or_default(),
            file: &text,
        }
    } else {
        At::Read { file: &text }
    };
    let moment = Moment {
        session,
        agent,
        kind: "read",
        root: &root,
        path: &path,
        problem: "",
        command: "",
    };
    choose_and_give(app, &moment, at, &pages, false).await
}

/// What the tools the agent runs are kept under: no file, the folder's own.
const COMMANDS: &str = "";

/// The tools a command runs (`core::matching::tools_in`), kept as what the
/// agent uses in the folder, and found in the graph by name in the
/// background, as libraries are: so lessons about `cargo`, `pytest` or
/// `pgrep` reach it with no file read. Never the work's own scripts.
async fn notice_tools(app: &Shared, root: &str, command: &str) -> ApiResult<()> {
    let tools = not_own(Path::new(root), matching::tools_in(command));
    if tools.is_empty() {
        return Ok(());
    }
    {
        let mut conn = app.db.acquire().await?;
        store::add_uses(&mut conn, root, COMMANDS, &tools).await?;
    }
    look_up_soon(app, tools);
    Ok(())
}

/// A command the agent ran: a lesson about a tool it runs here whose usual
/// way is in the command, given as it's run, as an edit's is as it's
/// written; never one already done its way.
async fn on_command(
    app: &Shared,
    session: &str,
    agent: &str,
    root: &str,
    command: &str,
) -> ApiResult<String> {
    let keys: Vec<String> = used_in(app, root, &[COMMANDS.to_string()])
        .await?
        .into_iter()
        .map(|i| i.key)
        .collect();
    if keys.is_empty() {
        return Ok(String::new());
    }
    let pages: Vec<LessonPage> = kept_about(app, &keys)
        .await?
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| {
            p.lesson
                .seen_as
                .as_deref()
                .and_then(lesson::cue)
                .is_some_and(|c| matching::cue_in(&c, command))
        })
        .filter(|p| !lesson::only_better(&p.lesson, command))
        .collect();
    if pages.is_empty() {
        return Ok(String::new());
    }
    // Kept as a lesson given as the usual way is written, with no file:
    // the commit, which checks a file, leaves it be.
    let moment = Moment {
        session,
        agent,
        kind: "read",
        root,
        path: "",
        problem: "",
        command,
    };
    let at = At::Edited {
        written: command,
        file: command,
    };
    choose_and_give(app, &moment, at, &pages, false).await
}

/// The fewest lines of its own earlier work an edit must replace to be
/// replaced work: more than a line or two.
const REPLACED_LINES: usize = 3;
/// The most lines of each way a lesson job is given.
const WAY_LINES: usize = 40;
/// The most jobs for replaced work in a session.
const REPLACED_JOBS: i64 = 2;

/// An edit's text, as kelve keeps it: what the agent wrote, so a later
/// edit that replaces it is seen as replaced work; and, for a lesson given
/// as it wrote the usual way, whether the usual way has since left the
/// file, done the lesson's way.
async fn kept_writing(
    app: &Shared,
    session: &str,
    root: &str,
    path: &str,
    file: &str,
    written: Option<&str>,
    replaced: Option<&str>,
) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    let text = file_text(file);
    if let Some(new) = written {
        let earlier = store::written_in(&mut conn, session, root, path).await?;
        // A whole file written over says nothing of what it replaced: what
        // it replaced is what the agent last wrote there that's gone, and
        // what replaced it what's new since.
        let (gone, came) = match (replaced, earlier.last()) {
            (None, Some(last)) => (Some(gone_from(last, &text)), Some(gone_from(new, last))),
            _ => (None, None),
        };
        let old = replaced.or(gone.as_deref()).unwrap_or_default();
        if replaces_own(old, &earlier.join("\n")) {
            store::keep_replaced(
                &mut conn,
                session,
                root,
                path,
                &first_lines(old, WAY_LINES),
                &first_lines(came.as_deref().unwrap_or(new), WAY_LINES),
            )
            .await?;
        }
    }
    if let Some(new) = written {
        store::keep_written(&mut conn, session, root, path, new).await?;
    }
    // Lessons given as it wrote the usual way: done its way once the line
    // it wrote has left the file.
    for g in store::given_in(&mut conn, session).await? {
        if !g.instead || g.done || g.path != path || g.outcome != "unknown" {
            continue;
        }
        if !lesson::still_in(&g.usual, &text) {
            store::done_its_way(&mut conn, g.id).await?;
        }
    }
    Ok(())
}

/// The lines of `earlier` no longer in `now`, in their order.
fn gone_from(earlier: &str, now: &str) -> String {
    let still: std::collections::HashSet<&str> = now.lines().map(str::trim).collect();
    earlier
        .lines()
        .filter(|l| !still.contains(l.trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether what an edit replaced is the agent's own earlier work: more
/// than a line or two of it, most of them lines it wrote.
fn replaces_own(old: &str, earlier: &str) -> bool {
    let lines: Vec<&str> = old.lines().map(str::trim).filter(|l| l.len() > 2).collect();
    if lines.len() < REPLACED_LINES {
        return false;
    }
    let mine: std::collections::HashSet<&str> = earlier.lines().map(str::trim).collect();
    let found = lines.iter().filter(|l| mine.contains(*l)).count();
    found * 5 >= lines.len() * 3
}

/// The first lines of some text, at most `most`.
fn first_lines(text: &str, most: usize) -> String {
    text.lines().take(most).collect::<Vec<_>>().join("\n")
}

/// Replaced work whose second way stands: a job to write what was learned,
/// if the graph has ideas it can be about; two a session at most.
async fn ask_replaced(app: &Shared, session: &str) -> ApiResult<String> {
    let (waiting, asked) = {
        let mut conn = app.db.acquire().await?;
        store::replaced_waiting(&mut conn, session).await?
    };
    let Some(first) = waiting.first() else {
        return Ok(String::new());
    };
    let ids: Vec<i64> = waiting.iter().map(|r| r.id).collect();
    let ideas = lesson_ideas(app, &first.root, std::slice::from_ref(&first.path)).await?;
    if asked >= REPLACED_JOBS || ideas.is_empty() {
        let mut conn = app.db.acquire().await?;
        store::replaced_settled(&mut conn, &ids, false).await?;
        return Ok(String::new());
    }
    // The last of what replaced the first: one job for the file.
    let last = waiting
        .iter()
        .rev()
        .find(|r| r.path == first.path)
        .unwrap_or(first);
    pipeline::add(
        app,
        WRITE,
        Some(&format!("{WRITE}:replaced:{}", last.id)),
        json!({ "replaced": last.id, "root": first.root, "files": [first.path] }),
        Utc::now(),
    )
    .await
    .map_err(ApiError::internal)?;
    {
        let mut conn = app.db.acquire().await?;
        store::replaced_settled(&mut conn, &ids, true).await?;
    }
    let names: Vec<&str> = ideas.iter().take(3).map(|i| i.name.as_str()).collect();
    Ok(format!(
        "kelve: you replaced what you wrote in {}. If the second way is better for anyone using {}, and a capable agent wouldn't choose it unprompted or find it in the docs, write it as a lesson: take kelve's next job. Otherwise answer none.",
        first.path,
        names.join(", ")
    ))
}

/// The work was committed: each lesson given as the agent wrote the usual
/// way, then done the lesson's way, held if it stayed so, and didn't if
/// the usual way came back. In code, with no model.
async fn on_commit(app: &Shared, session: &str) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    for g in store::given_in(&mut conn, session).await? {
        // What stayed outweighs what the agent said of it, so a lesson it
        // already said held is settled again by what the commit shows.
        if !g.instead || !g.done || g.path.is_empty() {
            continue;
        }
        let text = file_text(&Path::new(&g.root).join(&g.path).display().to_string());
        let (outcome, shown) = if lesson::still_in(&g.usual, &text) {
            (Outcome::Didnt, Shown::ChangedBack)
        } else {
            (Outcome::Held, Shown::Stayed)
        };
        came_of(app, &mut conn, g.id, outcome, shown, "", !g.withheld).await?;
    }
    drop(conn);
    app.publish(ServerEvent::Lessons);
    Ok(())
}

/// Where and when lessons are given.
struct Moment<'a> {
    session: &'a str,
    /// The agent, as its hook names it.
    agent: &'a str,
    kind: &'a str,
    root: &'a str,
    path: &'a str,
    problem: &'a str,
    command: &'a str,
}

/// Chooses the lessons for a moment from those offered, and gives them:
/// none given before in the session, none that doesn't hold, none that
/// isn't relevant; one in twenty that fits held back to measure the lessons,
/// unless the same problem came again.
async fn choose_and_give(
    app: &Shared,
    m: &Moment<'_>,
    at: At<'_>,
    pages: &[LessonPage],
    again: bool,
) -> ApiResult<String> {
    let so_far = given_so_far(app, m.session).await?;
    let given = so_far.iter().filter(|g| !g.withheld).count();
    if given >= lesson::MOST_IN_A_SESSION {
        return Ok(String::new());
    }
    // Never twice in a session, but for the error a lesson warned of
    // happening after all: then it's given again, once.
    let given_before = |id: &str| {
        so_far
            .iter()
            .any(|g| g.lesson == id && (m.kind != "failed" || g.moment == "failed"))
    };
    let pages: Vec<&LessonPage> = pages
        .iter()
        .filter(|p| p.state == LessonState::Live && !given_before(&p.id))
        .collect();
    let cards: Vec<Card> = pages.iter().map(|p| card_of(p)).collect();
    let candidates: Vec<Candidate> = pages
        .iter()
        .zip(&cards)
        .map(|(p, card)| Candidate {
            id: &p.id,
            about: &p.lesson.about,
            card,
            record: p.record,
            standing: p.standing,
            seen_as: p.lesson.seen_as.is_some(),
        })
        .collect();
    // What the agent just wrote, when it's the usual way a lesson replaces.
    let wrote = match at {
        At::Edited { written, .. } => written,
        _ => "",
    };
    // Not when it's already written the lesson's way (`usual_line_of`).
    let usual = |p: &LessonPage| lesson::usual_line_of(&p.lesson, wrote);
    let ctx = context(m.root);
    let most = lesson::MOST_AT_A_MOMENT.min(lesson::MOST_IN_A_SESSION - given);
    let chosen = matching::choose(at, &candidates, &ctx, most, &mut draw);
    if chosen.is_empty() {
        return Ok(String::new());
    }
    let mut out: Vec<(&LessonPage, bool, bool)> = Vec::new();
    {
        let mut conn = app.db.acquire().await?;
        for c in &chosen {
            let Some(page) = pages.iter().find(|p| p.id == c.id) else {
                continue;
            };
            // One in twenty held back, to measure the lessons: never the same
            // problem twice. Only a lesson widely used already: below that, its
            // every use is its own evidence, and it's always given.
            let hold_back = if page.given >= app.config.hold_back_from {
                app.config.hold_back
            } else {
                0
            };
            let withheld = !again && draw() * 1000.0 < hold_back as f64;
            store::give(
                &mut conn,
                &store::Give {
                    lesson: &page.id,
                    session: m.session,
                    moment: m.kind,
                    root: m.root,
                    path: m.path,
                    problem: m.problem,
                    command: m.command,
                    agent: m.agent,
                    withheld,
                    instead: usual(page).is_some(),
                    usual: usual(page).as_deref().unwrap_or_default(),
                    chance: if again {
                        1000
                    } else if withheld {
                        hold_back
                    } else {
                        1000 - hold_back
                    },
                },
            )
            .await?;
            if !withheld {
                crate::counts::count_on(app, &mut conn, "lessons.given").await;
                out.push((page, c.fit != Fit::Holds, usual(page).is_some()));
            }
        }
    }
    app.publish(ServerEvent::Lessons);
    if out.is_empty() {
        return Ok(String::new());
    }
    for_agent(app, &out).await
}

/// Something failed: the lesson that answers it, if there is one; and if a
/// lesson given for the same problem was meant to fix it, it didn't. The
/// problem is told by its fingerprint: the tool, its codes and its words.
async fn on_failure(
    app: &Shared,
    session: &str,
    agent: &str,
    root: &str,
    command: &str,
    problem: &str,
) -> ApiResult<String> {
    let print = matching::fingerprint(command, problem);
    let so_far = given_so_far(app, session).await?;
    let mut again = false;
    {
        let mut conn = app.db.acquire().await?;
        for g in so_far
            .iter()
            .filter(|g| g.moment == "failed" && g.outcome == "unknown")
        {
            if matching::same_problem(&matching::fingerprint(&g.command, &g.problem), &print) {
                again = true;
                came_of(
                    app,
                    &mut conn,
                    g.id,
                    Outcome::Didnt,
                    Shown::CameBack,
                    "",
                    !g.withheld,
                )
                .await?;
            }
        }
        again |= store::still_failing(&mut conn, session, command)
            .await?
            .iter()
            .any(|f| {
                matching::same_problem(&matching::fingerprint(&f.command, &f.problem), &print)
            });
        let files = serde_json::to_string(&touched(app, session).await?.paths)
            .unwrap_or_else(|_| "[]".into());
        store::failed(&mut conn, session, root, command, problem, &files).await?;
    }
    if problem.is_empty() {
        return Ok(String::new());
    }
    // The lessons kept that are seen as a problem: those about the ideas in
    // what kelve has read, as fetched.
    let pages: Vec<LessonPage> = {
        let mut conn = app.db.acquire().await?;
        store::seen_as_any(&mut conn)
            .await?
            .iter()
            .filter_map(|p| serde_json::from_str::<LessonPage>(p).ok())
            .collect()
    };
    let moment = Moment {
        session,
        agent,
        kind: "failed",
        root,
        path: "",
        problem,
        command,
    };
    choose_and_give(app, &moment, At::Failed(&print), &pages, again).await
}

/// A command passed: what failed under it before has passed now, and what
/// failed under the same tool with the same problem, as another form of
/// the command. Lessons given for it held (and those held back, the error
/// didn't come back without them), and what they answered isn't asked
/// about. What a hard stretch taught is asked as the turn ends
/// (`ask_stretch`), not here.
async fn on_pass(app: &Shared, session: &str, command: &str) -> ApiResult<()> {
    let failing = {
        let mut conn = app.db.acquire().await?;
        let all = store::unpassed(&mut conn, session).await?;
        let same: Vec<matching::Fingerprint> = all
            .iter()
            .filter(|f| f.command == command)
            .map(|f| matching::fingerprint(&f.command, &f.problem))
            .collect();
        let tool = matching::tool_of(command);
        all.into_iter()
            .filter(|f| {
                f.command == command
                    || (matching::tool_of(&f.command) == tool
                        && same.iter().any(|p| {
                            matching::same_problem(
                                p,
                                &matching::fingerprint(&f.command, &f.problem),
                            )
                        }))
            })
            .collect::<Vec<_>>()
    };
    if failing.is_empty() {
        return Ok(());
    }
    let mut held = false;
    let fixed_in = touched(app, session).await?.edited;
    {
        let mut conn = app.db.acquire().await?;
        let ids: Vec<i64> = failing.iter().map(|f| f.id).collect();
        store::passed(&mut conn, &ids).await?;
        for g in store::given_in(&mut conn, session).await? {
            if g.moment == "failed" && g.command == command && g.outcome == "unknown" {
                // Shown on the file the fix went into.
                if let Some(path) = &fixed_in
                    && g.path.is_empty()
                {
                    store::set_path(&mut conn, g.id, path).await?;
                }
                came_of(
                    app,
                    &mut conn,
                    g.id,
                    Outcome::Held,
                    Shown::Passed,
                    "",
                    !g.withheld,
                )
                .await?;
                held |= !g.withheld;
            }
        }
    }
    app.publish(ServerEvent::Lessons);
    // A lesson given for it held: what it took is answered already.
    if held {
        let mut conn = app.db.acquire().await?;
        let ids: Vec<i64> = failing.iter().map(|f| f.id).collect();
        store::settle(&mut conn, &ids, false).await?;
    }
    Ok(())
}

/// The most stretches of failures asked about in a session.
const STRETCH_JOBS: i64 = 3;

/// The turn ends, or the person writes again: what failed since kelve last
/// asked is one stretch, whatever the commands. When it was hard
/// (`lesson::Took::hard`, from the first failure to the last) and the agent got
/// past it, a command passing after the last failure, the agent is asked once
/// what it learned, with what it replaced on the way; when it wasn't, it's let
/// go, and what was replaced is asked alone. A turn that ends still failing
/// waits for the one that gets past it.
async fn ask_stretch(app: &Shared, session: &str) -> ApiResult<String> {
    let (failing, passed, asked) = {
        let mut conn = app.db.acquire().await?;
        (
            store::stretch(&mut conn, session).await?,
            store::last_passed(&mut conn, session).await?,
            store::stretches_asked(&mut conn, session).await?,
        )
    };
    let Some(last) = failing.iter().map(|f| f.failed_at).max() else {
        return ask_replaced(app, session).await;
    };
    if passed.is_none_or(|p| p <= last) {
        return Ok(String::new());
    }
    let ids: Vec<i64> = failing.iter().map(|f| f.id).collect();
    let took = took(&failing);
    let root = failing[0].root.clone();
    let t = touched(app, session).await?;
    let ideas = if took.hard() && asked < STRETCH_JOBS {
        lesson_ideas(app, &root, &t.paths).await?
    } else {
        Vec::new()
    };
    if ideas.is_empty() {
        {
            let mut conn = app.db.acquire().await?;
            store::settle(&mut conn, &ids, false).await?;
        }
        return ask_replaced(app, session).await;
    }
    let first = &failing[0];
    // Work it replaced on the way is part of the same job: one question.
    let replaced = {
        let mut conn = app.db.acquire().await?;
        let (waiting, _) = store::replaced_waiting(&mut conn, session).await?;
        let replaced_ids: Vec<i64> = waiting.iter().map(|r| r.id).collect();
        if !replaced_ids.is_empty() {
            store::replaced_settled(&mut conn, &replaced_ids, true).await?;
        }
        waiting.last().map(|r| (r.id, r.path.clone()))
    };
    let mut payload = json!({ "failed": ids, "root": root, "files": t.paths });
    if let Some((id, _)) = &replaced {
        payload["replaced"] = json!(id);
    }
    pipeline::add(
        app,
        WRITE,
        Some(&format!("{WRITE}:{}", first.id)),
        payload,
        Utc::now(),
    )
    .await
    .map_err(ApiError::internal)?;
    {
        let mut conn = app.db.acquire().await?;
        store::settle(&mut conn, &ids, true).await?;
    }
    let names: Vec<&str> = ideas.iter().take(3).map(|i| i.name.as_str()).collect();
    let (also, or) = match &replaced {
        Some((_, path)) => (
            format!(", and you replaced what you wrote in {path} on the way"),
            ", or the second way is better and a capable agent wouldn't choose it unprompted",
        ),
        None => (String::new(), ""),
    };
    Ok(format!(
        "kelve: this turn took {}, first with \"{}\"{also}. If what finally worked wasn't what the errors pointed to{or}, the docs don't say it, and it would help anyone using {}, write it as a lesson: take kelve's next job. Otherwise answer none.",
        took.what(),
        kelve_core::text::clip(&first.problem, 120),
        names.join(", ")
    ))
}

/// What it took, from the first of some failures to the last.
fn took(failing: &[store::Failed]) -> lesson::Took {
    let runs: Vec<(i64, &str)> = failing
        .iter()
        .map(|f| (f.failed_at, f.problem.as_str()))
        .collect();
    let last = runs.iter().map(|(at, _)| *at).max().unwrap_or_default();
    lesson::Took::of(&runs, last)
}

/// Gives lessons the agent asked for, and keeps that it was given each:
/// never one kelve knows doesn't hold for the work.
#[allow(clippy::too_many_arguments)]
async fn give(
    app: &Shared,
    session: &str,
    agent: &str,
    moment: &str,
    root: &str,
    path: &str,
    problem: &str,
    command: &str,
    pages: &[LessonPage],
) -> ApiResult<String> {
    let ctx = context(root);
    let pages: Vec<(&LessonPage, Fit)> = pages
        .iter()
        .map(|p| (p, matching::fits(&card_of(p), &ctx)))
        .filter(|(_, fit)| !matches!(fit, Fit::Not(_)))
        .collect();
    if pages.is_empty() {
        return Ok(String::new());
    }
    {
        let mut conn = app.db.acquire().await?;
        for (p, _) in &pages {
            store::give(
                &mut conn,
                &store::Give {
                    lesson: &p.id,
                    session,
                    moment,
                    root,
                    path,
                    problem,
                    command,
                    agent,
                    withheld: false,
                    instead: false,
                    usual: "",
                    chance: 1000,
                },
            )
            .await?;
            crate::counts::count_on(app, &mut conn, "lessons.given").await;
        }
    }
    app.publish(ServerEvent::Lessons);
    let refs: Vec<(&LessonPage, bool, bool)> = pages
        .iter()
        .map(|(p, fit)| (*p, *fit != Fit::Holds, false))
        .collect();
    for_agent(app, &refs).await
}

/// The answer to a lesson job.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Written {
    #[serde(default)]
    lessons: Vec<WrittenLesson>,
    #[serde(default)]
    same: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WrittenLesson {
    about: Vec<String>,
    says: String,
    #[serde(rename = "do")]
    act: String,
    #[serde(default)]
    example: Option<String>,
    holds_when: String,
    #[serde(default)]
    seen_as: Option<String>,
    #[serde(default)]
    backed_by: Option<lesson::Backing>,
}

/// The shape a lesson job's answer must have.
fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["lessons", "same"],
        "properties": {
            "lessons": {
                "type": "array",
                "maxItems": lesson::MOST_A_JOB,
                "items": {
                    "type": "object",
                    "required": ["about", "says", "do", "holdsWhen"],
                    "properties": {
                        "about": { "type": "array", "items": { "type": "string" }, "minItems": 1, "maxItems": lesson::MOST_ABOUT },
                        "says": { "type": "string", "maxLength": lesson::MOST_SAYS },
                        "do": { "type": "string", "maxLength": lesson::MOST_DO },
                        "example": { "type": "string", "maxLength": lesson::MOST_EXAMPLE },
                        "holdsWhen": { "type": "string", "maxLength": lesson::MOST_HOLDS },
                        "seenAs": { "type": "string", "maxLength": lesson::MOST_SEEN },
                        "backedBy": {
                            "type": "object",
                            "required": ["url", "quote"],
                            "properties": { "url": { "type": "string" }, "quote": { "type": "string" } }
                        }
                    }
                }
            },
            "same": { "type": "array", "items": { "type": "string" }, "description": "The ids of lessons already there that say what you learned." }
        }
    })
}

/// A lesson the agent wrote, with its ideas matched: the lesson, about the
/// graph's IDs, and their names. Or why it can't be taken.
fn matched(
    w: &WrittenLesson,
    ideas: &[(String, String)],
    work: &[&str],
) -> Result<(Lesson, Vec<String>), String> {
    let mut about = Vec::new();
    let mut names = Vec::new();
    for name in &w.about {
        let want = kelve_core::ideas::key(name);
        match ideas
            .iter()
            .find(|(_, n)| kelve_core::ideas::key(n) == want)
        {
            Some((key, n)) if kelve_core::ideas::is_id(key) => {
                about.push(key.clone());
                names.push(n.clone());
            }
            Some((_, n)) => {
                return Err(format!(
                    "{n} isn't in the shared graph yet, so a lesson can't be about it. Choose another of the ideas listed."
                ));
            }
            None => {
                return Err(format!(
                    "\"{name}\" isn't one of the ideas listed. A lesson is about the ideas listed, by name exactly as given."
                ));
            }
        }
    }
    let l = Lesson {
        about,
        says: w.says.trim().to_string(),
        act: w.act.trim().to_string(),
        example: w.example.clone().filter(|e| !e.trim().is_empty()),
        holds_when: w.holds_when.trim().to_string(),
        backed_by: w.backed_by.clone(),
        seen_as: w
            .seen_as
            .as_deref()
            .map(lesson::seen_as)
            .filter(|s| !s.is_empty()),
    };
    let problems = lesson::problems(&l, work);
    if !problems.is_empty() {
        return Err(problems.join(" "));
    }
    Ok((l, names))
}

/// The ideas a lesson job offers, by key and name, from its material.
fn offered(material: &Value) -> Vec<(String, String)> {
    material["offered"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|i| {
                    Some((
                        i["key"].as_str()?.to_string(),
                        i["name"].as_str()?.to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn work_names(material: &Value) -> Vec<String> {
    material["work"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a lesson job's answer is in its shape, with every lesson one
/// kelve can send: why not, in sentences the agent can act on.
pub fn shape(material: &Value, value: &Value) -> Result<(), String> {
    let w: Written = serde_json::from_value(value.clone())
        .map_err(|_| "The answer is `lessons`, a list, and `same`, a list of ids: give both, empty when there's nothing.".to_string())?;
    if w.lessons.len() > lesson::MOST_A_JOB {
        return Err(format!("At most {} lessons a job.", lesson::MOST_A_JOB));
    }
    let known: Vec<&str> = material["known"]
        .as_array()
        .map(|a| a.iter().filter_map(|k| k.as_str()).collect())
        .unwrap_or_default();
    if let Some(bad) = w.same.iter().find(|s| !known.contains(&s.as_str())) {
        return Err(format!(
            "{bad} isn't one of the lessons listed: `same` takes their ids."
        ));
    }
    let ideas = offered(material);
    let work = work_names(material);
    let work: Vec<&str> = work.iter().map(String::as_str).collect();
    let first = first_way(material);
    let failed = material["failed"]
        .as_str()
        .filter(|f| !f.contains("Nothing failed."));
    // Every problem at once, so one fix can mend them all.
    let mut problems = Vec::new();
    for (n, l) in w.lessons.iter().enumerate() {
        if let Err(why) = matched(l, &ideas, &work) {
            problems.push(format!("Lesson {}: {why}", n + 1));
        }
        // A better way is found again by the usual way it replaces, so that
        // has to be as it's written: in code, the next agent's own line. A
        // lesson about what failed is seen as the problem, or not at all.
        let Some((first, second)) = &first else {
            continue;
        };
        let seen_as = l
            .seen_as
            .as_deref()
            .map(lesson::seen_as)
            .filter(|s| !s.is_empty());
        let seen = match seen_as.as_deref() {
            Some(s) => {
                (lesson::in_a_line(s, first) && !lesson::in_a_line(s, second))
                    || failed.is_some_and(|f| lesson::written_as(s, f))
            }
            None => failed.is_some(),
        };
        if !seen {
            // The lines that went, as written in the material it was given,
            // to choose from: code, not what's said about it. The agent
            // takes the work's own names out.
            let still: std::collections::HashSet<&str> = second.lines().map(str::trim).collect();
            let gone: Vec<String> = first
                .lines()
                .map(str::trim)
                .filter(|l| l.chars().count() >= 6 && !still.contains(l))
                .filter(|l| !l.starts_with(['#', '"', '\'']) && !l.starts_with("//"))
                .take(5)
                .map(|l| format!("`{l}`"))
                .collect();
            let such = if gone.is_empty() {
                String::new()
            } else {
                format!(" These went: {}.", gone.join(", "))
            };
            problems.push(format!(
                "Lesson {}: for a better way, `seenAs` is code, never a description: the usual way it replaces, as it's written in the first way and gone from the second, one line of it or the start of one, with the work's own names taken out.{such} For what failed, it's the problem as given.",
                n + 1
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join(" "))
    }
}

/// The first way of replaced work and the second, from a lesson job's
/// material, when something was replaced.
fn first_way(material: &Value) -> Option<(String, String)> {
    const OPEN: &str = "The first way:\n```\n";
    const SECOND: &str = "\n```\nThe second way:\n```\n";
    let text = material["replaced"].as_str()?;
    let start = text.find(OPEN)? + OPEN.len();
    let end = start + text[start..].find(SECOND)?;
    let rest = &text[end + SECOND.len()..];
    let second = &rest[..rest.rfind("\n```").unwrap_or(rest.len())];
    Some((text[start..end].to_string(), second.to_string()))
}

/// What a file's name gives away: its name, and its stem when it's long
/// enough to be the work's own.
fn names_from(root: &str, files: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(name) = Path::new(root).file_name().and_then(|n| n.to_str()) {
        out.push(name.to_string());
    }
    for f in files {
        let p = Path::new(f);
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            out.push(name.to_string());
        }
        if let Some(stem) = p.file_stem().and_then(|n| n.to_str())
            && stem.len() >= 8
        {
            out.push(stem.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Writing what the agent learned: asks it, with what failed, the ideas in
/// the files it worked on and the lessons about them already; then keeps
/// what it wrote, waiting to leave.
pub async fn write_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let ids: Vec<i64> = payload["failed"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let root = payload["root"].as_str().unwrap_or_default().to_string();
    let files: Vec<String> = payload["files"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let (failed, replaced) = {
        let mut conn = app.db.acquire().await?;
        let replaced = match payload["replaced"].as_i64() {
            Some(id) => store::replaced_one(&mut conn, id).await?,
            None => None,
        };
        (store::failed_by_id(&mut conn, &ids).await?, replaced)
    };
    let ideas = lesson_ideas(app, &root, &files)
        .await
        .map_err(|e| pipeline::Failed::Other(e.message))?;
    let keys: Vec<String> = ideas.iter().map(|i| i.key.clone()).collect();
    refresh(app, &keys)
        .await
        .map_err(|e| pipeline::Failed::Other(e.message))?;
    let known = kept_about(app, &keys)
        .await
        .map_err(|e| pipeline::Failed::Other(e.message))?;
    let failed_text: String = failed
        .iter()
        .map(|f| {
            format!(
                "- `{}` failed with: {}",
                kelve_core::text::clip(&kelve_core::redact::redact(&f.command), 160),
                f.problem
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    // What it took, from the first failure to the last.
    let failed_text = if failed.is_empty() {
        "Nothing failed.".to_string()
    } else {
        format!("{failed_text}\n{}", took(&failed).line())
    };
    // Replaced work: the first way and the second, each as it was written.
    let replaced_text = replaced
        .as_ref()
        .map(|r| {
            format!(
                "In {}, the agent replaced what it wrote earlier.\nThe first way:\n```\n{}\n```\nThe second way:\n```\n{}\n```",
                r.path,
                kelve_core::redact::redact(&r.first),
                kelve_core::redact::redact(&r.second)
            )
        })
        .unwrap_or_else(|| "Nothing was replaced.".into());
    let lessons_text: String = known
        .iter()
        .map(|(p, _)| {
            format!(
                "- {}: {} Do: {} Holds when: {}",
                p.id, p.lesson.says, p.lesson.act, p.lesson.holds_when
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let ideas_text: String = ideas
        .iter()
        .map(|i| format!("- {}", i.name))
        .collect::<Vec<_>>()
        .join("\n");
    let material = json!({
        "failed": kelve_core::fence::fence("failed", &failed_text),
        "replaced": kelve_core::fence::fence("replaced", &replaced_text),
        "ideas": kelve_core::fence::fence("ideas", &ideas_text),
        "lessons": kelve_core::fence::fence("lessons", if lessons_text.is_empty() { "None yet." } else { &lessons_text }),
        "known": known.iter().map(|(p, _)| p.id.clone()).collect::<Vec<_>>(),
        "offered": ideas.iter().map(|i| json!({ "key": i.key, "name": i.name })).collect::<Vec<_>>(),
        "work": names_from(&root, &files),
    });
    // One job for each thing it's about, so a job run again asks again only
    // for what it asked before.
    let key = match (&replaced, ids.first()) {
        (Some(r), _) => format!("{WRITE}:replaced:{}", r.id),
        (None, Some(id)) => format!("{WRITE}:{id}"),
        (None, None) => format!("{WRITE}:asked:{payload}"),
    };
    let names: Vec<&str> = ideas.iter().take(2).map(|i| i.name.as_str()).collect();
    let Some(answer) = agent::ask(
        app,
        &key,
        Ask {
            kind: kelve_core::instructions::LESSON,
            about: format!("What was learned about {}", names.join(" and ")),
            idea: None,
            opened: None,
            material: material.clone(),
            schema: schema(),
            priority: Priority::Now,
            follows: None,
        },
        Resume::job(WRITE, key.clone(), payload.clone()),
    )
    .await?
    else {
        return Ok(Then::Done);
    };
    let Ok(w) = serde_json::from_value::<Written>(answer.value) else {
        return Ok(Then::Done);
    };
    let offered = offered(&answer.material);
    let work = work_names(&answer.material);
    let work: Vec<&str> = work.iter().map(String::as_str).collect();
    let mut conn = app.db.acquire().await?;
    // The same as one there: counted as confirming it.
    for id in &w.same {
        let given = store::give(
            &mut conn,
            &store::Give {
                lesson: id,
                session: &format!("job:{key}"),
                moment: "asked",
                root: &root,
                agent: answer.agent.as_deref().unwrap_or_default(),
                ..Default::default()
            },
        )
        .await?;
        came_of(
            app,
            &mut conn,
            given,
            Outcome::Held,
            Shown::Said,
            "it learned the same",
            true,
        )
        .await?;
    }
    let leaves = kelve_store::now() + app.config.lesson_hold * 1000;
    for l in &w.lessons {
        let Ok((lesson, names)) = matched(l, &offered, &work) else {
            continue;
        };
        let id = store::learn(
            &mut conn,
            &serde_json::to_string(&lesson).unwrap_or_default(),
            &serde_json::to_string(&names).unwrap_or_default(),
            &root,
            leaves,
            replaced.as_ref(),
        )
        .await?;
        crate::counts::count_on(app, &mut conn, "lessons.written").await;
        kelve_store::jobs::add(
            &mut conn,
            SEND,
            Some(&format!("{SEND}:{id}")),
            &json!({ "learned": id }),
            kelve_store::at(leaves),
        )
        .await?;
    }
    drop(conn);
    app.publish(ServerEvent::Lessons);
    Ok(Then::Done)
}

fn state_of(s: LessonState) -> &'static str {
    match s {
        LessonState::Checking => "checking",
        LessonState::Live => "live",
        LessonState::Waiting => "held",
        LessonState::TurnedAway => "turnedAway",
        LessonState::Same => "same",
    }
}

/// Sends what the agent learned, once it's waited and the person didn't
/// stop it; then follows the graph's checks of it until they're done.
pub async fn send_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let id = payload["learned"].as_i64().unwrap_or_default();
    let row = {
        let mut conn = app.db.acquire().await?;
        store::learned_one(&mut conn, id).await?
    };
    let Some(row) = row else {
        return Ok(Then::Done);
    };
    let later = |mins: i64| Ok(Then::Again(Utc::now() + chrono::Duration::minutes(mins)));
    // The graph checks most lessons in a minute or two: asked every minute
    // for a quarter of an hour after it left, then every ten.
    let checked_soon = kelve_store::now() - row.leaves_at < 15 * 60 * 1000;
    let poll = if checked_soon { 1 } else { 10 };
    match row.state.as_str() {
        "waiting" if row.leaves_at > kelve_store::now() => {
            return Ok(Then::Again(kelve_store::at(row.leaves_at)));
        }
        "waiting" => {}
        "checking" => {
            let Some(graph) = app.graph_reader() else {
                return later(60);
            };
            let Some(gid) = row.graph_id.as_deref() else {
                return Ok(Then::Done);
            };
            return match graph.lesson(gid).await {
                Ok(Some(page)) if page.state != LessonState::Checking => {
                    let mut conn = app.db.acquire().await?;
                    store::left(
                        &mut conn,
                        id,
                        state_of(page.state),
                        Some(gid),
                        page.same_as.as_deref(),
                        page.why.as_deref(),
                    )
                    .await?;
                    drop(conn);
                    app.publish(ServerEvent::Lessons);
                    Ok(Then::Done)
                }
                _ => later(poll),
            };
        }
        _ => return Ok(Then::Done),
    }
    if !app.settings().contribute {
        let mut conn = app.db.acquire().await?;
        store::left(
            &mut conn,
            id,
            "waiting",
            None,
            None,
            Some("Kept here: adding to the shared graph is off."),
        )
        .await?;
        return later(24 * 60);
    }
    let Some(graph) = app.graph_writer() else {
        let mut conn = app.db.acquire().await?;
        store::left(
            &mut conn,
            id,
            "waiting",
            None,
            None,
            Some("Waiting until you sign in to the shared graph."),
        )
        .await?;
        return later(60);
    };
    let Ok(lesson) = serde_json::from_str::<Lesson>(&row.lesson) else {
        return Ok(Then::Done);
    };
    let key = format!("lesson-{}-{id}", row.written_at);
    match graph.add_lesson(&lesson, &key).await {
        Ok(page) => {
            let mut conn = app.db.acquire().await?;
            store::left(
                &mut conn,
                id,
                state_of(page.state),
                Some(&page.id),
                page.same_as.as_deref(),
                page.why.as_deref(),
            )
            .await?;
            drop(conn);
            app.publish(ServerEvent::Lessons);
            if page.state == LessonState::Checking {
                return later(poll);
            }
            Ok(Then::Done)
        }
        Err(kelve_graph_api::client::Error::Refused { status: 422, said }) => {
            let mut conn = app.db.acquire().await?;
            store::left(&mut conn, id, "turnedAway", None, None, Some(&said)).await?;
            drop(conn);
            app.publish(ServerEvent::Lessons);
            Ok(Then::Done)
        }
        Err(kelve_graph_api::client::Error::Enough(limit)) => Ok(Then::Again(
            Utc::now()
                + chrono::Duration::from_std(limit.after).unwrap_or(chrono::Duration::hours(1)),
        )),
        Err(e) => {
            tracing::info!(error = %e, "a lesson didn't go to the shared graph; kelve will try again");
            later(30)
        }
    }
}

/// Sends what came of lessons given and hasn't gone: how many went.
pub async fn send_outcomes(app: &Shared) -> kelve_store::Result<usize> {
    if !app.settings().contribute {
        return Ok(0);
    }
    let Some(graph) = app.graph_writer() else {
        return Ok(0);
    };
    let waiting = {
        let mut conn = app.db.acquire().await?;
        store::outcomes_unsent(&mut conn, kelve_graph_api::MOST_OUTCOMES as i64).await?
    };
    let outcomes: Vec<kelve_graph_api::LessonOutcome> = waiting
        .iter()
        .filter(|(_, lesson, ..)| lesson.starts_with("kelve:l"))
        .filter_map(|(_, lesson, outcome, shown, withheld, chance)| {
            Some(kelve_graph_api::LessonOutcome {
                lesson: lesson.clone(),
                outcome: Outcome::parse(outcome)?,
                shown: Shown::parse(shown)?,
                withheld: *withheld,
                chance: u16::try_from(*chance).unwrap_or(1000),
            })
        })
        .collect();
    if waiting.is_empty() {
        return Ok(0);
    }
    if !outcomes.is_empty()
        && let Err(e) = graph
            .send_outcomes(&kelve_graph_api::Outcomes { outcomes })
            .await
    {
        tracing::info!(error = %e, "what came of lessons didn't go; kelve will try again later");
        return Ok(0);
    }
    let ids: Vec<i64> = waiting.iter().map(|w| w.0).collect();
    let mut conn = app.db.acquire().await?;
    store::outcomes_sent(&mut conn, &ids).await?;
    Ok(ids.len())
}

/// Sends what came of lessons, once an hour, for as long as kelve runs; as
/// often as lessons are held, when kelve's told to hold them a shorter
/// while (`KELVE_LESSON_HOLD`), so trying lessons out needn't wait an hour.
pub fn start(app: &Shared) {
    let app = app.clone();
    let every = EVERY.min(Duration::from_secs(
        app.config.lesson_hold.max(5).unsigned_abs(),
    ));
    tokio::spawn(async move {
        loop {
            tokio::select! {
                () = tokio::time::sleep(every) => {},
                () = app.stopped() => return,
            }
            if let Err(e) = send_outcomes(&app).await {
                tracing::warn!(error = %e, "couldn't send what came of lessons");
            }
        }
    });
}

/// What the agent asks for: lessons about ideas, or that answer a problem,
/// and, with `write`, a job to write what it learned.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowIn {
    /// The folder it works in.
    pub path: String,
    #[serde(default)]
    pub ideas: Vec<String>,
    #[serde(default)]
    pub problem: Option<String>,
    #[serde(default)]
    pub write: bool,
    /// The agent, as it named itself.
    #[serde(default)]
    pub agent: Option<String>,
}

/// Lessons, as the agent asks for them.
pub async fn know(
    State(app): State<Shared>,
    _agent: Agent,
    Json(k): Json<KnowIn>,
) -> ApiResult<Json<MomentOut>> {
    crate::counts::count(&app, "lessons.asked").await;
    let root = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::root_of(&mut conn, &k.path)
            .await?
            .unwrap_or_else(|| k.path.clone())
    };
    // The ideas named, as kelve knows them, or as the graph does by their
    // exact names, as a library's is found; else those in the work lately,
    // with the libraries and tools it uses.
    look_up(&app, &k.ideas).await?;
    let mut keys: Vec<String> = Vec::new();
    {
        let mut conn = app.db.acquire().await?;
        for name in &k.ideas {
            if let Some((i, _)) = kelve_store::entries::find(&mut conn, name, 1)
                .await?
                .into_iter()
                .next()
                && kelve_core::ideas::key(&i.name) == kelve_core::ideas::key(name)
            {
                keys.push(i.key);
            }
        }
    }
    let lately = touched_in(&app, &root).await?;
    if keys.is_empty() {
        keys = lesson_ideas(&app, &root, &lately)
            .await?
            .into_iter()
            .map(|i| i.key)
            .collect();
    }
    refresh(&app, &keys).await?;
    let problem = k
        .problem
        .as_deref()
        .map(lesson::seen_as)
        .unwrap_or_default();
    let answers = |p: &LessonPage| {
        p.lesson
            .seen_as
            .as_deref()
            .is_some_and(|s| lesson::answers(s, &problem))
    };
    let mut pages: Vec<LessonPage> = kept_about(&app, &keys)
        .await?
        .into_iter()
        .map(|(p, _)| p)
        .filter(|p| problem.is_empty() || answers(p))
        .collect();
    // A problem: whatever lesson kelve has that answers it, about anything.
    if !problem.is_empty() {
        let mut conn = app.db.acquire().await?;
        for p in store::seen_as_any(&mut conn)
            .await?
            .iter()
            .filter_map(|p| serde_json::from_str::<LessonPage>(p).ok())
        {
            if answers(&p) && !pages.iter().any(|q| q.id == p.id) {
                pages.push(p);
            }
        }
    }
    let pages: Vec<LessonPage> = pages
        .into_iter()
        .filter(|p| p.standing != Standing::Doubted)
        .take(lesson::MOST_IN_A_SESSION)
        .collect();
    let mut text = if pages.is_empty() {
        "The shared graph has no lessons about that yet.".to_string()
    } else {
        give(
            &app,
            &format!("asked:{root}"),
            k.agent.as_deref().unwrap_or_default(),
            "asked",
            &root,
            "",
            &problem,
            "",
            &pages,
        )
        .await?
    };
    if k.write {
        let files = if lately.is_empty() {
            Vec::new()
        } else {
            lately
        };
        if lesson_ideas(&app, &root, &files).await?.is_empty() {
            text.push_str("\n\nThere's nothing here a lesson could be about yet: none of the libraries or tools kelve has seen this work use is in the shared graph. Name what it's about in ideas, and ask again.");
        } else {
            pipeline::add(
                &app,
                WRITE,
                Some(&format!("{WRITE}:asked:{}", kelve_store::now())),
                json!({ "failed": [], "root": root, "files": files }),
                Utc::now(),
            )
            .await
            .map_err(ApiError::internal)?;
            text.push_str("\n\nkelve kept a job for what you learned: take it with next_job.");
        }
    }
    Ok(Json(MomentOut { text }))
}

/// The agent says whether a lesson held.
#[derive(Debug, Deserialize)]
pub struct HeldIn {
    pub path: String,
    pub lesson: String,
    /// Whether it held, where it applied.
    #[serde(default)]
    pub held: Option<bool>,
    /// False when it doesn't apply here at all: kelve's miss, not the
    /// lesson's, so it never counts against it.
    #[serde(default)]
    pub applies: Option<bool>,
    #[serde(default)]
    pub why: String,
    /// The agent, as it named itself.
    #[serde(default)]
    pub agent: Option<String>,
}

pub async fn held(
    State(app): State<Shared>,
    _agent: Agent,
    Json(h): Json<HeldIn>,
) -> ApiResult<Json<MomentOut>> {
    let why = kelve_core::text::clip(&kelve_core::redact::redact(h.why.trim()), 200);
    let mut conn = app.db.acquire().await?;
    let root = kelve_store::reader::root_of(&mut conn, &h.path)
        .await?
        .unwrap_or_else(|| h.path.clone());
    let given = store::given_at(&mut conn, &root, &[]).await?;
    let id = match given.iter().find(|g| g.lesson == h.lesson && !g.withheld) {
        Some(g) => g.id,
        None => {
            if store::page(&mut conn, &h.lesson).await?.is_none() {
                return Err(ApiError::not_found(
                    "kelve hasn't given you that lesson. Give its id exactly as it was given.",
                ));
            }
            store::give(
                &mut conn,
                &store::Give {
                    lesson: &h.lesson,
                    session: &format!("asked:{root}"),
                    moment: "asked",
                    root: &root,
                    agent: h.agent.as_deref().unwrap_or_default(),
                    ..Default::default()
                },
            )
            .await?
        }
    };
    let (outcome, shown) = match (h.applies, h.held) {
        (Some(false), _) => (Outcome::Unknown, Shown::NotHere),
        (_, Some(true)) => (Outcome::Held, Shown::Said),
        (_, Some(false)) => (Outcome::Didnt, Shown::Said),
        (_, None) => {
            return Err(ApiError::bad_request(
                "Say whether the lesson held, with held, or that it doesn't apply here, with applies false.",
            ));
        }
    };
    came_of(&app, &mut conn, id, outcome, shown, &why, true).await?;
    drop(conn);
    app.publish(ServerEvent::Lessons);
    Ok(Json(MomentOut {
        text: "Thanks: kelve keeps what came of it, and tells the shared graph, with nothing from your work.".into(),
    }))
}

/// A lesson as the reader shows it, with each time it was given in this
/// work.
fn view(
    page: &LessonPage,
    names: &BTreeMap<String, String>,
    used: &[store::Given],
    agent: &str,
) -> LessonView {
    LessonView {
        id: page.id.clone(),
        says: page.lesson.says.clone(),
        act: page.lesson.act.clone(),
        example: page.lesson.example.clone(),
        holds_when: page.lesson.holds_when.clone(),
        backed_by: page.lesson.backed_by.clone(),
        about: page
            .lesson
            .about
            .iter()
            .map(|k| IdeaRef {
                id: k.clone(),
                name: names.get(k).cloned().unwrap_or_else(|| k.clone()),
            })
            .collect(),
        standing: page.standing,
        record: page.record,
        basis: lesson::basis(page.record),
        instead: page
            .lesson
            .seen_as
            .clone()
            .filter(|seen| lesson::usual_way(seen)),
        another: Vec::new(),
        used: used
            .iter()
            .filter(|g| g.lesson == page.id && !g.withheld)
            .map(|g| {
                let outcome = Outcome::parse(&g.outcome).unwrap_or(Outcome::Unknown);
                let shown = Shown::parse(&g.shown).unwrap_or(Shown::Nothing);
                LessonUse {
                    outcome,
                    shown,
                    line: if g.session == PERSON {
                        "didn't hold for you, you said".to_string()
                    } else {
                        // The agent it was given to; the one seen last when
                        // it wasn't kept.
                        let who = if g.agent.is_empty() {
                            agent.to_string()
                        } else {
                            shown_name(&g.agent)
                        };
                        lesson::shown_line(outcome, shown, &who, &g.said)
                    },
                    path: g.path.clone(),
                    // Given as the agent wrote the usual way it replaces.
                    moment: if g.instead {
                        "wrote".to_string()
                    } else {
                        g.moment.clone()
                    },
                    at: kelve_store::at(g.given_at).into(),
                }
            })
            .collect(),
    }
}

fn learned_view(l: &store::Learned) -> LearnedView {
    let lesson: Option<Lesson> = serde_json::from_str(&l.lesson).ok();
    let names: Vec<String> = serde_json::from_str(&l.names).unwrap_or_default();
    let state = match l.state.as_str() {
        "stopped" => LearnedState::Stopped,
        "checking" => LearnedState::Checking,
        "live" => LearnedState::Live,
        "held" => LearnedState::Held,
        "same" => LearnedState::Same,
        "turnedAway" => LearnedState::TurnedAway,
        _ => LearnedState::Waiting,
    };
    LearnedView {
        id: l.id,
        says: lesson.as_ref().map(|x| x.says.clone()).unwrap_or_default(),
        act: lesson.as_ref().map(|x| x.act.clone()).unwrap_or_default(),
        holds_when: lesson
            .as_ref()
            .map(|x| x.holds_when.clone())
            .unwrap_or_default(),
        about: lesson
            .as_ref()
            .map(|x| {
                x.about
                    .iter()
                    .zip(names.iter())
                    .map(|(id, name)| IdeaRef {
                        id: id.clone(),
                        name: name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        state,
        leaves_at: kelve_store::at(l.leaves_at).into(),
        why: l.why.clone(),
        lesson: l.same_as.clone().or_else(|| l.graph_id.clone()),
        example: lesson.as_ref().and_then(|x| x.example.clone()),
        instead: lesson
            .as_ref()
            .and_then(|x| x.seen_as.clone())
            .filter(|seen| lesson::usual_way(seen)),
        path: l.path.clone(),
        first: l.first.clone(),
        second: l.second.clone(),
    }
}

/// The agent as the reader names it: Claude Code, not `claude-code`.
async fn agent_name(app: &Shared) -> ApiResult<Option<String>> {
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::asks::latest_agent(&mut conn)
        .await?
        .map(|a| shown_name(&a.name)))
}

/// As `agentName` in `web/src/words.ts` names it, by the name it gave
/// itself over MCP or its hook's.
fn shown_name(name: &str) -> String {
    match name.trim().to_lowercase().as_str() {
        "claude-code" | "claude code" | "claude" => "Claude Code".into(),
        "codex" | "codex-cli" | "codex-mcp-client" => "Codex".into(),
        "cursor" | "cursor-vscode" => "Cursor".into(),
        "copilot" | "github-copilot" | "visual-studio-code" => "Copilot".into(),
        "gemini" | "gemini-cli" => "Gemini CLI".into(),
        // Named by its hook: `qwen`, `droid`.
        other => match kelve_core::hook::Agent::parse(other) {
            Some(a) => a.shown().into(),
            None => {
                let words = name.replace(['-', '_'], " ");
                let mut c = words.trim().chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                    .unwrap_or_else(|| "your agent".into())
            }
        },
    }
}

/// Pages kept by their IDs, those given first.
async fn pages_of(app: &Shared, ids: &[String]) -> ApiResult<Vec<LessonPage>> {
    let mut conn = app.db.acquire().await?;
    let mut out = Vec::new();
    for id in ids {
        if let Some(p) = store::page(&mut conn, id).await?
            && let Ok(p) = serde_json::from_str::<LessonPage>(&p)
        {
            out.push(p);
        }
    }
    Ok(out)
}

/// The lessons about some files of a folder, for the reader: used here
/// first, then the rest about their ideas; and what the agent learned in it.
pub async fn about_files(app: &Shared, root: &str, paths: &[String]) -> ApiResult<LessonsView> {
    let agent = agent_name(app).await?;
    let who = agent.clone().unwrap_or_else(|| "your agent".into());
    let given = {
        let mut conn = app.db.acquire().await?;
        store::given_at(&mut conn, root, &[]).await?
    };
    let mut ideas = ideas_in(app, root, paths).await?;
    for i in used_in(app, root, paths).await? {
        if !ideas.iter().any(|o| o.key == i.key) {
            ideas.push(i);
        }
    }
    let keys: Vec<String> = ideas.iter().map(|i| i.key.clone()).collect();
    let about = kept_about(app, &keys).await?;
    // Used here: given on these files, or as something failed in the work.
    let used_ids: Vec<String> = given
        .iter()
        .filter(|g| !g.withheld)
        .filter(|g| paths.contains(&g.path) || g.moment != "read")
        .filter(|g| g.moment != "read" || about.iter().any(|(p, _)| p.id == g.lesson))
        .map(|g| g.lesson.clone())
        .fold(Vec::new(), |mut acc, id| {
            if !acc.contains(&id) {
                acc.push(id);
            }
            acc
        });
    let used_pages = pages_of(app, &used_ids).await?;
    let rest: Vec<&LessonPage> = about
        .iter()
        .map(|(p, _)| p)
        .filter(|p| !used_ids.contains(&p.id))
        .collect();
    let all: Vec<&LessonPage> = used_pages.iter().chain(rest.iter().copied()).collect();
    let names = names(app, &all).await?;
    let learned = {
        let mut conn = app.db.acquire().await?;
        store::learned(&mut conn, 50).await?
    };
    Ok(LessonsView {
        used: used_pages
            .iter()
            .map(|p| view(p, &names, &given, &who))
            .collect(),
        here: rest
            .iter()
            .take(LISTED)
            .map(|p| view(p, &names, &given, &who))
            .collect(),
        more: rest.len().saturating_sub(LISTED) as u32,
        learned: learned
            .iter()
            .filter(|l| l.root == root)
            .map(learned_view)
            .collect(),
        agent,
    })
}

/// The lessons about one idea, for its entry: a few, and how many in all.
pub async fn about_idea(app: &Shared, key: &str, all: bool) -> ApiResult<(Vec<LessonView>, u32)> {
    let keys = vec![key.to_string()];
    if tokio::time::timeout(Duration::from_secs(2), refresh(app, &keys))
        .await
        .is_err()
    {
        tracing::debug!("lessons for an idea took too long; shown as kept");
    }
    let about = kept_about(app, &keys).await?;
    let agent = agent_name(app)
        .await?
        .unwrap_or_else(|| "your agent".into());
    let given = {
        let mut conn = app.db.acquire().await?;
        store::given_lately(&mut conn, 500).await?
    };
    let pages: Vec<&LessonPage> = about.iter().map(|(p, _)| p).collect();
    let names = names(app, &pages).await?;
    let n = if all { pages.len() } else { 3 };
    Ok((
        pages
            .iter()
            .take(n)
            .map(|p| view(p, &names, &given, &agent))
            .collect(),
        pages.len() as u32,
    ))
}

/// One lesson, with each time the agent was given it.
pub async fn one(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<Json<LessonView>> {
    let page = pages_of(&app, std::slice::from_ref(&id))
        .await?
        .into_iter()
        .next();
    let page = match page {
        Some(p) => p,
        None => match app.graph_reader() {
            Some(g) => g
                .lesson(&id)
                .await
                .ok()
                .flatten()
                .ok_or_else(|| ApiError::not_found("kelve doesn't have that lesson."))?,
            None => return Err(ApiError::not_found("kelve doesn't have that lesson.")),
        },
    };
    let agent = agent_name(&app)
        .await?
        .unwrap_or_else(|| "your agent".into());
    let given = {
        let mut conn = app.db.acquire().await?;
        store::given_lately(&mut conn, 500).await?
    };
    let names = names(&app, &[&page]).await?;
    let mut v = view(&page, &names, &given, &agent);
    // Another way: the lessons it contradicts, as kelve has them.
    let mut conn = app.db.acquire().await?;
    for id in &page.contradicts {
        if let Some(other) = store::page(&mut conn, id)
            .await?
            .and_then(|p| serde_json::from_str::<LessonPage>(&p).ok())
        {
            v.another.push(LessonBrief {
                id: other.id.clone(),
                act: other.lesson.act.clone(),
                holds_when: other.lesson.holds_when.clone(),
                basis: lesson::basis(other.record),
            });
        }
    }
    Ok(Json(v))
}

#[derive(Debug, Default, Deserialize)]
pub struct AllOf {
    #[serde(default)]
    pub idea: Option<String>,
}

/// Every lesson about an idea, for the side.
pub async fn of_idea(
    State(app): State<Shared>,
    _me: Me,
    Query(q): Query<AllOf>,
) -> ApiResult<Json<Vec<LessonView>>> {
    let Some(key) = q.idea else {
        return Ok(Json(Vec::new()));
    };
    Ok(Json(about_idea(&app, &key, true).await?.0))
}

/// Lessons, as settings show them.
pub async fn settings(State(app): State<Shared>, _me: Me) -> ApiResult<Json<LessonsSettings>> {
    Ok(Json(settings_view(&app).await?))
}

async fn settings_view(app: &Shared) -> ApiResult<LessonsSettings> {
    let app = app.clone();
    let agent = agent_name(&app)
        .await?
        .unwrap_or_else(|| "your agent".into());
    let (learned, given) = {
        let mut conn = app.db.acquire().await?;
        (
            store::learned(&mut conn, 100).await?,
            store::given_lately(&mut conn, 200).await?,
        )
    };
    let mut ids: Vec<String> = Vec::new();
    for g in given.iter().filter(|g| !g.withheld) {
        if !ids.contains(&g.lesson) {
            ids.push(g.lesson.clone());
        }
    }
    let pages = pages_of(&app, &ids).await?;
    let refs: Vec<&LessonPage> = pages.iter().collect();
    let names = names(&app, &refs).await?;
    Ok(LessonsSettings {
        learned: learned.iter().map(learned_view).collect(),
        given: pages
            .iter()
            .map(|p| view(p, &names, &given, &agent))
            .collect(),
        contribute: app.settings().contribute,
        hold: app.config.lesson_hold,
    })
}

/// Keeps what came of a lesson given, as `store::came_of` does, and counts
/// how it went, once a day a lesson given: never for one held back, which
/// measures going without it.
async fn came_of(
    app: &Shared,
    conn: &mut kelve_store::Conn,
    id: i64,
    outcome: Outcome,
    shown: Shown,
    said: &str,
    counted: bool,
) -> kelve_store::Result<bool> {
    let kept = store::came_of(
        conn,
        id,
        outcome.as_str(),
        shown.as_str(),
        shown.strength(),
        said,
    )
    .await?;
    let what = match outcome {
        Outcome::Held => Some("lessons.held"),
        Outcome::Didnt => Some("lessons.didnt"),
        _ => None,
    };
    if kept
        && counted
        && let Some(what) = what
    {
        crate::counts::count_once_on(app, conn, what, &format!("given:{id}")).await;
    }
    Ok(kept)
}

/// The session the person's own word that a lesson didn't hold is kept under.
const PERSON: &str = "person";

/// The person says a lesson didn't hold for them: counted as not holding,
/// with nothing from their work.
pub async fn didnt(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<Json<Value>> {
    let mut conn = app.db.acquire().await?;
    if store::page(&mut conn, &id).await?.is_none() {
        return Err(ApiError::not_found("kelve doesn't have that lesson."));
    }
    let given = store::give(
        &mut conn,
        &store::Give {
            lesson: &id,
            session: PERSON,
            moment: "asked",
            ..Default::default()
        },
    )
    .await?;
    came_of(
        &app,
        &mut conn,
        given,
        Outcome::Didnt,
        Shown::Said,
        "",
        true,
    )
    .await?;
    drop(conn);
    app.publish(ServerEvent::Lessons);
    Ok(Json(json!({})))
}

/// The person stops what the agent learned from leaving, or lets it go now.
/// What the agent learned, one of them, as its pane shows it.
pub async fn learned_one(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<LearnedView>> {
    let mut conn = app.db.acquire().await?;
    store::learned_one(&mut conn, id)
        .await?
        .map(|l| Json(learned_view(&l)))
        .ok_or_else(|| ApiError::not_found("kelve doesn't have that any more."))
}

pub async fn stop(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<LessonsSettings>> {
    {
        let mut conn = app.db.acquire().await?;
        if !store::stop(&mut conn, id).await? {
            return Err(ApiError::conflict(
                "That's gone already, so it can't be stopped.",
            ));
        }
    }
    app.publish(ServerEvent::Lessons);
    Ok(Json(settings_view(&app).await?))
}

pub async fn send_now(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<LessonsSettings>> {
    {
        let mut conn = app.db.acquire().await?;
        if !store::send_now(&mut conn, id).await? {
            return Err(ApiError::conflict("That isn't waiting any more."));
        }
        kelve_store::jobs::add(
            &mut conn,
            SEND,
            Some(&format!("{SEND}:{id}:now")),
            &json!({ "learned": id }),
            Utc::now(),
        )
        .await?;
    }
    app.wake.notify_one();
    app.publish(ServerEvent::Lessons);
    Ok(Json(settings_view(&app).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_is_named_as_the_reader_names_it() {
        assert_eq!(shown_name("claude-code"), "Claude Code");
        assert_eq!(shown_name("claude"), "Claude Code");
        assert_eq!(shown_name("codex-mcp-client"), "Codex");
        assert_eq!(shown_name("cursor-vscode"), "Cursor");
        assert_eq!(shown_name("qwen"), "Qwen Code");
        assert_eq!(shown_name("droid"), "Factory Droid");
        assert_eq!(shown_name("zed_agent"), "Zed agent");
        assert_eq!(shown_name(""), "your agent");
    }

    #[test]
    fn a_better_way_is_seen_as_the_usual_way_as_it_was_written() {
        let material = |failed: &str| {
            json!({
                "failed": kelve_core::fence::fence("failed", failed),
                "replaced": kelve_core::fence::fence("replaced", "In a.py, the agent replaced what it wrote earlier.\nThe first way:\n```\n            time.sleep(delay)\n            delay *= 2\n```\nThe second way:\n```\n            time.sleep(random.uniform(0, delay))\n            delay *= 2\n```"),
                "known": [],
                "offered": [{ "key": "kelve:iwp0tsw4s1a49", "name": "Exponential backoff" }],
                "work": [],
            })
        };
        let answer = |seen: Option<&str>| {
            json!({
                "lessons": [{
                    "about": ["Exponential backoff"],
                    "says": "Clients that fail together and wait the same times retry together. A random wait up to the cap spreads them out.",
                    "do": "Draw each wait at random, from zero up to the cap.",
                    "holdsWhen": "Any client retrying a shared service.",
                    "seenAs": seen,
                }],
                "same": [],
            })
        };
        let nothing = material("Nothing failed.");
        assert!(shape(&nothing, &answer(Some("time.sleep(delay)"))).is_ok());
        let still = shape(&nothing, &answer(Some("time.sleep(")));
        assert!(still.is_err(), "the second way still has it");
        let prose = shape(
            &nothing,
            &answer(Some("Backoff with fixed waits, so clients retry together.")),
        );
        let why = prose.unwrap_err();
        assert!(why.contains("`seenAs` is code"), "{why}");
        assert!(why.contains("These went: `time.sleep(delay)`."), "{why}");
        let around = answer(Some("Backoff written as time.sleep(delay) each time."));
        assert!(
            shape(&nothing, &around).is_err(),
            "the line, never words around it"
        );
        assert!(shape(&nothing, &answer(None)).is_err());
        // Every problem at once: a Do in two sentences, and a description.
        let mut two = answer(Some("Backoff without jitter."));
        two["lessons"][0]["do"] = json!("Add jitter. Always.");
        let why = shape(&nothing, &two).unwrap_err();
        assert!(
            why.contains("`do`") && why.contains("`seenAs` is code"),
            "{why}"
        );

        // With what failed too, a lesson about that is seen as the problem.
        let both = material("- `pytest` failed with: AssertionError: waits differ");
        assert!(shape(&both, &answer(None)).is_ok());
        assert!(shape(&both, &answer(Some("AssertionError: waits differ"))).is_ok());
        assert!(shape(&both, &answer(Some("clients retry together"))).is_err());
    }

    #[test]
    fn a_file_written_over_replaces_what_the_agent_last_wrote_there_and_is_gone() {
        let first = "def charge(bank):\n    for attempt in range(5):\n        try:\n            return bank.send()\n        except Busy:\n            time.sleep(2 ** attempt)\n";
        let second = "def charge(bank):\n    for attempt in range(5):\n        try:\n            return bank.send()\n        except Busy:\n            delay = 2 ** attempt\n            time.sleep(random.uniform(0, delay))\n";
        let gone = gone_from(first, second);
        assert_eq!(gone.trim(), "time.sleep(2 ** attempt)");
        assert!(!replaces_own(&gone, first), "a line is too few");

        let third = "async def charge(bank):\n    await bank.send()\n";
        let gone = gone_from(second, third);
        assert!(replaces_own(&gone, second));
        assert_eq!(gone_from(third, second).lines().count(), 2);
    }
}
