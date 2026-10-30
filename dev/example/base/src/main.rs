mod api;
mod bank;
mod config;
mod error;
mod ledger;
mod reserve;
mod retry;

use axum::Router;
use sqlx::PgPool;

#[derive(Clone)]
pub struct App {
    pub db: PgPool,
    pub bank: bank::Client,
}

#[tokio::main]
async fn main() -> Result<(), error::Error> {
    let config = config::Config::from_env()?;
    let db = PgPool::connect(&config.database_url).await?;
    let app = App { db, bank: bank::Client::new(&config) };
    let routes = Router::new().nest("/v1", api::v1::routes()).with_state(app);
    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    axum::serve(listener, routes).await?;
    Ok(())
}
