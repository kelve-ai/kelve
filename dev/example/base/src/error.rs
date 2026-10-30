use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("the database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("the bank: {0}")]
    Bank(#[from] reqwest::Error),
    #[error("the bank said no: {0}")]
    Declined(String),
    #[error("a setting is missing: {0}")]
    Config(&'static str),
    #[error("a webhook isn't signed by the bank")]
    Unsigned,
    #[error("serving: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    /// Whether trying again might work: the bank was busy, or the line
    /// dropped. A declined card or a bad setting never will.
    pub fn retryable(&self) -> bool {
        match self {
            Error::Bank(e) => e.is_timeout() || e.is_connect() || e.status().is_some_and(|s| s.is_server_error()),
            Error::Database(sqlx::Error::PoolTimedOut) => true,
            _ => false,
        }
    }
}
