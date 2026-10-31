//! kelve's background work, as jobs in the local store (`store::jobs`):
//! marking what's open (`mark`), tracing ideas down (`relations`), the
//! entries ideas lead to (`entries`), and keeping the store tidy
//! (docs/architecture.md). Where the work needs a model, it asks the
//! person's agent (`agent`) and stops; the agent's answer runs it again.
//! Two workers share the jobs; a job that fails is tried again later, with
//! a growing wait.

pub mod entries;
pub mod mark;
pub mod relations;
pub mod splits;

use std::panic::AssertUnwindSafe;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::FutureExt;
use kelve_store::jobs;
use serde_json::{Value, json};

use crate::state::Shared;

/// The hourly tidy: old sign-in links, jobs that gave up, and the agent's
/// answers once they're taken up.
pub const SWEEP: &str = "sweep";

const WORKERS: usize = 2;

/// Why a job didn't finish. It's tried again later.
#[derive(Debug, thiserror::Error)]
pub enum Failed {
    #[error(transparent)]
    Store(#[from] kelve_store::Error),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    /// What was asked about isn't there: a 404 to the web app.
    #[error("{0}")]
    Missing(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Failed>;

/// What a job asks for once it's run: nothing more, or to run again then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Done,
    Again(DateTime<Utc>),
}

/// Starts the workers, once what's scheduled is: they stop when kelve does.
pub fn start(app: &Shared) {
    let app = app.clone();
    tokio::spawn(async move {
        schedule(&app).await;
        for _ in 0..WORKERS {
            let app = app.clone();
            tokio::spawn(async move { work(&app).await });
        }
    });
}

/// The jobs that come round on their own, added once.
async fn schedule(app: &Shared) {
    let now = Utc::now();
    let mut conn = match app.db.acquire().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "couldn't reach the database to schedule kelve's work");
            return;
        }
    };
    // What the last kelve was running when it stopped runs again now,
    // rather than when its lease runs out.
    if let Err(e) = jobs::release_all(&mut conn).await {
        tracing::warn!(error = %e, "couldn't let go of the jobs kelve was running when it stopped");
    }
    for (kind, at) in [(SWEEP, now)] {
        if let Err(e) = jobs::add_once(&mut conn, kind, kind, &json!({}), at).await {
            tracing::warn!(error = %e, job = kind, "couldn't schedule a job");
        }
    }
}

async fn work(app: &Shared) {
    loop {
        if app.is_stopping() {
            return;
        }
        match run_once(app).await {
            Ok(Some(_)) => {}
            Ok(None) => idle(app).await,
            Err(e) => {
                tracing::warn!(error = %e, "couldn't take a job");
                wait(app, Duration::from_secs(10)).await;
            }
        }
    }
}

/// Runs the next job that's due, if there is one, and says which kind it
/// was. What the workers do, one job at a time; tests drive it directly.
pub async fn run_once(app: &Shared) -> Result<Option<String>> {
    let job = {
        let mut conn = app.db.acquire().await?;
        jobs::claim(&mut conn).await?
    };
    let Some(job) = job else {
        return Ok(None);
    };
    let kind = job.kind.clone();
    // It runs as a task of its own, so it's finished and recorded even when
    // whatever ran it stops waiting: an agent's answer, whose caller hung
    // up. Left part way, it would stay taken until its lease ran out.
    let app = app.clone();
    tokio::spawn(async move { run(&app, job).await })
        .await
        .map_err(|e| Failed::Other(format!("a job's task ended part way: {e}")))?;
    Ok(Some(kind))
}

/// Waits for the next job that's due, a new one, or kelve stopping.
async fn idle(app: &Shared) {
    let next = match app.db.acquire().await {
        Ok(mut conn) => jobs::next_due(&mut conn).await.ok().flatten(),
        Err(_) => None,
    };
    let until = next
        .map(|at| (at - Utc::now()).to_std().unwrap_or(Duration::ZERO))
        .unwrap_or(Duration::from_secs(60))
        .min(Duration::from_secs(60))
        .max(Duration::from_millis(200));
    wait(app, until).await;
}

