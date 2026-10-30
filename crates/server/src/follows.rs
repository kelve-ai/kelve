//! Which ways down people take: opening an idea from a relation in another
//! idea's entry is kept, once ever for each relation, and sent to the shared
//! graph at most once an hour, in random order, without the account's key, with
//! nothing about who followed it. Opening an idea from the work never counts:
//! that would say which ideas the work uses.

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use kelve_core::relations::RelationKind;
use serde::Deserialize;

use crate::auth::Me;
use crate::error::ApiResult;
use crate::state::Shared;

/// How often kelve sends what was followed.
const EVERY: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Deserialize)]
pub struct FollowIn {
    from: String,
    kind: RelationKind,
    to: String,
}

/// The person opened an idea from a relation in another's entry.
pub async fn follow(
    State(app): State<Shared>,
    _me: Me,
    Json(f): Json<FollowIn>,
) -> ApiResult<Json<serde_json::Value>> {
    // Only the graph's own ideas: one of the person's own isn't the graph's to count.
    if kelve_core::ideas::is_id(&f.from) && kelve_core::ideas::is_id(&f.to) {
        let mut conn = app.db.acquire().await?;
        kelve_store::follows::follow(&mut conn, &f.from, f.kind.as_str(), &f.to).await?;
    }
    Ok(Json(serde_json::json!({})))
}

/// Sends what was followed and hasn't gone: in random order, without the
/// account's key. How many went.
pub async fn send_due(app: &Shared) -> kelve_store::Result<usize> {
    // Only once the person has joined the graph, and never with their key.
    let (Some(graph), true) = (app.graph.clone(), app.graph_reader().is_some()) else {
        return Ok(0);
    };
    let mut waiting = {
        let mut conn = app.db.acquire().await?;
        kelve_store::follows::unsent(&mut conn, kelve_graph_api::MOST_FOLLOWS as i64).await?
    };
    if waiting.is_empty() {
        return Ok(0);
    }
    // In random order: a hash seeded afresh each time.
    let order = std::collections::hash_map::RandomState::new();
    waiting.sort_by_key(|f| std::hash::BuildHasher::hash_one(&order, f));
    let follows = kelve_graph_api::Follows {
        follows: waiting
            .iter()
            .filter_map(|(from, kind, to)| {
                Some(kelve_graph_api::Followed {
                    from: from.clone(),
                    kind: RelationKind::parse(kind)?,
                    to: to.clone(),
                })
            })
            .collect(),
    };
    if let Err(e) = graph.send_follows(&follows).await {
        tracing::info!(error = %e, "the relations followed didn't go; kelve will try again later");
        return Ok(0);
    }
    let mut conn = app.db.acquire().await?;
    kelve_store::follows::sent(&mut conn, &waiting).await?;
    Ok(waiting.len())
}

/// Sends what was followed, once an hour, for as long as kelve runs.
pub fn start(app: &Shared) {
    let app = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                () = tokio::time::sleep(EVERY) => {},
                () = app.stopped() => return,
            }
            if let Err(e) = send_due(&app).await {
                tracing::warn!(error = %e, "couldn't send the relations followed");
            }
        }
    });
}
