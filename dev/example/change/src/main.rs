mod api;
mod bank;
mod config;
mod error;
mod idempotency;
mod ledger;
mod reconcile;
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
    // Charges left hanging are found and finished in the background, for
    // as long as the service runs.
    tokio::spawn(reconcile::run(app.clone(), config.reconcile_every));
    // Answers older than a day are forgotten, once an hour.
    let db = app.db.clone();
    tokio::spawn(async move {
        let mut hourly = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            hourly.tick().await;
            if let Err(e) = idempotency::forget_old(&db).await {
                eprintln!("forgetting old answers: {e}");
            }
        }
    });
    let routes = Router::new().nest("/v1", api::v1::routes()).with_state(app);
    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    axum::serve(listener, routes).await?;
    Ok(())
}
