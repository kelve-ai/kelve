//! The reader's routes: opening something, its files and changes with their
//! marks, an idea's entry, notes, and search. Only the person reads any of it;
//! kelve's command line can ask to open something, and learn whether a browser
//! opened, nothing more.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::{Path as UrlPath, Query, State};
use kelve_core::views::{
    AgentList, ChangeView, ChangedFile, DiffLine, DiffView, EntryState, FileItem, FileView, FlagIn,
    FolderView, Found, FoundIdea, FoundLine, IdeaPage, Marked, Marking, Note, NoteIn, NoteRef,
    OpenIn, OpenKind, Opened, Origins, PickIn, PickWhat, Picked, Place, Reading, ServerEvent,
    Stage,
};
use kelve_engine::git::{LineKind, Status};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::{Me, Who};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{entries, mark};
use crate::reader::{self, Open};
use crate::state::Shared;

/// How many things opened lately are listed.
const LATELY: i64 = 12;

/// What opening something did. To kelve's command line, only whether a
/// browser opened; to the app, what's open.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedOut {
    #[serde(skip_serializing_if = "Option::is_none")]
    opened: Option<Opened>,
    #[serde(skip_serializing_if = "Option::is_none")]
    browser: Option<bool>,
}

/// Opens a folder, a file, or a change in the repo a path is in.
pub async fn open(
    State(app): State<Shared>,
    who: Who,
    Json(open): Json<OpenIn>,
) -> ApiResult<Json<OpenedOut>> {
    let id = open_at(&app, open.path.trim(), open.change.as_deref()).await?;
    match who {
        // The command line learns only whether a browser opened, with a
        // one-time link straight to it: the person's, never the asker's.
        Who::Agent => Ok(Json(OpenedOut {
            opened: None,
            browser: Some(show(&app, id).await?),
        })),
        Who::Me => Ok(Json(OpenedOut {
            opened: Some(reader::opened(&app, id).await?.view()),
            browser: None,
        })),
    }
}

/// Opens again something opened before, from your agent's list or by going
/// to it in the reader: it's what's open, and its reading carries on where
/// it was. Going to what's open already changes nothing.
pub async fn open_again(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Opened>> {
    let open = reader::opened(&app, id).await?;
    {
        let mut conn = app.db.acquire().await?;
        if !kelve_store::reader::again(&mut conn, id).await? {
            return Ok(Json(open.view()));
        }
        kelve_store::asks::let_go_of_first(&mut conn).await?;
        // What an agent held of what's open no longer is paused, and isn't being done.
        kelve_store::asks::let_go_of_paused(&mut conn).await?;
    }
    app.publish(ServerEvent::Opened);
    app.publish(ServerEvent::Asks);
    app.wake.notify_one();
    Ok(Json(open.view()))
}

/// Shows the person what was opened for them, in a new browser tab, which
/// brings their browser to the front, so they see it at once; a tab they
/// have open on something else, perhaps another agent's work, stays where
/// it is (Nathan, 29 October). It's signed in by a one-time link that goes
/// to the person, never to whoever asked. Whether they see it.
pub async fn show(app: &Shared, id: i64) -> ApiResult<bool> {
    let link = crate::auth::link_to(app, Some(id)).await?;
    Ok(app.hand_off(&link))
}

/// Opens something at a path: a folder, a file, or, with `change`, a
/// change in the repository it's in. Its id.
pub async fn open_at(app: &Shared, path: &str, change: Option<&str>) -> ApiResult<i64> {
    let path = reader::unshown(path);
    if !path.is_absolute() {
        return Err(ApiError::bad_request(
            "Give kelve the whole path, from the root.",
        ));
    }
    let path = std::fs::canonicalize(&path).map_err(|_| {
        ApiError::not_found(format!("There's nothing at {}.", reader::shown(&path)))
    })?;
    let name = |p: &Path| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| reader::shown(p))
    };
    let (kind, root, rel, change, title) = match change.map(str::trim) {
        Some(words) if !words.is_empty() => {
            let which = reader::which(words).ok_or_else(|| {
                ApiError::bad_request(
                    "Say which change: uncommitted, against a branch, or a commit.",
                )
            })?;
            let start = if path.is_dir() {
                path.clone()
            } else {
                path.parent().map(Path::to_path_buf).unwrap_or_default()
            };
            let root = kelve_engine::git::root_of(&start)
                .ok_or_else(|| ApiError::bad_request("That folder isn't in a git repository."))?;
            let title = match which {
                kelve_engine::git::Which::Uncommitted => "Uncommitted changes".to_string(),
                kelve_engine::git::Which::Against(ref base) => {
                    format!("{} against {base}", name(&root))
                }
                kelve_engine::git::Which::Commit(ref rev) => format!("Commit {rev}"),
                kelve_engine::git::Which::Since(ref base) => {
                    format!("{} since {base}", name(&root))
                }
            };
            ("change", root, String::new(), words.to_string(), title)
        }
        _ if path.is_dir() => {
            let title = name(&path);
            ("folder", path, String::new(), String::new(), title)
        }
        _ => {
            // A file opens in its repository, or its own folder.
            let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let root = kelve_engine::git::root_of(&parent).unwrap_or(parent);
            let rel: Vec<String> = path
                .strip_prefix(&root)
                .map_err(|_| ApiError::bad_request("That file isn't in a folder kelve can open."))?
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            let title = name(&path);
            ("file", root, rel.join("/"), String::new(), title)
        }
    };
    let root_text = root.display().to_string();
    let id = {
        let mut conn = app.db.acquire().await?;
        let id =
            kelve_store::reader::open(&mut conn, kind, &root_text, &rel, &change, &title).await?;
        // What was opened last comes first: Do next said before it no longer
        // jumps ahead of it, though Do next said after it still does.
        kelve_store::asks::let_go_of_first(&mut conn).await?;
        // What an agent held of anything opened before it is paused, and
        // isn't being done.
        kelve_store::asks::let_go_of_paused(&mut conn).await?;
        // Opened again: reading still waiting from before is let go, and
        // asked for afresh, for what its files are now.
        let gone = kelve_store::asks::let_go_of_reading(&mut conn, id).await?;
        if gone > 0 {
            tracing::info!(gone, "let go of reading from before, opened again");
        }
        id
    };
    // A change's folders' pages, and its own, read plainly once its files
    // are, as part of reading it. A folder's are asked for as it's read,
    // once there's a way in when there's a lot.
    if kind == "change" {
        crate::plainly::ask_pages(app, &reader::opened(app, id).await?).await?;
    }
    // Its reading, as it's asked for again, in the order of its way in.
    if let Err(e) = crate::way::in_order(app, &reader::opened(app, id).await?).await {
        tracing::warn!(error = ?e, "couldn't put its reading in its way in's order");
    }
    app.publish(ServerEvent::Opened);
    app.publish(ServerEvent::Asks);
    tracing::info!(kind, "opened something");
    crate::counts::count(app, &format!("opened.{kind}")).await;
    // The top's page reads every file: worked out now, it opens at once.
    tokio::task::spawn_blocking(move || crate::way::parts_of(&root, ""));
    Ok(id)
}

