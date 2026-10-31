//! What the person's own agent calls, through `kelve mcp` or the command line,
//! with this computer's secret: open something for the person, take kelve's
//! jobs and hand in their answers, and see what's kept for it. And what the
//! reader asks about the agent, as the person.
//!
//! The agent never reads what kelve keeps beyond a job's material: not the
//! person's notes, except those they leave for it, not what was read before,
//! not the shared graph's key.

use std::path::{Path, PathBuf};

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use kelve_core::views::Kept;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::reader::{open_at, show};
use crate::agent::{self, Caller, HandedIn, Job};
use crate::auth::{Agent, Me};
use crate::error::{ApiError, ApiResult};
use crate::reader;
use crate::state::Shared;

/// What to open, as the agent understands the person's ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum What {
    /// A change: what isn't committed, a branch against a base, or one
    /// commit.
    Change,
    /// A worktree: everything since it left its base, committed or not.
    Worktree,
    Folder,
    /// Some files, in the folder they're in.
    Files,
    File,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenIn {
    pub what: What,
    /// Where, from the root: the folder the agent works in, when it says
    /// nothing else.
    pub path: Option<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    /// The branch a change or a worktree is read against.
    pub base: Option<String>,
    /// One commit, for a change.
    pub commit: Option<String>,
    /// What it is, in a few words, as the agent would title it: shown at
    /// the top, in place of git's own title.
    #[serde(default)]
    pub title: String,
    /// The agent's own line about the work: what it is, or what changed
    /// and why.
    #[serde(default)]
    pub said: String,
    #[serde(flatten)]
    pub caller: Caller,
}

/// What opening came to, for the agent: what's open, how much of it needs
/// reading, and whether the person sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    /// Its number: what the agent gives next_job, as `for`, to take only its jobs.
    pub id: i64,
    pub title: String,
    pub files: u32,
    /// How many of its folders are read plainly with it, each a page.
    pub folders: u32,
    /// How many of its files, and of its pages, need reading: its folders'
    /// and a change's own.
    pub to_read: u32,
    pub shown: bool,
}

/// The longest title an agent may give the work.
const TITLE: usize = 80;
/// The longest line an agent may say about the work.
const SAID: usize = 200;

/// Opens something for the person, as their agent asks.
pub async fn open(
    State(app): State<Shared>,
    _agent: Agent,
    Json(asked): Json<OpenIn>,
) -> ApiResult<Json<Opened>> {
    agent::hello(&app, &asked.caller).await?;
    let path = asked.path.as_deref().map(str::trim).unwrap_or_default();
    let (path, change, only) = match asked.what {
        What::Change => {
            let words = match (&asked.commit, &asked.base) {
                (Some(c), _) if !c.trim().is_empty() => format!("commit {}", c.trim()),
                (_, Some(b)) if !b.trim().is_empty() => format!("against {}", b.trim()),
                _ => "uncommitted".to_string(),
            };
            (path.to_string(), Some(words), None)
        }
        What::Worktree => {
            let base = asked
                .base
                .as_deref()
                .map(str::trim)
                .filter(|b| !b.is_empty())
                .unwrap_or("main");
            (path.to_string(), Some(format!("since {base}")), None)
        }
        What::Folder | What::File => (path.to_string(), None, None),
        What::Files => {
            let (root, rels) = files_in_folder(&asked.paths)?;
            (root.display().to_string(), None, Some(rels))
        }
    };
    if path.is_empty() {
        return Err(ApiError::bad_request(
            "Say where, from the root: the folder you're working in.",
        ));
    }
    let id = open_at(&app, &path, change.as_deref()).await?;
    // Notes it has just done lead to what it opened.
    reader::link_done_notes(&app, Path::new(&path), id);
    let plain = |text: &str, most: usize| -> String {
        text.chars()
            .filter(|c| !c.is_control())
            .take(most)
            .collect::<String>()
            .trim()
            .to_string()
    };
    let (title, said) = (plain(&asked.title, TITLE), plain(&asked.said, SAID));
    {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::opened_by(
            &mut conn,
            id,
            &asked.caller.agent,
            &title,
            &said,
            only.as_ref()
                .map(|o| serde_json::to_string(o).unwrap_or_default())
                .as_deref(),
        )
        .await?;
    }
    let open = reader::opened(&app, id).await?;
    let (files, to_read, folders) = to_read(&app, &open).await?;
    let shown = show(&app, id).await?;
    Ok(Json(Opened {
        id,
        title: open.row.title.clone(),
        files,
        folders,
        to_read,
        shown,
    }))
}

