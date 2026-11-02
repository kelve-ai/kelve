//! The bank: asking it to take money, to give it back, and what became of
//! a payment we lost track of.

mod breaker;
mod limit;

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::Error;
use crate::ledger::Minor;
use crate::retry::with_retries;

pub use breaker::State as BreakerState;

/// How long one call to the bank may take before we give up on it, so the
/// retry around it can start.
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    url: String,
    key: String,
    breaker: Arc<breaker::Breaker>,
    limit: Arc<limit::Limit>,
}

#[derive(Serialize)]
pub struct Payment<'a> {
    pub card: &'a str,
    pub amount: Minor,
    pub currency: &'a str,
}

#[derive(Deserialize)]
pub struct Receipt {
    pub id: String,
    pub approved: bool,
    pub reason: Option<String>,
}

/// What the bank says became of a request, by its key.
#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    /// It took the payment: here's the receipt.
    Taken { receipt: String },
    /// It turned the payment down.
    Declined { reason: String },
    /// It never got the request, so nothing was taken.
    Unknown,
    /// It's still working on it.
    Pending,
}

impl Client {
    pub fn new(config: &Config) -> Client {
        Client {
            http: reqwest::Client::new(),
            url: config.bank_url.clone(),
            key: config.bank_key.clone(),
            breaker: Arc::new(breaker::Breaker::new(config)),
            limit: Arc::new(limit::Limit::new(config)),
        }
    }

    /// Asks the bank to take a payment. `request` is the same on every try,
    /// so the bank takes the payment once however many times it's asked.
    pub async fn pay(&self, request: &str, payment: &Payment<'_>) -> Result<Receipt, Error> {
        with_retries(|_| self.call(self.http.post(format!("{}/payments", self.url)), request, payment)).await
    }

    /// Asks the bank to give a payment back, once, however many times it's
    /// asked.
    pub async fn refund(&self, request: &str, payment: &str, amount: Minor) -> Result<Receipt, Error> {
        let body = serde_json::json!({ "amount": amount });
        with_retries(|_| self.call(self.http.post(format!("{}/payments/{payment}/refunds", self.url)), request, &body)).await
    }

    /// Asks the bank what became of a request we sent but never heard back
    /// about: the call timed out, or ledger stopped while it waited.
    pub async fn outcome(&self, request: &str) -> Result<Outcome, Error> {
        with_retries(|_| self.call(self.http.get(format!("{}/requests/{request}", self.url)), request, &())).await
    }

    /// Where the breaker is, for the health check.
    pub fn breaker(&self) -> BreakerState {
        self.breaker.state()
    }

    /// One call to the bank. The breaker is asked first, so a bank that's
    /// down isn't called at all; then the limit, so a burst of calls is
    /// spread out; then the call goes, with the caller's key and a time
    /// limit, and the breaker hears how it went.
    async fn call<B, T>(&self, builder: reqwest::RequestBuilder, request: &str, body: &B) -> Result<T, Error>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        self.breaker.ask(Instant::now())?;
        self.limit.wait().await;
        let sent = builder
            .bearer_auth(&self.key)
            .header("Idempotency-Key", request)
            .timeout(TIMEOUT)
            .json(body)
            .send()
            .await
            .and_then(|r| r.error_for_status());
        let answer = match sent {
            Ok(response) => response.json::<T>().await.map_err(Error::from),
            Err(e) => Err(Error::from(e)),
        };
        match &answer {
            Ok(_) => self.breaker.worked(),
            Err(e) => self.breaker.failed(e, Instant::now()),
        }
        answer
    }
}
