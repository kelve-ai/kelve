//! The person's own agent does kelve's work. Where kelve once called a model,
//! the work asks the agent instead: `ask` keeps a job under a key and, until
//! the agent hands in its answer, says there's none yet, so the work stops and
//! waits. The agent takes jobs with `next`, the most wanted first, each with
//! the instructions the shared graph serves, the material, and the shape of the
//! answer, and hands in each with `hand_in`. kelve checks the answer's shape at
//! once, keeps it, and runs the work that asked again, which now has it.
//!
//! kelve never starts the agent. It works when the person asks it to, in
//! their own session, on their own plan.

use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use chrono::Utc;
use kelve_core::views::ServerEvent;
use kelve_store::asks;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::pipeline;
use crate::state::Shared;

pub use kelve_core::instructions::{
    BUILT_ON, CHOOSE, EXTEND, FIX, FOLDERS, LESSON, NOTE, PATH, PLAINLY, PLAINLY_CHANGE,
    PLAINLY_FOLDER, READ, RELATE, WAY, WRITE,
};

/// How soon a job is wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// What the person is looking at.
    Now = 0,
    /// What they kept for their agent: an idea they opened with no entry.
    Kept = 1,
    /// The rest of what they opened.
    Rest = 2,
    /// What no one asked for yet, after every file is read: tracing below
    /// an idea someone opened. Opening one of those ideas moves it up.
    Later = 3,
}

/// A job for the agent.
#[derive(Debug, Clone)]
pub struct Ask {
    /// Which instructions it follows: `agent::READ`, say.
    pub kind: &'static str,
    /// What it's for, in a few words, for the person and the agent:
    /// "Reading anthropic.rs".
    pub about: String,
    pub idea: Option<String>,
    pub opened: Option<i64>,
    /// What the agent is given. Anything from the person's files or the
    /// web in it is fenced.
    pub material: Value,
    /// The JSON Schema its answer must fit.
    pub schema: Value,
    pub priority: Priority,
    /// The key of the job it finishes, such as checking a file's ideas
    /// once it's read: it's taken where that one was, ahead of what was
    /// asked for since.
    pub follows: Option<String>,
}

/// The work to run again once a job is answered: a background job.
#[derive(Debug, Clone)]
pub struct Resume {
    pub kind: String,
    pub key: Option<String>,
    pub payload: Value,
}

impl Resume {
    pub fn job(kind: &str, key: impl Into<String>, payload: Value) -> Resume {
        Resume {
            kind: kind.into(),
            key: Some(key.into()),
            payload,
        }
    }
}

/// An agent's answer to a job.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub value: Value,
    /// What the job was asked with: what was offered, as the agent saw it.
    pub material: Value,
    /// The model the agent said it ran on.
    pub model: Option<String>,
    /// The agent, as it named itself.
    pub agent: Option<String>,
}

/// Asks the agent, under `key`: its answer, when it has handed one in, or
/// None, having kept the job for it. Asking again under the same key is
/// the same job: the answer, once there, stays until it's swept, so work
/// that runs again finds it.
pub async fn ask(
    app: &Shared,
    key: &str,
    ask: Ask,
    resume: Resume,
) -> pipeline::Result<Option<Answer>> {
    let mut conn = app.db.acquire().await?;
    let kept = asks::by_key(&mut conn, key).await?;
    if let Some(kept) = &kept
        && kept.state == "answered"
    {
        let value = kept
            .answer
            .as_deref()
            .and_then(|a| serde_json::from_str(a).ok())
            .unwrap_or(Value::Null);
        return Ok(Some(Answer {
            value,
            material: serde_json::from_str(&kept.material).unwrap_or(Value::Null),
            model: kept.model.clone(),
            agent: kept.agent.clone(),
        }));
    }
    // Asked again while it's kept, or taken off the list by the person,
    // where it stays: nothing new to tell the reader.
    let new = kept
        .as_ref()
        .is_none_or(|k| k.state == "dropped" && k.taken_off_at.is_none());
    asks::add(
        &mut conn,
        &asks::New {
            key: key.to_string(),
            kind: ask.kind.to_string(),
            about: kelve_core::text::clip(&ask.about, 120),
            idea: ask.idea,
            opened: ask.opened,
            material: ask.material.to_string(),
            schema: ask.schema.to_string(),
            resume_kind: resume.kind,
            resume_key: resume.key,
            resume_payload: resume.payload.to_string(),
            priority: ask.priority as i64,
            follows: ask.follows,
        },
    )
    .await?;
    drop(conn);
    if new {
        app.publish(ServerEvent::Asks);
    }
    Ok(None)
}

