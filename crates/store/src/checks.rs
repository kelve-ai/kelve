//! The person's past work, and what's redone on it (docs/skills.md): skill
//! checks, each part redone with and without it on a past task, and Fits
//! you, the person's agent against a standard one, picked blind.

use crate::{Conn, Result, now};

/// A past task: what the person asked, where it started, and what they kept.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PastTask {
    pub id: i64,
    pub request: String,
    pub root: String,
    pub start: String,
    pub kept: Option<String>,
    pub handover: Option<i64>,
    pub debrief: Option<i64>,
    pub at: i64,
}

/// A past task to keep.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewTask<'a> {
    pub request: &'a str,
    pub root: &'a str,
    pub start: &'a str,
    pub kept: Option<&'a str>,
    pub handover: Option<i64>,
    pub debrief: Option<i64>,
}

/// The longest request kept, in characters.
pub const REQUEST: usize = 4_000;

const TASK: &str = "select id, request, root, start, kept, handover, debrief, at from past_tasks";

/// Keeps a past task, its request short with secrets stripped: its id.
pub async fn add_task(conn: &mut Conn, t: &NewTask<'_>) -> Result<i64> {
    let request = kelve_core::text::clip(kelve_core::redact::redact(t.request).trim(), REQUEST);
    Ok(sqlx::query_scalar(
        "insert into past_tasks (request, root, start, kept, handover, debrief, at)
         values (?, ?, ?, ?, ?, ?, ?) returning id",
    )
    .bind(request)
    .bind(t.root)
    .bind(t.start)
    .bind(t.kept)
    .bind(t.handover)
    .bind(t.debrief)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

pub async fn task(conn: &mut Conn, id: i64) -> Result<Option<PastTask>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{TASK} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The latest past tasks whose kept work is known, the newest first: what
/// checks and Fits you redo.
pub async fn tasks(conn: &mut Conn, most: i64) -> Result<Vec<PastTask>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{TASK} where kept is not null order by at desc, id desc limit ?"
    )))
    .bind(most)
    .fetch_all(&mut *conn)
    .await?)
}

/// What the person kept of a past task: a commit, or a tree.
pub async fn kept(conn: &mut Conn, id: i64, kept: &str) -> Result<()> {
    sqlx::query("update past_tasks set kept = ? where id = ?")
        .bind(kept)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A part checked on a past task.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Check {
    pub id: i64,
    pub part: i64,
    pub task: i64,
    /// `waiting`, `running`, `done`, `failed` or `stopped`.
    pub state: String,
    /// Once done: `works`, `no_difference` or `failed`.
    pub verdict: Option<String>,
    pub notes: Option<String>,
    pub wording: Option<String>,
    pub asked_at: i64,
    pub done_at: Option<i64>,
}

const CHECK: &str =
    "select id, part, task, state, verdict, notes, wording, asked_at, done_at from checks";

/// The person started a check of a part on a past task: its id.
pub async fn ask_check(conn: &mut Conn, part: i64, task: i64) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "insert into checks (part, task, asked_at) values (?, ?, ?) returning id",
    )
    .bind(part)
    .bind(task)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

pub async fn check(conn: &mut Conn, id: i64) -> Result<Option<Check>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{CHECK} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// A part's checks, the earliest first.
pub async fn checks_of(conn: &mut Conn, part: i64) -> Result<Vec<Check>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{CHECK} where part = ? order by asked_at, id"
    )))
    .bind(part)
    .fetch_all(&mut *conn)
    .await?)
}

