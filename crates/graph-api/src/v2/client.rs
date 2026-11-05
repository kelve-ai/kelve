//! The one client for the hosted service's version 2, used by kelve's
//! server. It sends with version 1's client, so it retries and waits the
//! same way, at the same address; only what it says to people differs.
//! Every call but signing in carries the account's key: there's nothing
//! here to read anonymously.

use std::time::Duration;

use base64::Engine;

use super::{
    API, Account, Character, Confirm, Credential, Deleted, Estimate, HandleAsk, HandleCheck, Offer,
    PageSettings, RunRequest, RunStatus, Score, Scores, SignInCode, SignedIn,
};
use crate::client::{self as one};

/// Why the hosted service couldn't be used, in sentences for the person.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("kelve can’t reach kelve.ai. Check your connection and try again.")]
    Unreachable,
    /// Busy for a moment: try again `after` this long, when it said.
    #[error("kelve.ai is busy. Try again in a minute.")]
    Busy { after: Option<Duration> },
    /// A limit, such as the day's runs, or the service not taking changes
    /// for a while: try again `after` this long.
    #[error("{said}")]
    Limited { said: String, after: Duration },
    #[error("Sign in to kelve.ai to get a verified score and a page for your agent.")]
    SignIn,
    /// What the service said no to, in its own sentence.
    #[error("{said}")]
    Refused { status: u16, said: String },
    #[error(
        "kelve couldn’t understand kelve.ai’s answer. Update kelve, and if it keeps happening, tell us."
    )]
    Unreadable,
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<one::Error> for Error {
    fn from(e: one::Error) -> Self {
        match e {
            one::Error::Unreachable => Error::Unreachable,
            one::Error::Busy { after } => Error::Busy { after },
            one::Error::Enough(limit) => Error::Limited {
                said: limit.to_string(),
                after: limit.after,
            },
            one::Error::Limited { said, after } => Error::Limited { said, after },
            one::Error::ReadsUsed { after } => Error::Busy { after: Some(after) },
            one::Error::SignIn => Error::SignIn,
            one::Error::Refused { status, said } => Error::Refused { status, said },
            one::Error::Unreadable => Error::Unreadable,
        }
    }
}

/// A client for the hosted service at one address.
#[derive(Clone, Debug)]
pub struct Client {
    one: one::Client,
}

impl Client {
    /// For the service at `base`, such as `https://graph.kelve.ai`, as the
    /// account `key` when the person signed in.
    pub fn new(base: &str, key: Option<String>) -> Result<Self> {
        Ok(Client {
            one: one::Client::new(base, key)?,
        })
    }

    pub fn base(&self) -> &str {
        self.one.base()
    }

    /// Whether the person signed in.
    pub fn signed_in(&self) -> bool {
        self.one.key().is_some()
    }

    fn url(&self, path: &str) -> String {
        self.one.url_at(API, path)
    }

    fn key(&self) -> Result<&str> {
        self.one.key().ok_or(Error::SignIn)
    }

    async fn send<T: serde::de::DeserializeOwned>(
        &self,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<Option<T>> {
        Ok(self.one.send_as(build, false).await?)
    }

    /// Where to send the person's browser to sign in: on to GitHub, or
    /// Google with `provider` `google`, then back to `redirect`, on their
    /// own computer, with a one-time code and `state`. `challenge` is the
    /// PKCE challenge for a verifier only this kelve holds.
    pub fn sign_in_address(
        &self,
        redirect: &str,
        state: &str,
        challenge: &str,
        provider: Option<&str>,
    ) -> Result<String> {
        let mut url = reqwest::Url::parse(&self.url("/signin")).map_err(|_| Error::Unreachable)?;
        url.query_pairs_mut()
            .append_pair("redirect_uri", redirect)
            .append_pair("state", state)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        if let Some(p) = provider.filter(|p| *p != "github") {
            url.query_pairs_mut().append_pair("provider", p);
        }
        Ok(url.into())
    }

    /// Exchanges a sign-in's one-time code for a key. Sent once, never
    /// retried: the service takes a code once.
    pub async fn exchange(&self, code: &SignInCode) -> Result<SignedIn> {
        let res = self
            .one
            .http()
            .post(self.url("/signin/token"))
            .json(code)
            .send()
            .await
            .map_err(|_| Error::Unreachable)?;
        let status = res.status();
        if !status.is_success() {
            return Err(Error::Refused {
                status: status.as_u16(),
                said: one::said(res, status).await,
            });
        }
        res.json().await.map_err(|_| Error::Unreadable)
    }

    /// Has the service forget this kelve's key. A key it already forgot is
    /// fine.
    pub async fn sign_out(&self) -> Result<()> {
        let Some(key) = self.one.key() else {
            return Ok(());
        };
        let res = self
            .one
            .http()
            .delete(self.url("/signin"))
            .bearer_auth(key)
            .send()
            .await
            .map_err(|_| Error::Unreachable)?;
        let status = res.status();
        if status.is_success() || status == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(());
        }
        Err(Error::Refused {
            status: status.as_u16(),
            said: one::said(res, status).await,
        })
    }

    /// The person's account: their page, and the runs left today.
    pub async fn account(&self) -> Result<Account> {
        let key = self.key()?;
        let url = self.url("/account");
        self.send(|| self.one.http().get(&url).bearer_auth(key))
            .await?
            .ok_or(Error::SignIn)
    }

    /// Deletes the person's account, and their page, character and every
    /// score with it. This kelve's key, and every other, stops working.
    pub async fn delete_account(&self) -> Result<Deleted> {
        let key = self.key()?;
        let url = self.url("/account");
        self.send(|| self.one.http().delete(&url).bearer_auth(key))
            .await?
            .ok_or(Error::SignIn)
    }

