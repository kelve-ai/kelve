//! A stand-in for the hosted service's version 2: the same API, held in
//! memory, for kelve's tests and its sandbox. It keeps the rules code can
//! keep (an address's form, a character's shape, a day's runs) and fakes
//! the rest: a run moves on one task each time it's asked how it stands,
//! and comes out better on most tasks when the skills have one called
//! `standin-expert`, as the real runner's stand-in agent does, and about the
//! same otherwise. It serves no pages.
//!
//! The version 1 stand-in serves these routes too, on its own port, so one
//! address stands in for the whole service.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query as Params, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use base64::Engine;
use kelve_core::time::Timestamp;
use serde::Deserialize;

use super::{
    Account, Allowance, Character, Deleted, Estimate, HandleAsk, HandleCheck, ModelOffer,
    NO_RUNS_LEFT, PageSettings, RunRequest, RunResult, RunState, RunStatus, Score, Scores,
    SignedIn, SkillFile, TaskProgress, TaskState, Verdict, handle,
};
use crate::{Problem, SignInCode, pkce_challenge};

/// The account the stand-in knows, as version 1's does.
pub const KEY: &str = crate::stand_in::TOKEN;

pub use super::sample::{EXPERT, takes_key, the_offer};

/// Where its pages would be.
const SITE: &str = "https://kelve.ai";

#[derive(Debug, Default)]
struct Held {
    /// Accounts by key.
    accounts: BTreeMap<String, Held1>,
    /// Addresses someone else has, or kelve keeps.
    taken: BTreeSet<String>,
    runs: BTreeMap<String, Run>,
    /// Sign-ins under way: each code, its challenge and where it went.
    codes: BTreeMap<String, (String, String)>,
    minted: u64,
    a_day: u32,
}

#[derive(Debug, Default)]
struct Held1 {
    handle: Option<String>,
    name: Option<String>,
    public: bool,
    character: Option<Character>,
    scores: Vec<Score>,
    runs_today: u32,
}

#[derive(Debug)]
struct Run {
    key: String,
    estimate: Estimate,
    state: RunState,
    tasks: Vec<TaskProgress>,
    verdicts: Vec<Verdict>,
    score: Option<String>,
}

/// The stand-in's hosted service: what tests see of it, and change.
#[derive(Clone, Debug)]
pub struct Hosted {
    held: Arc<Mutex<Held>>,
}

impl Default for Hosted {
    fn default() -> Self {
        let mut held = Held {
            a_day: 1,
            ..Held::default()
        };
        held.accounts.insert(KEY.into(), Held1::default());
        held.taken
            .extend(["alex", "anna", "jo", "robot", "kelve", "admin", "help"].map(String::from));
        Hosted {
            held: Arc::new(Mutex::new(held)),
        }
    }
}

impl Hosted {
    /// Serves on a free local port of its own: the address.
    pub async fn start(&self) -> std::io::Result<String> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let app = self.routes();
        tokio::spawn(async move { axum::serve(listener, app).await.ok() });
        Ok(url)
    }

    /// Its routes, to serve beside others.
    pub fn routes(&self) -> Router {
        Router::new()
            .route("/v2/signin", get(sign_in).delete(sign_out))
            .route("/v2/signin/token", post(sign_in_token))
            .route("/v2/account", get(account).delete(delete_account))
            .route("/v2/account/handle", put(choose_handle))
            .route("/v2/account/page", put(set_page))
            .route("/v2/account/character", get(character).put(set_character))
            .route("/v2/handles/check", post(check_handle))
            .route("/v2/scores", get(scores))
            .route("/v2/scores/{id}/published", put(publish).delete(unpublish))
            .route("/v2/runs/offer", get(offer))
            .route("/v2/runs", post(estimate))
            .route("/v2/runs/{id}", get(status).delete(cancel))
            .route("/v2/runs/{id}/confirm", post(confirm))
            .route("/stand-in/v2", post(control))
            .with_state(self.held.clone())
    }

    /// Another account, signed in with `key`.
    pub fn add_account(&self, key: &str) {
        lock(&self.held)
            .accounts
            .insert(key.into(), Held1::default());
    }

    /// Someone else has `handle`.
    pub fn take(&self, handle: &str) {
        lock(&self.held).taken.insert(handle.into());
    }

    /// How many runs an account may start a day.
    pub fn runs_a_day(&self, n: u32) {
        lock(&self.held).a_day = n;
    }

    /// Finishes a run at once, as if every task had run.
    pub fn finish(&self, run: &str) {
        let mut held = lock(&self.held);
        while step(&mut held, run) {}
    }

    /// The account's page as the stand-in has it: its address, its name and
    /// whether it's public.
    pub fn page(&self, key: &str) -> Option<(Option<String>, Option<String>, bool)> {
        let held = lock(&self.held);
        held.accounts
            .get(key)
            .map(|a| (a.handle.clone(), a.name.clone(), a.public))
    }
}