/// Some files, as the folder they're all in and their paths from it.
fn files_in_folder(paths: &[String]) -> ApiResult<(PathBuf, Vec<String>)> {
    let paths: Vec<PathBuf> = paths
        .iter()
        .map(|p| reader::unshown(p.trim()))
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    let Some(first) = paths.first() else {
        return Err(ApiError::bad_request("Say which files, from the root."));
    };
    if paths.iter().any(|p| !p.is_absolute() || !p.is_file()) {
        return Err(ApiError::bad_request(
            "Give each file's whole path, from the root, and only files that are there.",
        ));
    }
    let mut root = first.parent().map(Path::to_path_buf).unwrap_or_default();
    while !paths.iter().all(|p| p.starts_with(&root)) {
        root = root
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| ApiError::bad_request("Those files share no folder."))?;
    }
    // In a repository, they're opened in it.
    let root = kelve_engine::git::root_of(&root).unwrap_or(root);
    let rels = paths
        .iter()
        .filter_map(|p| p.strip_prefix(&root).ok())
        .map(|p| {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    Ok((root, rels))
}

/// How many files what's open has, how many of them and of its pages need
/// reading, their ideas checked and read plainly, and how many of its
/// folders are read plainly with it: asking for them to be read.
async fn to_read(app: &Shared, open: &reader::Open) -> ApiResult<(u32, u32, u32)> {
    match open.kind() {
        kelve_core::views::OpenKind::Change => {
            let change = reader::change(open)?;
            // No way in yet: the agent's first job is to find one
            // (`reader::mark_change`), which its reading then follows.
            let reading = reader::mark_change(app, open, &change).await?;
            let pages = crate::plainly::pages(app, open, Some(&change), true).await?;
            Ok((
                change.files.len() as u32,
                reading.of - reading.read + pages.left,
                pages.folders,
            ))
        }
        _ => {
            // What's been read is kept in the project too: known first, so
            // it isn't counted as needing reading.
            crate::read::load(app, open).await;
            let walked = kelve_engine::walk::walk(&open.root)
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            let only = open.only();
            let read = reader::files_read(app, open).await?;
            // Read counts only if read under the instructions used now,
            // looking for all that's wanted now.
            let version = crate::pipeline::mark::read_version(app).await;
            let (wanted, _) = crate::pipeline::mark::lens_for(app);
            let under = reader::read_under(&version, &wanted);
            let written = crate::plainly::read_so_far(app).await?;
            let mut files = 0;
            let mut todo: Vec<(&str, bool)> = Vec::new();
            for f in walked.files.iter().filter(|f| {
                f.skip.is_none()
                    && !reader::is_kelves(&f.rel)
                    && only.as_ref().is_none_or(|o| o.contains(&f.rel))
                    && (open.row.path.is_empty() || open.row.path == f.rel)
            }) {
                files += 1;
                let r = read.get(&f.rel);
                let read_now = r.is_some_and(|r| {
                    r.skip.is_some() || r.hash.as_ref().is_some_and(|h| h.starts_with(&under))
                });
                todo.push((
                    &f.rel,
                    if read_now {
                        r.is_some_and(|r| reader::read_through(r, &f.rel, &written))
                    } else if crate::read::kept_as_is(open, &f.rel, &version, &wanted) {
                        // Read in the project, as it is now: read plainly too?
                        !written.wanted(&f.rel)
                            || kelve_engine::read::read(&open.root, &f.rel)
                                .is_ok_and(|t| written.read(&f.rel, &reader::text_hash(&t.text)))
                    } else {
                        false
                    },
                ));
            }
            // No way in yet: the agent's first job is to find one, with its
            // folders' lines, though much of it is read already; only the
            // way in is read, until the person asks for more.
            let left = todo.iter().filter(|(_, done)| !done).count();
            crate::way::ask_way_first(app, open).await?;
            let left = match crate::way::to_read_now(app, open).await {
                // Finding it: the way in is about as long as it'll be.
                Some(now) if now.is_empty() => left.min(crate::way::MOST_FOUND),
                Some(now) => todo
                    .iter()
                    .filter(|(p, done)| !done && now.contains(*p))
                    .count(),
                None => left,
            };
            reader::read_ahead(app, open).await?;
            let pages = crate::plainly::pages(app, open, None, true).await?;
            Ok((files, left as u32 + pages.left, pages.folders))
        }
    }
}

/// Says the agent is here: what settings show as connected.
pub async fn hello(
    State(app): State<Shared>,
    _agent: Agent,
    Json(caller): Json<Caller>,
) -> ApiResult<Json<Value>> {
    agent::hello(&app, &caller).await?;
    Ok(Json(serde_json::json!({ "connected": true })))
}

/// The next job for the agent, or none.
pub async fn next(
    State(app): State<Shared>,
    _agent: Agent,
    Json(caller): Json<Caller>,
) -> ApiResult<Json<Option<Job>>> {
    Ok(Json(agent::next(&app, &caller).await?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerIn {
    pub job: i64,
    pub answer: Value,
    /// false when it's the last job the agent will take: no next one is
    /// taken for it, and it's told how many are left.
    #[serde(default = "more")]
    pub more: bool,
    #[serde(flatten)]
    pub caller: Caller,
}

fn more() -> bool {
    true
}

/// What became of an answer, and the next job, when it was taken.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Answered {
    #[serde(flatten)]
    pub handed_in: HandedIn,
    pub next: Option<Job>,
    /// How many jobs are left, when the agent said it took its last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub left: Option<i64>,
}

/// The most kelve waits, as an answer comes in, for work running then to
/// ask for something more.
const LINGER: std::time::Duration = std::time::Duration::from_secs(5);

/// The most time kelve spends on what's due, as an answer comes in,
/// before it starts nothing more and replies: well inside the two minutes
/// the command line waits for it (`kelve mcp`), however much is due.
const SPEND: std::time::Duration = std::time::Duration::from_secs(15);

/// The agent hands in its answer to a job.
pub async fn answer(
    State(app): State<Shared>,
    _agent: Agent,
    Json(given): Json<AnswerIn>,
) -> ApiResult<Json<Answered>> {
    // The file or idea the job was for: the next for the same comes first.
    let answered = {
        let mut conn = app.db.acquire().await?;
        kelve_store::asks::get(&mut conn, given.job).await?
    };
    let after = kelve_store::asks::After {
        resume_key: answered.as_ref().and_then(|a| a.resume_key.as_deref()),
        idea: answered.as_ref().and_then(|a| a.idea.as_deref()),
    };
    let handed_in = agent::hand_in(&app, &given.caller, given.job, given.answer).await?;
    let next = match handed_in {
        HandedIn::Yes => {
            // The work the answer was for runs before the next job is
            // given, so what it asks next is in the list: its own job
            // first, ahead of whatever else is due, then what follows from
            // it. An agent waits for this reply and gives up after a
            // while, so past `SPEND` the rest is the workers'.
            let spent = std::time::Instant::now() + SPEND;
            if let Some(key) = answered.as_ref().and_then(|a| a.resume_key.as_deref()) {
                crate::pipeline::run_key(&app, key)
                    .await
                    .map_err(ApiError::internal)?;
            }
            for _ in 0..20 {
                if std::time::Instant::now() >= spent
                    || crate::pipeline::run_once(&app)
                        .await
                        .map_err(ApiError::internal)?
                        .is_none()
                {
                    break;
                }
            }
            if !given.more {
                let mut conn = app.db.acquire().await?;
                let left = kelve_store::asks::waiting(&mut conn).await?;
                return Ok(Json(Answered {
                    handed_in,
                    next: None,
                    left: Some(left),
                }));
            }
            let mut next = agent::next_after(&app, &given.caller, after).await?;
            // Work running in the background as the answer came, such as
            // a folder's reading that waited on this file, may ask for
            // something in a moment: wait a little for it, rather than
            // telling the agent there's nothing more, so it stops.
            let until = std::time::Instant::now() + LINGER;
            while next.is_none() && std::time::Instant::now() < until {
                let busy = {
                    let mut conn = app.db.acquire().await?;
                    kelve_store::jobs::busy(&mut conn).await?
                };
                if !busy {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                // What's due runs here while there's time, so what it asks
                // comes now; past that, the workers have it.
                while std::time::Instant::now() < until
                    && crate::pipeline::run_once(&app)
                        .await
                        .map_err(ApiError::internal)?
                        .is_some()
                {}
                next = agent::next_after(&app, &given.caller, after).await?;
            }
            next
        }
        HandedIn::No { .. } => None,
    };
    Ok(Json(Answered {
        handed_in,
        next,
        left: None,
    }))
}

/// What's kept for the agent, the most wanted first.
pub async fn kept(State(app): State<Shared>, _agent: Agent) -> ApiResult<Json<Vec<Kept>>> {
    Ok(Json(kept_list(&app).await?))
}

#[derive(Debug, Deserialize)]
pub struct Where {
    /// The folder the agent is working in.
    path: String,
}

/// The notes the person left for their agent, waiting, in the project the
/// agent is working in: fenced, to be done only as part of their work.
pub async fn notes(
    State(app): State<Shared>,
    _agent: Agent,
    axum::extract::Query(at): axum::extract::Query<Where>,
) -> ApiResult<Json<Vec<reader::ForAgent>>> {
    Ok(Json(
        reader::waiting_for_agent(&app, Path::new(&at.path)).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct Told {
    /// The folder the agent is working in.
    path: String,
    /// The agent, as its hook names it.
    agent: String,
}

/// How many notes wait for the agent that its hook hasn't told it of, as
/// they are now: the hook's one line counts only those.
pub async fn notes_new(
    State(app): State<Shared>,
    _agent: Agent,
    Json(told): Json<Told>,
) -> ApiResult<Json<Value>> {
    let agent = told.agent.trim();
    let new = reader::new_for_agent(&app, Path::new(&told.path), agent).await?;
    Ok(Json(serde_json::json!({ "new": new })))
}

#[derive(Debug, Deserialize)]
pub struct Done {
    path: String,
    note: String,
    did: String,
    /// The note's version, as notes gave it.
    #[serde(default)]
    version: String,
    #[serde(default)]
    opened: Option<i64>,
}

/// The agent did what a note asked: it says so, in a line, for the words
/// it read.
pub async fn note_done(
    State(app): State<Shared>,
    _agent: Agent,
    Json(done): Json<Done>,
) -> ApiResult<Json<Value>> {
    if done.version.trim().is_empty() {
        return Err(ApiError::bad_request(
            "Give the note's version, as notes gave it.",
        ));
    }
    reader::note_done(
        &app,
        Path::new(&done.path),
        &done.note,
        &done.did,
        done.opened,
        reader::DoneFor::Read(&done.version),
    )
    .await?;
    Ok(Json(serde_json::json!({ "done": true })))
}

/// What's kept for the agent, as the reader lists it.
pub async fn kept_for_me(State(app): State<Shared>, _me: Me) -> ApiResult<Json<Vec<Kept>>> {
    Ok(Json(kept_list(&app).await?))
}

async fn kept_list(app: &Shared) -> ApiResult<Vec<Kept>> {
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::asks::open(&mut conn)
        .await?
        .into_iter()
        .map(|a| Kept {
            id: a.id,
            kind: a.kind,
            about: a.about,
            idea: a.idea,
            taken: a.state == "taken",
        })
        .collect())
}

/// The person takes a job off the agent's list.
pub async fn put_aside(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<Vec<Kept>>> {
    agent::put_aside(&app, id).await?;
    Ok(Json(kept_list(&app).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn some_files_open_in_the_folder_they_share() {
        let root =
            std::env::temp_dir().join(format!("kelve-files-{}", uuid::Uuid::now_v7().simple()));
        std::fs::create_dir_all(root.join("notes")).unwrap_or_default();
        let a = root.join("notes/heat-pump.md");
        let b = root.join("notes/loft.md");
        std::fs::write(&a, "# Heat pump\n").unwrap_or_default();
        std::fs::write(&b, "# Loft\n").unwrap_or_default();
        let (folder, rels) = files_in_folder(&[a.display().to_string(), b.display().to_string()])
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert!(folder.ends_with("notes"));
        assert_eq!(rels, ["heat-pump.md", "loft.md"]);
        assert!(files_in_folder(&[]).is_err());
        assert!(files_in_folder(&["relative.md".into()]).is_err());
        std::fs::remove_dir_all(&root).ok();
    }
}

/// The command line put kelve's hook in the agent's settings, or took it
/// out: a page that's open says so at once.
pub async fn hook_changed(State(app): State<Shared>, _agent: Agent) -> Json<serde_json::Value> {
    app.publish(kelve_core::views::ServerEvent::Settings);
    Json(serde_json::json!({}))
}

/// What kelve marks for the person, for their agent to read before it
/// changes it.
pub async fn marks(
    State(app): State<Shared>,
    _agent: Agent,
) -> ApiResult<Json<kelve_core::views::MarksView>> {
    Ok(Json(crate::api::settings::marks_view(&app)))
}

#[derive(Debug, Deserialize)]
pub struct SetMarks {
    spec: kelve_core::spec::Spec,
}

/// The person's agent writes what kelve marks for them, as they asked.
pub async fn set_marks(
    State(app): State<Shared>,
    _agent: Agent,
    Json(set): Json<SetMarks>,
) -> ApiResult<Json<kelve_core::views::MarksView>> {
    crate::spec::save(&app, crate::spec::checked(set.spec)?, "agent")?;
    app.publish(kelve_core::views::ServerEvent::Spec);
    Ok(Json(crate::api::settings::marks_view(&app)))
}
