//! What the skills page and Fits you need of the store beside `skills` and
//! `checks` (docs/skills.md): the projects whose skills kelve reads, checks
//! started together, how close each run came to what the person kept, and
//! Fits you's pairs with their changes.

use crate::checks::PastTask;
use crate::{Conn, Result, now};

/// Every project kelve knows of: those opened in it, those past work was
/// done in, and those whose skills it keeps. Absolute, each once.
pub async fn known_roots(conn: &mut Conn) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select root from opened where root != ''
         union select root from past_tasks where root != ''
         union select root from skills where root != ''
         order by 1",
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// The past tasks a check redoes, whose kept work is known, the newest
/// first: those in `root` first, when it's given.
pub async fn tasks_for(conn: &mut Conn, root: Option<&str>, most: i64) -> Result<Vec<PastTask>> {
    Ok(sqlx::query_as(
        "select id, request, root, start, kept, handover, debrief, at from past_tasks
          where kept is not null
          order by root = ? desc, at desc, id desc limit ?",
    )
    .bind(root.unwrap_or(""))
    .bind(most)
    .fetch_all(&mut *conn)
    .await?)
}

/// How many past tasks there are to redo.
pub async fn tasks_known(conn: &mut Conn) -> Result<i64> {
    Ok(
        sqlx::query_scalar("select count(*) from past_tasks where kept is not null")
            .fetch_one(&mut *conn)
            .await?,
    )
}

/// The person started a check of a part on a past task, with others in
/// `batch`: its id.
pub async fn ask_in(conn: &mut Conn, part: i64, task: i64, batch: i64) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "insert into checks (part, task, batch, asked_at) values (?, ?, ?, ?) returning id",
    )
    .bind(part)
    .bind(task)
    .bind(batch)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

/// How close each run of a check came to what the person kept.
pub async fn check_close(conn: &mut Conn, id: i64, with: f64, without: f64) -> Result<()> {
    sqlx::query("update checks set with_close = ?, without_close = ? where id = ?")
        .bind(with.clamp(0.0, 1.0))
        .bind(without.clamp(0.0, 1.0))
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The verdicts of a part's latest check, once every task in it is done:
/// none while any is still waiting or running.
pub async fn latest_verdicts(conn: &mut Conn, part: i64) -> Result<Option<Vec<String>>> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "select state, verdict from checks
          where part = ?1 and batch = (select max(batch) from checks where part = ?1)
            and state != 'stopped'
          order by id",
    )
    .bind(part)
    .fetch_all(&mut *conn)
    .await?;
    if rows
        .iter()
        .any(|(state, _)| state == "waiting" || state == "running")
    {
        return Ok(None);
    }
    Ok(Some(rows.into_iter().filter_map(|(_, v)| v).collect()))
}

/// How far a part's latest check has got: its tasks done, of how many.
pub async fn progress(conn: &mut Conn, part: i64) -> Result<(i64, i64)> {
    Ok(sqlx::query_as(
        "select coalesce(sum(state not in ('waiting', 'running')), 0), count(*) from checks
          where part = ?1 and batch = (select max(batch) from checks where part = ?1)
            and state != 'stopped'",
    )
    .bind(part)
    .fetch_one(&mut *conn)
    .await?)
}

/// What the latest check of a part found when it failed: each task's notes
/// and the clearer wording the agent gave, the earliest first.
pub async fn failures(conn: &mut Conn, part: i64) -> Result<Vec<(String, Option<String>)>> {
    Ok(sqlx::query_as(
        "select coalesce(notes, ''), wording from checks
          where part = ?1 and verdict = 'failed'
            and batch = (select max(batch) from checks where part = ?1)
          order by id",
    )
    .bind(part)
    .fetch_all(&mut *conn)
    .await?)
}

/// A check running for a skill's parts: how many parts, and how many of
/// its checks are done, of how many. None when none is running.
pub async fn running_for(conn: &mut Conn, skill: i64) -> Result<Option<(i64, i64, i64)>> {
    let (parts, done, all): (i64, i64, i64) = sqlx::query_as(
        "with b as (
           select max(c.batch) as batch from checks c join parts p on p.id = c.part
            where p.skill = ?1 and c.state in ('waiting', 'running')
         )
         select count(distinct c.part),
                coalesce(sum(c.state not in ('waiting', 'running')), 0),
                count(*)
           from checks c join parts p on p.id = c.part, b
          where p.skill = ?1 and c.batch = b.batch and c.state != 'stopped'",
    )
    .bind(skill)
    .fetch_one(&mut *conn)
    .await?;
    Ok((all > 0 && done < all).then_some((parts, done, all)))
}

