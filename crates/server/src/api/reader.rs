//! What's open, as the web app reads it: opening something, its files and
//! changes, the agent's list, and notes. Only the person reads any of it;
//! kelve's command line can ask to open something, and learn whether a
//! browser opened, nothing more.

use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::{Path as UrlPath, Query, State};
use kelve_core::views::{
    AgentList, ChangeView, ChangedFile, DiffLine, DiffView, FileItem, FileView, FolderView, Note,
    NoteIn, NoteRef, OpenIn, OpenKind, Opened, PickIn, PickWhat, Picked, Place, ServerEvent,
};
use kelve_engine::git::{LineKind, Status};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::{Me, Who};
use crate::error::{ApiError, ApiResult};
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
/// to it: it's what's open. Going to what's open already changes nothing.
pub async fn open_again(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Opened>> {
    let open = reader::opened(&app, id).await?;
    {
        let mut conn = app.db.acquire().await?;
        if !kelve_store::reader::again(&mut conn, id).await? {
            return Ok(Json(view_of(&app, &open).await?));
        }
        kelve_store::asks::let_go_of_first(&mut conn).await?;
    }
    app.publish(ServerEvent::Opened);
    app.publish(ServerEvent::Asks);
    Ok(Json(view_of(&app, &open).await?))
}

/// Shows the person what was opened for them, in a new browser tab, which
/// brings their browser to the front, so they see it at once; a tab they
/// have open on something else, perhaps another agent's work, stays where
/// it is (Nathan, 29 October). A tab already showing the same thing gives
/// the new one its place, where the person was in it, and closes (the web
/// app's `tabs.ts`): no page can bring itself forward, so only a new tab
/// is sure to be seen. It's signed in by a one-time link that goes to the
/// person, never to whoever asked. Whether they see it.
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
        id
    };
    app.publish(ServerEvent::Opened);
    app.publish(ServerEvent::Asks);
    tracing::info!(kind, "opened something");
    crate::counts::count(app, &format!("opened.{kind}")).await;
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
        out.push(open.view());
    }
    Ok(Json(out))
}

pub async fn one(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Opened>> {
    let open = reader::opened(&app, id).await?;
    Ok(Json(view_of(&app, &open).await?))
}

/// What's open, with the handover kept for it and its debrief, if there
/// are: where the web app shows it.
async fn view_of(app: &Shared, open: &Open) -> ApiResult<Opened> {
    let mut view = open.view();
    if open.kind() == OpenKind::Change {
        let mut conn = app.db.acquire().await?;
        if let Some((handover, debrief)) =
            kelve_store::handovers::latest_for(&mut conn, open.row.id).await?
        {
            view.handover = Some(handover);
            view.debrief = debrief;
        }
    }
    Ok(view)
}

/// A folder's files: for a folder or a file, the folder it's in, or only
/// the files opened.
pub async fn folder(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<FolderView>> {
    let open = reader::opened(&app, id).await?;
    let walked =
        kelve_engine::walk::walk(&open.root).map_err(|e| ApiError::bad_request(e.to_string()))?;
    let only = open.only();
    let files = walked
        .files
        .into_iter()
        .filter(|f| !reader::is_kelves(&f.rel))
        .filter(|f| only.as_ref().is_none_or(|o| o.contains(&f.rel)))
        .map(|f| FileItem {
            skip: f.skip.map(|s| s.says().to_string()),
            path: f.rel,
        })
        .collect();
    Ok(Json(FolderView {
        opened: view_of(&app, &open).await?,
        files,
        more: walked.more,
    }))
}

#[derive(Debug, Deserialize)]
pub struct At {
    path: String,
}

/// A file in what's open, as it is, secrets masked.
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
            }));
        }
        Err(e) => return Err(ApiError::not_found(e.to_string())),
    };
    Ok(Json(FileView {
        path: path.to_string(),
        lang: read.lang.map(str::to_string),
        text: Some(read.text),
        skip: None,
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

/// A change, read from git: its files.
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
    Ok(Json(ChangeView {
        opened: view_of(&app, &open).await?,
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
                moved_from: f.moved_from.clone(),
            })
            .collect(),
        more: change.more,
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
}

/// Takes something off your agent's list: it isn't done.
pub async fn list_off(
    State(app): State<Shared>,
    _me: Me,
    Json(off): Json<Off>,
) -> ApiResult<Json<AgentList>> {
    crate::list::take_off(&app, &off.asks).await?;
    Ok(Json(crate::list::view(&app).await?))
}

/// One file of a change, whole, with what it took out where it was.
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
    }))
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
