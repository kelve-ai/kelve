use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::App;
use crate::error::Error;

/// What the bank tells us happened, signed with the secret we share, so
/// no one else can pretend to be the bank.
pub async fn receive(State(app): State<App>, headers: HeaderMap, body: Bytes) -> Result<(), Error> {
    let secret = std::env::var("WEBHOOK_SECRET").map_err(|_| Error::Config("WEBHOOK_SECRET"))?;
    let signature = headers.get("Bank-Signature").and_then(|v| v.to_str().ok()).ok_or(Error::Unsigned)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| Error::Unsigned)?;
    mac.update(&body);
    // Compared in constant time, so the check can't be timed.
    mac.verify_slice(&hex(signature)?).map_err(|_| Error::Unsigned)?;
    let _ = app;
    Ok(())
}

fn hex(s: &str) -> Result<Vec<u8>, Error> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| Error::Unsigned)).collect()
}
