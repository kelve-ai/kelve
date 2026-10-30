//! The person's settings, and the keys they pasted in, sealed by the server
//! before they get here.

use crate::{Conn, Result, now};

pub async fn get(conn: &mut Conn) -> Result<serde_json::Value> {
    let text: Option<String> = sqlx::query_scalar("select settings from instance where id = 1")
        .fetch_optional(&mut *conn)
        .await?;
    Ok(text
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({})))
}

pub async fn put(conn: &mut Conn, settings: &serde_json::Value) -> Result<()> {
    sqlx::query(
        "insert into instance (id, settings, updated_at) values (1, ?, ?)
         on conflict (id) do update set settings = excluded.settings, updated_at = excluded.updated_at",
    )
    .bind(settings.to_string())
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn secret(conn: &mut Conn, name: &str) -> Result<Option<Vec<u8>>> {
    Ok(
        sqlx::query_scalar("select value from secrets where name = ?")
            .bind(name)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

pub async fn put_secret(conn: &mut Conn, name: &str, value: &[u8]) -> Result<()> {
    sqlx::query(
        "insert into secrets (name, value, updated_at) values (?, ?, ?)
         on conflict (name) do update set value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(name)
    .bind(value)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn delete_secret(conn: &mut Conn, name: &str) -> Result<()> {
    sqlx::query("delete from secrets where name = ?")
        .bind(name)
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
    async fn settings_and_secrets_are_kept_replaced_and_forgotten() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        assert_eq!(get(&mut conn).await.expect("read"), serde_json::json!({}));
        put(&mut conn, &serde_json::json!({"cap": 2}))
            .await
            .expect("kept");
        assert_eq!(get(&mut conn).await.expect("read")["cap"], 2);
        put_secret(&mut conn, "key", b"one").await.expect("kept");
        put_secret(&mut conn, "key", b"two")
            .await
            .expect("replaced");
        assert_eq!(
            secret(&mut conn, "key").await.expect("read"),
            Some(b"two".to_vec())
        );
        delete_secret(&mut conn, "key").await.expect("forgotten");
        assert_eq!(secret(&mut conn, "key").await.expect("read"), None);
    }
}
