//! Knowing a newer kelve is out (docs/releasing.md): at most once a day, in
//! the background, kelve asks where its releases are for the newest one,
//! and keeps its version and the first line of its notes. The request is a
//! plain GET, with nothing about the person or their work: whoever serves
//! it sees only an address and kelve's version. `KELVE_UPDATES=off` stops
//! it. Settings show what it found, `kelve update` installs it, and the
//! person's agent is told of it once for each version, to ask them.

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use kelve_core::update::{agent_line, is_newer, notes_line};
use kelve_core::views::{NewerKelve, ServerEvent, UpdateView};
use serde::Serialize;
use serde_json::Value;

use crate::auth::{Agent, Me};
use crate::error::ApiResult;
use crate::state::Shared;

/// This kelve's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// How often kelve asks: once a day.
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// Where kelve's releases are by default: the newest release of the
/// repository `install.sh` names, from GitHub's API.
pub fn latest() -> String {
    let repo = env!("CARGO_PKG_REPOSITORY")
        .trim_end_matches('/')
        .trim_start_matches("https://github.com/");
    format!("https://api.github.com/repos/{repo}/releases/latest")
}

/// Where the background check asks, from `KELVE_UPDATES`: `off` for
/// nowhere; an address to ask there instead, in the shape of GitHub's
/// newest release; and, when it's not set, GitHub, but for a development
/// build, whose version no release has.
pub fn source(given: Option<&str>) -> Result<Option<String>, String> {
    match given.map(str::trim).filter(|v| !v.is_empty()) {
        Some("off") => Ok(None),
        Some(address) => web_address(address).map(Some),
        None if cfg!(debug_assertions) => Ok(None),
        None => Ok(Some(latest())),
    }
}

/// Where `kelve update` asks, when the person runs it: `KELVE_UPDATES`'s
/// address if it gives one, else GitHub. `off` stops only the background
/// check.
pub fn source_to_update() -> Result<String, String> {
    match std::env::var("KELVE_UPDATES")
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some(v) if !v.is_empty() && v != "off" => web_address(v),
        _ => Ok(latest()),
    }
}

fn web_address(given: &str) -> Result<String, String> {
    match url::Url::parse(given) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => {
            Ok(given.trim_end_matches('/').to_string())
        }
        _ => Err("use off, or an http or https address".into()),
    }
}

/// The newest release, as where kelve's releases are says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Its version, without the tag's `v`: `0.3.0`.
    pub version: String,
    /// The first line of its notes, plain.
    pub line: String,
    /// Each file it has, by name, and where to download it.
    pub files: Vec<(String, String)>,
}

impl Release {
    /// Where to download one of its files.
    pub fn file(&self, name: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, url)| url.as_str())
    }
}

/// Asks `source` for the newest release. Its files must come over https
/// when it does.
pub async fn newest(http: &reqwest::Client, source: &str) -> Result<Release, String> {
    let response = http
        .get(source)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("couldn't ask for kelve's newest release: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "asking for kelve's newest release came to {}",
            response.status()
        ));
    }
    let release: Value = response
        .json()
        .await
        .map_err(|e| format!("kelve's newest release didn't read: {e}"))?;
    read(&release, source.starts_with("https://"))
}

