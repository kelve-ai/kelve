//! The bank: asking it to take money, and to give it back.

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::Error;
use crate::ledger::Minor;
use crate::retry::with_retries;

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

    /// Asks the bank to take a payment.
    pub async fn pay(&self, payment: &Payment<'_>) -> Result<Receipt, Error> {
        with_retries(|_| async {
            let receipt: Receipt = self
                .http
                .post(format!("{}/payments", self.url))
                .bearer_auth(&self.key)
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

    /// Asks the bank to give a payment back.
    pub async fn refund(&self, payment: &str, amount: Minor) -> Result<Receipt, Error> {
        with_retries(|_| async {
            let receipt: Receipt = self
                .http
                .post(format!("{}/payments/{payment}/refunds", self.url))
                .bearer_auth(&self.key)
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