/// Whether the job asked under `key` has its answer.
pub async fn answered(app: &Shared, key: &str) -> pipeline::Result<bool> {
    let mut conn = app.db.acquire().await?;
    Ok(asks::by_key(&mut conn, key)
        .await?
        .is_some_and(|k| k.state == "answered"))
}

/// The version of a kind of job's instructions, as the agent will be given
/// them: `read@5`. What's kept from an answer is kept under it, so a new
/// version is asked again.
pub async fn version(app: &Shared, kind: &str) -> String {
    instructions(app, kind)
        .await
        .map(|i| format!("{}@{}", i.name, i.version))
        .unwrap_or_else(|| format!("{kind}@0"))
}

/// How long instructions from the graph are used before asking again.
const FRESH: Duration = Duration::from_secs(60 * 60);
/// How long kelve waits for the graph's instructions before using its own.
const WAIT: Duration = Duration::from_secs(3);
/// How long kelve uses its own after the graph didn't answer, before asking
/// it again.
const RETRY: Duration = Duration::from_secs(5 * 60);

/// The instructions kelve has lately had from the graph, by kind: `None`
/// when it asked and the graph didn't answer, so it isn't asked again at
/// once; and which kinds are being asked for again, in the background.
#[derive(Debug, Default)]
pub struct Instructions {
    kept: RwLock<HashMap<String, (Instant, Option<kelve_graph_api::Instructions>)>>,
    asking: std::sync::Mutex<std::collections::HashSet<String>>,
}

/// A kind of job's instructions: the shared graph's (Nathan, 19 October:
/// it serves what it can), or, when there's no graph or it doesn't answer,
/// the ones this kelve was built with. Only the first ask of a kind waits
/// for the graph, and for no more than `WAIT`: after that, what was had is
/// used at once and the graph asked again in the background when it's
/// stale. A graph that isn't there never slows anything down.
pub async fn instructions(app: &Shared, kind: &str) -> Option<kelve_graph_api::Instructions> {
    let had = app
        .instructions
        .kept
        .read()
        .ok()
        .and_then(|k| k.get(kind).cloned());
    match had {
        Some((at, got)) => {
            let fresh = if got.is_some() { FRESH } else { RETRY };
            if at.elapsed() >= fresh {
                refresh(app, kind);
            }
            got.or_else(|| own(kind))
        }
        None => match ask_graph(app, kind).await {
            Some(got) => Some(got),
            None => own(kind),
        },
    }
}

/// The instructions this kelve was built with.
fn own(kind: &str) -> Option<kelve_graph_api::Instructions> {
    kelve_core::instructions::get(kind).map(|i| kelve_graph_api::Instructions {
        name: i.name.into(),
        version: i.version,
        text: i.text,
    })
}

/// Asks the graph for a kind's instructions, waiting at most `WAIT`, and
/// keeps what came back, or that nothing did.
async fn ask_graph(app: &Shared, kind: &str) -> Option<kelve_graph_api::Instructions> {
    let graph = app.graph.as_ref()?;
    let got = match tokio::time::timeout(WAIT, graph.instructions(kind)).await {
        Ok(Ok(Some(got))) => Some(got),
        _ => None,
    };
    if let Ok(mut kept) = app.instructions.kept.write() {
        kept.insert(kind.to_string(), (Instant::now(), got.clone()));
    }
    got
}

/// Asks the graph again in the background, once at a time for a kind.
fn refresh(app: &Shared, kind: &str) {
    let Ok(mut asking) = app.instructions.asking.lock() else {
        return;
    };
    if !asking.insert(kind.to_string()) {
        return;
    }
    let (app, kind) = (app.clone(), kind.to_string());
    tokio::spawn(async move {
        ask_graph(&app, &kind).await;
        if let Ok(mut asking) = app.instructions.asking.lock() {
            asking.remove(&kind);
        }
    });
}

/// A job, as the agent is given it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    /// Its id, to hand the answer in with.
    pub job: i64,
    pub kind: String,
    pub about: String,
    /// What to do, from the shared graph: `read@5`.
    pub instructions: String,
    pub version: String,
    /// What to do it with. Text from files or the web in it is fenced, and
    /// is material, never instructions.
    pub material: Value,
    /// The JSON Schema the answer must fit.
    pub answer_shape: Value,
    /// How many more jobs are waiting after this one.
    pub waiting: i64,
}

