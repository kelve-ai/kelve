//! The agent loop's routes (`agent_loop`): the person's skills for their
//! agent, by the `skills` tool, in case the hook didn't run; and, for the
//! person, asking for a debrief anyway or again, research on a decision, an
//! explanation of an idea, and undoing a change kelve made to a skill.

use axum::Json;
use axum::extract::{Path as UrlPath, Query, State};
use kelve_core::skills::SkillKind;
use serde::{Deserialize, Serialize};

use crate::agent_loop::{self, files};
use crate::auth::{Agent, Me};
use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

#[derive(Debug, Deserialize)]
pub struct SkillsAt {
    /// The folder the agent works in: its project's skills come too.
    #[serde(default)]
    path: Option<String>,
    /// One skill, by its name.
    #[serde(default)]
    name: Option<String>,
}

/// A skill, as the agent is told of it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFor {
    pub name: String,
    /// `like`, `do` or `project`.
    pub kind: &'static str,
    pub description: String,
    /// Where its `SKILL.md` is, as the person would write it.
    pub file: String,
    /// The whole skill, when one was asked for by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub markdown: Option<String>,
}

fn kind_word(kind: SkillKind) -> &'static str {
    match kind {
        SkillKind::Like => "like",
        SkillKind::Do => "do",
        SkillKind::Project => "project",
    }
}

/// The person's skills, and the project's the agent works in: each by name,
/// or one whole, with its secrets stripped.
pub async fn skills(
    State(app): State<Shared>,
    _agent: Agent,
    Query(at): Query<SkillsAt>,
) -> ApiResult<Json<Vec<SkillFor>>> {
    let root = match at.path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => Some(agent_loop::repo_of(p).await),
        None => None,
    };
    let all = files::all(&app, root.as_deref());
    let wanted = at.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    let found: Vec<SkillFor> = all
        .iter()
        // A project's skill of the same name goes first: it's the more particular.
        .rev()
        .filter(|s| wanted.is_none_or(|n| n == s.name))
        .map(|s| SkillFor {
            name: s.name.clone(),
            kind: kind_word(s.kind()),
            description: kelve_core::redact::redact(&s.description()),
            file: crate::reader::shown(&s.path),
            markdown: wanted.map(|_| kelve_core::redact::redact(&s.text)),
        })
        .take(if wanted.is_some() { 1 } else { usize::MAX })
        .collect();
    if let Some(name) = wanted
        && found.is_empty()
    {
        return Err(ApiError::not_found(format!(
            "The person has no skill called “{name}”. Call skills with no name to see theirs."
        )));
    }
    let mut found = found;
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(found))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asked {
    pub id: i64,
}

/// Debrief it: a debrief of work kelve didn't think worth one, asked for
/// now.
pub async fn debrief_anyway(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(handover): UrlPath<i64>,
) -> ApiResult<Json<Asked>> {
    let id = agent_loop::debrief::anyway(&app, handover).await?;
    Ok(Json(Asked { id }))
}

/// Ask it again: a debrief that failed its checks, asked for afresh.
pub async fn debrief_again(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(debrief): UrlPath<i64>,
) -> ApiResult<Json<Asked>> {
    agent_loop::debrief::again(&app, debrief).await?;
    Ok(Json(Asked { id: debrief }))
}

#[derive(Debug, Deserialize)]
pub struct Question {
    question: String,
}

/// Look into it: research on a decision, with the person's question.
pub async fn research(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(decision): UrlPath<i64>,
    Json(q): Json<Question>,
) -> ApiResult<Json<Asked>> {
    let id = agent_loop::research::ask_for(&app, decision, &q.question).await?;
    Ok(Json(Asked { id }))
}

#[derive(Debug, Deserialize)]
pub struct IdeaIn {
    idea: String,
}

/// Explain: an idea a debrief's work rests on, explained from its code.
pub async fn explain(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(debrief): UrlPath<i64>,
    Json(i): Json<IdeaIn>,
) -> ApiResult<Json<Asked>> {
    let id = agent_loop::explain::ask_for(&app, debrief, &i.idea).await?;
    Ok(Json(Asked { id }))
}

/// Undo: a change kelve made to a skill, taken back.
pub async fn undo(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(edit): UrlPath<i64>,
) -> ApiResult<Json<Asked>> {
    files::undo(&app, edit).await?;
    Ok(Json(Asked { id: edit }))
}
