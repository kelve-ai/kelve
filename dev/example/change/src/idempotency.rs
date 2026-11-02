//! Requests answered once: a shop sends a key with each charge or refund,
//! and the same key gets the same answer, however many times it's sent.
//!
//! A key is claimed before anything else happens, so two copies of the same
//! request arriving together can't both go to the bank. The first claims
//! it; the second is told the first is still going, and to ask again.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::error::Error;

/// How long a claim lasts with no answer before another copy of the same
/// request may take it over: longer than any call to the bank can take.
const CLAIM_LASTS: &str = "5 minutes";

/// What a key's claim found.
#[derive(Debug, PartialEq, Eq)]
pub enum Claim {
    /// The key is new: this request is the one that goes ahead.
    Fresh,
    /// The key was answered before: here's the answer it got.
    Answered(Answer),
    /// Another copy of this request is still going.
    InFlight,
}

/// An answer kept by its key: what the caller got back the first time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer {
    pub status: u16,
    pub body: serde_json::Value,
}

/// Claims `key` for a request whose body is `body`. The key is claimed with
/// an insert before anything is charged, and the stored row comes back in
/// the same statement, so no other request can slip in between the two.
pub async fn claim(db: &PgPool, key: &str, body: &[u8]) -> Result<Claim, Error> {
    if key.is_empty() || key.len() > 255 {
        return Err(Error::BadKey("a key is 1 to 255 characters"));
    }
    let fingerprint = fingerprint(body);
    let row: (String, Option<i32>, Option<serde_json::Value>, bool, bool) = sqlx::query_as(
        "INSERT INTO requests (key, fingerprint, state) VALUES ($1, $2, 'started') \
         ON CONFLICT (key) DO UPDATE SET key = excluded.key \
         RETURNING fingerprint, status, body, (xmax = 0) AS fresh, \
                   (state = 'started' AND at < now() - $3::interval) AS stale",
    )
    .bind(key)
    .bind(&fingerprint)
    .bind(CLAIM_LASTS)
    .fetch_one(db)
    .await?;
    let (kept, status, body, fresh, stale) = row;
    if fresh {
        return Ok(Claim::Fresh);
    }
    // The same key with a different request is a mistake by the caller,
    // not a retry: answering it would hide their bug.
    if kept != fingerprint {
        return Err(Error::BadKey("this key was used for a different request"));
    }
    match (status, body) {
        (Some(status), Some(body)) => Ok(Claim::Answered(Answer { status: status as u16, body })),
        _ if stale => take_over(db, key).await,
        _ => Ok(Claim::InFlight),
    }
}

/// Keeps the answer a claimed request got, so the next copy gets it too.
pub async fn answer(db: &PgPool, key: &str, answer: &Answer) -> Result<(), Error> {
    sqlx::query("UPDATE requests SET state = 'answered', status = $2, body = $3, answered_at = now() WHERE key = $1")
        .bind(key)
        .bind(answer.status as i32)
        .bind(&answer.body)
        .execute(db)
        .await?;
    Ok(())
}

/// Lets a claim go when its request failed before anything was charged,
/// so the caller may send it again and have it go ahead.
pub async fn let_go(db: &PgPool, key: &str) -> Result<(), Error> {
    sqlx::query("DELETE FROM requests WHERE key = $1 AND state = 'started'").bind(key).execute(db).await?;
    Ok(())
}

/// Takes over a claim whose request never answered: ledger stopped while it
/// was going. Only one copy can take it over, the one that moves its time.
async fn take_over(db: &PgPool, key: &str) -> Result<Claim, Error> {
    let moved = sqlx::query("UPDATE requests SET at = now() WHERE key = $1 AND state = 'started' AND at < now() - $2::interval")
        .bind(key)
        .bind(CLAIM_LASTS)
        .execute(db)
        .await?;
    Ok(if moved.rows_affected() == 1 { Claim::Fresh } else { Claim::InFlight })
}

/// A request's fingerprint: a hash of its body, so the same key sent with a
/// different request can be told apart from a retry.
pub fn fingerprint(body: &[u8]) -> String {
    let hash = Sha256::digest(body);
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// Forgets answers older than a day: no shop retries a request that late.
pub async fn forget_old(db: &PgPool) -> Result<u64, Error> {
    let gone = sqlx::query("DELETE FROM requests WHERE state = 'answered' AND answered_at < now() - interval '1 day'")
        .execute(db)
        .await?;
    Ok(gone.rows_affected())
}