async fn wait(app: &Shared, d: Duration) {
    tokio::select! {
        () = tokio::time::sleep(d) => {}
        () = app.wake.notified() => {}
        () = app.stopped() => {}
    }
}

async fn run(app: &Shared, job: jobs::Job) {
    let _working = app.working();
    let started = std::time::Instant::now();
    // A job that panics is let go, to be tried again later, rather than
    // left taken; the panic is kept as a problem report (`problems`).
    let result = AssertUnwindSafe(dispatch(app, &job))
        .catch_unwind()
        .await
        .unwrap_or_else(|_| Err(Failed::Other("It stopped part way.".into())));
    let Ok(mut conn) = app.db.acquire().await else {
        // The lease runs out and the job is taken again.
        return;
    };
    // Claiming a job counts as a try, so more than one means it failed before.
    let changed = !matches!(result, Ok(Then::Done)) || job.attempts > 1;
    let saved = match result {
        Ok(Then::Done) => jobs::done(&mut conn, job.id).await,
        Ok(Then::Again(at)) => jobs::later(&mut conn, job.id, at).await,
        Err(e) => {
            tracing::warn!(job = %job.kind, attempts = job.attempts, error = %e, "a job failed; it'll be tried again");
            jobs::failed(&mut conn, job.id, &e.to_string()).await
        }
    };
    if let Err(e) = saved {
        tracing::warn!(job = %job.kind, error = %e, "couldn't record how a job went");
    }
    // How reading a folder stands shows as it changes: failing, or going
    // again after. Not each time it runs: the reader asks for it as it
    // looks, and hearing that it ran would ask again.
    if job.kind == crate::reader::READ_FOLDER
        && changed
        && let Some(opened) = job.payload["opened"].as_i64()
    {
        app.publish(kelve_core::views::ServerEvent::Read { opened });
    }
    tracing::debug!(job = %job.kind, ms = started.elapsed().as_millis() as u64, "job ran");
}

async fn dispatch(app: &Shared, job: &jobs::Job) -> Result<Then> {
    let idea = job.payload["idea"].as_str().unwrap_or_default();
    match job.kind.as_str() {
        SWEEP => sweep(app).await,
        entries::WRITE => entries::write(app, idea).await,
        entries::SEND => entries::send(app, idea).await,
        entries::FOLLOW => entries::follow(app, idea).await,
        entries::FIX => entries::fix(app, idea).await,
        entries::REVISE => entries::revise(app, idea).await,
        entries::REFRESH => entries::refresh(app, idea).await,
        relations::RELATE => {
            relations::propose(
                app,
                idea,
                job.payload["depth"].as_u64().unwrap_or(0),
                job.payload["below"].as_bool().unwrap_or(false),
            )
            .await
        }
        crate::reader::READ_FILE => crate::reader::read_job(app, &job.payload, false).await,
        crate::reader::READ_DIFF => crate::reader::read_job(app, &job.payload, true).await,
        crate::reader::READ_FOLDER => crate::reader::read_folder_job(app, &job.payload).await,
        crate::reader::BUILT_ON => crate::reader::built_on_job(app, &job.payload).await,
        crate::plainly::JOB => crate::plainly::job(app, &job.payload).await,
        crate::plainly::FOLDER_JOB => crate::plainly::folder_job(app, &job.payload).await,
        crate::plainly::CHANGE_JOB => crate::plainly::change_job(app, &job.payload).await,
        crate::way::FOLDERS => crate::way::folders_job(app, &job.payload).await,
        crate::way::FIND => crate::way::find_job(app, &job.payload).await,
        crate::way::REST => crate::way::rest_job(app, &job.payload).await,
        crate::list::NOTE_DONE => crate::list::note_done_job(app, &job.payload).await,
        crate::lessons::WRITE => crate::lessons::write_job(app, &job.payload).await,
        crate::lessons::SEND => crate::lessons::send_job(app, &job.payload).await,
        other => {
            tracing::warn!(job = other, "a job of a kind kelve doesn't know; dropped");
            Ok(Then::Done)
        }
    }
}

