//! kelve's local store: one SQLite file on the person's computer. The
//! schema is in `migrations/`, embedded in the binary and run on start.
//! Every query lives in this crate, grouped by what it's about; each
//! function takes a connection, so callers decide what runs in one
//! transaction. Times are milliseconds since 1970, UTC (`ms`).

pub mod asks;
pub mod auth;
pub mod counts;
pub mod entries;
pub mod follows;
pub mod jobs;
pub mod lessons;
pub mod marks;
pub mod reader;
pub mod relations;
pub mod sends;
pub mod settings;
pub mod updates;

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
pub use sqlx::{Sqlite, SqliteConnection as Conn, SqlitePool as Pool, Transaction};

/// Why a query failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error(
        "kelve's data was made by a newer kelve than this one, so this one can't read it. Run `kelve update`, then start kelve again."
    )]
    Newer,
    #[error(
        "kelve couldn't copy its data aside before updating it, to {0}: {1}. Check there's room on the disk, then start kelve again."
    )]
    Copy(String, String),
}

pub type Result<T> = std::result::Result<T, Error>;

static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// A time as the store keeps it.
pub fn ms(at: DateTime<Utc>) -> i64 {
    at.timestamp_millis()
}

/// A time the store kept.
pub fn at(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap_or_default()
}

/// Now, as the store keeps it.
pub fn now() -> i64 {
    ms(Utc::now())
}

fn options(url: &str) -> Result<SqliteConnectOptions> {
    Ok(SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        // Readers never wait for the writer, and a crash loses nothing
        // that was committed.
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10)))
}

/// Opens the store at `path`, making it if it isn't there. Only its owner
/// can read the folder it's in.
pub async fn open(path: &Path) -> Result<Pool> {
    if let Some(dir) = path.parent() {
        private_dir(dir).map_err(|e| Error::Sql(sqlx::Error::Io(e)))?;
    }
    let options = options("sqlite://")?.filename(path);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;
    Ok(pool)
}

/// A store that lives in memory, for tests: one connection, so every query
/// sees the same database.
pub async fn memory() -> Result<Pool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options("sqlite::memory:")?)
        .await?;
    migrate(&pool).await?;
    Ok(pool)
}

/// Makes a folder only its owner can open, if it isn't there.
fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Brings the schema up to date. Safe to run on every start. Data made by a
/// newer kelve, with migrations this one doesn't know, is refused plainly.
pub async fn migrate(pool: &Pool) -> Result<()> {
    use sqlx::migrate::MigrateError;
    match MIGRATIONS.run(pool).await {
        Err(MigrateError::VersionMissing(_)) => Err(Error::Newer),
        other => Ok(other?),
    }
}

/// How many copies of the store are kept from before migrations: the last
/// two.
const COPIES: usize = 2;

/// Brings the store at `path` up to date, as `migrate` does. When this
/// kelve brings migrations a store made before doesn't have, it copies the
/// store aside first, beside it (`kelve.db.before-<ms>`), keeping the last
/// two. Data made by a newer kelve is refused before anything is touched.
pub async fn migrate_file(pool: &Pool, path: &Path) -> Result<()> {
    let applied = applied(pool).await?;
    let known: Vec<i64> = MIGRATIONS.iter().map(|m| m.version).collect();
    if applied.iter().any(|v| !known.contains(v)) {
        return Err(Error::Newer);
    }
    if !applied.is_empty() && known.iter().any(|v| !applied.contains(v)) {
        copy_aside(pool, path).await?;
    }
    migrate(pool).await
}

/// The migrations a store has had: none for a new one.
async fn applied(pool: &Pool) -> Result<Vec<i64>> {
    let made: Option<String> = sqlx::query_scalar(
        "select name from sqlite_master where type = 'table' and name = '_sqlx_migrations'",
    )
    .fetch_optional(pool)
    .await?;
    if made.is_none() {
        return Ok(Vec::new());
    }
    Ok(
        sqlx::query_scalar("select version from _sqlx_migrations where success = 1")
            .fetch_all(pool)
            .await?,
    )
}

/// Copies the store whole, consistent while it's open, beside itself, and
/// keeps only the last copies.
async fn copy_aside(pool: &Pool, path: &Path) -> Result<()> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Ok(());
    };
    let prefix = format!("{}.before-", name.to_string_lossy());
    let copy = dir.join(format!("{prefix}{}", now()));
    sqlx::query("vacuum into ?")
        .bind(copy.to_string_lossy().into_owned())
        .execute(pool)
        .await
        .map_err(|e| Error::Copy(copy.display().to_string(), e.to_string()))?;
    tracing::info!(copy = %copy.display(), "copied kelve's data aside before updating it");
    let mut copies: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with(&prefix))
                })
                .collect()
        })
        .unwrap_or_default();
    // Named by when they were made, so the newest sort last.
    copies.sort();
    let old = copies.len().saturating_sub(COPIES);
    for gone in &copies[..old] {
        if let Err(e) = std::fs::remove_file(gone) {
            tracing::warn!(copy = %gone.display(), error = %e, "couldn't remove an old copy of kelve's data");
        }
    }
    Ok(())
}

/// What Delete everything empties: every table but those in [`KEPT`].
/// Children before what they point at.
const EVERYTHING: [&str; 39] = [
    "jobs",
    "marks",
    "entries",
    "ideas",
    "idea_relations",
    "opened",
    "read_files",
    "built_on",
    "secrets",
    "counts",
    "counted",
    "counts_sent",
    "ideas_read",
    "sends",
    "graph_held",
    "walked_roots",
    "walked_files",
    "kept_way",
    "plainly",
    "asked",
    "asks",
    "agents",
    "follows",
    "uses",
    "libraries",
    "lesson_ideas",
    "lessons",
    "lessons_fetched",
    "learned",
    "given",
    "held_for_agent",
    "failed",
    "files_seen",
    "touched",
    "replaced",
    "written",
    "moments",
    "notes_told",
    "sign_in_links",
];

