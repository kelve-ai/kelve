//! The agent loop's own queries (docs/skills.md): whether a handover was
//! worth a debrief, the debrief of a piece of work, the commit last compared
//! with a handover, evidence the learn job has had, skills given to a
//! session through the hook, changes kelve made to skill files, and the
//! past task each piece of work becomes.

use crate::candidates::Evidence;
use crate::checks::{self, NewTask, PastTask};
use crate::{Conn, Result, now};

/// Whether a debrief was due for a handover, and why, in a sentence.
pub async fn set_due(conn: &mut Conn, handover: i64, due: bool, why: &str) -> Result<()> {
    sqlx::query("update handovers set due = ?, due_why = ? where id = ?")
        .bind(due)
        .bind(why)
        .bind(handover)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Whether a debrief was due for a handover, and why: none till kelve
/// decided.
pub async fn due_of(conn: &mut Conn, handover: i64) -> Result<Option<(bool, String)>> {
    let row: Option<(Option<bool>, String)> =
        sqlx::query_as("select due, due_why from handovers where id = ?")
            .bind(handover)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.and_then(|(due, why)| due.map(|d| (d, why))))
}

/// The debrief of a piece of work, brought up to the latest handover: the
/// one not yet sent for the same repository and branch, in the same
/// session when both know theirs, or a new one. Its id, and whether it's
/// new. Bringing it up to date sets it waiting again, with its tries
/// forgotten.
pub async fn debrief_for_work(
    conn: &mut Conn,
    handover: i64,
    root: &str,
    branch: &str,
    session: &str,
) -> Result<(i64, bool)> {
    let at = now();
    let open: Option<i64> = sqlx::query_scalar(
        "select d.id from debriefs d join handovers h on h.id = d.handover
          where d.root = ?1 and h.branch = ?2 and d.state != 'sent'
            and (?3 = '' or d.session = '' or d.session = ?3)
          order by d.made_at desc, d.id desc limit 1",
    )
    .bind(root)
    .bind(branch)
    .bind(session)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(id) = open {
        sqlx::query(
            "update debriefs set handover = ?, updated_at = ?,
               session = case when session = '' then ? else session end,
               state = case when state in ('ready', 'failed') then 'waiting' else state end,
               tries = 0, last_error = null
             where id = ?",
        )
        .bind(handover)
        .bind(at)
        .bind(session)
        .bind(id)
        .execute(&mut *conn)
        .await?;
        return Ok((id, false));
    }
    let id = sqlx::query_scalar(
        "insert into debriefs (handover, root, session, made_at, updated_at) values (?, ?, ?, ?, ?)
         returning id",
    )
    .bind(handover)
    .bind(root)
    .bind(session)
    .bind(at)
    .bind(at)
    .fetch_one(&mut *conn)
    .await?;
    Ok((id, true))
}

/// The debrief of the work a handover is part of, when there is one.
pub async fn debrief_of_handover(conn: &mut Conn, handover: i64) -> Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("select id from debriefs where handover = ? order by id desc limit 1")
            .bind(handover)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The session a debrief's work was done in, once the hook says.
