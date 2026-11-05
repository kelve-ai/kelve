//! The person's own agent does kelve's work. Where kelve would call a model,
//! the work asks the agent instead: `ask` keeps a job under a key and, until
//! the agent hands in its answer, says there's none yet, so the work stops and
//! waits. The agent takes jobs with `next`, the most wanted first, each with
//! the instructions this kelve was built with, the material, and the shape of
//! the answer, and hands in each with `hand_in`. kelve checks the answer's shape at
//! once, keeps it, and runs the work that asked again, which now has it.
//!
//! kelve never starts the agent. It works when the person asks it to, in
//! their own session, on their own plan.

use std::path::Path;
use std::time::{Duration, Instant};

use chrono::Utc;
use kelve_core::views::ServerEvent;
use kelve_store::asks;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ApiError, ApiResult};
use crate::pipeline;
use crate::state::Shared;

pub use kelve_core::instructions::{DEBRIEF, EXPLAIN, LEARN, REDO, RESEARCH};

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
/// them: `debrief@1`. What's kept from an answer is kept under it, so a new
/// version is asked again.
pub fn version(kind: &str) -> String {
    instructions(kind)
        .map(|i| i.id())
        .unwrap_or_else(|| format!("{kind}@0"))
}

/// A kind of job's instructions: the ones this kelve was built with
/// (`core::instructions`).
pub fn instructions(kind: &str) -> Option<kelve_core::instructions::Instructions> {
    kelve_core::instructions::get(kind)
}

/// A job, as the agent is given it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    /// Its id, to hand the answer in with.
    pub job: i64,
    pub kind: String,
    pub about: String,
    /// What to do: the instructions, in full.
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
    /// The folder it works in, when it says: it's given only what's to redo
    /// in the project that folder is in.
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
    // Only what's to redo in the project it works in, when it says where.
    let redos = match caller.path.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => Some(crate::list::redos_in(app, Path::new(p)).await?),
        _ => None,
    };
    let mut conn = app.db.acquire().await?;
    let Some(row) = asks::take_for(
        &mut conn,
        &caller.name(),
        redos.as_deref(),
        caller.only,
        after,
    )
    .await?
    else {
        return Ok(None);
    };
    drop(conn);
    Ok(Some(job_of(app, row).await?))
}

/// A job taken, as the agent is given it.
async fn job_of(app: &Shared, row: asks::Ask) -> ApiResult<Job> {
    let waiting = {
        let mut conn = app.db.acquire().await?;
        asks::waiting(&mut conn).await?
    };
    app.publish(ServerEvent::Asks);
    crate::agent_loop::taken(app, &row).await;
    let instructions = instructions(&row.kind).ok_or_else(|| {
        ApiError::internal(format!("no instructions for a job of kind {}", row.kind))
    })?;
    let kind = row.kind;
    Ok(Job {
        job: row.id,
        kind: kind.clone(),
        about: row.about,
        version: instructions.id(),
        instructions: instructions.text,
        material: given(
            &kind,
            serde_json::from_str(&row.material).unwrap_or(Value::Null),
        ),
        answer_shape: serde_json::from_str(&row.schema).unwrap_or(Value::Null),
        waiting,
    })
}

/// The next job, as `next_after` gives it. When none is ready, kelve waits
/// at most `wait` while work that may ask for one runs or is due, running
/// what's due meanwhile, a job at a time, so what it asks comes now: for
/// one thing opened, asked for by its number, that thing's own work; else
/// any. And whether that thing's next part is still being prepared, when
/// none came in time: its own work still runs, or is due. When nothing of
/// its is, it's done, or stuck, and the agent is told there's no more.
pub async fn next_waiting(
    app: &Shared,
    caller: &Caller,
    after: asks::After<'_>,
    wait: Duration,
) -> ApiResult<(Option<Job>, bool)> {
    let mut next = next_after(app, caller, after).await?;
    let until = Instant::now() + wait;
    while next.is_none() && Instant::now() < until {
        if !busy(app, caller.only).await? {
            break;
        }
        // What's due runs here while there's time, so what it asks comes
        // now, in its order; with nothing due, what the workers are running
        // is waited for.
        let mut ran = false;
        while Instant::now() < until
            && pipeline::run_once(app)
                .await
                .map_err(ApiError::internal)?
                .is_some()
        {
            ran = true;
        }
        if !ran {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        next = next_after(app, caller, after).await?;
    }
    let preparing = match (&next, caller.only) {
        (None, Some(only)) => busy(app, Some(only)).await?,
        _ => false,
    };
    Ok((next, preparing))
}

/// Whether work that may ask the agent for something runs now, or is due:
/// one thing opened's own, by its number, or any.
async fn busy(app: &Shared, only: Option<i64>) -> ApiResult<bool> {
    let mut conn = app.db.acquire().await?;
    Ok(match only {
        Some(id) => kelve_store::jobs::busy_for(&mut conn, id).await?,
        None => kelve_store::jobs::busy(&mut conn).await?,
    })
}

/// What of a job's material kelve keeps for reading its answer, and never
/// gives the agent, by kind: what a debrief's change was, and what a
/// learning job was given and its evidence.
const KELVES: [(&str, &str); 3] = [(DEBRIEF, "changed"), (LEARN, "given"), (LEARN, "evidence")];

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
    let shaped = shape(&row.kind, &material, &value);
    if let Err(why) = shaped {
        let put_aside = asks::wrong(&mut conn, row.id, &why).await?;
        tracing::info!(kind = %row.kind, put_aside, "an agent's answer wasn't in the shape asked for");
        crate::agent_loop::wrong(app, &row, &why, put_aside).await;
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
        REDO => crate::list::redo_shape(value),
        DEBRIEF => crate::agent_loop::debrief::shape(material, value),
        RESEARCH => crate::agent_loop::research::shape(value),
        EXPLAIN => crate::agent_loop::explain::shape(value),
        LEARN => crate::agent_loop::learn::shape(material, value),
        kelve_core::instructions::COMPARE => crate::skill_checks::shape(material, value),
        kelve_core::instructions::DRAW => crate::your_agent::shape(value),
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
