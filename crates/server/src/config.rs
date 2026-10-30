//! How the install runs: read once, from the environment, at start. The rest
//! is set in the app, and lives in the local store; secrets from the
//! environment never do.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use chrono::FixedOffset;

use crate::crypto::Key;

#[derive(Clone)]
pub struct Config {
    /// Where the person opens kelve, without a trailing slash:
    /// `http://kelve.localhost:4600`. Browsers and macOS send any name
    /// under `.localhost` to this computer, so kelve has a name of its own
    /// without anyone touching their hosts file.
    pub url: String,
    pub bind: SocketAddr,
    /// The local store: one SQLite file, in the data folder unless
    /// `KELVE_STORE` says otherwise: one for each shared graph
    /// (`store_name`).
    pub store: PathBuf,
    /// Seals the keys pasted in the app before they're stored.
    pub key: Key,
    /// Where kelve keeps files of its own.
    pub data: PathBuf,
    /// The secret kelve and its command line share (`kelve open`, `kelve
    /// diff`), in a file only its owner can read.
    pub secret_file: PathBuf,
    /// Keys the install is handed in its environment. They win over keys
    /// pasted in the app.
    pub env: EnvKeys,
    /// Where kelve reaches each provider: their own APIs, unless a gateway
    /// (or a test's stand-in) is set.
    pub endpoints: Endpoints,
    /// The person's time zone, as an offset from UTC: what "today" means.
    pub offset: FixedOffset,
    /// Whether a development build may add to the real shared graph
    /// (`KELVE_WRITE_REAL_GRAPH=1`). Without it, one built from a checkout
    /// only reads the real graph, so trying kelve out never adds to it.
    pub write_real_graph: bool,
    /// Claude Code's folder, whose settings hold the hook that tells it of
    /// notes left for it (`KELVE_CLAUDE_DIR`, else Claude Code's own
    /// `CLAUDE_CONFIG_DIR`, else `~/.claude`).
    pub claude_dir: PathBuf,
    /// The home folder other agents' settings are in, for their hooks
    /// (`KELVE_AGENTS_HOME`, else `HOME`).
    pub agents_home: PathBuf,
    /// How often a lesson that fits is held back from the agent, to measure
    /// the lessons, in thousandths (`KELVE_HOLD_BACK`, else 50: one in
    /// twenty), once it's been given `hold_back_from` times across everyone.
    /// Tests and the example's recording turn it off.
    pub hold_back: i64,
    /// How many times a lesson must have been given, across every account,
    /// before it's ever held back: a new or rare lesson's every use is
    /// evidence of its own (100; tests may lower it).
    pub hold_back_from: u32,
    /// How long a lesson the agent wrote waits before it leaves for the
    /// shared graph, so the person can stop it, in seconds
    /// (`KELVE_LESSON_HOLD`, else a day). For trying lessons out.
    pub lesson_hold: i64,
    /// Where kelve asks for its newest release, once a day: GitHub's
    /// newest release of its repository, or `KELVE_UPDATES`'s address.
    /// None with `KELVE_UPDATES=off`, and for a development build that
    /// isn't given an address (`updates::source`).
    pub updates: Option<String>,
    /// What turned counts off where kelve runs, whatever the person chose:
    /// `DO_NOT_TRACK` set to anything but `0` or `false`, or
    /// `KELVE_TELEMETRY=off`.
    pub counts_off: Option<String>,
}

/// The shared graph people use.
pub const REAL_GRAPH: &str = "https://graph.kelve.ai";

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("url", &self.url)
            .field("bind", &self.bind)
            .finish_non_exhaustive()
    }
}

/// Keys the install is handed in its environment.
#[derive(Clone, Default)]
pub struct EnvKeys {
    /// The key kelve adds to the shared graph with. Signing in, which
    /// gives a person's kelve one, comes with hosting.
    pub graph: Option<String>,
}

impl std::fmt::Debug for EnvKeys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnvKeys")
            .field("graph", &self.graph.is_some())
            .finish()
    }
}