/// The parts of a skill an agent is checking right now: those with a check
/// not done whose job (`compare:check:<id>`, in `server::skill_checks`) an
/// agent has taken or has just answered. A check asked for and not taken is
/// only waiting.
pub async fn being_done(conn: &mut Conn, skill: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "select distinct c.part from checks c
           join parts p on p.id = c.part
           join asks a on a.key = 'compare:check:' || c.id
          where p.skill = ?1 and c.state in ('waiting', 'running')
            and a.state in ('taken', 'answered')",
    )
    .bind(skill)
    .fetch_all(&mut *conn)
    .await?)
}

/// The checks of a skill's parts still waiting or running, by id.
pub async fn open_checks(conn: &mut Conn, skill: i64) -> Result<Vec<(i64, i64)>> {
    Ok(sqlx::query_as(
        "select c.id, c.part from checks c join parts p on p.id = c.part
          where p.skill = ? and c.state in ('waiting', 'running') order by c.id",
    )
    .bind(skill)
    .fetch_all(&mut *conn)
    .await?)
}

/// The person no longer keeps a part that made no difference: Undo.
pub async fn unkeep(conn: &mut Conn, part: i64) -> Result<()> {
    sqlx::query("update parts set kept = 0 where id = ?")
        .bind(part)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A Fits you pair as the page shows it.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Pair {
    pub id: i64,
    pub task: i64,
    pub skills_on: String,
    pub picked: Option<String>,
    pub skills_diff: Option<String>,
    pub standard_diff: Option<String>,
    pub skills_close: Option<f64>,
    pub standard_close: Option<f64>,
}

const PAIR: &str = "select id, task, skills_on, picked, skills_diff, standard_diff, skills_close,
     standard_close from fits_picks";

pub async fn pair(conn: &mut Conn, id: i64) -> Result<Option<Pair>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{PAIR} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The oldest pair whose changes are in, not picked yet.
pub async fn next_pair(conn: &mut Conn) -> Result<Option<Pair>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{PAIR} where picked is null and skills_diff is not null order by made_at, id limit 1"
    )))
    .fetch_optional(&mut *conn)
    .await?)
}

/// How many pairs are ready to pick between.
pub async fn pairs_ready(conn: &mut Conn) -> Result<u32> {
    let n: i64 = sqlx::query_scalar(
        "select count(*) from fits_picks where picked is null and skills_diff is not null",
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// How many pairs the agent is still redoing.
pub async fn pairs_waiting(conn: &mut Conn) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "select count(*) from fits_picks where picked is null and skills_diff is null",
    )
    .fetch_one(&mut *conn)
    .await?)
}

