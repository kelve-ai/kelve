//! The relations the person followed, each kept once, until it's sent.

use crate::{Conn, Result};

/// A relation followed: from one idea, by a kind of relation, to another.
pub type Followed = (String, String, String);

/// Keeps a relation followed, once: true when it's new.
pub async fn follow(conn: &mut Conn, from: &str, kind: &str, to: &str) -> Result<bool> {
    let done =
        sqlx::query("insert or ignore into follows (from_idea, kind, to_idea) values (?, ?, ?)")
            .bind(from)
            .bind(kind)
            .bind(to)
            .execute(&mut *conn)
            .await?;
    Ok(done.rows_affected() == 1)
}

/// The relations followed that haven't gone, at most `limit`.
pub async fn unsent(conn: &mut Conn, limit: i64) -> Result<Vec<Followed>> {
    Ok(
        sqlx::query_as("select from_idea, kind, to_idea from follows where sent = 0 limit ?")
            .bind(limit)
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// Notes that these have gone.
pub async fn sent(conn: &mut Conn, these: &[Followed]) -> Result<()> {
    for (from, kind, to) in these {
        sqlx::query("update follows set sent = 1 where from_idea = ? and kind = ? and to_idea = ?")
            .bind(from)
            .bind(kind)
            .bind(to)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[tokio::test]
    async fn a_relation_is_kept_once_and_sent_once() {
        let pool = crate::memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        assert!(
            follow(&mut conn, "kelve:a", "restsOn", "kelve:b")
                .await
                .unwrap()
        );
        assert!(
            !follow(&mut conn, "kelve:a", "restsOn", "kelve:b")
                .await
                .unwrap(),
            "once, ever"
        );
        let waiting = unsent(&mut conn, 10).await.unwrap();
        assert_eq!(waiting.len(), 1);
        sent(&mut conn, &waiting).await.unwrap();
        assert!(unsent(&mut conn, 10).await.unwrap().is_empty());
        assert!(
            !follow(&mut conn, "kelve:a", "restsOn", "kelve:b")
                .await
                .unwrap(),
            "not again after it's gone"
        );
    }
}
