//! kelve's background work: a job that's taken is finished and let go,
//! even when whatever ran it stops waiting part way, as a request does
//! when its caller hangs up.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::time::Duration;

use kelve_server::config::test_config;
use kelve_server::pipeline::{self, relations};
use kelve_server::state::{App, Shared};
use tokio::net::TcpListener;
use tokio::sync::watch;

/// A graph that takes every call and answers none, until it's let go:
/// then it hangs up on each.
async fn stalled_graph() -> (String, watch::Sender<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (go, gone) = watch::channel(false);
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let mut gone = gone.clone();
            tokio::spawn(async move {
                let _ = gone.wait_for(|g| *g).await;
                drop(socket);
            });
        }
    });
    (url, go)
}

async fn kelve(graph: &str) -> Shared {
    let mut config = test_config();
    config.endpoints.graph = graph.into();
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate(&pool).await.unwrap();
    App::new(config, pool).await.unwrap()
}

async fn taken(app: &Shared) -> i64 {
    let mut conn = app.db.acquire().await.unwrap();
    sqlx::query_scalar("select count(*) from jobs where locked_at is not null")
        .fetch_one(&mut *conn)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_job_is_finished_and_let_go_when_whatever_ran_it_stops_waiting() {
    let (graph, go) = stalled_graph().await;
    let app = kelve(&graph).await;
    // An idea the graph has, whose relations kelve asks it for.
    let id = "kelve:i3k9xq2m7twa8";
    let idea = kelve_store::entries::Idea {
        key: id.into(),
        name: "Advisory locks".into(),
        definition: "A lock a program names, and the database keeps.".into(),
        kind: "concept".into(),
        became: None,
    };
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::entries::keep_idea(&mut conn, &idea)
            .await
            .unwrap();
    }
    // Opened: how it relates is asked for.
    relations::ensure(&app, &idea).await.unwrap();

    // The job is taken, and whatever ran it gives up while the graph is
    // slow to answer.
    let ran = tokio::time::timeout(Duration::from_millis(500), pipeline::run_once(&app)).await;
    assert!(ran.is_err(), "still waiting on the graph");
    assert_eq!(taken(&app).await, 1, "the job is taken");

    // The graph answers at last, and the job finishes all the same: it's
    // let go, not left taken until its lease runs out.
    go.send(true).unwrap();
    for _ in 0..200 {
        if taken(&app).await == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the job was left taken");
}