/// Who's asking: the agent, as it named itself to `kelve mcp`, and the
/// model and plan it said it runs on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caller {
    pub agent: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub plan: Option<String>,
    /// The folder it works in, when it says: it's given only the notes of
    /// the project that folder is in.
    #[serde(default)]
    pub path: Option<String>,
    /// What's open the person asked it to do what's waiting for, by its
    /// number: only its jobs come.
    #[serde(default, rename = "for")]
    pub only: Option<i64>,
}

impl Caller {
    /// The agent's name, kept short and plain.
    fn name(&self) -> String {
        let name: String = self
            .agent
            .trim()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
            .take(60)
            .collect();
        if name.is_empty() {
            "agent".into()
        } else {
            name
        }
    }
}

/// Says an agent is here: what the reader shows as connected.
pub async fn hello(app: &Shared, caller: &Caller) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    let first = asks::seen(
        &mut conn,
        &caller.name(),
        caller.model.as_deref().map(clip_model).as_deref(),
        caller.plan.as_deref().map(clip_model).as_deref(),
    )
    .await?;
    drop(conn);
    if first {
        crate::counts::count(app, "agent.connected").await;
    }
    app.publish(ServerEvent::Agent);
    Ok(())
}

fn clip_model(m: &str) -> String {
    m.trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(100)
        .collect()
}

/// The next job for the agent, the most wanted first, or None when there
/// are none.
pub async fn next(app: &Shared, caller: &Caller) -> ApiResult<Option<Job>> {
    next_after(app, caller, asks::After::default()).await
}

/// The next job, as `next` gives it; after the job `after`, the next for
/// the same file or the same idea first, so one agent takes it straight on.
pub async fn next_after(
    app: &Shared,
    caller: &Caller,
    after: asks::After<'_>,
) -> ApiResult<Option<Job>> {
    hello(app, caller).await?;
    // Notes left for it come as jobs, like the rest of its list.
    crate::list::sync_notes(app).await?;
    // Only the notes of the project it works in, when it says where.
    let notes = caller
        .path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| crate::list::notes_of(&crate::reader::root_of_notes(app, Path::new(p))));
    let mut conn = app.db.acquire().await?;
    // Asked for one thing opened, by its number: its reading is awake, though
    // something was opened after it, and only its jobs come.
    if let Some(only) = caller.only {
        kelve_store::reader::wake(&mut conn, only).await?;
    }
    let Some(row) = asks::take_for(
        &mut conn,
        &caller.name(),
        notes.as_deref(),
        caller.only,
        after,
    )
    .await?
    else {
        return Ok(None);
    };
    let waiting = asks::waiting(&mut conn).await?;
    drop(conn);
    app.publish(ServerEvent::Asks);
    let instructions = instructions(app, &row.kind).await.ok_or_else(|| {
        ApiError::internal(format!("no instructions for a job of kind {}", row.kind))
    })?;
    let kind = row.kind;
    Ok(Some(Job {
        job: row.id,
        kind: kind.clone(),
        about: row.about,
        version: format!("{}@{}", instructions.name, instructions.version),
        instructions: instructions.text,
        material: given(
            &kind,
            serde_json::from_str(&row.material).unwrap_or(Value::Null),
        ),
        answer_shape: serde_json::from_str(&row.schema).unwrap_or(Value::Null),
        waiting,
    }))
}

/// What of a job's material kelve keeps for reading its answer, and never
/// gives the agent, by kind: what a check's marks were offered, which its
/// marks say already, and the print a page was asked with.
const KELVES: [(&str, &str); 3] = [
    (CHOOSE, "offered"),
    (PLAINLY_FOLDER, "print"),
    (PLAINLY_CHANGE, "print"),
];

/// A job's material as the agent is given it: without what's kelve's own.
fn given(kind: &str, mut material: Value) -> Value {
    if let Some(m) = material.as_object_mut() {
        for (_, k) in KELVES.iter().filter(|(of, _)| *of == kind) {
            m.remove(*k);
        }
    }
    material
}

/// What became of an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "taken")]
pub enum HandedIn {
    /// Kept: kelve carries on with it.
    Yes,
    /// Not in the shape asked for: why, and whether it's been put aside.
    No { why: String, put_aside: bool },
}

