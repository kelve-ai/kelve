//! The agent loop (docs/skills.md): what kelve does with the person's agent
//! as it works, so what the person knows becomes skills their agents use.
//!
//! - As the agent hands work over, kelve decides whether it's worth a
//!   debrief, and asks for one at once, while the agent remembers why
//!   (`debrief`). One piece of work has one debrief till it's sent.
//! - From a debrief, the person can ask for research on a decision
//!   (`research`), or an explanation of an idea (`explain`).
//! - What the person corrects while the agent works, what they change after
//!   it hands over, and what they send from a debrief, become parts of
//!   their skills (`learn`), written into the files in place, and undone
//!   if they ask (`files`).
//! - What they change after a handover is seen however they commit it,
//!   lazily, as kelve next hears from the agent or the person (`after`).
//! - Skills reach the agent through the hook, each once a session
//!   (`deliver`), and through each agent's own skills folder (`links`).
//!
//! Nothing here leaves the person's computer.

pub mod after;
pub mod debrief;
pub mod deliver;
pub mod explain;
pub mod files;
pub mod learn;
pub mod links;
pub mod research;

use kelve_store::asks;
use serde_json::Value;

use crate::error::ApiError;
use crate::pipeline::Failed;
use crate::state::Shared;

/// A pipeline job's failure, from an API error.
pub(crate) fn fail(e: ApiError) -> Failed {
    Failed::Other(e.message)
}

/// The agent took one of the loop's jobs: what it's for is being written.
pub async fn taken(app: &Shared, row: &asks::Ask) {
    let payload: Value = serde_json::from_str(&row.resume_payload).unwrap_or_default();
    let Ok(mut conn) = app.db.acquire().await else {
        return;
    };
    let done = match row.kind.as_str() {
        kelve_core::instructions::DEBRIEF => match payload["debrief"].as_i64() {
            Some(id) => kelve_store::debriefs::writing(&mut conn, id).await,
            None => Ok(()),
        },
        kelve_core::instructions::RESEARCH => match payload["research"].as_i64() {
            Some(id) => kelve_store::debriefs::research_state(&mut conn, id, "running").await,
            None => Ok(()),
        },
        kelve_core::instructions::EXPLAIN => match payload["explanation"].as_i64() {
            Some(id) => kelve_store::debriefs::explain_state(&mut conn, id, "running").await,
            None => Ok(()),
        },
        _ => Ok(()),
    };
    if let Err(e) = done {
        tracing::warn!(error = %e, kind = %row.kind, "couldn't keep that the agent took a job");
    }
    // The debrief, research or explanation shows it's being written.
    if matches!(
        row.kind.as_str(),
        kelve_core::instructions::DEBRIEF
            | kelve_core::instructions::RESEARCH
            | kelve_core::instructions::EXPLAIN
    ) {
        app.publish(kelve_core::views::ServerEvent::Debriefs);
    }
}

/// The agent's answer to one of the loop's jobs wasn't in its shape: kept
/// for the person to see, and, once it's put aside, failed, so home offers
/// to ask again.
pub async fn wrong(app: &Shared, row: &asks::Ask, why: &str, put_aside: bool) {
    let payload: Value = serde_json::from_str(&row.resume_payload).unwrap_or_default();
    let Ok(mut conn) = app.db.acquire().await else {
        return;
    };
    let done = match row.kind.as_str() {
        kelve_core::instructions::DEBRIEF => match payload["debrief"].as_i64() {
            Some(id) => {
                let kept = kelve_store::debriefs::wrong(&mut conn, id, why).await;
                match (kept, put_aside) {
                    (Ok(_), true) => {
                        kelve_store::agent_loop::debrief_failed(&mut conn, id, why).await
                    }
                    (Ok(_), false) => {
                        // Back with the agent, to fix while it has it.
                        kelve_store::debriefs::writing(&mut conn, id).await
                    }
                    (Err(e), _) => Err(e),
                }
            }
            None => Ok(()),
        },
        kelve_core::instructions::RESEARCH if put_aside => match payload["research"].as_i64() {
            Some(id) => kelve_store::debriefs::research_state(&mut conn, id, "failed").await,
            None => Ok(()),
        },
        kelve_core::instructions::EXPLAIN if put_aside => match payload["explanation"].as_i64() {
            Some(id) => kelve_store::debriefs::explain_state(&mut conn, id, "failed").await,
            None => Ok(()),
        },
        _ => Ok(()),
    };
    if let Err(e) = done {
        tracing::warn!(error = %e, kind = %row.kind, "couldn't keep that an answer was wrong");
    }
    drop(conn);
    app.publish(kelve_core::views::ServerEvent::Debriefs);
    app.publish(kelve_core::views::ServerEvent::Asks);
}

/// The repository a folder is in, as kelve names roots, or the folder.
pub(crate) async fn repo_of(dir: &str) -> String {
    let at = std::path::PathBuf::from(dir);
    tokio::task::spawn_blocking(move || kelve_engine::git::root_of(&at))
        .await
        .ok()
        .flatten()
        .map_or_else(|| dir.to_string(), |r| r.display().to_string())
}
