//! The verified runs this kelve asked kelve.ai for: what each is, and how
//! it stands, as kelve.ai last said. Never the skills that went, or the
//! key that paid.

use crate::{Conn, Result, now};

/// A run, as kelve keeps it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Run {
    pub id: String,
    pub kind: String,
    pub kind_name: String,
    pub agent: String,
    pub agent_name: String,
    pub model: String,
    pub model_name: String,
    pub provider: String,
    pub provider_name: String,
    pub state: String,
    pub score: Option<String>,
    pub at: i64,
}

const RUN: &str = "select id, kind, kind_name, agent, agent_name, model, model_name, provider, provider_name, state, score, at from hosted_runs";

/// Keeps a run kelve.ai estimated, or the same again.
pub async fn add(conn: &mut Conn, r: &Run) -> Result<()> {
    sqlx::query(
        "insert into hosted_runs (id, kind, kind_name, agent, agent_name, model, model_name, provider, provider_name, state, score, at)
         values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         on conflict (id) do update set state = excluded.state",
    )
    .bind(&r.id)
    .bind(&r.kind)
    .bind(&r.kind_name)
    .bind(&r.agent)
    .bind(&r.agent_name)
    .bind(&r.model)
    .bind(&r.model_name)
    .bind(&r.provider)
    .bind(&r.provider_name)
    .bind(&r.state)
    .bind(&r.score)
    .bind(if r.at > 0 { r.at } else { now() })
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn get(conn: &mut Conn, id: &str) -> Result<Option<Run>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{RUN} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// How a run stands now, and its score once it has one.
pub async fn stands(conn: &mut Conn, id: &str, state: &str, score: Option<&str>) -> Result<()> {
    sqlx::query("update hosted_runs set state = ?, score = coalesce(?, score) where id = ?")
        .bind(state)
        .bind(score)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The runs that finished with a score since `since` (ms), the latest first.
pub async fn scored_since(conn: &mut Conn, since: i64, most: i64) -> Result<Vec<Run>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{RUN} where state = 'finished' and score is not null and at >= ? order by at desc limit ?"
    )))
    .bind(since)
    .bind(most)
    .fetch_all(&mut *conn)
    .await?)
}

/// The latest run that's confirmed and not finished: the one to go back to.
pub async fn going(conn: &mut Conn) -> Result<Option<Run>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{RUN} where state in ('queued', 'running') order by at desc limit 1"
    )))
    .fetch_optional(&mut *conn)
    .await?)
}

/// The names of the secrets kept under `prefix`, in order: which
/// providers' keys are kept, say. Never the secrets.
pub async fn secrets_named(conn: &mut Conn, prefix: &str) -> Result<Vec<String>> {
    let like = format!("{}%", prefix.replace('%', "").replace('_', "\\_"));
    Ok(
        sqlx::query_scalar("select name from secrets where name like ? escape '\\' order by name")
            .bind(like)
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// Forgets every run: the account they were for is gone.
pub async fn forget(conn: &mut Conn) -> Result<()> {
    sqlx::query("delete from hosted_runs")
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn run(id: &str) -> Run {
        Run {
            id: id.into(),
            kind: "rust".into(),
            kind_name: "Rust".into(),
            agent: "claude-code".into(),
            agent_name: "Claude Code".into(),
            model: "claude-sonnet-5-6".into(),
            model_name: "Sonnet 5.6".into(),
            provider: "anthropic".into(),
            provider_name: "Anthropic".into(),
            state: "estimated".into(),
            score: None,
            at: 0,
        }
    }

    #[tokio::test]
    async fn a_run_is_kept_and_found_while_it_goes() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        add(&mut conn, &run("run1")).await.unwrap();
        assert_eq!(going(&mut conn).await.unwrap(), None);
        stands(&mut conn, "run1", "running", None).await.unwrap();
        assert_eq!(going(&mut conn).await.unwrap().unwrap().id, "run1");
        stands(&mut conn, "run1", "finished", Some("s2"))
            .await
            .unwrap();
        let r = get(&mut conn, "run1").await.unwrap().unwrap();
        assert_eq!(
            (r.state.as_str(), r.score.as_deref()),
            ("finished", Some("s2"))
        );
        assert_eq!(going(&mut conn).await.unwrap(), None);
        crate::settings::put_secret(&mut conn, "keys.provider.anthropic", b"x")
            .await
            .unwrap();
        crate::settings::put_secret(&mut conn, "keys.graph", b"x")
            .await
            .unwrap();
        assert_eq!(
            secrets_named(&mut conn, "keys.provider.").await.unwrap(),
            ["keys.provider.anthropic"]
        );
        forget(&mut conn).await.unwrap();
        assert_eq!(get(&mut conn, "run1").await.unwrap(), None);
    }
}