/// The agent took a check: it's running.
pub async fn running(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query("update checks set state = 'running' where id = ? and state = 'waiting'")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// What a check found: its verdict, notes, and a clearer wording when the
/// part failed.
pub async fn check_done(
    conn: &mut Conn,
    id: i64,
    verdict: &str,
    notes: &str,
    wording: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "update checks set state = 'done', verdict = ?, notes = ?, wording = ?, done_at = ?
         where id = ? and state in ('waiting', 'running')",
    )
    .bind(verdict)
    .bind(notes)
    .bind(wording)
    .bind(now())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A check that couldn't be done.
pub async fn check_failed(conn: &mut Conn, id: i64, notes: &str) -> Result<()> {
    sqlx::query(
        "update checks set state = 'failed', notes = ?, done_at = ? where id = ? and state in ('waiting', 'running')",
    )
    .bind(notes)
    .bind(now())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The person stopped a part's checks: those not done. How many.
pub async fn stop(conn: &mut Conn, part: i64) -> Result<u64> {
    let done = sqlx::query(
        "update checks set state = 'stopped', done_at = ? where part = ? and state in ('waiting', 'running')",
    )
    .bind(now())
    .bind(part)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected())
}

/// A Fits you pick: one past task redone both ways, shown side by side.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Pick {
    pub id: i64,
    pub task: i64,
    /// Which side the person's agent was shown on: `left` or `right`.
    pub skills_on: String,
    /// `left`, `right` or `same`, once picked.
    pub picked: Option<String>,
    pub made_at: i64,
    pub picked_at: Option<i64>,
}

/// How each side came out, with how close each came to what was kept.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Closeness {
    pub skills_close: Option<f64>,
    pub standard_close: Option<f64>,
}

/// A pick to show, its sides as chosen: its id.
pub async fn add_pick(conn: &mut Conn, task: i64, skills_on: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "insert into fits_picks (task, skills_on, made_at) values (?, ?, ?) returning id",
    )
    .bind(task)
    .bind(skills_on)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