/// Shows the system's picker for a folder or a file, and waits for the
/// person: the path they chose, for the page to open.
pub async fn pick(
    State(_app): State<Shared>,
    _me: Me,
    Json(asked): Json<PickIn>,
) -> ApiResult<Json<Picked>> {
    let what = match asked.what {
        PickWhat::Folder => crate::pick::What::Folder,
        PickWhat::File => crate::pick::What::File,
    };
    let path = crate::pick::pick(what)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(Picked {
        path: path.map(|p| p.display().to_string()),
    }))
}

/// Shows the person's notes folder in the system's file manager.
pub async fn show_notes(State(app): State<Shared>, _me: Me) -> ApiResult<Json<serde_json::Value>> {
    let dir = reader::own_notes(&app);
    std::fs::create_dir_all(&dir).map_err(ApiError::internal)?;
    crate::pick::reveal(&dir).await.map_err(|_| {
        ApiError::bad_request(format!(
            "kelve couldn't show the folder. It's {}.",
            reader::shown(&dir)
        ))
    })?;
    Ok(Json(json!({})))
}

/// What was opened lately, latest first.
pub async fn lately(State(app): State<Shared>, _me: Me) -> ApiResult<Json<Vec<Opened>>> {
    let rows = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::lately(&mut conn, LATELY).await?
    };
    let mut out = Vec::new();
    for row in rows {
        let open = Open {
            root: PathBuf::from(&row.root),
            row,
        };
        let mut view = open.view();
        // What's ready in it, for the start page to say.
        let ready = crate::ready::list(&app, &open).await?;
        view.ready = (!ready.is_empty()).then_some(ready);
        out.push(view);
    }
    Ok(Json(out))
}