/// A release as GitHub's API gives it.
pub fn read(release: &Value, https: bool) -> Result<Release, String> {
    let tag = release["tag_name"].as_str().unwrap_or_default();
    let version = kelve_core::update::Version::parse(tag)
        .ok_or_else(|| "kelve's newest release has no version kelve reads".to_string())?;
    let files = release["assets"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|a| {
                    let name = a["name"].as_str()?;
                    let url = a["browser_download_url"].as_str()?;
                    let fine = if https {
                        url.starts_with("https://")
                    } else {
                        url.starts_with("http://") || url.starts_with("https://")
                    };
                    fine.then(|| (name.to_string(), url.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Release {
        version: version.to_string(),
        line: notes_line(release["body"].as_str().unwrap_or_default()),
        files,
    })
}

/// Starts the daily check, unless the install has none.
pub fn start(app: &Shared) {
    if app.config.updates.is_none() {
        return;
    }
    let app = app.clone();
    tokio::spawn(async move {
        loop {
            let wait = match due_in(&app).await {
                Some(Duration::ZERO) => {
                    check(&app).await;
                    DAY
                }
                Some(wait) => wait,
                // The store didn't answer: try again in an hour.
                None => Duration::from_secs(60 * 60),
            };
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = app.stopped() => return,
            }
        }
    });
}

/// How long until the next check is due: none now.
async fn due_in(app: &Shared) -> Option<Duration> {
    let mut conn = app.db.acquire().await.ok()?;
    let heard = kelve_store::updates::heard(&mut conn).await.ok()?;
    let since = kelve_store::now().saturating_sub(heard.checked_at);
    let since = Duration::from_millis(u64::try_from(since).unwrap_or(0));
    Some(DAY.saturating_sub(since))
}

/// Asks for the newest release now, and keeps what it found. One that
/// fails waits for the next day, as one that worked does.
pub async fn check(app: &Shared) {
    let Some(source) = app.config.updates.clone() else {
        return;
    };
    let Ok(mut conn) = app.db.acquire().await else {
        return;
    };
    if let Err(e) = kelve_store::updates::asked(&mut conn).await {
        tracing::warn!(error = %e, "couldn't note that kelve asked for its newest release");
        return;
    }
    match newest(&app.http, &source).await {
        Ok(release) => {
            if let Err(e) =
                kelve_store::updates::newest(&mut conn, &release.version, &release.line).await
            {
                tracing::warn!(error = %e, "couldn't keep kelve's newest release");
                return;
            }
            if is_newer(&release.version, VERSION) {
                tracing::info!(newest = %release.version, "a newer kelve is out; `kelve update` installs it");
                app.publish(ServerEvent::Settings);
            }
        }
        Err(e) => {
            tracing::info!(error = %e, "couldn't hear of kelve's newest release; asking again tomorrow")
        }
    }
}

/// Which kelve this is, and the newest, when it's newer.
pub async fn view(app: &Shared) -> ApiResult<UpdateView> {
    let mut conn = app.db.acquire().await?;
    let heard = kelve_store::updates::heard(&mut conn).await?;
    Ok(UpdateView {
        version: VERSION.to_string(),
        newer: heard
            .newest
            .filter(|(v, _)| is_newer(v, VERSION))
            .map(|(version, line)| NewerKelve { version, line }),
    })
}

/// The line the agent is told of a newer kelve: once for each version, and
/// never again after, even once kelve restarts.
pub async fn tell(app: &Shared) -> ApiResult<Option<String>> {
    let Some(newer) = view(app).await?.newer else {
        return Ok(None);
    };
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::updates::tell(&mut conn, &newer.version)
        .await?
        .then(|| agent_line(&newer.version, VERSION, &newer.line)))
}

/// What settings show.
pub async fn for_me(State(app): State<Shared>, _me: Me) -> ApiResult<Json<UpdateView>> {
    Ok(Json(view(&app).await?))
}

/// What `kelve update` and `kelve status` ask of the kelve running.
pub async fn for_agent(State(app): State<Shared>, _agent: Agent) -> ApiResult<Json<UpdateView>> {
    Ok(Json(view(&app).await?))
}

#[derive(Debug, Serialize)]
pub struct Told {
    /// The line for the agent, the first time it's asked for a version.
    pub line: Option<String>,
}

/// The line for the agent, once: what kelve's hook and its tools add to
/// what they say.
pub async fn tell_agent(State(app): State<Shared>, _agent: Agent) -> ApiResult<Json<Told>> {
    Ok(Json(Told {
        line: tell(&app).await?,
    }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn releases_are_found_where_the_installer_finds_them() {
        assert_eq!(
            latest(),
            "https://api.github.com/repos/kelve-ai/kelve/releases/latest"
        );
        assert_eq!(source(Some("off")), Ok(None));
        assert_eq!(
            source(Some("http://127.0.0.1:4630/latest/")),
            Ok(Some("http://127.0.0.1:4630/latest".into()))
        );
        assert!(source(Some("not an address")).is_err());
        assert_eq!(source(None).unwrap().is_none(), cfg!(debug_assertions));
    }

    #[test]
    fn a_release_reads_with_its_files_over_https_only_when_asked_over_https() {
        let release = json!({
            "tag_name": "v0.3.0",
            "body": "## What's Changed\n* Updates through your agent.\n",
            "assets": [
                { "name": "kelve-macos-arm64.tar.gz", "browser_download_url": "https://github.com/x/kelve-macos-arm64.tar.gz" },
                { "name": "SHA256SUMS", "browser_download_url": "http://example.com/SHA256SUMS" },
            ],
        });
        let r = read(&release, true).unwrap();
        assert_eq!(r.version, "0.3.0");
        assert_eq!(r.line, "Updates through your agent.");
        assert!(r.file("kelve-macos-arm64.tar.gz").is_some());
        assert_eq!(r.file("SHA256SUMS"), None, "not over plain http");
        assert!(read(&release, false).unwrap().file("SHA256SUMS").is_some());
        assert!(read(&json!({ "tag_name": "nightly" }), true).is_err());
    }
}