/// Where kelve reaches each service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoints {
    /// The shared knowledge graph; empty for none.
    pub graph: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Endpoints {
            // A build can say where the graph is when it's made.
            // `KELVE_GRAPH_URL` still wins.
            graph: option_env!("KELVE_DEFAULT_GRAPH_URL")
                .unwrap_or(REAL_GRAPH)
                .into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{name} isn't valid: {why}")]
    Invalid { name: &'static str, why: String },
    #[error("couldn't read or write {path}: {source}")]
    File {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error(
        "{0} can be read by other people on this computer. Make it yours alone with `chmod 600 {0}`, then start kelve again."
    )]
    Exposed(String),
}

fn var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn http_url(name: &'static str, value: String) -> Result<String, ConfigError> {
    match url::Url::parse(&value) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => {
            Ok(value.trim_end_matches('/').to_string())
        }
        _ => Err(ConfigError::Invalid {
            name,
            why: "use an http or https address".into(),
        }),
    }
}

impl Config {
    /// Reads the environment. See `docs/self-hosting.md` for every setting.
    pub fn from_env() -> Result<Config, ConfigError> {
        let bind: SocketAddr = var("KELVE_BIND")
            .unwrap_or_else(|| "127.0.0.1:4600".into())
            .parse()
            .map_err(|e: std::net::AddrParseError| ConfigError::Invalid {
                name: "KELVE_BIND",
                why: e.to_string(),
            })?;
        let url = http_url(
            "KELVE_URL",
            var("KELVE_URL").unwrap_or_else(|| format!("http://kelve.localhost:{}", bind.port())),
        )?;
        let data = data_dir();
        let key = match var("KELVE_SECRET_KEY") {
            Some(hex) => Key::from_hex(&hex).ok_or(ConfigError::Invalid {
                name: "KELVE_SECRET_KEY",
                why: "use 64 hex characters".into(),
            })?,
            None => key_file(&data)?,
        };
        let secret_file = var("KELVE_SECRET_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(secret_path);
        let mut endpoints = Endpoints::default();
        // `off` uses no shared graph: kelve shows only what it has kept.
        if let Some(value) = var("KELVE_GRAPH_URL") {
            endpoints.graph = if value == "off" {
                String::new()
            } else {
                http_url("KELVE_GRAPH_URL", value)?
            };
        }
        let offset = match var("KELVE_UTC_OFFSET_MINUTES") {
            Some(m) => m
                .parse::<i32>()
                .ok()
                .and_then(|m| FixedOffset::east_opt(m * 60))
                .ok_or(ConfigError::Invalid {
                    name: "KELVE_UTC_OFFSET_MINUTES",
                    why: "use a whole number of minutes, like 60 or -300".into(),
                })?,
            None => *chrono::Local::now().offset(),
        };
        let claude_dir = var("KELVE_CLAUDE_DIR")
            .or_else(|| var("CLAUDE_CONFIG_DIR"))
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::home_dir().unwrap_or_default().join(".claude"));
        // The home folder as the system has it: `HOME`, or on Windows the
        // user's profile.
        let agents_home = var("KELVE_AGENTS_HOME")
            .map(PathBuf::from)
            .or_else(std::env::home_dir)
            .unwrap_or_default();
        let hold_back = match var("KELVE_HOLD_BACK") {
            Some(v) => v
                .parse::<i64>()
                .ok()
                .filter(|n| (0..=1000).contains(n))
                .ok_or(ConfigError::Invalid {
                    name: "KELVE_HOLD_BACK",
                    why: "use a whole number of thousandths, from 0 to 1000, like 50".into(),
                })?,
            None => 50,
        };
        let lesson_hold = match var("KELVE_LESSON_HOLD") {
            Some(v) => v
                .parse::<i64>()
                .ok()
                .filter(|n| (0..=30 * 24 * 60 * 60).contains(n))
                .ok_or(ConfigError::Invalid {
                    name: "KELVE_LESSON_HOLD",
                    why: "use a whole number of seconds, from 0 to 30 days, like 86400 for a day"
                        .into(),
                })?,
            None => kelve_core::lesson::HOLD_SECS,
        };
        let updates = crate::updates::source(var("KELVE_UPDATES").as_deref()).map_err(|why| {
            ConfigError::Invalid {
                name: "KELVE_UPDATES",
                why,
            }
        })?;
        Ok(Config {
            updates,
            counts_off: counts_off(var("DO_NOT_TRACK"), var("KELVE_TELEMETRY")),
            claude_dir,
            agents_home,
            hold_back,
            hold_back_from: kelve_core::lesson::HOLD_BACK_FROM,
            lesson_hold,
            url,
            bind,
            store: var("KELVE_STORE")
                .map(PathBuf::from)
                .unwrap_or_else(|| data.join(store_name(&endpoints.graph))),
            key,
            data,
            secret_file,
            env: env_keys()?,
            endpoints,
            offset,
            write_real_graph: var("KELVE_WRITE_REAL_GRAPH").as_deref() == Some("1"),
        })
    }

