use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};

use crate::App;
use crate::bank::Payment;
use crate::error::Error;
use crate::idempotency::{self, Answer, Claim};
use crate::ledger::Minor;
use crate::{reconcile, reserve};

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
/// the first one's answer, or is told the first is still going.
pub async fn create(State(app): State<App>, headers: HeaderMap, body: Bytes) -> Result<(StatusCode, Json<serde_json::Value>), Error> {
    let key = headers.get("Idempotency-Key").and_then(|v| v.to_str().ok()).ok_or(Error::BadKey("Idempotency-Key is missing"))?;
    match idempotency::claim(&app.db, key, &body).await? {
        Claim::Answered(answer) => return Ok(replay(answer)),
        Claim::InFlight => return Err(Error::InFlight),
        Claim::Fresh => {}
    }
    let c: Charge = match serde_json::from_slice(&body) {
        Ok(c) => c,
        Err(e) => {
            idempotency::let_go(&app.db, key).await?;
            return Err(Error::BadRequest(e.to_string()));
        }
    };
    let answer = match charge(&app, key, &c).await {
        Ok(receipt) => Answer { status: 201, body: serde_json::json!(Charged { receipt }) },
        Err(Error::Declined(reason)) => Answer { status: 402, body: serde_json::json!({ "declined": reason }) },
        // Nothing was charged: the caller may try again with the same key.
        Err(e @ (Error::BankDown { .. } | Error::Database(_))) => {
            idempotency::let_go(&app.db, key).await?;
            return Err(e);
        }
        // The bank may have taken it: the key stays claimed until the
        // reconciler finds out, so a retry can't charge again meanwhile.
        Err(e) => return Err(e),
    };
    idempotency::answer(&app.db, key, &answer).await?;
    Ok(replay(answer))
}

/// The charge itself, once its key is claimed. The money is held, and the
/// call written down as pending before the bank is asked, so a call that
/// never comes back is found and finished by the reconciler.
async fn charge(app: &App, key: &str, c: &Charge) -> Result<String, Error> {
    let held = reserve::hold(&app.db, &c.customer, c.amount, &c.currency).await?;
    reconcile::pending(&app.db, key, held, &c.customer, &c.merchant, c.amount, &c.currency).await?;
    let receipt = match app.bank.pay(key, &Payment { card: &c.card, amount: c.amount, currency: &c.currency }).await {
        Ok(receipt) => receipt,
        Err(e @ Error::BankDown { .. }) => {
            // The breaker stopped the call before it went: nothing was taken.
            reserve::release(&app.db, &c.customer, c.amount, &c.currency).await?;
            reconcile::done(&app.db, key).await?;
            return Err(e);
        }
        Err(e) => return Err(e),
    };
    if !receipt.approved {
        reserve::release(&app.db, &c.customer, c.amount, &c.currency).await?;
        reconcile::done(&app.db, key).await?;
        return Err(Error::Declined(receipt.reason.unwrap_or_default()));
    }
    reserve::settle(&app.db, &c.merchant, c.amount, &c.currency).await?;
    reconcile::done(&app.db, key).await?;
    Ok(receipt.id)
}

/// An answer, as it was given the first time.
fn replay(answer: Answer) -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::from_u16(answer.status).unwrap_or(StatusCode::OK), Json(answer.body))
}
