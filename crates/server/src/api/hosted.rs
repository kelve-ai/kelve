//! kelve.ai from the app, under `/api/hosted`: signing in and out,
//! deleting the account, the page's address, publishing the page and its
//! scores, the person's provider keys, and verified runs, from the offer
//! to the score. Only the person asks: every route takes `Me`.
//!
//! Signing in follows RFC 8252, as signing in to the shared graph did: kelve
//! listens on a port of its own for one sign-in, sends the person's
//! browser to kelve.ai with a PKCE challenge, and swaps the code that
//! comes back, with the verifier only it holds, for a key of its own.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use kelve_core::hosted::{
    AgentChoice, CredentialKind, EstimateView, ExampleCheckView, ExampleTaskView, HandleView,
    HostedView, KindChoice, ModelChoice, OfferView, RunState, RunView, ScoreView, TaskSquare,
    credential_problem, provider_name,
};
use kelve_core::views::{HostedSignIn, ServerEvent};
use kelve_graph_api::v2::client::Client;
use kelve_graph_api::v2::{self as v2, handle, tar};
use kelve_graph_api::{SignInCode, pkce_challenge};
use serde::Deserialize;
use tokio::sync::oneshot;

use crate::auth::Me;
use crate::error::{ApiError, ApiResult};
use crate::hosted::{self, Account, said};
use crate::state::Shared;

/// How long kelve waits for the browser to come back.
const WAIT: Duration = Duration::from_secs(10 * 60);

pub async fn account(State(app): State<Shared>, _me: Me) -> ApiResult<Json<HostedView>> {
    Ok(Json(hosted::view(&app).await?))
}

#[derive(Debug, Default, Deserialize)]
pub struct SignInWith {
    #[serde(default)]
    provider: Option<String>,
}

/// Starts signing in, with GitHub or Google: where to send the browser.
pub async fn sign_in(
    State(app): State<Shared>,
    _me: Me,
    with: Option<Json<SignInWith>>,
) -> ApiResult<Json<HostedSignIn>> {
    let provider = with.and_then(|Json(w)| w.provider);
    if let Some(p) = provider.as_deref()
        && !matches!(p, "github" | "google")
    {
        return Err(ApiError::bad_request("Sign in with GitHub or Google."));
    }
    let Some(service) = hosted::service(&app) else {
        return Err(ApiError::bad_request(
            "This kelve has no kelve.ai to sign in to. Take KELVE_GRAPH_URL=off out of where kelve runs, then try again.",
        ));
    };
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(ApiError::internal)?;
    let port = listener.local_addr().map_err(ApiError::internal)?.port();
    let redirect = format!("http://127.0.0.1:{port}/signed-in");
    let verifier = crate::crypto::token();
    let state = crate::crypto::token();
    let address = service
        .sign_in_address(
            &redirect,
            &state,
            &pkce_challenge(&verifier),
            provider.as_deref(),
        )
        .map_err(ApiError::internal)?;
    let (done, finished) = oneshot::channel();
    let waiting = Arc::new(Waiting {
        app: app.clone(),
        service,
        verifier,
        state,
        redirect,
        provider: provider.unwrap_or_else(|| "github".into()),
        done: std::sync::Mutex::new(Some(done)),
    });
    let back = Router::new()
        .route("/signed-in", get(signed_in))
        .with_state(waiting);
    let told = app.clone();
    let task = tokio::spawn(async move {
        let serving = axum::serve(listener, back).with_graceful_shutdown(async move {
            let _ = finished.await;
        });
        if tokio::time::timeout(WAIT, serving).await.is_err() {
            tracing::info!("a sign-in to kelve.ai ran out of time");
        }
        told.publish(ServerEvent::Settings);
    });
    // One sign-in at a time: a new one replaces any still waiting.
    if let Ok(mut w) = app.hosted.signing_in.lock()
        && let Some(old) = w.replace(task.abort_handle())
    {
        old.abort();
    }
    app.publish(ServerEvent::Settings);
    Ok(Json(HostedSignIn { address }))
}

/// Stops waiting for a sign-in the person gave up on.
pub async fn stop_signing_in(State(app): State<Shared>, _me: Me) -> ApiResult<Json<HostedView>> {
    app.hosted.stop_waiting();
    app.publish(ServerEvent::Settings);
    Ok(Json(hosted::view(&app).await?))
}

