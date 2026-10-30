use std::time::Duration;

use rand::Rng;

use crate::error::Error;

/// The longest a single wait may be.
const MAX_WAIT: Duration = Duration::from_secs(30);
const BASE: Duration = Duration::from_millis(200);
const MAX_ATTEMPTS: u32 = 6;

/// Calls `call` until it works, or fails in a way a retry won't fix.
pub async fn with_retries<T, F, Fut>(mut call: F) -> Result<T, Error>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut attempt = 0;
    loop {
        match call(attempt).await {
            Ok(v) => return Ok(v),
            Err(e) if attempt + 1 == MAX_ATTEMPTS || !e.retryable() => return Err(e),
            Err(_) => {}
        }
        tokio::time::sleep(wait(attempt)).await;
        attempt += 1;
    }
}

/// The longest the next wait may be: twice the last, up to `MAX_WAIT`.
fn ceiling(attempt: u32) -> Duration {
    (BASE * 2u32.pow(attempt)).min(MAX_WAIT)
}

/// How long to wait before the next try: a random time below the ceiling,
/// so clients that failed together don't come back together.
fn wait(attempt: u32) -> Duration {
    rand::thread_rng().gen_range(Duration::ZERO..=ceiling(attempt))
}
