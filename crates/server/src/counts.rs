//! The weekly count of how kelve was used, which the person may choose to share
//! with the shared graph. kelve counts a few things a day, here, whether or not
//! it's shared: what was opened, by kind; entries read, and how many came from
//! the shared graph; links followed, and how many led to an entry already
//! written; notes written, by where they're kept; jobs the agent did; files
//! read plainly; how lessons went; and the day an agent first said hello. Only
//! numbers: never what was opened, read or written, and nothing that says who.
//! What goes with them is the install's random ID, made when the person says
//! yes and deleted when they stop, so kelves can be counted and seen to come
//! back; never with the graph's key or a reading token. Nothing goes until the
//! person says yes, or ever with `DO_NOT_TRACK` or `KELVE_TELEMETRY=off`; the
//! settings page shows exactly what would go, and each week sent is kept, as
//! sent, for them to read.

use chrono::{Datelike, Duration, NaiveDate};
use kelve_core::views::{CountSent, CountsView};
use kelve_graph_api::Counts;

use crate::error::ApiResult;
use crate::state::Shared;

/// Everything kelve counts. Nothing else goes in a report.
pub const KINDS: &[&str] = &[
    "opened.folder",
    "opened.file",
    "opened.change",
    "entries.read",
    "links.followed",
    "links.written",
    "notes.you",
    "notes.work",
    "notes.agent",
    "entries.graph",
    "jobs.done",
    "files.plainly",
    "lessons.asked",
    "lessons.written",
    "lessons.given",
    "lessons.held",
    "lessons.didnt",
    "agent.connected",
];

/// How many links in a day make a deep one.
const DEEP: i64 = 3;

/// How often kelve looks for a week to send.
const EVERY: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// Counts one more of `what` today. Counting never stops what's counted:
/// a failure is logged, and that's all.
pub async fn count(app: &Shared, what: &str) {
    let day = app.config.today().to_string();
    let counted = async {
        let mut conn = app.db.acquire().await?;
        kelve_store::counts::add(&mut conn, &day, what).await
    };
    if let Err(e) = counted.await {
        tracing::warn!(error = %e, what, "couldn't count");
    }
}

/// Counts one more of `what` today, on a connection already held.
pub async fn count_on(app: &Shared, conn: &mut kelve_store::Conn, what: &str) {
    let day = app.config.today().to_string();
    if let Err(e) = kelve_store::counts::add(conn, &day, what).await {
        tracing::warn!(error = %e, what, "couldn't count");
    }
}

/// Counts `what` for `key` once today, on a connection already held.
pub async fn count_once_on(app: &Shared, conn: &mut kelve_store::Conn, what: &str, key: &str) {
    let day = app.config.today().to_string();
    if let Err(e) = kelve_store::counts::add_once(conn, &day, what, key).await {
        tracing::warn!(error = %e, what, "couldn't count");
    }
}

/// Counts `what` for `key` once today: whether it was counted now.
pub async fn count_once(app: &Shared, what: &str, key: &str) -> bool {
    let day = app.config.today().to_string();
    let counted = async {
        let mut conn = app.db.acquire().await?;
        kelve_store::counts::add_once(&mut conn, &day, what, key).await
    };
    counted.await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, what, "couldn't count");
        false
    })
}

/// The Monday of the week a day is in.
pub fn monday(day: NaiveDate) -> NaiveDate {
    day - Duration::days(i64::from(day.weekday().num_days_from_monday()))
}

