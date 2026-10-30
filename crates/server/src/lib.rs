//! kelve's local service: it shows the person what their work is built on, from
//! what their own agent read and wrote for it (`agent`), keeps their notes,
//! adds what the agent learned to the shared knowledge graph, and serves the
//! reader, on the person's own computer (docs/architecture.md).

pub mod agent;
pub mod api;
pub mod auth;
pub mod config;
pub mod counts;
pub mod crypto;
pub mod error;
pub mod follows;
pub mod handoff;
pub mod hooks;
pub mod lessons;
pub mod list;
pub mod pick;
pub mod pipeline;
pub mod plainly;
pub mod problems;
pub mod read;
pub mod reader;
pub mod ready;
pub mod settings;
pub mod spec;
pub mod state;
pub mod updates;
pub mod way;
pub mod web;

use std::future::IntoFuture;
use std::time::Duration;

use axum::Router;
use axum::http::{HeaderValue, header};
use axum::routing::get;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

pub use config::{Config, ConfigError};
pub use state::{App, Shared};

/// The most a request may carry.
const BODY_LIMIT: usize = 4 * 1024 * 1024;

/// What kelve's pages may load and run: only kelve's own, nothing inline
/// but styles, and never anything a model wrote. Only kelve's own pages may
/// frame it: the first run shows the example in one.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data:; connect-src 'self'; font-src 'self'; frame-ancestors 'self'; \
base-uri 'none'; form-action 'self'; object-src 'none'";

/// Every route kelve serves.
pub fn router(app: Shared) -> Router {
    Router::new()
        .nest("/api", api::router())
        .route("/signin", get(auth::sign_in))
        .route("/healthz", get(|| async { "ok" }))
        .fallback(web::serve)
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(axum::middleware::from_fn_with_state(
            app.clone(),
            auth::guard,
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("same-origin"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(app)
}

/// Opens and migrates the store, and starts the work done in the background.
async fn start(config: Config) -> Result<Shared, ServeError> {
    let db = kelve_store::open(&config.store).await?;
    kelve_store::migrate_file(&db, &config.store).await?;
    let app = App::new(config, db).await?;
    problems::watch(&app.config.data);
    pipeline::start(&app);
    counts::start(&app);
    follows::start(&app);
    lessons::start(&app);
    updates::start(&app);
    hooks::up_to_date(&app.config.agents_home, &app.config.claude_dir);
    Ok(app)
}

/// Runs kelve: opens and migrates the store, starts the pipeline, and serves until
/// told to stop. `hand_off` gives the person each one-time sign-in link:
/// the first as kelve starts listening, and each one `kelve open` asks for
/// after. It opens their browser, or shows the link where they started
/// kelve, and says whether a browser opened.
pub async fn serve(
    config: Config,
    hand_off: impl Fn(&str) -> bool + Send + Sync + 'static,
) -> Result<(), ServeError> {
    let bind = config.bind;
    let app = start(config).await?;
    app.set_hand_off(std::sync::Arc::new(hand_off));
    let listener = tokio::net::TcpListener::bind(bind).await?;
    let link = auth::new_link(&app)
        .await
        .map_err(|e| ServeError::Start(e.message))?;
    app.hand_off(&link);
    tracing::info!(url = %app.config.url, %bind, "kelve is running");
    if !app.config.may_write_graph() {
        tracing::info!(
            "a development build reads the real shared graph but adds nothing to it; set KELVE_WRITE_REAL_GRAPH=1 to add to it"
        );
    }
    let stopping = app.clone();
    let server = axum::serve(listener, router(app.clone()))
        .with_graceful_shutdown(async move {
            // A signal, or `kelve stop`.
            tokio::select! {
                () = shutdown() => {},
                () = stopping.stopped() => {},
            }
            stopping.stop();
        })
        .into_future();
    // Requests in flight get a few seconds. One that takes longer doesn't
    // keep kelve running, and pressing Ctrl+C again stops it at once.
    let grace = async {
        app.stopped().await;
        tokio::select! {
            () = tokio::time::sleep(GRACE) => {},
            _ = tokio::signal::ctrl_c() => {},
        }
    };
    tokio::select! {
        served = server => served?,
        () = grace => tracing::warn!("stopped with requests still in flight"),
    }
    // Jobs running finish too, so what they did is kept, as when `kelve
    // update` restarts kelve. One cut short runs again from the start.
    tokio::select! {
        done = app.jobs_done(JOBS_GRACE) => if !done {
            tracing::warn!("stopped with jobs still running; they run again when kelve starts");
        },
        _ = tokio::signal::ctrl_c() => {},
    }
    Ok(())
}

/// How long requests in flight get to finish once kelve is told to stop.
const GRACE: Duration = Duration::from_secs(5);

/// How long jobs running get to finish after that.
const JOBS_GRACE: Duration = Duration::from_secs(20);

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    let pressed = tokio::select! {
        () = ctrl_c => true,
        () = terminate => false,
    };
    let wait = GRACE.as_secs();
    if pressed {
        tracing::info!(
            "stopping; finishing requests in flight, for up to {wait} seconds (Ctrl+C again stops now)"
        );
    } else {
        tracing::info!("stopping; finishing requests in flight, for up to {wait} seconds");
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
}

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error(transparent)]
    Store(#[from] kelve_store::Error),
    #[error("couldn't listen: {0}")]
    Io(#[from] std::io::Error),
    #[error("couldn't start: {0}")]
    Start(String),
}