fn lock(held: &Mutex<Held>) -> std::sync::MutexGuard<'_, Held> {
    held.lock().unwrap_or_else(|p| p.into_inner())
}

type Shared = State<Arc<Mutex<Held>>>;

fn problem(status: StatusCode, title: &str, detail: &str) -> Response {
    let p = Problem {
        kind: "about:blank".into(),
        title: title.into(),
        status: status.as_u16(),
        detail: detail.into(),
    };
    (status, Json(p)).into_response()
}

fn signed_out() -> Response {
    problem(
        StatusCode::UNAUTHORIZED,
        "Not signed in",
        "Sign in to kelve.ai from kelve.",
    )
}

/// The key a request is sent with, when it's an account's.
fn key_of(held: &Held, headers: &HeaderMap) -> Option<String> {
    let key = headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")?
        .trim();
    held.accounts.contains_key(key).then(|| key.to_string())
}

fn midnight() -> Timestamp {
    let now = chrono::Utc::now();
    let next = (now.date_naive() + chrono::Days::new(1))
        .and_hms_opt(0, 0, 0)
        .map_or(now, |t| t.and_utc());
    Timestamp(next)
}

fn account_of(held: &Held, key: &str) -> Account {
    let a = held.accounts.get(key);
    let handle = a.and_then(|a| a.handle.clone());
    Account {
        page: handle.as_ref().map(|h| format!("{SITE}/@{h}")),
        handle,
        name: a.and_then(|a| a.name.clone()),
        public: a.is_some_and(|a| a.public),
        character: a.is_some_and(|a| a.character.is_some()),
        runs: Allowance {
            left: held.a_day.saturating_sub(a.map_or(0, |a| a.runs_today)),
            a_day: held.a_day,
            more: midnight(),
        },
    }
}

#[derive(Deserialize)]
struct SignInParams {
    redirect_uri: String,
    state: String,
    code_challenge: String,
}

/// Signing in, with no GitHub: straight back to the person's kelve.
async fn sign_in(State(held): Shared, Params(p): Params<SignInParams>) -> Response {
    let Ok(mut back) = reqwest::Url::parse(&p.redirect_uri) else {
        return problem(
            StatusCode::BAD_REQUEST,
            "Bad address",
            "That isn't an address.",
        );
    };
    let mut held = lock(&held);
    held.minted += 1;
    let code = format!("code-{}", held.minted);
    held.codes
        .insert(code.clone(), (p.code_challenge, p.redirect_uri.clone()));
    back.query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &p.state);
    (
        StatusCode::SEE_OTHER,
        [(axum::http::header::LOCATION, back.to_string())],
    )
        .into_response()
}

async fn sign_in_token(State(held): Shared, Json(asked): Json<SignInCode>) -> Response {
    let mut held = lock(&held);
    match held.codes.remove(&asked.code) {
        Some((challenge, redirect))
            if redirect == asked.redirect_uri
                && challenge == pkce_challenge(&asked.code_verifier) =>
        {
            let handle = held.accounts.get(KEY).and_then(|a| a.handle.clone());
            Json(SignedIn {
                key: KEY.into(),
                handle,
            })
            .into_response()
        }
        _ => problem(
            StatusCode::BAD_REQUEST,
            "Can't sign in",
            "That sign-in has run out, or was used already. Sign in again from kelve.",
        ),
    }
}