/// The week that began on `week`, counted.
pub async fn report(app: &Shared, week: NaiveDate) -> ApiResult<Counts> {
    let mut conn = app.db.acquire().await?;
    let last = week + Duration::days(6);
    let rows =
        kelve_store::counts::between(&mut conn, &week.to_string(), &last.to_string()).await?;
    let first = kelve_store::counts::first_day(&mut conn)
        .await?
        .and_then(|d| d.parse::<NaiveDate>().ok());
    let mut deep = std::collections::BTreeSet::new();
    let mut counts = std::collections::BTreeMap::new();
    let mut by_day: std::collections::BTreeMap<String, std::collections::BTreeMap<String, u32>> =
        std::collections::BTreeMap::new();
    for (day, what, n) in rows {
        if !KINDS.contains(&what.as_str()) {
            continue;
        }
        if what == "links.followed" && n >= DEEP {
            deep.insert(day.clone());
        }
        let n = u32::try_from(n).unwrap_or(u32::MAX);
        *counts.entry(what.clone()).or_insert(0u32) += n;
        *by_day.entry(day).or_default().entry(what).or_insert(0) += n;
    }
    let weeks_in = first.map_or(0, |f| {
        u32::try_from((week - monday(f)).num_days().max(0) / 7).unwrap_or(0)
    });
    Ok(Counts {
        week: week.to_string(),
        install: app.settings().install.clone().unwrap_or_default(),
        agent: kelve_graph_api::client::AGENT.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        platform: platform().into(),
        weeks_in,
        days: u8::try_from(by_day.len()).unwrap_or(7),
        deep_days: u8::try_from(deep.len()).unwrap_or(7),
        contributing: app.settings().contribute,
        counts,
        by_day,
    })
}

fn platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        "linux" => "linux",
        _ => "other",
    }
}

fn written(counts: &Counts) -> String {
    serde_json::to_string_pretty(counts).unwrap_or_default()
}

/// What the settings page shows: this week so far, and the weeks sent.
pub async fn view(app: &Shared) -> ApiResult<CountsView> {
    let this_week = report(app, monday(app.config.today())).await?;
    let mut conn = app.db.acquire().await?;
    let sent = kelve_store::counts::all_sent(&mut conn, 12)
        .await?
        .into_iter()
        .map(|(week, report)| CountSent { week, report })
        .collect();
    Ok(CountsView {
        this_week: written(&this_week),
        sent,
    })
}

/// Sends last week's count, if the person shares it, it hasn't gone, and
/// kelve was used that week: the week sent.
pub async fn send_due(app: &Shared) -> ApiResult<Option<String>> {
    let today = app.config.today();
    {
        let mut conn = app.db.acquire().await?;
        let fortnight = (today - Duration::days(14)).to_string();
        kelve_store::counts::forget_keys(&mut conn, &fortnight).await?;
    }
    // Only once the person has joined the graph, and never with their key.
    let (Some(graph), true) = (app.graph.clone(), app.graph_reader().is_some()) else {
        return Ok(None);
    };
    let settings = app.settings();
    if !settings.share_counts || settings.install.is_none() || app.config.counts_off.is_some() {
        return Ok(None);
    }
    let week = monday(today) - Duration::days(7);
    {
        let mut conn = app.db.acquire().await?;
        if kelve_store::counts::sent(&mut conn, &week.to_string())
            .await?
            .is_some()
        {
            return Ok(None);
        }
    }
    let counts = report(app, week).await?;
    if counts.days == 0 {
        return Ok(None);
    }
    if let Err(e) = graph.send_counts(&counts).await {
        tracing::info!(error = %e, "the week's count didn't go; kelve will try again later");
        return Ok(None);
    }
    let mut conn = app.db.acquire().await?;
    kelve_store::counts::put_sent(&mut conn, &counts.week, &written(&counts)).await?;
    tracing::info!("sent the week's count, as the person chose");
    Ok(Some(counts.week))
}

/// Looks for a week to send now and then, for as long as kelve runs.
pub fn start(app: &Shared) {
    let app = app.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = send_due(&app).await {
                tracing::warn!(error = %e.message, "couldn't look for a week's count to send");
            }
            tokio::select! {
                () = tokio::time::sleep(EVERY) => {},
                () = app.stopped() => return,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_week_starts_on_its_monday() {
        let day = |s: &str| s.parse::<NaiveDate>().ok();
        assert_eq!(
            Some(monday(day("2026-10-15").unwrap_or_default())),
            day("2026-10-12")
        );
        assert_eq!(
            Some(monday(day("2026-10-12").unwrap_or_default())),
            day("2026-10-12")
        );
        assert_eq!(
            Some(monday(day("2026-10-18").unwrap_or_default())),
            day("2026-10-12")
        );
    }
}
