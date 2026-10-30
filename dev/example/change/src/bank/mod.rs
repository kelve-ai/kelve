//! The bank: asking it to take money, and to give it back.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::Error;
use crate::ledger::Minor;
use crate::retry::with_retries;

/// How long one call to the bank may take before we give up on it, so the
/// retry around it can start.
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    url: String,
    key: String,
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

impl Client {
    pub fn new(config: &Config) -> Client {
        Client { http: reqwest::Client::new(), url: config.bank_url.clone(), key: config.bank_key.clone() }
    }

    /// Asks the bank to take a payment. `request` is the same on every try,
    /// so the bank takes the payment once however many times it's asked.
    pub async fn pay(&self, request: &str, payment: &Payment<'_>) -> Result<Receipt, Error> {
        with_retries(|_| async {
            let receipt: Receipt = self
                .http
                .post(format!("{}/payments", self.url))
                .bearer_auth(&self.key)
                .header("Idempotency-Key", request)
                .timeout(TIMEOUT)
                .json(payment)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            Ok(receipt)
        })
        .await
    }

    /// Asks the bank to give a payment back, once, however many times it's
    /// asked.
    pub async fn refund(&self, request: &str, payment: &str, amount: Minor) -> Result<Receipt, Error> {
        with_retries(|_| async {
            let receipt: Receipt = self
                .http
                .post(format!("{}/payments/{payment}/refunds", self.url))
                .bearer_auth(&self.key)
                .header("Idempotency-Key", request)
                .timeout(TIMEOUT)
                .json(&serde_json::json!({ "amount": amount }))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            Ok(receipt)
        })
        .await
    }
}