async fn sign_out() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn account(State(held): Shared, headers: HeaderMap) -> Response {
    let held = lock(&held);
    match key_of(&held, &headers) {
        Some(key) => Json(account_of(&held, &key)).into_response(),
        None => signed_out(),
    }
}

async fn delete_account(State(held): Shared, headers: HeaderMap) -> Response {
    let mut held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    let gone = held.accounts.remove(&key).unwrap_or_default();
    held.runs.retain(|_, r| r.key != key);
    Json(Deleted {
        page: gone.handle.is_some(),
        scores: u32::try_from(gone.scores.len()).unwrap_or(u32::MAX),
    })
    .into_response()
}

/// Whether `typed` can be had by the account `key`.
fn checked(held: &Held, key: &str, typed: &str) -> HandleCheck {
    let normal = handle::normal(typed);
    let mine = held.accounts.get(key).and_then(|a| a.handle.as_deref());
    let refuse = |why: &str, instead: Vec<String>| HandleCheck {
        handle: normal.clone(),
        available: false,
        why: Some(why.into()),
        instead,
    };
    match handle::check(typed) {
        Err(why) => refuse(why, Vec::new()),
        Ok(h) if mine == Some(h.as_str()) => HandleCheck {
            handle: h,
            available: true,
            why: None,
            instead: Vec::new(),
        },
        Ok(h) if taken(held, &h) => {
            let instead = handle::instead(&h)
                .into_iter()
                .filter(|c| !taken(held, c))
                .take(2)
                .collect();
            refuse("Someone has that one.", instead)
        }
        Ok(h) => HandleCheck {
            handle: h,
            available: true,
            why: Some("You can have that one.".into()),
            instead: Vec::new(),
        },
    }
}

fn taken(held: &Held, h: &str) -> bool {
    held.taken.contains(h)
        || held
            .accounts
            .values()
            .any(|a| a.handle.as_deref() == Some(h))
}

async fn check_handle(
    State(held): Shared,
    headers: HeaderMap,
    Json(ask): Json<HandleAsk>,
) -> Response {
    let held = lock(&held);
    match key_of(&held, &headers) {
        Some(key) => Json(checked(&held, &key, &ask.handle)).into_response(),
        None => signed_out(),
    }
}

async fn choose_handle(
    State(held): Shared,
    headers: HeaderMap,
    Json(ask): Json<HandleAsk>,
) -> Response {
    let mut held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    let c = checked(&held, &key, &ask.handle);
    if !c.available {
        let status = if handle::check(&ask.handle).is_err() {
            StatusCode::UNPROCESSABLE_ENTITY
        } else {
            StatusCode::CONFLICT
        };
        return problem(
            status,
            "Can't have that address",
            c.why.as_deref().unwrap_or_default(),
        );
    }
    if let Some(a) = held.accounts.get_mut(&key) {
        a.handle = Some(c.handle);
    }
    Json(account_of(&held, &key)).into_response()
}

async fn set_page(
    State(held): Shared,
    headers: HeaderMap,
    Json(page): Json<PageSettings>,
) -> Response {
    let mut held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    let name = page
        .name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    if name
        .as_ref()
        .is_some_and(|n| n.chars().count() > super::MOST_NAME)
    {
        return problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Name too long",
            "Use 40 characters or fewer.",
        );
    }
    let Some(a) = held.accounts.get_mut(&key) else {
        return signed_out();
    };
    if page.public && a.handle.is_none() {
        return problem(
            StatusCode::CONFLICT,
            "No address yet",
            "Choose your page’s address before you publish it.",
        );
    }
    a.name = name;
    a.public = page.public;
    Json(account_of(&held, &key)).into_response()
}

async fn character(State(held): Shared, headers: HeaderMap) -> Response {
    let held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    match held.accounts.get(&key).and_then(|a| a.character.clone()) {
        Some(c) => Json(c).into_response(),
        None => problem(
            StatusCode::NOT_FOUND,
            "No character",
            "Your agent has no character yet.",
        ),
    }
}

