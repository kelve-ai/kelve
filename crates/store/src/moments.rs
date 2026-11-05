//! What the person's agent did as it worked, as kelve's hook saw it: each
//! moment in a session, the files it read or edited, and what's kept for an
//! agent that can't be given text at once.
//! Kept a day.

use crate::{Conn, Result, now};

/// A moment in a session, now: when the one before it was, if there was one
/// in the last day.
pub async fn moment_now(conn: &mut Conn, session: &str) -> Result<Option<i64>> {
    let day_ago = now() - 24 * 60 * 60 * 1000;
    let before: Option<i64> = sqlx::query_scalar("select at from moments where session = ?")
        .bind(session)
        .fetch_optional(&mut *conn)
        .await?;
    sqlx::query(
        "insert into moments (session, at) values (?, ?)
         on conflict (session) do update set at = excluded.at",
    )
    .bind(session)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    sqlx::query("delete from moments where at < ?")
        .bind(day_ago)
        .execute(&mut *conn)
        .await?;
    Ok(before.filter(|at| *at >= day_ago))
}

/// Keeps that the agent read or edited a file in a session.
pub async fn touch(
    conn: &mut Conn,
    session: &str,
    root: &str,
    path: &str,
    edited: bool,
) -> Result<()> {
    let at = now();
    sqlx::query(
        "insert into touched (session, root, path, edited, at, edited_at) values (?, ?, ?, ?, ?, ?)
         on conflict (session, root, path) do update set at = excluded.at,
           edited = max(edited, excluded.edited),
           edited_at = coalesce(excluded.edited_at, edited_at)",
    )
    .bind(session)
    .bind(root)
    .bind(path)
    .bind(edited)
    .bind(at)
    .bind(edited.then_some(at))
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The deepest folder the agent's hook saw it work in that holds `dir`, or
/// is it, with folders told apart however the system writes them.
pub async fn touched_root_of(conn: &mut Conn, dir: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "select root from touched where root = ?1 or replace(?1, ?2, '/') like replace(root, ?2, '/') || '/%' order by length(root) desc limit 1",
    )
    .bind(dir)
    .bind(std::path::MAIN_SEPARATOR_STR)
    .fetch_optional(&mut *conn)
    .await?)
}

/// When the agent last edited a file in a session, and in which folder.
pub async fn last_edit(conn: &mut Conn, session: &str) -> Result<Option<(String, i64)>> {
    Ok(sqlx::query_as(
        "select root, edited_at from touched where session = ? and edited_at is not null
          order by edited_at desc limit 1",
    )
    .bind(session)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The files the agent touched in a session, in the folder it touched last,
/// oldest first; and the one it edited last.
pub async fn touched(
    conn: &mut Conn,
    session: &str,
) -> Result<(String, Vec<String>, Option<String>)> {
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("select root, path, edited from touched where session = ? order by at")
            .bind(session)
            .fetch_all(&mut *conn)
            .await?;
    let Some(root) = rows.last().map(|r| r.0.clone()) else {
        return Ok((String::new(), Vec::new(), None));
    };
    let here: Vec<&(String, String, i64)> = rows.iter().filter(|r| r.0 == root).collect();
    let edited = here.iter().rev().find(|r| r.2 == 1).map(|r| r.1.clone());
    Ok((root, here.iter().map(|r| r.1.clone()).collect(), edited))
}

/// Keeps what kelve has for an agent until it next calls its model.
pub async fn hold_for(conn: &mut Conn, session: &str, text: &str) -> Result<()> {
    sqlx::query("insert into held_for_agent (session, text, at) values (?, ?, ?)")
        .bind(session)
        .bind(text)
        .bind(now())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// What was kept for an agent, oldest first, now given: kept no longer.
pub async fn take_held(conn: &mut Conn, session: &str) -> Result<Vec<String>> {
    let texts: Vec<String> =
        sqlx::query_scalar("select text from held_for_agent where session = ? order by id")
            .bind(session)
            .fetch_all(&mut *conn)
            .await?;
    sqlx::query("delete from held_for_agent where session = ?")
        .bind(session)
        .execute(&mut *conn)
        .await?;
    Ok(texts)
}

/// How long what the hook saw is kept: a day.
pub async fn sweep(conn: &mut Conn) -> Result<()> {
    const DAY: i64 = 24 * 60 * 60 * 1000;
    let at = now();
    for table in ["touched", "held_for_agent"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "delete from {table} where at < ?"
        )))
        .bind(at - DAY)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}