/// What stays: kelve's own row, whose settings are emptied instead; the
/// browsers signed in, so the page that asked stays signed in; and what
/// kelve last heard of its newest release, which is nothing of anyone's.
/// Named so the test can tell a new table was left out of both.
#[cfg(test)]
const KEPT: [&str; 3] = ["instance", "browser_sessions", "updates"];

/// Deletes everything kelve keeps: settings, keys, marks, ideas, entries,
/// lessons, readings and work in hand. The browsers signed in stay signed
/// in.
pub async fn delete_everything(conn: &mut Conn) -> Result<()> {
    for table in EVERYTHING {
        sqlx::query(sqlx::AssertSqlSafe(format!("delete from {table}")))
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("update instance set settings = '{}', updated_at = ?")
        .bind(now())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[tokio::test]
    async fn delete_everything_names_every_table_as_emptied_or_kept() {
        let pool = memory().await.expect("opened");
        let tables: Vec<String> = sqlx::query_scalar(
            "select name from sqlite_master where type = 'table' \
             and name not like 'sqlite_%' and name != '_sqlx_migrations'",
        )
        .fetch_all(&pool)
        .await
        .expect("listed");
        let named: Vec<&str> = EVERYTHING.iter().chain(KEPT.iter()).copied().collect();
        let missed: Vec<&String> = tables
            .iter()
            .filter(|t| !named.contains(&t.as_str()))
            .collect();
        assert!(
            missed.is_empty(),
            "add these to EVERYTHING or KEPT: {missed:?}"
        );
        let gone: Vec<&&str> = named
            .iter()
            .filter(|t| !tables.iter().any(|n| n == **t))
            .collect();
        assert!(gone.is_empty(), "no such tables: {gone:?}");
        let mut conn = pool.acquire().await.expect("conn");
        delete_everything(&mut conn).await.expect("deleted");
    }

    #[tokio::test]
    async fn a_store_is_made_private_and_migrates_once() {
        let dir =
            std::env::temp_dir().join(format!("kelve-store-{}", uuid::Uuid::now_v7().simple()));
        let path = dir.join("kelve.db");
        let pool = open(&path).await.expect("opened");
        migrate(&pool).await.expect("migrated");
        migrate(&pool).await.expect("migrated again");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&dir).expect("there").permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
        pool.close().await;
        std::fs::remove_dir_all(&dir).expect("tidied");
    }

    fn copies_in(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .expect("there")
            .filter(|e| {
                e.as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().contains(".before-"))
            })
            .count()
    }

    #[tokio::test]
    async fn data_from_a_newer_kelve_is_refused_saying_what_to_do() {
        let dir =
            std::env::temp_dir().join(format!("kelve-newer-{}", uuid::Uuid::now_v7().simple()));
        let path = dir.join("kelve.db");
        let pool = open(&path).await.expect("opened");
        migrate_file(&pool, &path).await.expect("migrated");
        // A migration only a newer kelve has.
        sqlx::query(
            "insert into _sqlx_migrations (version, description, success, checksum, execution_time)
             values (99990101, 'from the future', 1, x'00', 0)",
        )
        .execute(&pool)
        .await
        .expect("added");
        let refused = migrate_file(&pool, &path).await;
        assert!(matches!(refused, Err(Error::Newer)), "{refused:?}");
        let said = refused.err().map(|e| e.to_string()).unwrap_or_default();
        assert!(said.contains("`kelve update`"), "{said}");
        assert!(matches!(migrate(&pool).await, Err(Error::Newer)));
        assert_eq!(copies_in(&dir), 0, "nothing copied for a store it refuses");
        pool.close().await;
        std::fs::remove_dir_all(&dir).expect("tidied");
    }

    #[tokio::test]
    async fn the_store_is_copied_aside_keeping_the_last_two() {
        let dir =
            std::env::temp_dir().join(format!("kelve-copies-{}", uuid::Uuid::now_v7().simple()));
        let path = dir.join("kelve.db");
        let pool = open(&path).await.expect("opened");
        migrate_file(&pool, &path).await.expect("migrated");
        assert_eq!(copies_in(&dir), 0, "a new store has nothing to copy");
        migrate_file(&pool, &path).await.expect("migrated again");
        assert_eq!(copies_in(&dir), 0, "nor one already up to date");
        sqlx::query("insert into updates (id, checked_at, newest) values (1, 1, '0.3.0')")
            .execute(&pool)
            .await
            .expect("kept");
        for _ in 0..3 {
            copy_aside(&pool, &path).await.expect("copied");
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        assert_eq!(copies_in(&dir), COPIES);
        let copy = std::fs::read_dir(&dir)
            .expect("there")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .find(|p| p.to_string_lossy().contains(".before-"))
            .expect("a copy");
        let copied = open(&copy).await.expect("it opens");
        let newest: String = sqlx::query_scalar("select newest from updates")
            .fetch_one(&copied)
            .await
            .expect("with what was kept");
        assert_eq!(newest, "0.3.0");
        copied.close().await;
        pool.close().await;
        std::fs::remove_dir_all(&dir).expect("tidied");
    }

    #[test]
    fn times_round_trip() {
        let t = Utc::now();
        assert_eq!(at(ms(t)).timestamp_millis(), t.timestamp_millis());
    }
}
