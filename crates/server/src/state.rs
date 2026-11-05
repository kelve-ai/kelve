//! What the server holds while it runs: its config, its person's settings,
//! the stream of events the web app listens to, and a way to wake the
//! pipeline when there's work.

use std::sync::OnceLock;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use kelve_core::views::ServerEvent;
use kelve_store::Pool;
use tokio::sync::{Notify, broadcast, watch};

use crate::config::Config;
use crate::error::{ApiError, ApiResult};
use crate::settings::Settings;

pub type Shared = Arc<App>;

pub struct App {
    pub config: Config,
    pub db: Pool,
    /// For calls out.
    pub http: reqwest::Client,
    /// kelve's hosted service, version 1 of its API, unless the install
    /// has none: only the weekly counts go to it, when the person says yes
    /// (`counts`).
    pub graph: Option<kelve_graph_api::client::Client>,
    /// kelve.ai from the app: a sign-in to it under way.
    pub hosted: crate::hosted::Hosted,
    /// The secret kelve shares with its command line.
    pub secret: String,
    settings: RwLock<Arc<Settings>>,
    pub events: broadcast::Sender<ServerEvent>,
    /// Wakes the pipeline: a job was added.
    pub wake: Notify,
    stopping: watch::Sender<bool>,
    /// Where a new sign-in link goes: to the person, through whatever runs
    /// kelve, never back to whoever asked for it.
    hand_off: OnceLock<HandOff>,
    /// How many of kelve's jobs are running: stopping lets them finish.
    working: std::sync::atomic::AtomicUsize,
}

/// A job running, until it's dropped.
#[derive(Debug)]
pub struct Working<'a>(&'a App);

impl Drop for Working<'_> {
    fn drop(&mut self) {
        self.0
            .working
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Gives a one-time sign-in link to the person: opens their browser with
/// it, or shows it where they started kelve. Whether a browser opened.
pub type HandOff = Arc<dyn Fn(&str) -> bool + Send + Sync>;

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl App {
    pub async fn new(config: Config, db: Pool) -> Result<Shared, crate::ServeError> {
        let secret = crate::config::ensure_secret(&config.secret_file)
            .map_err(|e| crate::ServeError::Start(e.to_string()))?;
        let settings = load_settings(&db).await?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!(
                "kelve/",
                env!("CARGO_PKG_VERSION"),
                " (+https://kelve.ai)"
            ))
            .build()
            .map_err(|e| crate::ServeError::Start(e.to_string()))?;
        let graph = match config.endpoints.graph.as_str() {
            "" => None,
            url => Some(
                kelve_graph_api::client::Client::new(url, None)
                    .map_err(|e| crate::ServeError::Start(e.to_string()))?,
            ),
        };
        let (events, _) = broadcast::channel(256);
        Ok(Arc::new(App {
            config,
            db,
            http,
            graph,
            hosted: crate::hosted::Hosted::default(),
            secret,
            settings: RwLock::new(Arc::new(settings)),
            events,
            wake: Notify::new(),
            stopping: watch::Sender::new(false),
            hand_off: OnceLock::new(),
            working: std::sync::atomic::AtomicUsize::new(0),
        }))
    }

    /// Sets where sign-in links go, once.
    pub fn set_hand_off(&self, hand_off: HandOff) {
        let _ = self.hand_off.set(hand_off);
    }

    /// Gives a sign-in link to the person. Whether a browser opened; with
    /// nowhere to give it, it goes nowhere.
    pub fn hand_off(&self, link: &str) -> bool {
        self.hand_off.get().is_some_and(|give| give(link))
    }

    pub fn settings(&self) -> Arc<Settings> {
        self.settings.read().map(|s| s.clone()).unwrap_or_default()
    }

    /// Saves the person's settings.
    pub async fn save_settings(&self, settings: Settings) -> ApiResult<()> {
        let value = serde_json::to_value(&settings).map_err(ApiError::internal)?;
        let mut conn = self.db.acquire().await?;
        kelve_store::settings::put(&mut conn, &value).await?;
        if let Ok(mut s) = self.settings.write() {
            *s = Arc::new(settings);
        }
        Ok(())
    }

    /// Reads the settings again, after something outside the settings page
    /// changed them, such as deleting everything.
    pub async fn reload(&self) -> ApiResult<()> {
        let settings = load_settings(&self.db).await?;
        if let Ok(mut s) = self.settings.write() {
            *s = Arc::new(settings);
        }
        Ok(())
    }

    /// Tells the web app something changed.
    pub fn publish(&self, event: ServerEvent) {
        // Nobody listening is fine.
        let _ = self.events.send(event);
    }

    /// Starts stopping: live streams end, so tabs left open don't hold
    /// kelve up.
    pub fn stop(&self) {
        self.stopping.send_replace(true);
    }

    pub fn is_stopping(&self) -> bool {
        *self.stopping.borrow()
    }

    /// Counts a job as running until what this gives is dropped.
    pub fn working(&self) -> Working<'_> {
        self.working
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Working(self)
    }

    /// Waits until no job is running, for up to `most`. Whether none is.
    pub async fn jobs_done(&self, most: Duration) -> bool {
        let since = std::time::Instant::now();
        while self.working.load(std::sync::atomic::Ordering::SeqCst) > 0 {
            if since.elapsed() >= most {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        true
    }

    /// Ready once kelve has started stopping.
    pub async fn stopped(&self) {
        let _ = self
            .stopping
            .subscribe()
            .wait_for(|stopping| *stopping)
            .await;
    }
}

async fn load_settings(db: &Pool) -> Result<Settings, kelve_store::Error> {
    let mut conn = db.acquire().await?;
    let value = kelve_store::settings::get(&mut conn).await?;
    // Nothing kept yet: a new install, with every default.
    if value.as_object().is_some_and(serde_json::Map::is_empty) {
        return Ok(Settings::default());
    }
    Ok(serde_json::from_value(value).unwrap_or_else(|e| {
        tracing::error!(error = %e, "the saved settings don't read, so kelve starts without them");
        Settings::default()
    }))
}
