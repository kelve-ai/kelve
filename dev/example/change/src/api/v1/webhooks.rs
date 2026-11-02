use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use crate::App;
use crate::bank::Outcome;
use crate::error::Error;

/// What the bank tells us happened.
#[derive(Deserialize)]
struct Event {
    /// The bank's own ID for the event: the same on every copy it sends.
    id: String,
    kind: String,
    /// The key of the request it's about, when it's about one of ours.
    request: Option<String>,
    #[serde(flatten)]
    outcome: Option<Outcome>,
}

/// What the bank tells us happened, signed with the secret we share, so
/// no one else can pretend to be the bank.
pub async fn receive(State(app): State<App>, headers: HeaderMap, body: Bytes) -> Result<(), Error> {
    let secret = std::env::var("WEBHOOK_SECRET").map_err(|_| Error::Config("WEBHOOK_SECRET"))?;
    let signature = headers.get("Bank-Signature").and_then(|v| v.to_str().ok()).ok_or(Error::Unsigned)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| Error::Unsigned)?;
    mac.update(&body);
    // Compared in constant time, so the check can't be timed.
    mac.verify_slice(&hex(signature)?).map_err(|_| Error::Unsigned)?;
    let event: Event = serde_json::from_slice(&body).map_err(|e| Error::BadRequest(e.to_string()))?;
    if !first_time(&app, &event).await? {
        // The bank sends an event again until we say we have it; this one
        // was acted on already, so it's only acknowledged.
        return Ok(());
    }
    // An event about a charge we lost track of finishes it now, rather than
    // when the reconciler next asks.
    if let (Some(request), Some(outcome)) = (&event.request, event.outcome) {
        crate::reconcile::heard(&app, request, outcome).await?;
    }
    Ok(())
}

/// Writes the event down by its ID; false if it was written down before.
async fn first_time(app: &App, event: &Event) -> Result<bool, Error> {
    let written = sqlx::query("INSERT INTO bank_events (id, kind, request) VALUES ($1, $2, $3) ON CONFLICT (id) DO UPDATE SET id = excluded.id RETURNING (xmax = 0)")
        .bind(&event.id)
        .bind(&event.kind)
        .bind(&event.request)
        .fetch_one(&app.db)
        .await?;
    Ok(sqlx::Row::get::<bool, _>(&written, 0))
}

fn hex(s: &str) -> Result<Vec<u8>, Error> {
    if s.len() % 2 != 0 {
        return Err(Error::Unsigned);
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| Error::Unsigned)).collect()
}
