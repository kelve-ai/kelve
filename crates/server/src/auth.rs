//! Signing in, on the person's own computer. There's no account: `kelve serve`
//! opens the browser with a one-time link, valid for a minute, which the
//! browser swaps for a session cookie. `kelve open` asks for a new one. Agents
//! report with the secret kelve shares with its command line, which lets them
//! report and nothing else.
//!
//! A link is never handed to whoever asks for it. kelve gives it to the
//! person itself, through whatever runs it: it opens their browser, or
//! shows the link where they started kelve. So the secret, which agents'
//! hooks hold, can open a browser on this computer, but never sign
//! anything in.
//!
//! Every request must be addressed to kelve's own host, which stops a web
//! page from reaching kelve by pointing a name of its own at this computer
//! (DNS rebinding). A browser's changes must come from kelve's own pages
//! (`Origin`, `Sec-Fetch-Site`), which stops another site from acting as
//! its person.

use axum::Json;
use axum::extract::{FromRequestParts, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::crypto;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

/// The session cookie's name.
pub const COOKIE: &str = "kelve";

/// How long a one-time link lasts.
const LINK_SECONDS: i64 = 60;

/// How long a browser stays signed in: until it signs out, its cookies are
/// cleared, or this long passes.
const SESSION_DAYS: i64 = 400;

/// The hosts kelve answers to: its own address's, and this computer's own
/// names on kelve's port.
pub fn allowed_hosts(app: &crate::state::App) -> Vec<String> {
    let port = app.config.bind.port();
    let mut hosts = vec![
        app.config.host().to_ascii_lowercase(),
        format!("localhost:{port}"),
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
    ];
    if let Ok(url) = url::Url::parse(&app.config.url)
        && url.port().is_none()
        && let Some(host) = url.host_str()
    {
        hosts.push(host.to_ascii_lowercase());
    }
    hosts
}

/// Turns away requests addressed to any host but kelve's own.
pub async fn guard(State(app): State<Shared>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .map(str::to_ascii_lowercase);
    match host {
        Some(host) if allowed_hosts(&app).contains(&host) => next.run(request).await,
        _ => (
            StatusCode::MISDIRECTED_REQUEST,
            "kelve only answers at its own address.",
        )
            .into_response(),
    }
}

/// Whether a request that changes something came from kelve's own pages.
fn same_origin(app: &crate::state::App, headers: &HeaderMap) -> bool {
    if let Some(site) = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()) {
        return matches!(site, "same-origin" | "none");
    }
    match headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        Some(origin) => url::Url::parse(origin)
            .ok()
            .and_then(|u| {
                let host = u.host_str()?.to_ascii_lowercase();
                Some(match u.port_or_known_default() {
                    Some(port) => format!("{host}:{port}"),
                    None => host,
                })
            })
            .is_some_and(|o| {
                let port = app.config.bind.port();
                allowed_hosts(app).contains(&o) || o == format!("{}:{port}", app.config.host())
            }),
        // Neither header: not a browser, so not one acting for another site.
        None => true,
    }
}

/// The person, signed in on this browser.
#[derive(Debug, Clone, Copy)]
pub struct Me;

impl FromRequestParts<Shared> for Me {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &Shared) -> ApiResult<Me> {
        let token = cookie(&parts.headers, COOKIE)
            .ok_or_else(|| ApiError::unauthorized("Open kelve with `kelve open`."))?;
        let mut conn = app.db.acquire().await?;
        if !kelve_store::auth::session(&mut conn, &crypto::hash(&token)).await? {
            return Err(ApiError::unauthorized(
                "This browser was signed out. Open kelve again with `kelve open`.",
            ));
        }
        if !matches!(parts.method, Method::GET | Method::HEAD) && !same_origin(app, &parts.headers)
        {
            return Err(ApiError::forbidden(
                "That came from another site, so kelve ignored it.",
            ));
        }
        Ok(Me)
    }
}

/// kelve's own command line, on this computer: `kelve open`, `kelve diff`.
/// It can ask kelve to open something, and learn whether it could; it never
/// reads what kelve keeps.
#[derive(Debug, Clone, Copy)]
pub struct Agent;

impl FromRequestParts<Shared> for Agent {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &Shared) -> ApiResult<Agent> {
        let given = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .unwrap_or_default();
        if !given.is_empty() && constant_eq(given.as_bytes(), app.secret.as_bytes()) {
            Ok(Agent)
        } else {
            Err(ApiError::unauthorized(
                "kelve didn't recognise this computer's secret. Restart kelve with `kelve serve`.",
            ))
        }
    }
}

/// Who's asking: the person in their browser, or kelve's command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Who {
    Me,
    Agent,
}

