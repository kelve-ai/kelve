//! Reports of what went wrong, for the person to read, post as an issue
//! themselves, or forget. kelve sends them nowhere.

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use kelve_core::views::{PageProblem, Problem};
use serde_json::json;

use crate::auth::Me;
use crate::error::ApiResult;
use crate::state::Shared;

/// The reports kept, newest first.
pub async fn list(State(app): State<Shared>, _me: Me) -> ApiResult<Json<Vec<Problem>>> {
    Ok(Json(crate::problems::list(&app.config.data)))
}

/// Keeps a report of an error on kelve's page.
pub async fn from_page(
    State(app): State<Shared>,
    _me: Me,
    Json(p): Json<PageProblem>,
) -> ApiResult<Json<serde_json::Value>> {
    crate::problems::from_page(&app.config.data, &p.message, &p.stack)?;
    Ok(Json(json!({})))
}

/// Forgets a report.
pub async fn forget(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<Json<serde_json::Value>> {
    crate::problems::forget(&app.config.data, &id)?;
    Ok(Json(json!({})))
}