    /// Whether kelve may add to the shared graph it reads: always, but for
    /// a development build pointed at the real one, unless it's told to.
    pub fn may_write_graph(&self) -> bool {
        !cfg!(debug_assertions) || self.write_real_graph || !is_real_graph(&self.endpoints.graph)
    }

    /// The host people see in links: `kelve.localhost:4600`.
    pub fn host(&self) -> String {
        url::Url::parse(&self.url)
            .ok()
            .and_then(|u| {
                let host = u.host_str()?.to_string();
                Some(match u.port() {
                    Some(port) => format!("{host}:{port}"),
                    None => host,
                })
            })
            .unwrap_or_default()
    }

    /// Whether cookies need the Secure flag: everywhere but plain http.
    pub fn secure(&self) -> bool {
        self.url.starts_with("https://")
    }

    /// Today, in the person's time zone.
    pub fn today(&self) -> chrono::NaiveDate {
        chrono::Utc::now().with_timezone(&self.offset).date_naive()
    }

    /// The day a moment falls on, in the person's time zone.
    pub fn day_of(&self, at: chrono::DateTime<chrono::Utc>) -> chrono::NaiveDate {
        at.with_timezone(&self.offset).date_naive()
    }
}

/// What turns counts off, from `DO_NOT_TRACK` and `KELVE_TELEMETRY`.
fn counts_off(dnt: Option<String>, telemetry: Option<String>) -> Option<String> {
    let on = |v: &str| {
        !matches!(
            v.to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    };
    if dnt.as_deref().is_some_and(on) {
        Some("DO_NOT_TRACK".into())
    } else if telemetry.is_some_and(|v| v.eq_ignore_ascii_case("off")) {
        Some("KELVE_TELEMETRY".into())
    } else {
        None
    }
}

/// The keys in the environment.
fn env_keys() -> Result<EnvKeys, ConfigError> {
    Ok(EnvKeys {
        graph: var("KELVE_GRAPH_KEY"),
    })
}

/// Where kelve keeps its files: `KELVE_DATA`, or beside the secret
/// (`~/.config/kelve`), so it's the same folder wherever kelve is started.
pub fn data_dir() -> PathBuf {
    let data = var("KELVE_DATA").map(PathBuf::from).unwrap_or_else(|| {
        secret_path()
            .parent()
            .map_or_else(|| PathBuf::from("data"), Path::to_path_buf)
    });
    std::path::absolute(&data).unwrap_or(data)
}

/// Where the secret kelve shares with its command line lives:
/// `~/.config/kelve/secret` (or under `$XDG_CONFIG_HOME`, or `%APPDATA%`).
/// `KELVE_SECRET_FILE` overrides it, for both.
pub fn secret_path() -> PathBuf {
    if let Some(path) = var("KELVE_SECRET_FILE") {
        return PathBuf::from(path);
    }
    let base = var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| var("APPDATA").filter(|_| cfg!(windows)).map(PathBuf::from))
        .or_else(|| std::env::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("kelve").join("secret")
}

/// The shared secret: read, or made the first time, in a file only its
/// owner can read. kelve won't use a secret others can read.
pub fn ensure_secret(path: &Path) -> Result<String, ConfigError> {
    let fail = |source| ConfigError::File {
        path: path.to_path_buf(),
        source,
    };
    if let Ok(text) = std::fs::read_to_string(path) {
        check_private(path)?;
        let secret = text.trim().to_string();
        if secret.len() >= 32 {
            return Ok(secret);
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(fail)?;
        private_dir(dir).map_err(fail)?;
    }
    let secret = crate::crypto::token();
    write_private(path, &secret, true).map_err(fail)?;
    tracing::info!(path = %path.display(), "made the secret kelve shares with its command line");
    Ok(secret)
}

/// Reads the shared secret, if there is one. For the command line.
pub fn read_secret(path: &Path) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            check_private(path)?;
            Ok(Some(text.trim().to_string()).filter(|s| !s.is_empty()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ConfigError::File {
            path: path.to_path_buf(),
            source,
        }),
    }
}

#[cfg(unix)]
fn check_private(path: &Path) -> Result<(), ConfigError> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|source| ConfigError::File {
        path: path.to_path_buf(),
        source,
    })?;
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(ConfigError::Exposed(path.display().to_string()));
    }
    Ok(())
}

