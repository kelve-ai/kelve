//! What home and the debrief's page read and change beyond the debrief
//! itself (docs/skills.md): handovers too small for a debrief, the person's
//! calls on decisions, what waits for their agent, and what happened lately
//! in their skills, a line each on home's timeline.

use crate::debriefs::{Debrief, Research};
use crate::handovers::Handover;
use crate::{Conn, Result, now};

const HANDOVER: &str = "select h.id, h.opened, h.root, h.branch, h.base, h.change, h.head,
     h.uncommitted, h.agent, h.session, h.title, h.said, h.files, h.added, h.removed, h.cut,
     h.at, h.compared_at from handovers h";

/// Handovers since a time with no debrief, the latest first: work too small
/// for one, which home lists as one quiet line each. A handover whose work
/// has a debrief already, brought up to date by a later one in the same
/// session before it was sent, is that debrief's.
pub async fn small(conn: &mut Conn, since: i64, most: i64) -> Result<Vec<Handover>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{HANDOVER} where h.at >= ? and not exists (
           select 1 from debriefs d where d.handover = h.id
              or (h.session != '' and d.session = h.session and d.root = h.root
                  and d.made_at <= h.at and (d.sent_at is null or d.sent_at >= h.at)))
         order by h.at desc, h.id desc limit ?"
    )))
    .bind(since)
    .bind(most)
    .fetch_all(&mut *conn)
    .await?)
}

/// The debrief of a handover, when it has one.
pub async fn debrief_of(conn: &mut Conn, handover: i64) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar(
        "select id from debriefs where handover = ? order by made_at desc limit 1",
    )
    .bind(handover)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Debriefs touched since a time, the latest first.
pub async fn debriefs_since(conn: &mut Conn, since: i64, most: i64) -> Result<Vec<Debrief>> {
    Ok(sqlx::query_as(
        "select id, handover, root, session, state, title, size, markdown, tries, last_error,
           made_at, updated_at, sent_at from debriefs
          where updated_at >= ? order by updated_at desc, id desc limit ?",
    )
    .bind(since)
    .bind(most)
    .fetch_all(&mut *conn)
    .await?)
}

/// When the person last sent their agent a message, as its hook saw it.
pub async fn last_message_at(conn: &mut Conn) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar("select max(at) from messages")
        .fetch_one(&mut *conn)
        .await?)
}

/// When the person's agent last handed work over.
pub async fn last_handover_at(conn: &mut Conn) -> Result<Option<i64>> {
    Ok(sqlx::query_scalar("select max(at) from handovers")
        .fetch_one(&mut *conn)
        .await?)
}

/// Whether kelve's hook has ever told kelve anything: a message, or a
/// handover in an agent's session.
pub async fn hook_heard(conn: &mut Conn) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "select exists (select 1 from messages) or exists (select 1 from handovers where session != '')",
    )
    .fetch_one(&mut *conn)
    .await?)
}