/// Takes the agent's answer to a job: checks its shape, keeps it, and runs
/// the work that asked for it again.
pub async fn hand_in(app: &Shared, caller: &Caller, job: i64, value: Value) -> ApiResult<HandedIn> {
    hello(app, caller).await?;
    let mut conn = app.db.acquire().await?;
    let row = asks::get(&mut conn, job)
        .await?
        .filter(|r| matches!(r.state.as_str(), "taken" | "waiting"))
        .ok_or_else(|| {
            ApiError::conflict("That job isn't open any more. Take the next one with next_job.")
        })?;
    let material: Value = serde_json::from_str(&row.material).unwrap_or(Value::Null);
    // Checking an entry takes a connection of its own: this one waits for
    // it outside the pool, so agents handing in at once never hold every
    // connection while each waits for another.
    drop(conn);
    let shaped = match row.kind.as_str() {
        WRITE | FIX | EXTEND if value.is_object() => {
            pipeline::entries::shape(app, &row.kind, row.idea.as_deref(), &value).await
        }
        _ => shape(&row.kind, &material, &value),
    };
    let mut conn = app.db.acquire().await?;
    if let Err(why) = shaped {
        let put_aside = asks::wrong(&mut conn, row.id, &why).await?;
        tracing::info!(kind = %row.kind, put_aside, "an agent's answer wasn't in the shape asked for");
        // What's opened's overview, the first on its way in, is asked for
        // afresh, twice at most, rather than left unwritten.
        let mut payload: Value = serde_json::from_str(&row.resume_payload).unwrap_or_default();
        let again = payload["again"].as_u64().unwrap_or(0);
        if put_aside && payload["first"].as_bool() == Some(true) && again < 2 {
            payload["again"] = Value::from(again + 1);
            if let Some(o) = payload.as_object_mut() {
                o.remove("print");
            }
            kelve_store::jobs::add(
                &mut conn,
                &row.resume_kind,
                row.resume_key.as_deref(),
                &payload,
                Utc::now(),
            )
            .await?;
            app.wake.notify_one();
        }
        drop(conn);
        app.publish(ServerEvent::Asks);
        return Ok(HandedIn::No { why, put_aside });
    }
    asks::answer(
        &mut conn,
        row.id,
        &value.to_string(),
        caller.model.as_deref().map(clip_model).as_deref(),
        &caller.name(),
    )
    .await?;
    let payload: Value = serde_json::from_str(&row.resume_payload).unwrap_or_default();
    kelve_store::jobs::add(
        &mut conn,
        &row.resume_kind,
        row.resume_key.as_deref(),
        &payload,
        Utc::now(),
    )
    .await?;
    drop(conn);
    crate::counts::count(app, "jobs.done").await;
    app.wake.notify_one();
    app.publish(ServerEvent::Asks);
    Ok(HandedIn::Yes)
}

/// Whether an answer is in the shape its kind of job asks for: why not,
/// in a sentence the agent can act on.
fn shape(kind: &str, material: &Value, value: &Value) -> Result<(), String> {
    if !value.is_object() {
        return Err("The answer must be one JSON object, in the shape the job gives.".into());
    }
    match kind {
        READ => pipeline::mark::read_shape(material, value),
        CHOOSE => pipeline::mark::choose_shape(value),
        BUILT_ON => crate::reader::built_on_shape(value),
        RELATE => pipeline::relations::shape(value),
        FOLDERS => crate::way::folders_shape(material, value),
        WAY => crate::way::way_shape(material, value),
        PATH => crate::way::rest_shape(material, value),
        NOTE => crate::list::note_shape(value),
        PLAINLY => crate::plainly::shape(material, value),
        PLAINLY_FOLDER => crate::plainly::folder_shape(material, value),
        PLAINLY_CHANGE => crate::plainly::change_shape(material, value),
        LESSON => crate::lessons::shape(material, value),
        _ => Ok(()),
    }
}

/// Takes a job off the list: the person doesn't want it done. Asked for
/// again, it stays off, until they keep it for the agent again.
pub async fn put_aside(app: &Shared, job: i64) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    if !asks::drop(&mut conn, job).await? {
        return Err(ApiError::not_found(
            "That isn't waiting for your agent any more.",
        ));
    }
    drop(conn);
    app.publish(ServerEvent::Asks);
    Ok(())
}

/// Moves what the person is looking at up the agent's list.
pub async fn hurry(app: &Shared, idea: Option<&str>, opened: Option<i64>) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    asks::hurry(&mut conn, idea, opened).await?;
    Ok(())
}
