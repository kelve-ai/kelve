//! Debriefs (docs/skills.md): one for each piece of work, written by the
//! person's agent as one markdown file; the decisions in each and the
//! person's call on them; research asked on a decision; and explanations of
//! the ideas the work rests on.

use crate::{Conn, Result, now};

/// A debrief, as kept.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Debrief {
    pub id: i64,
    pub handover: Option<i64>,
    pub root: String,
    pub session: String,
    /// `waiting`, `writing`, `ready`, `sent` or `failed`.
    pub state: String,
    pub title: String,
    /// `small` or `large`, '' till it's written.
    pub size: String,
    pub markdown: Option<String>,
    pub tries: i64,
    pub last_error: Option<String>,
    pub made_at: i64,
    pub updated_at: i64,
    pub sent_at: Option<i64>,
}

const DEBRIEF: &str = "select id, handover, root, session, state, title, size, markdown, tries,
     last_error, made_at, updated_at, sent_at from debriefs";

/// The debrief for a handover's piece of work: the one not yet sent for the
/// same session and repository, brought up to the latest handover, or a new
/// one. Its id.
pub async fn for_handover(
    conn: &mut Conn,
    handover: i64,
    root: &str,
    session: &str,
) -> Result<i64> {
    let at = now();
    if !session.is_empty() {
        let open: Option<i64> = sqlx::query_scalar(
            "select id from debriefs where session = ? and root = ? and state != 'sent'
              order by made_at desc limit 1",
        )
        .bind(session)
        .bind(root)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(id) = open {
            sqlx::query(
                "update debriefs set handover = ?, updated_at = ?,
                   state = case when state = 'ready' or state = 'failed' then 'waiting' else state end,
                   tries = 0, last_error = null
                 where id = ?",
            )
            .bind(handover)
            .bind(at)
            .bind(id)
            .execute(&mut *conn)
            .await?;
            cover(conn, handover, id).await?;
            return Ok(id);
        }
    }
    let id: i64 = sqlx::query_scalar(
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
    cover(conn, handover, id).await?;
    Ok(id)
}

/// Keeps that `debrief` covers `handover`, so the handover is never taken
/// for a small change once a debrief has been asked for its work.
async fn cover(conn: &mut Conn, handover: i64, debrief: i64) -> Result<()> {
    sqlx::query("update handovers set debrief = ? where id = ?")
        .bind(debrief)
        .bind(handover)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn get(conn: &mut Conn, id: i64) -> Result<Option<Debrief>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{DEBRIEF} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The latest debriefs, the newest first: what home lists.
pub async fn latest(conn: &mut Conn, most: i64) -> Result<Vec<Debrief>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{DEBRIEF} order by updated_at desc, id desc limit ?"
    )))
    .bind(most)
    .fetch_all(&mut *conn)
    .await?)
}

