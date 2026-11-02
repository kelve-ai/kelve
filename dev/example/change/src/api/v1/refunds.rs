use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};

use crate::App;
use crate::error::Error;
use crate::idempotency::{self, Answer, Claim};
use crate::ledger::{self, Minor, Movement};

#[derive(Deserialize)]
pub struct Refund {
    payment: String,
    customer: String,
    merchant: String,
    amount: Minor,
    currency: String,
}

#[derive(Serialize)]
pub struct Refunded {
    receipt: String,
}

/// Gives money back: the bank first, then the ledger. The caller's key goes
/// to the bank, so a refund retried is given once, and is claimed here
/// first, so two copies sent together can't both reach the bank.
pub async fn create(State(app): State<App>, headers: HeaderMap, body: Bytes) -> Result<(StatusCode, Json<serde_json::Value>), Error> {
    let key = headers.get("Idempotency-Key").and_then(|v| v.to_str().ok()).ok_or(Error::BadKey("Idempotency-Key is missing"))?;
    match idempotency::claim(&app.db, key, &body).await? {
        Claim::Answered(answer) => return Ok(replay(answer)),
        Claim::InFlight => return Err(Error::InFlight),
        Claim::Fresh => {}
    }
    let r: Refund = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            idempotency::let_go(&app.db, key).await?;
            return Err(Error::BadRequest(e.to_string()));
        }
    };
    let receipt = match app.bank.refund(key, &r.payment, r.amount).await {
        Ok(receipt) => receipt,
        Err(e @ Error::BankDown { .. }) => {
            idempotency::let_go(&app.db, key).await?;
            return Err(e);
        }
        Err(e) => return Err(e),
    };
    ledger::post(&app.db, Movement { from: &r.merchant, to: &r.customer, amount: r.amount, currency: &r.currency, why: "refund" }).await?;
    let answer = Answer { status: 201, body: serde_json::json!(Refunded { receipt: receipt.id }) };
    idempotency::answer(&app.db, key, &answer).await?;
    Ok(replay(answer))
}

/// An answer, as it was given the first time.
fn replay(answer: Answer) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::from_u16(answer.status).unwrap_or(StatusCode::OK), Json(answer.body))
}