/// Signs out: kelve.ai forgets this kelve's key, and kelve forgets it too.
pub async fn sign_out(State(app): State<Shared>, _me: Me) -> ApiResult<Json<HostedView>> {
    if let Some(c) = hosted::client(&app).await?
        && let Err(e) = c.sign_out().await
    {
        // Forgotten here all the same: what's left there is a key nobody holds.
        tracing::warn!(error = %e, "kelve.ai couldn't be told this kelve signed out");
    }
    hosted::keep_account(&app, None).await?;
    tracing::info!("signed out of kelve.ai");
    app.publish(ServerEvent::Settings);
    Ok(Json(hosted::view(&app).await?))
}

/// Deletes the account on kelve.ai, with its page, character and scores,
/// then forgets its key here. If kelve.ai can't be reached, nothing is
/// forgotten, so the person can try again.
pub async fn delete_account(State(app): State<Shared>, _me: Me) -> ApiResult<Json<HostedView>> {
    let Some(c) = hosted::client(&app).await? else {
        return Err(ApiError::bad_request(
            "You’re not signed in to kelve.ai, so there’s no account to delete.",
        ));
    };
    match c.delete_account().await {
        // Already gone there: forgetting it here is all that's left.
        Ok(_) | Err(v2::client::Error::SignIn) => {}
        Err(e) => return Err(said(e)),
    }
    hosted::keep_account(&app, None).await?;
    tracing::info!("deleted the person's account on kelve.ai, as asked");
    app.publish(ServerEvent::Settings);
    Ok(Json(hosted::view(&app).await?))
}

// ---------------------------------------------------------------- the page

#[derive(Debug, Deserialize)]
pub struct HandleIn {
    handle: String,
}

/// Whether an address can be had, as it's typed: the rules anyone can
/// check first, then kelve.ai.
pub async fn check_handle(
    State(app): State<Shared>,
    _me: Me,
    Json(ask): Json<HandleIn>,
) -> ApiResult<Json<HandleView>> {
    if let Err(why) = handle::check(&ask.handle) {
        return Ok(Json(HandleView {
            handle: handle::normal(&ask.handle),
            available: false,
            why: Some(why.into()),
            instead: Vec::new(),
        }));
    }
    let c = hosted::signed_in(&app).await?;
    let c = c.check_handle(&ask.handle).await.map_err(said)?;
    Ok(Json(HandleView {
        handle: c.handle,
        available: c.available,
        why: c.why,
        instead: c.instead,
    }))
}

/// Takes an address for the person's page.
pub async fn choose_handle(
    State(app): State<Shared>,
    _me: Me,
    Json(ask): Json<HandleIn>,
) -> ApiResult<Json<HostedView>> {
    if let Err(why) = handle::check(&ask.handle) {
        return Err(ApiError::bad_request(why));
    }
    let c = hosted::signed_in(&app).await?;
    let a = c.choose_handle(&ask.handle).await.map_err(said)?;
    if let Some(mut held) = hosted::account(&app).await? {
        held.handle = a.handle;
        hosted::keep_account(&app, Some(&held)).await?;
    }
    app.publish(ServerEvent::Settings);
    Ok(Json(hosted::view(&app).await?))
}

#[derive(Debug, Deserialize)]
pub struct PageIn {
    public: bool,
}

/// Publishes the page, with the agent's character and name as they are
/// now, or takes it down.
pub async fn set_page(
    State(app): State<Shared>,
    _me: Me,
    Json(page): Json<PageIn>,
) -> ApiResult<Json<HostedView>> {
    let c = hosted::signed_in(&app).await?;
    hosted::set_page(&app, &c, page.public).await?;
    app.publish(ServerEvent::Settings);
    Ok(Json(hosted::view(&app).await?))
}

/// The account's scores, newest first.
pub async fn scores(State(app): State<Shared>, _me: Me) -> ApiResult<Json<Vec<ScoreView>>> {
    let Some(c) = hosted::client(&app).await? else {
        return Ok(Json(Vec::new()));
    };
    let scores = c.scores().await.map_err(said)?;
    Ok(Json(scores.iter().map(hosted::score_view).collect()))
}

#[derive(Debug, Deserialize)]
pub struct ShownIn {
    shown: bool,
}

