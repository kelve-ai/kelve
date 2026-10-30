use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};

use crate::App;
use crate::bank::Payment;
use crate::error::Error;
use crate::ledger::Minor;
use crate::reserve;

#[derive(Deserialize)]
pub struct Charge {
    customer: String,
    merchant: String,
    card: String,
    amount: Minor,
    currency: String,
}

#[derive(Serialize)]
pub struct Charged {
    receipt: String,
}

/// Charges a card: holds the money, asks the bank, then settles or lets go.
/// A charge sent twice with the same key is charged once: the second gets
/// the first one's receipt.
pub async fn create(State(app): State<App>, headers: HeaderMap, Json(c): Json<Charge>) -> Result<Json<Charged>, Error> {
    let key = headers.get("Idempotency-Key").and_then(|v| v.to_str().ok()).ok_or(Error::Config("Idempotency-Key"))?;
    if let Some(receipt) = seen(&app, key).await? {
        return Ok(Json(Charged { receipt }));
    }
    reserve::hold(&app.db, &c.customer, c.amount, &c.currency).await?;
    let receipt = app.bank.pay(key, &Payment { card: &c.card, amount: c.amount, currency: &c.currency }).await?;
    if !receipt.approved {
        reserve::release(&app.db, &c.customer, c.amount, &c.currency).await?;
        return Err(Error::Declined(receipt.reason.unwrap_or_default()));
    }
    reserve::settle(&app.db, &c.merchant, c.amount, &c.currency).await?;
    remember(&app, key, &receipt.id).await?;
    Ok(Json(Charged { receipt: receipt.id }))
}

/// The receipt for a request already answered.
async fn seen(app: &App, key: &str) -> Result<Option<String>, Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT receipt FROM requests WHERE key = $1").bind(key).fetch_optional(&app.db).await?;
    Ok(row.map(|(r,)| r))
}

async fn remember(app: &App, key: &str, receipt: &str) -> Result<(), Error> {
    sqlx::query("INSERT INTO requests (key, receipt) VALUES ($1, $2) ON CONFLICT (key) DO NOTHING")
        .bind(key)
        .bind(receipt)
        .execute(&app.db)
        .await?;
    Ok(())
}