    /// Whether an address can be had, as the person types it.
    pub async fn check_handle(&self, typed: &str) -> Result<HandleCheck> {
        let key = self.key()?;
        let url = self.url("/handles/check");
        let ask = HandleAsk {
            handle: typed.into(),
        };
        self.send(|| self.one.http().post(&url).bearer_auth(key).json(&ask))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// Takes an address for the person's page. Someone else having it is
    /// a refusal, with what to do.
    pub async fn choose_handle(&self, typed: &str) -> Result<Account> {
        let key = self.key()?;
        let url = self.url("/account/handle");
        let ask = HandleAsk {
            handle: typed.into(),
        };
        self.send(|| self.one.http().put(&url).bearer_auth(key).json(&ask))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// Names the agent on the page, and publishes the page or takes it down.
    pub async fn set_page(&self, page: &PageSettings) -> Result<Account> {
        let key = self.key()?;
        let url = self.url("/account/page");
        self.send(|| self.one.http().put(&url).bearer_auth(key).json(page))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// The agent's character as the service has it, if it has one.
    pub async fn character(&self) -> Result<Option<Character>> {
        let key = self.key()?;
        let url = self.url("/account/character");
        self.send(|| self.one.http().get(&url).bearer_auth(key))
            .await
    }

    /// Gives the agent its character, which the service checks first.
    pub async fn set_character(&self, character: &Character) -> Result<Account> {
        let key = self.key()?;
        let url = self.url("/account/character");
        self.send(|| self.one.http().put(&url).bearer_auth(key).json(character))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// The account's scores, newest first, published or not.
    pub async fn scores(&self) -> Result<Vec<Score>> {
        let key = self.key()?;
        let url = self.url("/scores");
        let scores: Option<Scores> = self
            .send(|| self.one.http().get(&url).bearer_auth(key))
            .await?;
        Ok(scores.ok_or(Error::Unreadable)?.scores)
    }

    /// Puts a score on the person's page, or takes it off: None when
    /// there's no such score of theirs.
    pub async fn publish(&self, score: &str, published: bool) -> Result<Option<Score>> {
        let key = self.key()?;
        let url = self.url(&format!("/scores/{}/published", segment(score)));
        self.send(|| {
            let req = if published {
                self.one.http().put(&url)
            } else {
                self.one.http().delete(&url)
            };
            req.bearer_auth(key)
        })
        .await
    }

    /// What a verified run can be: the kinds of work, agents and models.
    pub async fn offer(&self) -> Result<Offer> {
        let key = self.key()?;
        let url = self.url("/runs/offer");
        self.send(|| self.one.http().get(&url).bearer_auth(key))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// Sends the skills for a run, and has what it'll take back. Nothing
    /// runs until it's confirmed. Retried like any call: an estimate
    /// nobody confirms is dropped, skills and all.
    pub async fn estimate(
        &self,
        kind: &str,
        agent: &str,
        model: &str,
        skills_tar: &[u8],
    ) -> Result<Estimate> {
        self.estimate_paying(kind, agent, model, skills_tar, Credential::ApiKey)
            .await
    }

    /// As [`Client::estimate`], for a run paid with `credential`: in tokens
    /// for a subscription, in dollars for an API key.
    pub async fn estimate_paying(
        &self,
        kind: &str,
        agent: &str,
        model: &str,
        skills_tar: &[u8],
        credential: Credential,
    ) -> Result<Estimate> {
        let key = self.key()?;
        let url = self.url("/runs");
        let ask = RunRequest {
            kind: kind.into(),
            agent: agent.into(),
            model: model.into(),
            skills: base64::engine::general_purpose::STANDARD.encode(skills_tar),
            credential,
        };
        self.send(|| self.one.http().post(&url).bearer_auth(key).json(&ask))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// Starts a run the person confirmed, paid for on their own key for the
    /// model's provider, which goes for this run only. Confirming twice
    /// starts it once.
    pub async fn confirm(&self, run: &str, provider_key: &str) -> Result<RunStatus> {
        self.confirm_paying(run, provider_key, Credential::ApiKey)
            .await
    }

    /// As [`Client::confirm`], paid with `credential`: an API key, or a
    /// Claude Code subscription's token.
    pub async fn confirm_paying(
        &self,
        run: &str,
        provider_key: &str,
        credential: Credential,
    ) -> Result<RunStatus> {
        let key = self.key()?;
        let url = self.url(&format!("/runs/{}/confirm", segment(run)));
        let confirm = Confirm {
            provider_key: provider_key.into(),
            credential,
        };
        self.send(|| self.one.http().post(&url).bearer_auth(key).json(&confirm))
            .await?
            .ok_or_else(|| gone(run))
    }

    /// How a run stands: None when there's no such run of theirs.
    pub async fn run(&self, run: &str) -> Result<Option<RunStatus>> {
        let key = self.key()?;
        let url = self.url(&format!("/runs/{}", segment(run)));
        self.send(|| self.one.http().get(&url).bearer_auth(key))
            .await
    }

    /// Stops a run, or drops an estimate; its skills are deleted.
    pub async fn cancel(&self, run: &str) -> Result<RunStatus> {
        let key = self.key()?;
        let url = self.url(&format!("/runs/{}", segment(run)));
        self.send(|| self.one.http().delete(&url).bearer_auth(key))
            .await?
            .ok_or_else(|| gone(run))
    }
}

fn gone(_run: &str) -> Error {
    Error::Refused {
        status: 404,
        said: "kelve.ai has no such run. Start another from kelve.".into(),
    }
}

/// An ID made safe for a path: the service's IDs are letters and digits,
/// so anything else is dropped.
fn segment(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}