/// Adds a job and wakes a worker.
pub async fn add(
    app: &Shared,
    kind: &str,
    key: Option<&str>,
    payload: Value,
    at: DateTime<Utc>,
) -> Result<()> {
    let mut conn = app.db.acquire().await?;
    jobs::add(&mut conn, kind, key, &payload, at).await?;
    app.wake.notify_one();
    Ok(())
}

/// Asks for a job by a time at the latest, under a key: asking again
/// never puts it off.
pub async fn add_by(app: &Shared, kind: &str, key: &str, at: DateTime<Utc>) -> Result<()> {
    let mut conn = app.db.acquire().await?;
    jobs::add_by(&mut conn, kind, key, &json!({}), at).await?;
    app.wake.notify_one();
    Ok(())
}

/// What an entry says while it waits to be sent, the account having added
/// as much as the shared graph takes a day.
pub const WAITING_FOR_LIMIT: &str = "Waiting to go to the shared graph until today’s limit resets.";

/// Sends a changeset to the shared graph as the person: entries, ideas
/// and how they relate, which the graph's day's limits count. Once the
/// graph says the account has added as much as it may today, nothing more
/// is sent until its limits reset, at midnight UTC, even after a restart:
/// each send is told so at once, without asking the graph.
pub async fn apply(
    app: &Shared,
    graph: &kelve_graph_api::client::Client,
    changes: &kelve_graph_api::Changeset,
    key: &str,
) -> std::result::Result<kelve_graph_api::Applied, kelve_graph_api::client::Error> {
    use kelve_graph_api::client::{Error, Limit, Quota};
    if let Some(held) = app.sends_held() {
        let left = held.until - kelve_store::now();
        return Err(Error::Enough(Limit {
            what: match held.what.as_str() {
                "ideas" => Some(Quota::Ideas),
                "entries" => Some(Quota::Entries),
                "relations" => Some(Quota::Relations),
                "lessons" => Some(Quota::Lessons),
                _ => None,
            },
            new_account: held.new_account,
            after: Duration::from_millis(u64::try_from(left).unwrap_or(0)),
        }));
    }
    let applied = graph.apply(changes, key).await;
    if let Err(Error::Enough(limit)) = &applied {
        let what = match limit.what {
            Some(Quota::Ideas) => "ideas",
            Some(Quota::Entries) => "entries",
            Some(Quota::Relations) => "relations",
            Some(Quota::Lessons) => "lessons",
            None => "",
        };
        let after = i64::try_from(limit.after.as_millis()).unwrap_or(i64::MAX);
        let held = kelve_store::sends::Held {
            until: kelve_store::now().saturating_add(after),
            what: what.into(),
            new_account: limit.new_account,
        };
        tracing::info!(
            what,
            minutes = after / 60_000,
            "the account added as much as the shared graph takes today; sending waits until it resets"
        );
        if let Err(e) = app.hold_sends(Some(held)).await {
            tracing::warn!(error = %e.message, "couldn't keep until when sending to the shared graph waits");
        }
    }
    applied
}

async fn sweep(app: &Shared) -> Result<Then> {
    let mut conn = app.db.acquire().await?;
    kelve_store::auth::sweep(&mut conn).await?;
    kelve_store::jobs::sweep(&mut conn).await?;
    kelve_store::asks::sweep(&mut conn).await?;
    kelve_store::sends::sweep(&mut conn).await?;
    kelve_store::lessons::sweep(&mut conn).await?;
    drop(conn);
    // What was kept only until it could go, such as entries written before
    // the person agreed, goes once it can.
    match entries::send_held(app).await {
        Ok(n) if n > 0 => tracing::info!(entries = n, "sending what was kept until it could go"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "couldn't send what was kept until it could go"),
    }
    Ok(Then::Again(Utc::now() + chrono::Duration::hours(1)))
}
