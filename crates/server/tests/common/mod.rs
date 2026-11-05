//! What the server's tests share: a kelve with a fresh store of its own, a
//! stand-in for the person's agent, and a way to run kelve's work until
//! there's none left.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use kelve_graph_api::stand_in::StandIn;
use kelve_server::agent::{self, Caller, HandedIn, Job};
use kelve_server::config::test_config;
use kelve_server::state::{App, Shared};
use serde_json::Value;

/// A kelve with a store of its own, using the stand-in hosted service when
/// there is one.
pub async fn kelve(graph: Option<&StandIn>) -> Shared {
    let mut config = test_config();
    if let Some(g) = graph {
        config.endpoints.graph = g.url.clone();
    }
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate(&pool).await.unwrap();
    App::new(config, pool).await.unwrap()
}

/// A folder of files, in a temporary folder of its own.
pub fn folder(files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kelve-test-{}", uuid::Uuid::now_v7().simple()));
    for (rel, text) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::canonicalize(&root).unwrap()
}

/// The stand-in agent, as it names itself.
pub fn caller() -> Caller {
    Caller {
        agent: "stand-in-agent".into(),
        model: Some("stand-in-model".into()),
        plan: None,
        path: None,
        only: None,
    }
}

/// Makes every background job due now, as if time had passed.
pub async fn due_now(app: &Shared) {
    let mut conn = app.db.acquire().await.unwrap();
    sqlx::query("update jobs set run_at = 0 where locked_at is null")
        .execute(&mut *conn)
        .await
        .unwrap();
}

/// Runs every background job, as if a few seconds passed between rounds:
/// what's due, then what each asked to run soon after, for a few rounds.
pub async fn jobs(app: &Shared) {
    for _ in 0..4 {
        due_now(app).await;
        let mut ran = 0;
        while kelve_server::pipeline::run_once(app)
            .await
            .unwrap()
            .is_some()
        {
            ran += 1;
            assert!(ran < 500, "kelve's jobs never stopped");
        }
        if ran == 0 {
            return;
        }
    }
}

/// Runs kelve's work, with `answer` as the person's agent: the background
/// jobs, then each job kelve keeps for the agent, answered, until there are
/// none. The jobs the agent was given, in order.
pub async fn work(app: &Shared, answer: impl Fn(&Job) -> Value) -> Vec<Job> {
    let mut given = Vec::new();
    for _ in 0..500 {
        jobs(app).await;
        let Some(job) = agent::next(app, &caller()).await.unwrap() else {
            return given;
        };
        let handed = agent::hand_in(app, &caller(), job.job, answer(&job))
            .await
            .unwrap();
        assert_eq!(handed, HandedIn::Yes, "{} wasn't taken", job.about);
        given.push(job);
    }
    panic!("the agent's work never stopped");
}

/// The text of a fenced material string, without its tags.
pub fn unfenced(text: &Value) -> String {
    let text = text.as_str().unwrap_or_default();
    let body = text.split_once('\n').map(|(_, b)| b).unwrap_or(text);
    body.rsplit_once('\n')
        .map(|(b, _)| b)
        .unwrap_or(body)
        .to_string()
}
