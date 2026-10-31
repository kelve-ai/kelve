//! Signing in to the shared graph, which adding to it needs. The graph
//! signs people in with GitHub, as an app on someone's own computer should
//! (RFC 8252): kelve listens on a port of its own, for one sign-in, and
//! hands the person's browser an address at the graph with a PKCE
//! challenge. The graph sends them to GitHub and back, then here with a
//! one-time code, which kelve exchanges, with the verifier only it holds,
//! for a key of its own. The key is sealed with the install's key, like
//! every key kelve keeps.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use kelve_core::views::{GraphAccount, GraphSignIn, ServerEvent};
use kelve_graph_api::{SignInCode, pkce_challenge};
use serde::Deserialize;
use tokio::sync::oneshot;

use crate::auth::Me;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

/// How long kelve waits for the browser to come back.
const WAIT: Duration = Duration::from_secs(10 * 60);

pub async fn account(State(app): State<Shared>, _me: Me) -> Json<GraphAccount> {
    Json(view(&app).await)
}

/// How long kelve waits to ask the graph whether the person agreed.
const ASK: Duration = Duration::from_secs(3);

/// The version of the terms the graph serves now, or the one this kelve was
/// built with when it can't be asked. The graph decides which terms stand,
/// so a kelve built before or after they changed agrees to the graph's.
pub(crate) async fn terms_now(app: &Shared) -> String {
    let served = match &app.graph {
        Some(graph) => tokio::time::timeout(ASK, graph.terms())
            .await
            .ok()
            .and_then(Result::ok),
        None => None,
    };
    served.map_or_else(|| kelve_core::terms::VERSION.into(), |t| t.version)
}

async fn view(app: &Shared) -> GraphAccount {
    let held = app.graph_key();
    // Whether they agreed to the terms as they are now, as the graph says.
    let agreed = match app.graph_writer() {
        Some(graph) => {
            match tokio::join!(tokio::time::timeout(ASK, graph.account()), terms_now(app)) {
                (Ok(Ok(account)), now) => Some(account.agreed.as_deref() == Some(now.as_str())),
                _ => None,
            }
        }
        None => None,
    };
    GraphAccount {
        agreed,
        graph: app.graph.is_some(),
        from: held.as_ref().map(|(from, _, _)| *from),
        handle: held.and_then(|(_, _, handle)| handle),
        waiting: app
            .signing_in
            .lock()
            .ok()
            .and_then(|w| w.as_ref().map(|t| !t.is_finished()))
            .unwrap_or(false),
    }
}

/// The terms for adding to the shared graph, as the graph serves them, or
/// as this kelve was built with when it can't be asked.
pub async fn terms(State(app): State<Shared>, _me: Me) -> Json<kelve_core::views::Terms> {
    let served = match &app.graph {
        Some(graph) => tokio::time::timeout(ASK, graph.terms())
            .await
            .ok()
            .and_then(Result::ok),
        None => None,
    };
    Json(match served {
        Some(t) => kelve_core::views::Terms {
            version: t.version,
            points: t.points,
            text: t.text,
        },
        None => kelve_core::views::Terms {
            version: kelve_core::terms::VERSION.into(),
            points: kelve_core::terms::POINTS
                .iter()
                .map(|p| p.to_string())
                .collect(),
            text: kelve_core::terms::text(),
        },
    })
}

#[derive(Debug, Deserialize)]
pub struct AgreeIn {
    version: String,
}

/// The person agrees to the terms for adding to the shared graph, as the
/// version they read: when the terms changed since they signed in.
pub async fn agree(
    State(app): State<Shared>,
    _me: Me,
    Json(agree): Json<AgreeIn>,
) -> ApiResult<Json<GraphAccount>> {
    agree_to(&app, &agree.version).await?;
    app.publish(ServerEvent::Settings);
    Ok(Json(view(&app).await))
}

/// Tells the graph the person agreed to a version of the terms, then sends
/// what the agent wrote before, kept here until now, as it would have gone.
async fn agree_to(app: &Shared, version: &str) -> ApiResult<()> {
    let graph = app.graph_writer().ok_or_else(|| {
        ApiError::bad_request("Sign in to the shared graph first, then agree to its terms.")
    })?;
    match graph.agree(version).await {
        Ok(_) => {}
        Err(kelve_graph_api::client::Error::Refused { said, .. }) => {
            return Err(ApiError::bad_request(said));
        }
        Err(kelve_graph_api::client::Error::SignIn) => {
            return Err(ApiError::bad_request(
                "The shared graph doesn’t know this kelve any more. Sign in to it again.",
            ));
        }
        Err(e) => {
            tracing::info!(error = %e, "couldn't tell the shared graph the person agreed");
            return Err(ApiError::bad_request(
                "kelve couldn’t reach the shared graph. Try again in a moment.",
            ));
        }
    }
    tracing::info!("agreed to the shared graph's terms");
    match crate::pipeline::entries::send_kept(app).await {
        Ok(n) if n > 0 => tracing::info!(entries = n, "sending what was kept before agreeing"),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "couldn't send what was kept before agreeing"),
    }
    Ok(())
}