/// Ask for a debrief that failed again: it waits for the agent, as new.
pub async fn ask_again(conn: &mut Conn, id: i64) -> Result<bool> {
    let done = sqlx::query(
        "update debriefs set state = 'waiting', tries = 0, last_error = null, updated_at = ?
          where id = ? and state = 'failed'",
    )
    .bind(now())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// The person's call on a decision, or none to take it back, with the
/// note for their agent when they want it changed. Whether they want it
/// this way every time stays as it was.
pub async fn decide(
    conn: &mut Conn,
    decision: i64,
    choice: Option<&str>,
    note: Option<&str>,
) -> Result<bool> {
    let done = sqlx::query(
        "update decisions set choice = ?, note = ?,
           chosen_at = case when ? is null then null else ? end
         where id = ?",
    )
    .bind(choice)
    .bind(note)
    .bind(choice)
    .bind(now())
    .bind(decision)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Whether the person wants a decision this way every time, and the skill
/// it goes in, when they have it: none for a skill still to make.
pub async fn every_time(
    conn: &mut Conn,
    decision: i64,
    every_time: bool,
    skill: Option<i64>,
) -> Result<bool> {
    let done = sqlx::query("update decisions set every_time = ?, skill = ? where id = ?")
        .bind(every_time)
        .bind(skill.filter(|_| every_time))
        .bind(decision)
        .execute(&mut *conn)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Research not answered yet, the earliest first: what waits for the agent.
/// Each with its decision's number and title, and its debrief's title.
pub async fn research_waiting(conn: &mut Conn) -> Result<Vec<Waiting>> {
    Ok(sqlx::query_as(
        "select r.id, d.debrief, d.number, d.title, b.title as about from research r
           join decisions d on d.id = r.decision join debriefs b on b.id = d.debrief
          where r.state in ('waiting', 'running') order by r.asked_at, r.id",
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// Explanations not written yet, the earliest first, each with its idea.
pub async fn explanations_waiting(conn: &mut Conn) -> Result<Vec<Waiting>> {
    Ok(sqlx::query_as(
        "select e.id, coalesce(e.debrief, 0) as debrief, 0 as number, e.idea as title,
           coalesce(b.title, '') as about from explanations e
           left join debriefs b on b.id = e.debrief
          where e.state in ('waiting', 'running') order by e.asked_at, e.id",
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// Something waiting for the person's agent.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Waiting {
    pub id: i64,
    pub debrief: i64,
    /// The decision's number, for research.
    pub number: i64,
    /// The decision's title, or the idea's name.
    pub title: String,
    /// The debrief's title.
    pub about: String,
}

/// The latest research on each of a debrief's decisions, by decision.
pub async fn research_of(conn: &mut Conn, debrief: i64) -> Result<Vec<Research>> {
    Ok(sqlx::query_as(
        "select r.id, r.decision, r.state, r.question, r.answer, r.asked_at, r.answered_at
           from research r join decisions d on d.id = r.decision
          where d.debrief = ? and r.id = (
            select r2.id from research r2 where r2.decision = r.decision
             order by r2.asked_at desc, r2.id desc limit 1)
          order by d.number",
    )
    .bind(debrief)
    .fetch_all(&mut *conn)
    .await?)
}

/// A debrief with research ready on a decision the person hasn't answered
/// since: home says Research ready.
pub async fn research_ready(conn: &mut Conn, debrief: i64) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "select exists (select 1 from research r join decisions d on d.id = r.decision
          where d.debrief = ? and r.state = 'ready' and d.choice = 'look')",
    )
    .bind(debrief)
    .fetch_one(&mut *conn)
    .await?)
}

/// Something that happened in the person's skills, for home's timeline.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SkillEvent {
    /// `made`: a skill started; `added`: a part added to one; `checked`: a
    /// part's check done.
    pub kind: String,
    pub id: i64,
    /// The skill's name.
    pub skill: String,
    /// The part's heading, for a part; '' for a skill.
    pub heading: String,
    /// What a check found: `works`, `no_difference` or `failed`.
    pub verdict: String,
    pub at: i64,
}

/// What happened in the person's skills since a time, the latest first:
/// skills made, candidates accepted into them, and checks done.
pub async fn skill_events(conn: &mut Conn, since: i64, most: i64) -> Result<Vec<SkillEvent>> {
    Ok(sqlx::query_as(
        "select * from (
           select 'made' as kind, s.id, s.name as skill, '' as heading, '' as verdict, s.made_at as at
             from skills s where s.state = 'made' and s.made_at >= ?1
           union all
           select 'added', c.id, coalesce(s.name, ''), c.heading, '', c.decided_at
             from candidates c left join skills s on s.id = c.skill
            where c.state = 'accepted' and c.decided_at >= ?1
           union all
           select 'checked', k.id, s.name, p.heading, coalesce(k.verdict, ''), k.done_at
             from checks k join parts p on p.id = k.part join skills s on s.id = p.skill
            where k.state = 'done' and k.done_at >= ?1
         ) order by at desc, id desc limit ?2",
    )
    .bind(since)
    .bind(most)
    .fetch_all(&mut *conn)
    .await?)
}

/// Parts of the person's skills not checked yet, by skill: what home offers
/// to check. Each skill's name, and how many.
pub async fn unchecked(conn: &mut Conn) -> Result<Vec<(String, i64)>> {
    Ok(sqlx::query_as(
        "select s.name, count(*) from parts p join skills s on s.id = p.skill
          where s.state = 'made' and p.state = 'unchecked' and p.removed_at is null
          group by s.id order by max(p.seen_at) desc",
    )
    .fetch_all(&mut *conn)
    .await?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::debriefs::{self, NewDecision};
    use crate::handovers::{self, NewHandover};
    use crate::memory;

    async fn handover(conn: &mut Conn, session: &str) -> i64 {
        handovers::add(
            conn,
            &NewHandover {
                root: "/w",
                change: "since main",
                session,
                title: "w since main",
                ..NewHandover::default()
            },
            &[],
        )
        .await
        .expect("kept")
    }

    #[tokio::test]
    async fn a_handover_with_no_debrief_is_a_small_change() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let typo = handover(&mut conn, "").await;
        let work = handover(&mut conn, "s").await;
        let id = debriefs::for_handover(&mut conn, work, "/w", "s")
            .await
            .expect("made");
        // More on the same work, before it's sent: the same debrief's.
        let more = handover(&mut conn, "s").await;
        debriefs::for_handover(&mut conn, more, "/w", "s")
            .await
            .expect("the same");
        let small: Vec<i64> = small(&mut conn, 0, 10)
            .await
            .expect("read")
            .iter()
            .map(|h| h.id)
            .collect();
        assert_eq!(small, [typo]);
        assert_eq!(debrief_of(&mut conn, more).await.expect("read"), Some(id));
        assert_eq!(debrief_of(&mut conn, typo).await.expect("read"), None);
        assert!(hook_heard(&mut conn).await.expect("read"));
    }

    #[tokio::test]
    async fn a_call_on_a_decision_can_be_taken_back_and_kept_for_every_time() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let h = handover(&mut conn, "s").await;
        let d = debriefs::for_handover(&mut conn, h, "/w", "s")
            .await
            .expect("made");
        let ids = debriefs::keep_decisions(
            &mut conn,
            d,
            &[NewDecision {
                number: 1,
                title: "Retry only what might pass",
                origin: "own",
                lines: "[]",
                part: "",
            }],
        )
        .await
        .expect("kept");
        assert!(
            decide(&mut conn, ids[0], Some("change"), Some("Retry 429 too."))
                .await
                .expect("chosen")
        );
        assert!(
            every_time(&mut conn, ids[0], true, None)
                .await
                .expect("kept")
        );
        let got = debriefs::decision(&mut conn, ids[0])
            .await
            .expect("read")
            .expect("there");
        assert_eq!(got.choice.as_deref(), Some("change"));
        assert_eq!(got.note.as_deref(), Some("Retry 429 too."));
        assert!(got.every_time && got.chosen_at.is_some());
        decide(&mut conn, ids[0], None, None)
            .await
            .expect("taken back");
        let got = debriefs::decision(&mut conn, ids[0])
            .await
            .expect("read")
            .expect("there");
        assert_eq!((got.choice, got.chosen_at), (None, None));
        assert!(got.every_time, "every time stays as it was");

        // Research asked waits for the agent; ready, home says so.
        let r = debriefs::ask_research(&mut conn, ids[0], "Retry 429?")
            .await
            .expect("asked");
        let waiting = research_waiting(&mut conn).await.expect("read");
        assert_eq!((waiting[0].id, waiting[0].number), (r, 1));
        decide(&mut conn, ids[0], Some("look"), None)
            .await
            .expect("chosen");
        debriefs::answer_research(&mut conn, r, "It found.")
            .await
            .expect("answered");
        assert!(research_waiting(&mut conn).await.expect("read").is_empty());
        assert!(research_ready(&mut conn, d).await.expect("read"));
        assert_eq!(research_of(&mut conn, d).await.expect("read")[0].id, r);
    }

    #[tokio::test]
    async fn a_failed_debrief_can_be_asked_for_again() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let h = handover(&mut conn, "s").await;
        let d = debriefs::for_handover(&mut conn, h, "/w", "s")
            .await
            .expect("made");
        assert!(!ask_again(&mut conn, d).await.expect("not failed"));
        for _ in 0..debriefs::MOST_TRIES {
            debriefs::wrong(&mut conn, d, "No title.")
                .await
                .expect("kept");
        }
        assert!(ask_again(&mut conn, d).await.expect("asked"));
        let got = debriefs::get(&mut conn, d)
            .await
            .expect("read")
            .expect("there");
        assert_eq!((got.state.as_str(), got.tries), ("waiting", 0));
    }
}
