//! Background work: one row a job, claimed in one statement so workers
//! never take the same one, retried with a growing wait, and debounced by
//! key: one waiting job per key, its time moved on by each new request.

use chrono::{DateTime, Utc};

use crate::{Conn, Result, ms, now};

/// How long a claimed job may run before another worker may take it.
const LEASE_MS: i64 = 15 * 60 * 1000;

/// The most times a job is tried before it's given up.
pub const MAX_ATTEMPTS: i64 = 6;

/// How long a job that gave up is kept, for anyone looking.
const KEEP_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub key: Option<String>,
    pub payload: serde_json::Value,
    pub attempts: i64,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    kind: String,
    key: Option<String>,
    payload: String,
    attempts: i64,
}

impl From<Row> for Job {
    fn from(r: Row) -> Self {
        Job {
            id: r.id,
            kind: r.kind,
            key: r.key,
            payload: serde_json::from_str(&r.payload).unwrap_or(serde_json::Value::Null),
            attempts: r.attempts,
        }
    }
}

/// Adds a job to run at `run_at`. With a key, a job already waiting under
/// it is moved to the new time instead (and given the new payload), so a
/// burst of requests makes one job. One running now runs again once it's
/// done.
pub async fn add(
    conn: &mut Conn,
    kind: &str,
    key: Option<&str>,
    payload: &serde_json::Value,
    run_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "insert into jobs (kind, key, payload, run_at) values (?1, ?2, ?3, ?4)
         on conflict (key) do update set
           payload = excluded.payload,
           run_at = case when jobs.locked_at is null then excluded.run_at else jobs.run_at end,
           again_at = case when jobs.locked_at is null then null else excluded.run_at end,
           attempts = case when jobs.locked_at is null then 0 else jobs.attempts end,
           last_error = case when jobs.locked_at is null then null else jobs.last_error end",
    )
    .bind(kind)
    .bind(key)
    .bind(payload.to_string())
    .bind(ms(run_at))
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Adds a job with a key only if none is waiting under it: the earlier
/// time stands. One under it that gave up is asked for afresh, so work a
/// blip stopped, the network gone for a while, starts again when it's next
/// asked for.
pub async fn add_once(
    conn: &mut Conn,
    kind: &str,
    key: &str,
    payload: &serde_json::Value,
    run_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "insert into jobs (kind, key, payload, run_at) values (?1, ?2, ?3, ?4)
         on conflict (key) do update set
           payload = excluded.payload, run_at = excluded.run_at, attempts = 0, last_error = null
         where jobs.attempts >= ?5 and jobs.locked_at is null",
    )
    .bind(kind)
    .bind(key)
    .bind(payload.to_string())
    .bind(ms(run_at))
    .bind(MAX_ATTEMPTS)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Asks for a job by `run_at` at the latest: one already waiting under the
/// key keeps the earlier time, so asking often never puts it off. One
/// running now runs again once it's done.
pub async fn add_by(
    conn: &mut Conn,
    kind: &str,
    key: &str,
    payload: &serde_json::Value,
    run_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "insert into jobs (kind, key, payload, run_at) values (?1, ?2, ?3, ?4)
         on conflict (key) do update set
           run_at = case when jobs.locked_at is null then min(jobs.run_at, excluded.run_at)
                         else jobs.run_at end,
           again_at = case when jobs.locked_at is null then jobs.again_at
                           else min(coalesce(jobs.again_at, excluded.run_at), excluded.run_at) end",
    )
    .bind(kind)
    .bind(key)
    .bind(payload.to_string())
    .bind(ms(run_at))
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Claims the next job that's due, if any.
/// Claims one job by its key, if it's due and no one has it: the work an
/// agent's answer was for, run as the answer comes in.
pub async fn claim_key(conn: &mut Conn, key: &str) -> Result<Option<Job>> {
    let t = now();
    let row = sqlx::query_as::<_, Row>(
        "update jobs set locked_at = ?1, attempts = attempts + 1
         where key = ?4 and run_at <= ?1
           and (locked_at is null or locked_at < ?1 - ?2)
           and attempts < ?3
         returning id, kind, key, payload, attempts",
    )
    .bind(t)
    .bind(LEASE_MS)
    .bind(MAX_ATTEMPTS)
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(Job::from))
}