/// What the account has added to the shared graph, and how many times
/// others read it.
pub async fn added(
    State(app): State<Shared>,
    _me: Me,
) -> ApiResult<Json<kelve_core::views::Added>> {
    let Some(graph) = app.graph_writer() else {
        return Err(ApiError::bad_request(
            "Sign in to the shared graph to see what you’ve added.",
        ));
    };
    let a = graph.added().await.map_err(|e| {
        tracing::info!(error = %e, "couldn't ask the shared graph what was added");
        ApiError::bad_request("kelve couldn’t reach the shared graph. Try again in a moment.")
    })?;
    Ok(Json(kelve_core::views::Added {
        entries: a.entries,
        relations: a.relations,
        reads: a.reads,
    }))
}

/// What was sent to the shared graph today, since its day began at
/// midnight UTC, and how the graph's check of each stands; and, once the
/// account has added as much as the graph takes a day, until when the
/// rest waits. Asks the graph nothing.
pub async fn today(
    State(app): State<Shared>,
    _me: Me,
) -> ApiResult<Json<kelve_core::views::SentToday>> {
    use kelve_core::views::{Quota, SendsHeld, SentToday};
    let midnight = chrono::Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .map(|t| t.and_utc())
        .unwrap_or_default();
    let mut conn = app.db.acquire().await?;
    let s = kelve_store::sends::since(&mut conn, kelve_store::ms(midnight)).await?;
    let held = app.sends_held().map(|h| SendsHeld {
        until: kelve_store::at(h.until).into(),
        what: match h.what.as_str() {
            "ideas" => Some(Quota::Ideas),
            "entries" => Some(Quota::Entries),
            "relations" => Some(Quota::Relations),
            _ => None,
        },
        new_account: h.new_account,
    });
    Ok(Json(SentToday {
        sent: s.sent,
        accepted: s.accepted,
        checking: s.checking,
        turned_away: s.turned_away,
        held,
    }))
}

/// How the person signs in to the graph: with GitHub, or with Google.
#[derive(Debug, Default, serde::Deserialize)]
pub struct SignInWith {
    #[serde(default)]
    provider: Option<String>,
}

/// Starts signing in: where to send the person's browser.
pub async fn sign_in(
    State(app): State<Shared>,
    _me: Me,
    with: Option<Json<SignInWith>>,
) -> ApiResult<Json<GraphSignIn>> {
    let provider = with.and_then(|Json(w)| w.provider);
    if let Some(p) = provider.as_deref()
        && !matches!(p, "github" | "google")
    {
        return Err(ApiError::bad_request("Sign in with GitHub or Google."));
    }
    let Some(graph) = app.graph.clone() else {
        return Err(ApiError::bad_request(
            "This kelve has no shared graph to sign in to.",
        ));
    };
    if app.config.env.graph.is_some() {
        return Err(ApiError::bad_request(
            "This kelve’s graph key is set where it runs (KELVE_GRAPH_KEY), and that one wins. Change it there.",
        ));
    }
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(ApiError::internal)?;
    let port = listener.local_addr().map_err(ApiError::internal)?.port();
    let redirect = format!("http://127.0.0.1:{port}/signed-in");
    let verifier = crate::crypto::token();
    let state = crate::crypto::token();
    let address = graph
        .sign_in_address(
            &redirect,
            &state,
            &pkce_challenge(&verifier),
            provider.as_deref(),
        )
        .map_err(ApiError::internal)?;
    let (done, finished) = oneshot::channel();
    let waiting = Arc::new(Waiting {
        app: app.clone(),
        graph,
        verifier,
        state,
        redirect,
        done: std::sync::Mutex::new(Some(done)),
    });
    let back = Router::new()
        .route("/signed-in", get(signed_in))
        .with_state(waiting);
    let told = app.clone();
    let task = tokio::spawn(async move {
        let serving = axum::serve(listener, back).with_graceful_shutdown(async move {
            let _ = finished.await;
        });
        if tokio::time::timeout(WAIT, serving).await.is_err() {
            tracing::info!("a sign-in to the shared graph ran out of time");
        }
        told.publish(ServerEvent::Settings);
    });
    // One sign-in at a time: a new one replaces any still waiting.
    if let Ok(mut w) = app.signing_in.lock()
        && let Some(old) = w.replace(task.abort_handle())
    {
        old.abort();
    }
    app.publish(ServerEvent::Settings);
    Ok(Json(GraphSignIn { address }))
}

