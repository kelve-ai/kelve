//! Ideas kelve knows about, and each one's entry: written here, or taken
//! from the shared graph (`server::pipeline::entries`).

use crate::{Conn, Result, now};

/// An idea kelve knows about.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Idea {
    pub key: String,
    pub name: String,
    pub definition: String,
    pub kind: String,
    pub became: Option<String>,
}

/// An idea's entry, as far as it's got.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Entry {
    pub idea: String,
    pub state: String,
    pub step: String,
    pub plan: Option<String>,
    pub entry: Option<String>,
    pub claims: Option<String>,
    pub revision: Option<String>,
    pub from_graph: bool,
    pub sent: bool,
    pub cost: f64,
    /// What the shared graph's checks found wrong with the revision kept,
    /// as JSON.
    pub problems: Option<String>,
    /// While a replacement is written: the entry it replaces, still shown,
    /// and its revision.
    pub replacing: Option<String>,
    pub replaces: Option<String>,
    /// When the graph was last asked about it.
    pub checked_at: Option<i64>,
    /// Written ahead, before anyone asked: as batches, when that's wanted.
    pub ahead: bool,
    /// The batch a step is waiting on, as JSON.
    pub pending: Option<String>,
    /// How the shared graph's check of it stands, as JSON.
    pub checking: Option<String>,
}

/// Keeps an idea, or what's new about it.
pub async fn keep_idea(conn: &mut Conn, idea: &Idea) -> Result<()> {
    sqlx::query(
        "insert into ideas (key, name, definition, kind, became, updated_at) values (?, ?, ?, ?, ?, ?)
         on conflict (key) do update set name = excluded.name, definition = excluded.definition,
             kind = excluded.kind, became = coalesce(excluded.became, ideas.became),
             updated_at = excluded.updated_at",
    )
    .bind(&idea.key)
    .bind(&idea.name)
    .bind(&idea.definition)
    .bind(&idea.kind)
    .bind(&idea.became)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Ideas whose names or definitions hold `words`, ignoring case: those
/// named so first, then those with a ready entry. Each, and whether its
/// entry is ready.
pub async fn find(conn: &mut Conn, words: &str, limit: i64) -> Result<Vec<(Idea, bool)>> {
    let like = format!(
        "%{}%",
        words
            .to_lowercase()
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let rows: Vec<(String, String, String, String, Option<String>, bool)> = sqlx::query_as(
        "select i.key, i.name, i.definition, i.kind, i.became, coalesce(e.state = 'ready', 0)
           from ideas i left join entries e on e.idea = i.key
          where lower(i.name) like ?1 escape '\\' or lower(i.definition) like ?1 escape '\\'
          order by lower(i.name) like ?1 escape '\\' desc, coalesce(e.state = 'ready', 0) desc,
                   length(i.name), i.name
          limit ?2",
    )
    .bind(like)
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(key, name, definition, kind, became, ready)| {
            (
                Idea {
                    key,
                    name,
                    definition,
                    kind,
                    became,
                },
                ready,
            )
        })
        .collect())
}