/// What's ready for the person in what's opened: what they asked for by
/// name, written, and not seen yet.
pub async fn ready(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Vec<kelve_core::views::ReadyItem>>> {
    let open = reader::opened(&app, id).await?;
    Ok(Json(crate::ready::list(&app, &open).await?))
}

/// Entries the person asked for in what's opened lately and that are
/// written now, seen or not: newly written, for search.
pub async fn written(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Vec<kelve_core::views::ReadyItem>>> {
    let open = reader::opened(&app, id).await?;
    Ok(Json(crate::ready::written(&app, &open).await?))
}

#[derive(Debug, Deserialize)]
pub struct AskedIn {
    key: String,
}

/// The person opened an idea with no entry yet in what's opened: it's
/// theirs to be told of when it's written.
pub async fn asked(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Json(a): Json<AskedIn>,
) -> ApiResult<Json<serde_json::Value>> {
    let open = reader::opened(&app, id).await?;
    known(&app, &a.key).await?;
    crate::ready::asked_entry(&app, open.row.id, &a.key).await?;
    Ok(Json(json!({})))
}

pub async fn one(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Opened>> {
    let open = reader::opened(&app, id).await?;
    let mut view = open.view();
    view.way = crate::way::shown(&app, &open).await;
    Ok(Json(view))
}

#[derive(Debug, Deserialize)]
pub struct PartAt {
    #[serde(default)]
    path: String,
}

/// A folder of what's open, as its page shows it: what it's for, its
/// parts, and which use which.
pub async fn part(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(at): Query<PartAt>,
) -> ApiResult<Json<kelve_core::views::PartView>> {
    let open = reader::opened(&app, id).await?;
    Ok(Json(crate::way::part(&app, &open, &at.path).await?))
}

#[derive(Debug, Deserialize)]
pub struct StepIn {
    path: String,
}

/// The person read a step of the way in: In order fills it in, wherever
/// what's open is shown.
pub async fn read_step(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Json(step): Json<StepIn>,
) -> ApiResult<Json<serde_json::Value>> {
    let open = reader::opened(&app, id).await?;
    // A file on the way, or its overview, by the path "".
    let on = crate::way::view(&app, &open)
        .await
        .is_some_and(|w| step.path.is_empty() || crate::way::path_of(&w).contains(&step.path));
    if !on {
        return Err(ApiError::not_found(
            "That file isn't on the way through what's open.",
        ));
    }
    let mut conn = app.db.acquire().await?;
    if kelve_store::reader::step_read(&mut conn, id, &step.path).await? {
        app.publish(ServerEvent::Opened);
    }
    Ok(Json(serde_json::json!({})))
}

#[derive(Debug, Deserialize)]
pub struct WayPartIn {
    name: String,
}

/// The person asks for a part of the path through what's open to be read.
pub async fn read_part(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Json(part): Json<WayPartIn>,
) -> ApiResult<Json<serde_json::Value>> {
    let open = reader::opened(&app, id).await?;
    crate::way::read_part(&app, &open, &part.name).await?;
    Ok(Json(serde_json::json!({})))
}

/// A code or text file of what's open, read plainly: as kept, or asked of the
/// person's agent.
pub async fn plainly(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(at): Query<At>,
) -> ApiResult<Json<kelve_core::views::PlainlyView>> {
    let open = reader::opened(&app, id).await?;
    let view = crate::plainly::view(&app, &open, &at.path, !at.ahead).await?;
    // Read plainly by the person, not looked at ahead: once a day a file.
    if !at.ahead && view.what.is_some() {
        crate::counts::count_once(&app, "files.plainly", &format!("{id}:{}", at.path)).await;
    }
    Ok(Json(view))
}

/// A folder of what's open, read plainly: "" for its top.
pub async fn plainly_folder(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(at): Query<PartAt>,
) -> ApiResult<Json<kelve_core::views::FolderPlainly>> {
    let open = reader::opened(&app, id).await?;
    Ok(Json(
        crate::plainly::folder_view(&app, &open, &at.path).await?,
    ))
}

/// The change that's open, read plainly.
pub async fn plainly_change(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<kelve_core::views::ChangePlainly>> {
    let open = reader::opened(&app, id).await?;
    Ok(Json(crate::plainly::change_view(&app, &open).await?))
}

#[derive(Debug, Default, Deserialize)]
pub struct OnFile {
    /// One file of a change, when the side is on it.
    #[serde(default)]
    path: Option<String>,
}

/// The lessons about what's open: those the agent used in it first, then the
/// rest about the ideas marked in it.
pub async fn lessons(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(on): Query<OnFile>,
) -> ApiResult<Json<kelve_core::views::LessonsView>> {
    let open = reader::opened(&app, id).await?;
    let paths: Vec<String> = match (open.kind(), on.path) {
        (_, Some(p)) => vec![p],
        (OpenKind::File, None) => vec![open.row.path.clone()],
        (OpenKind::Change, None) => reader::change(&open)?
            .files
            .iter()
            .map(|f| f.path.clone())
            .collect(),
        (OpenKind::Folder, None) => Vec::new(),
    };
    Ok(Json(
        crate::lessons::about_files(&app, &open.row.root, &paths).await?,
    ))
}

#[derive(Debug, Default, Deserialize)]
pub struct Listing {
    /// Only its files, as a list to choose from: nothing is read for it.
    #[serde(default)]
    list: bool,
}

/// A folder's files, and what it's built on: for a folder or a file, the
/// folder it's in. Asked for as a list, beside a change, nothing is read.
pub async fn folder(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(listing): Query<Listing>,
) -> ApiResult<Json<FolderView>> {
    let open = reader::opened(&app, id).await?;
    let walked =
        kelve_engine::walk::walk(&open.root).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let read = reader::files_read(&app, &open).await?;
    let written = crate::plainly::read_so_far(&app).await?;
    let only = open.only();
    // Shown, not listed: its way in is found first, before anything's read.
    if !listing.list {
        crate::way::ask_way_first(&app, &open).await?;
    }
    // What's read now: the way in, and the parts the person asked for.
    let now = crate::way::to_read_now(&app, &open).await;
    let (mut done, mut of) = (0, 0);
    let files = walked
        .files
        .into_iter()
        .filter(|f| !reader::is_kelves(&f.rel))
        .filter(|f| only.as_ref().is_none_or(|o| o.contains(&f.rel)))
        .map(|f| {
            let r = read.get(&f.rel);
            if f.skip.is_none() && now.as_ref().is_none_or(|n| n.contains(&f.rel)) {
                // Read once its ideas are checked and it's read plainly.
                of += 1;
                done += u32::from(r.is_some_and(|r| reader::read_through(r, &f.rel, &written)));
            }
            FileItem {
                ideas: r.filter(|r| r.skip.is_none()).map(|r| r.ideas.clone()),
                skip: f
                    .skip
                    .map(|s| s.says().to_string())
                    .or_else(|| r.and_then(|r| r.skip.clone())),
                path: f.rel,
            }
        })
        .collect();
    if listing.list {
        let (done, of, pages) = reader::with_page(&app, &open, None, done, of, false).await?;
        let marking = if done >= of {
            Marking::Done
        } else {
            Marking::Marking
        };
        return Ok(Json(FolderView {
            built_on: Vec::new(),
            opened: open.view(),
            files,
            more: walked.more,
            reading: Reading {
                read: done,
                of,
                checking: reader::checking(&app, open.row.id, of.saturating_sub(done)).await?,
                marking,
                pages: pages.of,
                pages_left: pages.left,
            },
        }));
    }
    // Opening a folder has the person's agent read what isn't read yet.
    let reading = reader::reading(&app, &open, done, of).await?;
    Ok(Json(FolderView {
        built_on: reader::folder_built_on(&app, &open, reading.marking == Marking::Marking).await?,
        opened: open.view(),
        files,
        more: walked.more,
        reading,
    }))
}

#[derive(Debug, Deserialize)]
pub struct At {
    path: String,
    /// Only looking, ahead of the person opening it: shown as it is, with
    /// nothing asked of the agent for it.
    #[serde(default)]
    ahead: bool,
}

/// A file in what's open, with its marks so far. Marking it is asked for
/// here, and its marks come in as they're made.
pub async fn file(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(at): Query<At>,
) -> ApiResult<Json<FileView>> {
    let open = reader::opened(&app, id).await?;
    let path = at.path.trim();
    let read = match kelve_engine::read::read(&open.root, path) {
        Ok(t) => t,
        Err(kelve_engine::read::ReadError::Skipped(skip)) => {
            return Ok(Json(FileView {
                path: path.to_string(),
                lang: kelve_core::code::language(path).map(str::to_string),
                text: None,
                skip: Some(skip.says().to_string()),
                marks: Vec::new(),
                marking: kelve_core::views::Marking::Done,
            }));
        }
        Err(e) => return Err(ApiError::not_found(e.to_string())),
    };
    let (marks, whole) = mark::kept(&app, path, &read.text, None).await?;
    // Marked before, perhaps in another folder: kept as read in this one.
    if whole && !marks.is_empty() && !at.ahead {
        reader::keep_ideas(&app, &open, path, &marks, Some(&read.text))
            .await
            .map_err(ApiError::internal)?;
        // The ideas in the file being read are what's opened next: their
        // entries are written ahead.
        let keys: Vec<String> = marks.iter().map(reader::key_of).collect();
        reader::take_ahead(&app, keys.iter().map(String::as_str))
            .await
            .map_err(ApiError::internal)?;
    }
    if !whole && !at.ahead {
        reader::ask(
            &app,
            reader::READ_FILE,
            &format!("{}:{id}:{path}", reader::READ_FILE),
            json!({ "opened": id, "path": path, "priority": crate::agent::Priority::Now as i64 }),
        )
        .await?;
        crate::agent::hurry(&app, None, Some(id)).await?;
    }
    Ok(Json(FileView {
        path: path.to_string(),
        lang: read.lang.map(str::to_string),
        text: Some(read.text),
        skip: None,
        marks: reader::marked(mark::shown(&app, marks)),
        marking: reader::marking(whole),
    }))
}

/// An image in what's open, as it is, for the reader to draw. An SVG can
/// hold script, so whatever asks for it, it's served sandboxed: nothing in
/// it runs, here or opened on its own.
pub async fn image(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(at): Query<At>,
) -> ApiResult<axum::response::Response> {
    use axum::http::header;
    use axum::response::IntoResponse;
    let open = reader::opened(&app, id).await?;
    let (bytes, kind) = kelve_engine::read::image(&open.root, at.path.trim())
        .map_err(|e| ApiError::not_found(e.to_string()))?;
    Ok((
        [
            (header::CONTENT_TYPE, kind),
            (header::CACHE_CONTROL, "no-cache"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; img-src data:; sandbox",
            ),
        ],
        bytes,
    )
        .into_response())
}

/// A change: its files, and what it's built on, new ideas first. Opening it
/// asks for its first files to be marked.
pub async fn change(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<ChangeView>> {
    let open = reader::opened(&app, id).await?;
    if open.kind() != OpenKind::Change {
        return Err(ApiError::bad_request("That isn't a change."));
    }
    let change = reader::change(&open)?;
    let files = reader::mark_change(&app, &open, &change).await?;
    // Its folders' pages and its own read plainly once its files are: part of
    // reading it.
    let (read, of, pages) =
        reader::with_page(&app, &open, Some(&change), files.read, files.of, true).await?;
    let reading = Reading {
        read,
        of,
        marking: reader::marking(read >= of),
        pages: pages.of,
        pages_left: pages.left,
        ..files
    };
    let (built_on, mut ideas) = reader::change_built_on(&app, &open, &change).await?;
    Ok(Json(ChangeView {
        opened: open.view(),
        title: change.title.clone(),
        files: change
            .files
            .iter()
            .map(|f| ChangedFile {
                path: f.path.clone(),
                status: match f.status {
                    Status::Added => "added",
                    Status::Modified => "modified",
                    Status::Deleted => "deleted",
                }
                .into(),
                added: f.added,
                removed: f.removed,
                skip: f.skip.map(|s| s.says().to_string()),
                test: kelve_engine::passages::test_file(&f.path),
                ideas: ideas.remove(&f.path),
                moved_from: f.moved_from.clone(),
            })
            .collect(),
        more: change.more,
        built_on,
        reading,
    }))
}

/// Your agent's list: what it's doing, what's next, what's ready.
pub async fn list(State(app): State<Shared>, _me: Me) -> ApiResult<Json<AgentList>> {
    Ok(Json(crate::list::view(&app).await?))
}

#[derive(Debug, Deserialize)]
pub struct First {
    asks: Vec<i64>,
}

/// Moves something on your agent's list to the front.
pub async fn list_first(
    State(app): State<Shared>,
    _me: Me,
    Json(first): Json<First>,
) -> ApiResult<Json<AgentList>> {
    crate::list::first(&app, &first.asks).await?;
    Ok(Json(crate::list::view(&app).await?))
}

#[derive(Debug, Deserialize)]
pub struct Off {
    /// The jobs a row of the list stands for.
    #[serde(default)]
    asks: Vec<i64>,
    /// Something paused, by what was opened: the rest of its reading.
    #[serde(default)]
    paused: Option<i64>,
}

/// Takes something off your agent's list: it isn't done.
pub async fn list_off(
    State(app): State<Shared>,
    _me: Me,
    Json(off): Json<Off>,
) -> ApiResult<Json<AgentList>> {
    crate::list::take_off(&app, &off.asks, off.paused).await?;
    Ok(Json(crate::list::view(&app).await?))
}

/// One file of a change: its hunks, and the marks on its added lines.
pub async fn diff(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(at): Query<At>,
) -> ApiResult<Json<DiffView>> {
    let open = reader::opened(&app, id).await?;
    let change = reader::change(&open)?;
    let fc = change
        .files
        .iter()
        .find(|f| f.path == at.path)
        .cloned()
        .ok_or_else(|| ApiError::not_found("That file isn't in this change."))?;
    // The file whole, its marks on every line, each saying whether its
    // idea is new in the change.
    let (marks, whole) = match &fc.after {
        Some(after) => mark::kept(&app, &fc.path, after, None).await?,
        None => (Vec::new(), true),
    };
    if !whole && !at.ahead {
        reader::ask_diff(&app, id, &fc.path).await?;
    }
    let new: BTreeSet<String> = {
        let (built_on, _) = reader::change_built_on(&app, &open, &change).await?;
        built_on
            .into_iter()
            .filter(|b| b.new)
            .map(|b| b.key)
            .collect()
    };
    let lines = match (&fc.before, &fc.after) {
        (None, None) => Vec::new(),
        (before, after) => {
            kelve_engine::git::whole(
                before.as_deref().unwrap_or(""),
                after.as_deref().unwrap_or(""),
            )
            .lines
        }
    };
    let kind = |k: LineKind| match k {
        LineKind::Added => "added",
        LineKind::Removed => "removed",
        LineKind::Context => "context",
    };
    Ok(Json(DiffView {
        lang: kelve_core::code::language(&fc.path).map(str::to_string),
        lines: lines
            .iter()
            .map(|l| DiffLine {
                kind: kind(l.kind).into(),
                old: l.old,
                new: l.new,
                text: l.text.clone(),
            })
            .collect(),
        skip: fc.skip.map(|s| s.says().to_string()),
        path: fc.path,
        marks: reader::marked(mark::shown(&app, marks))
            .into_iter()
            .map(|m| Marked {
                new: new.contains(&m.key),
                ..m
            })
            .collect(),
        marking: reader::marking(whole),
    }))
}

/// Every idea whose entry the person has opened, by key: the squares that
/// are filled.
pub async fn read(State(app): State<Shared>, _me: Me) -> ApiResult<Json<Vec<String>>> {
    let mut conn = app.db.acquire().await?;
    Ok(Json(kelve_store::reader::ideas_read(&mut conn).await?))
}

/// An idea kelve knows, or one of the graph's it hasn't met yet, kept now.
async fn known(app: &Shared, key: &str) -> ApiResult<kelve_store::entries::Idea> {
    let mut conn = app.db.acquire().await?;
    if let Some(idea) = kelve_store::entries::idea(&mut conn, key).await? {
        return Ok(idea);
    }
    drop(conn);
    if kelve_core::ideas::is_id(key)
        && let Some(graph) = app.graph_reader()
        && let Ok(found) = graph.ideas(&[key.to_string()]).await
        && let Some(i) = found.ideas.into_iter().next()
    {
        let kept = kelve_store::entries::Idea {
            key: key.to_string(),
            name: i.name,
            definition: i.definition,
            kind: i.kind.as_str().into(),
            became: None,
        };
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::keep_idea(&mut conn, &kept).await?;
        return Ok(kept);
    }
    Err(ApiError::not_found("kelve doesn't know that idea."))
}

#[derive(Debug, Default, Deserialize)]
pub struct Reached {
    /// Reached by following a link on from something opened.
    #[serde(default)]
    followed: bool,
}

/// An idea and its entry: taken from the shared graph, or written now,
/// with how far it's got.
pub async fn idea(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(key): UrlPath<String>,
    Query(reached): Query<Reached>,
) -> ApiResult<Json<IdeaPage>> {
    let idea = known(&app, &key).await?;
    // Found to be two: the two are offered instead.
    let split = crate::pipeline::splits::of(&app, &key)
        .await
        .map_err(ApiError::internal)?;
    if !split.is_empty() {
        return Ok(Json(IdeaPage {
            key,
            name: idea.name,
            definition: idea.definition,
            question: String::new(),
            entry: None,
            state: EntryState::Waiting,
            step: String::new(),
            stage: None,
            problems: Vec::new(),
            rewriting: false,
            can_flag: false,
            split,
            pages: Vec::new(),
            agent: None,
            checking: None,
            kept_here: false,
            lessons: Vec::new(),
            lessons_all: 0,
            from_graph: false,
            reads: 0,
        }));
    }
    let row = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::get(&mut conn, &key).await?
    };
    // Shown with its words: seen, so it leaves Ready for you.
    if row
        .as_ref()
        .is_some_and(|r| matches!(r.state.as_str(), "ready" | "checking"))
    {
        crate::ready::seen_entry(&app, &key).await?;
    }
    // Asked for now: what the agent does for it goes first.
    crate::agent::hurry(&app, Some(&key), None).await?;
    // No entry yet, or one that waited: asked for now. A dropped one waits
    // to be asked for again, so it isn't written over and over. One being
    // replaced is shown until its replacement is ready.
    if row
        .as_ref()
        .is_none_or(|r| r.state == "waiting" && r.replacing.is_none())
    {
        entries::want(&app, &idea)
            .await
            .map_err(ApiError::internal)?;
    }
    // A ready entry the graph has: asked about again now and then, for a
    // newer revision, or what its checks found wrong.
    if app.graph_reader().is_some()
        && let Some(r) = &row
        && r.state == "ready"
        && r.revision.is_some()
        && r.checked_at
            .is_none_or(|at| kelve_store::now() - at > entries::FRESH.as_millis() as i64)
    {
        crate::pipeline::add(
            &app,
            entries::REFRESH,
            Some(&format!("{}:{key}", entries::REFRESH)),
            json!({ "idea": key }),
            chrono::Utc::now(),
        )
        .await
        .map_err(ApiError::internal)?;
    }
    let row = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::get(&mut conn, &key).await?
    };
    let shown = |r: &kelve_store::entries::Entry| {
        r.replacing
            .as_deref()
            .or(r.entry.as_deref())
            .and_then(|e| serde_json::from_str(e).ok())
    };
    let (state, step, entry) = match &row {
        Some(r) if r.state != "ready" && r.replacing.is_some() => {
            (EntryState::Ready, String::new(), shown(r))
        }
        Some(r) if r.state == "ready" => (
            EntryState::Ready,
            r.step.clone(),
            r.entry
                .as_deref()
                .and_then(|e| serde_json::from_str(e).ok()),
        ),
        Some(r) if r.state == "waiting" => (EntryState::Waiting, r.step.clone(), None),
        Some(r) if r.state == "dropped" => (EntryState::Dropped, r.step.clone(), None),
        // Written by the agent and with the graph: read at once, while
        // it's checked.
        Some(r) if r.state == "checking" => (
            EntryState::Writing,
            r.step.clone(),
            r.entry
                .as_deref()
                .and_then(|e| serde_json::from_str(e).ok()),
        ),
        // Turned away by the graph and kept for the agent to fix: the entry
        // it sent is read still, its failed claims marked where they're
        // made, until the fix comes.
        Some(r) if r.checking.is_some() => (
            EntryState::Writing,
            r.step.clone(),
            r.entry
                .as_deref()
                .and_then(|e| serde_json::from_str(e).ok()),
        ),
        Some(r) => (EntryState::Writing, r.step.clone(), None),
        None => (EntryState::Writing, "Kept for your agent".into(), None),
    };
    // Read, for its square: from the first time its page is opened.
    {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::read_idea(&mut conn, &key).await?;
    }
    // Counted once a day an idea, however often the page asks again.
    let ready = state == EntryState::Ready;
    let from_graph = ready && row.as_ref().is_some_and(|r| r.from_graph);
    if ready {
        crate::counts::count_once(&app, "entries.read", &key).await;
    }
    if from_graph {
        crate::counts::count_once(&app, "entries.graph", &key).await;
    }
    if reached.followed && crate::counts::count_once(&app, "links.followed", &key).await && ready {
        crate::counts::count_once(&app, "links.written", &key).await;
    }
    let can_flag =
        ready && app.graph_reader().is_some() && row.as_ref().is_some_and(|r| r.revision.is_some());
    let kept_here = ready
        && row
            .as_ref()
            .is_some_and(|r| !r.from_graph && !r.sent && r.revision.is_none() && r.entry.is_some());
    // Which step it's on, while it's written: kept for the agent, with the
    // agent, with the graph, or being fixed.
    let revised = |r: &kelve_store::entries::Entry| {
        r.plan
            .as_deref()
            .and_then(|p| serde_json::from_str::<entries::Plan>(p).ok())
            .is_some_and(|p| p.revisions > 0)
    };
    let (taken, off, agent) = {
        let mut conn = app.db.acquire().await?;
        let holding = kelve_store::asks::open(&mut conn)
            .await?
            .into_iter()
            .find(|a| a.idea.as_deref() == Some(key.as_str()) && a.state == "taken");
        let off = kelve_store::asks::taken_off(&mut conn, &key).await?;
        // The agent that wrote it, as its job kept it; else the one writing
        // it now; else the one seen last.
        let wrote = row
            .as_ref()
            .and_then(|r| r.plan.as_deref())
            .and_then(|p| serde_json::from_str::<entries::Plan>(p).ok())
            .and_then(|p| p.agent);
        let agent = match wrote.or_else(|| holding.as_ref().and_then(|a| a.agent.clone())) {
            Some(name) => Some(name),
            None => kelve_store::asks::latest_agent(&mut conn)
                .await?
                .map(|a| a.name),
        };
        (holding.is_some(), off, agent)
    };
    let stage = (state == EntryState::Writing).then(|| match row.as_ref() {
        _ if off => Stage::Off,
        Some(r) if r.state == "checking" => Stage::Checking,
        Some(r) if r.state == "writing" && revised(r) => Stage::Fixing,
        _ if taken => Stage::Writing,
        _ => Stage::Kept,
    });
    // How the graph's check stands, claim by claim, with what each says.
    let checking = row.as_ref().and_then(|r| {
        let c: kelve_graph_api::Checking = serde_json::from_str(r.checking.as_deref()?).ok()?;
        let claims: Vec<kelve_core::entry::draft::Claim> =
            serde_json::from_str(r.claims.as_deref().unwrap_or("[]")).unwrap_or_default();
        let word = |s: kelve_graph_api::ClaimState| match s {
            kelve_graph_api::ClaimState::Waiting => "waiting",
            kelve_graph_api::ClaimState::Held => "held",
            kelve_graph_api::ClaimState::Failed => "failed",
        };
        Some(kelve_core::views::Checking {
            status: match c.status {
                kelve_graph_api::CheckStatus::Pending => "pending",
                kelve_graph_api::CheckStatus::Accepted => "accepted",
                kelve_graph_api::CheckStatus::Rejected => "rejected",
            }
            .into(),
            claims: c
                .claims
                .iter()
                .map(|cc| {
                    let claim = (cc.n as usize).checked_sub(1).and_then(|i| claims.get(i));
                    kelve_core::views::ClaimCheck {
                        n: cc.n,
                        text: claim.map(|c| c.text.clone()).unwrap_or_default(),
                        state: word(cc.state).into(),
                        found: cc.found.clone(),
                        block: claim.map(|c| c.block.clone()).unwrap_or_default(),
                        words: claim.map(|c| c.words.clone()).unwrap_or_default(),
                    }
                })
                .collect(),
        })
    });
    // Said of the entry shown: not of a replacement that's ready.
    let problems: Vec<String> = row
        .as_ref()
        .filter(|r| !(r.state == "ready" && r.replacing.is_some()))
        .and_then(|r| r.problems.as_deref())
        .and_then(|p| serde_json::from_str(p).ok())
        .unwrap_or_default();
    let rewriting = row.as_ref().is_some_and(|r| {
        r.replacing.is_some() && !matches!(r.state.as_str(), "ready" | "dropped" | "waiting")
    });
    // While it's written: the sites of the pages it's written from.
    let pages: Vec<String> = if state == EntryState::Writing {
        row.as_ref()
            .and_then(|r| r.plan.as_deref())
            .and_then(|p| serde_json::from_str::<entries::Plan>(p).ok())
            .map(|p| {
                let mut sites: Vec<String> = Vec::new();
                for page in &p.pages {
                    let site = page
                        .url
                        .split("://")
                        .nth(1)
                        .and_then(|rest| rest.split('/').next())
                        .unwrap_or_default()
                        .trim_start_matches("www.")
                        .to_string();
                    if !site.is_empty() && !sites.contains(&site) {
                        sites.push(site);
                    }
                }
                sites
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let question = match entry
        .as_ref()
        .map(|e: &kelve_core::Entry| e.question.clone())
    {
        Some(q) if !q.is_empty() => q,
        _ => {
            let mut conn = app.db.acquire().await.map_err(ApiError::internal)?;
            kelve_store::relations::get(&mut conn, &key)
                .await
                .map_err(ApiError::internal)?
                .map(|k| k.question)
                .unwrap_or_default()
        }
    };
    let (lessons, lessons_all) = crate::lessons::about_idea(&app, &key, false).await?;
    let reads = if from_graph {
        let mut conn = app.db.acquire().await?;
        u32::try_from(kelve_store::entries::reads(&mut conn, &key).await?).unwrap_or(u32::MAX)
    } else {
        0
    };
    Ok(Json(IdeaPage {
        key,
        name: idea.name,
        definition: idea.definition,
        question,
        entry,
        state,
        step,
        stage,
        problems,
        rewriting,
        can_flag,
        split: Vec::new(),
        pages,
        agent,
        checking,
        kept_here,
        lessons,
        lessons_all,
        from_graph,
        reads,
    }))
}

/// Where the entries for what's open came from: the graph, or the
/// person's agent.
pub async fn origins(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Origins>> {
    let open = reader::opened(&app, id).await?;
    let keys: Vec<String> = if open.kind() == OpenKind::Change {
        let change = reader::change(&open)?;
        reader::change_built_on(&app, &open, &change)
            .await?
            .0
            .into_iter()
            .map(|b| b.key)
            .collect()
    } else {
        reader::folder_ideas(&app, &open)
            .await?
            .0
            .into_iter()
            .map(|(key, _, _)| key)
            .collect()
    };
    let mut origins = Origins {
        ideas: keys.len() as u32,
        from_graph: 0,
        by_agent: 0,
    };
    let mut conn = app.db.acquire().await?;
    for key in &keys {
        match kelve_store::entries::get(&mut conn, key).await? {
            Some(e) if e.from_graph => origins.from_graph += 1,
            Some(e) if e.entry.is_some() => origins.by_agent += 1,
            _ => {}
        }
    }
    Ok(Json(origins))
}

/// Tells the shared graph's reviewers an entry, or a link in it, is wrong.
/// The person's words go with the revision they read, and nothing else:
/// secrets are stripped from them first, as from everything that leaves.
pub async fn flag(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(key): UrlPath<String>,
    Json(flag): Json<FlagIn>,
) -> ApiResult<Json<serde_json::Value>> {
    let why = kelve_core::redact::redact(flag.why.trim());
    if why.chars().count() < 3 {
        return Err(ApiError::bad_request("Say what’s wrong, in a few words."));
    }
    if why.chars().count() > 1_000 {
        return Err(ApiError::bad_request(
            "Say what’s wrong in under 1,000 characters.",
        ));
    }
    let row = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::get(&mut conn, &key).await?
    };
    let revision = row.and_then(|r| r.revision);
    // How the idea relates is reported by the graph's IDs for both ends,
    // with or without an entry.
    let relation = match &flag.relation {
        None => None,
        Some(r) => {
            use crate::pipeline::relations::graph_id;
            match (
                graph_id(&app, &key).await.map_err(ApiError::internal)?,
                graph_id(&app, &r.to).await.map_err(ApiError::internal)?,
            ) {
                (Some(from), Some(to)) => Some(kelve_graph_api::RelationRef {
                    from,
                    kind: r.kind,
                    to,
                }),
                _ => {
                    return Err(ApiError::bad_request(
                        "That relation isn’t in the shared graph yet, so there’s no one to tell.",
                    ));
                }
            }
        }
    };
    let Some(graph) = app.graph_reader() else {
        return Err(ApiError::bad_request(
            "kelve isn’t using the shared graph, so there’s no one to tell.",
        ));
    };
    if revision.is_none() && relation.is_none() {
        return Err(ApiError::bad_request(
            "This entry isn’t in the shared graph, so there’s no one to tell. Write it again instead.",
        ));
    }
    let sent = kelve_graph_api::Flag {
        revision: revision.unwrap_or_default(),
        link: flag
            .link
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty()),
        relation,
        why,
    };
    graph
        .flag(&sent)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    tracing::info!("told the shared graph's reviewers an entry, or how its idea relates, is wrong");
    Ok(Json(json!({})))
}

/// Tells the shared graph an entry didn't help the person understand, and
/// what it needs. The revision they read and one word go, and nothing
/// else: the next agent to add to the graph that opens it adds what's
/// asked.
pub async fn ask(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(key): UrlPath<String>,
    Json(asked): Json<kelve_core::views::AskIn>,
) -> ApiResult<Json<serde_json::Value>> {
    if !asked.kind.from_readers() {
        return Err(ApiError::bad_request(
            "Say it needs an example, that it’s too hard, or that something’s missing.",
        ));
    }
    let revision = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::get(&mut conn, &key)
            .await?
            .and_then(|r| r.revision)
    };
    let Some(graph) = app.graph_reader() else {
        return Err(ApiError::bad_request(
            "kelve isn’t using the shared graph, so there’s no one to tell.",
        ));
    };
    let Some(revision) = revision else {
        return Err(ApiError::bad_request(
            "This entry isn’t in the shared graph, so there’s no one to tell.",
        ));
    };
    graph
        .ask(&kelve_graph_api::Ask {
            revision,
            kind: asked.kind,
        })
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    tracing::info!("told the shared graph an entry didn't help its reader understand");
    Ok(Json(json!({})))
}

/// Writes a dropped entry again, asked for by the person.
pub async fn write_again(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(key): UrlPath<String>,
) -> ApiResult<Json<serde_json::Value>> {
    let idea = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::idea(&mut conn, &key).await?
    }
    .ok_or_else(|| ApiError::not_found("kelve doesn't know that idea."))?;
    let back = {
        let mut conn = app.db.acquire().await?;
        if let Some(r) = kelve_store::entries::get(&mut conn, &key).await?
            && r.state == "dropped"
        {
            kelve_store::entries::set(&mut conn, &key, "waiting", "").await?;
        }
        // Taken off the agent's list, and asked for again: back on it.
        kelve_store::asks::put_back(&mut conn, &key).await?
    };
    if back {
        app.publish(ServerEvent::Asks);
    }
    entries::want(&app, &idea)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(json!({})))
}

pub async fn notes(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Vec<NoteRef>>> {
    let open = reader::opened(&app, id).await?;
    Ok(Json(reader::notes(&app, &open)))
}

#[derive(Debug, Deserialize)]
pub struct NoteAt {
    note: String,
}

pub async fn note(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(at): Query<NoteAt>,
) -> ApiResult<Json<Note>> {
    let open = reader::opened(&app, id).await?;
    Ok(Json(reader::note(&app, &open, &at.note)?))
}

pub async fn forget_note(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(at): Query<NoteAt>,
) -> ApiResult<Json<serde_json::Value>> {
    let open = reader::opened(&app, id).await?;
    reader::forget_note(&app, &open, &at.note)?;
    Ok(Json(json!({})))
}

/// Your own notes on an idea, from every project.
pub async fn notes_on_idea(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(key): UrlPath<String>,
) -> ApiResult<Json<Vec<Note>>> {
    Ok(Json(reader::notes_on_idea(&app, &key)))
}

pub async fn keep_note(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Json(note): Json<NoteIn>,
) -> ApiResult<Json<Note>> {
    let open = reader::opened(&app, id).await?;
    let kept = reader::keep_note(&app, &open, &note)?;
    if note.id.is_none() {
        let place = match note.place {
            Place::You => "notes.you",
            Place::Work => "notes.work",
            Place::Agent => "notes.agent",
        };
        crate::counts::count(&app, place).await;
    }
    Ok(Json(kept))
}

#[derive(Debug, Deserialize)]
pub struct Search {
    q: String,
    opened: Option<i64>,
}

/// Ideas, by what they're called or what they are; files, by their paths
/// and what they say; and notes.
pub async fn search(
    State(app): State<Shared>,
    _me: Me,
    Query(s): Query<Search>,
) -> ApiResult<Json<Found>> {
    let q = s.q.trim().to_lowercase();
    let mut found = Found {
        ideas: Vec::new(),
        files: Vec::new(),
        lines: Vec::new(),
        notes: Vec::new(),
    };
    if q.chars().count() < 2 {
        return Ok(Json(found));
    }
    let open = match s.opened {
        Some(id) => reader::opened(&app, id).await.ok(),
        None => None,
    };
    // Ideas in what's open first, the most used first, with how many of
    // its files each is in.
    let own = match &open {
        Some(open) => super::own::own(&app, open).await.ok(),
        None => None,
    };
    if let Some(own) = &own {
        let mut conn = app.db.acquire().await?;
        for t in own
            .top
            .iter()
            .filter(|t| t.name.to_lowercase().contains(&q))
            .take(6)
        {
            let idea = kelve_store::entries::idea(&mut conn, &t.key).await?;
            let ready = kelve_store::entries::get(&mut conn, &t.key)
                .await?
                .is_some_and(|e| e.state == "ready");
            found.ideas.push(FoundIdea {
                key: t.key.clone(),
                name: t.name.clone(),
                definition: idea.map(|i| i.definition).unwrap_or_default(),
                ready,
                files: Some(t.files),
            });
        }
    }
    {
        let mut conn = app.db.acquire().await?;
        for (idea, ready) in kelve_store::entries::find(&mut conn, &q, 8).await? {
            if found.ideas.iter().any(|i| i.key == idea.key) {
                continue;
            }
            // Found by what it is, it may still be in what's open.
            let files = own
                .iter()
                .flat_map(|o| &o.top)
                .find(|t| t.key == idea.key)
                .map(|t| t.files);
            found.ideas.push(FoundIdea {
                key: idea.key,
                name: idea.name,
                definition: idea.definition,
                ready,
                files,
            });
        }
    }
    // The shared graph's, when it answers quickly, and only a plain name.
    if found.ideas.len() < 8
        && kelve_core::redact::query_ok(&q, &[]).is_ok()
        && let Some(graph) = app.graph_reader()
    {
        let mut req = kelve_graph_api::MatchRequest::default();
        req.queries.insert(
            "q".into(),
            kelve_graph_api::Query {
                query: q.clone(),
                context: None,
                limit: Some(6),
            },
        );
        if let Ok(Ok(answer)) =
            tokio::time::timeout(reader::GRAPH_WAIT, graph.candidates(&req)).await
            && let Some(c) = answer.get("q")
        {
            for c in &c.result {
                if found.ideas.len() == 8 || found.ideas.iter().any(|i| i.key == c.id) {
                    continue;
                }
                found.ideas.push(FoundIdea {
                    key: c.id.clone(),
                    name: c.name.clone(),
                    definition: c.definition.clone(),
                    ready: false,
                    files: None,
                });
            }
        }
    }
    if let Some(open) = &open {
        let (root, words) = (open.root.clone(), q.clone());
        if let Ok((files, lines)) =
            tokio::task::spawn_blocking(move || in_files(&root, &words)).await
        {
            found.files = files;
            found.lines = lines;
        }
        found.notes = reader::notes(&app, open)
            .into_iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&q)
                    || reader::note(&app, open, &n.id)
                        .is_ok_and(|note| note.text.to_lowercase().contains(&q))
            })
            .take(6)
            .collect();
    }
    Ok(Json(found))
}

