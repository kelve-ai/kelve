//! Each passage's marks, kept against its hash, the prompt and the model
//! (`server::pipeline::mark`).

use crate::{Conn, Result, now};

/// A passage's kept marks, as JSON, if it's been marked this way before.
pub async fn get(
    conn: &mut Conn,
    passage: &str,
    prompt: &str,
    model: &str,
) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar(
            "select marks from marks where passage = ? and prompt = ? and model = ?",
        )
        .bind(passage)
        .bind(prompt)
        .bind(model)
        .fetch_optional(&mut *conn)
        .await?,
    )
}

/// Passages' kept marks, as JSON, by passage: those marked this way before.
pub async fn get_each(
    conn: &mut Conn,
    passages: &[&str],
    prompt: &str,
    model: &str,
) -> Result<std::collections::HashMap<String, String>> {
    let mut found = std::collections::HashMap::new();
    // As many at once as SQLite takes in one statement, and then some.
    for some in passages.chunks(500) {
        let mut query = sqlx::QueryBuilder::new("select passage, marks from marks where prompt = ");
        query.push_bind(prompt);
        query.push(" and model = ");
        query.push_bind(model);
        query.push(" and passage in (");
        let mut list = query.separated(", ");
        for p in some {
            list.push_bind(*p);
        }
        query.push(")");
        let rows: Vec<(String, String)> = query.build_query_as().fetch_all(&mut *conn).await?;
        found.extend(rows);
    }
    Ok(found)
}

/// Keeps a passage's marks, replacing any kept before.
pub async fn put(
    conn: &mut Conn,
    passage: &str,
    prompt: &str,
    model: &str,
    marks: &str,
) -> Result<()> {
    sqlx::query(
        "insert into marks (passage, prompt, model, marks, made_at) values (?, ?, ?, ?, ?)
         on conflict (passage, prompt, model) do update set marks = excluded.marks, made_at = excluded.made_at",
    )
    .bind(passage)
    .bind(prompt)
    .bind(model)
    .bind(marks)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Forgets every passage's marks that lead to this idea in the shared
/// graph, by its ID, so they're marked again when their file next opens:
/// how many passages.
pub async fn forget_idea(conn: &mut Conn, id: &str) -> Result<u64> {
    Ok(sqlx::query("delete from marks where instr(marks, ?) > 0")
        .bind(format!("\"id\":\"{id}\""))
        .execute(&mut *conn)
        .await?
        .rows_affected())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;

    #[tokio::test]
    async fn a_passages_marks_are_kept_by_hash_prompt_and_model() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        assert_eq!(
            get(&mut conn, "h1", "mark-1", "m").await.expect("read"),
            None
        );
        put(&mut conn, "h1", "mark-1", "m", "[]")
            .await
            .expect("kept");
        put(&mut conn, "h1", "mark-1", "m", "[1]")
            .await
            .expect("replaced");
        assert_eq!(
            get(&mut conn, "h1", "mark-1", "m")
                .await
                .expect("read")
                .as_deref(),
            Some("[1]")
        );
        assert_eq!(
            get(&mut conn, "h1", "mark-2", "m").await.expect("read"),
            None
        );
        crate::delete_everything(&mut conn).await.expect("deleted");
        assert_eq!(
            get(&mut conn, "h1", "mark-1", "m").await.expect("read"),
            None
        );
    }
}
