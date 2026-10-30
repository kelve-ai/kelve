//! The one client for the shared graph, used by kelve's server. It's the
//! only place that retries: a short timeout on every call, and a couple of
//! retries with capped exponential backoff and full jitter on what's worth
//! retrying, as the AWS Builders' Library recommends, waiting as long as
//! the graph's `Retry-After` asks when it says. A wait longer than it
//! keeps a call waiting is handed back, with how long, and so is the day's
//! limit on what an account adds (`Error::Enough`), which retrying never
//! helps before it resets. Every call is safe to retry: reads change
//! nothing, and a changeset carries a key the graph answers once.
//!
//! Reads never carry an account: looking ideas up says nothing about who asked.
//! Only a changeset does, so the graph knows who wrote it.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{
    Account, Agree, Applied, BUCKET_CHARS, Bucket, Candidates, Changeset, Checking, Counts,
    EntryPage, Flag, Foundations, Ideas, Instructions, Manifest, MatchRequest, Problem, Relations,
    SignInCode, SignedIn, Terms, UpPage,
};

/// What every request says made it.
pub const AGENT: &str = concat!("kelve/", env!("CARGO_PKG_VERSION"));

/// The most names one request looks up, and the most IDs one asks for. A
/// longer list goes in several requests.
pub const MAX_QUERIES: usize = 50;
pub const MAX_IDS: usize = 100;

/// The largest answer the client reads: anything larger is refused, not
/// held in memory.
const MAX_BODY: usize = 8 * 1024 * 1024;

const ATTEMPTS: u32 = 3;
const FIRST_WAIT: Duration = Duration::from_millis(250);

/// The longest the client waits for a busy graph before giving up for now.
const LONGEST_WAIT: Duration = Duration::from_secs(20);

/// Why the graph couldn't be used, in sentences for the person.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error(
        "kelve couldn’t reach the shared graph of ideas. Check you’re online; kelve will try again."
    )]
    Unreachable,
    /// Busy for a moment: try again `after` this long, when the graph said.
    #[error("The shared graph of ideas is busy. kelve will try again shortly.")]
    Busy { after: Option<Duration> },
    /// The account has added as much as it may today.
    #[error("{0}")]
    Enough(Limit),
    /// A limit the graph keeps, or the graph not taking changes for a
    /// while: try again `after` this long.
    #[error("{said}")]
    Limited { said: String, after: Duration },
    #[error("Sign in to kelve’s shared graph to read it and add to it.")]
    SignIn,
    #[error("The shared graph of ideas said no: {said}")]
    Refused { status: u16, said: String },
    #[error(
        "kelve couldn’t understand the shared graph’s answer. Update kelve, and if it keeps happening, tell us."
    )]
    Unreadable,
}

pub type Result<T> = std::result::Result<T, Error>;

/// A day's limit on what an account adds, reached: what ran out, whether
/// it's a new account's limit, and how long until it resets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit {
    /// What ran out, when the graph said.
    pub what: Option<Quota>,
    /// A new account's limit, which rises once ten of its entries are
    /// accepted, or after a week.
    pub new_account: bool,
    /// How long until the day's limits reset.
    pub after: Duration,
}

/// What an account adds a day, each with its own limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quota {
    Ideas,
    Entries,
    Relations,
    Lessons,
}

impl Limit {
    /// The limit a graph's "Enough for today" problem says was reached:
    /// what ran out, from its detail, which names it ("as many ideas").
    fn from_problem(detail: &str, after: Duration) -> Limit {
        let what = [
            ("ideas", Quota::Ideas),
            ("entries", Quota::Entries),
            ("relations", Quota::Relations),
            ("lessons", Quota::Lessons),
        ]
        .into_iter()
        .find(|(word, _)| detail.contains(&format!("as many {word} ")))
        .map(|(_, q)| q);
        Limit {
            what,
            new_account: detail.contains("as a new account"),
            after,
        }
    }
}

impl std::fmt::Display for Limit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = match self.what {
            Some(Quota::Ideas) => "as many ideas",
            Some(Quota::Entries) => "as many entries",
            Some(Quota::Relations) => "as many ways ideas relate",
            Some(Quota::Lessons) => "as many lessons",
            None => "as much",
        };
        let who = if self.new_account {
            "as a new account can"
        } else {
            "as you can"
        };
        write!(
            f,
            "You’ve added {what} {who} today. kelve sends the rest once the limit resets."
        )
    }
}

/// How long until midnight UTC, when the graph's day begins.
fn to_midnight() -> Duration {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Duration::from_secs(86_400 - secs % 86_400)
}

/// A client for one graph.
#[derive(Clone)]
pub struct Client {
    base: String,
    token: Option<String>,
    /// The account reading tokens are asked for with, when it isn't the
    /// one writing: a kelve that only reads.
    reading_as: Option<String>,
    /// Reading tokens, each read with once: shared by every clone.
    tokens: Tokens,
    http: reqwest::Client,
    /// Buckets of the name index fetched lately, by prefix: shared by every
    /// clone.
    buckets: Buckets,
}