impl FromRequestParts<Shared> for Who {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &Shared) -> ApiResult<Who> {
        if parts.headers.contains_key(header::AUTHORIZATION) {
            return Agent::from_request_parts(parts, app)
                .await
                .map(|_| Who::Agent);
        }
        Me::from_request_parts(parts, app).await.map(|_| Who::Me)
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
        .filter(|v| !v.is_empty())
}

/// Makes a one-time link that signs a browser in.
pub async fn new_link(app: &Shared) -> ApiResult<String> {
    link_to(app, None).await
}

/// Makes a one-time link that signs a browser in and opens something:
/// `Some(7)` for the seventh thing opened.
pub async fn link_to(app: &Shared, opened: Option<i64>) -> ApiResult<String> {
    let token = crypto::token();
    let mut conn = app.db.acquire().await?;
    kelve_store::auth::add_link(
        &mut conn,
        &crypto::hash(&token),
        Utc::now() + Duration::seconds(LINK_SECONDS),
    )
    .await?;
    let then = opened.map(|id| format!("&opened={id}")).unwrap_or_default();
    Ok(format!("{}/signin?token={token}{then}", app.config.url))
}

/// What asking for a link did: whether a browser opened with it. The link
/// itself never comes back.
#[derive(Debug, Serialize)]
pub struct Opened {
    pub opened: bool,
}

/// `kelve open` asks for a link, with the shared secret. kelve gives it to
/// the person, not to whoever asked.
pub async fn link(State(app): State<Shared>, _agent: Agent) -> ApiResult<Json<Opened>> {
    let link = new_link(&app).await?;
    Ok(Json(Opened {
        opened: app.hand_off(&link),
    }))
}

#[derive(Debug, Deserialize)]
pub struct SignIn {
    token: Option<String>,
    /// What to show once signed in: something opened, by its id.
    opened: Option<i64>,
}

/// Swaps a one-time link for a session cookie, then goes to Next, leaving
/// the link out of the address bar and the history.
pub async fn sign_in(State(app): State<Shared>, Query(q): Query<SignIn>) -> Response {
    let Some(token) = q.token.filter(|t| !t.is_empty()) else {
        return signed_out_page();
    };
    let used = match app.db.acquire().await {
        Ok(mut conn) => kelve_store::auth::use_link(&mut conn, &crypto::hash(&token)).await,
        Err(e) => Err(e.into()),
    };
    match used {
        Ok(true) => {}
        Ok(false) => return signed_out_page(),
        Err(e) => return ApiError::internal(e).into_response(),
    }
    let session = crypto::token();
    let kept = match app.db.acquire().await {
        Ok(mut conn) => {
            kelve_store::auth::add_session(
                &mut conn,
                &crypto::hash(&session),
                Some(Utc::now() + Duration::days(SESSION_DAYS)),
            )
            .await
        }
        Err(e) => Err(e.into()),
    };
    if let Err(e) = kept {
        return ApiError::internal(e).into_response();
    }
    let secure = if app.config.secure() { "; Secure" } else { "" };
    let cookie = format!(
        "{COOKIE}={session}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}{secure}",
        SESSION_DAYS * 86_400
    );
    let to = q
        .opened
        .map_or_else(|| "/".to_string(), |id| format!("/#/o/{id}"));
    let mut response = Redirect::to(&to).into_response();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, value);
    }
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response
}

fn signed_out_page() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        "<!doctype html><meta charset=utf-8><title>kelve</title>\
<body style='font:17px/1.6 system-ui;max-width:32rem;margin:18vh auto;padding:0 1rem;color:#2b2a28;background:#f7f2e7'>\
<p style='font-weight:600'>That link has already been used, or it ran out.</p>\
<p>Links to kelve work once, for a minute. Run <code>kelve open</code> for a new one.</p>",
    )
        .into_response()
}

/// Signs this browser out.
pub async fn sign_out(
    State(app): State<Shared>,
    _me: Me,
    headers: HeaderMap,
) -> ApiResult<StatusCode> {
    if let Some(token) = cookie(&headers, COOKIE) {
        let mut conn = app.db.acquire().await?;
        kelve_store::auth::end_session(&mut conn, &crypto::hash(&token)).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookies_are_read_by_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; kelve=abc; b=2"),
        );
        assert_eq!(cookie(&headers, "kelve").as_deref(), Some("abc"));
        assert_eq!(cookie(&headers, "missing"), None);
    }

    #[test]
    fn secrets_compare_whole() {
        assert!(constant_eq(b"abc", b"abc"));
        assert!(!constant_eq(b"abc", b"abd"));
        assert!(!constant_eq(b"abc", b"abcd"));
    }
}
