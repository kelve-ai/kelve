use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::App;
use crate::error::Error;
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

/// Gives money back: the bank first, then the ledger.
pub async fn create(State(app): State<App>, Json(r): Json<Refund>) -> Result<Json<Refunded>, Error> {
    let receipt = app.bank.refund(&r.payment, r.amount).await?;
    ledger::post(&app.db, Movement { from: &r.merchant, to: &r.customer, amount: r.amount, currency: &r.currency, why: "refund" }).await?;
    Ok(Json(Refunded { receipt: receipt.id }))
}
