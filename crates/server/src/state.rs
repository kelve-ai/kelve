//! What the server holds while it runs: its config, its person's settings
//! and the key the shared graph gave them, the stream of events the web app
//! listens to, and a way to wake the pipeline when there's work.

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

/// Where the graph's key lives in the database, sealed.
const GRAPH_KEY: &str = "keys.graph";

pub struct App {
    pub config: Config,
    pub db: Pool,
    /// For calls out.
    pub http: reqwest::Client,
    /// The shared knowledge graph, unless the install has none. It reads
    /// without an account; `graph_writer` writes as the person.
    pub graph: Option<kelve_graph_api::client::Client>,
    /// A sign-in to the shared graph under way, waiting for the browser
    /// to come back.
    pub signing_in: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
    /// The instructions for the agent's jobs, as the graph lately served
    /// them.
    pub instructions: crate::agent::Instructions,
    /// The secret kelve shares with its command line.
    pub secret: String,
    settings: RwLock<Arc<Settings>>,
    keys: RwLock<Held>,
    /// Until when nothing goes to the shared graph: the account added as
    /// much as the graph takes a day. Kept in the store too.
    sends_held: RwLock<Option<kelve_store::sends::Held>>,
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

/// The key signing in to the shared graph gave, opened.
#[derive(Clone, Default)]
struct Held {
    graph: Option<GraphKey>,
}

/// The key the shared graph gave this kelve when the person signed in,
/// and the name what it adds is shown with.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct GraphKey {
    key: String,
    handle: String,
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
        let held = load_held(&db, &config).await?;
        let sends_held = load_sends_held(&db).await?;
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
            signing_in: std::sync::Mutex::new(None),
            instructions: crate::agent::Instructions::default(),
            secret,
            settings: RwLock::new(Arc::new(settings)),
            keys: RwLock::new(held),
            sends_held: RwLock::new(sends_held),
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
        let held = load_held(&self.db, &self.config).await?;
        let sends_held = load_sends_held(&self.db).await?;
        if let Ok(mut s) = self.settings.write() {
            *s = Arc::new(settings);
        }
        if let Ok(mut k) = self.keys.write() {
            *k = held;
        }
        if let Ok(mut h) = self.sends_held.write() {
            *h = sends_held;
        }
        Ok(())
    }

    /// Until when nothing goes to the shared graph, while the account has
    /// added as much as the graph takes a day. None once it resets.
    pub fn sends_held(&self) -> Option<kelve_store::sends::Held> {
        self.sends_held
            .read()
            .ok()?
            .clone()
            .filter(|h| h.until > kelve_store::now())
    }

    /// Holds everything sent to the shared graph until its day's limits
    /// reset, or lets it go, keeping that in the store so a restart
    /// waits too, and tells the web app.
    pub async fn hold_sends(&self, held: Option<kelve_store::sends::Held>) -> ApiResult<()> {
        let mut conn = self.db.acquire().await?;
        kelve_store::sends::hold(&mut conn, held.as_ref()).await?;
        if let Ok(mut h) = self.sends_held.write() {
            *h = held;
        }
        self.publish(ServerEvent::Settings);
        Ok(())
    }

    /// The shared graph, writing as the person: with the key from the
    /// environment, or the one signing in gave. None until there's one, and
    /// for a development build pointed at the real graph unless it's told
    /// it may write (`Config::may_write_graph`).
    pub fn graph_writer(&self) -> Option<kelve_graph_api::client::Client> {
        if !self.config.may_write_graph() {
            return None;
        }
        let (_, key, _) = self.graph_key()?;
        Some(self.graph.as_ref()?.with_token(Some(key)))
    }

    /// The shared graph, reading as the person: none until they sign in.
    /// Reading it is part of adding to it, so signed out, kelve works as it
    /// does without one, and the person's agent writes every entry for
    /// them alone.
    pub fn graph_reader(&self) -> Option<kelve_graph_api::client::Client> {
        let (_, key, _) = self.graph_key()?;
        Some(self.graph.as_ref()?.with_token(Some(key)))
    }

    /// The key kelve writes to the shared graph with: where it came from,
    /// the key, and the handle the graph made up, when signed in here.
    pub fn graph_key(&self) -> Option<(kelve_core::views::KeyFrom, String, Option<String>)> {
        if let Some(key) = &self.config.env.graph {
            return Some((kelve_core::views::KeyFrom::Env, key.clone(), None));
        }
        let held = self.keys.read().ok()?.graph.clone()?;
        Some((kelve_core::views::KeyFrom::App, held.key, Some(held.handle)))
    }

    /// Keeps the key signing in to the shared graph gave, sealed, or
    /// forgets it.
    pub async fn hold_graph_key(&self, signed: Option<(&str, &str)>) -> ApiResult<()> {
        let held = signed.map(|(key, handle)| GraphKey {
            key: key.to_string(),
            handle: handle.to_string(),
        });
        let json = held
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(ApiError::internal)?;
        self.hold(GRAPH_KEY, json.as_deref()).await?;
        if let Ok(mut k) = self.keys.write() {
            k.graph = held;
        }
        // Another account, or none, has a day's limits of its own.
        self.hold_sends(None).await
    }

    async fn hold(&self, name: &str, key: Option<&str>) -> ApiResult<()> {
        let mut conn = self.db.acquire().await?;
        match key {
            Some(key) => {
                let sealed = self.config.key.seal(key.as_bytes());
                kelve_store::settings::put_secret(&mut conn, name, &sealed).await?;
            }
            None => kelve_store::settings::delete_secret(&mut conn, name).await?,
        }
        Ok(())
    }

    /// Tells the web app something changed.
    pub fn publish(&self, event: ServerEvent) {
        if matches!(
            event,
            ServerEvent::Marks { .. } | ServerEvent::Read { .. } | ServerEvent::Spec
        ) {
            crate::reader::marked_again();
        }
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
    let mut settings: Settings = serde_json::from_value(value).unwrap_or_else(|e| {
        tracing::error!(error = %e, "the saved settings don't read, so kelve starts without them");
        Settings::default()
    });
    // Adding to the shared graph is part of using it: one turned off
    // before is on.
    settings.contribute = true;
    Ok(settings)
}

async fn load_held(db: &Pool, config: &Config) -> Result<Held, kelve_store::Error> {
    let mut conn = db.acquire().await?;
    let open = |sealed: Option<Vec<u8>>, name: &str| {
        let sealed = sealed?;
        let opened = config
            .key
            .open(&sealed)
            .and_then(|b| String::from_utf8(b).ok());
        if opened.is_none() {
            tracing::warn!(
                secret = name,
                "a key pasted in the app doesn't open with this install's sealing key; paste it again"
            );
        }
        opened
    };
    let graph = kelve_store::settings::secret(&mut conn, GRAPH_KEY).await?;
    Ok(Held {
        graph: open(graph, GRAPH_KEY).and_then(|json| serde_json::from_str(&json).ok()),
    })
}

async fn load_sends_held(
    db: &Pool,
) -> Result<Option<kelve_store::sends::Held>, kelve_store::Error> {
    let mut conn = db.acquire().await?;
    kelve_store::sends::held(&mut conn).await
}