// Windows keeps a user's profile folders private by default; kelve doesn't
// set an access list of its own yet (docs/self-hosting.md says so).
#[cfg(not(unix))]
fn check_private(_path: &Path) -> Result<(), ConfigError> {
    Ok(())
}

#[cfg(unix)]
fn private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn private_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

/// The install's sealing key, kept in a file under the data directory when
/// it isn't given: made on first start, readable only by kelve.
fn key_file(dir: &Path) -> Result<Key, ConfigError> {
    let path = dir.join("secret.key");
    let fail = |source| ConfigError::File {
        path: path.clone(),
        source,
    };
    if let Ok(hex) = std::fs::read_to_string(&path) {
        return Key::from_hex(hex.trim()).ok_or(ConfigError::Invalid {
            name: "the key file",
            why: "it should hold 64 hex characters".into(),
        });
    }
    std::fs::create_dir_all(dir).map_err(fail)?;
    let key = Key::generate();
    write_private(&path, &key.to_hex(), false).map_err(fail)?;
    tracing::info!(path = %path.display(), "made a new sealing key; keep it with your backups");
    Ok(key)
}

#[cfg(unix)]
fn write_private(path: &Path, text: &str, replace: bool) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).mode(0o600);
    if replace {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(path)?;
    file.write_all(text.as_bytes())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn write_private(path: &Path, text: &str, _replace: bool) -> std::io::Result<()> {
    std::fs::write(path, text)
}

/// A config for tests: a fresh data folder, store and secret, and the
/// stand-ins' addresses where the test gives them.
pub fn test_config() -> Config {
    let data = std::env::temp_dir().join(format!("kelve-test-{}", uuid::Uuid::now_v7().simple()));
    Config {
        url: "http://localhost:4600".into(),
        bind: SocketAddr::from(([127, 0, 0, 1], 4600)),
        store: data.join("kelve.db"),
        key: Key::generate(),
        secret_file: data.join("secret"),
        data: data.clone(),
        env: EnvKeys::default(),
        // Tests never reach the real shared graph.
        endpoints: Endpoints {
            graph: String::new(),
        },
        offset: FixedOffset::east_opt(0)
            .unwrap_or_else(|| *chrono::Utc::now().fixed_offset().offset()),
        write_real_graph: false,
        // Never the person's own Claude Code settings.
        claude_dir: data.join("claude"),
        agents_home: data.join("home"),
        hold_back: 0,
        hold_back_from: kelve_core::lesson::HOLD_BACK_FROM,
        lesson_hold: kelve_core::lesson::HOLD_SECS,
        // Never GitHub.
        updates: None,
        counts_off: None,
    }
}

/// Whether an address is the real shared graph's.
/// The store's file in the data folder for a shared graph's address:
/// `kelve.db` for the graph a build reads unless told otherwise, and
/// `kelve-<host>-<port>.db` for any other, or `kelve-off.db` for none. Each
/// graph has its own sign-in and its own ideas, entries and lessons, so
/// changing `KELVE_GRAPH_URL` changes graphs whole, and changing it back
/// finds everything as it was.
pub fn store_name(graph: &str) -> String {
    let same = |a: &str, b: &str| {
        a.trim_end_matches('/')
            .eq_ignore_ascii_case(b.trim_end_matches('/'))
    };
    if same(graph, &Endpoints::default().graph) {
        return "kelve.db".into();
    }
    let address = match url::Url::parse(graph) {
        Ok(u) => {
            let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
            let path = u.path().trim_matches('/');
            [
                host,
                u.port().map(|p| p.to_string()).unwrap_or_default(),
                path.to_string(),
            ]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-")
        }
        Err(_) => String::new(),
    };
    let address: String = address
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if address.is_empty() {
        "kelve-off.db".into()
    } else {
        format!("kelve-{address}.db")
    }
}

fn is_real_graph(url: &str) -> bool {
    let host = |url: &str| url::Url::parse(url).ok()?.host_str().map(str::to_string);
    host(url).is_some_and(|h| Some(h) == host(REAL_GRAPH))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn a_development_build_writes_to_the_real_graph_only_when_told() {
        let mut config = test_config();
        config.endpoints.graph = "http://127.0.0.1:4620".into();
        assert!(
            config.may_write_graph(),
            "a graph of one's own is always fine"
        );
        config.endpoints.graph = REAL_GRAPH.into();
        assert_eq!(config.may_write_graph(), !cfg!(debug_assertions));
        config.endpoints.graph = "https://graph.kelve.ai/".into();
        assert_eq!(config.may_write_graph(), !cfg!(debug_assertions));
        config.write_real_graph = true;
        assert!(config.may_write_graph());
    }

    #[test]
    fn each_graph_has_a_store_of_its_own() {
        let default = Endpoints::default().graph;
        assert_eq!(store_name(&default), "kelve.db");
        assert_eq!(store_name(&format!("{default}/")), "kelve.db");
        assert_eq!(
            store_name("https://graph.staging.example.com"),
            "kelve-graph.staging.example.com.db"
        );
        assert_eq!(
            store_name("http://127.0.0.1:4620/"),
            "kelve-127.0.0.1-4620.db"
        );
        assert_eq!(
            store_name("http://LOCALHOST:4620"),
            "kelve-localhost-4620.db"
        );
        assert_eq!(
            store_name("https://example.com/graphs/a"),
            "kelve-example.com-graphs-a.db"
        );
        assert_eq!(store_name(""), "kelve-off.db");
        assert_ne!(
            store_name("http://127.0.0.1:4620"),
            store_name("http://127.0.0.1:4621")
        );
    }

    #[test]
    fn do_not_track_or_kelve_telemetry_off_turns_counts_off() {
        let some = |v: &str| Some(v.to_string());
        assert_eq!(counts_off(None, None), None);
        assert_eq!(counts_off(some("1"), None).as_deref(), Some("DO_NOT_TRACK"));
        assert_eq!(
            counts_off(some("true"), None).as_deref(),
            Some("DO_NOT_TRACK")
        );
        assert_eq!(counts_off(some("0"), None), None);
        assert_eq!(
            counts_off(None, some("off")).as_deref(),
            Some("KELVE_TELEMETRY")
        );
        assert_eq!(counts_off(None, some("on")), None);
    }

    #[test]
    fn the_secret_is_made_once_and_kept_private() {
        let dir =
            std::env::temp_dir().join(format!("kelve-secret-{}", uuid::Uuid::now_v7().simple()));
        let path = dir.join("kelve").join("secret");
        let first = ensure_secret(&path).expect("made");
        let again = ensure_secret(&path).expect("read");
        assert_eq!(first, again);
        assert!(first.len() >= 32);
        assert_eq!(
            read_secret(&path).expect("read").as_deref(),
            Some(first.as_str())
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
                .expect("opened up");
            assert!(matches!(read_secret(&path), Err(ConfigError::Exposed(_))));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