async fn set_character(
    State(held): Shared,
    headers: HeaderMap,
    Json(c): Json<Character>,
) -> Response {
    let mut held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    if let Err(why) = c.check() {
        return problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Not a character kelve can show",
            &why,
        );
    }
    if let Some(a) = held.accounts.get_mut(&key) {
        a.character = Some(c);
    }
    Json(account_of(&held, &key)).into_response()
}

async fn scores(State(held): Shared, headers: HeaderMap) -> Response {
    let held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    let mut scores = held
        .accounts
        .get(&key)
        .map(|a| a.scores.clone())
        .unwrap_or_default();
    scores.reverse();
    Json(Scores { scores }).into_response()
}

async fn publish(State(held): Shared, headers: HeaderMap, Path(id): Path<String>) -> Response {
    published(held, headers, id, true)
}

async fn unpublish(State(held): Shared, headers: HeaderMap, Path(id): Path<String>) -> Response {
    published(held, headers, id, false)
}

fn published(held: Arc<Mutex<Held>>, headers: HeaderMap, id: String, on: bool) -> Response {
    let mut held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    let Some(a) = held.accounts.get_mut(&key) else {
        return signed_out();
    };
    if on && a.handle.is_none() {
        return problem(
            StatusCode::CONFLICT,
            "No address yet",
            "Choose your page’s address before you publish a score.",
        );
    }
    let Some(s) = a.scores.iter_mut().find(|s| s.id == id) else {
        return problem(
            StatusCode::NOT_FOUND,
            "No such score",
            "You have no such score.",
        );
    };
    s.published = on;
    let s = s.clone();
    // Publishing a score publishes the page it's on.
    if on {
        a.public = true;
    }
    Json(s).into_response()
}

async fn offer(State(held): Shared, headers: HeaderMap) -> Response {
    let held = lock(&held);
    match key_of(&held, &headers) {
        Some(_) => Json(the_offer()).into_response(),
        None => signed_out(),
    }
}

/// A model an agent can run on, when it's on offer.
fn model_offer(agent: &str, model: &str) -> Option<ModelOffer> {
    the_offer()
        .agents
        .into_iter()
        .find(|a| a.agent == agent)?
        .models
        .into_iter()
        .find(|m| m.model == model)
}

/// The names of a kind, agent and model, when they're on offer.
fn names(kind: &str, agent: &str, model: &str) -> Option<(String, String, String, u32, String)> {
    let offer = the_offer();
    let k = offer.kinds.iter().find(|k| k.kind == kind)?;
    let a = offer.agents.iter().find(|a| a.agent == agent)?;
    let m = a.models.iter().find(|m| m.model == model)?;
    Some((
        k.name.clone(),
        a.name.clone(),
        m.name.clone(),
        k.tasks,
        k.bank.clone(),
    ))
}

