//! The ledger: every movement of money is two entries, one taking it from
//! an account and one giving it to another, written together or not at all.

use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::error::Error;

/// An amount of money, in the smallest unit of its currency: pence, cents.
pub type Minor = i64;

pub struct Movement<'a> {
    pub from: &'a str,
    pub to: &'a str,
    pub amount: Minor,
    pub currency: &'a str,
    pub why: &'a str,
}

/// Moves money between two accounts: a debit and a credit of the same
/// amount, in one transaction, so the books always balance.
pub async fn post(db: &PgPool, m: Movement<'_>) -> Result<Uuid, Error> {
    let mut tx = db.begin().await?;
    let id = post_in(&mut tx, m).await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn post_in(tx: &mut Transaction<'_, Postgres>, m: Movement<'_>) -> Result<Uuid, Error> {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO entries (movement, account, amount, currency, why) VALUES ($1, $2, $3, $4, $5), ($1, $6, $7, $4, $5)")
        .bind(id)
        .bind(m.from)
        .bind(-m.amount)
        .bind(m.currency)
        .bind(m.why)
        .bind(m.to)
        .bind(m.amount)
        .execute(&mut **tx)
        .await?;
    Ok(id)
}

/// What an account holds: the sum of its entries. Never stored, so it can
/// never disagree with them.
pub async fn balance(db: &PgPool, account: &str, currency: &str) -> Result<Minor, Error> {
    let (sum,): (Option<Minor>,) = sqlx::query_as("SELECT SUM(amount) FROM entries WHERE account = $1 AND currency = $2")
        .bind(account)
        .bind(currency)
        .fetch_one(db)
        .await?;
    Ok(sum.unwrap_or(0))
}
