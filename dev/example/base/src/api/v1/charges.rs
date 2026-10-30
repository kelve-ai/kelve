use axum::Json;
use axum::extract::State;
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
pub async fn create(State(app): State<App>, Json(c): Json<Charge>) -> Result<Json<Charged>, Error> {
    reserve::hold(&app.db, &c.customer, c.amount, &c.currency).await?;
    let receipt = app.bank.pay(&Payment { card: &c.card, amount: c.amount, currency: &c.currency }).await?;
    if !receipt.approved {
        reserve::release(&app.db, &c.customer, c.amount, &c.currency).await?;
        return Err(Error::Declined(receipt.reason.unwrap_or_default()));
    }
    reserve::settle(&app.db, &c.merchant, c.amount, &c.currency).await?;
    Ok(Json(Charged { receipt: receipt.id }))
}
