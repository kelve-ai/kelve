//! What the reader keeps: what's open, which ideas each file read is built
//! on, and what each folder is built on.

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
    /// The way in the agent gave, as JSON.
    pub way: Option<String>,
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

/// The person's agent was asked to do what's waiting for something opened,
/// by its number: its reading isn't paused, for an hour after the last
/// time, though something else was opened after it (`asks::PAUSED`).
pub async fn wake(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query("update opened set woken_at = ? where id = ?")
        .bind(now())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
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

/// Keeps the way in the agent gave with what it opened, or forgets it.
pub async fn opened_way(conn: &mut Conn, id: i64, way: Option<&str>) -> Result<()> {
    sqlx::query("update opened set way = ? where id = ?")
        .bind(way)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The steps of its way in the person has read, of something opened.
pub async fn steps_read(conn: &mut Conn, id: i64) -> Result<Vec<String>> {
    let kept: Option<String> = sqlx::query_scalar("select steps_read from opened where id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .flatten();
    Ok(kept
        .and_then(|k| serde_json::from_str(&k).ok())
        .unwrap_or_default())
}

/// The person read a step of the way in of something opened. Whether it's
/// new.
pub async fn step_read(conn: &mut Conn, id: i64, path: &str) -> Result<bool> {
    let mut read = steps_read(conn, id).await?;
    if read.iter().any(|p| p == path) {
        return Ok(false);
    }
    read.push(path.to_string());
    sqlx::query("update opened set steps_read = ? where id = ?")
        .bind(serde_json::to_string(&read).unwrap_or_default())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

/// A project's way in, kept here when it can't be kept in the project.
pub async fn kept_way(conn: &mut Conn, root: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("select way from kept_way where root = ?")
            .bind(root)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

pub async fn keep_way(conn: &mut Conn, root: &str, way: &str) -> Result<()> {
    sqlx::query("insert into kept_way (root, way) values (?, ?) on conflict (root) do update set way = excluded.way")
        .bind(root)
        .bind(way)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A file, a folder or a change read plainly, as kept: by its kind, what it's
/// about, what it was written from, and the instructions' version.
pub async fn kept_plainly(
    conn: &mut Conn,
    kind: &str,
    at: &str,
    print: &str,
    version: &str,
) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "select answer from plainly where kind = ? and at = ? and print = ? and version = ?",
    )
    .bind(kind)
    .bind(at)
    .bind(print)
    .bind(version)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The latest kept for what something is about, whatever it was written
/// from: shown while it's written again.
pub async fn latest_plainly(
    conn: &mut Conn,
    kind: &str,
    at: &str,
    version: &str,
) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "select answer from plainly where kind = ? and at = ? and version = ?
          order by written_at desc limit 1",
    )
    .bind(kind)
    .bind(at)
    .bind(version)
    .fetch_optional(&mut *conn)
    .await?)
}

/// What's kept of a kind under the instructions' version: what each is
/// about and was written from, to tell what's read plainly already.
pub async fn plainly_kept(
    conn: &mut Conn,
    kind: &str,
    version: &str,
) -> Result<Vec<(String, String)>> {
    Ok(
        sqlx::query_as("select at, print from plainly where kind = ? and version = ?")
            .bind(kind)
            .bind(version)
            .fetch_all(&mut *conn)
            .await?,
    )
}

pub async fn keep_plainly(
    conn: &mut Conn,
    kind: &str,
    at: &str,
    print: &str,
    version: &str,
    answer: &str,
) -> Result<()> {
    sqlx::query(
        "insert into plainly (kind, at, print, version, answer, written_at) values (?, ?, ?, ?, ?, ?)
         on conflict (kind, at, print, version) do update set answer = excluded.answer, written_at = excluded.written_at",
    )
    .bind(kind)
    .bind(at)
    .bind(print)
    .bind(version)
    .bind(answer)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Keeps that the person asked for something by name in what's opened;
/// asked again, it waits to be seen again.
pub async fn ask(conn: &mut Conn, opened: i64, kind: &str, target: &str) -> Result<()> {
    sqlx::query(
        "insert into asked (opened, kind, target, asked_at) values (?, ?, ?, ?)
         on conflict (opened, kind, target) do update set asked_at = excluded.asked_at, seen_at = null",
    )
    .bind(opened)
    .bind(kind)
    .bind(target)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// What was asked for in what's opened since `since`, seen or not, the
/// latest first: each its kind and target.
pub async fn asked_since(
    conn: &mut Conn,
    opened: i64,
    since: i64,
) -> Result<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "select kind, target from asked where opened = ? and asked_at >= ? order by asked_at desc",
    )
    .bind(opened)
    .bind(since)
    .fetch_all(&mut *conn)
    .await?)
}

/// What was asked for in what's opened and not seen yet, the latest first:
/// each its kind and target.
pub async fn asked(conn: &mut Conn, opened: i64) -> Result<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "select kind, target from asked where opened = ? and seen_at is null order by asked_at desc",
    )
    .bind(opened)
    .fetch_all(&mut *conn)
    .await?)
}

