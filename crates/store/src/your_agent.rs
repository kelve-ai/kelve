//! What the Your agent page and its settings ask the store: how many skill
//! checks have run lately, for the most a week the person allows.

use crate::{Conn, Result};

/// How many checks were asked for since `since`, in ms, whatever came of
/// them, but for those stopped before they were done.
pub async fn checks_since(conn: &mut Conn, since: i64) -> Result<i64> {
    Ok(
        sqlx::query_scalar(
            "select count(*) from checks where asked_at >= ? and state != 'stopped'",
        )
        .bind(since)
        .fetch_one(&mut *conn)
        .await?,
    )
}
