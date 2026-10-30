//! Signing a browser in to `kelve serve`: one-time links, and the browsers
//! they signed in. Only hashes are kept; the server makes the tokens and
//! hashes them before they get here.

use chrono::{DateTime, Utc};

use crate::{Conn, Result, ms, now};

/// How long a used or expired link is kept before it's swept.
const KEEP_MS: i64 = 24 * 60 * 60 * 1000;

/// Keeps a new one-time link, valid until `expires_at`.
pub async fn add_link(conn: &mut Conn, hash: &[u8], expires_at: DateTime<Utc>) -> Result<()> {
    sqlx::query("insert into sign_in_links (hash, expires_at) values (?, ?)")
        .bind(hash)
        .bind(ms(expires_at))
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Uses a one-time link: true the first time, while it's valid; false if
/// it was used before, has expired, or never was.
pub async fn use_link(conn: &mut Conn, hash: &[u8]) -> Result<bool> {
    let t = now();
    let used = sqlx::query(
        "update sign_in_links set used_at = ?
         where hash = ? and used_at is null and expires_at > ?",
    )
    .bind(t)
    .bind(hash)
    .bind(t)
    .execute(&mut *conn)
    .await?;
    Ok(used.rows_affected() == 1)
}

pub async fn add_session(
    conn: &mut Conn,
    hash: &[u8],
    expires_at: Option<DateTime<Utc>>,
) -> Result<()> {
    sqlx::query("insert into browser_sessions (hash, expires_at) values (?, ?)")
        .bind(hash)
        .bind(expires_at.map(ms))
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Whether a browser session is good, noting that it was seen.
pub async fn session(conn: &mut Conn, hash: &[u8]) -> Result<bool> {
    let t = now();
    let seen = sqlx::query(
        "update browser_sessions set last_seen_at = ?
         where hash = ? and (expires_at is null or expires_at > ?)",
    )
    .bind(t)
    .bind(hash)
    .bind(t)
    .execute(&mut *conn)
    .await?;
    Ok(seen.rows_affected() == 1)
}

/// Whether a browser was signed in and seen since a time: a tab may be
/// open, and coming back.
pub async fn seen_since(conn: &mut Conn, since: DateTime<Utc>) -> Result<bool> {
    let seen: Option<i64> =
        sqlx::query_scalar("select 1 from browser_sessions where last_seen_at >= ? limit 1")
            .bind(ms(since))
            .fetch_optional(&mut *conn)
            .await?;
    Ok(seen.is_some())
}

pub async fn end_session(conn: &mut Conn, hash: &[u8]) -> Result<()> {
    sqlx::query("delete from browser_sessions where hash = ?")
        .bind(hash)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Sweeps out links and sessions that can no longer be used.
pub async fn sweep(conn: &mut Conn) -> Result<()> {
    let t = now();
    sqlx::query("delete from sign_in_links where expires_at < ?")
        .bind(t - KEEP_MS)
        .execute(&mut *conn)
        .await?;
    sqlx::query("delete from browser_sessions where expires_at < ?")
        .bind(t)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;

    #[tokio::test]
    async fn a_link_works_once_and_only_while_valid() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        add_link(
            &mut conn,
            b"link",
            Utc::now() + chrono::Duration::minutes(1),
        )
        .await
        .expect("kept");
        assert!(use_link(&mut conn, b"link").await.expect("used"));
        assert!(!use_link(&mut conn, b"link").await.expect("used again"));
        add_link(&mut conn, b"old", Utc::now() - chrono::Duration::seconds(1))
            .await
            .expect("kept");
        assert!(!use_link(&mut conn, b"old").await.expect("expired"));
        assert!(!use_link(&mut conn, b"never").await.expect("unknown"));
    }

    #[tokio::test]
    async fn a_session_lasts_until_it_ends_or_expires() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        add_session(&mut conn, b"session", None)
            .await
            .expect("kept");
        assert!(session(&mut conn, b"session").await.expect("checked"));
        end_session(&mut conn, b"session").await.expect("ended");
        assert!(!session(&mut conn, b"session").await.expect("checked"));
        add_session(
            &mut conn,
            b"short",
            Some(Utc::now() - chrono::Duration::seconds(1)),
        )
        .await
        .expect("kept");
        assert!(!session(&mut conn, b"short").await.expect("checked"));
        sweep(&mut conn).await.expect("swept");
    }
}