/// Puts a score on the page, or takes it off. Putting one on publishes
/// the page, with the agent's character and name.
pub async fn show_score(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
    Json(shown): Json<ShownIn>,
) -> ApiResult<Json<ScoreView>> {
    let c = hosted::signed_in(&app).await?;
    if shown.shown {
        let a = c.account().await.map_err(said)?;
        if a.handle.is_some() && !a.public {
            hosted::set_page(&app, &c, true).await?;
        } else if a.public {
            // Sent again, in case the agent changed since.
            let (look, _) = hosted::look_and_name(&app).await;
            c.set_character(&hosted::character(&app, &look))
                .await
                .map_err(said)?;
        }
    }
    let s = c
        .publish(&id, shown.shown)
        .await
        .map_err(said)?
        .ok_or_else(|| ApiError::not_found("kelve.ai has no such score of yours."))?;
    app.publish(ServerEvent::Settings);
    Ok(Json(hosted::score_view(&s)))
}

// ---------------------------------------------------------------- provider keys

#[derive(Deserialize)]
pub struct KeyIn {
    provider: String,
    #[serde(default)]
    kind: CredentialKind,
    key: String,
}

// Never the key.
impl std::fmt::Debug for KeyIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyIn")
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

/// What a credential is called, as settings and errors say it.
fn called(kind: CredentialKind, provider: &str) -> String {
    match kind {
        CredentialKind::ApiKey => format!("{} key", provider_name(provider)),
        CredentialKind::Subscription => "Claude subscription token".into(),
    }
}

/// Keeps the person's key for a provider, sealed, in place of any kept.
pub async fn keep_key(
    State(app): State<Shared>,
    _me: Me,
    Json(k): Json<KeyIn>,
) -> ApiResult<Json<HostedView>> {
    let key = k.key.trim();
    if let Some(problem) = credential_problem(k.kind, key) {
        return Err(ApiError::bad_request(problem));
    }
    hosted::keep_provider_key(&app, &k.provider, k.kind, Some(key)).await?;
    tracing::info!(provider = %k.provider, kind = ?k.kind, "kept the person's {} for a provider", called(k.kind, &k.provider));
    app.publish(ServerEvent::Settings);
    Ok(Json(hosted::view(&app).await?))
}

/// Forgets the person's key for a provider: `api-key` or `subscription`.
pub async fn forget_key(
    State(app): State<Shared>,
    _me: Me,
    Path((kind, provider)): Path<(String, String)>,
) -> ApiResult<Json<HostedView>> {
    let kind = match kind.as_str() {
        "subscription" => CredentialKind::Subscription,
        _ => CredentialKind::ApiKey,
    };
    hosted::keep_provider_key(&app, &provider, kind, None).await?;
    tracing::info!(provider = %provider, kind = ?kind, "forgot a credential the person kept");
    app.publish(ServerEvent::Settings);
    Ok(Json(hosted::view(&app).await?))
}

// ---------------------------------------------------------------- a verified run

/// A retired task as the app shows it: its starting files without the
/// hidden tests, which are kept under `tests/`.
fn example_of(t: v2::ExampleTask) -> ExampleTaskView {
    ExampleTaskView {
        name: t.name,
        request: t.instruction,
        files: t
            .files
            .into_iter()
            .map(|f| f.path)
            .filter(|p| !p.starts_with("tests/"))
            .collect(),
        checks: t
            .checks
            .into_iter()
            .map(|c| ExampleCheckView {
                kind: c.kind,
                about: c.about,
            })
            .collect(),
    }
}

/// What a run can be, and every file of the person's skills as it would go.
pub async fn offer(State(app): State<Shared>, _me: Me) -> ApiResult<Json<OfferView>> {
    let c = hosted::signed_in(&app).await?;
    let (offer, account) = tokio::join!(c.offer(), c.account());
    let (offer, account) = (offer.map_err(said)?, account.map_err(said)?);
    let accepts = hosted::accepts(&offer.agents);
    let files = hosted::skills(&app.config.skills_dir).map_err(ApiError::internal)?;
    Ok(Json(OfferView {
        kinds: offer
            .kinds
            .into_iter()
            .map(|k| KindChoice {
                kind: k.kind,
                name: k.name,
                about: k.about,
                tasks: k.tasks,
                examples: k.examples.into_iter().map(example_of).collect(),
            })
            .collect(),
        agents: offer
            .agents
            .into_iter()
            .map(|a| AgentChoice {
                agent: a.agent,
                name: a.name,
                models: a
                    .models
                    .into_iter()
                    .map(|m| ModelChoice {
                        model: m.model,
                        name: m.name,
                        provider: m.provider,
                        provider_name: m.provider_name,
                    })
                    .collect(),
            })
            .collect(),
        skills: hosted::listed(&files),
        runs: hosted::runs_left(&account.runs),
        accepts,
    }))
}

