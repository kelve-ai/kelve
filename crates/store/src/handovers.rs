//! What the person's agent handed over, as it showed its work in kelve: the
//! change as kelve read it then, kept to compare with what's committed
//! later (`server::handover`). And what the person said to the agent as it
//! worked, as its hook saw each message, kept a week.

use crate::{Conn, Result, now};

/// A handover, as kept.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Handover {
    pub id: i64,
    pub opened: Option<i64>,
    pub root: String,
    pub branch: String,
    pub base: String,
    pub change: String,
    pub head: String,
    pub uncommitted: bool,
    pub agent: String,
    pub session: String,
    pub title: String,
    pub said: String,
    pub files: i64,
    pub added: i64,
    pub removed: i64,
    /// More was handed over than kelve kept.
    pub cut: bool,
    pub at: i64,
    pub compared_at: Option<i64>,
}

/// A handover to keep.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewHandover<'a> {
    pub opened: Option<i64>,
    pub root: &'a str,
    pub branch: &'a str,
    pub base: &'a str,
    pub change: &'a str,
    pub head: &'a str,
    pub uncommitted: bool,
    pub agent: &'a str,
    pub session: &'a str,
    pub title: &'a str,
    pub said: &'a str,
    pub cut: bool,
}

/// A file handed over: its status, its hunks as JSON, and its text, secrets
/// masked, when it wasn't too big to keep.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct HandedFile {
    pub path: String,
    /// `added`, `modified` or `deleted`.
    pub status: String,
    pub moved_from: Option<String>,
    pub skip: Option<String>,
    pub added: i64,
    pub removed: i64,
    pub hunks: String,
    pub text: Option<String>,
}

const HANDOVER: &str = "select id, opened, root, branch, base, change, head, uncommitted, agent,
     session, title, said, files, added, removed, cut, at, compared_at from handovers";

/// Keeps a handover and its files: its id.
pub async fn add(conn: &mut Conn, h: &NewHandover<'_>, files: &[HandedFile]) -> Result<i64> {
    let added: i64 = files.iter().map(|f| f.added).sum();
    let removed: i64 = files.iter().map(|f| f.removed).sum();
    let id: i64 = sqlx::query_scalar(
        "insert into handovers (opened, root, branch, base, change, head, uncommitted, agent,
           session, title, said, files, added, removed, cut, at)
         values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         returning id",
    )
    .bind(h.opened)
    .bind(h.root)
    .bind(h.branch)
    .bind(h.base)
    .bind(h.change)
    .bind(h.head)
    .bind(h.uncommitted)
    .bind(h.agent)
    .bind(h.session)
    .bind(h.title)
    .bind(h.said)
    .bind(i64::try_from(files.len()).unwrap_or(i64::MAX))
    .bind(added)
    .bind(removed)
    .bind(h.cut)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?;
    for f in files {
        sqlx::query(
            "insert into handover_files (handover, path, status, moved_from, skip, added, removed, hunks, text)
             values (?, ?, ?, ?, ?, ?, ?, ?, ?)
             on conflict (handover, path) do nothing",
        )
        .bind(id)
        .bind(&f.path)
        .bind(&f.status)
        .bind(&f.moved_from)
        .bind(&f.skip)
        .bind(f.added)
        .bind(f.removed)
        .bind(&f.hunks)
        .bind(&f.text)
        .execute(&mut *conn)
        .await?;
    }
    Ok(id)
}

pub async fn get(conn: &mut Conn, id: i64) -> Result<Option<Handover>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{HANDOVER} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The files of a handover, by path.
pub async fn files(conn: &mut Conn, id: i64) -> Result<Vec<HandedFile>> {
    Ok(sqlx::query_as(
        "select path, status, moved_from, skip, added, removed, hunks, text
           from handover_files where handover = ? order by path",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?)
}

/// The latest handover in a repository since a time: what a commit there
/// is compared with.
pub async fn latest_in(conn: &mut Conn, root: &str, since: i64) -> Result<Option<Handover>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{HANDOVER} where root = ? and at >= ? order by at desc, id desc limit 1"
    )))
    .bind(root)
    .bind(since)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The latest handover of a session.
