//! The outline the Knowledge window draws: an idea with what it rests on, a row
//! a level, the open one followed down to the ground, and what's beside and
//! above it; and a folder, drawn the way an idea is. From kept relations and
//! the shared graph only: it never asks a model.

use axum::Json;
use axum::extract::{Path as UrlPath, Query, State};
use kelve_core::relations::is_foundation;
use kelve_core::views::{Outline, OutlineIdea, OutlineRow};
use kelve_graph_api::{Node, Rel};
use serde::Deserialize;

use crate::auth::Me;
use crate::error::{ApiError, ApiResult};
use crate::pipeline::relations::{self, Known, has_own};
use crate::state::Shared;

/// The most levels an outline goes down: past this, the trail in the
/// window's bar carries on.
const DEEPEST: usize = 8;
/// Which idea is open at each level, by key, from the top: any level not
/// said opens its most needed.
#[derive(Debug, Default, Deserialize)]
pub struct Along {
    #[serde(default)]
    path: String,
}

impl Along {
    fn keys(&self) -> Vec<&str> {
        self.path.split(',').filter(|k| !k.is_empty()).collect()
    }
}

/// An idea the outline draws, from a relation's other end.
fn drawn(node: &Node, why: Option<&str>, traced: bool) -> OutlineIdea {
    OutlineIdea {
        key: node.id.clone(),
        name: node.name.clone(),
        foundation: node.foundation || is_foundation(&node.name),
        traced: traced || node.foundation || node.depth.is_some() || is_foundation(&node.name),
        on: false,
        busy: false,
        why: why.map(str::to_string),
        files: None,
    }
}

fn from(rels: &[Rel]) -> Vec<OutlineIdea> {
    rels.iter()
        .map(|r| drawn(&r.idea, Some(&r.why), false))
        .collect()
}

/// The rows going down from what an idea rests on: at each level, the idea
/// the path names, or the most needed, followed down until a first
/// principle, or an idea whose relations aren't known.
async fn rows(app: &Shared, first: &[Rel], path: &[&str]) -> ApiResult<Vec<OutlineRow>> {
    let mut rows = Vec::new();
    let mut level: Vec<Rel> = first.to_vec();
    for depth in 0..DEEPEST {
        if level.is_empty() {
            break;
        }
        let known = relations::many(
            app,
            &level.iter().map(|r| r.idea.id.clone()).collect::<Vec<_>>(),
        )
        .await
        .map_err(ApiError::internal)?;
        let mut items: Vec<OutlineIdea> = level
            .iter()
            .map(|r| {
                // Known: what it rests on, or that it rests on nothing.
                let traced = known
                    .get(&r.idea.id)
                    .is_some_and(|k| !k.relations.down.is_empty() || k.proposed);
                drawn(&r.idea, Some(&r.why), traced)
            })
            .collect();
        {
            let mut conn = app.db.acquire().await.map_err(ApiError::internal)?;
            for i in items.iter_mut().filter(|i| !i.traced) {
                i.busy =
                    kelve_store::jobs::due(&mut conn, &format!("{}:{}", relations::RELATE, i.key))
                        .await
                        .map_err(ApiError::internal)?
                        .is_some();
            }
        }
        let open = path
            .get(depth)
            .and_then(|k| items.iter().position(|i| i.key == *k))
            .unwrap_or(0);
        items[open].on = true;
        let next = items[open].clone();
        rows.push(OutlineRow { items });
        if next.foundation {
            break;
        }
        level = known
            .get(&next.key)
            .map(|k| k.relations.down.clone())
            .unwrap_or_default();
    }
    Ok(rows)
}

/// The first principles at the end of the open path, for an idea whose
/// grounds the graph hasn't worked out.
fn ends(rows: &[OutlineRow]) -> Vec<OutlineIdea> {
    let mut out: Vec<OutlineIdea> = Vec::new();
    for row in rows {
        for i in row.items.iter().filter(|i| i.foundation) {
            if !out.iter().any(|o| o.key == i.key) {
                out.push(OutlineIdea {
                    why: None,
                    on: false,
                    ..i.clone()
                });
            }
        }
    }
    out
}

/// An idea's outline.
pub async fn outline(
    State(app): State<Shared>,
    _me: Me,
    UrlPath(key): UrlPath<String>,
    Query(along): Query<Along>,
) -> ApiResult<Json<Outline>> {
    let idea = {
        let mut conn = app.db.acquire().await.map_err(ApiError::internal)?;
        kelve_store::entries::idea(&mut conn, &key)
            .await
            .map_err(ApiError::internal)?
    }
    .ok_or_else(|| {
        ApiError::not_found("kelve doesn’t know that idea yet. Open it from a file or a link.")
    })?;
    let known: Option<Known> = relations::of(&app, &key)
        .await
        .map_err(ApiError::internal)?;
    let foundation = is_foundation(&idea.name);
    // Nothing known, and nothing proposed yet: proposed now. What's
    // proposed is kept, even when it's that the idea rests on nothing.
    let proposing = !foundation
        && known
            .as_ref()
            .is_none_or(|k| !has_own(&k.relations) && !k.proposed);
    if proposing {
        relations::ensure(&app, &idea)
            .await
            .map_err(ApiError::internal)?;
    }
    let k = known.clone().unwrap_or_default();
    let rows = rows(&app, &k.relations.down, &along.keys()).await?;
    // What it rests on, traced down to first principles as it's drawn.
    if !proposing
        && rows
            .iter()
            .flat_map(|r| &r.items)
            .any(|i| !i.traced && !i.foundation)
    {
        relations::trace(&app, &key)
            .await
            .map_err(ApiError::internal)?;
    }
    let grounds = if k.relations.grounds.is_empty() {
        ends(&rows)
    } else {
        k.relations
            .grounds
            .iter()
            .map(|n| drawn(n, None, true))
            .collect()
    };
    let shown = k.relations.up.first.len() as u32;
    Ok(Json(Outline {
        question: k.question.clone(),
        foundation,
        kind_of: from(&k.relations.kind_of),
        answers: from(&k.relations.answers),
        alternatives: from(&k.relations.alternatives),
        used_with: from(&k.relations.used_with),
        rows,
        above: from(&k.relations.up.first),
        above_more: k.relations.up.count.saturating_sub(shown),
        grounds,
        proposed: k.proposed,
        proposing,
        key,
        name: idea.name,
    }))
}
