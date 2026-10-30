//! Knowing a newer kelve is out: asked of a local stand-in for where
//! releases are, never GitHub; kept; shown; and told to the agent once for
//! each version, even after a restart.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Json;
use axum::routing::get;
use kelve_server::updates::{self, VERSION};
use serde_json::{Value, json};

/// Where releases are, standing in for GitHub's API: the release it's
/// given, and how many times it was asked.
async fn releases(release: Value) -> (String, Arc<AtomicUsize>) {
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = asked.clone();
    let app = axum::Router::new().route(
        "/latest",
        get(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            let release = release.clone();
            async move { Json(release) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let at = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{at}/latest"), asked)
}

/// A version newer than this kelve's.
fn newer() -> String {
    let v = kelve_core::update::Version::parse(VERSION)
        .unwrap()
        .to_string();
    let major: u64 = v.split('.').next().unwrap().parse().unwrap();
    format!("{}.0.0", major + 1)
}

#[tokio::test]
async fn a_newer_kelve_is_heard_of_kept_shown_and_told_once() {
    let version = newer();
    let (url, asked) = releases(json!({
        "tag_name": format!("v{version}"),
        "body": "## What's Changed\n* Updates come through your agent.\n",
        "assets": [],
    }))
    .await;
    let app = common::kelve(None).await;
    assert!(updates::view(&app).await.unwrap().newer.is_none());
    assert_eq!(
        updates::tell(&app).await.unwrap(),
        None,
        "nothing heard yet"
    );

    // The background check is off in tests; this asks as it would.
    let mut config = app.config.clone();
    config.updates = Some(url.clone());
    let app = kelve_server::state::App::new(config, app.db.clone())
        .await
        .unwrap();
    updates::check(&app).await;
    assert_eq!(asked.load(Ordering::SeqCst), 1);

    let view = updates::view(&app).await.unwrap();
    assert_eq!(view.version, VERSION);
    let newer = view.newer.expect("a newer kelve");
    assert_eq!(newer.version, version);
    assert_eq!(newer.line, "Updates come through your agent.");

    let line = updates::tell(&app).await.unwrap().expect("told once");
    assert_eq!(
        line,
        format!(
            "kelve {version} is out (you have {VERSION}): Updates come through your agent. Ask the person whether to update kelve; if they say yes, call kelve's update tool."
        )
    );
    assert_eq!(updates::tell(&app).await.unwrap(), None, "and only once");

    // Restarted, on the same store: still not told again.
    let again = kelve_server::state::App::new(app.config.clone(), app.db.clone())
        .await
        .unwrap();
    assert_eq!(updates::tell(&again).await.unwrap(), None);
}

#[tokio::test]
async fn a_release_no_newer_shows_nothing() {
    let (url, _) = releases(json!({ "tag_name": format!("v{VERSION}"), "body": "" })).await;
    let app = common::kelve(None).await;
    let mut config = app.config.clone();
    config.updates = Some(url);
    let app = kelve_server::state::App::new(config, app.db.clone())
        .await
        .unwrap();
    updates::check(&app).await;
    assert!(updates::view(&app).await.unwrap().newer.is_none());
    assert_eq!(updates::tell(&app).await.unwrap(), None);
}

#[tokio::test]
async fn releases_are_read_from_the_stand_in_with_their_files() {
    let (url, _) = releases(json!({
        "tag_name": "v9.9.9",
        "body": "Faster.",
        "assets": [{ "name": "SHA256SUMS", "browser_download_url": "http://127.0.0.1:1/SHA256SUMS" }],
    }))
    .await;
    let release = updates::newest(&reqwest::Client::new(), &url)
        .await
        .unwrap();
    assert_eq!(release.version, "9.9.9");
    assert_eq!(release.line, "Faster.");
    assert_eq!(
        release.file("SHA256SUMS"),
        Some("http://127.0.0.1:1/SHA256SUMS")
    );
}