pub async fn claim(conn: &mut Conn) -> Result<Option<Job>> {
    let t = now();
    let row = sqlx::query_as::<_, Row>(
        "update jobs set locked_at = ?1, attempts = attempts + 1
         where id = (
           select id from jobs
           where run_at <= ?1
             and (locked_at is null or locked_at < ?1 - ?2)
             and attempts < ?3
           order by run_at
           limit 1)
         returning id, kind, key, payload, attempts",
    )
    .bind(t)
    .bind(LEASE_MS)
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(Job::from))
}

/// The job is done: it goes, unless it was asked for again while it ran.
pub async fn done(conn: &mut Conn, id: i64) -> Result<()> {
    // Deleted first: asked for again between the two, it's either gone,
    // and added afresh, or it's let go below. The other way round, it was
    // neither, and stayed taken until its lease ran out.
    sqlx::query("delete from jobs where id = ? and again_at is null and locked_at is not null")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "update jobs set run_at = again_at, again_at = null, locked_at = null, attempts = 0,
                         last_error = null
         where id = ? and again_at is not null",
    )
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The wait before trying again after `attempts` tries: e^attempts
/// seconds, at most about six hours.
fn backoff_ms(attempts: i64) -> i64 {
    let secs = (attempts.clamp(0, 10) as f64).exp();
    (secs * 1000.0) as i64
}

/// The job failed: tried again after a wait that grows with each attempt.
pub async fn failed(conn: &mut Conn, id: i64, error: &str) -> Result<()> {
    let attempts: Option<i64> = sqlx::query_scalar("select attempts from jobs where id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(attempts) = attempts else {
        return Ok(());
    };
    sqlx::query("update jobs set locked_at = null, last_error = ?, run_at = ? where id = ?")
        .bind(error)
        .bind(now() + backoff_ms(attempts))
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Puts a job off until later, without counting it as a try: for work that
/// waits on the day's spending, or goes on in rounds. It went well, so
/// what failed before is forgotten.
pub async fn later(conn: &mut Conn, id: i64, run_at: DateTime<Utc>) -> Result<()> {
    sqlx::query(
        "update jobs set locked_at = null, attempts = 0, last_error = null,
                         run_at = min(?1, coalesce(again_at, ?1)), again_at = null
         where id = ?2",
    )
    .bind(ms(run_at))
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Brings the waiting jobs of a kind whose keys start with `prefix`
/// forward to now: work that was waiting on something just done.
pub async fn due_now(conn: &mut Conn, kind: &str, prefix: &str) -> Result<()> {
    sqlx::query(
        "update jobs set run_at = ?1
         where kind = ?2 and locked_at is null and substr(key, 1, length(?3)) = ?3",
    )
    .bind(ms(Utc::now()))
    .bind(kind)
    .bind(prefix)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Brings the waiting job under one key forward to now, if there is one:
/// work that was waiting on something just done, and only that.
pub async fn due_now_at(conn: &mut Conn, key: &str) -> Result<()> {
    sqlx::query("update jobs set run_at = ?1 where key = ?2 and locked_at is null")
        .bind(ms(Utc::now()))
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Brings the jobs put off until exactly `until` (work that waited on the
/// day's spending, until the start of tomorrow) forward to now: how many.
/// Each checks the day's spending again as it runs.
pub async fn sooner(conn: &mut Conn, until: DateTime<Utc>) -> Result<u64> {
    let done = sqlx::query(
        "update jobs set run_at = ?1 where locked_at is null and run_at = ?2 and attempts < ?3",
    )
    .bind(now())
    .bind(ms(until))
    .bind(MAX_ATTEMPTS)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected())
}

/// When the next job is due, for a worker deciding how long to sleep.
pub async fn next_due(conn: &mut Conn) -> Result<Option<DateTime<Utc>>> {
    let t: Option<i64> =
        sqlx::query_scalar("select min(run_at) from jobs where locked_at is null and attempts < ?")
            .bind(MAX_ATTEMPTS)
            .fetch_one(&mut *conn)
            .await?;
    Ok(t.map(crate::at))
}

/// Lets go of every job a kelve that stopped was running, to run again now:
/// one kelve keeps a store, so at its start, nothing is running.
/// Whether any job is running now, or due and not yet taken: work that may
/// ask the agent for something in a moment.
pub async fn busy(conn: &mut Conn) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, i64>(
        "select count(*) from jobs where locked_at is not null or run_at <= ?",
    )
    .bind(crate::now())
    .fetch_one(&mut *conn)
    .await?
        > 0)
}

pub async fn release_all(conn: &mut Conn) -> Result<()> {
    sqlx::query(
        "update jobs set locked_at = null, run_at = min(run_at, ?), again_at = null,
                         attempts = max(attempts - 1, 0)
          where locked_at is not null",
    )
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The keys of the jobs still to run, or running, that start with `prefix`.
pub async fn due_like(conn: &mut Conn, prefix: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select key from jobs where substr(key, 1, length(?1)) = ?1 and attempts < ?2",
    )
    .bind(prefix)
    .bind(MAX_ATTEMPTS)
    .fetch_all(&mut *conn)
    .await?)
}

/// When the job under a key runs next: now or before for one that's due or
/// running, `None` when there's none or it gave up.
pub async fn due(conn: &mut Conn, key: &str) -> Result<Option<DateTime<Utc>>> {
    let t: Option<i64> = sqlx::query_scalar(
        "select case when locked_at is not null then min(run_at, ?2) else run_at end
         from jobs where key = ?1 and attempts < ?3",
    )
    .bind(key)
    .bind(now())
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(t.map(crate::at))
}

/// The payload of the job under a key, if there is one.
pub async fn payload(conn: &mut Conn, key: &str) -> Result<Option<serde_json::Value>> {
    let text: Option<String> = sqlx::query_scalar("select payload from jobs where key = ?")
        .bind(key)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(text.and_then(|t| serde_json::from_str(&t).ok()))
}

/// What went wrong when the job under a key last ran, if it failed.
pub async fn last_error(conn: &mut Conn, key: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar::<_, Option<String>>("select last_error from jobs where key = ?")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?
            .flatten(),
    )
}