/// Buckets of the name index by prefix, with when each was fetched.
type Buckets = Arc<Mutex<HashMap<String, (Instant, Arc<Bucket>)>>>;

/// Reading tokens on hand, and the key they're signed with, with when it
/// was fetched.
#[derive(Default)]
struct Pool {
    key: Option<(crate::tokens::TokenKey, Instant)>,
    tokens: Vec<String>,
}

// Never the keys: only where, and whether it writes or reads as someone.
impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("base", &self.base)
            .field("writes", &self.token.is_some())
            .field(
                "reads",
                &(self.token.is_some() || self.reading_as.is_some()),
            )
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool")
            .field("tokens", &self.tokens.len())
            .finish_non_exhaustive()
    }
}

type Tokens = Arc<tokio::sync::Mutex<Pool>>;

/// How long the graph's token key is used before it's asked for again.
const TOKEN_KEY_KEPT: Duration = Duration::from_secs(60 * 60);

/// How many reading tokens are asked for at a time.
const TOKENS_AT_ONCE: usize = 100;

/// How long a bucket of the name index is kept before it's asked for again.
const BUCKET_KEPT: Duration = Duration::from_secs(60 * 60);

/// The most buckets kept at once.
const MOST_BUCKETS: usize = 4096;

impl Client {
    /// For the graph at `base`, such as `https://graph.kelve.ai`. `token`
    /// is the account changesets are written as, if the person signed in;
    /// with it, each read spends a reading token, never the account's key.
    pub fn new(base: &str, token: Option<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(4))
            .user_agent(AGENT)
            // Followed here instead: a token is read with once, so each hop
            // spends its own.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| Error::Unreachable)?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            token,
            reading_as: None,
            tokens: Arc::default(),
            http,
            buckets: Arc::default(),
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// Whether changesets can be sent: the person signed in.
    pub fn can_write(&self) -> bool {
        self.token.is_some()
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}{path}", self.base, crate::API)
    }

    /// What the graph is.
    pub async fn manifest(&self) -> Result<Manifest> {
        let url = self.url("");
        self.send(|| self.http.get(&url))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// The ideas each name may be, by the asker's own keys. Names the graph
    /// has nothing for come back with no candidates.
    ///
    /// A name the graph has exactly is found in the name index, by an
    /// anonymous bucket of names: the graph never learns which name was
    /// asked. Only names it doesn't have exactly are sent, for the nearest
    /// spellings.
    pub async fn candidates(&self, queries: &MatchRequest) -> Result<BTreeMap<String, Candidates>> {
        let mut found = BTreeMap::new();
        let mut near = Vec::new();
        for (k, q) in &queries.queries {
            let hash = crate::name_hash(&q.query);
            let limit = q.limit.unwrap_or(5).clamp(1, 20) as usize;
            match self.bucket(&hash[..BUCKET_CHARS]).await {
                Ok(bucket) => match bucket.names.get(&hash) {
                    Some(exact) if !exact.is_empty() => {
                        let result = exact.iter().take(limit).cloned().collect();
                        found.insert(k.clone(), Candidates { result });
                    }
                    _ => near.push((k, q)),
                },
                Err(Error::Unreachable) => return Err(Error::Unreachable),
                Err(_) => near.push((k, q)),
            }
        }
        let all = near;
        for chunk in all.chunks(MAX_QUERIES) {
            let body = MatchRequest {
                queries: chunk
                    .iter()
                    .map(|(k, q)| ((*k).clone(), (*q).clone()))
                    .collect(),
            };
            let url = self.url("/match");
            let got: BTreeMap<String, Candidates> = self
                .send(|| self.http.post(&url).json(&body))
                .await?
                .ok_or(Error::Unreadable)?;
            found.extend(got);
        }
        Ok(found)
    }

    /// One bucket of the name index, from those fetched lately or asked
    /// for now. A graph without a name index has empty buckets.
    pub async fn bucket(&self, prefix: &str) -> Result<Arc<Bucket>> {
        let kept = self.buckets.lock().ok().and_then(|b| {
            b.get(prefix)
                .filter(|(at, _)| at.elapsed() < BUCKET_KEPT)
                .map(|(_, bucket)| bucket.clone())
        });
        if let Some(bucket) = kept {
            return Ok(bucket);
        }
        if prefix.len() != BUCKET_CHARS || !prefix.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(Arc::default());
        }
        let url = self.url(&format!("/names/{prefix}"));
        let bucket: Arc<Bucket> =
            Arc::new(self.send(|| self.http.get(&url)).await?.unwrap_or_default());
        if let Ok(mut b) = self.buckets.lock() {
            if b.len() >= MOST_BUCKETS {
                b.clear();
            }
            b.insert(prefix.to_string(), (Instant::now(), bucket.clone()));
        }
        Ok(bucket)
    }

    /// Ideas by ID, following redirects for ideas that were merged.
    pub async fn ideas(&self, ids: &[String]) -> Result<Ideas> {
        let mut all = Ideas::default();
        let mut ids: Vec<&String> = ids.iter().collect();
        ids.sort();
        ids.dedup();
        for chunk in ids.chunks(MAX_IDS) {
            let url = self.url("/ideas");
            let ids = chunk
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let got: Ideas = self
                .send(|| self.http.get(&url).query(&[("ids", ids.as_str())]))
                .await?
                .ok_or(Error::Unreadable)?;
            all.ideas.extend(got.ideas);
            all.redirects.extend(got.redirects);
            all.split.extend(got.split);
            all.missing.extend(got.missing);
        }
        Ok(all)
    }

    /// How ideas relate, both ways, by their IDs, following redirects: what
    /// each rests on, is a kind of, answers, its alternatives, what it's used
    /// with, what rests on it, and the first principles it comes down to.
    pub async fn relations(&self, ids: &[String]) -> Result<Relations> {
        let mut all = Relations::default();
        let mut ids: Vec<&String> = ids.iter().filter(|i| kelve_core::ideas::is_id(i)).collect();
        ids.sort();
        ids.dedup();
        for chunk in ids.chunks(crate::MOST_RELATIONS) {
            let url = self.url("/relations");
            let ids = chunk
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let got: Relations = self
                .send(|| self.http.get(&url).query(&[("ids", ids.as_str())]))
                .await?
                .ok_or(Error::Unreadable)?;
            all.relations.extend(got.relations);
            all.redirects.extend(got.redirects);
            all.split.extend(got.split);
            all.missing.extend(got.missing);
        }
        Ok(all)
    }

    /// A page of what rests on an idea, most read first: `after` is the
    /// last page's `next`.
    pub async fn up(&self, id: &str, after: Option<&str>) -> Result<UpPage> {
        if !kelve_core::ideas::is_id(id) {
            return Ok(UpPage::default());
        }
        let url = self.url(&format!("/relations/{id}/up"));
        self.send(|| {
            let req = self.http.get(&url);
            match after {
                Some(a) => req.query(&[("after", a)]),
                None => req,
            }
        })
        .await?
        .ok_or(Error::Unreadable)
    }

    /// The ground floor: every idea that's a first principle.
    pub async fn foundations(&self) -> Result<Foundations> {
        let url = self.url("/foundations");
        self.send(|| self.http.get(&url))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// The instructions for one kind of job, at their current version: what
    /// a person's agent is told to do. None for a kind the graph doesn't
    /// know.
    pub async fn instructions(&self, name: &str) -> Result<Option<Instructions>> {
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Ok(None);
        }
        let url = self.url(&format!("/instructions/{name}"));
        self.send(|| self.http.get(&url)).await
    }

    /// The terms a person agrees to before adding to the graph.
    pub async fn terms(&self) -> Result<Terms> {
        let url = self.url("/terms");
        self.send(|| self.http.get(&url))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// Agrees to a version of the terms, as the person signed in.
    pub async fn agree(&self, version: &str) -> Result<Account> {
        let token = self.token.as_ref().ok_or(Error::SignIn)?;
        let url = self.url("/terms");
        let agree = Agree {
            version: version.to_string(),
        };
        self.send(|| json(self.http.post(&url), &agree).bearer_auth(token))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// The person's account: the name the graph made up for them, and the
    /// terms they agreed to.
    pub async fn account(&self) -> Result<Account> {
        let token = self.token.as_ref().ok_or(Error::SignIn)?;
        let url = self.url("/account");
        self.send(|| self.http.get(&url).bearer_auth(token))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// How the graph's check of an entry this account wrote stands, by the
    /// revision `apply` gave it. None when there's no such revision of
    /// theirs.
    pub async fn checking(&self, revision: &str) -> Result<Option<Checking>> {
        let token = self.token.as_ref().ok_or(Error::SignIn)?;
        if revision.is_empty() || !revision.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Ok(None);
        }
        let url = self.url(&format!("/revisions/{revision}/check"));
        self.send(|| self.http.get(&url).bearer_auth(token)).await
    }

    /// An idea's entry: the current one, or one revision of it. None when
    /// the idea has no entry yet, or there's no such revision.
    pub async fn entry(&self, id: &str, revision: Option<&str>) -> Result<Option<EntryPage>> {
        if !kelve_core::ideas::is_id(id) {
            return Ok(None);
        }
        // A revision never changes, so it has a URL of its own that can be
        // kept for good; the current entry is a redirect to one.
        let url = match revision {
            Some(r) if !r.is_empty() && r.chars().all(|c| c.is_ascii_alphanumeric()) => {
                self.url(&format!("/entries/{id}/revisions/{r}"))
            }
            Some(_) => return Ok(None),
            None => self.url(&format!("/entries/{id}")),
        };
        self.send(|| self.http.get(&url)).await
    }

    /// Applies a changeset, as the person signed in. `key` makes a retry
    /// safe: the graph gives the same key its first answer, and applies it
    /// once.
    pub async fn apply(&self, changes: &Changeset, key: &str) -> Result<Applied> {
        let Some(token) = &self.token else {
            return Err(Error::SignIn);
        };
        let url = self.url("/changesets");
        self.send(|| {
            json(self.http.post(&url), changes)
                .bearer_auth(token)
                .header("idempotency-key", key)
        })
        .await?
        .ok_or(Error::Unreadable)
    }

    /// The same graph, writing as `token`: the key this kelve got when the
    /// person signed in, or none.
    pub fn with_token(&self, token: Option<String>) -> Client {
        Client {
            base: self.base.clone(),
            token,
            reading_as: self.reading_as.clone(),
            tokens: self.tokens.clone(),
            http: self.http.clone(),
            buckets: self.buckets.clone(),
        }
    }

    /// The same graph, reading as `account` but writing as no one: its
    /// reading tokens are asked for with that account's key.
    pub fn reading_as(self, account: &str) -> Client {
        Client {
            reading_as: Some(account.to_string()),
            ..self
        }
    }

    /// A reading token to spend: one on hand, or one of a new batch, signed
    /// blind with the account's key, so the graph can't tell whose it is
    /// when it's read with. None for someone not signed in, or when the
    /// graph won't give more today.
    async fn reading_token(&self) -> Option<String> {
        let account = self.reading_as.as_ref().or(self.token.as_ref())?;
        let mut pool = self.tokens.lock().await;
        if let Some(t) = pool.tokens.pop() {
            return Some(t);
        }
        let key = match &pool.key {
            Some((key, at)) if at.elapsed() < TOKEN_KEY_KEPT => key.clone(),
            _ => {
                // Asked with no account, so the key can't be one for this
                // account alone.
                let res = self.http.get(self.url("/tokens/key")).send().await.ok()?;
                let key: crate::tokens::TokenKey =
                    serde_json::from_slice(&capped(res).await.ok()?).ok()?;
                pool.key = Some((key.clone(), Instant::now()));
                key
            }
        };
        let (pending, ask) = crate::tokens::blind(&key, TOKENS_AT_ONCE).ok()?;
        let res = self
            .http
            .post(self.url("/account/tokens"))
            .bearer_auth(account)
            .json(&ask)
            .send()
            .await
            .ok()
            .filter(|r| r.status().is_success())?;
        let grant: crate::tokens::TokenGrant =
            serde_json::from_slice(&capped(res).await.ok()?).ok()?;
        match crate::tokens::finish(pending, &grant) {
            Ok(tokens) => pool.tokens = tokens,
            // Signed with a key the graph doesn't show everyone: none of
            // them, and the key is asked for again.
            Err(_) => pool.key = None,
        }
        pool.tokens.pop()
    }

    /// Forgets the tokens on hand and their key, when the graph turned one
    /// away: its key changed.
    async fn forget_tokens(&self) {
        *self.tokens.lock().await = Pool::default();
    }

    /// Where to send the person's browser to sign in. The graph sends it on
    /// to GitHub, or Google with `provider` `google`, then back to
    /// `redirect`, on the person's own computer, with a one-time code and
    /// `state`. `challenge` is the PKCE challenge for a verifier only this
    /// kelve holds.
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
    /// retried: the graph takes a code once.
    pub async fn exchange(&self, code: &SignInCode) -> Result<SignedIn> {
        let url = self.url("/signin/token");
        let res = json(self.http.post(&url), code)
            .send()
            .await
            .map_err(|_| Error::Unreachable)?;
        let status = res.status();
        if !status.is_success() {
            return Err(Error::Refused {
                status: status.as_u16(),
                said: said(res, status).await,
            });
        }
        let body = capped(res).await?;
        serde_json::from_slice(&body).map_err(|_| Error::Unreadable)
    }

    /// Has the graph forget this kelve's key. A key it already forgot is
    /// fine.
    pub async fn sign_out(&self) -> Result<()> {
        let Some(token) = &self.token else {
            return Ok(());
        };
        let url = self.url("/signin");
        let res = self
            .http
            .delete(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| Error::Unreachable)?;
        let status = res.status();
        if status.is_success() || status == reqwest::StatusCode::UNAUTHORIZED {
            return Ok(());
        }
        Err(Error::Refused {
            status: status.as_u16(),
            said: said(res, status).await,
        })
    }

    /// Deletes the person's account on the graph: it forgets who they are,
    /// and this kelve's key, and every other, stops working.
    pub async fn delete_account(&self) -> Result<()> {
        let Some(token) = &self.token else {
            return Err(Error::SignIn);
        };
        let url = self.url("/account");
        let res = self
            .http
            .delete(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| Error::Unreachable)?;
        let status = res.status();
        if status.is_success() {
            return Ok(());
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::SignIn);
        }
        Err(Error::Refused {
            status: status.as_u16(),
            said: said(res, status).await,
        })
    }

    /// Tells the graph an entry, or a link in it, is wrong, so it's checked
    /// again. No account goes with it.
    pub async fn flag(&self, flag: &Flag) -> Result<()> {
        let url = self.url("/flags");
        let _: Option<serde_json::Value> = self.send(|| json(self.http.post(&url), flag)).await?;
        Ok(())
    }

    /// Tells the graph a reader found an entry didn't help them
    /// understand, and what it needs. No account goes with it.
    pub async fn ask(&self, ask: &crate::Ask) -> Result<()> {
        let url = self.url("/asks");
        let _: Option<serde_json::Value> = self.send(|| json(self.http.post(&url), ask)).await?;
        Ok(())
    }

    /// Sends a week's counts, which the person chose to share. No account
    /// goes with them.
    pub async fn send_counts(&self, counts: &Counts) -> Result<()> {
        let url = self.url("/counts");
        // Nor a reading token: the install's ID never goes beside anything
        // the graph gave an account.
        let _: Option<serde_json::Value> = self
            .send_as(|| json(self.http.post(&url), counts), false)
            .await?;
        Ok(())
    }

    /// Sends the relations readers followed. No account goes with them.
    pub async fn send_follows(&self, follows: &crate::Follows) -> Result<()> {
        let url = self.url("/follows");
        let _: Option<serde_json::Value> =
            self.send(|| json(self.http.post(&url), follows)).await?;
        Ok(())
    }

    /// The live lessons about ideas, by their IDs, best first.
    pub async fn lessons(&self, ids: &[String]) -> Result<crate::Lessons> {
        let mut all = crate::Lessons::default();
        let mut ids: Vec<&String> = ids.iter().filter(|i| kelve_core::ideas::is_id(i)).collect();
        ids.sort();
        ids.dedup();
        for chunk in ids.chunks(MAX_IDS) {
            let url = self.url("/lessons");
            let ids = chunk
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(",");
            let got: crate::Lessons = self
                .send(|| self.http.get(&url).query(&[("ids", ids.as_str())]))
                .await?
                .ok_or(Error::Unreadable)?;
            all.lessons.extend(got.lessons);
        }
        Ok(all)
    }

    /// One lesson: live, or, to its author, in whatever state it's in. None
    /// when there's no such lesson, or it isn't one to show.
    pub async fn lesson(&self, id: &str) -> Result<Option<crate::LessonPage>> {
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':') {
            return Ok(None);
        }
        let url = self.url(&format!("/lessons/{id}"));
        // As its author, when signed in: the graph shows a lesson that isn't
        // live only to whoever wrote it.
        self.send(|| match &self.token {
            Some(token) => self.http.get(&url).bearer_auth(token),
            None => self.http.get(&url),
        })
        .await
    }

    /// Adds a lesson, as the person signed in. `key` makes a retry safe.
    pub async fn add_lesson(&self, lesson: &crate::Lesson, key: &str) -> Result<crate::LessonPage> {
        let Some(token) = &self.token else {
            return Err(Error::SignIn);
        };
        let url = self.url("/lessons");
        self.send(|| {
            json(self.http.post(&url), lesson)
                .bearer_auth(token)
                .header("idempotency-key", key)
        })
        .await?
        .ok_or(Error::Unreadable)
    }

    /// Sends what came of lessons agents were given, as the person signed
    /// in, so one project counts once.
    pub async fn send_outcomes(&self, outcomes: &crate::Outcomes) -> Result<()> {
        let token = self.token.as_ref().ok_or(Error::SignIn)?;
        let url = self.url("/lessons/outcomes");
        let _: Option<serde_json::Value> = self
            .send(|| json(self.http.post(&url), outcomes).bearer_auth(token))
            .await?;
        Ok(())
    }

    /// What the signed-in account has added, and how often it was read.
    pub async fn added(&self) -> Result<crate::Added> {
        let token = self.token.as_ref().ok_or(Error::SignIn)?;
        let url = self.url("/account/added");
        self.send(|| self.http.get(&url).bearer_auth(token))
            .await?
            .ok_or(Error::Unreadable)
    }

    /// Sends a request, retrying what's worth retrying. A 404 is None.
    async fn send<T: DeserializeOwned>(
        &self,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<Option<T>> {
        self.send_as(build, true).await
    }

    /// Sends a request as `send` does; with `reading` false, never with a
    /// reading token.
    async fn send_as<T: DeserializeOwned>(
        &self,
        build: impl Fn() -> reqwest::RequestBuilder,
        reading: bool,
    ) -> Result<Option<T>> {
        let mut wait = FIRST_WAIT;
        let mut tokens_again = false;
        for attempt in 1..=ATTEMPTS {
            let last = attempt == ATTEMPTS;
            let res = match self.execute(build(), reading).await {
                Ok(res) => res,
                Err(e) if !last && (e.is_connect() || e.is_timeout() || e.is_request()) => {
                    tokio::time::sleep(jittered(wait)).await;
                    wait *= 2;
                    continue;
                }
                Err(_) => return Err(Error::Unreachable),
            };
            let status = res.status();
            if status.is_success() {
                let body = capped(res).await?;
                return serde_json::from_slice(&body)
                    .map(Some)
                    .map_err(|_| Error::Unreadable);
            }
            if status == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            let busy = status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status == reqwest::StatusCode::SERVICE_UNAVAILABLE;
            if busy || status.is_server_error() {
                let asked = retry_after(&res);
                let problem = if busy { problem(res).await } else { None };
                // The day's limit on what the account adds: nothing more
                // goes until it resets, however soon it's asked again.
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS
                    && let Some(p) = problem
                        .as_ref()
                        .filter(|p| p.title == crate::ENOUGH_FOR_TODAY)
                {
                    return Err(Error::Enough(Limit::from_problem(
                        &p.detail,
                        asked.unwrap_or_else(to_midnight),
                    )));
                }
                // A long wait the graph asks for, a limit or the graph not
                // taking changes for a while, comes with its own sentence.
                if let Some(after) = asked.filter(|a| *a > LONGEST_WAIT)
                    && busy
                {
                    return Err(Error::Limited {
                        said: said_in(problem, status),
                        after,
                    });
                }
                if last || asked.is_some_and(|a| a > LONGEST_WAIT) {
                    return Err(if busy {
                        Error::Busy { after: asked }
                    } else {
                        Error::Unreachable
                    });
                }
                tokio::time::sleep(asked.unwrap_or_else(|| jittered(wait))).await;
                wait *= 2;
                continue;
            }
            if status == reqwest::StatusCode::UNAUTHORIZED {
                // A token turned away, its key changed since: new ones, once.
                if !tokens_again && (self.reading_as.is_some() || self.token.is_some()) {
                    tokens_again = true;
                    self.forget_tokens().await;
                    continue;
                }
                return Err(Error::SignIn);
            }
            return Err(Error::Refused {
                status: status.as_u16(),
                said: said(res, status).await,
            });
        }
        Err(Error::Unreachable)
    }

    /// Sends a request. One that doesn't carry the account's key, a read,
    /// spends a reading token: the graph is read only by someone signed in,
    /// and never knows who.
    async fn execute(
        &self,
        req: reqwest::RequestBuilder,
        reading: bool,
    ) -> reqwest::Result<reqwest::Response> {
        use reqwest::header::{AUTHORIZATION, LOCATION};
        let mut req = req.build()?;
        let mut hops = 0;
        loop {
            let read = reading && !req.headers().contains_key(AUTHORIZATION);
            if read
                && let Some(token) = self.reading_token().await
                && let Ok(mut bearer) = reqwest::header::HeaderValue::from_str(&format!(
                    "{} token={token}",
                    crate::tokens::SCHEME
                ))
            {
                bearer.set_sensitive(true);
                req.headers_mut().insert(AUTHORIZATION, bearer);
            }
            let again = (req.method() == reqwest::Method::GET)
                .then(|| req.try_clone())
                .flatten();
            let res = self.http.execute(req).await?;
            let to = res
                .headers()
                .get(LOCATION)
                .and_then(|l| l.to_str().ok())
                .and_then(|l| res.url().join(l).ok());
            match (res.status().is_redirection(), again, to) {
                (true, Some(mut next), Some(to)) if hops < 5 => {
                    hops += 1;
                    // A read's token is spent; and no key goes to anywhere
                    // but the graph.
                    if read || to.origin() != next.url().origin() {
                        next.headers_mut().remove(AUTHORIZATION);
                    }
                    *next.url_mut() = to;
                    req = next;
                }
                _ => return Ok(res),
            }
        }
    }
}