/// Each side's change, as the agent handed it in.
pub async fn pair_diffs(conn: &mut Conn, id: i64, skills: &str, standard: &str) -> Result<()> {
    sqlx::query("update fits_picks set skills_diff = ?, standard_diff = ? where id = ?")
        .bind(skills)
        .bind(standard)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A pair the agent couldn't redo: gone, as nothing's to pick.
pub async fn forget_pair(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query("delete from fits_picks where id = ? and picked is null")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Each pick so far, the oldest first: whether the person picked their own
/// agent's work.
pub async fn picked_own(conn: &mut Conn) -> Result<Vec<bool>> {
    Ok(sqlx::query_scalar(
        "select picked = skills_on from fits_picks where picked is not null
          order by picked_at, id",
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// How close each agent came to what the person kept, on average over the
/// pairs picked: none till one is.
pub async fn closeness_so_far(conn: &mut Conn) -> Result<Option<(f64, f64)>> {
    let (skills, standard): (Option<f64>, Option<f64>) = sqlx::query_as(
        "select avg(skills_close), avg(standard_close) from fits_picks
          where picked is not null and skills_close is not null",
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(skills.zip(standard))
}

/// The tasks already in a pair, picked or not: Fits you redoes each once.
pub async fn tasks_paired(conn: &mut Conn) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar("select distinct task from fits_picks")
        .fetch_all(&mut *conn)
        .await?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::checks::{self, NewTask};
    use crate::memory;
    use crate::skills::{self, NewSkill, SeenPart};

    #[tokio::test]
    async fn a_check_runs_until_every_task_in_it_is_done() {
        let pool = memory().await.expect("store");
        let mut conn = pool.acquire().await.expect("conn");
        let skill = skills::add(
            &mut conn,
            &NewSkill {
                name: "rust",
                kind: "like",
                work: "rust",
                ..Default::default()
            },
        )
        .await
        .expect("skill");
        let ids = skills::seen(
            &mut conn,
            skill,
            &[SeenPart {
                key: "one",
                heading: "One",
                section: "like",
                body_hash: "h",
            }],
        )
        .await
        .expect("parts");
        let task = |root| NewTask {
            request: "Add a flag",
            root,
            start: "abc",
            kept: Some("def"),
            ..Default::default()
        };
        let a = checks::add_task(&mut conn, &task("/elsewhere"))
            .await
            .expect("task");
        let b = checks::add_task(&mut conn, &task("/here"))
            .await
            .expect("task");
        let tasks = tasks_for(&mut conn, Some("/here"), 2).await.expect("tasks");
        assert_eq!(tasks.iter().map(|t| t.id).collect::<Vec<_>>(), [b, a]);
        assert_eq!(tasks_known(&mut conn).await.expect("count"), 2);

        let one = ask_in(&mut conn, ids[0], a, 7).await.expect("check");
        let two = ask_in(&mut conn, ids[0], b, 7).await.expect("check");
        assert_eq!(
            running_for(&mut conn, skill).await.expect("running"),
            Some((1, 0, 2))
        );
        assert_eq!(latest_verdicts(&mut conn, ids[0]).await.expect("v"), None);
        checks::check_done(&mut conn, one, "works", "Why.", None)
            .await
            .expect("done");
        assert_eq!(
            running_for(&mut conn, skill).await.expect("running"),
            Some((1, 1, 2))
        );
        checks::check_done(
            &mut conn,
            two,
            "failed",
            "Not followed.",
            Some("### Better"),
        )
        .await
        .expect("done");
        assert_eq!(running_for(&mut conn, skill).await.expect("running"), None);
        assert_eq!(
            latest_verdicts(&mut conn, ids[0]).await.expect("v"),
            Some(vec!["works".to_string(), "failed".to_string()])
        );
        assert_eq!(
            failures(&mut conn, ids[0]).await.expect("failures"),
            vec![("Not followed.".to_string(), Some("### Better".to_string()))]
        );
    }

    #[tokio::test]
    async fn fits_you_pairs_are_picked_once_their_changes_are_in() {
        let pool = memory().await.expect("store");
        let mut conn = pool.acquire().await.expect("conn");
        let task = checks::add_task(
            &mut conn,
            &NewTask {
                request: "Add a flag",
                root: "/here",
                start: "abc",
                kept: Some("def"),
                ..Default::default()
            },
        )
        .await
        .expect("task");
        let id = checks::add_pick(&mut conn, task, "right")
            .await
            .expect("pick");
        assert_eq!(pairs_waiting(&mut conn).await.expect("waiting"), 1);
        assert_eq!(next_pair(&mut conn).await.expect("next"), None);
        pair_diffs(&mut conn, id, "+mine", "+theirs")
            .await
            .expect("diffs");
        checks::close(&mut conn, id, 0.8, 0.4).await.expect("close");
        assert_eq!(
            next_pair(&mut conn).await.expect("next").map(|p| p.id),
            Some(id)
        );
        assert_eq!(closeness_so_far(&mut conn).await.expect("close"), None);
        assert!(checks::pick(&mut conn, id, "right").await.expect("pick"));
        assert_eq!(picked_own(&mut conn).await.expect("own"), [true]);
        assert_eq!(
            closeness_so_far(&mut conn).await.expect("close"),
            Some((0.8, 0.4))
        );
        assert_eq!(tasks_paired(&mut conn).await.expect("paired"), [task]);
    }
}
