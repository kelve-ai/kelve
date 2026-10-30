//! How kelve was used, counted a day at a time, for the weekly count the
//! person may choose to share. Days are the person's own dates, as text:
//! `2026-10-15`.

use crate::{Conn, Result, now};

/// Counts one more of `what` on `day`.
pub async fn add(conn: &mut Conn, day: &str, what: &str) -> Result<()> {
    sqlx::query(
        "insert into counts (day, what, n) values (?, ?, 1)
         on conflict (day, what) do update set n = n + 1",
    )
    .bind(day)
    .bind(what)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Counts `what` for `key` once a day: whether it was counted now.
pub async fn add_once(conn: &mut Conn, day: &str, what: &str, key: &str) -> Result<bool> {
    let new =
        sqlx::query("insert into counted (day, what, key) values (?, ?, ?) on conflict do nothing")
            .bind(day)
            .bind(what)
            .bind(key)
            .execute(&mut *conn)
            .await?
            .rows_affected()
            > 0;
    if new {
        add(conn, day, what).await?;
    }
    Ok(new)
}

/// Every count from `from` to `to`, both days included: day, what, how many.
pub async fn between(conn: &mut Conn, from: &str, to: &str) -> Result<Vec<(String, String, i64)>> {
    Ok(sqlx::query_as(
        "select day, what, n from counts where day >= ? and day <= ? order by day, what",
    )
    .bind(from)
    .bind(to)
    .fetch_all(&mut *conn)
    .await?)
}

/// The first day anything was counted.
pub async fn first_day(conn: &mut Conn) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("select min(day) from counts")
        .fetch_one(&mut *conn)
        .await?)
}

/// Forgets which keys were counted before `day`: only the numbers stay.
pub async fn forget_keys(conn: &mut Conn, day: &str) -> Result<()> {
    sqlx::query("delete from counted where day < ?")
        .bind(day)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The week's count as sent, if it was.
pub async fn sent(conn: &mut Conn, week: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("select report from counts_sent where week = ?")
            .bind(week)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// Keeps a week's count as it was sent.
pub async fn put_sent(conn: &mut Conn, week: &str, report: &str) -> Result<()> {
    sqlx::query(
        "insert into counts_sent (week, report, sent_at) values (?, ?, ?)
         on conflict (week) do nothing",
    )
    .bind(week)
    .bind(report)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The counts sent, newest first: week and report.
pub async fn all_sent(conn: &mut Conn, limit: i64) -> Result<Vec<(String, String)>> {
    Ok(
        sqlx::query_as("select week, report from counts_sent order by week desc limit ?")
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[tokio::test]
    async fn a_thing_counted_once_a_day_is_counted_once() {
        let pool = crate::memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        add(&mut conn, "2026-10-12", "opened.change")
            .await
            .expect("counted");
        add(&mut conn, "2026-10-12", "opened.change")
            .await
            .expect("counted");
        assert!(
            add_once(&mut conn, "2026-10-12", "links.followed", "k1")
                .await
                .expect("once")
        );
        assert!(
            !add_once(&mut conn, "2026-10-12", "links.followed", "k1")
                .await
                .expect("again")
        );
        assert!(
            add_once(&mut conn, "2026-10-13", "links.followed", "k1")
                .await
                .expect("next day")
        );
        let all = between(&mut conn, "2026-10-12", "2026-10-18")
            .await
            .expect("read");
        assert_eq!(
            all,
            vec![
                ("2026-10-12".into(), "links.followed".into(), 1),
                ("2026-10-12".into(), "opened.change".into(), 2),
                ("2026-10-13".into(), "links.followed".into(), 1),
            ]
        );
        assert_eq!(
            first_day(&mut conn).await.expect("read").as_deref(),
            Some("2026-10-12")
        );
        forget_keys(&mut conn, "2026-10-13")
            .await
            .expect("forgotten");
        assert!(
            add_once(&mut conn, "2026-10-12", "links.followed", "k1")
                .await
                .expect("keys gone")
        );
    }
}
