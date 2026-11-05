//! The names of v2's background jobs, in one place, so the parts of kelve that
//! queue them and the parts that do them agree. Each is a pipeline job (the
//! `jobs` table): queue it with [`queue`], keyed so the same thing is never
//! queued twice. The job a pipeline job asks of the person's agent is named in
//! `kelve_core::instructions` (`debrief`, `learn`, `research`, `explain`,
//! `compare`, `draw`).
//!
//! Who does each (the module that handles it in `pipeline::dispatch`):
//!
//! | Job | Key | Done by |
//! |---|---|---|
//! | [`DEBRIEF`] | the handover's id | the agent loop: asks the `debrief` job, then keeps the answer |
//! | [`RESEARCH`] | the decision's id | the agent loop: asks `research`, keeps the answer |
//! | [`EXPLAIN`] | the explanation's id | the agent loop: asks `explain`, keeps the answer |
//! | [`LEARN`] | `debrief:<id>` when one is sent, `message:<id>` or `edit:<handover>` for a correction | the agent loop: asks `learn`, turns the answer into candidates |
//! | [`CHECK`] | the check's id | skills: asks `compare`, keeps the verdict |
//! | [`DRAW`] | a fresh id per ask | your agent: asks `draw`, keeps the part or character |

use crate::pipeline;
use crate::state::Shared;
use chrono::Utc;
use serde_json::Value;

/// Ask the agent for a debrief of a handover.
pub const DEBRIEF: &str = "v2.debrief";
/// Ask the agent to look into a decision.
pub const RESEARCH: &str = "v2.research";
/// Ask the agent to explain an idea in a debrief.
pub const EXPLAIN: &str = "v2.explain";
/// Ask the agent to turn what the person kept or corrected into parts of skills.
pub const LEARN: &str = "v2.learn";
/// Ask the agent to check a part of a skill on past work.
pub const CHECK: &str = "v2.check";
/// Ask the agent to draw a part or a character.
pub const DRAW: &str = "v2.draw";

/// Queues `kind` for `key` now, with `payload`. The same kind and key queued
/// again replaces the payload instead of adding a second job. Keys are kept
/// under the kind (`v2.research:12`), as the jobs table's keys are one
/// namespace: research on decision 12 and an explanation numbered 12 are
/// two jobs.
pub async fn queue(app: &Shared, kind: &str, key: &str, payload: Value) -> pipeline::Result<()> {
    pipeline::add(
        app,
        kind,
        Some(&format!("{kind}:{key}")),
        payload,
        Utc::now(),
    )
    .await
}
