//! What the server's tests share: a kelve with a fresh store of its own, a
//! stand-in for the person's agent, and a way to run kelve's work until
//! there's none left.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use kelve_graph_api::stand_in::StandIn;
use kelve_server::agent::{self, Caller, HandedIn, Job};
use kelve_server::config::test_config;
use kelve_server::state::{App, Shared};
use serde_json::{Value, json};

/// A kelve with a store of its own, reading the stand-in graph when there
/// is one, and adding to it as the person signed in.
pub async fn kelve(graph: Option<&StandIn>) -> Shared {
    let mut config = test_config();
    if let Some(g) = graph {
        config.endpoints.graph = g.url.clone();
        config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
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

/// The stand-in agent's plainly, for a file, a folder or a change: valid,
/// and no more. A file's parts are its runs of lines between blank ones.
pub fn plainly(kind: &str, material: &Value) -> Option<Value> {
    Some(match kind {
        "plainly" => {
            let count = material["lineCount"].as_u64().unwrap_or(0);
            let blank: Vec<u64> = material["blankLines"]
                .as_array()
                .map(|b| b.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default();
            let mut parts: Vec<(u64, u64)> = Vec::new();
            for n in 1..=count {
                if blank.contains(&n) {
                    continue;
                }
                match parts.last_mut() {
                    Some((_, to)) if *to + 1 == n => *to = n,
                    _ => parts.push((n, n)),
                }
            }
            let parts: Vec<Value> = parts
                .into_iter()
                .map(|(from, to)| serde_json::json!({ "from": from, "to": to, "goal": "Does its part", "says": "Does what these lines say." }))
                .collect();
            serde_json::json!({ "what": "What the file is for.", "parts": parts })
        }
        "plainlyFolder" => {
            let parts: Vec<Value> = material["parts"]
                .as_array()
                .unwrap()
                .iter()
                .take(7)
                .map(|p| serde_json::json!({ "part": p["path"], "goal": "Holds a part", "says": "What it holds." }))
                .collect();
            serde_json::json!({ "what": "What the folder is for.", "shape": "parts", "parts": parts })
        }
        "plainlyChange" => {
            let first = material["files"][0]["path"].clone();
            serde_json::json!({
                "what": "What the change does.", "kind": "updates",
                "parts": [{ "mark": "changed", "goal": "Changes a file", "says": "It changes.", "files": [first] }],
            })
        }
        _ => return None,
    })
}

/// The stand-in agent's way in: every file it's given, in the order kelve
/// lists them, the ones the rest lean on first, up to the most a way in
/// has; no parts, and a line for each folder.
pub fn way(material: &Value) -> Value {
    let order: Vec<Value> = material["files"]
        .as_array()
        .into_iter()
        .flatten()
        .take(40)
        .map(|p| json!({ "path": p, "why": "Where it fits." }))
        .collect();
    let folders: Vec<Value> = material["folders"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| json!({ "path": p, "line": "What it holds." }))
        .collect();
    json!({ "order": order, "parts": [], "folders": folders })
}
