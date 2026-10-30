//! Calls to the kelve running on this computer, with the secret it shares
//! with its command line. They never go through a proxy: kelve is local,
//! and the secret stays on this computer.

use std::time::Duration;

use anyhow::bail;
use serde_json::Value;

use crate::config;

/// What to do when nothing answers at kelve's address.
pub const NOT_RUNNING: &str = "kelve isn't running. Start it with `kelve serve`.";

/// An HTTP client for kelve that gives up after `within`.
pub fn http(within: Duration) -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(within)
        .connect_timeout(Duration::from_secs(2).min(within))
        .no_proxy()
        .user_agent(concat!("kelve-cli/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// kelve, as the command line reaches it.
#[derive(Debug)]
pub struct Kelve {
    pub url: String,
    secret: String,
    http: reqwest::Client,
}

impl Kelve {
    /// kelve at its address, with the secret, giving up on a call after `within`.
    pub fn new(within: Duration) -> anyhow::Result<Kelve> {
        let url = config::url();
        if !config::safe_for_secret(&url) {
            anyhow::bail!(
                "kelve's address, {url}, is plain http on another computer, so its secret would cross the network unencrypted. Give KELVE_URL an https address, or reach kelve through localhost."
            );
        }
        Ok(Kelve {
            url,
            secret: config::secret()?,
            http: http(within)?,
        })
    }

    /// A POST with the secret: `path` from kelve's root, like `/api/activity`.
    pub fn post(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .post(format!("{}{path}", self.url))
            .bearer_auth(&self.secret)
    }

    /// A GET with the secret.
    pub fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .get(format!("{}{path}", self.url))
            .bearer_auth(&self.secret)
    }

    /// Sends a request, and reads its JSON answer, or says what went wrong.
    pub async fn send(&self, request: reqwest::RequestBuilder) -> anyhow::Result<Value> {
        let response = request.send().await.map_err(unreachable)?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("{}", said(status, &text));
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }
}

/// What a failed request means for the person.
pub fn unreachable(e: reqwest::Error) -> anyhow::Error {
    if e.is_connect() {
        anyhow::anyhow!(NOT_RUNNING)
    } else if e.is_timeout() {
        anyhow::anyhow!("kelve didn't answer in time. Check it's running with `kelve status`.")
    } else {
        anyhow::anyhow!("Couldn't reach kelve: {e}")
    }
}

/// What kelve said when it said no: its own sentence, when it gave one.
pub fn said(status: reqwest::StatusCode, text: &str) -> String {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_string))
        .or_else(|| {
            let text = text.trim();
            (!text.is_empty() && text.len() < 300 && !text.starts_with('<'))
                .then(|| text.to_string())
        })
        .unwrap_or_else(|| format!("kelve said no ({status})."))
}

/// Whether kelve answers at `url`.
pub async fn running(url: &str) -> bool {
    let Ok(http) = http(Duration::from_secs(2)) else {
        return false;
    };
    match http.get(format!("{url}/healthz")).send().await {
        Ok(response) => response.text().await.is_ok_and(|body| body.trim() == "ok"),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kelve_is_quoted_when_it_says_no() {
        let status = reqwest::StatusCode::UNAUTHORIZED;
        assert_eq!(
            said(status, r#"{"error":"Open kelve with `kelve open`."}"#),
            "Open kelve with `kelve open`."
        );
        assert_eq!(
            said(
                reqwest::StatusCode::MISDIRECTED_REQUEST,
                "kelve only answers at its own address."
            ),
            "kelve only answers at its own address."
        );
        assert_eq!(said(status, ""), "kelve said no (401 Unauthorized).");
    }
}