/// Whether the job under a key failed when it last ran, and waits to try
/// again, or gave up.
pub async fn failing(conn: &mut Conn, key: &str) -> Result<bool> {
    Ok(sqlx::query_scalar::<_, i64>(
        "select count(*) from jobs where key = ? and last_error is not null",
    )
    .bind(key)
    .fetch_one(&mut *conn)
    .await?
        > 0)
}

/// Jobs that gave up: kept for a day for anyone looking, then swept.
pub async fn sweep(conn: &mut Conn) -> Result<()> {
    sqlx::query("delete from jobs where attempts >= ? and coalesce(locked_at, run_at) < ?")
        .bind(MAX_ATTEMPTS)
        .bind(now() - KEEP_MS)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;

    #[test]
    fn the_wait_grows_and_stops_growing() {
        assert_eq!(backoff_ms(0), 1000);
        assert!(backoff_ms(3) > backoff_ms(2));
        assert_eq!(backoff_ms(20), backoff_ms(10));
    }

    #[tokio::test]
    async fn only_the_job_under_a_key_is_brought_forward() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let later = Utc::now() + chrono::Duration::minutes(2);
        let json = serde_json::json!({});
        for key in ["folder:1:", "folder:1:src", "folder:1:src/api"] {
            add_once(&mut conn, "folder", key, &json, later)
                .await
                .expect("added");
        }
        due_now_at(&mut conn, "folder:1:src").await.expect("due");
        let job = claim(&mut conn).await.expect("claimed").expect("due now");
        assert_eq!(job.key.as_deref(), Some("folder:1:src"));
        assert!(claim(&mut conn).await.expect("claimed").is_none());
    }

    #[tokio::test]
    async fn a_job_that_gave_up_is_asked_for_afresh() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let now = Utc::now() - chrono::Duration::seconds(1);
        let json = serde_json::json!({});
        add_once(&mut conn, "read", "k", &json, now)
            .await
            .expect("added");
        for _ in 0..MAX_ATTEMPTS {
            let job = claim(&mut conn).await.expect("claimed").expect("due");
            assert!(failing(&mut conn, "k").await.expect("read") == (job.attempts > 1));
            failed(&mut conn, job.id, "couldn't reach the API")
                .await
                .expect("failed");
            sqlx::query("update jobs set run_at = ?")
                .bind(ms(now))
                .execute(&mut *conn)
                .await
                .expect("due");
        }
        assert!(
            claim(&mut conn).await.expect("claimed").is_none(),
            "it gave up"
        );
        assert!(failing(&mut conn, "k").await.expect("read"));
        // Asked for again, it starts afresh.
        add_once(&mut conn, "read", "k", &json, now)
            .await
            .expect("asked again");
        let job = claim(&mut conn).await.expect("claimed").expect("due again");
        assert_eq!(job.attempts, 1);
        assert!(!failing(&mut conn, "k").await.expect("read"));
        // A round that goes well forgets what failed before.
        failed(&mut conn, job.id, "couldn't reach the API")
            .await
            .expect("failed");
        sqlx::query("update jobs set run_at = ?")
            .bind(ms(now))
            .execute(&mut *conn)
            .await
            .expect("due");
        let job = claim(&mut conn).await.expect("claimed").expect("due");
        later(&mut conn, job.id, Utc::now()).await.expect("later");
        assert!(!failing(&mut conn, "k").await.expect("read"));
        // One that asks for itself again while it runs, going well, forgets
        // it too.
        sqlx::query("update jobs set run_at = ?")
            .bind(ms(now))
            .execute(&mut *conn)
            .await
            .expect("due");
        let job = claim(&mut conn).await.expect("claimed").expect("due");
        failed(&mut conn, job.id, "couldn't reach the API")
            .await
            .expect("failed");
        sqlx::query("update jobs set run_at = ?")
            .bind(ms(now))
            .execute(&mut *conn)
            .await
            .expect("due");
        let job = claim(&mut conn).await.expect("claimed").expect("due");
        add(&mut conn, "read", Some("k"), &json, Utc::now())
            .await
            .expect("asked again");
        done(&mut conn, job.id).await.expect("done");
        assert!(!failing(&mut conn, "k").await.expect("read"));
    }

    #[tokio::test]
    async fn a_key_makes_one_job_and_a_failure_waits() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let later = Utc::now() + chrono::Duration::hours(1);
        add(
            &mut conn,
            "test",
            Some("k"),
            &serde_json::json!({"n": 1}),
            later,
        )
        .await
        .expect("added");
        add(
            &mut conn,
            "test",
            Some("k"),
            &serde_json::json!({"n": 2}),
            Utc::now() - chrono::Duration::seconds(1),
        )
        .await
        .expect("moved on");
        let count: i64 = sqlx::query_scalar("select count(*) from jobs")
            .fetch_one(&mut *conn)
            .await
            .expect("counted");
        assert_eq!(count, 1);
        let job = claim(&mut conn).await.expect("claimed").expect("due");
        assert_eq!(job.payload["n"], 2);
        assert!(
            claim(&mut conn).await.expect("claimed").is_none(),
            "taken once"
        );
        failed(&mut conn, job.id, "the model was busy")
            .await
            .expect("failed");
        let run_at: i64 = sqlx::query_scalar("select run_at from jobs where id = ?")
            .bind(job.id)
            .fetch_one(&mut *conn)
            .await
            .expect("read");
        assert!(run_at > now(), "it waits before trying again");
        assert!(next_due(&mut conn).await.expect("read").is_some());
    }

    #[tokio::test]
    async fn asking_by_a_time_never_puts_a_job_off_and_reruns_one_running() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let soon = Utc::now() - chrono::Duration::seconds(1);
        let none = serde_json::json!({});
        add_by(&mut conn, "test", "k", &none, soon)
            .await
            .expect("added");
        add_by(
            &mut conn,
            "test",
            "k",
            &none,
            soon + chrono::Duration::minutes(5),
        )
        .await
        .expect("asked again");
        let run_at: i64 = sqlx::query_scalar("select run_at from jobs where key = 'k'")
            .fetch_one(&mut *conn)
            .await
            .expect("read");
        assert!(run_at <= ms(soon), "the earlier time stands");
        let job = claim(&mut conn).await.expect("claimed").expect("due");
        add_by(&mut conn, "test", "k", &none, soon)
            .await
            .expect("asked while running");
        done(&mut conn, job.id).await.expect("done");
        let again: Option<i64> = sqlx::query_scalar("select run_at from jobs where key = 'k'")
            .fetch_optional(&mut *conn)
            .await
            .expect("read");
        assert!(again.is_some(), "it stays, to run again");
        let job = claim(&mut conn).await.expect("claimed").expect("due again");
        done(&mut conn, job.id).await.expect("done");
        let left: i64 = sqlx::query_scalar("select count(*) from jobs")
            .fetch_one(&mut *conn)
            .await
            .expect("counted");
        assert_eq!(left, 0, "then it goes");
    }

    #[tokio::test]
    async fn waiting_for_tomorrow_isnt_a_try() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        add(
            &mut conn,
            "test",
            None,
            &serde_json::json!({}),
            Utc::now() - chrono::Duration::seconds(1),
        )
        .await
        .expect("added");
        let job = claim(&mut conn).await.expect("claimed").expect("due");
        later(&mut conn, job.id, Utc::now() + chrono::Duration::hours(1))
            .await
            .expect("put off");
        let attempts: i64 = sqlx::query_scalar("select attempts from jobs where id = ?")
            .bind(job.id)
            .fetch_one(&mut *conn)
            .await
            .expect("read");
        assert_eq!(attempts, 0);
        sweep(&mut conn).await.expect("swept");
    }

    #[tokio::test]
    async fn work_waiting_for_tomorrow_comes_forward_and_nothing_else() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let tomorrow = crate::at(ms(Utc::now() + chrono::Duration::hours(5)));
        let next_week = Utc::now() + chrono::Duration::days(7);
        let json = serde_json::json!({});
        add(&mut conn, "test", Some("waiting"), &json, tomorrow)
            .await
            .expect("added");
        add(&mut conn, "test", Some("next week"), &json, next_week)
            .await
            .expect("added");
        assert_eq!(sooner(&mut conn, tomorrow).await.expect("brought"), 1);
        let now = due(&mut conn, "waiting").await.expect("read").expect("due");
        assert!(now <= Utc::now());
        let later = due(&mut conn, "next week")
            .await
            .expect("read")
            .expect("due");
        assert!(later > Utc::now() + chrono::Duration::days(6));
    }

    #[tokio::test]
    async fn what_a_stopped_kelve_was_running_runs_again_at_once() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        add(
            &mut conn,
            "test",
            Some("k"),
            &serde_json::json!({}),
            Utc::now(),
        )
        .await
        .expect("added");
        claim(&mut conn).await.expect("claimed").expect("due");
        assert!(claim(&mut conn).await.expect("read").is_none(), "held");
        release_all(&mut conn).await.expect("released");
        let again = claim(&mut conn).await.expect("claimed").expect("due again");
        assert_eq!(again.attempts, 1, "not counted as a try");
    }

    #[tokio::test]
    async fn when_a_job_runs_next() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        assert_eq!(due(&mut conn, "k").await.expect("read"), None);
        let then = Utc::now() + chrono::Duration::hours(1);
        add(&mut conn, "test", Some("k"), &serde_json::json!({}), then)
            .await
            .expect("added");
        let at = due(&mut conn, "k").await.expect("read").expect("waiting");
        assert!(at > Utc::now(), "waiting for later");
        sqlx::query("update jobs set run_at = 0, locked_at = 1")
            .execute(&mut *conn)
            .await
            .expect("running");
        let at = due(&mut conn, "k").await.expect("read").expect("running");
        assert!(at <= Utc::now(), "running now");
    }

    #[tokio::test]
    async fn a_job_asked_for_again_as_it_finishes_is_never_left_taken() {
        // A store on disk, with connections of its own, as kelve's: the job
        // is finished on one while the agent's answer asks for it again on
        // another.
        let path = std::env::temp_dir()
            .join(format!("kelve-test-{}", uuid::Uuid::now_v7().simple()))
            .join("kelve.db");
        let pool = crate::open(&path).await.expect("a store");
        crate::migrate(&pool).await.expect("migrated");
        let json = serde_json::json!({});
        let past = Utc::now() - chrono::Duration::seconds(1);
        for _ in 0..200 {
            let mut one = pool.acquire().await.expect("a connection");
            let mut two = pool.acquire().await.expect("another");
            add(&mut one, "relate", Some("k"), &json, past)
                .await
                .expect("added");
            let job = claim(&mut one).await.expect("claimed").expect("due");
            let (finished, asked) = tokio::join!(
                done(&mut one, job.id),
                add(&mut two, "relate", Some("k"), &json, past)
            );
            finished.expect("done");
            asked.expect("asked again");
            let taken: i64 =
                sqlx::query_scalar("select count(*) from jobs where locked_at is not null")
                    .fetch_one(&mut *one)
                    .await
                    .expect("counted");
            assert_eq!(taken, 0, "the job was left taken");
            sqlx::query("delete from jobs")
                .execute(&mut *one)
                .await
                .expect("cleared");
        }
        drop(pool);
        let _ = std::fs::remove_dir_all(path.parent().expect("its folder"));
    }
}
