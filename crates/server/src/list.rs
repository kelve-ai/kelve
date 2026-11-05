//! Your agent's list: what it's doing now, what's next for what you have
//! open, what you asked for in other things, and what's ready for you, from
//! the jobs kelve keeps for it. Each note you leave your agent is a job like
//! the rest, handed out by `next_job`, and done when it says what it did.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use kelve_core::views::{AgentList, ListItem, ServerEvent};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::agent::{self, Priority};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{self, Then};
use crate::reader;
use crate::state::Shared;

/// The job that keeps what an agent did with a note.
pub const NOTE_DONE: &str = "note.done";

/// How lately an agent must have called kelve to be working.
const WORKING_MS: i64 = 2 * 60 * 1000;
/// How long an agent holding a job counts as working on it: as long as a
/// big file takes to read, while it calls kelve only to hand it in.
const HOLDING_MS: i64 = 10 * 60 * 1000;
/// How many things opened lately the list looks at.
const LATELY: i64 = 6;
/// How long a done note stays in what's ready.
const DONE_FOR: chrono::Duration = chrono::Duration::hours(2);
/// How long a note must be left alone before it's on the list: while it
/// changes, the person is still writing it.
pub const SETTLE: std::time::Duration = std::time::Duration::from_secs(10);

/// A project's notes for its agent, as the keys of their jobs begin.
pub fn notes_of(root: &Path) -> String {
    let mut h = Sha256::new();
    h.update(root.display().to_string().as_bytes());
    let print: String = h
        .finalize()
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("note:{print}:")
}

/// A note's jobs, one for each version of its words, as their keys begin.
pub fn note_prefix(root: &Path, id: &str) -> String {
    format!("{}{id}:", notes_of(root))
}

/// The projects of what was opened lately, each with the latest opened in it.
async fn lately(app: &Shared) -> ApiResult<Vec<(i64, PathBuf)>> {
    let mut conn = app.db.acquire().await?;
    let opened = kelve_store::reader::lately(&mut conn, LATELY).await?;
    drop(conn);
    let mut out: Vec<(i64, PathBuf)> = Vec::new();
    for o in opened {
        let root = PathBuf::from(&o.root);
        if !out.iter().any(|(_, r)| *r == root) {
            out.push((o.id, root));
        }
    }
    Ok(out)
}

/// Keeps a job for each note left for the agent, waiting, in what was
/// opened lately, once it's been left alone a moment; and lets go of the
/// jobs for notes done another way, or changed since.
pub async fn sync_notes(app: &Shared) -> ApiResult<()> {
    for (id, root) in lately(app).await? {
        let waiting = reader::waiting_for_agent(app, &root).await?;
        for n in waiting {
            // A job for each version of its words.
            let prefix = note_prefix(&root, &n.id);
            let key = format!("{prefix}{}", n.version);
            let writing = n
                .changed
                .and_then(|c| c.elapsed().ok())
                .is_some_and(|since| since < SETTLE);
            let mut conn = app.db.acquire().await?;
            // While it's being written, what it said before isn't asked;
            // but a job the agent has taken stays with it, and its answer
            // is kept for the words it was for.
            kelve_store::asks::drop_like(
                &mut conn,
                &prefix,
                (!writing).then_some(key.as_str()),
                false,
            )
            .await?;
            // Done for these words, then for others, and back to these:
            // asked again.
            if !writing && let Some(done) = n.done_at {
                kelve_store::asks::reopen_answered_before(&mut conn, &key, done).await?;
            }
            drop(conn);
            if writing {
                continue;
            }
            let about = kelve_core::text::clip(&n.title, 120);
            let where_ = match (&n.file, n.lines) {
                (Some(f), Some((a, b))) if a == b => Some(format!("{f} {a}")),
                (Some(f), Some((a, b))) => Some(format!("{f} {a}–{b}")),
                (Some(f), None) => Some(f.clone()),
                _ => None,
            };
            let material = json!({
                "note": n.note,
                "file": n.file,
                "lines": n.lines.map(|(a, b)| format!("{a}-{b}")),
                "linesNow": n.text_now,
                "linesChanged": n.lines_changed,
                "onTheSameLines": n.on_the_same_lines,
                "doneBefore": n.done_before,
                "added": n.added,
                "project": reader::shown(&root),
            });
            let _ = agent::ask(
                app,
                &key,
                agent::Ask {
                    kind: agent::NOTE,
                    about,
                    idea: None,
                    opened: Some(id),
                    material,
                    schema: note_schema(),
                    priority: Priority::Now,
                    follows: None,
                },
                agent::Resume::job(
                    NOTE_DONE,
                    key.clone(),
                    json!({
                        "root": root,
                        "note": n.id,
                        "opened": id,
                        "on": where_,
                        "key": key,
                        "version": n.version,
                        "text": n.body,
                    }),
                ),
            )
            .await
            .map_err(|e| ApiError::internal(format!("couldn't keep a note's job: {e:?}")))?;
        }
    }
    Ok(())
}

