//! Your agent's list: what it's doing now, what's next for what you have
//! open, what you asked for in other things, and what's ready for you, from
//! the jobs kelve keeps for it. Each decision you send back from a debrief,
//! to have it another way, is a job like the rest, handed out by `next_job`,
//! and done when your agent says what it did.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use kelve_core::views::{AgentList, ListItem, ServerEvent};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::agent::{self, Priority};
use crate::error::{ApiError, ApiResult};
use crate::reader;
use crate::state::Shared;

/// The work a redo's answer runs: nothing more, since what the agent said
/// stays with its job, for the list.
pub const REDO_DONE: &str = "redo.done";

/// How lately an agent must have called kelve to be working.
const WORKING_MS: i64 = 2 * 60 * 1000;
/// How long an agent holding a job counts as working on it: as long as a
/// big file takes to read, while it calls kelve only to hand it in.
const HOLDING_MS: i64 = 10 * 60 * 1000;
/// How long a redo that's done stays in what's ready.
const DONE_FOR: chrono::Duration = chrono::Duration::hours(2);

/// A project's redos, as the keys of their jobs begin.
fn redos_of(root: &Path) -> String {
    let mut h = Sha256::new();
    h.update(root.display().to_string().as_bytes());
    let print: String = h
        .finalize()
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{}:{print}:", agent::REDO)
}

/// The project a folder is in, among those with something to redo: the
/// longest it's in. Else the folder itself.
async fn project_of(app: &Shared, folder: &Path) -> ApiResult<PathBuf> {
    let mut conn = app.db.acquire().await?;
    let roots = kelve_store::asks::roots_of(&mut conn, agent::REDO).await?;
    Ok(roots
        .iter()
        .map(PathBuf::from)
        // As Windows tells folders apart too: an agent's `C:\…` is the
        // `\\?\C:\…` kelve keeps.
        .filter(|root| reader::within(folder, root))
        .max_by_key(|root| root.as_os_str().len())
        .unwrap_or_else(|| folder.to_path_buf()))
}

/// What's to redo in the project a folder is in, as the keys of its jobs
/// begin.
pub async fn redos_in(app: &Shared, folder: &Path) -> ApiResult<String> {
    Ok(redos_of(&project_of(app, folder).await?))
}

/// A decision the person wants another way, from the debrief they just sent.
#[derive(Debug)]
pub struct Redo<'a> {
    pub debrief: i64,
    pub n: u32,
    /// What the decision chose.
    pub decision: &'a str,
    /// The debrief's title.
    pub title: &'a str,
    /// How they want it, in their own words: maybe nothing.
    pub how: &'a str,
    /// The file it's on, and its first and last lines.
    pub on: Option<(&'a str, u32, u32)>,
}

/// Keeps a job for the agent to redo a decision: it goes first, because it
/// changes the work, and only to an agent in that project.
pub async fn ask_redo(app: &Shared, open: &reader::Open, redo: &Redo<'_>) -> ApiResult<()> {
    let request = format!(
        "Redo decision {} of “{}”\n\nYou chose: {}\n\nDo it this way instead: {}",
        redo.n,
        redo.title,
        redo.decision,
        if redo.how.trim().is_empty() {
            "the person wants it another way. Ask them how."
        } else {
            redo.how.trim()
        }
    );
    let request = kelve_core::fence::fence("request", &kelve_core::redact::redact(&request));
    let lines_now = redo.on.and_then(|(file, a, b)| {
        let read = kelve_engine::read::read(&open.root, file).ok()?;
        let lines: Vec<&str> = read.text.lines().collect();
        let (a, b) = (a as usize, b as usize);
        (a >= 1 && a <= b && b <= lines.len()).then(|| lines[a - 1..b].join("\n"))
    });
    let material = json!({
        "request": request,
        "file": redo.on.map(|(file, _, _)| file),
        "lines": redo.on.map(|(_, a, b)| format!("{a}-{b}")),
        "linesThen": lines_now.map(|t| kelve_core::fence::fence("lines", &t)),
        "project": reader::shown(&open.root),
    });
    let key = format!("{}{}:{}", redos_of(&open.root), redo.debrief, redo.n);
    let on = redo.on.map(|(file, a, b)| {
        if a == b {
            format!("{file} {a}")
        } else {
            format!("{file} {a}–{b}")
        }
    });
    agent::ask(
        app,
        &key,
        agent::Ask {
            kind: agent::REDO,
            about: format!(
                "Redo in {}: {}",
                open.root
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default(),
                redo.decision
            ),
            idea: None,
            opened: Some(open.row.id),
            material,
            schema: redo_schema(),
            priority: Priority::Now,
            follows: None,
        },
        agent::Resume::job(
            REDO_DONE,
            key.clone(),
            json!({ "decision": redo.decision, "on": on }),
        ),
    )
    .await
    .map_err(|e| ApiError::internal(format!("couldn't keep a decision to redo: {e:?}")))?;
    Ok(())
}

fn redo_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "did": { "type": "string" } },
        "required": ["did"],
        "additionalProperties": false,
    })
}