pub async fn idea(conn: &mut Conn, key: &str) -> Result<Option<Idea>> {
    Ok(
        sqlx::query_as("select key, name, definition, kind, became from ideas where key = ?")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The new ideas kelve keeps that the graph hasn't taken in yet: what a
/// mark might be, besides the graph's own.
pub async fn new_ideas(conn: &mut Conn) -> Result<Vec<Idea>> {
    Ok(sqlx::query_as(
        "select key, name, definition, kind, became from ideas
         where key like 'new:%' and became is null order by key",
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// The key of the new idea that became the graph's `id`, if one did.
pub async fn became_from(conn: &mut Conn, id: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("select key from ideas where became = ? order by key limit 1")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// Notes the graph's ID a new idea became.
pub async fn became(conn: &mut Conn, key: &str, id: &str) -> Result<()> {
    sqlx::query("update ideas set became = ?, updated_at = ? where key = ?")
        .bind(id)
        .bind(now())
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Notes that the shared graph found an idea to be two: the two, as JSON.
pub async fn split(conn: &mut Conn, key: &str, into: &str) -> Result<()> {
    sqlx::query("update ideas set split_into = ?, updated_at = ? where key = ?")
        .bind(into)
        .bind(now())
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The two an idea was found to be, as JSON, when it was split.
pub async fn split_into(conn: &mut Conn, key: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar::<_, Option<String>>("select split_into from ideas where key = ?")
            .bind(key)
            .fetch_optional(&mut *conn)
            .await?
            .flatten(),
    )
}

pub async fn get(conn: &mut Conn, idea: &str) -> Result<Option<Entry>> {
    Ok(sqlx::query_as(
        "select idea, state, step, plan, entry, claims, revision, from_graph, sent, cost,
                problems, replacing, replaces, checked_at, ahead, pending, checking
           from entries where idea = ?",
    )
    .bind(idea)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Starts an entry for an idea, unless it has one under way or done:
/// whether it started one. A dropped or waiting entry starts again.
pub async fn start(conn: &mut Conn, idea: &str) -> Result<bool> {
    let started = sqlx::query(
        "insert into entries (idea, state, step, created_at, updated_at) values (?, 'writing', 'Kept for your agent', ?, ?)
         on conflict (idea) do update set state = 'writing', step = 'Kept for your agent', plan = null,
             entry = null, claims = null, checking = null, updated_at = excluded.updated_at
          where entries.state in ('dropped', 'waiting')",
    )
    .bind(idea)
    .bind(now())
    .bind(now())
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(started == 1)
}

/// Keeps an entry taken from the shared graph, at its revision.
pub async fn taken(conn: &mut Conn, idea: &str, entry: &str, revision: &str) -> Result<()> {
    sqlx::query(
        "insert into entries (idea, state, entry, revision, from_graph, checked_at, created_at, updated_at)
         values (?, 'ready', ?, ?, 1, ?, ?, ?)
         on conflict (idea) do update set state = 'ready', step = '', entry = excluded.entry,
             revision = excluded.revision, from_graph = 1, sent = 0, problems = null,
             replacing = null, replaces = null, checked_at = excluded.checked_at,
             updated_at = excluded.updated_at",
    )
    .bind(idea)
    .bind(entry)
    .bind(revision)
    .bind(now())
    .bind(now())
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Marks an entry as written ahead, or, once it's asked for, as not.
pub async fn set_ahead(conn: &mut Conn, idea: &str, ahead: bool) -> Result<()> {
    sqlx::query("update entries set ahead = ? where idea = ?")
        .bind(ahead)
        .bind(idea)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Keeps the batch a step of an entry is waiting on, or lets it go.
pub async fn set_pending(conn: &mut Conn, idea: &str, pending: Option<&str>) -> Result<()> {
    sqlx::query("update entries set pending = ? where idea = ?")
        .bind(pending)
        .bind(idea)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Notes that the graph was asked about an entry, and what its checks
/// found wrong with it, if anything.
pub async fn checked(conn: &mut Conn, idea: &str, problems: Option<&str>) -> Result<()> {
    sqlx::query("update entries set problems = ?, checked_at = ? where idea = ?")
        .bind(problems)
        .bind(now())
        .bind(idea)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Keeps how many times the shared graph says an idea's entry was read.
pub async fn set_reads(conn: &mut Conn, idea: &str, reads: i64) -> Result<()> {
    sqlx::query("update entries set reads = ? where idea = ?")
        .bind(reads)
        .bind(idea)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// How many times the shared graph last said an idea's entry was read: 0
/// when it hasn't said.
pub async fn reads(conn: &mut Conn, idea: &str) -> Result<i64> {
    Ok(
        sqlx::query_scalar::<_, i64>("select reads from entries where idea = ?")
            .bind(idea)
            .fetch_optional(&mut *conn)
            .await?
            .unwrap_or(0),
    )
}

/// Starts writing a revision of an entry the graph has: a fix, an extend
/// or a rewrite, keeping the entry shown until then: whether it started
/// one.
pub async fn revise(conn: &mut Conn, idea: &str) -> Result<bool> {
    let started = sqlx::query(
        "update entries set state = 'writing', step = 'Kept for your agent', replacing = entry,
             replaces = revision, from_graph = 0, sent = 0, plan = null, claims = null,
             checking = null, updated_at = ?
          where idea = ? and state = 'ready' and revision is not null and replacing is null",
    )
    .bind(now())
    .bind(idea)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    Ok(started == 1)
}

/// Moves an entry on: its state, and what it's doing or why.
pub async fn set(conn: &mut Conn, idea: &str, state: &str, step: &str) -> Result<()> {
    sqlx::query("update entries set state = ?, step = ?, updated_at = ? where idea = ?")
        .bind(state)
        .bind(step)
        .bind(now())
        .bind(idea)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn set_plan(conn: &mut Conn, idea: &str, plan: &str) -> Result<()> {
    sqlx::query("update entries set plan = ?, updated_at = ? where idea = ?")
        .bind(plan)
        .bind(now())
        .bind(idea)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Keeps a written entry and its claims, to be checked.
pub async fn written(conn: &mut Conn, idea: &str, entry: &str, claims: &str) -> Result<()> {
    sqlx::query(
        "update entries set state = 'checking', step = 'Checking every claim against its source',
             entry = ?, claims = ?, updated_at = ? where idea = ?",
    )
    .bind(entry)
    .bind(claims)
    .bind(now())
    .bind(idea)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Keeps how the shared graph's check of an entry stands, or forgets it.
pub async fn set_checking(conn: &mut Conn, idea: &str, checking: Option<&str>) -> Result<()> {
    sqlx::query("update entries set checking = ?, updated_at = ? where idea = ?")
        .bind(checking)
        .bind(now())
        .bind(idea)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Notes that an entry the graph turned away is to be sent again, once
/// it's fixed.
pub async fn unsent(conn: &mut Conn, idea: &str) -> Result<()> {
    sqlx::query("update entries set sent = 0, revision = null, updated_at = ? where idea = ?")
        .bind(now())
        .bind(idea)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The ideas whose entries the agent wrote that were kept here, never
/// sent: written before the person agreed to add to the shared graph.
pub async fn kept_here(conn: &mut Conn) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select idea from entries where entry is not null and from_graph = 0 and sent = 0
             and state = 'ready' and revision is null order by idea",
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// Notes that an entry was sent to the shared graph, as this revision.
pub async fn sent(conn: &mut Conn, idea: &str, revision: &str, entry: &str) -> Result<()> {
    sqlx::query(
        "update entries set sent = 1, revision = ?, entry = ?, problems = null, replacing = null,
             replaces = null, updated_at = ? where idea = ?",
    )
    .bind(revision)
    .bind(entry)
    .bind(now())
    .bind(idea)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn add_cost(conn: &mut Conn, idea: &str, dollars: f64) -> Result<()> {
    sqlx::query("update entries set cost = cost + ? where idea = ?")
        .bind(dollars)
        .bind(idea)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;

    fn locks() -> Idea {
        Idea {
            key: "new:advisory lock".into(),
            name: "Advisory locks".into(),
            definition: "A lock a program names, and the database keeps.".into(),
            kind: "concept".into(),
            became: None,
        }
    }

    #[tokio::test]
    async fn an_entry_starts_once_and_again_only_when_it_stopped() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        keep_idea(&mut conn, &locks()).await.expect("kept");
        assert!(
            start(&mut conn, "new:advisory lock")
                .await
                .expect("started")
        );
        assert!(
            !start(&mut conn, "new:advisory lock")
                .await
                .expect("under way")
        );
        set(
            &mut conn,
            "new:advisory lock",
            "dropped",
            "It found no sources.",
        )
        .await
        .expect("dropped");
        assert!(start(&mut conn, "new:advisory lock").await.expect("again"));
        let e = get(&mut conn, "new:advisory lock")
            .await
            .expect("read")
            .expect("there");
        assert_eq!(
            (e.state.as_str(), e.step.as_str()),
            ("writing", "Kept for your agent")
        );
        became(&mut conn, "new:advisory lock", "kelve:i000000000001")
            .await
            .expect("noted");
        keep_idea(&mut conn, &locks()).await.expect("kept again");
        let idea = idea(&mut conn, "new:advisory lock")
            .await
            .expect("read")
            .expect("there");
        assert_eq!(
            idea.became.as_deref(),
            Some("kelve:i000000000001"),
            "kept, not forgotten"
        );
        // And back: the new idea the graph's ID came from.
        assert_eq!(
            became_from(&mut conn, "kelve:i000000000001")
                .await
                .expect("read")
                .as_deref(),
            Some("new:advisory lock")
        );
        assert_eq!(
            became_from(&mut conn, "kelve:i000000000009")
                .await
                .expect("read"),
            None
        );
        crate::delete_everything(&mut conn).await.expect("deleted");
        assert_eq!(
            get(&mut conn, "new:advisory lock").await.expect("read"),
            None
        );
    }

    #[tokio::test]
    async fn an_entry_the_graph_has_is_revised_and_shown_until_then() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        keep_idea(&mut conn, &locks()).await.expect("kept");
        let key = "new:advisory lock";
        assert!(!revise(&mut conn, key).await.expect("no entry yet"));
        taken(&mut conn, key, "{\"old\":1}", "r1")
            .await
            .expect("taken");
        assert!(revise(&mut conn, key).await.expect("started"));
        assert!(!revise(&mut conn, key).await.expect("once"));
        let e = get(&mut conn, key).await.expect("read").expect("there");
        assert_eq!(e.state, "writing");
        assert_eq!(e.replacing.as_deref(), Some("{\"old\":1}"));
        assert_eq!(e.replaces.as_deref(), Some("r1"));
        assert!(!e.from_graph);
        sent(&mut conn, key, "r2", "{\"new\":1}")
            .await
            .expect("sent");
        let e = get(&mut conn, key).await.expect("read").expect("there");
        assert_eq!(
            (e.revision.as_deref(), e.replacing, e.problems),
            (Some("r2"), None, None)
        );
    }
}