pub async fn debrief_session(conn: &mut Conn, debrief: i64, session: &str) -> Result<()> {
    sqlx::query("update debriefs set session = ? where id = ? and session = ''")
        .bind(session)
        .bind(debrief)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A debrief asked for again after it failed: waiting, its tries forgotten.
pub async fn debrief_again(conn: &mut Conn, debrief: i64) -> Result<bool> {
    let done = sqlx::query(
        "update debriefs set state = 'waiting', tries = 0, last_error = null, updated_at = ?
          where id = ? and state != 'sent'",
    )
    .bind(now())
    .bind(debrief)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// The debrief failed its checks for good: home says so, with Ask it again.
pub async fn debrief_failed(conn: &mut Conn, debrief: i64, why: &str) -> Result<()> {
    sqlx::query(
        "update debriefs set state = 'failed', last_error = ?, updated_at = ? where id = ? and state != 'sent'",
    )
    .bind(why)
    .bind(now())
    .bind(debrief)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// When the session last had a debrief sent: its next piece of work began
/// after.
pub async fn last_sent(conn: &mut Conn, session: &str) -> Result<Option<i64>> {
    Ok(
        sqlx::query_scalar(
            "select max(sent_at) from debriefs where session = ? and state = 'sent'",
        )
        .bind(session)
        .fetch_one(&mut *conn)
        .await?,
    )
}

/// The commit last compared with a handover, now.
pub async fn compared_head(conn: &mut Conn, handover: i64, head: &str) -> Result<()> {
    sqlx::query("update handovers set compared_head = ?, compared_at = ? where id = ?")
        .bind(head)
        .bind(now())
        .bind(handover)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The commit last compared with a handover, '' for none.
pub async fn compared_of(conn: &mut Conn, handover: i64) -> Result<String> {
    Ok(
        sqlx::query_scalar("select compared_head from handovers where id = ?")
            .bind(handover)
            .fetch_optional(&mut *conn)
            .await?
            .unwrap_or_default(),
    )
}

const EVIDENCE_ROW: &str = "select id, kind, part, candidate, session, root, handover, path,
     decision, text, at from evidence";

/// Evidence of a kind the learn job hasn't had, the earliest first: for a
/// session's messages, or a handover's edits.
pub async fn unlearned(
    conn: &mut Conn,
    kind: &str,
    session: Option<&str>,
    handover: Option<i64>,
) -> Result<Vec<Evidence>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{EVIDENCE_ROW} where kind = ?1 and learned_at is null and candidate is null and part is null
           and (?2 is null or session = ?2) and (?3 is null or handover = ?3)
         order by at, id"
    )))
    .bind(kind)
    .bind(session)
    .bind(handover)
    .fetch_all(&mut *conn)
    .await?)
}

/// The learn job had this evidence.
pub async fn learned(conn: &mut Conn, ids: &[i64]) -> Result<()> {
    let at = now();
    for id in ids {
        sqlx::query("update evidence set learned_at = ? where id = ?")
            .bind(at)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Evidence brought up to date, as an edit is when the person commits
/// again: the learn job has it afresh.
pub async fn unlearn(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query(
        "update evidence set learned_at = null where id = ? and candidate is null and part is null",
    )
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Evidence by its ids, in that order where it's there.
pub async fn evidence(conn: &mut Conn, ids: &[i64]) -> Result<Vec<Evidence>> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let e: Option<Evidence> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("{EVIDENCE_ROW} where id = ?")))
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
        out.extend(e);
    }
    Ok(out)
}

/// Gives a skill to a session: whether it's the first time.
pub async fn give(conn: &mut Conn, session: &str, skill: &str) -> Result<bool> {
    let done = sqlx::query(
        "insert into skills_given (session, skill, at) values (?, ?, ?)
         on conflict (session, skill) do nothing",
    )
    .bind(session)
    .bind(skill)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Whether a session has had a skill.
pub async fn given(conn: &mut Conn, session: &str, skill: &str) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "select exists(select 1 from skills_given where session = ? and skill = ?)",
    )
    .bind(session)
    .bind(skill)
    .fetch_one(&mut *conn)
    .await?)
}

/// Whether a session has had any skill, or any line from kelve about them.
pub async fn given_any(conn: &mut Conn, session: &str) -> Result<bool> {
    Ok(
        sqlx::query_scalar("select exists(select 1 from skills_given where session = ?)")
            .bind(session)
            .fetch_one(&mut *conn)
            .await?,
    )
}

/// Forgets what sessions were given before a time: they've ended.
pub async fn sweep_given(conn: &mut Conn, before: i64) -> Result<u64> {
    let done = sqlx::query("delete from skills_given where at < ?")
        .bind(before)
        .execute(&mut *conn)
        .await?;
    Ok(done.rows_affected())
}

/// A change kelve made to a skill's file.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SkillEdit {
    pub id: i64,
    pub skill: Option<i64>,
    pub candidate: Option<i64>,
    pub root: String,
    pub name: String,
    /// `new` or `replaced`.
    pub relation: String,
    pub heading: String,
    pub old_heading: Option<String>,
    pub old_body: Option<String>,
    pub before: String,
    pub after: String,
    pub at: i64,
    pub undone_at: Option<i64>,
}

/// A change to keep.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewEdit<'a> {
    pub skill: Option<i64>,
    pub candidate: Option<i64>,
    pub root: &'a str,
    pub name: &'a str,
    pub relation: &'a str,
    pub heading: &'a str,
    pub old_heading: Option<&'a str>,
    pub old_body: Option<&'a str>,
    pub before: &'a str,
    pub after: &'a str,
}

const EDIT: &str =
    "select id, skill, candidate, root, name, relation, heading, old_heading, old_body,
     before, after, at, undone_at from skill_edits";