#[derive(Debug, Deserialize)]
pub struct RunIn {
    kind: String,
    agent: String,
    model: String,
    /// What pays: what they chose, or else what kelve keeps already, or
    /// else the first the agent takes.
    #[serde(default)]
    credential: Option<CredentialKind>,
}

/// Sends the person's skills to kelve.ai for a run, and has what it will
/// take back. Nothing runs until they confirm it.
pub async fn estimate(
    State(app): State<Shared>,
    _me: Me,
    Json(ask): Json<RunIn>,
) -> ApiResult<Json<EstimateView>> {
    let c = hosted::signed_in(&app).await?;
    let offer = c.offer().await.map_err(said)?;
    let kind = offer.kinds.iter().find(|k| k.kind == ask.kind);
    let agent = offer.agents.iter().find(|a| a.agent == ask.agent);
    let model = agent.and_then(|a| a.models.iter().find(|m| m.model == ask.model));
    let (Some(kind), Some(agent), Some(model)) = (kind, agent, model) else {
        return Err(ApiError::bad_request(
            "kelve.ai doesn’t run that kind of work, agent or model. Choose from what it offers.",
        ));
    };
    let files = hosted::skills(&app.config.skills_dir).map_err(ApiError::internal)?;
    if files.is_empty() {
        return Err(ApiError::bad_request(
            "Your agent has no skills yet, so there’s nothing to score. Work with it for a while, then try again.",
        ));
    }
    let skills = tar::write(&files).map_err(ApiError::bad_request)?;
    if skills.len() > v2::MOST_SKILLS {
        return Err(ApiError::bad_request(
            "Your skills come to more than a megabyte. Make them shorter, then try again.",
        ));
    }
    let accepts = hosted::accepts([agent]);
    let mut kept = Vec::new();
    for kind in &accepts {
        if hosted::provider_key(&app, &model.provider, *kind)
            .await?
            .is_some()
        {
            kept.push(*kind);
        }
    }
    // Estimated for what will pay, so confirming it never finds it was
    // estimated for something else.
    let paying = ask
        .credential
        .filter(|k| accepts.contains(k))
        .or_else(|| accepts.iter().copied().find(|k| kept.contains(k)))
        .unwrap_or(accepts[0]);
    let e = c
        .estimate_paying(
            &kind.kind,
            &agent.agent,
            &model.model,
            &skills,
            paying.into(),
        )
        .await
        .map_err(said)?;
    tracing::info!(kind = %e.kind, model = %e.model, files = files.len(), "sent the person's skills to kelve.ai for an estimate");
    {
        let mut conn = app.db.acquire().await?;
        kelve_store::hosted::add(
            &mut conn,
            &kelve_store::hosted::Run {
                id: e.run.clone(),
                kind: kind.kind.clone(),
                kind_name: kind.name.clone(),
                agent: agent.agent.clone(),
                agent_name: agent.name.clone(),
                model: model.model.clone(),
                model_name: model.name.clone(),
                provider: e.provider.clone(),
                provider_name: e.provider_name.clone(),
                state: "estimated".into(),
                score: None,
                at: 0,
            },
        )
        .await?;
    }
    Ok(Json(EstimateView {
        run: e.run,
        kind_name: kind.name.clone(),
        agent_name: agent.name.clone(),
        model_name: model.name.clone(),
        tasks: e.tasks,
        runs_per_task: e.runs_per_task,
        minutes: e.minutes,
        provider: e.provider,
        provider_name: e.provider_name,
        dollars: e.dollars,
        agent_runs: e.tasks.saturating_mul(e.runs_per_task),
        runs_left: e.runs_left,
        accepts,
        kept,
        paying,
        skills: e
            .skills
            .into_iter()
            .map(|f| kelve_core::hosted::SkillFileView {
                path: f.path,
                bytes: u32::try_from(f.bytes).unwrap_or(u32::MAX),
            })
            .collect(),
    }))
}

