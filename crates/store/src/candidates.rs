//! Candidates: proposed parts for the person's skills, each with what it
//! rests on (docs/skills.md). Evidence is what the person said, changed or
//! decided, kept short with secrets stripped; it never leaves.

use crate::{Conn, Result, now};

/// A proposed part.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Candidate {
    pub id: i64,
    pub skill: Option<i64>,
    pub work: String,
    /// `like`, `steps`, `watch`, or ''.
    pub section: String,
    pub heading: String,
    pub body: String,
    pub part: Option<i64>,
    /// `new`, `same`, `disagrees` or `rewords`.
    pub relation: String,
    /// `waiting`, `proposed`, `accepted` or `dismissed`.
    pub state: String,
    pub debrief: Option<i64>,
    pub made_at: i64,
    pub decided_at: Option<i64>,
}

/// A candidate to keep.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewCandidate<'a> {
    pub skill: Option<i64>,
    pub work: &'a str,
    pub section: &'a str,
    pub heading: &'a str,
    pub body: &'a str,
    pub part: Option<i64>,
    /// '' for `new`.
    pub relation: &'a str,
    pub debrief: Option<i64>,
}

const CANDIDATE: &str = "select id, skill, work, section, heading, body, part, relation, state,
     debrief, made_at, decided_at from candidates";

pub async fn add(conn: &mut Conn, c: &NewCandidate<'_>) -> Result<i64> {
    let relation = if c.relation.is_empty() {
        "new"
    } else {
        c.relation
    };
    Ok(sqlx::query_scalar(
        "insert into candidates (skill, work, section, heading, body, part, relation, debrief, made_at)
         values (?, ?, ?, ?, ?, ?, ?, ?, ?) returning id",
    )
    .bind(c.skill)
    .bind(c.work)
    .bind(c.section)
    .bind(c.heading)
    .bind(c.body)
    .bind(c.part)
    .bind(relation)
    .bind(c.debrief)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

pub async fn get(conn: &mut Conn, id: i64) -> Result<Option<Candidate>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{CANDIDATE} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The candidates in a state, the earliest first: `waiting` for the learn
/// job, `proposed` for the person.
pub async fn in_state(conn: &mut Conn, state: &str) -> Result<Vec<Candidate>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{CANDIDATE} where state = ? order by made_at, id"
    )))
    .bind(state)
    .fetch_all(&mut *conn)
    .await?)
}

/// What a debrief adds to the person's skills.
pub async fn of_debrief(conn: &mut Conn, debrief: i64) -> Result<Vec<Candidate>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{CANDIDATE} where debrief = ? order by id"
    )))
    .bind(debrief)
    .fetch_all(&mut *conn)
    .await?)
}

/// What's proposed for a skill, not yet decided.
pub async fn for_skill(conn: &mut Conn, skill: i64) -> Result<Vec<Candidate>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{CANDIDATE} where skill = ? and state in ('waiting', 'proposed') order by id"
    )))
    .bind(skill)
    .fetch_all(&mut *conn)
    .await?)
}

/// Where kelve puts a candidate, and what it is to the parts there: the
/// learn job's answer, proposed to the person.
pub async fn propose(
    conn: &mut Conn,
    id: i64,
    skill: i64,
    part: Option<i64>,
    relation: &str,
) -> Result<()> {
    sqlx::query(
        "update candidates set skill = ?, part = ?, relation = ?, state = 'proposed' where id = ?",
    )
    .bind(skill)
    .bind(part)
    .bind(relation)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The person's call: `accepted` or `dismissed`. Accepted into a part, its
/// evidence is that part's too.
pub async fn decide(conn: &mut Conn, id: i64, state: &str, into: Option<i64>) -> Result<()> {
    sqlx::query("update candidates set state = ?, decided_at = ? where id = ?")
        .bind(state)
        .bind(now())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    if let Some(part) = into.filter(|_| state == "accepted") {
        sqlx::query("update evidence set part = ? where candidate = ?")
            .bind(part)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// The longest evidence kept, in characters.
pub const EVIDENCE: usize = 1_200;

/// What a part or candidate rests on.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Evidence {
    pub id: i64,
    /// `message`, `edit` or `decision`.
    pub kind: String,
    pub part: Option<i64>,
    pub candidate: Option<i64>,
    pub session: String,
    pub root: String,
    pub handover: Option<i64>,
    pub path: String,
    pub decision: Option<i64>,
    pub text: String,
    pub at: i64,
}

/// Evidence to keep.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewEvidence<'a> {
    pub kind: &'a str,
    pub part: Option<i64>,
    pub candidate: Option<i64>,
    pub session: &'a str,
    pub root: &'a str,
    pub handover: Option<i64>,
    pub path: &'a str,
    pub decision: Option<i64>,
    pub text: &'a str,
}

const EVIDENCE_ROW: &str = "select id, kind, part, candidate, session, root, handover, path,
     decision, text, at from evidence";

/// Keeps evidence, short, with secrets stripped: its id. The person's edit
/// to a file after a handover is one piece of evidence, brought up to date
/// as they commit again.
pub async fn add_evidence(conn: &mut Conn, e: &NewEvidence<'_>) -> Result<i64> {
    let text = kelve_core::text::clip(kelve_core::redact::redact(e.text).trim(), EVIDENCE);
    Ok(sqlx::query_scalar(
        "insert into evidence (kind, part, candidate, session, root, handover, path, decision, text, at)
         values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         on conflict (handover, path) where kind = 'edit' do update set
           text = excluded.text, at = excluded.at
         returning id",
    )
    .bind(e.kind)
    .bind(e.part)
    .bind(e.candidate)
    .bind(e.session)
    .bind(e.root)
    .bind(e.handover)
    .bind(e.path)
    .bind(e.decision)
    .bind(text)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

/// Evidence not yet behind any candidate or part, the earliest first:
/// what the learn job learns from.
pub async fn unused(conn: &mut Conn, since: i64) -> Result<Vec<Evidence>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{EVIDENCE_ROW} where candidate is null and part is null and at >= ? order by at, id"
    )))
    .bind(since)
    .fetch_all(&mut *conn)
    .await?)
}

