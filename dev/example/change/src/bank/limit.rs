//! A limit on how fast ledger calls the bank: a bucket of tokens that fills
//! at a steady rate, one token for each call. A burst of calls takes what's
//! in the bucket; after that, each call waits for the next token.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::config::Config;

pub struct Limit {
    bucket: Mutex<Bucket>,
    /// Tokens added each second: the most calls a second, over time.
    rate: f64,
    /// The most tokens the bucket holds: the biggest burst allowed.
    burst: f64,
}

struct Bucket {
    tokens: f64,
    at: Instant,
}

impl Limit {
    pub fn new(config: &Config) -> Limit {
        Limit::with(config.bank_calls_per_second, config.bank_burst, Instant::now())
    }

    pub fn with(rate: f64, burst: f64, now: Instant) -> Limit {
        Limit { bucket: Mutex::new(Bucket { tokens: burst, at: now }), rate, burst }
    }

    /// Takes a token, if there is one; if not, how long until there will be.
    pub fn take(&self, now: Instant) -> Result<(), Duration> {
        let mut bucket = self.bucket.lock().expect("the limit's lock");
        bucket.fill(now, self.rate, self.burst);
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            Err(Duration::from_secs_f64((1.0 - bucket.tokens) / self.rate))
        }
    }

    /// Waits for a token, then takes it. Calls that wait together are let
    /// through one token apart, not all at once when the bucket refills.
    pub async fn wait(&self) {
        loop {
            match self.take(Instant::now()) {
                Ok(()) => return,
                Err(wait) => tokio::time::sleep(wait).await,
            }
        }
    }
}

impl Bucket {
    /// Adds the tokens earned since it was last filled, up to the burst.
    fn fill(&mut self, now: Instant, rate: f64, burst: f64) {
        let earned = now.saturating_duration_since(self.at).as_secs_f64() * rate;
        self.tokens = (self.tokens + earned).min(burst);
        self.at = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_then_one_at_a_time() {
        let start = Instant::now();
        let limit = Limit::with(10.0, 3.0, start);
        assert!(limit.take(start).is_ok());
        assert!(limit.take(start).is_ok());
        assert!(limit.take(start).is_ok());
        assert_eq!(limit.take(start), Err(Duration::from_millis(100)));
        assert!(limit.take(start + Duration::from_millis(100)).is_ok());
    }

    #[test]
    fn the_bucket_never_holds_more_than_the_burst() {
        let start = Instant::now();
        let limit = Limit::with(10.0, 3.0, start);
        let later = start + Duration::from_secs(60);
        for _ in 0..3 {
            assert!(limit.take(later).is_ok());
        }
        assert!(limit.take(later).is_err());
    }
}
