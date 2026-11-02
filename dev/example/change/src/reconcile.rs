//! Charges left hanging: a call to the bank that timed out, or a ledger
//! that stopped while it waited, leaves money held and no one knowing if
//! the bank took it. The reconciler finds those, asks the bank what became
//! of each, and settles the charge or gives the money back.

use std::time::Duration;

use sqlx::PgPool;
use uuid::Uuid;

use crate::App;
use crate::bank::Outcome;
use crate::error::Error;
use crate::idempotency::{self, Answer};
use crate::ledger::Minor;
use crate::reserve;

/// How long a call is left alone before it counts as hanging: longer than
/// all of a call's retries can take.
const HANGING_AFTER: &str = "3 minutes";
/// How many hanging calls one round looks at.
const BATCH: i64 = 50;
/// After this long, a call the bank still can't account for is given up:
/// the money goes back to the customer, and someone is told.
const GIVE_UP_AFTER: &str = "1 day";
/// The lock only one ledger at a time may hold, so two never reconcile the
/// same call. Any number fits; this is "ledger" in ASCII.
const ONE_AT_A_TIME: i64 = 0x6c6564676572;

/// A charge written down before the bank was asked.
#[derive(sqlx::FromRow)]
struct Pending {
    key: String,
    held: Uuid,
    customer: String,
    merchant: String,
    amount: Minor,
    currency: String,
    expired: bool,
}

/// Writes a charge down as pending, before the bank is asked.
pub async fn pending(db: &PgPool, key: &str, held: Uuid, customer: &str, merchant: &str, amount: Minor, currency: &str) -> Result<(), Error> {
    sqlx::query("INSERT INTO pending_calls (key, held, customer, merchant, amount, currency) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(key)
        .bind(held)
        .bind(customer)
        .bind(merchant)
        .bind(amount)
        .bind(currency)
        .execute(db)
        .await?;
    Ok(())
}

/// The bank answered: the charge isn't hanging.
pub async fn done(db: &PgPool, key: &str) -> Result<(), Error> {
    sqlx::query("DELETE FROM pending_calls WHERE key = $1").bind(key).execute(db).await?;
    Ok(())
}

/// The bank told us, unasked, what became of a request: if it's a charge
/// still hanging, it's finished now.
pub async fn heard(app: &App, key: &str, outcome: Outcome) -> Result<(), Error> {
    let call: Option<Pending> = sqlx::query_as(
        "SELECT key, held, customer, merchant, amount, currency, false AS expired FROM pending_calls WHERE key = $1",
    )
    .bind(key)
    .fetch_optional(&app.db)
    .await?;
    if let Some(call) = call {
        finish(app, &call, outcome).await?;
    }
    Ok(())
}

/// Runs for as long as ledger does: a round every `every`.
pub async fn run(app: App, every: Duration) {
    let mut tick = tokio::time::interval(every);
    loop {
        tick.tick().await;
        match round(&app).await {
            Ok(0) => {}
            Ok(n) => eprintln!("reconciled {n} hanging charges"),
            Err(e) => eprintln!("reconciling: {e}"),
        }
    }
}

/// One round: takes the lock, so only one ledger reconciles at a time,
/// then deals with each hanging call in turn. Returns how many it settled
/// or gave back.
pub async fn round(app: &App) -> Result<usize, Error> {
    let mut conn = app.db.acquire().await?;
    let (locked,): (bool,) = sqlx::query_as("SELECT pg_try_advisory_lock($1)").bind(ONE_AT_A_TIME).fetch_one(&mut *conn).await?;
    if !locked {
        return Ok(0);
    }
    let result = hanging(app).await;
    sqlx::query("SELECT pg_advisory_unlock($1)").bind(ONE_AT_A_TIME).execute(&mut *conn).await?;
    result
}

async fn hanging(app: &App) -> Result<usize, Error> {
    let calls: Vec<Pending> = sqlx::query_as(
        "SELECT key, held, customer, merchant, amount, currency, at < now() - $3::interval AS expired \
         FROM pending_calls WHERE at < now() - $1::interval ORDER BY at LIMIT $2",
    )
    .bind(HANGING_AFTER)
    .bind(BATCH)
    .bind(GIVE_UP_AFTER)
    .fetch_all(&app.db)
    .await?;
    let mut finished = 0;
    for call in calls {
        // A bank that's down now will be asked next round.
        let outcome = match app.bank.outcome(&call.key).await {
            Ok(outcome) => outcome,
            Err(Error::BankDown { .. }) => break,
            Err(e) => return Err(e),
        };
        if finish(app, &call, outcome).await? {
            finished += 1;
        }
    }
    Ok(finished)
}

/// Finishes one hanging charge as the bank says it went: taken, so the
/// money goes to the merchant; declined or never received, so it goes back
/// to the customer. Still pending, it waits, unless it has waited too long.
async fn finish(app: &App, call: &Pending, outcome: Outcome) -> Result<bool, Error> {
    let answer = match outcome {
        Outcome::Taken { receipt } => {
            reserve::settle(&app.db, &call.merchant, call.amount, &call.currency).await?;
            Answer { status: 201, body: serde_json::json!({ "receipt": receipt }) }
        }
        Outcome::Declined { reason } => {
            reserve::release(&app.db, &call.customer, call.amount, &call.currency).await?;
            Answer { status: 402, body: serde_json::json!({ "declined": reason }) }
        }
        Outcome::Unknown => {
            reserve::release(&app.db, &call.customer, call.amount, &call.currency).await?;
            done(&app.db, &call.key).await?;
            // The bank never saw it: the shop may send it again.
            idempotency::let_go(&app.db, &call.key).await?;
            return Ok(true);
        }
        Outcome::Pending if call.expired => {
            eprintln!("giving up on charge {} (hold {}): the bank still can't say what became of it", call.key, call.held);
            reserve::release(&app.db, &call.customer, call.amount, &call.currency).await?;
            Answer { status: 504, body: serde_json::json!({ "gave_up": true }) }
        }
        Outcome::Pending => return Ok(false),
    };
    idempotency::answer(&app.db, &call.key, &answer).await?;
    done(&app.db, &call.key).await?;
    Ok(true)
}
