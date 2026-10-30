//! Ready for you: what the person asked for by name in something opened, an
//! idea's entry, once there's something to read, until they've seen it. Nothing
//! kelve does on its own account shows: not reading, not tracing.

use kelve_core::views::{ReadyItem, ServerEvent};

use crate::error::ApiResult;
use crate::reader::Open;
use crate::state::Shared;

const ENTRY: &str = "entry";

/// The person opened an idea with no entry yet, in what's opened.
pub async fn asked_entry(app: &Shared, opened: i64, key: &str) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::reader::ask(&mut conn, opened, ENTRY, key).await?;
    Ok(())
}

/// An entry shown with its words: seen, wherever it was asked for. When
/// it was waiting to be seen, the reader hears, so it leaves the bar then,
/// not with whatever changes next.
pub async fn seen_entry(app: &Shared, key: &str) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    if kelve_store::reader::seen(&mut conn, ENTRY, key).await? {
        app.publish(ServerEvent::Asks);
    }
    Ok(())
}

/// How long an entry asked for counts as newly written, for search.
const LATELY_MS: i64 = 24 * 60 * 60 * 1000;

/// What's ready for the person in what's opened, the latest asked first.
pub async fn list(app: &Shared, open: &Open) -> ApiResult<Vec<ReadyItem>> {
    let asked = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::asked(&mut conn, open.row.id).await?
    };
    readable(app, asked).await
}

/// What the person asked for by name in what's opened in the last day and
/// is written now, seen or not, the latest asked first: search lists it as
/// newly written, so an entry read once is easy to find again.
pub async fn written(app: &Shared, open: &Open) -> ApiResult<Vec<ReadyItem>> {
    let asked = {
        let mut conn = app.db.acquire().await?;
        kelve_store::reader::asked_since(&mut conn, open.row.id, kelve_store::now() - LATELY_MS)
            .await?
    };
    readable(app, asked).await
}

/// Of what was asked for, the entries there's something to read of.
async fn readable(app: &Shared, asked: Vec<(String, String)>) -> ApiResult<Vec<ReadyItem>> {
    let mut ready = Vec::new();
    for (kind, target) in asked {
        if kind != ENTRY {
            continue;
        }
        let mut conn = app.db.acquire().await?;
        let readable = kelve_store::entries::get(&mut conn, &target)
            .await?
            .is_some_and(|e| matches!(e.state.as_str(), "ready" | "checking"));
        if !readable {
            continue;
        }
        let name = kelve_store::entries::idea(&mut conn, &target)
            .await?
            .map(|i| i.name)
            .unwrap_or_default();
        ready.push(ReadyItem::Entry { key: target, name });
    }
    Ok(ready)
}