/// Keeps a change kelve made to a skill's file: its id.
pub async fn add_edit(conn: &mut Conn, e: &NewEdit<'_>) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "insert into skill_edits (skill, candidate, root, name, relation, heading, old_heading,
                                  old_body, before, after, at)
         values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) returning id",
    )
    .bind(e.skill)
    .bind(e.candidate)
    .bind(e.root)
    .bind(e.name)
    .bind(e.relation)
    .bind(e.heading)
    .bind(e.old_heading)
    .bind(e.old_body)
    .bind(e.before)
    .bind(e.after)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

pub async fn edit(conn: &mut Conn, id: i64) -> Result<Option<SkillEdit>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{EDIT} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The change a candidate made, when it made one and it isn't undone.
pub async fn edit_of(conn: &mut Conn, candidate: i64) -> Result<Option<SkillEdit>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{EDIT} where candidate = ? and undone_at is null order by id desc limit 1"
    )))
    .bind(candidate)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The latest changes kelve made to skills, the newest first.
pub async fn edits(conn: &mut Conn, most: i64) -> Result<Vec<SkillEdit>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{EDIT} order by at desc, id desc limit ?"
    )))
    .bind(most)
    .fetch_all(&mut *conn)
    .await?)
}

/// A change undone: whether it was there to undo.
pub async fn undone(conn: &mut Conn, id: i64) -> Result<bool> {
    let done =
        sqlx::query("update skill_edits set undone_at = ? where id = ? and undone_at is null")
            .bind(now())
            .bind(id)
            .execute(&mut *conn)
            .await?;
    Ok(done.rows_affected() > 0)
}

/// The past task a piece of work is, by its debrief, when it's kept.
pub async fn task_of(conn: &mut Conn, debrief: i64) -> Result<Option<PastTask>> {
    let id: Option<i64> =
        sqlx::query_scalar("select id from past_tasks where debrief = ? order by id desc limit 1")
            .bind(debrief)
            .fetch_optional(&mut *conn)
            .await?;
    match id {
        Some(id) => checks::task(conn, id).await,
        None => Ok(None),
    }
}

/// Keeps a piece of work as a past task, or brings it up to date: the
/// latest handover, what the person kept when it's known, and their
/// request when there is one. Where it started stays as it was first
/// kept. Its id.
pub async fn keep_task(conn: &mut Conn, t: &NewTask<'_>) -> Result<i64> {
    let Some(debrief) = t.debrief else {
        return checks::add_task(conn, t).await;
    };
    match task_of(conn, debrief).await? {
        Some(task) => {
            let request = kelve_core::text::clip(
                kelve_core::redact::redact(t.request).trim(),
                checks::REQUEST,
            );
            sqlx::query(
                "update past_tasks set handover = coalesce(?, handover),
                   kept = coalesce(?, kept),
                   request = case when ? = '' then request else ? end
                 where id = ?",
            )
            .bind(t.handover)
            .bind(t.kept)
            .bind(&request)
            .bind(&request)
            .bind(task.id)
            .execute(&mut *conn)
            .await?;
            Ok(task.id)
        }
        None => checks::add_task(conn, t).await,
    }
}

/// What the person kept of the work handed over: the commit it's in.
pub async fn kept_at(conn: &mut Conn, handover: i64, kept: &str) -> Result<u64> {
    let done = sqlx::query("update past_tasks set kept = ? where handover = ?")
        .bind(kept)
        .bind(handover)
        .execute(&mut *conn)
        .await?;
    Ok(done.rows_affected())
}

/// The jobs of the agent loop waiting for the agent: debriefs, research,
/// explanations and learning.
pub async fn waiting_jobs(conn: &mut Conn) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "select count(*) from asks where state = 'waiting'
           and kind in ('debrief', 'learn', 'research', 'explain')",
    )
    .fetch_one(&mut *conn)
    .await?)
}

/// The latest research asked on a decision that isn't answered yet.
pub async fn research_waiting(conn: &mut Conn, decision: i64) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar(
        "select id from research where decision = ? and state in ('waiting', 'running', 'failed')
          order by id desc limit 1",
    )
    .bind(decision)
    .fetch_optional(&mut *conn)
    .await?)
}

