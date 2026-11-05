//! kelve's background work, as jobs in the local store (`store::jobs`):
//! the agent loop's debriefs, research, explanations and learning, skill
//! checks, and keeping the store tidy (docs/architecture.md).
//! Where the work needs a model, it asks the person's agent (`agent`) and
//! stops; the agent's answer runs it again. Two workers share the jobs; a
//! job that fails is tried again later, with a growing wait.

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
    run_claimed(app, job).await
}

/// Runs the job with this key, if it's due and no one has it: the work an
/// agent's answer was for, and nothing else that's due.
pub async fn run_key(app: &Shared, key: &str) -> Result<Option<String>> {
    let job = {
        let mut conn = app.db.acquire().await?;
        jobs::claim_key(&mut conn, key).await?
    };
    run_claimed(app, job).await
}

async fn run_claimed(app: &Shared, job: Option<jobs::Job>) -> Result<Option<String>> {
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
    tracing::debug!(job = %job.kind, ms = started.elapsed().as_millis() as u64, "job ran");
}

async fn dispatch(app: &Shared, job: &jobs::Job) -> Result<Then> {
    match job.kind.as_str() {
        SWEEP => sweep(app).await,
        crate::list::REDO_DONE => Ok(Then::Done),
        crate::v2jobs::DEBRIEF => crate::agent_loop::debrief::job(app, &job.payload).await,
        crate::v2jobs::RESEARCH => crate::agent_loop::research::job(app, &job.payload).await,
        crate::v2jobs::EXPLAIN => crate::agent_loop::explain::job(app, &job.payload).await,
        crate::v2jobs::LEARN => crate::agent_loop::learn::job(app, job).await,
        crate::v2jobs::CHECK => crate::skill_checks::check_job(app, &job.payload).await,
        crate::skill_checks::FITS => crate::skill_checks::fits_job(app, &job.payload).await,
        crate::v2jobs::DRAW => crate::your_agent::job(app, &job.payload).await,
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

async fn sweep(app: &Shared) -> Result<Then> {
    let mut conn = app.db.acquire().await?;
    kelve_store::auth::sweep(&mut conn).await?;
    kelve_store::jobs::sweep(&mut conn).await?;
    kelve_store::asks::sweep(&mut conn).await?;
    kelve_store::moments::sweep(&mut conn).await?;
    // What the person said to their agent is kept a week, and what it
    // handed over a month, unless a debrief or a past task is of it.
    const DAY: i64 = 24 * 60 * 60 * 1000;
    kelve_store::handovers::sweep_messages(&mut conn, kelve_store::now() - 7 * DAY).await?;
    kelve_store::handovers::sweep(&mut conn, kelve_store::now() - 30 * DAY).await?;
    // Which skills a session had: sessions end within a day or two.
    kelve_store::agent_loop::sweep_given(&mut conn, kelve_store::now() - 2 * DAY).await?;
    Ok(Then::Again(Utc::now() + chrono::Duration::hours(1)))
}