#[derive(Default, Deserialize)]
pub struct ConfirmIn {
    #[serde(default)]
    key: Option<String>,
    /// What pays: an API key unless they chose their subscription.
    #[serde(default)]
    kind: CredentialKind,
}

// Never the key.
impl std::fmt::Debug for ConfirmIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfirmIn").finish_non_exhaustive()
    }
}

/// Starts a run the person confirmed, paid for on their own key for the
/// model's provider: the one they paste, which kelve then keeps sealed,
/// or the one it kept. The key goes to kelve.ai for this run and nowhere
/// else.
pub async fn confirm(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
    confirm: Option<Json<ConfirmIn>>,
) -> ApiResult<Json<RunView>> {
    let c = hosted::signed_in(&app).await?;
    let run = kept_run(&app, &id).await?;
    let Json(confirm) = confirm.unwrap_or_default();
    let kind = confirm.kind;
    let offer = c.offer().await.map_err(said)?;
    let agents = offer.agents.iter().filter(|a| a.agent == run.agent);
    if !hosted::accepts(agents).contains(&kind) {
        return Err(ApiError::bad_request(
            "kelve.ai takes an API key for this run. Paste one to start it.",
        )
        .with(serde_json::json!({ "key": true })));
    }
    let pasted = confirm
        .key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    if let Some(problem) = pasted.as_deref().and_then(|k| credential_problem(kind, k)) {
        return Err(ApiError::bad_request(problem).with(serde_json::json!({ "key": true })));
    }
    let key = match &pasted {
        Some(k) => k.clone(),
        None => hosted::provider_key(&app, &run.provider, kind)
            .await?
            .ok_or_else(|| {
                ApiError::bad_request(format!(
                    "Paste your {} to start the run.",
                    called(kind, &run.provider)
                ))
                .with(serde_json::json!({ "key": true }))
            })?,
    };
    let status = match c.confirm_paying(&id, &key, kind.into()).await {
        Ok(s) => s,
        Err(v2::client::Error::Refused { status: 422, said }) => {
            // The key, most likely: ask for it again.
            return Err(ApiError::bad_request(said).with(serde_json::json!({ "key": true })));
        }
        Err(e) => return Err(said(e)),
    };
    if let Some(k) = &pasted {
        hosted::keep_provider_key(&app, &run.provider, kind, Some(k)).await?;
    }
    tracing::info!(run = %id, "started a verified run the person confirmed");
    app.publish(ServerEvent::Settings);
    Ok(Json(run_view(&app, &c, &run, status).await?))
}

/// How a run stands, task by task, and its score once it's finished.
pub async fn run(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
) -> ApiResult<Json<RunView>> {
    let c = hosted::signed_in(&app).await?;
    let run = kept_run(&app, &id).await?;
    let status = c
        .run(&id)
        .await
        .map_err(said)?
        .ok_or_else(|| ApiError::not_found("kelve.ai has no such run. Start another."))?;
    Ok(Json(run_view(&app, &c, &run, status).await?))
}

/// Stops a run, or drops an estimate; kelve.ai deletes its skills.
pub async fn cancel(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
) -> ApiResult<Json<RunView>> {
    let c = hosted::signed_in(&app).await?;
    let run = kept_run(&app, &id).await?;
    let status = c.cancel(&id).await.map_err(said)?;
    tracing::info!(run = %id, "cancelled a verified run, as asked");
    app.publish(ServerEvent::Settings);
    Ok(Json(run_view(&app, &c, &run, status).await?))
}

/// A run this kelve asked for.
async fn kept_run(app: &Shared, id: &str) -> ApiResult<kelve_store::hosted::Run> {
    let mut conn = app.db.acquire().await?;
    kelve_store::hosted::get(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("kelve has no such run. Start another from Your agent."))
}

