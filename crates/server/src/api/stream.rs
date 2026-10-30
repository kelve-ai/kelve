//! Server-sent events: something changed, so the web app fetches it again;
//! and, first, what was shown the person lately, so a tab coming back late
//! moves to it, and the page's build, so a tab on an older one reloads.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::Stream;
use futures::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use kelve_core::views::ServerEvent;

use crate::auth::Me;
use crate::state::Shared;

pub async fn stream(
    State(app): State<Shared>,
    _me: Me,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stopping = app.clone();
    // First, the page's build, which a tab on another one reloads for.
    let first = futures::stream::iter([Ok(ServerEvent::Page {
        build: crate::web::build().to_string(),
    })]);
    let events = first
        .chain(BroadcastStream::new(app.events.subscribe()))
        .filter_map(|event| async move {
            let event = event.ok()?;
            serde_json::to_string(&event)
                .ok()
                .map(|data| Ok(Event::default().data(data)))
        })
        // Ends when kelve stops, so open tabs don't hold it up.
        .take_until(async move { stopping.stopped().await });
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}