/// What the graph said, from its problem details.
async fn said(res: reqwest::Response, status: reqwest::StatusCode) -> String {
    said_in(problem(res).await, status)
}

/// An answer's problem details, when it has them.
async fn problem(res: reqwest::Response) -> Option<Problem> {
    capped(res)
        .await
        .ok()
        .and_then(|b| serde_json::from_slice::<Problem>(&b).ok())
}

/// What problem details say, or the status when there are none.
fn said_in(problem: Option<Problem>, status: reqwest::StatusCode) -> String {
    problem
        .map(|p| {
            if p.detail.is_empty() {
                p.title
            } else {
                p.detail
            }
        })
        .unwrap_or_else(|| status.to_string())
}

fn json<T: Serialize>(req: reqwest::RequestBuilder, body: &T) -> reqwest::RequestBuilder {
    req.json(body)
}

/// An answer's body, refused once it's larger than `MAX_BODY`.
async fn capped(mut res: reqwest::Response) -> Result<Vec<u8>> {
    if res.content_length().is_some_and(|n| n > MAX_BODY as u64) {
        return Err(Error::Unreadable);
    }
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|_| Error::Unreachable)? {
        if body.len() + chunk.len() > MAX_BODY {
            return Err(Error::Unreadable);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// How long the graph asked to be left, in seconds, as `Retry-After` says.
fn retry_after(res: &reqwest::Response) -> Option<Duration> {
    res.headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Full jitter: anywhere from nothing to `wait`, so many kelves that lost
/// the graph together don't all come back together.
fn jittered(wait: Duration) -> Duration {
    let mut bytes = [0u8; 8];
    let r = getrandom::fill(&mut bytes)
        .map(|()| u64::from_le_bytes(bytes))
        .unwrap_or(0);
    let ms = u64::try_from(wait.as_millis()).unwrap_or(u64::MAX).max(1);
    Duration::from_millis(r % ms)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use axum::Router;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::get;

    use super::*;

    async fn serve(router: Router) -> Client {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.ok() });
        Client::new(&format!("http://{addr}"), None).unwrap()
    }

    fn manifest() -> String {
        serde_json::to_string(&Manifest {
            space: "kelve".into(),
            name: "test".into(),
            version: "1".into(),
            min_client: None,
        })
        .unwrap()
    }

    #[tokio::test]
    async fn a_failing_graph_is_tried_again_then_answers() {
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let g = serve(Router::new().route(
            "/v1",
            get(move || {
                let n = seen.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n == 0 {
                        (StatusCode::BAD_GATEWAY, String::new())
                    } else {
                        (StatusCode::OK, manifest())
                    }
                }
            }),
        ))
        .await;
        assert_eq!(g.manifest().await.unwrap().space, "kelve");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_graph_that_asks_for_a_long_wait_is_left_alone() {
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let g = serve(Router::new().route(
            "/v1",
            get(move || {
                seen.fetch_add(1, Ordering::SeqCst);
                async {
                    let mut h = HeaderMap::new();
                    h.insert("retry-after", "3600".parse().unwrap());
                    (StatusCode::TOO_MANY_REQUESTS, h, String::new())
                }
            }),
        ))
        .await;
        assert!(
            matches!(g.manifest().await.unwrap_err(), Error::Limited { after, .. } if after.as_secs() == 3600)
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    /// A graph whose changesets all get a 429 with `title`, `detail` and a
    /// `Retry-After` of `after` seconds, and how many it was sent.
    async fn refusing(
        title: &'static str,
        detail: &'static str,
        after: &'static str,
    ) -> (Client, Arc<AtomicU32>) {
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let g = serve(Router::new().route(
            "/v1/changesets",
            axum::routing::post(move || {
                seen.fetch_add(1, Ordering::SeqCst);
                async move {
                    let mut h = HeaderMap::new();
                    h.insert("retry-after", after.parse().unwrap());
                    let p = Problem {
                        kind: "about:blank".into(),
                        title: title.into(),
                        status: 429,
                        detail: detail.into(),
                    };
                    (
                        StatusCode::TOO_MANY_REQUESTS,
                        h,
                        serde_json::to_string(&p).unwrap(),
                    )
                }
            }),
        ))
        .await;
        (g.with_token(Some("t".into())), calls)
    }

    fn changeset() -> Changeset {
        Changeset {
            agent: AGENT.into(),
            comment: String::new(),
            changes: vec![],
            written_by: None,
        }
    }

    #[tokio::test]
    async fn the_days_limit_is_told_from_busy_with_what_ran_out_and_when_it_resets() {
        let (g, calls) = refusing(
            crate::ENOUGH_FOR_TODAY,
            "You’ve added as many ideas as a new account can in a day. kelve will send the rest tomorrow; the limit rises once ten of your entries are accepted, or after a week.",
            "37000",
        )
        .await;
        let e = g.apply(&changeset(), "k").await.unwrap_err();
        assert_eq!(
            e,
            Error::Enough(Limit {
                what: Some(Quota::Ideas),
                new_account: true,
                after: Duration::from_secs(37000),
            })
        );
        assert_eq!(
            e.to_string(),
            "You’ve added as many ideas as a new account can today. kelve sends the rest once the limit resets."
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1, "a limit is never retried");

        // Even when it resets in a moment, and for an account past its first week.
        let (g, calls) = refusing(
            crate::ENOUGH_FOR_TODAY,
            "You’ve added as many relations as you can today. kelve will send the rest tomorrow.",
            "3",
        )
        .await;
        assert_eq!(
            g.apply(&changeset(), "k").await.unwrap_err(),
            Error::Enough(Limit {
                what: Some(Quota::Relations),
                new_account: false,
                after: Duration::from_secs(3),
            })
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn too_many_requests_waits_as_asked_then_says_busy_with_the_wait() {
        let (g, calls) = refusing(
            "Too many requests",
            "That’s more than the shared graph takes at once. kelve will try again shortly.",
            "1",
        )
        .await;
        let started = Instant::now();
        assert_eq!(
            g.apply(&changeset(), "k").await.unwrap_err(),
            Error::Busy {
                after: Some(Duration::from_secs(1))
            }
        );
        assert_eq!(calls.load(Ordering::SeqCst), ATTEMPTS);
        // A second between each try, as Retry-After asked.
        assert!(started.elapsed() >= Duration::from_secs(u64::from(ATTEMPTS - 1)));
    }

    #[tokio::test]
    async fn a_refusal_says_what_the_graph_said_and_is_not_retried() {
        let calls = Arc::new(AtomicU32::new(0));
        let seen = calls.clone();
        let g = serve(Router::new().route(
            "/v1",
            get(move || {
                seen.fetch_add(1, Ordering::SeqCst);
                async {
                    let p = Problem {
                        kind: "about:blank".into(),
                        title: "Bad request".into(),
                        status: 400,
                        detail: "That isn’t an idea ID.".into(),
                    };
                    (StatusCode::BAD_REQUEST, serde_json::to_string(&p).unwrap())
                }
            }),
        ))
        .await;
        let e = g.manifest().await.unwrap_err();
        assert_eq!(
            e.to_string(),
            "The shared graph of ideas said no: That isn’t an idea ID."
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn an_unreachable_graph_says_to_check_the_connection() {
        let g = Client::new("http://127.0.0.1:9", None).unwrap();
        assert_eq!(g.manifest().await.unwrap_err(), Error::Unreachable);
    }

    #[tokio::test]
    async fn writing_needs_an_account_and_each_read_spends_a_token_never_the_account() {
        use crate::tokens::{Issuer, TokenAsk, from_header};
        let issuer = Arc::new(Issuer::from_seed(&[1u8; 32]).unwrap());
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let (check, key, sign, kept) =
            (issuer.clone(), issuer.clone(), issuer.clone(), seen.clone());
        let g = serve(
            Router::new()
                .route(
                    "/v1",
                    get(move |h: HeaderMap| {
                        let (check, kept) = (check.clone(), kept.clone());
                        async move {
                            let said = h
                                .get("authorization")
                                .and_then(|v| v.to_str().ok())
                                .unwrap_or_default();
                            let token =
                                from_header(said).expect("a read carries a token, not the account");
                            assert!(check.check(token).is_some(), "one the graph signed");
                            kept.lock().unwrap().push(token.to_string());
                            manifest()
                        }
                    }),
                )
                .route(
                    "/v1/tokens/key",
                    get(move |h: HeaderMap| {
                        let key = key.clone();
                        async move {
                            assert!(
                                h.get("authorization").is_none(),
                                "the key is asked for as no one"
                            );
                            axum::Json(key.key().clone())
                        }
                    }),
                )
                .route(
                    "/v1/account/tokens",
                    axum::routing::post(
                        move |h: HeaderMap, axum::Json(ask): axum::Json<TokenAsk>| {
                            let sign = sign.clone();
                            async move {
                                assert_eq!(
                                    h.get("authorization").and_then(|v| v.to_str().ok()),
                                    Some("Bearer t")
                                );
                                axum::Json(sign.sign(&ask, 900).unwrap())
                            }
                        },
                    ),
                ),
        )
        .await;
        let signed_in = Client::new(g.base(), Some("t".into())).unwrap();
        signed_in.manifest().await.unwrap();
        signed_in.manifest().await.unwrap();
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 2);
        assert_ne!(seen[0], seen[1], "each read spends its own");
        // Not signed in, nothing to read with.
        assert!(
            Client::new(g.base(), None)
                .unwrap()
                .reading_token()
                .await
                .is_none()
        );
        let cs = Changeset {
            agent: AGENT.into(),
            comment: String::new(),
            changes: vec![],
            written_by: None,
        };
        assert_eq!(g.apply(&cs, "k").await.unwrap_err(), Error::SignIn);
    }

    #[tokio::test]
    async fn a_name_the_graph_has_is_found_in_its_bucket_and_never_sent() {
        use crate::{Candidate, Query, name_hash};

        let wal = name_hash("Write-ahead logging");
        let asked = Arc::new(Mutex::new(Vec::<String>::new()));
        let (seen, buckets) = (asked.clone(), Arc::new(AtomicU32::new(0)));
        let fetched = buckets.clone();
        let hash = wal.clone();
        let g = serve(
            Router::new()
                .route(
                    "/v1/names/{prefix}",
                    get(
                        move |axum::extract::Path(prefix): axum::extract::Path<String>| {
                            fetched.fetch_add(1, Ordering::SeqCst);
                            let mut bucket = Bucket::default();
                            if hash.starts_with(&prefix) {
                                bucket.names.insert(
                                    hash.clone(),
                                    vec![Candidate {
                                        id: "kelve:i000000000001".into(),
                                        name: "Write-ahead logging".into(),
                                        definition: "Logging each change before making it.".into(),
                                        score: 1.0,
                                        matched: true,
                                        same_as: None,
                                    }],
                                );
                            }
                            async move { axum::Json(bucket) }
                        },
                    ),
                )
                .route(
                    "/v1/match",
                    axum::routing::post(move |axum::Json(req): axum::Json<MatchRequest>| {
                        seen.lock()
                            .unwrap()
                            .extend(req.queries.values().map(|q| q.query.clone()));
                        async move { axum::Json(BTreeMap::<String, Candidates>::new()) }
                    }),
                ),
        )
        .await;
        let ask = |names: &[&str]| MatchRequest {
            queries: names
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    (
                        i.to_string(),
                        Query {
                            query: (*n).into(),
                            context: None,
                            limit: None,
                        },
                    )
                })
                .collect(),
        };
        let found = g
            .candidates(&ask(&["write ahead logging", "Our billing service"]))
            .await
            .unwrap();
        assert!(found["0"].result[0].matched);
        assert_eq!(
            *asked.lock().unwrap(),
            vec!["Our billing service".to_string()],
            "only the name the graph lacks is sent"
        );
        let before = buckets.load(Ordering::SeqCst);
        g.candidates(&ask(&["Write-Ahead  Logging"])).await.unwrap();
        assert_eq!(
            buckets.load(Ordering::SeqCst),
            before,
            "a bucket is kept a while"
        );
    }

    #[test]
    fn jitter_never_waits_longer_than_asked() {
        for _ in 0..100 {
            assert!(jittered(Duration::from_millis(250)) < Duration::from_millis(250));
        }
    }
}