/// A run as its page shows it, kept as it now stands.
async fn run_view(
    app: &Shared,
    c: &Client,
    run: &kelve_store::hosted::Run,
    s: v2::RunStatus,
) -> ApiResult<RunView> {
    let state = match s.state {
        v2::RunState::Estimated => RunState::Estimated,
        v2::RunState::Queued => RunState::Queued,
        v2::RunState::Running => RunState::Running,
        v2::RunState::Finished => RunState::Finished,
        v2::RunState::Cancelled => RunState::Cancelled,
        v2::RunState::Failed => RunState::Failed,
    };
    let name = serde_json::to_value(state)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default();
    {
        let mut conn = app.db.acquire().await?;
        kelve_store::hosted::stands(&mut conn, &run.id, &name, s.score.as_deref()).await?;
    }
    // The score as the account has it, so whether it's on the page is right.
    let score = match &s.score {
        Some(id) => c
            .scores()
            .await
            .map_err(said)?
            .iter()
            .find(|x| &x.id == id)
            .map(hosted::score_view),
        None => None,
    };
    Ok(RunView {
        run: run.id.clone(),
        state,
        kind_name: run.kind_name.clone(),
        agent_name: run.agent_name.clone(),
        model_name: run.model_name.clone(),
        squares: s
            .tasks
            .iter()
            .map(|t| match t.state {
                v2::TaskState::Waiting => TaskSquare::Waiting,
                v2::TaskState::Running => TaskSquare::Running,
                v2::TaskState::Better => TaskSquare::Better,
                v2::TaskState::Same => TaskSquare::Same,
                v2::TaskState::Worse => TaskSquare::Worse,
            })
            .collect(),
        minutes_left: s.minutes_left,
        paused_until: s.paused.map(|p| p.until),
        why: s.why,
        score,
    })
}

// ---------------------------------------------------------------- signing in

/// What one sign-in waits with.
struct Waiting {
    app: Shared,
    service: Client,
    verifier: String,
    state: String,
    redirect: String,
    provider: String,
    done: std::sync::Mutex<Option<oneshot::Sender<()>>>,
}

impl Waiting {
    fn finish(&self) {
        if let Some(done) = self.done.lock().ok().and_then(|mut d| d.take()) {
            let _ = done.send(());
        }
    }
}

#[derive(Debug, Deserialize)]
struct Back {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// The browser, back from kelve.ai. Only the state kelve made, which only
/// this sign-in's address carried, is listened to.
async fn signed_in(State(w): State<Arc<Waiting>>, Query(back): Query<Back>) -> Response {
    if back.state.as_deref() != Some(w.state.as_str()) {
        return page(
            &w,
            StatusCode::BAD_REQUEST,
            "This isn’t the sign-in kelve is waiting for. Sign in again from kelve’s settings.",
        )
        .await;
    }
    let who = if w.provider == "google" {
        "Google"
    } else {
        "GitHub"
    };
    let said = match (back.code, back.error) {
        (Some(code), _) => {
            let asked = SignInCode {
                code,
                code_verifier: w.verifier.clone(),
                redirect_uri: w.redirect.clone(),
            };
            match w.service.exchange(&asked).await {
                Ok(signed) => {
                    let account = Account {
                        key: signed.key,
                        handle: signed.handle,
                    };
                    match hosted::keep_account(&w.app, Some(&account)).await {
                        Ok(()) => {
                            tracing::info!("signed in to kelve.ai");
                            Ok("Close this tab and go back to kelve.".to_string())
                        }
                        Err(e) => {
                            tracing::error!(error = %e.message, "couldn't keep kelve.ai's key");
                            Err("kelve couldn’t keep the key kelve.ai gave it. Sign in again from kelve’s settings.".to_string())
                        }
                    }
                }
                Err(e) => Err(format!("{e} Sign in again from kelve’s settings.")),
            }
        }
        (None, Some(_)) => Err(format!(
            "{who} didn’t finish signing you in. Try again from kelve’s settings."
        )),
        (None, None) => {
            Err("kelve.ai sent no code back. Sign in again from kelve’s settings.".to_string())
        }
    };
    w.finish();
    w.app.publish(ServerEvent::Settings);
    match said {
        Ok(said) => page(&w, StatusCode::OK, &said).await,
        Err(said) => page(&w, StatusCode::BAD_REQUEST, &said).await,
    }
}

/// The browser tab the sign-in ended in: the person's agent, and a line.
async fn page(w: &Waiting, status: StatusCode, said: &str) -> Response {
    let (look, _) = hosted::look_and_name(&w.app).await;
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
            ),
        ],
        hosted::signed_in_page(&w.app, &look, status.is_success(), said),
    )
        .into_response()
}
