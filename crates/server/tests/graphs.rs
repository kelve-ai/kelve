//! Changing the shared graph kelve reads (`KELVE_GRAPH_URL`): each graph
//! has a store of its own, so its own sign-in and what kelve kept from it,
//! and going back to one finds it as it was.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kelve_server::config::{Config, store_name, test_config};
use kelve_server::state::{App, Shared};

const LOCAL: &str = "http://127.0.0.1:4620";
const STAGING: &str = "https://graph.staging.example.com";

/// kelve started on `graph`, with the data folder and sealing key of `base`,
/// as a restart with another `KELVE_GRAPH_URL` makes it.
async fn on(base: &Config, graph: &str) -> Shared {
    let mut config = base.clone();
    config.endpoints.graph = graph.into();
    config.store = config.data.join(store_name(graph));
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate_file(&pool, &config.store)
        .await
        .unwrap();
    App::new(config, pool).await.unwrap()
}

fn handle(app: &Shared) -> Option<String> {
    app.graph_key().and_then(|(_, _, handle)| handle)
}

#[tokio::test]
async fn each_graph_keeps_its_own_sign_in() {
    let base = test_config();

    let local = on(&base, LOCAL).await;
    local
        .hold_graph_key(Some(("local-key", "quiet-heron")))
        .await
        .unwrap();
    drop(local);

    // Another graph has never been signed in to: the local graph's key
    // never goes to it.
    let staging = on(&base, STAGING).await;
    assert_eq!(handle(&staging), None);
    staging
        .hold_graph_key(Some(("staging-key", "brisk-otter")))
        .await
        .unwrap();
    drop(staging);

    // Back on each, its own sign-in is there, with no signing in again.
    let local = on(&base, LOCAL).await;
    assert_eq!(handle(&local).as_deref(), Some("quiet-heron"));
    assert_eq!(
        local.graph_key().map(|(_, key, _)| key).as_deref(),
        Some("local-key")
    );
    drop(local);
    let staging = on(&base, STAGING).await;
    assert_eq!(handle(&staging).as_deref(), Some("brisk-otter"));

    // Signing out of one leaves the other signed in.
    staging.hold_graph_key(None).await.unwrap();
    drop(staging);
    assert_eq!(
        handle(&on(&base, LOCAL).await).as_deref(),
        Some("quiet-heron")
    );

    let _ = std::fs::remove_dir_all(&base.data);
}
