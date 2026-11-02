use std::time::Duration;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("the database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("the bank: {0}")]
    Bank(#[from] reqwest::Error),
    #[error("the bank said no: {0}")]
    Declined(String),
    #[error("the bank is down; try again in {} seconds", retry_in.as_secs())]
    BankDown { retry_in: Duration },
    #[error("the bank took too long to answer")]
    Timeout,
    #[error("a setting is missing: {0}")]
    Config(&'static str),
    #[error("a webhook isn't signed by the bank")]
    Unsigned,
    #[error("the request's key: {0}")]
    BadKey(&'static str),
    #[error("the same request is still going; ask again in a moment")]
    InFlight,
    #[error("the request: {0}")]
    BadRequest(String),
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
            Error::Timeout => true,
            _ => false,
        }
    }
}

/// What a shop is told when a request fails. A bank that's down, or a copy
/// of a request still going, says when to try again, so a shop's own
/// retries don't add to the load.
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, retry_after) = match &self {
            Error::Declined(_) => (StatusCode::PAYMENT_REQUIRED, None),
            Error::BankDown { retry_in } => (StatusCode::SERVICE_UNAVAILABLE, Some(retry_in.as_secs().max(1))),
            Error::InFlight => (StatusCode::CONFLICT, Some(1)),
            Error::BadKey(_) | Error::BadRequest(_) => (StatusCode::UNPROCESSABLE_ENTITY, None),
            Error::Unsigned => (StatusCode::UNAUTHORIZED, None),
            Error::Timeout | Error::Bank(_) => (StatusCode::BAD_GATEWAY, None),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, None),
        };
        let mut response = (status, Json(serde_json::json!({ "error": self.to_string() }))).into_response();
        if let Some(seconds) = retry_after {
            response.headers_mut().insert("Retry-After", seconds.into());
        }
        response
    }
}