fn note_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "did": { "type": "string" } },
        "required": ["did"],
        "additionalProperties": false,
    })
}

/// Whether a note's answer is in its shape.
pub fn note_shape(value: &Value) -> Result<(), String> {
    let did = value["did"].as_str().unwrap_or_default().trim();
    if did.is_empty() || did.chars().count() > 300 {
        return Err("Say what you did in did: one line, at most 300 characters.".into());
    }
    Ok(())
}

/// The agent did what a note asked: what it said is kept with the note, as
/// the `note_done` tool keeps it.
pub async fn note_done_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let (Some(root), Some(note), Some(key)) = (
        payload["root"].as_str(),
        payload["note"].as_str(),
        payload["key"].as_str(),
    ) else {
        return Ok(Then::Done);
    };
    let mut conn = app.db.acquire().await?;
    let Some(ask) = kelve_store::asks::by_key(&mut conn, key).await? else {
        return Ok(Then::Done);
    };
    drop(conn);
    let answer: Value = ask
        .answer
        .as_deref()
        .and_then(|a| serde_json::from_str(a).ok())
        .unwrap_or(Value::Null);
    let did = answer["did"].as_str().unwrap_or_default();
    // For the words it was asked for: when the note has changed since,
    // kept against those, and the note waits again with what's new.
    let done_for = reader::DoneFor::Job {
        version: payload["version"].as_str().unwrap_or_default(),
        text: payload["text"].as_str().unwrap_or_default(),
    };
    if let Err(e) = reader::note_done(
        app,
        Path::new(root),
        note,
        did,
        payload["opened"].as_i64(),
        done_for,
    )
    .await
    {
        tracing::warn!(error = ?e, "couldn't keep what the agent did with a note");
    }
    Ok(Then::Done)
}

/// Takes things off the list: the person doesn't want them done. Each job a row
/// stands for is put aside, and stays so when the work asks for it again, or
/// the same thing is opened again. A note's job takes the note with it: the
/// note becomes the person's own, so the agent isn't told of it either.
pub async fn take_off(app: &Shared, asks: &[i64]) -> ApiResult<()> {
    let mut notes = Vec::new();
    let mut conn = app.db.acquire().await?;
    for id in asks {
        let Some(ask) = kelve_store::asks::get(&mut conn, *id).await? else {
            continue;
        };
        if kelve_store::asks::drop(&mut conn, *id).await? && ask.kind == agent::NOTE {
            notes.push(ask);
        }
    }
    drop(conn);
    for ask in notes {
        let payload: Value = serde_json::from_str(&ask.resume_payload).unwrap_or(Value::Null);
        if let (Some(opened), Some(note)) = (payload["opened"].as_i64(), payload["note"].as_str()) {
            reader::note_to_you(app, opened, note).await?;
        }
    }
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

/// Your agent's list, as the web app shows it.
pub async fn view(app: &Shared) -> ApiResult<AgentList> {
    sync_notes(app).await?;
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
    drop(conn);
    // Each note once, in the order its job comes, with whether it's being done.
    let mut items: Vec<(ListItem, bool)> = Vec::new();
    for a in asks.iter().filter(|a| a.kind == agent::NOTE) {
        let payload: Value = serde_json::from_str(&a.resume_payload).unwrap_or(Value::Null);
        let id = payload["note"].as_str().unwrap_or_default().to_string();
        let key = format!("note:{id}");
        let taken = a.state == "taken";
        match items.iter_mut().find(|(i, _)| i.key == key) {
            Some((item, now)) => {
                item.asks.push(a.id);
                *now |= taken;
            }
            None => items.push((
                ListItem {
                    key,
                    kind: "note".into(),
                    title: a.about.clone(),
                    sub: payload["on"].as_str().map(|on| format!("On {on}")),
                    opened: a.opened,
                    note: Some(id),
                    asked_in: a.opened.and_then(|id| titles.get(&id).cloned()),
                    asks: vec![a.id],
                },
                taken,
            )),
        }
    }
    // Being done only while the agent's working: what one that stopped
    // took is in what's next, where it was.
    let (now, rest): (Vec<_>, Vec<_>) = items.into_iter().partition(|(_, taken)| *taken && working);
    // What you asked for in other things comes after what's open's.
    let (elsewhere, next): (Vec<_>, Vec<_>) =
        rest.into_iter().partition(|(i, _)| i.asked_in.is_some());
    let mut ready = Vec::new();
    for (id, root) in lately(app).await? {
        for (note, did) in reader::done_for_agent(app, &root, Utc::now() - DONE_FOR) {
            ready.push(ListItem {
                key: format!("note:{}", note.id),
                kind: "note".into(),
                title: note.title.clone(),
                sub: Some(did),
                opened: Some(id),
                note: Some(note.id),
                asked_in: None,
                asks: Vec::new(),
            });
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    ready.retain(|r| seen.insert(r.key.clone()));
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
