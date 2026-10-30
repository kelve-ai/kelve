//! How each idea relates, as kelve keeps it to draw its outline: what the
//! shared graph served, or what this kelve proposed when the graph had none
//! (`server::pipeline::relations`).

use crate::{Conn, Result, now};

/// One idea's relations, as kept.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Kept {
    pub key: String,
    /// The graph API's `IdeaRelations`, as JSON.
    pub relations: String,
    /// `graph` or `proposed`.
    pub source: String,
    pub question: String,
    /// What was proposed, as the graph's changes, until it's sent.
    pub changes: Option<String>,
    pub sent: bool,
    pub fetched_at: i64,
}

/// An idea's relations, if any are kept.
pub async fn get(conn: &mut Conn, key: &str) -> Result<Option<Kept>> {
    Ok(sqlx::query_as::<_, Kept>(
        "select key, relations, source, question, changes, sent, fetched_at
           from idea_relations where key = ?",
    )
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Forgets what's kept about how an idea relates.
pub async fn forget(conn: &mut Conn, key: &str) -> Result<()> {
    sqlx::query("delete from idea_relations where key = ?")
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Keeps what the shared graph served about an idea. What this kelve
/// proposed stays, unsent, until the graph serves some of its own.
pub async fn served(conn: &mut Conn, key: &str, relations: &str) -> Result<()> {
    sqlx::query(
        "insert into idea_relations (key, relations, source, fetched_at) values (?, ?, 'graph', ?)
         on conflict (key) do update set relations = excluded.relations, source = 'graph',
             fetched_at = excluded.fetched_at",
    )
    .bind(key)
    .bind(relations)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Keeps what this kelve proposed for an idea with what the graph added to
/// it, such as what rests on it, while the graph hasn't served its own:
/// still proposed, its changes and whether they're sent as they were.
pub async fn refreshed(conn: &mut Conn, key: &str, relations: &str) -> Result<()> {
    sqlx::query("update idea_relations set relations = ?, fetched_at = ? where key = ?")
        .bind(relations)
        .bind(now())
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Notes that the graph was asked about an idea and served nothing new, so
/// it isn't asked again for a while.
pub async fn asked(conn: &mut Conn, key: &str) -> Result<()> {
    sqlx::query("update idea_relations set fetched_at = ? where key = ?")
        .bind(now())
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Keeps what this kelve proposed for an idea, the question it answers, and
/// the changes that send it to the graph.
pub async fn proposed(
    conn: &mut Conn,
    key: &str,
    relations: &str,
    question: &str,
    changes: &str,
) -> Result<()> {
    sqlx::query(
        "insert into idea_relations (key, relations, source, question, changes, sent, fetched_at)
         values (?, ?, 'proposed', ?, ?, 0, ?)
         on conflict (key) do update set relations = excluded.relations, source = 'proposed',
             question = excluded.question, changes = excluded.changes, sent = 0,
             fetched_at = excluded.fetched_at",
    )
    .bind(key)
    .bind(relations)
    .bind(question)
    .bind(changes)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Marks what was proposed for an idea as sent.
pub async fn sent(conn: &mut Conn, key: &str) -> Result<()> {
    sqlx::query("update idea_relations set sent = 1 where key = ?")
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::memory;

    #[tokio::test]
    async fn what_was_proposed_waits_to_be_sent_and_the_graph_wins_after() {
        let pool = memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        assert!(get(&mut conn, "new:a").await.unwrap().is_none());
        proposed(&mut conn, "new:a", "{}", "How?", "[]")
            .await
            .unwrap();
        let k = get(&mut conn, "new:a").await.unwrap().unwrap();
        assert_eq!(
            (k.source.as_str(), k.sent, k.question.as_str()),
            ("proposed", false, "How?")
        );
        sent(&mut conn, "new:a").await.unwrap();
        served(&mut conn, "new:a", "{\"down\":[]}").await.unwrap();
        let k = get(&mut conn, "new:a").await.unwrap().unwrap();
        assert_eq!(
            (k.source.as_str(), k.question.as_str()),
            ("graph", "How?"),
            "the question stays"
        );
    }
}