/// The agent took the debrief's job: it's being written.
pub async fn writing(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query(
        "update debriefs set state = 'writing', updated_at = ? where id = ? and state in ('waiting', 'failed')",
    )
    .bind(now())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The agent's debrief passed kelve's checks: kept, ready to read.
pub async fn written(
    conn: &mut Conn,
    id: i64,
    title: &str,
    size: &str,
    markdown: &str,
) -> Result<()> {
    sqlx::query(
        "update debriefs set state = 'ready', title = ?, size = ?, markdown = ?,
           tries = tries + 1, last_error = null, updated_at = ?
         where id = ?",
    )
    .bind(title)
    .bind(size)
    .bind(markdown)
    .bind(now())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The agent's debrief failed a check: what was wrong, kept for it to fix
/// once; failed after `MOST_TRIES`. Its state now.
pub async fn wrong(conn: &mut Conn, id: i64, why: &str) -> Result<String> {
    Ok(sqlx::query_scalar(
        "update debriefs set tries = tries + 1, last_error = ?, updated_at = ?,
           state = case when tries + 1 >= ? then 'failed' else 'waiting' end
         where id = ? returning state",
    )
    .bind(why)
    .bind(now())
    .bind(MOST_TRIES)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .unwrap_or_default())
}

/// How many times the agent writes a debrief before home says it failed.
pub const MOST_TRIES: i64 = 2;

/// The person sent the debrief to their agent.
pub async fn sent(conn: &mut Conn, id: i64) -> Result<bool> {
    let at = now();
    let done = sqlx::query(
        "update debriefs set state = 'sent', sent_at = ?, updated_at = ? where id = ? and state = 'ready'",
    )
    .bind(at)
    .bind(at)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// A decision in a debrief, and the person's call on it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Decision {
    pub id: i64,
    pub debrief: i64,
    pub number: i64,
    pub title: String,
    /// `own`, `said` or `edit`.
    pub origin: String,
    /// Its lines in the change, as JSON.
    pub lines: String,
    pub part: String,
    /// `agree`, `change` or `look`, once the person chose.
    pub choice: Option<String>,
    pub note: Option<String>,
    pub every_time: bool,
    pub skill: Option<i64>,
    pub chosen_at: Option<i64>,
}

/// A decision as the debrief reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewDecision<'a> {
    pub number: i64,
    pub title: &'a str,
    pub origin: &'a str,
    pub lines: &'a str,
    pub part: &'a str,
}

const DECISION: &str = "select id, debrief, number, title, origin, lines, part, choice, note,
     every_time, skill, chosen_at from decisions";

/// Keeps a debrief's decisions as it reads now. One with the same number
/// and title as before keeps the person's call on it, and its research;
/// the rest are new, and those no longer in it go.
pub async fn keep_decisions(
    conn: &mut Conn,
    debrief: i64,
    decisions: &[NewDecision<'_>],
) -> Result<Vec<i64>> {
    let before = self::decisions(conn, debrief).await?;
    for gone in before.iter().filter(|b| {
        !decisions
            .iter()
            .any(|d| d.number == b.number && d.title == b.title)
    }) {
        sqlx::query("delete from decisions where id = ?")
            .bind(gone.id)
            .execute(&mut *conn)
            .await?;
    }
    let mut ids = Vec::with_capacity(decisions.len());
    for d in decisions {
        let id: i64 = sqlx::query_scalar(
            "insert into decisions (debrief, number, title, origin, lines, part)
             values (?, ?, ?, ?, ?, ?)
             on conflict (debrief, number) do update set
               origin = excluded.origin, lines = excluded.lines, part = excluded.part
             returning id",
        )
        .bind(debrief)
        .bind(d.number)
        .bind(d.title)
        .bind(d.origin)
        .bind(d.lines)
        .bind(d.part)
        .fetch_one(&mut *conn)
        .await?;
        ids.push(id);
    }
    Ok(ids)
}

/// A debrief's decisions, by number.
pub async fn decisions(conn: &mut Conn, debrief: i64) -> Result<Vec<Decision>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{DECISION} where debrief = ? order by number"
    )))
    .bind(debrief)
    .fetch_all(&mut *conn)
    .await?)
}

pub async fn decision(conn: &mut Conn, id: i64) -> Result<Option<Decision>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{DECISION} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The person's call on a decision: `agree`, `change` (with a note to
/// their agent) or `look`; and whether they want it this way every time,
/// in which skill.
pub async fn choose(
    conn: &mut Conn,
    id: i64,
    choice: &str,
    note: Option<&str>,
    every_time: Option<i64>,
) -> Result<bool> {
    let done = sqlx::query(
        "update decisions set choice = ?, note = ?, every_time = ?, skill = ?, chosen_at = ?
         where id = ?",
    )
    .bind(choice)
    .bind(note)
    .bind(every_time.is_some())
    .bind(every_time)
    .bind(now())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Agree with the rest: every decision not answered yet, agreed. How many.
pub async fn agree_with_rest(conn: &mut Conn, debrief: i64) -> Result<u64> {
    let done = sqlx::query(
        "update decisions set choice = 'agree', chosen_at = ? where debrief = ? and choice is null",
    )
    .bind(now())
    .bind(debrief)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected())
}

/// Research asked on a decision.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Research {
    pub id: i64,
    pub decision: i64,
    /// `waiting`, `running`, `ready` or `failed`.
    pub state: String,
    pub question: String,
    pub answer: Option<String>,
    pub asked_at: i64,
    pub answered_at: Option<i64>,
}

/// The person asks their agent to look into a decision: its id.
pub async fn ask_research(conn: &mut Conn, decision: i64, question: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "insert into research (decision, question, asked_at) values (?, ?, ?) returning id",
    )
    .bind(decision)
    .bind(question)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

/// The research on a decision, the latest first.
pub async fn research(conn: &mut Conn, decision: i64) -> Result<Vec<Research>> {
    Ok(sqlx::query_as(
        "select id, decision, state, question, answer, asked_at, answered_at from research
          where decision = ? order by asked_at desc, id desc",
    )
    .bind(decision)
    .fetch_all(&mut *conn)
    .await?)
}

