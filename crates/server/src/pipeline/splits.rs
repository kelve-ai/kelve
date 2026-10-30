//! An idea the shared graph found to be two ("normalization" for text, and for
//! probabilities). The graph answers the old ID with the two it is now. kelve
//! keeps the two, so the reader can offer them in its place, and forgets the
//! marks that led to the old one, so each passage is marked again when its file
//! next opens, and the check chooses between the two by what the passage says.
//! Nothing is written for the old idea again.

use kelve_core::ideas::{IdeaRef, is_id};
use kelve_store::entries::Idea;

use super::Result;
use crate::state::Shared;

/// The two an idea was found to be, as kept, if it was split.
pub async fn of(app: &Shared, key: &str) -> Result<Vec<IdeaRef>> {
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::entries::split_into(&mut conn, key)
        .await?
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default())
}

/// Asks the shared graph whether an idea kelve knows by a graph ID was
/// split, and keeps what it says: whether it was.
pub async fn ask(app: &Shared, idea: &Idea) -> Result<bool> {
    let id = if is_id(&idea.key) {
        idea.key.clone()
    } else if let Some(id) = &idea.became {
        id.clone()
    } else {
        return Ok(false);
    };
    let Some(graph) = app.graph_reader() else {
        return Ok(false);
    };
    match graph.ideas(std::slice::from_ref(&id)).await {
        Ok(got) => match got.split.get(&id) {
            Some(into) => {
                found(app, &idea.key, &id, into).await?;
                Ok(true)
            }
            None => Ok(false),
        },
        Err(e) => {
            tracing::info!(error = %e, "couldn't ask the shared graph about an idea");
            Ok(false)
        }
    }
}

/// Keeps that an idea, known here by `key` and in the graph by `id`, is
/// now the ideas `into`: each kept to open, the marks that led to the old
/// one forgotten, and what's kept of how it relates let go.
pub async fn found(app: &Shared, key: &str, id: &str, into: &[String]) -> Result<()> {
    let ideas = match app.graph_reader() {
        Some(graph) => graph.ideas(into).await.map(|g| g.ideas).unwrap_or_default(),
        None => Vec::new(),
    };
    let two: Vec<IdeaRef> = into
        .iter()
        .map(|i| IdeaRef {
            id: i.clone(),
            name: ideas
                .iter()
                .find(|n| &n.id == i)
                .map_or_else(|| i.clone(), |n| n.name.clone()),
        })
        .collect();
    let mut conn = app.db.acquire().await?;
    for idea in &ideas {
        kelve_store::entries::keep_idea(
            &mut conn,
            &Idea {
                key: idea.id.clone(),
                name: idea.name.clone(),
                definition: idea.definition.clone(),
                kind: idea.kind.as_str().to_string(),
                became: None,
            },
        )
        .await?;
    }
    kelve_store::entries::split(
        &mut conn,
        key,
        &serde_json::to_string(&two).unwrap_or_default(),
    )
    .await?;
    let forgotten = kelve_store::marks::forget_idea(&mut conn, id).await?;
    kelve_store::relations::forget(&mut conn, key).await?;
    drop(conn);
    tracing::info!(
        passages = forgotten,
        "an idea was found to be two; its marks are made again"
    );
    app.publish(kelve_core::views::ServerEvent::Idea {
        key: key.to_string(),
    });
    Ok(())
}