/// Whether a redo's answer is in its shape.
pub fn redo_shape(value: &Value) -> Result<(), String> {
    let did = value["did"].as_str().unwrap_or_default().trim();
    if did.is_empty() || did.chars().count() > 300 {
        return Err("Say what you did in did: one line, at most 300 characters.".into());
    }
    Ok(())
}

/// How many decisions wait to be redone, in the project a folder is in, that
/// an agent's hook hasn't told it of: each is told of now.
pub async fn redos_new(app: &Shared, folder: &Path, agent_name: &str) -> ApiResult<usize> {
    let prefix = redos_in(app, folder).await?;
    let mut conn = app.db.acquire().await?;
    let mut new = 0;
    for ask in kelve_store::asks::open(&mut conn).await? {
        if ask.kind == agent::REDO
            && ask.key.starts_with(&prefix)
            && kelve_store::asks::tell(&mut conn, agent_name, ask.id).await?
        {
            new += 1;
        }
    }
    Ok(new)
}

/// Takes things off the list: the person doesn't want them done. Each job a row
/// stands for is put aside, and stays so when the work asks for it again, or
/// the same thing is opened again.
pub async fn take_off(app: &Shared, asks: &[i64]) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    for id in asks {
        kelve_store::asks::drop(&mut conn, *id).await?;
    }
    drop(conn);
    app.publish(ServerEvent::Asks);
    Ok(())
}

/// Moves a thing on the list to the front.
pub async fn first(app: &Shared, asks: &[i64]) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::asks::first(&mut conn, asks).await?;
    drop(conn);
    app.publish(ServerEvent::Asks);
    Ok(())
}

/// A redo's row on the list, with what's said under it.
fn row_of(ask: &kelve_store::asks::Ask, sub: Option<String>, asked_in: Option<String>) -> ListItem {
    let payload: Value = serde_json::from_str(&ask.resume_payload).unwrap_or(Value::Null);
    ListItem {
        key: ask.key.clone(),
        kind: agent::REDO.into(),
        title: payload["decision"]
            .as_str()
            .map_or_else(|| ask.about.clone(), str::to_string),
        sub: sub.or_else(|| payload["on"].as_str().map(|on| format!("On {on}"))),
        opened: ask.opened,
        asked_in,
        asks: if ask.state == "answered" {
            Vec::new()
        } else {
            vec![ask.id]
        },
    }
}

/// Your agent's list, as the web app shows it.
pub async fn view(app: &Shared) -> ApiResult<AgentList> {
    let mut conn = app.db.acquire().await?;
    // What an agent that stopped held isn't being done.
    kelve_store::asks::let_go_of_lapsed(&mut conn).await?;
    let asks = kelve_store::asks::open(&mut conn).await?;
    // Working while it's called kelve lately, or holds a job taken lately.
    let latest = kelve_store::asks::latest_agent(&mut conn).await?;
    let seen = latest.as_ref().map(|a| a.last_seen_at + WORKING_MS);
    let holding = kelve_store::asks::latest_taken(&mut conn)
        .await?
        .map(|at| at + HOLDING_MS);
    let working_until = seen.max(holding);
    let working = working_until.is_some_and(|until| Utc::now().timestamp_millis() < until);
    // What you have open.
    let open = kelve_store::reader::lately(&mut conn, 1)
        .await?
        .first()
        .map(|o| o.id);
    // What each thing opened is called, for what was asked in it.
    let mut titles: BTreeMap<i64, String> = BTreeMap::new();
    for a in &asks {
        if let Some(id) = a
            .opened
            .filter(|id| Some(*id) != open && !titles.contains_key(id))
            && let Some(row) = kelve_store::reader::get(&mut conn, id).await?
        {
            titles.insert(id, row.title);
        }
    }
    // Each redo, in the order its job comes, with whether it's being done.
    let items: Vec<(ListItem, bool)> = asks
        .iter()
        .filter(|a| a.kind == agent::REDO)
        .map(|a| {
            let asked_in = a.opened.and_then(|id| titles.get(&id).cloned());
            (row_of(a, None, asked_in), a.state == "taken")
        })
        .collect();
    // Being done only while the agent's working: what one that stopped
    // took is in what's next, where it was.
    let (now, rest): (Vec<_>, Vec<_>) = items.into_iter().partition(|(_, taken)| *taken && working);
    // What you asked for in other things comes after what's open's.
    let (elsewhere, next): (Vec<_>, Vec<_>) =
        rest.into_iter().partition(|(i, _)| i.asked_in.is_some());
    let since = (Utc::now() - DONE_FOR).timestamp_millis();
    let ready = kelve_store::asks::answered_since(&mut conn, agent::REDO, since)
        .await?
        .iter()
        .map(|a| {
            let answer: Value = serde_json::from_str(a.answer.as_deref().unwrap_or_default())
                .unwrap_or(Value::Null);
            row_of(a, answer["did"].as_str().map(str::to_string), None)
        })
        .collect();
    drop(conn);
    Ok(AgentList {
        working,
        working_until,
        by: latest.filter(|_| working).map(|a| a.name),
        open,
        now: now.into_iter().map(|(i, _)| i).collect(),
        next: next.into_iter().map(|(i, _)| i).collect(),
        elsewhere: elsewhere.into_iter().map(|(i, _)| i).collect(),
        ready,
    })
}
