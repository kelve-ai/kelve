//! The person's settings: how things look, the ideas they know, and the
//! weekly count they may share.

use axum::Json;
use axum::extract::State;
use kelve_core::views::{CountsView, ServerEvent, Settings, SettingsIn};
use serde::Deserialize;

use crate::auth::Me;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

pub async fn get(State(app): State<Shared>, _me: Me) -> ApiResult<Json<Settings>> {
    Ok(Json(view(&app)))
}

/// The settings as the page shows them.
fn view(app: &Shared) -> Settings {
    let mut view = app.settings().view();
    view.hook = hook_installed(app);
    view.hooks_elsewhere = hooks_elsewhere(app);
    view.counts_off = app.config.counts_off.clone();
    view
}

/// The agents other than the person's whose settings have kelve's hook, as
/// people name them: so a hook put in for another agent is no surprise.
fn hooks_elsewhere(app: &Shared) -> Vec<String> {
    let mine = hook_agent(app);
    kelve_core::hook::Agent::ALL
        .into_iter()
        .filter(|a| Some(*a) != mine)
        .filter(|a| crate::hooks::installed(*a, &app.config.agents_home, &app.config.claude_dir))
        .map(|a| a.shown().to_string())
        .collect()
}

/// The hooks of the agent the person uses, when it has them.
fn hook_agent(app: &Shared) -> Option<kelve_core::hook::Agent> {
    use kelve_core::hook::Agent;
    use kelve_core::views::AgentKind;
    match app.settings().agent {
        AgentKind::ClaudeCode => Some(Agent::Claude),
        AgentKind::Codex => Some(Agent::Codex),
        AgentKind::Cursor => Some(Agent::Cursor),
        AgentKind::Gemini => Some(Agent::Gemini),
        AgentKind::Other => None,
    }
}

fn hook_installed(app: &Shared) -> bool {
    hook_agent(app).is_some_and(|a| {
        crate::hooks::installed(a, &app.config.agents_home, &app.config.claude_dir)
    })
}

/// Adds kelve's hook to the person's agent's settings, or takes it out, leaving
/// everything else in the file as it was.
fn set_hook(app: &Shared, on: bool) -> ApiResult<()> {
    let Some(agent) = hook_agent(app) else {
        return Err(ApiError::bad_request(
            "kelve has no hook for that agent. It reads your skills as kelve's instructions ask.",
        ));
    };
    crate::hooks::set(agent, on, &app.config.agents_home, &app.config.claude_dir)
        .map(|_| ())
        .map_err(|e| {
            tracing::warn!(agent = agent.name(), "couldn't change an agent's hooks");
            ApiError::bad_request(e)
        })
}

pub async fn post(
    State(app): State<Shared>,
    _me: Me,
    Json(change): Json<SettingsIn>,
) -> ApiResult<Json<Settings>> {
    if let Some(on) = change.hook {
        set_hook(&app, on)?;
    }
    let mut settings = (*app.settings()).clone();
    settings.apply(change).map_err(ApiError::bad_request)?;
    app.save_settings(settings).await?;
    app.publish(ServerEvent::Settings);
    Ok(Json(view(&app)))
}

/// The weekly count: this week's so far, exactly as it would go, and the
/// weeks sent.
pub async fn counts(State(app): State<Shared>, _me: Me) -> ApiResult<Json<CountsView>> {
    Ok(Json(crate::counts::view(&app).await?))
}

/// The ideas the person said they know, from a debrief.
pub async fn known(State(app): State<Shared>, _me: Me) -> ApiResult<Json<Vec<String>>> {
    Ok(Json(crate::spec::read(&app).never))
}

#[derive(Debug, Deserialize)]
pub struct Known {
    idea: String,
}

/// Takes back "I know this": the idea is new to them again.
pub async fn forget_known(
    State(app): State<Shared>,
    _me: Me,
    Json(known): Json<Known>,
) -> ApiResult<Json<Vec<String>>> {
    let mut spec = crate::spec::read(&app);
    spec.never.retain(|i| *i != known.idea);
    let spec = crate::spec::save(&app, spec)?;
    app.publish(ServerEvent::Spec);
    app.publish(ServerEvent::Debriefs);
    Ok(Json(spec.never))
}