/// A research's row, by its id.
pub async fn research(conn: &mut Conn, id: i64) -> Result<Option<crate::debriefs::Research>> {
    Ok(sqlx::query_as(
        "select id, decision, state, question, answer, asked_at, answered_at from research where id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?)
}

/// An explanation's row, by its id.
pub async fn explanation(conn: &mut Conn, id: i64) -> Result<Option<crate::debriefs::Explanation>> {
    Ok(sqlx::query_as(
        "select id, debrief, idea, key, print, state, answer, asked_at, answered_at
           from explanations where id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Proposed candidates caught in a repository not yet shown in any
/// debrief: they go under For your skills in the next one there.
pub async fn unshown_in(conn: &mut Conn, root: &str) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "select distinct c.id from candidates c join evidence e on e.candidate = c.id
          where c.state = 'proposed' and c.debrief is null and e.root = ?
          order by c.id",
    )
    .bind(root)
    .fetch_all(&mut *conn)
    .await?)
}

/// The part a candidate goes after, by its heading as written.
pub async fn set_after(conn: &mut Conn, candidate: i64, after: Option<&str>) -> Result<()> {
    sqlx::query("update candidates set after_part = ? where id = ?")
        .bind(after)
        .bind(candidate)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The part a candidate goes after, when it says.
pub async fn after_of(conn: &mut Conn, candidate: i64) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("select after_part from candidates where id = ?")
            .bind(candidate)
            .fetch_optional(&mut *conn)
            .await?
            .flatten(),
    )
}

/// The branch a handover was read on, when it didn't say.
pub async fn set_branch(conn: &mut Conn, handover: i64, branch: &str) -> Result<()> {
    sqlx::query("update handovers set branch = ? where id = ? and branch = ''")
        .bind(branch)
        .bind(handover)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The debrief not yet sent of the work on a repository's branch, when
/// there is one.
pub async fn open_debrief(
    conn: &mut Conn,
    root: &str,
    branch: &str,
    session: &str,
) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar(
        "select d.id from debriefs d join handovers h on h.id = d.handover
          where d.root = ?1 and h.branch = ?2 and d.state != 'sent'
            and (?3 = '' or d.session = '' or d.session = ?3)
          order by d.made_at desc, d.id desc limit 1",
    )
    .bind(root)
    .bind(branch)
    .bind(session)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Shows a candidate in a debrief, under For your skills.
pub async fn show_in(conn: &mut Conn, candidate: i64, debrief: i64) -> Result<()> {
    sqlx::query("update candidates set debrief = ? where id = ?")
        .bind(debrief)
        .bind(candidate)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::handovers::{self, NewHandover};
    use crate::memory;

    async fn handover(conn: &mut Conn, branch: &str) -> i64 {
        handovers::add(
            conn,
            &NewHandover {
                root: "/w",
                branch,
                change: "since main",
                ..Default::default()
            },
            &[],
        )
        .await
        .expect("handed over")
    }

    #[tokio::test]
    async fn more_on_the_same_work_brings_its_debrief_up_to_date() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let first = handover(&mut conn, "fix").await;
        let (id, new) = debrief_for_work(&mut conn, first, "/w", "fix", "")
            .await
            .expect("a debrief");
        assert!(new);
        let again = handover(&mut conn, "fix").await;
        let (same, new) = debrief_for_work(&mut conn, again, "/w", "fix", "s1")
            .await
            .expect("the same");
        assert_eq!((same, new), (id, false));
        let other = handover(&mut conn, "other").await;
        let (another, _) = debrief_for_work(&mut conn, other, "/w", "other", "s1")
            .await
            .expect("another");
        assert_ne!(another, id);
        // Once sent, the next piece of work gets its own.
        crate::debriefs::written(&mut conn, id, "Fix", "small", "x")
            .await
            .expect("written");
        assert!(crate::debriefs::sent(&mut conn, id).await.expect("sent"));
        let more = handover(&mut conn, "fix").await;
        let (next, new) = debrief_for_work(&mut conn, more, "/w", "fix", "s1")
            .await
            .expect("next");
        assert!(new && next != id);
    }

    #[tokio::test]
    async fn a_skill_is_given_to_a_session_once() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        assert!(give(&mut conn, "s", "|rust").await.expect("given"));
        assert!(!give(&mut conn, "s", "|rust").await.expect("again"));
        assert!(
            give(&mut conn, "t", "|rust")
                .await
                .expect("another session")
        );
        assert!(given(&mut conn, "s", "|rust").await.expect("asked"));
    }
}
