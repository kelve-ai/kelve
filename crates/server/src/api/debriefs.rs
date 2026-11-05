//! Home, debriefs and their changes, for the person signed in on this
//! browser (`Me`) only: their agent reaches debriefs through its jobs.

use axum::Json;
use axum::extract::{Path as UrlPath, Query, State};
use kelve_core::views::{
    DebriefChangeView, DebriefDiffView, DebriefView, DecisionChoice, HomeView,
};
use serde::{Deserialize, Serialize};

use crate::auth::Me;
use crate::debriefs;
use crate::error::ApiResult;
use crate::state::Shared;

pub async fn home(State(app): State<Shared>, _me: Me) -> ApiResult<Json<HomeView>> {
    Ok(Json(crate::home::view(&app).await?))
}

/// "Not now", to an ask on home: asked again tomorrow.
#[derive(Debug, Deserialize)]
pub struct LaterIn {
    pub ask: String,
}

pub async fn later(
    State(app): State<Shared>,
    _me: Me,
    Json(later): Json<LaterIn>,
) -> ApiResult<Json<HomeView>> {
    if later.ask == "hook" {
        crate::home::hook_later(&app);
    }
    Ok(Json(crate::home::view(&app).await?))
}

pub async fn one(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<DebriefView>> {
    Ok(Json(debriefs::view(&app, id).await?))
}

/// The person's call on a decision, by its number: none takes it back.
#[derive(Debug, Deserialize)]
pub struct DecideIn {
    pub n: u32,
    pub choice: Option<DecisionChoice>,
    #[serde(default)]
    pub note: Option<String>,
}

pub async fn decide(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Json(d): Json<DecideIn>,
) -> ApiResult<Json<DebriefView>> {
    debriefs::decide(&app, id, d.n, d.choice, d.note.as_deref()).await?;
    Ok(Json(debriefs::view(&app, id).await?))
}

/// Whether the person wants a decision this way every time.
#[derive(Debug, Deserialize)]
pub struct RememberIn {
    pub n: u32,
    pub on: bool,
}

pub async fn remember(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Json(r): Json<RememberIn>,
) -> ApiResult<Json<DebriefView>> {
    debriefs::remember(&app, id, r.n, r.on).await?;
    Ok(Json(debriefs::view(&app, id).await?))
}

pub async fn agree_rest(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<DebriefView>> {
    debriefs::agree_with_rest(&app, id).await?;
    Ok(Json(debriefs::view(&app, id).await?))
}

/// Look into a decision: the person's question for their agent.
#[derive(Debug, Deserialize)]
pub struct LookIn {
    pub n: u32,
    pub question: String,
}

pub async fn look_into(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Json(l): Json<LookIn>,
) -> ApiResult<Json<DebriefView>> {
    debriefs::look_into(&app, id, l.n, &l.question).await?;
    Ok(Json(debriefs::view(&app, id).await?))
}

/// An idea the work rests on, by name.
#[derive(Debug, Deserialize)]
pub struct IdeaIn {
    pub idea: String,
}

pub async fn explain(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Json(i): Json<IdeaIn>,
) -> ApiResult<Json<DebriefView>> {
    debriefs::explain(&app, id, &i.idea).await?;
    Ok(Json(debriefs::view(&app, id).await?))
}

pub async fn know(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Json(i): Json<IdeaIn>,
) -> ApiResult<Json<DebriefView>> {
    debriefs::know(&app, &i.idea).await?;
    Ok(Json(debriefs::view(&app, id).await?))
}

pub async fn send(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<DebriefView>> {
    Ok(Json(debriefs::send(&app, id).await?))
}

pub async fn again(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<DebriefView>> {
    debriefs::again(&app, id).await?;
    Ok(Json(debriefs::view(&app, id).await?))
}

/// The debrief asked for, of work too small for one.
#[derive(Debug, Serialize)]
pub struct Asked {
    pub id: String,
}

pub async fn debrief_it(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(handover): UrlPath<i64>,
) -> ApiResult<Json<Asked>> {
    let id = debriefs::debrief_it(&app, handover).await?;
    Ok(Json(Asked { id: id.to_string() }))
}

pub async fn change(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<DebriefChangeView>> {
    Ok(Json(debriefs::change(&app, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct PathIn {
    pub path: String,
}

pub async fn diff(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(p): Query<PathIn>,
) -> ApiResult<Json<DebriefDiffView>> {
    Ok(Json(debriefs::diff(&app, id, &p.path).await?))
}

/// What was handed over, file by file, while there's no debrief of it.
pub async fn handover_change(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
) -> ApiResult<Json<DebriefChangeView>> {
    Ok(Json(debriefs::handover_change(&app, id).await?))
}

/// One file of what was handed over, while there's no debrief of it.
pub async fn handover_diff(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<i64>,
    Query(p): Query<PathIn>,
) -> ApiResult<Json<DebriefDiffView>> {
    Ok(Json(debriefs::handover_diff(&app, id, &p.path).await?))
}