/// Says something asked for has been seen, wherever it was asked for:
/// whether anything was waiting to be.
pub async fn seen(conn: &mut Conn, kind: &str, target: &str) -> Result<bool> {
    let done = sqlx::query(
        "update asked set seen_at = ? where kind = ? and target = ? and seen_at is null",
    )
    .bind(now())
    .bind(kind)
    .bind(target)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

pub async fn get(conn: &mut Conn, id: i64) -> Result<Option<Opened>> {
    Ok(sqlx::query_as(
        "select id, kind, root, path, change, title, opened_at, said, agent, only, way from opened where id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?)
}

/// What was opened lately, latest first.
pub async fn lately(conn: &mut Conn, limit: i64) -> Result<Vec<Opened>> {
    Ok(sqlx::query_as(
        "select id, kind, root, path, change, title, opened_at, said, agent, only, way from opened
          order by opened_at desc, id desc limit ?",
    )
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?)
}

/// Keeps the ideas a file is built on, as JSON, and a hash of the text it
/// held then.
pub async fn read_file(
    conn: &mut Conn,
    root: &str,
    path: &str,
    ideas: &str,
    hash: &str,
) -> Result<()> {
    sqlx::query(
        "insert into read_files (root, path, ideas, read_at, hash) values (?, ?, ?, ?, ?)
         on conflict (root, path) do update set
           ideas = excluded.ideas, read_at = excluded.read_at, skip = null,
           hash = excluded.hash",
    )
    .bind(root)
    .bind(path)
    .bind(ideas)
    .bind(now())
    .bind(hash)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Keeps that a file in a folder couldn't be read, and why, in a sentence.
pub async fn skipped_file(conn: &mut Conn, root: &str, path: &str, skip: &str) -> Result<()> {
    sqlx::query(
        "insert into read_files (root, path, ideas, read_at, skip) values (?, ?, '[]', ?, ?)
         on conflict (root, path) do update set
           ideas = '[]', read_at = excluded.read_at, skip = excluded.skip",
    )
    .bind(root)
    .bind(path)
    .bind(now())
    .bind(skip)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A file read in a folder: the ideas it's built on, as JSON, or why it
/// couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ReadFile {
    pub path: String,
    pub ideas: String,
    pub skip: Option<String>,
    /// A hash of the text it held when it was read.
    pub hash: Option<String>,
}

/// Every file read in a folder.
pub async fn read_files(conn: &mut Conn, root: &str) -> Result<Vec<ReadFile>> {
    Ok(sqlx::query_as(
        "select path, ideas, skip, hash from read_files where root = ? order by path",
    )
    .bind(root)
    .fetch_all(&mut *conn)
    .await?)
}

/// The file read at an absolute path, in the deepest folder read that holds
/// it: that folder, the file's path in it, and the ideas marked in it. Its
/// folders are told apart by the system's separator, and by `/`, which
/// Windows takes as one too.
pub async fn read_at(conn: &mut Conn, file: &str) -> Result<Option<(String, String, String)>> {
    Ok(sqlx::query_as(
        "select root, path, ideas from read_files where replace(root, ?2, '/') || '/' || path = replace(?1, ?2, '/') order by length(root) desc limit 1",
    )
    .bind(file)
    .bind(std::path::MAIN_SEPARATOR_STR)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The deepest folder kelve has read files in that holds `dir`, or is it,
/// with folders told apart as `read_at` tells them.
pub async fn root_of(conn: &mut Conn, dir: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "select root from read_files where root = ?1 or replace(?1, ?2, '/') like replace(root, ?2, '/') || '/%' order by length(root) desc limit 1",
    )
    .bind(dir)
    .bind(std::path::MAIN_SEPARATOR_STR)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The ideas marked in one file of a folder, as kept.
pub async fn ideas_in(conn: &mut Conn, root: &str, path: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("select ideas from read_files where root = ? and path = ?")
            .bind(root)
            .bind(path)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

pub async fn built_on(conn: &mut Conn, fingerprint: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("select chosen from built_on where fingerprint = ?")
            .bind(fingerprint)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// Keeps what a folder is built on, as chosen when `files` of its files
/// had been read.
pub async fn keep_built_on(
    conn: &mut Conn,
    fingerprint: &str,
    root: &str,
    files: i64,
    chosen: &str,
) -> Result<()> {
    sqlx::query(
        "insert into built_on (fingerprint, root, files, chosen, made_at) values (?, ?, ?, ?, ?)
         on conflict (fingerprint) do update set chosen = excluded.chosen, made_at = excluded.made_at",
    )
    .bind(fingerprint)
    .bind(root)
    .bind(files)
    .bind(chosen)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// What a folder was last chosen to be built on, and how many of its files
/// had been read then.
pub async fn last_built_on(conn: &mut Conn, root: &str) -> Result<Option<(String, i64)>> {
    Ok(sqlx::query_as(
        "select chosen, files from built_on where root = ?
         order by made_at desc, rowid desc limit 1",
    )
    .bind(root)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Keeps that the person opened an idea's entry: the first time only.
pub async fn read_idea(conn: &mut Conn, idea: &str) -> Result<()> {
    sqlx::query("insert into ideas_read (idea, first_read) values (?, ?) on conflict do nothing")
        .bind(idea)
        .bind(crate::now())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Every idea the person has opened the entry of, by key.
pub async fn ideas_read(conn: &mut Conn) -> Result<Vec<String>> {
    Ok(
        sqlx::query_scalar("select idea from ideas_read order by first_read")
            .fetch_all(&mut *conn)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;

    #[tokio::test]
    async fn a_file_and_a_folder_are_found_as_the_system_writes_them() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let root = std::env::temp_dir().join("dice");
        let root_s = root.display().to_string();
        read_file(&mut conn, &root_s, "src/lib.rs", "[]", "h")
            .await
            .expect("read");
        let file = root.join("src").join("lib.rs").display().to_string();
        let found = read_at(&mut conn, &file).await.expect("found");
        assert_eq!(
            found,
            Some((root_s.clone(), "src/lib.rs".into(), "[]".into()))
        );
        let inner = root.join("src").display().to_string();
        assert_eq!(
            root_of(&mut conn, &inner).await.expect("found"),
            Some(root_s)
        );
    }

    #[tokio::test]
    async fn the_steps_read_are_kept_with_what_was_opened_once_each() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let a = open(&mut conn, "folder", "/p/kelve", "", "", "kelve")
            .await
            .expect("opened");
        assert!(steps_read(&mut conn, a).await.expect("read").is_empty());
        assert!(step_read(&mut conn, a, "README.md").await.expect("kept"));
        assert!(step_read(&mut conn, a, "src/main.rs").await.expect("kept"));
        assert!(
            !step_read(&mut conn, a, "README.md").await.expect("kept"),
            "once"
        );
        assert_eq!(
            steps_read(&mut conn, a).await.expect("read"),
            ["README.md", "src/main.rs"]
        );
        // Opened again, they're still read; something else has none.
        let again = open(&mut conn, "folder", "/p/kelve", "", "", "kelve")
            .await
            .expect("again");
        assert_eq!(steps_read(&mut conn, again).await.expect("read").len(), 2);
        let other = open(&mut conn, "folder", "/p/other", "", "", "other")
            .await
            .expect("opened");
        assert!(steps_read(&mut conn, other).await.expect("read").is_empty());
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
        assert_eq!(
            get(&mut conn, b)
                .await
                .expect("read")
                .map(|o| o.change)
                .as_deref(),
            Some("uncommitted")
        );
        read_file(&mut conn, "/p/kelve", "src/a.rs", "[]", "h1")
            .await
            .expect("kept");
        read_file(&mut conn, "/p/kelve", "src/a.rs", "[1]", "h2")
            .await
            .expect("replaced");
        skipped_file(&mut conn, "/p/kelve", "b.bin", "This isn’t a text file.")
            .await
            .expect("kept");
        assert_eq!(
            read_files(&mut conn, "/p/kelve").await.expect("read"),
            [
                ReadFile {
                    path: "b.bin".into(),
                    ideas: "[]".into(),
                    skip: Some("This isn’t a text file.".into()),
                    hash: None,
                },
                ReadFile {
                    path: "src/a.rs".into(),
                    ideas: "[1]".into(),
                    skip: None,
                    hash: Some("h2".into()),
                },
            ]
        );
        assert_eq!(
            last_built_on(&mut conn, "/p/kelve").await.expect("read"),
            None
        );
        keep_built_on(&mut conn, "f1", "/p/kelve", 2, "[a]")
            .await
            .expect("kept");
        keep_built_on(&mut conn, "f2", "/p/kelve", 4, "[b]")
            .await
            .expect("kept");
        assert_eq!(
            last_built_on(&mut conn, "/p/kelve").await.expect("read"),
            Some(("[b]".to_string(), 4))
        );
    }

    #[tokio::test]
    async fn seeing_what_was_asked_for_says_whether_it_was_waiting() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let a = open(&mut conn, "folder", "/p/kelve", "", "", "kelve")
            .await
            .expect("opened");
        assert!(
            !seen(&mut conn, "entry", "k1").await.expect("seen"),
            "never asked for"
        );
        ask(&mut conn, a, "entry", "k1").await.expect("asked");
        assert!(seen(&mut conn, "entry", "k1").await.expect("seen"));
        assert!(
            !seen(&mut conn, "entry", "k1").await.expect("seen"),
            "seen already"
        );
        ask(&mut conn, a, "entry", "k1").await.expect("asked again");
        assert!(
            seen(&mut conn, "entry", "k1").await.expect("seen"),
            "waiting again"
        );
    }

    #[tokio::test]
    async fn plainly_is_kept_by_what_its_about_what_it_was_written_from_and_its_version() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        assert_eq!(
            kept_plainly(&mut conn, "file", "h1", "", "plainly@2")
                .await
                .expect("kept"),
            None
        );
        keep_plainly(&mut conn, "file", "h1", "", "plainly@2", "[1]")
            .await
            .expect("kept");
        keep_plainly(&mut conn, "file", "h1", "", "plainly@2", "[2]")
            .await
            .expect("kept");
        assert_eq!(
            kept_plainly(&mut conn, "file", "h1", "", "plainly@2")
                .await
                .expect("kept")
                .as_deref(),
            Some("[2]"),
            "written again, the last stands"
        );
        // In a change, from the text it had before: another.
        assert_eq!(
            kept_plainly(&mut conn, "file", "h1", "b1", "plainly@2")
                .await
                .expect("kept"),
            None
        );
        assert_eq!(
            kept_plainly(&mut conn, "file", "h1", "", "plainly@3")
                .await
                .expect("kept"),
            None,
            "new instructions write it again"
        );
        assert_eq!(
            plainly_kept(&mut conn, "file", "plainly@2")
                .await
                .expect("kept"),
            [("h1".to_string(), String::new())]
        );
        // A folder's latest stands in while it's written again.
        keep_plainly(
            &mut conn,
            "folder",
            "/p:src",
            "m1",
            "plainlyFolder@1",
            "{\"a\":1}",
        )
        .await
        .expect("kept");
        assert_eq!(
            kept_plainly(&mut conn, "folder", "/p:src", "m2", "plainlyFolder@1")
                .await
                .expect("kept"),
            None
        );
        assert_eq!(
            latest_plainly(&mut conn, "folder", "/p:src", "plainlyFolder@1")
                .await
                .expect("kept")
                .as_deref(),
            Some("{\"a\":1}")
        );
    }
}