pub async fn latest_of(conn: &mut Conn, session: &str) -> Result<Option<Handover>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{HANDOVER} where session = ? order by at desc, id desc limit 1"
    )))
    .bind(session)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The hook saw the agent open something in kelve, in its session: the
/// handovers made as it opened it, not yet of a session, are that
/// session's.
pub async fn of_session(conn: &mut Conn, opened: i64, session: &str) -> Result<u64> {
    let done = sqlx::query("update handovers set session = ? where opened = ? and session = ''")
        .bind(session)
        .bind(opened)
        .execute(&mut *conn)
        .await?;
    Ok(done.rows_affected())
}

/// The files the agent edited in a session since a time, each with the
/// folder it's in, as its hook saw them (`lessons::touch`): its own work
/// after a handover, not the person's.
pub async fn edited_since(
    conn: &mut Conn,
    session: &str,
    since: i64,
) -> Result<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "select root, path from touched where session = ? and edited_at >= ? order by edited_at",
    )
    .bind(session)
    .bind(since)
    .fetch_all(&mut *conn)
    .await?)
}

/// A commit was compared with a handover, now.
pub async fn compared(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query("update handovers set compared_at = ? where id = ?")
        .bind(now())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Lets go of handovers from before a time, and their files, but those a
/// debrief or a past task is of.
pub async fn sweep(conn: &mut Conn, before: i64) -> Result<u64> {
    let done = sqlx::query(
        "delete from handovers where at < ?
           and id not in (select handover from debriefs where handover is not null)
           and id not in (select handover from past_tasks where handover is not null)",
    )
    .bind(before)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected())
}

/// The longest message kept, in characters.
pub const MESSAGE: usize = 2_000;

/// A message the person sent their agent.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Message {
    pub id: i64,
    pub session: String,
    pub agent: String,
    pub root: String,
    pub text: String,
    pub at: i64,
}

/// Keeps a message the person sent their agent: short, secrets stripped.
/// Its id, or none for an empty one.
pub async fn keep_message(
    conn: &mut Conn,
    session: &str,
    agent: &str,
    root: &str,
    text: &str,
) -> Result<Option<i64>> {
    let text = kelve_core::text::clip(kelve_core::redact::redact(text).trim(), MESSAGE);
    if text.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        sqlx::query_scalar(
            "insert into messages (session, agent, root, text, at) values (?, ?, ?, ?, ?)
             returning id",
        )
        .bind(session)
        .bind(agent)
        .bind(root)
        .bind(text)
        .bind(now())
        .fetch_one(&mut *conn)
        .await?,
    ))
}

/// The messages of a session since a time, the earliest first.
pub async fn messages(conn: &mut Conn, session: &str, since: i64) -> Result<Vec<Message>> {
    Ok(sqlx::query_as(
        "select id, session, agent, root, text, at from messages
          where session = ? and at >= ? order by at, id",
    )
    .bind(session)
    .bind(since)
    .fetch_all(&mut *conn)
    .await?)
}

/// The session the person last sent a message in, in a repository: whose
/// work is handed over there, before the hook says.
pub async fn last_session(conn: &mut Conn, root: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "select session from messages where root = ? order by at desc, id desc limit 1",
    )
    .bind(root)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Lets go of messages from before a time.