/// The person picked the side they'd rather keep, or neither.
pub async fn pick(conn: &mut Conn, id: i64, picked: &str) -> Result<bool> {
    let done = sqlx::query(
        "update fits_picks set picked = ?, picked_at = ? where id = ? and picked is null",
    )
    .bind(picked)
    .bind(now())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// How close each side came to what the person kept, from 0 to 1.
pub async fn close(conn: &mut Conn, id: i64, skills: f64, standard: f64) -> Result<()> {
    sqlx::query("update fits_picks set skills_close = ?, standard_close = ? where id = ?")
        .bind(skills.clamp(0.0, 1.0))
        .bind(standard.clamp(0.0, 1.0))
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The picks, the newest first.
pub async fn picks(conn: &mut Conn, most: i64) -> Result<Vec<Pick>> {
    Ok(sqlx::query_as(
        "select id, task, skills_on, picked, made_at, picked_at from fits_picks
          order by made_at desc, id desc limit ?",
    )
    .bind(most)
    .fetch_all(&mut *conn)
    .await?)
}

pub async fn closeness(conn: &mut Conn, id: i64) -> Result<Option<Closeness>> {
    Ok(
        sqlx::query_as("select skills_close, standard_close from fits_picks where id = ?")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// Fits you so far: how many times the person picked their own agent's
/// work, the standard agent's, and neither.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tally {
    pub skills: i64,
    pub standard: i64,
    pub same: i64,
}

pub async fn tally(conn: &mut Conn) -> Result<Tally> {
    let (skills, standard, same): (i64, i64, i64) = sqlx::query_as(
        "select coalesce(sum(picked = skills_on), 0),
                coalesce(sum(picked != skills_on and picked != 'same'), 0),
                coalesce(sum(picked = 'same'), 0)
           from fits_picks where picked is not null",
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(Tally {
        skills,
        standard,
        same,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;
    use crate::skills::{self, NewSkill, SeenPart};

    async fn a_part(conn: &mut Conn) -> i64 {
        let skill = skills::add(
            conn,
            &NewSkill {
                name: "rust",
                kind: "like",
                work: "rust",
                ..NewSkill::default()
            },
        )
        .await
        .expect("kept");
        skills::seen(
            conn,
            skill,
            &[SeenPart {
                key: "exit 1",
                heading: "Exit 1",
                section: "like",
                body_hash: "a",
            }],
        )
        .await
        .expect("seen")[0]
    }

    #[tokio::test]
    async fn a_part_is_checked_on_past_tasks_and_can_be_stopped() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let part = a_part(&mut conn).await;
        let t = add_task(
            &mut conn,
            &NewTask {
                request: "Add a --quiet flag. My key is ghp_abcdefghijklmnopqrstuvwxyz0123456789",
                root: "/w",
                start: "3f2a9c1",
                ..NewTask::default()
            },
        )
        .await
        .expect("kept");
        assert!(
            tasks(&mut conn, 10).await.expect("read").is_empty(),
            "not till what was kept is known"
        );
        kept(&mut conn, t, "9b8c7d6").await.expect("kept");
        let listed = tasks(&mut conn, 10).await.expect("read");
        assert_eq!(listed.len(), 1);
        assert!(
            !listed[0].request.contains("ghp_abc"),
            "{}",
            listed[0].request
        );

        let one = ask_check(&mut conn, part, t).await.expect("asked");
        let two = ask_check(&mut conn, part, t).await.expect("asked");
        running(&mut conn, one).await.expect("taken");
        check_done(
            &mut conn,
            one,
            "works",
            "It exited 1 only with the part.",
            None,
        )
        .await
        .expect("done");
        assert_eq!(stop(&mut conn, part).await.expect("stopped"), 1);
        let all = checks_of(&mut conn, part).await.expect("read");
        assert_eq!(all[0].verdict.as_deref(), Some("works"));
        assert_eq!(all[1].state, "stopped");
        check_done(&mut conn, two, "works", "", None)
            .await
            .expect("asked");
        assert_eq!(
            check(&mut conn, two)
                .await
                .expect("read")
                .expect("there")
                .state,
            "stopped",
            "a stopped check stays stopped"
        );
        let three = ask_check(&mut conn, part, t).await.expect("asked");
        check_failed(&mut conn, three, "The worktree couldn't be made.")
            .await
            .expect("failed");
        assert_eq!(
            check(&mut conn, three)
                .await
                .expect("read")
                .expect("there")
                .state,
            "failed"
        );
    }

    #[tokio::test]
    async fn fits_you_counts_the_picks_by_which_side_was_whose() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let t = add_task(
            &mut conn,
            &NewTask {
                request: "Add a --quiet flag.",
                root: "/w",
                start: "3f2a9c1",
                kept: Some("9b8c7d6"),
                ..NewTask::default()
            },
        )
        .await
        .expect("kept");
        let a = add_pick(&mut conn, t, "left").await.expect("made");
        let b = add_pick(&mut conn, t, "right").await.expect("made");
        let c = add_pick(&mut conn, t, "left").await.expect("made");
        let d = add_pick(&mut conn, t, "right").await.expect("made");
        assert_eq!(tally(&mut conn).await.expect("read"), Tally::default());
        assert!(pick(&mut conn, a, "left").await.expect("picked"));
        assert!(!pick(&mut conn, a, "right").await.expect("asked"), "once");
        assert!(pick(&mut conn, b, "right").await.expect("picked"));
        assert!(pick(&mut conn, c, "right").await.expect("picked"));
        assert!(pick(&mut conn, d, "same").await.expect("picked"));
        assert_eq!(
            tally(&mut conn).await.expect("read"),
            Tally {
                skills: 2,
                standard: 1,
                same: 1
            }
        );
        close(&mut conn, a, 0.9, 1.4).await.expect("kept");
        let c = closeness(&mut conn, a).await.expect("read").expect("there");
        assert_eq!((c.skills_close, c.standard_close), (Some(0.9), Some(1.0)));
        assert_eq!(picks(&mut conn, 10).await.expect("read").len(), 4);
    }
}