/// Signs out: the graph forgets this kelve's key, and kelve forgets it too.
pub async fn sign_out(State(app): State<Shared>, _me: Me) -> ApiResult<Json<GraphAccount>> {
    if app.config.env.graph.is_some() {
        return Err(ApiError::bad_request(
            "This kelve’s graph key is set where it runs (KELVE_GRAPH_KEY). Remove it there.",
        ));
    }
    if let Some(writer) = app.graph_writer()
        && let Err(e) = writer.sign_out().await
    {
        // Forgotten here all the same: what's left at the graph is a key
        // nobody holds.
        tracing::warn!(error = %e, "the shared graph couldn't be told this kelve signed out");
    }
    app.hold_graph_key(None).await?;
    app.publish(ServerEvent::Settings);
    Ok(Json(view(&app).await))
}

/// Deletes the person's account on the shared graph, then forgets its key
/// here. If the graph can't be reached, nothing is forgotten, so they can
/// try again.
pub async fn delete_account(State(app): State<Shared>, _me: Me) -> ApiResult<Json<GraphAccount>> {
    if app.config.env.graph.is_some() {
        return Err(ApiError::bad_request(
            "This kelve’s graph key is set where it runs (KELVE_GRAPH_KEY). Delete that account from wherever the key came from.",
        ));
    }
    let Some(writer) = app.graph_writer() else {
        return Err(ApiError::bad_request(
            "You’re not signed in to the shared graph, so there’s no account to delete.",
        ));
    };
    match writer.delete_account().await {
        // Already gone at the graph: forgetting it here is all that's left.
        Ok(()) | Err(kelve_graph_api::client::Error::SignIn) => {}
        Err(e) => return Err(ApiError::bad_request(e.to_string())),
    }
    app.hold_graph_key(None).await?;
    tracing::info!("deleted the person's account on the shared graph, as asked");
    app.publish(ServerEvent::Settings);
    Ok(Json(view(&app).await))
}

/// What one sign-in waits with.
struct Waiting {
    app: Shared,
    graph: kelve_graph_api::client::Client,
    verifier: String,
    state: String,
    redirect: String,
    done: std::sync::Mutex<Option<oneshot::Sender<()>>>,
}

impl Waiting {
    fn finish(&self) {
        if let Some(done) = self.done.lock().ok().and_then(|mut d| d.take()) {
            let _ = done.send(());
        }
    }
}

#[derive(Debug, Deserialize)]
struct Back {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// The browser, back from the graph. Only the state kelve made, which only
/// this sign-in's address carried, is listened to: anything else is told
/// it isn't the sign-in kelve is waiting for.
async fn signed_in(State(w): State<Arc<Waiting>>, Query(back): Query<Back>) -> Response {
    if back.state.as_deref() != Some(w.state.as_str()) {
        return page(
            StatusCode::BAD_REQUEST,
            "This isn’t the sign-in kelve is waiting for. Sign in again from kelve’s settings.",
        );
    }
    let said = match (back.code, back.error) {
        (Some(code), _) => {
            let asked = SignInCode {
                code,
                code_verifier: w.verifier.clone(),
                redirect_uri: w.redirect.clone(),
            };
            match w.graph.exchange(&asked).await {
                Ok(signed) => match w.app.hold_graph_key(Some((&signed.key, &signed.handle))).await
                {
                    Ok(()) => {
                        tracing::info!("signed in to the shared graph");
                        // Signing in is agreeing to the terms as they are
                        // now, as the sign-in buttons say (Nathan, 29 October):
                        // the graph's, which the buttons lead to.
                        let now = terms_now(&w.app).await;
                        if let Err(e) = agree_to(&w.app, &now).await {
                            tracing::warn!(error = %e.message, "couldn't agree to the terms on signing in");
                        }
                        Ok(format!(
                            "You’re signed in to kelve’s shared graph as {}. You can close this tab.",
                            signed.handle
                        ))
                    }
                    Err(e) => {
                        tracing::error!(error = %e.message, "couldn't keep the shared graph's key");
                        Err("kelve couldn’t keep the key the shared graph gave it. Sign in again from kelve’s settings.".to_string())
                    }
                },
                Err(e) => Err(format!("{e} Sign in again from kelve’s settings.")),
            }
        }
        (None, Some(_)) => Err(
            "You didn’t let GitHub say who you are, so kelve isn’t signed in to the shared graph. You can close this tab."
                .to_string(),
        ),
        (None, None) => Err(
            "The shared graph sent no code back. Sign in again from kelve’s settings.".to_string(),
        ),
    };
    w.finish();
    match said {
        Ok(said) => page(StatusCode::OK, &said),
        Err(said) => page(StatusCode::BAD_REQUEST, &said),
    }
}

/// A sentence for the browser tab the sign-in ended in.
fn page(status: StatusCode, said: &str) -> Response {
    let said = said
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
            ),
        ],
        format!(
            "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\"><title>kelve</title><body style=\"font:16px/1.5 system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem\"><p>{said}</p>"
        ),
    )
        .into_response()
}
