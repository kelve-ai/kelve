use std::time::Duration;

use crate::error::Error;

pub struct Config {
    pub bind: String,
    pub database_url: String,
    pub bank_url: String,
    pub bank_key: String,
    pub webhook_secret: String,
    /// Failures in a row that stop calls to the bank for a while.
    pub breaker_trip_after: u32,
    /// How long calls stop for, before one is let through to test the bank.
    pub breaker_cool_off: Duration,
    /// The most calls a second to the bank, over time.
    pub bank_calls_per_second: f64,
    /// The most calls to the bank at once, in a burst.
    pub bank_burst: f64,
    /// How often hanging charges are looked for.
    pub reconcile_every: Duration,
}

impl Config {
    pub fn from_env() -> Result<Config, Error> {
        let var = |name: &'static str| std::env::var(name).map_err(|_| Error::Config(name));
        Ok(Config {
            bind: std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:8080".into()),
            database_url: var("DATABASE_URL")?,
            bank_url: var("BANK_URL")?,
            bank_key: var("BANK_KEY")?,
            webhook_secret: var("WEBHOOK_SECRET")?,
            breaker_trip_after: number("BREAKER_TRIP_AFTER", 5)?,
            breaker_cool_off: Duration::from_secs(number("BREAKER_COOL_OFF_SECONDS", 30)?),
            bank_calls_per_second: number("BANK_CALLS_PER_SECOND", 20.0)?,
            bank_burst: number("BANK_BURST", 40.0)?,
            reconcile_every: Duration::from_secs(number("RECONCILE_EVERY_SECONDS", 60)?),
        })
    }
}

/// A number from the environment, or `default` when it isn't set. One
/// that's set but isn't a number stops the service, rather than being
/// quietly ignored.
fn number<T: std::str::FromStr>(name: &'static str, default: T) -> Result<T, Error> {
    match std::env::var(name) {
        Ok(v) => v.parse().map_err(|_| Error::Config(name)),
        Err(_) => Ok(default),
    }
}