pub async fn sweep_messages(conn: &mut Conn, before: i64) -> Result<u64> {
    let done = sqlx::query("delete from messages where at < ?")
        .bind(before)
        .execute(&mut *conn)
        .await?;
    Ok(done.rows_affected())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;

    fn file(path: &str, text: Option<&str>) -> HandedFile {
        HandedFile {
            path: path.into(),
            status: "modified".into(),
            moved_from: None,
            skip: None,
            added: 2,
            removed: 1,
            hunks: "[]".into(),
            text: text.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn a_handover_is_kept_with_its_files_and_found_by_its_session() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let opened = crate::reader::open(&mut conn, "change", "/w", "", "since main", "w")
            .await
            .expect("opened");
        let id = add(
            &mut conn,
            &NewHandover {
                opened: Some(opened),
                root: "/w",
                branch: "retry",
                base: "main",
                change: "since main",
                head: "3f2a9c1",
                uncommitted: true,
                agent: "claude-code",
                title: "Retry failed downloads",
                ..NewHandover::default()
            },
            &[
                file("src/retry.rs", Some("fn retry() {}\n")),
                file("Cargo.lock", None),
            ],
        )
        .await
        .expect("kept");
        let h = get(&mut conn, id).await.expect("read").expect("there");
        assert_eq!((h.files, h.added, h.removed), (2, 4, 2));
        assert!(h.uncommitted && !h.cut && h.session.is_empty());
        let kept = files(&mut conn, id).await.expect("read");
        assert_eq!(kept[0].path, "Cargo.lock");
        assert_eq!(kept[1].text.as_deref(), Some("fn retry() {}\n"));

        assert_eq!(of_session(&mut conn, opened, "s1").await.expect("set"), 1);
        assert_eq!(
            of_session(&mut conn, opened, "s2").await.expect("set"),
            0,
            "a handover's session stays"
        );
        assert_eq!(
            latest_of(&mut conn, "s1")
                .await
                .expect("read")
                .map(|h| h.id),
            Some(id)
        );
        assert_eq!(
            latest_in(&mut conn, "/w", 0)
                .await
                .expect("read")
                .map(|h| h.id),
            Some(id)
        );
        assert!(
            latest_in(&mut conn, "/w", now() + 1000)
                .await
                .expect("read")
                .is_none()
        );
        crate::lessons::touch(&mut conn, "s1", "/w", "src/retry.rs", true)
            .await
            .expect("touched");
        crate::lessons::touch(&mut conn, "s1", "/w", "README.md", false)
            .await
            .expect("touched");
        assert_eq!(
            edited_since(&mut conn, "s1", h.at).await.expect("read"),
            vec![("/w".to_string(), "src/retry.rs".to_string())]
        );
        assert!(
            edited_since(&mut conn, "s1", now() + 1000)
                .await
                .expect("read")
                .is_empty()
        );
        compared(&mut conn, id).await.expect("compared");
        assert!(
            get(&mut conn, id)
                .await
                .expect("read")
                .expect("there")
                .compared_at
                .is_some()
        );

        assert_eq!(sweep(&mut conn, now() + 1000).await.expect("swept"), 1);
        assert!(files(&mut conn, id).await.expect("read").is_empty());
    }

    #[tokio::test]
    async fn messages_are_kept_short_with_secrets_stripped() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let said = "use a loop here, and the key is ghp_abcdefghijklmnopqrstuvwxyz0123456789 ok";
        keep_message(&mut conn, "s", "claude-code", "/w", said)
            .await
            .expect("kept");
        let long = "x".repeat(MESSAGE * 2);
        keep_message(&mut conn, "s", "claude-code", "/w", &long)
            .await
            .expect("kept");
        assert!(
            keep_message(&mut conn, "s", "claude-code", "/w", "  ")
                .await
                .expect("asked")
                .is_none()
        );
        let kept = messages(&mut conn, "s", 0).await.expect("read");
        assert_eq!(kept.len(), 2);
        assert!(kept[0].text.starts_with("use a loop here"));
        assert!(!kept[0].text.contains("ghp_abc"), "{}", kept[0].text);
        assert_eq!(kept[1].text.chars().count(), MESSAGE);
        assert!(
            messages(&mut conn, "other", 0)
                .await
                .expect("read")
                .is_empty()
        );
        assert_eq!(
            last_session(&mut conn, "/w").await.expect("read"),
            Some("s".to_string())
        );
        assert_eq!(last_session(&mut conn, "/x").await.expect("read"), None);
        assert_eq!(
            sweep_messages(&mut conn, now() + 1000)
                .await
                .expect("swept"),
            2
        );
    }
}
