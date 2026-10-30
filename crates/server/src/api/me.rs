//! Whether kelve can do anything yet, stopping it, and deleting everything.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use kelve_core::views::{Me as MeView, ServerEvent};

use crate::auth::{Me, Who};
use crate::error::ApiResult;
use crate::state::Shared;

/// How long ago an agent may have last asked kelve for something to be
/// among those settings show.
const LATELY_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub async fn me(State(app): State<Shared>, _me: Me) -> ApiResult<Json<MeView>> {
    let mut conn = app.db.acquire().await?;
    let seen = |a: kelve_store::asks::Agent| kelve_core::views::AgentSeen {
        name: a.name,
        model: a.model,
        plan: a.plan,
        last_seen_at: kelve_core::Timestamp(kelve_store::at(a.last_seen_at)),
    };
    let agent = kelve_store::asks::latest_agent(&mut conn).await?;
    let agents = kelve_store::asks::agents_since(&mut conn, kelve_store::now() - LATELY_MS).await?;
    Ok(Json(MeView {
        agent: agent.map(seen),
        agents: agents.into_iter().map(seen).collect(),
        found: found_agents(),
    }))
}

/// The coding agents installed here: each command kelve knows, found on
/// the PATH. Nothing is run.
fn found_agents() -> Vec<kelve_core::views::AgentKind> {
    use kelve_core::views::AgentKind;
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path).collect();
    let on_path = |name: &str| {
        dirs.iter().any(|d| {
            let f = d.join(name);
            f.is_file()
                || d.join(format!("{name}.exe")).is_file()
                || d.join(format!("{name}.cmd")).is_file()
        })
    };
    [
        (AgentKind::ClaudeCode, "claude"),
        (AgentKind::Codex, "codex"),
        (AgentKind::Cursor, "cursor-agent"),
        (AgentKind::Gemini, "gemini"),
    ]
    .into_iter()
    .filter(|(_, command)| on_path(command))
    .map(|(kind, _)| kind)
    .collect()
}

/// Stops kelve, as `kelve stop` asks: requests in flight finish first.
pub async fn stop(State(app): State<Shared>, _who: Who) -> StatusCode {
    tracing::info!("stopping, as asked");
    app.stop();
    StatusCode::ACCEPTED
}

/// Deletes everything kelve keeps: settings, the graph's key, and work in
/// hand. Notes are the person's own files, and stay. It can't be undone;
/// the settings page says so.
pub async fn everything(State(app): State<Shared>, _me: Me) -> ApiResult<StatusCode> {
    let mut tx = app.db.begin().await?;
    kelve_store::delete_everything(&mut tx).await?;
    tx.commit().await?;
    app.reload().await?;
    app.publish(ServerEvent::Settings);
    tracing::info!("deleted everything, as asked");
    Ok(StatusCode::NO_CONTENT)
}
