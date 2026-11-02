//! A request sent twice is answered once: the second copy gets the first
//! answer, or is told the first is still going. Run against a database:
//! `DATABASE_URL` must point at one with the migrations applied.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

async fn db() -> PgPool {
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL, for the tests' database");
    let db = PgPool::connect(&url).await.expect("the tests' database");
    sqlx::query("TRUNCATE requests CASCADE").execute(&db).await.expect("an empty requests table");
    db
}

fn fingerprint(body: &[u8]) -> String {
    Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect()
}

/// Claims a key as `src/idempotency.rs` does: the row comes back with
/// whether this insert made it.
async fn claim(db: &PgPool, key: &str, body: &[u8]) -> (String, Option<i32>, bool) {
    sqlx::query_as(
        "INSERT INTO requests (key, fingerprint, state) VALUES ($1, $2, 'started') \
         ON CONFLICT (key) DO UPDATE SET key = excluded.key RETURNING fingerprint, status, (xmax = 0)",
    )
    .bind(key)
    .bind(fingerprint(body))
    .fetch_one(db)
    .await
    .expect("a claim")
}

#[tokio::test]
async fn the_first_claim_goes_ahead() {
    let db = db().await;
    let (_, status, fresh) = claim(&db, "charge-1", b"{\"amount\":500}").await;
    assert!(fresh);
    assert_eq!(status, None);
}

#[tokio::test]
async fn a_second_claim_finds_the_first() {
    let db = db().await;
    claim(&db, "charge-2", b"{\"amount\":500}").await;
    let (_, _, fresh) = claim(&db, "charge-2", b"{\"amount\":500}").await;
    assert!(!fresh, "a retry must not go to the bank again");
}

#[tokio::test]
async fn an_answer_is_kept_for_the_next_copy() {
    let db = db().await;
    claim(&db, "charge-3", b"{\"amount\":500}").await;
    sqlx::query("UPDATE requests SET state = 'answered', status = 201, body = '{\"receipt\":\"r-1\"}', answered_at = now() WHERE key = $1")
        .bind("charge-3")
        .execute(&db)
        .await
        .unwrap();
    let (_, status, fresh) = claim(&db, "charge-3", b"{\"amount\":500}").await;
    assert!(!fresh);
    assert_eq!(status, Some(201));
}

#[tokio::test]
async fn the_same_key_with_another_body_is_told_apart() {
    let db = db().await;
    claim(&db, "charge-4", b"{\"amount\":500}").await;
    let (kept, _, _) = claim(&db, "charge-4", b"{\"amount\":900}").await;
    assert_ne!(kept, fingerprint(b"{\"amount\":900}"));
}

#[tokio::test]
async fn two_copies_at_once_make_one_claim() {
    let db = db().await;
    let body = b"{\"amount\":500}";
    let (a, b) = tokio::join!(claim(&db, "charge-5", body), claim(&db, "charge-5", body));
    assert!(a.2 ^ b.2, "exactly one copy goes ahead");
}
