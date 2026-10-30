//! A charge in two steps: the money is held first, then taken once the bank
//! agrees, or let go if it doesn't.

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Error;
use crate::ledger::{self, Minor, Movement};

/// Holds money in a customer's account for a charge, so two charges at once
/// can't both spend it.
pub async fn hold(db: &PgPool, customer: &str, amount: Minor, currency: &str) -> Result<Uuid, Error> {
    let mut tx = db.begin().await?;
    // One charge at a time for each customer: the lock is let go when the
    // transaction ends.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(customer).execute(&mut *tx).await?;
    let held = ledger::post_in(&mut tx, Movement { from: customer, to: "held", amount, currency, why: "hold" }).await?;
    tx.commit().await?;
    Ok(held)
}

/// Takes the money held: from held to the merchant.
pub async fn settle(db: &PgPool, merchant: &str, amount: Minor, currency: &str) -> Result<Uuid, Error> {
    ledger::post(db, Movement { from: "held", to: merchant, amount, currency, why: "settle" }).await
}

/// Gives the money held back to the customer.
pub async fn release(db: &PgPool, customer: &str, amount: Minor, currency: &str) -> Result<Uuid, Error> {
    ledger::post(db, Movement { from: "held", to: customer, amount, currency, why: "release" }).await
}
