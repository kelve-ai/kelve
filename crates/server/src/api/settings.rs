//! The person's settings: the daily ceiling, whether kelve adds to the
//! shared knowledge graph, and the weekly count they may share.

use axum::Json;
use axum::extract::State;
use kelve_core::spec::Spec;
use kelve_core::views::{CountsView, MarksView, ServerEvent, Settings, SettingsIn};

use crate::auth::Me;
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

pub async fn get(State(app): State<Shared>, _me: Me) -> ApiResult<Json<Settings>> {
    Ok(Json(view(&app)))
}

/// The settings as the page shows them: where notes kept only for you go,
/// kelve's own folder when none's chosen.
fn view(app: &Shared) -> Settings {
    let mut view = app.settings().view();
    view.notes_dir = crate::reader::shown(&crate::reader::own_notes(app));
    view.notes_hook = hook_installed(app);
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
            "kelve has no hook for that agent. It checks for notes, and asks for lessons, as kelve's instructions ask.",
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
    if let Some(on) = change.notes_hook {
        set_hook(&app, on)?;
    }
    let mut settings = (*app.settings()).clone();
    settings.apply(change).map_err(ApiError::bad_request)?;
    // Notes kept only for you go with you to a folder you choose.
    let (from, to) = (
        crate::reader::own_notes(&app),
        crate::reader::own_notes_in(&app, &settings),
    );
    if from != to {
        crate::reader::move_own_notes(&from, &to).map_err(|e| {
            tracing::warn!(error = %e, "couldn't move notes kept only for the person");
            ApiError::bad_request(format!(
                "kelve couldn't move your notes to {}: {e}. Check you can write there, and try again.",
                crate::reader::shown(&to)
            ))
        })?;
    }
    app.save_settings(settings).await?;
    app.publish(ServerEvent::Settings);
    app.publish(ServerEvent::Notes);
    Ok(Json(view(&app)))
}

/// The weekly count: this week's so far, exactly as it would go, and the
/// weeks sent.
pub async fn counts(State(app): State<Shared>, _me: Me) -> ApiResult<Json<CountsView>> {
    Ok(Json(crate::counts::view(&app).await?))
}

/// What kelve marks for the person, as settings and the reader show it.
pub fn marks_view(app: &Shared) -> MarksView {
    MarksView {
        spec: crate::spec::read(app),
        kept: crate::spec::exists(app),
        file: crate::reader::shown(&crate::spec::path(app)),
    }
}

pub async fn marks(State(app): State<Shared>, _me: Me) -> ApiResult<Json<MarksView>> {
    Ok(Json(marks_view(&app)))
}

/// Keeps what the person changed in what kelve marks for them.
pub async fn set_marks(
    State(app): State<Shared>,
    _me: Me,
    Json(spec): Json<Spec>,
) -> ApiResult<Json<MarksView>> {
    crate::spec::save(&app, crate::spec::checked(spec)?, "you")?;
    app.publish(ServerEvent::Spec);
    Ok(Json(marks_view(&app)))
}
