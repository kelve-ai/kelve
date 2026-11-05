//! What the person's own agent calls, through `kelve mcp` or the command line,
//! with this computer's secret: open something for the person, take kelve's
//! jobs and hand in their answers, and see what's kept for it. And what the
//! reader asks about the agent, as the person.
//!
//! The agent never reads what kelve keeps beyond a job's material: not the
//! person's notes, except those they leave for it, not their skills' evidence,
//! not the hosted service's key.

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
    /// The person asked for a debrief of this work ("debrief this").
    #[serde(default)]
    pub debrief: bool,
    #[serde(flatten)]
    pub caller: Caller,
}

/// What opening came to, for the agent: what's open, and whether the
/// person sees it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    /// Its number: what the agent gives next_job, as `for`, to take only its jobs.
    pub id: i64,
    pub title: String,
    pub shown: bool,
    /// Work handed over: whether a debrief is due, and, when it is, its
    /// number, for the agent to write it now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debrief: Option<crate::agent_loop::debrief::Handed>,
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
            let (root, rels) = in_known_folder(&app, root, rels).await?;
            (root.display().to_string(), None, Some(rels))
        }
    };
    if path.is_empty() {
        return Err(ApiError::bad_request(
            "Say where, from the root: the folder you're working in.",
        ));
    }
    // What the person committed since the last handover there is compared
    // with it first, so it's learned from before more is handed over.
    if change.is_some()
        && let Err(e) = crate::agent_loop::after::look(&app, &path).await
    {
        tracing::warn!(error = %e.message, "couldn't compare what was committed with a handover");
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
    // Work shown as a change or a worktree is handed over: kept as it is
    // now, to compare with what's committed later.
    let mut debrief = None;
    if matches!(asked.what, What::Change | What::Worktree) {
        match crate::handover::record(&app, &open, &asked.caller.agent, &title, &said).await {
            Ok(Some(handover)) => {
                match crate::agent_loop::debrief::on_handover(&app, &open, handover, asked.debrief)
                    .await
                {
                    Ok(handed) => debrief = Some(handed),
                    Err(e) => {
                        tracing::warn!(error = %e.message, "couldn't ask for a debrief of what was handed over")
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e.message, "couldn't keep what the agent handed over")
            }
        }
    }
    let shown = show(&app, id).await?;
    Ok(Json(Opened {
        id,
        title: open.row.title.clone(),
        shown,
        debrief,
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

/// Files outside a repository, in the folder kelve knows them in, opened
/// or worked in by the agent, when it holds them: the same folder whichever of them are shown,
/// so showing them again is the same thing in kelve, brought up to date.
async fn in_known_folder(
    app: &Shared,
    root: PathBuf,
    rels: Vec<String>,
) -> ApiResult<(PathBuf, Vec<String>)> {
    if kelve_engine::git::root_of(&root).is_some() {
        return Ok((root, rels));
    }
    let at = root.display().to_string();
    let known = {
        let mut conn = app.db.acquire().await?;
        match kelve_store::reader::root_of(&mut conn, &at).await? {
            Some(known) => Some(known),
            None => kelve_store::moments::touched_root_of(&mut conn, &at).await?,
        }
    };
    let Some(known) = known.map(PathBuf::from) else {
        return Ok((root, rels));
    };
    let Ok(under) = root.strip_prefix(&known) else {
        return Ok((root, rels));
    };
    let under: Vec<String> = under
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let rels = rels
        .into_iter()
        .map(|r| {
            under
                .iter()
                .cloned()
                .chain([r])
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    Ok((known, rels))
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NextIn {
    #[serde(flatten)]
    pub caller: Caller,
    /// The caller can say kelve is still preparing the next part
    /// (`Next::Preparing`). An older `kelve mcp`, which can't, is given
    /// none then, as before.
    #[serde(default)]
    pub says_preparing: bool,
}

/// The next job, or, for one thing opened, that kelve is still preparing
/// its next part: the agent asks again.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Next {
    Job(Box<Job>),
    Preparing { preparing: bool },
}

/// The next job for the agent, or none. Asked for one thing opened, by its
/// number, kelve waits a while for its next part, while it's prepared.
pub async fn next(
    State(app): State<Shared>,
    _agent: Agent,
    Json(asked): Json<NextIn>,
) -> ApiResult<Json<Option<Next>>> {
    let wait = match asked.caller.only {
        Some(_) => PREPARING,
        None => std::time::Duration::ZERO,
    };
    let (job, preparing) =
        agent::next_waiting(&app, &asked.caller, Default::default(), wait).await?;
    Ok(Json(match job {
        Some(job) => Some(Next::Job(Box::new(job))),
        None if preparing && asked.says_preparing => Some(Next::Preparing { preparing }),
        None => None,
    }))
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
    /// No next job came in time, but kelve is still preparing the next part
    /// of what the agent asked for by its number: it asks again.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub preparing: bool,
}

/// The most kelve waits, as an answer comes in, for work running then to
/// ask for something more.
const LINGER: std::time::Duration = std::time::Duration::from_secs(5);

/// The most kelve waits for the next part of one thing opened, asked for by
/// its number, while its own work runs: a debrief's next job comes a little
/// after the answer before it. Inside the two minutes the command line waits for a reply
/// (`kelve mcp`), with `SPEND` before it.
const PREPARING: std::time::Duration = std::time::Duration::from_secs(45);

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
    let (next, preparing) = match handed_in {
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
                    preparing: false,
                }));
            }
            // Work running in the background as the answer came may ask
            // for something in a moment: wait for it, rather than telling the
            // agent there's nothing more, so it stops. A little, for any
            // job; for one thing's, asked for by its number, while that
            // thing's next part is prepared.
            let wait = match given.caller.only {
                Some(_) => PREPARING,
                None => LINGER,
            };
            agent::next_waiting(&app, &given.caller, after, wait).await?
        }
        HandedIn::No { .. } => (None, false),
    };
    Ok(Json(Answered {
        handed_in,
        next,
        left: None,
        preparing,
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
    // Setup may have linked the person's skills into an agent's folder too.
    crate::agent_loop::links::sync_setting(&app).await;
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
