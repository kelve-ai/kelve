//! Your agent, for the person signed in on this browser: its character and
//! name, the drawings they asked it for, the Your agent page, and the
//! settings for it and for skill checks (`crate::your_agent`).

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use kelve_core::views::{
    AgentSettings, AgentSettingsIn, CharacterIn, CharacterView, DrawIn, DrawingView, YourAgentView,
};

use crate::auth::Me;
use crate::error::ApiResult;
use crate::state::Shared;
use crate::your_agent;

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/character", get(character).post(set_character))
        .route("/character/draw", post(draw))
        .route("/character/drawings", get(drawings))
        .route("/your-agent", get(page))
        .route("/settings/agent", get(settings).post(set_settings))
}

async fn character(State(app): State<Shared>, _me: Me) -> ApiResult<Json<CharacterView>> {
    Ok(Json(your_agent::view(&app)))
}

async fn set_character(
    State(app): State<Shared>,
    _me: Me,
    Json(change): Json<CharacterIn>,
) -> ApiResult<Json<CharacterView>> {
    Ok(Json(your_agent::set(&app, change).await?))
}

async fn draw(
    State(app): State<Shared>,
    _me: Me,
    Json(asked): Json<DrawIn>,
) -> ApiResult<Json<DrawingView>> {
    Ok(Json(your_agent::draw(&app, asked).await?))
}

async fn drawings(State(app): State<Shared>, _me: Me) -> ApiResult<Json<Vec<DrawingView>>> {
    Ok(Json(your_agent::drawings(&app).await?))
}

async fn page(State(app): State<Shared>, _me: Me) -> ApiResult<Json<YourAgentView>> {
    Ok(Json(your_agent::page(&app).await?))
}

async fn settings(State(app): State<Shared>, _me: Me) -> ApiResult<Json<AgentSettings>> {
    Ok(Json(your_agent::settings(&app).await?))
}

async fn set_settings(
    State(app): State<Shared>,
    _me: Me,
    Json(change): Json<AgentSettingsIn>,
) -> ApiResult<Json<AgentSettings>> {
    Ok(Json(your_agent::set_settings(&app, change).await?))
}
