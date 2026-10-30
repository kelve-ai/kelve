use std::time::Duration;

use crate::error::Error;

const WAIT: Duration = Duration::from_millis(500);
const MAX_ATTEMPTS: u32 = 3;

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
        tokio::time::sleep(WAIT).await;
        attempt += 1;
    }
}