/// The evidence a candidate came from now rests behind it.
pub async fn behind(conn: &mut Conn, evidence: &[i64], candidate: i64) -> Result<()> {
    for id in evidence {
        sqlx::query("update evidence set candidate = ? where id = ?")
            .bind(candidate)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// What a part rests on, the latest first.
pub async fn evidence_for_part(conn: &mut Conn, part: i64) -> Result<Vec<Evidence>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{EVIDENCE_ROW} where part = ? order by at desc, id desc"
    )))
    .bind(part)
    .fetch_all(&mut *conn)
    .await?)
}

/// What a candidate rests on, the earliest first.
pub async fn evidence_for(conn: &mut Conn, candidate: i64) -> Result<Vec<Evidence>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{EVIDENCE_ROW} where candidate = ? order by at, id"
    )))
    .bind(candidate)
    .fetch_all(&mut *conn)
    .await?)
}

/// The person's edits after a handover, by file.
pub async fn edits_after(conn: &mut Conn, handover: i64) -> Result<Vec<Evidence>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{EVIDENCE_ROW} where kind = 'edit' and handover = ? order by path"
    )))
    .bind(handover)
    .fetch_all(&mut *conn)
    .await?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::handovers::{self, NewHandover};
    use crate::memory;
    use crate::skills::{self, NewSkill, SeenPart};

    #[tokio::test]
    async fn a_candidate_carries_its_evidence_into_the_part_it_becomes() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let said = add_evidence(
            &mut conn,
            &NewEvidence {
                kind: "message",
                session: "s",
                root: "/w",
                text: "that should exit 1, not 2; token=sk-abcdefghijklmnopqrstuvwxyz012345",
                ..NewEvidence::default()
            },
        )
        .await
        .expect("kept");
        let unused_now = unused(&mut conn, 0).await.expect("read");
        assert_eq!(unused_now.len(), 1);
        assert!(
            !unused_now[0].text.contains("sk-abcdef"),
            "{}",
            unused_now[0].text
        );

        let candidate = add(
            &mut conn,
            &NewCandidate {
                work: "rust",
                section: "like",
                heading: "Bad arguments exit 1",
                body: "### Bad arguments exit 1\nSo scripts can tell.",
                ..NewCandidate::default()
            },
        )
        .await
        .expect("kept");
        behind(&mut conn, &[said], candidate).await.expect("moved");
        assert!(unused(&mut conn, 0).await.expect("read").is_empty());
        assert_eq!(
            evidence_for(&mut conn, candidate)
                .await
                .expect("read")
                .len(),
            1
        );
        assert_eq!(in_state(&mut conn, "waiting").await.expect("read").len(), 1);

        let skill = skills::add(
            &mut conn,
            &NewSkill {
                name: "rust",
                kind: "like",
                work: "rust",
                ..NewSkill::default()
            },
        )
        .await
        .expect("kept");
        propose(&mut conn, candidate, skill, None, "new")
            .await
            .expect("proposed");
        assert_eq!(for_skill(&mut conn, skill).await.expect("read").len(), 1);
        let part = skills::seen(
            &mut conn,
            skill,
            &[SeenPart {
                key: "bad arguments exit 1",
                heading: "Bad arguments exit 1",
                section: "like",
                body_hash: "a",
            }],
        )
        .await
        .expect("seen")[0];
        decide(&mut conn, candidate, "accepted", Some(part))
            .await
            .expect("decided");
        let c = get(&mut conn, candidate)
            .await
            .expect("read")
            .expect("there");
        assert_eq!(c.state, "accepted");
        assert!(c.decided_at.is_some());
        assert!(for_skill(&mut conn, skill).await.expect("read").is_empty());
        let behind_part = evidence_for_part(&mut conn, part).await.expect("read");
        assert_eq!(behind_part.len(), 1);
        assert_eq!(behind_part[0].id, said);
    }

    #[tokio::test]
    async fn an_edit_after_handover_is_one_piece_of_evidence_per_file() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let h = handovers::add(
            &mut conn,
            &NewHandover {
                root: "/w",
                change: "since main",
                ..NewHandover::default()
            },
            &[],
        )
        .await
        .expect("kept");
        let edit = |text| NewEvidence {
            kind: "edit",
            root: "/w",
            handover: Some(h),
            path: "src/main.rs",
            text,
            ..NewEvidence::default()
        };
        let first = add_evidence(&mut conn, &edit("-a\n+b"))
            .await
            .expect("kept");
        let again = add_evidence(&mut conn, &edit("-a\n+c"))
            .await
            .expect("kept");
        assert_eq!(first, again);
        let long = "x".repeat(EVIDENCE * 3);
        add_evidence(
            &mut conn,
            &NewEvidence {
                path: "src/lib.rs",
                ..edit(&long)
            },
        )
        .await
        .expect("kept");
        let edits = edits_after(&mut conn, h).await.expect("read");
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].text.chars().count(), EVIDENCE);
        assert_eq!(edits[1].text, "-a\n+c");
    }
}