/// Research's state, as the agent takes it or it fails.
pub async fn research_state(conn: &mut Conn, id: i64, state: &str) -> Result<()> {
    sqlx::query("update research set state = ? where id = ?")
        .bind(state)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The agent's answer, in markdown, as it passed kelve's checks.
pub async fn answer_research(conn: &mut Conn, id: i64, answer: &str) -> Result<()> {
    sqlx::query("update research set state = 'ready', answer = ?, answered_at = ? where id = ?")
        .bind(answer)
        .bind(now())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// An explanation of an idea.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Explanation {
    pub id: i64,
    pub debrief: Option<i64>,
    pub idea: String,
    pub key: String,
    pub print: String,
    /// `waiting`, `running`, `ready` or `failed`.
    pub state: String,
    pub answer: Option<String>,
    pub asked_at: i64,
    pub answered_at: Option<i64>,
}

const EXPLANATION: &str = "select id, debrief, idea, key, print, state, answer, asked_at,
     answered_at from explanations";

/// The person asks for an idea to be explained, from a debrief, for the
/// lines that use it (`print`, their hash): the same idea for the same code
/// is explained once, so one asked already is given back, asked again if
/// it failed. Its id.
pub async fn ask_explain(
    conn: &mut Conn,
    debrief: Option<i64>,
    idea: &str,
    print: &str,
) -> Result<i64> {
    let key = idea_key(idea);
    Ok(sqlx::query_scalar(
        "insert into explanations (debrief, idea, key, print, asked_at) values (?, ?, ?, ?, ?)
         on conflict (key, print) do update set
           state = case when explanations.state = 'failed' then 'waiting' else explanations.state end,
           debrief = coalesce(explanations.debrief, excluded.debrief)
         returning id",
    )
    .bind(debrief)
    .bind(idea)
    .bind(&key)
    .bind(print)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

/// An idea's name as explanations are kept by: in lower case, its spaces
/// one.
pub fn idea_key(idea: &str) -> String {
    idea.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The explanation of an idea for some lines, when there's one.
pub async fn explained(conn: &mut Conn, idea: &str, print: &str) -> Result<Option<Explanation>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{EXPLANATION} where key = ? and print = ?"
    )))
    .bind(idea_key(idea))
    .bind(print)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The explanations asked from a debrief.
pub async fn explanations(conn: &mut Conn, debrief: i64) -> Result<Vec<Explanation>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{EXPLANATION} where debrief = ? order by id"
    )))
    .bind(debrief)
    .fetch_all(&mut *conn)
    .await?)
}

