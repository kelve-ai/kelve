//! What was sent to the shared graph, and how its checks stand, for what
//! settings say was sent today; and, once the account has added as much as
//! the graph takes a day, until when sending waits.

use crate::{Conn, Result, now};

/// How the graph's check of one send stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sent {
    Checking,
    Accepted,
    TurnedAway,
}

impl Sent {
    fn as_str(self) -> &'static str {
        match self {
            Sent::Checking => "checking",
            Sent::Accepted => "accepted",
            Sent::TurnedAway => "turned_away",
        }
    }
}

/// Notes that an idea's entry was sent, as `revision` when the graph gave
/// it one.
pub async fn record(
    conn: &mut Conn,
    idea: &str,
    revision: Option<&str>,
    state: Sent,
) -> Result<()> {
    sqlx::query("insert into sends (idea, revision, state, sent_at) values (?, ?, ?, ?)")
        .bind(idea)
        .bind(revision)
        .bind(state.as_str())
        .bind(now())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// How the graph's check of the send that became `revision` ended.
pub async fn settle(conn: &mut Conn, revision: &str, state: Sent) -> Result<()> {
    sqlx::query("update sends set state = ? where revision = ?")
        .bind(state.as_str())
        .bind(revision)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Sends since `since`, by how each stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Since {
    pub sent: u32,
    pub accepted: u32,
    pub checking: u32,
    pub turned_away: u32,
}

pub async fn since(conn: &mut Conn, since: i64) -> Result<Since> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("select state, count(*) from sends where sent_at >= ? group by state")
            .bind(since)
            .fetch_all(&mut *conn)
            .await?;
    let mut s = Since::default();
    for (state, n) in rows {
        let n = u32::try_from(n).unwrap_or(u32::MAX);
        s.sent = s.sent.saturating_add(n);
        match state.as_str() {
            "accepted" => s.accepted = n,
            "turned_away" => s.turned_away = n,
            _ => s.checking = n,
        }
    }
    Ok(s)
}

/// Until when sending waits, and why: the day's limit that ran out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub until: i64,
    /// `ideas`, `entries` or `relations`, or empty when the graph didn't
    /// say.
    pub what: String,
    pub new_account: bool,
}

pub async fn held(conn: &mut Conn) -> Result<Option<Held>> {
    let row: Option<(i64, String, i64)> =
        sqlx::query_as("select until, what, new_account from graph_held where id = 1")
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.map(|(until, what, new_account)| Held {
        until,
        what,
        new_account: new_account != 0,
    }))
}

/// Keeps until when sending waits, or forgets it.
pub async fn hold(conn: &mut Conn, held: Option<&Held>) -> Result<()> {
    match held {
        Some(h) => {
            sqlx::query(
                "insert into graph_held (id, until, what, new_account) values (1, ?, ?, ?)
                 on conflict (id) do update set until = excluded.until, what = excluded.what,
                     new_account = excluded.new_account",
            )
            .bind(h.until)
            .bind(&h.what)
            .bind(i64::from(h.new_account))
            .execute(&mut *conn)
            .await?;
        }
        None => {
            sqlx::query("delete from graph_held")
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// Forgets sends older than a week: settings only say what was sent today.
pub async fn sweep(conn: &mut Conn) -> Result<()> {
    sqlx::query("delete from sends where sent_at < ?")
        .bind(now() - 7 * 24 * 60 * 60 * 1000)
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
    async fn sends_are_counted_by_how_their_checks_stand() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let start = now();
        record(&mut conn, "a", Some("r1"), Sent::Checking)
            .await
            .expect("kept");
        record(&mut conn, "b", Some("r2"), Sent::Checking)
            .await
            .expect("kept");
        record(&mut conn, "c", None, Sent::TurnedAway)
            .await
            .expect("kept");
        settle(&mut conn, "r1", Sent::Accepted)
            .await
            .expect("settled");
        assert_eq!(
            since(&mut conn, start).await.expect("counted"),
            Since {
                sent: 3,
                accepted: 1,
                checking: 1,
                turned_away: 1
            }
        );
        assert_eq!(
            since(&mut conn, now() + 1000).await.expect("counted"),
            Since::default()
        );
    }

    #[tokio::test]
    async fn how_long_sending_waits_is_kept_and_forgotten() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        assert_eq!(held(&mut conn).await.expect("read"), None);
        let h = Held {
            until: 42,
            what: "ideas".into(),
            new_account: true,
        };
        hold(&mut conn, Some(&h)).await.expect("kept");
        hold(&mut conn, Some(&h)).await.expect("kept again");
        assert_eq!(held(&mut conn).await.expect("read"), Some(h));
        hold(&mut conn, None).await.expect("forgotten");
        assert_eq!(held(&mut conn).await.expect("read"), None);
    }
}