/// The most lines search shows, and from any one file.
const LINES: usize = 12;
const LINES_A_FILE: usize = 3;
/// About how much of a line search shows.
const LINE_CHARS: usize = 120;

/// Files whose paths hold `q`, and lines that say it, in a walk of `root`.
/// Each file is read as the reader reads it, its secrets masked, so search
/// finds nothing the reader wouldn't show.
fn in_files(root: &Path, q: &str) -> (Vec<String>, Vec<FoundLine>) {
    let Ok(walked) = kelve_engine::walk::walk(root) else {
        return (Vec::new(), Vec::new());
    };
    let rels: Vec<String> = walked
        .files
        .into_iter()
        .map(|f| f.rel)
        .filter(|p| !reader::is_kelves(p))
        .collect();
    let files = rels
        .iter()
        .filter(|p| p.to_lowercase().contains(q))
        .take(8)
        .cloned()
        .collect();
    let mut lines = Vec::new();
    for rel in &rels {
        if lines.len() == LINES {
            break;
        }
        let Ok(text) = kelve_engine::read::read(root, rel) else {
            continue;
        };
        for (n, line) in text
            .text
            .lines()
            .enumerate()
            .filter(|(_, l)| l.to_lowercase().contains(q))
            .take(LINES_A_FILE.min(LINES - lines.len()))
        {
            lines.push(FoundLine {
                path: rel.clone(),
                line: u32::try_from(n + 1).unwrap_or(u32::MAX),
                text: around(line, q),
            });
        }
    }
    (files, lines)
}

/// `line`, trimmed, and cut to the part around `q` when it's long.
fn around(line: &str, q: &str) -> String {
    let line = line.trim();
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= LINE_CHARS {
        return line.to_string();
    }
    let lower = line.to_lowercase();
    let at = lower
        .find(q)
        .map_or(0, |b| lower[..b].chars().count())
        .min(chars.len());
    let from = at.saturating_sub(LINE_CHARS / 3);
    let to = (from + LINE_CHARS).min(chars.len());
    let mut cut: String = chars[from..to].iter().collect();
    if from > 0 {
        cut.insert(0, '…');
    }
    if to < chars.len() {
        cut.push('…');
    }
    cut
}