/// An explanation's state, as the agent takes it or it fails.
pub async fn explain_state(conn: &mut Conn, id: i64, state: &str) -> Result<()> {
    sqlx::query("update explanations set state = ? where id = ?")
        .bind(state)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The agent's explanation, in markdown.
pub async fn answer_explain(conn: &mut Conn, id: i64, answer: &str) -> Result<()> {
    sqlx::query(
        "update explanations set state = 'ready', answer = ?, answered_at = ? where id = ?",
    )
    .bind(answer)
    .bind(now())
    .bind(id)
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

    async fn handover(conn: &mut Conn, session: &str) -> i64 {
        handovers::add(
            conn,
            &NewHandover {
                root: "/w",
                change: "since main",
                session,
                ..NewHandover::default()
            },
            &[],
        )
        .await
        .expect("kept")
    }

    #[tokio::test]
    async fn one_piece_of_work_has_one_debrief_until_its_sent() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let first = handover(&mut conn, "s").await;
        let id = for_handover(&mut conn, first, "/w", "s")
            .await
            .expect("made");
        writing(&mut conn, id).await.expect("taken");
        assert_eq!(
            wrong(&mut conn, id, "Line 4 isn't in the change.")
                .await
                .expect("kept"),
            "waiting"
        );
        written(
            &mut conn,
            id,
            "Retry downloads",
            "small",
            "## What it did\n",
        )
        .await
        .expect("kept");
        let d = get(&mut conn, id).await.expect("read").expect("there");
        assert_eq!(
            (d.state.as_str(), d.tries, d.last_error),
            ("ready", 2, None)
        );

        // More asked of the same work: the same debrief, brought up to date.
        let second = handover(&mut conn, "s").await;
        assert_eq!(
            for_handover(&mut conn, second, "/w", "s")
                .await
                .expect("found"),
            id
        );
        let d = get(&mut conn, id).await.expect("read").expect("there");
        assert_eq!((d.state.as_str(), d.handover), ("waiting", Some(second)));
        assert!(
            !sent(&mut conn, id).await.expect("asked"),
            "only once ready"
        );
        written(
            &mut conn,
            id,
            "Retry downloads",
            "small",
            "## What it did\n",
        )
        .await
        .expect("kept");
        assert!(sent(&mut conn, id).await.expect("sent"));

        // Sent: the next piece of work gets its own.
        let third = handover(&mut conn, "s").await;
        let next = for_handover(&mut conn, third, "/w", "s")
            .await
            .expect("made");
        assert_ne!(next, id);
        assert_eq!(
            latest(&mut conn, 10)
                .await
                .expect("listed")
                .first()
                .map(|d| d.id),
            Some(next)
        );
        assert_eq!(wrong(&mut conn, next, "a").await.expect("kept"), "waiting");
        assert_eq!(wrong(&mut conn, next, "b").await.expect("kept"), "failed");
    }

    #[tokio::test]
    async fn decisions_keep_the_persons_call_as_the_debrief_is_rewritten() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let h = handover(&mut conn, "s").await;
        let debrief = for_handover(&mut conn, h, "/w", "s").await.expect("made");
        let skill = crate::skills::add(
            &mut conn,
            &crate::skills::NewSkill {
                name: "rust",
                kind: "like",
                work: "rust",
                ..Default::default()
            },
        )
        .await
        .expect("kept");
        let one = NewDecision {
            number: 1,
            title: "Retry only failures that might pass",
            origin: "own",
            lines: r#"[{"path":"src/fetch.rs","from":59,"to":66}]"#,
            part: "",
        };
        let two = NewDecision {
            number: 2,
            title: "Wait longer each time",
            origin: "said",
            ..one.clone()
        };
        let ids = keep_decisions(&mut conn, debrief, &[one.clone(), two.clone()])
            .await
            .expect("kept");
        assert!(
            choose(
                &mut conn,
                ids[0],
                "change",
                Some("Retry 429 too."),
                Some(skill)
            )
            .await
            .expect("chosen")
        );
        let research_id = ask_research(&mut conn, ids[0], "Should 429 be retried?")
            .await
            .expect("asked");
        research_state(&mut conn, research_id, "running")
            .await
            .expect("taken");
        answer_research(&mut conn, research_id, "#### What your agent found\n")
            .await
            .expect("answered");
        let found = research(&mut conn, ids[0]).await.expect("read");
        assert_eq!(found[0].state, "ready");

        // Rewritten: the first stays, the second is another decision now.
        let other = NewDecision {
            number: 2,
            title: "Give up after five tries",
            ..two
        };
        let again = keep_decisions(&mut conn, debrief, &[one, other])
            .await
            .expect("kept");
        assert_eq!(again[0], ids[0]);
        let kept = decisions(&mut conn, debrief).await.expect("read");
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].choice.as_deref(), Some("change"));
        assert!(kept[0].every_time && kept[0].skill == Some(skill));
        assert_eq!(kept[1].title, "Give up after five tries");
        assert_eq!(research(&mut conn, ids[0]).await.expect("read").len(), 1);

        assert_eq!(
            agree_with_rest(&mut conn, debrief).await.expect("agreed"),
            1
        );
        let after = decision(&mut conn, kept[1].id)
            .await
            .expect("read")
            .expect("there");
        assert_eq!(after.choice.as_deref(), Some("agree"));
        assert_eq!(
            decision(&mut conn, kept[0].id)
                .await
                .expect("read")
                .expect("there")
                .choice
                .as_deref(),
            Some("change"),
            "a call already made stays"
        );
    }

    #[tokio::test]
    async fn an_idea_is_explained_once_for_the_same_code() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let h = handover(&mut conn, "s").await;
        let debrief = for_handover(&mut conn, h, "/w", "s").await.expect("made");
        let id = ask_explain(&mut conn, Some(debrief), "Exponential  backoff", "ab12")
            .await
            .expect("asked");
        assert_eq!(
            ask_explain(&mut conn, None, "exponential backoff", "ab12")
                .await
                .expect("asked"),
            id
        );
        assert_ne!(
            ask_explain(&mut conn, None, "exponential backoff", "cd34")
                .await
                .expect("asked"),
            id,
            "other code, another explanation"
        );
        explain_state(&mut conn, id, "failed").await.expect("set");
        ask_explain(&mut conn, None, "Exponential backoff", "ab12")
            .await
            .expect("asked again");
        let e = explained(&mut conn, "exponential backoff", "ab12")
            .await
            .expect("read")
            .expect("there");
        assert_eq!((e.state.as_str(), e.debrief), ("waiting", Some(debrief)));
        answer_explain(&mut conn, id, "Waiting longer after each failure.")
            .await
            .expect("answered");
        let all = explanations(&mut conn, debrief).await.expect("read");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].state, "ready");
    }
}