async fn estimate(
    State(held): Shared,
    headers: HeaderMap,
    Json(ask): Json<RunRequest>,
) -> Response {
    let mut held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    let Some((_, _, _, tasks, _)) = names(&ask.kind, &ask.agent, &ask.model) else {
        return problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Not on offer",
            "kelve.ai doesn’t run that kind of work, agent or model. Choose from what it offers.",
        );
    };
    let Ok(tar) = base64::engine::general_purpose::STANDARD.decode(ask.skills.as_bytes()) else {
        return problem(
            StatusCode::BAD_REQUEST,
            "Not a tar",
            "The skills didn’t arrive whole. Send them again.",
        );
    };
    if tar.len() > super::MOST_SKILLS {
        return problem(
            StatusCode::PAYLOAD_TOO_LARGE,
            "Too many skills",
            "Your skills come to more than a megabyte. Make them shorter, then try again.",
        );
    }
    let files = match super::tar::read(&tar) {
        Ok(files) => files,
        Err(why) => return problem(StatusCode::UNPROCESSABLE_ENTITY, "Not skills", &why),
    };
    let left = account_of(&held, &key).runs.left;
    if left == 0 {
        return no_runs_left();
    }
    let Some(provider) = model_offer(&ask.agent, &ask.model) else {
        return no_run();
    };
    held.minted += 1;
    let run = format!("run{}", held.minted);
    let expert = files
        .iter()
        .any(|f| f.path.split('/').next() == Some(EXPERT));
    let estimate = Estimate {
        run: run.clone(),
        kind: ask.kind.clone(),
        agent: ask.agent.clone(),
        model: ask.model.clone(),
        tasks,
        runs_per_task: 3,
        minutes: 20,
        provider: provider.provider.clone(),
        provider_name: provider.provider_name.clone(),
        dollars: if ask.model.contains("opus") {
            120.0
        } else {
            60.0
        },
        runs_left: left - 1,
        expires: Timestamp(chrono::Utc::now() + chrono::Duration::hours(1)),
        skills: files
            .iter()
            .map(|f| SkillFile {
                path: f.path.clone(),
                bytes: f.bytes.len() as u64,
            })
            .collect(),
        working: Vec::new(),
    };
    let pattern = if expert {
        "bbbbbbbbbbsw"
    } else {
        "bssssswsssss"
    };
    let verdicts: Vec<Verdict> = pattern
        .chars()
        .cycle()
        .take(tasks as usize)
        .map(|c| match c {
            'b' => Verdict::Better,
            'w' => Verdict::Worse,
            _ => Verdict::Same,
        })
        .collect();
    held.runs.insert(
        run,
        Run {
            key,
            estimate: estimate.clone(),
            state: RunState::Estimated,
            tasks: vec![
                TaskProgress {
                    state: TaskState::Waiting,
                    runs: 0,
                };
                tasks as usize
            ],
            verdicts,
            score: None,
        },
    );
    (StatusCode::CREATED, Json(estimate)).into_response()
}

fn no_runs_left() -> Response {
    let mut res = problem(
        StatusCode::TOO_MANY_REQUESTS,
        NO_RUNS_LEFT,
        "You’ve used today’s verified runs. More come at midnight UTC.",
    );
    let secs = (midnight().0 - chrono::Utc::now()).num_seconds().max(1);
    if let Ok(v) = axum::http::HeaderValue::from_str(&secs.to_string()) {
        res.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
    }
    res
}

fn status_of(held: &Held, id: &str) -> Option<RunStatus> {
    let r = held.runs.get(id)?;
    let e = &r.estimate;
    let result = (r.state == RunState::Finished).then(|| result_of(r));
    let left = r
        .tasks
        .iter()
        .filter(|t| matches!(t.state, TaskState::Waiting | TaskState::Running))
        .count();
    Some(RunStatus {
        run: id.into(),
        state: r.state,
        kind: e.kind.clone(),
        agent: e.agent.clone(),
        model: e.model.clone(),
        tasks: r.tasks.clone(),
        minutes_left: matches!(r.state, RunState::Queued | RunState::Running)
            .then(|| u32::try_from(left * 2).unwrap_or(u32::MAX)),
        why: None,
        result,
        score: r.score.clone(),
    })
}

fn result_of(r: &Run) -> RunResult {
    let e = &r.estimate;
    let (kind_name, agent_name, model_name, _, bank) =
        names(&e.kind, &e.agent, &e.model).unwrap_or_default();
    let n = r.verdicts.len() as u32;
    let better = r.verdicts.iter().filter(|v| **v == Verdict::Better).count() as u32;
    let worse = r.verdicts.iter().filter(|v| **v == Verdict::Worse).count() as u32;
    let possible = n * 3;
    let standard = possible * 2 / 3;
    let passed = (standard + better).saturating_sub(worse).min(possible);
    let gain = f64::from(better * 15) / f64::from(n.max(1)) * 1.2;
    RunResult {
        kind: e.kind.clone(),
        kind_name,
        agent: e.agent.clone(),
        agent_name,
        model: e.model.clone(),
        model_name,
        bank,
        tasks: r.verdicts.clone(),
        runs_per_task: 3,
        passed,
        possible,
        standard_passed: standard,
        gain,
        gain_low: gain - 9.0,
        gain_high: gain + 9.0,
        verified: better * 2 > n,
        finished: Timestamp::now(),
    }
}

