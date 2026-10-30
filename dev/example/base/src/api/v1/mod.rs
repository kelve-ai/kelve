mod charges;
mod refunds;
mod webhooks;

use axum::Router;
use axum::routing::post;

use crate::App;

pub fn routes() -> Router<App> {
    Router::new()
        .route("/charges", post(charges::create))
        .route("/refunds", post(refunds::create))
        .route("/webhooks/bank", post(webhooks::receive))
}
