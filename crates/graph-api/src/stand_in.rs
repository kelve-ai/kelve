//! A stand-in for kelve's hosted service, held in memory, for tests and for
//! working on kelve without the real one: version 2 of its API, as kelve
//! uses it (`v2::stand_in`), and from version 1 only what kelve still sends,
//! the weekly counts.
//!
//! Run on its own (`kelve-graph-stand-in`), it serves on a port of its own
//! and forgets everything when it stops.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};

use crate::{Counts, Problem};

/// What the stand-in holds.
#[derive(Debug, Default)]
struct Held {
    /// The counts kelves shared.
    counts: Vec<Counts>,
}

/// A stand-in service, serving on a port of its own.
#[derive(Clone, Debug)]
pub struct StandIn {
    pub url: String,
    held: Arc<Mutex<Held>>,
    /// The hosted service's version 2.
    pub hosted: crate::v2::stand_in::Hosted,
}

/// The key a stand-in takes as an account's.
pub const TOKEN: &str = "stand-in";

impl StandIn {
    /// Starts one on a free local port.
    pub async fn start() -> std::io::Result<StandIn> {
        Self::serve(SocketAddr::from(([127, 0, 0, 1], 0))).await
    }

    /// Starts one at `addr`.
    pub async fn serve(addr: SocketAddr) -> std::io::Result<StandIn> {
        let held = Arc::new(Mutex::new(Held::default()));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let url = format!("http://{}", listener.local_addr()?);
        let hosted = crate::v2::stand_in::Hosted::default();
        let app = router(held.clone()).merge(hosted.routes());
        tokio::spawn(async move { axum::serve(listener, app).await.ok() });
        Ok(StandIn { url, held, hosted })
    }

    /// The counts kelves shared, in the order they came.
    pub fn counts(&self) -> Vec<Counts> {
        lock(&self.held).counts.clone()
    }
}

fn lock(held: &Mutex<Held>) -> std::sync::MutexGuard<'_, Held> {
    held.lock().unwrap_or_else(|p| p.into_inner())
}

fn router(held: Arc<Mutex<Held>>) -> Router {
    Router::new()
        .route("/v1/counts", post(counts))
        .with_state(held)
}

async fn counts(
    State(held): State<Arc<Mutex<Held>>>,
    headers: HeaderMap,
    Json(counts): Json<Counts>,
) -> Response {
    // Counts come with no account, as the real service asks: one that does
    // is turned away, so a test sees it.
    if headers.contains_key("authorization") {
        let detail = "Counts never come with a key.";
        return (
            StatusCode::BAD_REQUEST,
            [("content-type", "application/problem+json")],
            Json(Problem {
                kind: "about:blank".into(),
                title: detail.into(),
                status: StatusCode::BAD_REQUEST.as_u16(),
                detail: detail.into(),
            }),
        )
            .into_response();
    }
    lock(&held).counts.push(counts);
    Json(serde_json::json!({})).into_response()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::client::Client;

    #[tokio::test]
    async fn counts_come_with_no_key() {
        let s = StandIn::start().await.unwrap();
        let counts = Counts {
            week: "2026-11-02".into(),
            install: "0".repeat(32),
            agent: crate::client::AGENT.into(),
            version: "0.1.0".into(),
            platform: "macos".into(),
            weeks_in: 0,
            days: 1,
            deep_days: 0,
            contributing: false,
            counts: Default::default(),
            by_day: Default::default(),
        };
        // Signed in or not, the client sends them with no key.
        Client::new(&s.url, Some(TOKEN.into()))
            .unwrap()
            .send_counts(&counts)
            .await
            .unwrap();
        let got = s.counts();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].week, counts.week);
    }
}