/// Moves a run on by one task: whether there was more to do.
fn step(held: &mut Held, id: &str) -> bool {
    let Some(r) = held.runs.get_mut(id) else {
        return false;
    };
    if !matches!(r.state, RunState::Queued | RunState::Running) {
        return false;
    }
    r.state = RunState::Running;
    match r.tasks.iter().position(|t| t.state == TaskState::Running) {
        Some(i) => {
            r.tasks[i] = TaskProgress {
                state: match r.verdicts.get(i) {
                    Some(Verdict::Better) => TaskState::Better,
                    Some(Verdict::Worse) => TaskState::Worse,
                    _ => TaskState::Same,
                },
                runs: 3,
            };
            if let Some(next) = r.tasks.get_mut(i + 1) {
                next.state = TaskState::Running;
                next.runs = 1;
            }
        }
        None => {
            if let Some(first) = r.tasks.first_mut() {
                first.state = TaskState::Running;
                first.runs = 1;
            }
        }
    }
    if r.tasks
        .iter()
        .all(|t| !matches!(t.state, TaskState::Waiting | TaskState::Running))
    {
        r.state = RunState::Finished;
        let result = result_of(r);
        let key = r.key.clone();
        held.minted += 1;
        let id_ = format!("s{}", held.minted);
        if let Some(r) = held.runs.get_mut(id) {
            r.score = Some(id_.clone());
        }
        if let Some(a) = held.accounts.get_mut(&key) {
            a.scores.push(Score {
                link: format!("{SITE}/s/{id_}"),
                id: id_,
                run: id.into(),
                published: false,
                result,
            });
        }
        return false;
    }
    true
}

/// A run of the account's, by ID.
fn mine<'a>(held: &'a Held, key: &str, id: &str) -> Option<&'a Run> {
    held.runs.get(id).filter(|r| r.key == key)
}

async fn confirm(
    State(held): Shared,
    headers: HeaderMap,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let mut held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    let Some(r) = mine(&held, &key, &id) else {
        return no_run();
    };
    // Read here, so nothing of the key is ever said back in an error.
    let provider_key = serde_json::from_slice::<super::Confirm>(&body).ok();
    let provider = model_offer(&r.estimate.agent, &r.estimate.model)
        .map_or_else(|| "the model’s provider".to_string(), |m| m.provider_name);
    if !provider_key.is_some_and(|c| takes_key(c.provider_key.trim())) {
        return problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Key not accepted",
            &format!(
                "{provider} didn’t accept that key. Check it’s your {provider} key, then try again."
            ),
        );
    }
    if r.state == RunState::Estimated {
        if account_of(&held, &key).runs.left == 0 {
            return no_runs_left();
        }
        if let Some(a) = held.accounts.get_mut(&key) {
            a.runs_today += 1;
        }
        if let Some(r) = held.runs.get_mut(&id) {
            r.state = RunState::Queued;
        }
    }
    match status_of(&held, &id) {
        Some(s) => (StatusCode::ACCEPTED, Json(s)).into_response(),
        None => no_run(),
    }
}

async fn status(State(held): Shared, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let mut held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    if mine(&held, &key, &id).is_none() {
        return no_run();
    }
    step(&mut held, &id);
    match status_of(&held, &id) {
        Some(s) => Json(s).into_response(),
        None => no_run(),
    }
}

async fn cancel(State(held): Shared, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let mut held = lock(&held);
    let Some(key) = key_of(&held, &headers) else {
        return signed_out();
    };
    if mine(&held, &key, &id).is_none() {
        return no_run();
    }
    if let Some(r) = held.runs.get_mut(&id)
        && !matches!(r.state, RunState::Finished | RunState::Failed)
    {
        r.state = RunState::Cancelled;
        for t in &mut r.tasks {
            if matches!(t.state, TaskState::Running) {
                t.state = TaskState::Waiting;
            }
        }
    }
    match status_of(&held, &id) {
        Some(s) => Json(s).into_response(),
        None => no_run(),
    }
}

