//! What's been opened in kelve: by whom, with what they said of it.

use crate::{Conn, Result, now};

/// Something opened.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Opened {
    pub id: i64,
    pub kind: String,
    pub root: String,
    pub path: String,
    pub change: String,
    pub title: String,
    pub opened_at: i64,
    /// The agent's own line about the work, when an agent opened it.
    pub said: String,
    /// The agent that opened it, or '' for the person.
    pub agent: String,
    /// When only some files of a folder were opened: which, as JSON.
    pub only: Option<String>,
}

/// Opens something, or brings it to the top when it's open already: its id.
pub async fn open(
    conn: &mut Conn,
    kind: &str,
    root: &str,
    path: &str,
    change: &str,
    title: &str,
) -> Result<i64> {
    Ok(sqlx::query_scalar(
        // Each opening is later than the last, even within a millisecond,
        // so the latest is always first.
        "insert into opened (kind, root, path, change, title, opened_at)
         values (?, ?, ?, ?, ?, max(?, coalesce((select max(opened_at) from opened), 0) + 1))
         on conflict (kind, root, path, change) do update set title = excluded.title, opened_at = excluded.opened_at
         returning id",
    )
    .bind(kind)
    .bind(root)
    .bind(path)
    .bind(change)
    .bind(title)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?)
}

/// Opens again something opened before, by its id, bringing it to the top.
/// Whether it moved: not when it's already the latest, or isn't there.
pub async fn again(conn: &mut Conn, id: i64) -> Result<bool> {
    let done = sqlx::query(
        "update opened set opened_at = max(?2, coalesce((select max(opened_at) from opened), 0) + 1)
          where id = ?1
            and exists (select 1 from opened o where o.id != ?1 and o.opened_at >= opened.opened_at)",
    )
    .bind(id)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Says who opened something, what they said of it, and which files.
pub async fn opened_by(
    conn: &mut Conn,
    id: i64,
    agent: &str,
    title: &str,
    said: &str,
    only: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "update opened set agent = ?, title = case when ? = '' then title else ? end, said = ?, only = ? where id = ?",
    )
    .bind(agent)
    .bind(title)
    .bind(title)
    .bind(said)
    .bind(only)
    .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn get(conn: &mut Conn, id: i64) -> Result<Option<Opened>> {
    Ok(sqlx::query_as(
        "select id, kind, root, path, change, title, opened_at, said, agent, only from opened where id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?)
}

/// What was opened lately, latest first.
pub async fn lately(conn: &mut Conn, limit: i64) -> Result<Vec<Opened>> {
    Ok(sqlx::query_as(
        "select id, kind, root, path, change, title, opened_at, said, agent, only from opened
          order by opened_at desc, id desc limit ?",
    )
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?)
}

/// The deepest folder opened in kelve that holds `dir`, or is it, with
/// folders told apart however the system writes them.
pub async fn root_of(conn: &mut Conn, dir: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "select root from opened where root = ?1 or replace(?1, ?2, '/') like replace(root, ?2, '/') || '/%' order by length(root) desc limit 1",
    )
    .bind(dir)
    .bind(std::path::MAIN_SEPARATOR_STR)
    .fetch_optional(&mut *conn)
    .await?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;

    #[tokio::test]
    async fn a_folder_is_found_as_the_system_writes_it() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let root = std::env::temp_dir().join("dice");
        let root_s = root.display().to_string();
        open(&mut conn, "folder", &root_s, "", "", "dice")
            .await
            .expect("opened");
        let inside = root.join("src").display().to_string();
        assert_eq!(
            root_of(&mut conn, &inside).await.expect("found").as_deref(),
            Some(root_s.as_str())
        );
        assert_eq!(
            root_of(&mut conn, &root_s).await.expect("found").as_deref(),
            Some(root_s.as_str())
        );
        let beside = format!("{root_s}-two");
        assert_eq!(root_of(&mut conn, &beside).await.expect("found"), None);
    }

    #[tokio::test]
    async fn opening_again_brings_it_to_the_top() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let a = open(&mut conn, "folder", "/p/kelve", "", "", "kelve")
            .await
            .expect("opened");
        let b = open(
            &mut conn,
            "change",
            "/p/kelve",
            "",
            "uncommitted",
            "Uncommitted changes",
        )
        .await
        .expect("opened");
        assert_ne!(a, b);
        let again = open(&mut conn, "folder", "/p/kelve", "", "", "kelve")
            .await
            .expect("again");
        assert_eq!(again, a);
        let list = lately(&mut conn, 10).await.expect("listed");
        assert_eq!(list[0].id, a, "the latest first");
        assert!(again_moves(&mut conn, b).await, "an older one moves up");
        assert!(!again_moves(&mut conn, b).await, "the latest doesn't");
    }

    async fn again_moves(conn: &mut Conn, id: i64) -> bool {
        again(conn, id).await.expect("again")
    }
}
