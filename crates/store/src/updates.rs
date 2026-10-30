//! What kelve last heard of its newest release, and which version the
//! person's agent was last told of.

use crate::{Conn, Result, now};

/// What the last check found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Heard {
    /// When kelve last asked, whatever came of it: 0 for never.
    pub checked_at: i64,
    /// The newest release's version, and the first line of its notes.
    pub newest: Option<(String, String)>,
}

pub async fn heard(conn: &mut Conn) -> Result<Heard> {
    let row: Option<(i64, Option<String>, Option<String>)> =
        sqlx::query_as("select checked_at, newest, line from updates where id = 1")
            .fetch_optional(&mut *conn)
            .await?;
    Ok(
        row.map_or_else(Heard::default, |(checked_at, newest, line)| Heard {
            checked_at,
            newest: newest.map(|v| (v, line.unwrap_or_default())),
        }),
    )
}

/// Notes kelve asked just now, before it hears back: one that fails waits
/// a day too.
pub async fn asked(conn: &mut Conn) -> Result<()> {
    sqlx::query(
        "insert into updates (id, checked_at) values (1, ?)
         on conflict (id) do update set checked_at = excluded.checked_at",
    )
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Keeps the newest release kelve heard of.
pub async fn newest(conn: &mut Conn, version: &str, line: &str) -> Result<()> {
    sqlx::query(
        "insert into updates (id, checked_at, newest, line) values (1, ?, ?, ?)
         on conflict (id) do update set newest = excluded.newest, line = excluded.line",
    )
    .bind(now())
    .bind(version)
    .bind(line)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Whether the agent is to be told of `version` now: true once for each
/// version, and never again after, even across restarts.
pub async fn tell(conn: &mut Conn, version: &str) -> Result<bool> {
    let changed = sqlx::query(
        "update updates set told = ?1 where id = 1 and newest = ?1 and coalesce(told, '') != ?1",
    )
    .bind(version)
    .execute(&mut *conn)
    .await?;
    Ok(changed.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;

    #[tokio::test]
    async fn each_version_is_told_once() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        assert_eq!(heard(&mut conn).await.expect("read"), Heard::default());
        assert!(
            !tell(&mut conn, "0.3.0").await.expect("asked"),
            "nothing heard, nothing told"
        );
        asked(&mut conn).await.expect("asked");
        newest(&mut conn, "0.3.0", "Updates through your agent.")
            .await
            .expect("kept");
        let h = heard(&mut conn).await.expect("read");
        assert!(h.checked_at > 0);
        assert_eq!(
            h.newest,
            Some(("0.3.0".into(), "Updates through your agent.".into()))
        );
        assert!(
            !tell(&mut conn, "0.2.9").await.expect("asked"),
            "only what's newest"
        );
        assert!(tell(&mut conn, "0.3.0").await.expect("asked"));
        assert!(!tell(&mut conn, "0.3.0").await.expect("asked"), "once");
        newest(&mut conn, "0.4.0", "").await.expect("kept");
        assert!(
            tell(&mut conn, "0.4.0").await.expect("asked"),
            "then the next"
        );
    }
}