fn no_run() -> Response {
    problem(
        StatusCode::NOT_FOUND,
        "No such run",
        "kelve.ai has no such run. Start another from kelve.",
    )
}

/// What a test, or the sandbox, changes in the stand-in, by `POST
/// /stand-in/v2`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Control {
    /// Finishes this run at once.
    finish: Option<String>,
    /// Someone else has these addresses.
    taken: Vec<String>,
    /// How many runs an account may start a day.
    runs_a_day: Option<u32>,
}

async fn control(State(held): Shared, Json(c): Json<Control>) -> StatusCode {
    let hosted = Hosted { held };
    if let Some(run) = &c.finish {
        hosted.finish(run);
    }
    for h in &c.taken {
        hosted.take(h);
    }
    if let Some(n) = c.runs_a_day {
        hosted.runs_a_day(n);
    }
    StatusCode::NO_CONTENT
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;
    use crate::v2::client::{Client, Error};
    use crate::v2::tar::{File, write};

    async fn signed_in() -> (Hosted, Client) {
        let hosted = Hosted::default();
        let url = hosted.start().await.unwrap();
        (hosted, Client::new(&url, Some(KEY.into())).unwrap())
    }

    fn skills(names: &[&str]) -> Vec<u8> {
        let files: Vec<File> = names
            .iter()
            .map(|n| File {
                path: format!("{n}/SKILL.md"),
                bytes: format!("---\nname: {n}\n---\n").into_bytes(),
            })
            .collect();
        write(&files).unwrap()
    }

    #[tokio::test]
    async fn an_address_is_checked_then_taken_and_others_are_offered_instead() {
        let (hosted, client) = signed_in().await;
        let c = client.check_handle("Sam").await.unwrap();
        assert!(c.available);
        assert_eq!(c.handle, "sam");
        let c = client.check_handle("alex").await.unwrap();
        assert_eq!(c.why.as_deref(), Some("Someone has that one."));
        assert_eq!(c.instead, ["alex-agent", "alexbuilds"]);
        let c = client.check_handle("-x").await.unwrap();
        assert!(!c.available);

        let account = client.choose_handle("@Sam").await.unwrap();
        assert_eq!(account.handle.as_deref(), Some("sam"));
        assert_eq!(account.page.as_deref(), Some("https://kelve.ai/@sam"));
        assert!(!account.public);
        hosted.take("jo-2");
        let Err(Error::Refused { status, said }) = client.choose_handle("alex").await else {
            panic!("someone has it");
        };
        assert_eq!((status, said.as_str()), (409, "Someone has that one."));
        assert!(matches!(
            client.choose_handle("a b").await,
            Err(Error::Refused { status: 422, .. })
        ));
    }

    #[tokio::test]
    async fn a_run_is_estimated_confirmed_and_moves_on_until_it_scores() {
        let (hosted, client) = signed_in().await;
        client.choose_handle("sam").await.unwrap();
        let offer = client.offer().await.unwrap();
        assert_eq!(offer.kinds[0].kind, "rust");

        let e = client
            .estimate(
                "rust",
                "claude-code",
                "claude-sonnet-5-6",
                &skills(&["rust", EXPERT]),
            )
            .await
            .unwrap();
        assert_eq!(e.tasks, 12);
        assert_eq!(e.runs_left, 0);
        assert_eq!(e.skills[0].path, "rust/SKILL.md");
        let s = client.run(&e.run).await.unwrap().unwrap();
        assert_eq!(s.state, RunState::Estimated);

        // The run is paid for on the person's own key.
        assert_eq!((e.provider.as_str(), e.dollars), ("anthropic", 60.0));
        let refused = client.confirm(&e.run, "sk-ant-real").await;
        let Err(Error::Refused { status: 422, said }) = refused else {
            panic!("only a test's key: {refused:?}");
        };
        assert_eq!(
            said,
            "Anthropic didn’t accept that key. Check it’s your Anthropic key, then try again."
        );
        assert_eq!(
            client.run(&e.run).await.unwrap().unwrap().state,
            RunState::Estimated
        );
        let s = client.confirm(&e.run, "sk-test-1").await.unwrap();
        assert_eq!(s.state, RunState::Queued);
        // Confirming again starts it once.
        client.confirm(&e.run, "sk-test-1").await.unwrap();
        assert_eq!(client.account().await.unwrap().runs.left, 0);

        let s = client.run(&e.run).await.unwrap().unwrap();
        assert_eq!(s.state, RunState::Running);
        assert_eq!(s.tasks[0].state, TaskState::Running);
        let s = client.run(&e.run).await.unwrap().unwrap();
        assert_eq!(s.tasks[0].state, TaskState::Better);
        hosted.finish(&e.run);
        let s = client.run(&e.run).await.unwrap().unwrap();
        assert_eq!(s.state, RunState::Finished);
        let result = s.result.unwrap();
        assert_eq!((result.better(), result.tasks.len()), (10, 12));
        assert!(result.verified);

        let scores = client.scores().await.unwrap();
        assert_eq!(scores.len(), 1);
        assert_eq!(Some(scores[0].id.clone()), s.score);
        assert!(!scores[0].published);
        let published = client.publish(&scores[0].id, true).await.unwrap().unwrap();
        assert!(published.published);
        assert!(client.account().await.unwrap().public);
        assert!(client.publish("nope", true).await.unwrap().is_none());

        // That was the day's run.
        let no = client
            .estimate(
                "rust",
                "claude-code",
                "claude-sonnet-5-6",
                &skills(&["rust"]),
            )
            .await;
        assert!(matches!(no, Err(Error::Limited { .. })), "{no:?}");
    }

    #[tokio::test]
    async fn without_the_expert_skill_a_run_shows_no_clear_difference() {
        let (hosted, client) = signed_in().await;
        let e = client
            .estimate(
                "rust",
                "claude-code",
                "claude-sonnet-5-6",
                &skills(&["rust"]),
            )
            .await
            .unwrap();
        client.confirm(&e.run, "sk-test-1").await.unwrap();
        hosted.finish(&e.run);
        let result = client.run(&e.run).await.unwrap().unwrap().result.unwrap();
        assert!(!result.verified);
        assert_eq!(result.better(), 1);
    }

    #[tokio::test]
    async fn what_is_not_on_offer_or_not_skills_is_refused_with_a_reason() {
        let (_hosted, client) = signed_in().await;
        let e = client
            .estimate(
                "cobol",
                "claude-code",
                "claude-sonnet-5-6",
                &skills(&["rust"]),
            )
            .await;
        assert!(matches!(e, Err(Error::Refused { status: 422, .. })));
        let e = client
            .estimate("rust", "claude-code", "claude-sonnet-5-6", b"not a tar")
            .await;
        assert!(matches!(e, Err(Error::Refused { status: 422, .. })));
        let stranger = Client::new(client.base(), Some("someone".into())).unwrap();
        assert_eq!(stranger.account().await, Err(Error::SignIn));
        let nobody = Client::new(client.base(), None).unwrap();
        assert_eq!(nobody.scores().await, Err(Error::SignIn));
    }

    #[tokio::test]
    async fn a_page_is_named_published_and_taken_down_with_its_account() {
        let (hosted, client) = signed_in().await;
        let early = client
            .set_page(&PageSettings {
                name: None,
                public: true,
            })
            .await;
        assert!(matches!(early, Err(Error::Refused { status: 409, .. })));
        client.choose_handle("sam").await.unwrap();
        let a = client
            .set_page(&PageSettings {
                name: Some(" Robo ".into()),
                public: true,
            })
            .await
            .unwrap();
        assert_eq!((a.name.as_deref(), a.public), (Some("Robo"), true));
        assert_eq!(
            hosted.page(KEY),
            Some((Some("sam".into()), Some("Robo".into()), true))
        );
        assert!(client.character().await.unwrap().is_none());
        let deleted = client.delete_account().await.unwrap();
        assert!(deleted.page);
        assert_eq!(hosted.page(KEY), None);
    }
}
