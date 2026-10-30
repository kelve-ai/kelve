//! Your agent's list (boards Q-1 to Q-9, J-1 to J-3): what it's doing now,
//! what's next for what you have open, what you asked for in other things,
//! what's ready for you, and what's paused, from the jobs kelve keeps for it.
//! Reading what you opened is one thing on it, its files read plainly too, as
//! part of reading them; an idea's entry another; and each note you leave your
//! agent is a job like the rest, handed out by `next_job`, and done when it
//! says what it did. What you have open is what was opened last: reading
//! anything opened before it is paused until it's opened again.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use kelve_core::views::{AgentList, ListItem, Paused, ReadyItem, ServerEvent, StepAt, WorkStage};
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

/// The kinds of job that are reading what was opened: reading it plainly too.
const READING: [&str; 9] = [
    agent::READ,
    agent::CHOOSE,
    agent::BUILT_ON,
    agent::FOLDERS,
    agent::WAY,
    agent::PATH,
    agent::PLAINLY,
    agent::PLAINLY_FOLDER,
    agent::PLAINLY_CHANGE,
];
/// What the way in's pages line says under it.
const PAGES_SAID: &str = "Each once its files are read plainly, the deepest first.";
/// What the rest's pages line says under it.
const REST_SAID: &str = "Every folder and the folders in it, the shallowest first, so you can find your own way after the way in.";

/// The kinds that are an idea's entry.
const ENTRY: [&str; 4] = [agent::WRITE, agent::FIX, agent::EXTEND, agent::RELATE];

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
                "ideas": n.ideas,
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
/// note becomes the person's own, so the agent isn't told of it either. What's
/// `paused`, by what was opened, has the rest of its reading put aside.
pub async fn take_off(app: &Shared, asks: &[i64], paused: Option<i64>) -> ApiResult<()> {
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
    if let Some(opened) = paused {
        kelve_store::asks::drop_reading(&mut conn, opened).await?;
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

/// Your agent's list, as the reader shows it.
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
    // What you have open, and what was opened before it with reading left.
    let open = kelve_store::reader::lately(&mut conn, 1)
        .await?
        .first()
        .map(|o| o.id);
    let mut paused = Vec::new();
    for p in kelve_store::asks::paused(&mut conn).await? {
        let Some(row) = kelve_store::reader::get(&mut conn, p.opened).await? else {
            continue;
        };
        let was = reader::Open {
            root: PathBuf::from(&row.root),
            row,
        };
        paused.push(Paused {
            opened: p.opened,
            kind: was.kind(),
            title: was.row.title.clone(),
            left: u32::try_from(p.left).unwrap_or(0),
            opened_at: kelve_core::Timestamp(kelve_store::at(was.row.opened_at)),
            paused_at: kelve_core::Timestamp(kelve_store::at(p.paused_at)),
        });
    }
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
    // Each thing once, in the order its first job comes, with whether any of it is being done.
    let mut items: Vec<(ListItem, bool)> = Vec::new();
    let mut at: BTreeMap<String, usize> = BTreeMap::new();
    // What reading has left, as its lines count it: each file once, by
    // its path, however many parts it's read in, with the check of its
    // ideas and its plainly; and each page, its folder's or the change's.
    let mut left: BTreeMap<String, (std::collections::BTreeSet<String>, u32)> = BTreeMap::new();
    // The files each reading line is reading, to say whether it's the way in.
    let mut reading_files: BTreeMap<String, std::collections::BTreeSet<String>> = BTreeMap::new();
    for a in &asks {
        let payload: Value = serde_json::from_str(&a.resume_payload).unwrap_or(Value::Null);
        let taken = a.state == "taken";
        let (key, kind, title, sub, note) = if READING.contains(&a.kind.as_str()) {
            let Some(opened) = a.opened else { continue };
            // Paused, unless the agent's reading it now.
            if Some(opened) != open && open.is_some() && !(taken && working) {
                continue;
            }
            match a.kind.as_str() {
                // The rest's pages, after the way in: a line of their own.
                agent::PLAINLY_FOLDER
                    if payload["priority"].as_i64() == Some(Priority::Later as i64) =>
                {
                    (
                        format!("rest:{opened}"),
                        "pages",
                        "Folder pages for the rest".to_string(),
                        Some(REST_SAID.to_string()),
                        None,
                    )
                }
                agent::PLAINLY_FOLDER | agent::PLAINLY_CHANGE => (
                    format!("pages:{opened}"),
                    "pages",
                    String::new(),
                    None,
                    None,
                ),
                _ => (
                    format!("read:{opened}"),
                    "reading",
                    String::new(),
                    None,
                    None,
                ),
            }
        } else if ENTRY.contains(&a.kind.as_str()) {
            let Some(idea) = &a.idea else { continue };
            let name = {
                let mut conn = app.db.acquire().await?;
                kelve_store::entries::idea(&mut conn, idea)
                    .await?
                    .map(|i| i.name)
            };
            (
                format!("idea:{idea}"),
                "entry",
                name.unwrap_or_else(|| a.about.clone()),
                None,
                None,
            )
        } else if a.kind == agent::NOTE {
            let id = payload["note"].as_str().unwrap_or_default().to_string();
            let sub = payload["on"].as_str().map(|on| format!("On {on}"));
            (format!("note:{id}"), "note", a.about.clone(), sub, Some(id))
        } else {
            continue;
        };
        // In hand: what the agent's reading, or whose page it's writing.
        let mut hand = None;
        if kind == "reading" || kind == "pages" {
            let (files, pages) = left.entry(key.clone()).or_default();
            let path = a
                .resume_key
                .as_deref()
                .and_then(|k| k.splitn(3, ':').nth(2))
                .map(str::to_string);
            match a.kind.as_str() {
                agent::READ | agent::CHOOSE | agent::PLAINLY => {
                    files.insert(path.clone().unwrap_or_else(|| a.id.to_string()));
                    if let Some(p) = &path {
                        reading_files
                            .entry(key.clone())
                            .or_default()
                            .insert(p.clone());
                    }
                }
                agent::PLAINLY_FOLDER | agent::PLAINLY_CHANGE => *pages += 1,
                _ => {}
            }
            let stage = match a.kind.as_str() {
                agent::READ => Some(WorkStage::Reading),
                agent::CHOOSE => Some(WorkStage::Checking),
                agent::PLAINLY | agent::PLAINLY_FOLDER | agent::PLAINLY_CHANGE => {
                    Some(WorkStage::Writing)
                }
                _ => None,
            };
            // A folder by its path, the top or a change's own page by
            // what it's called.
            let what = match a.kind.as_str() {
                agent::PLAINLY_FOLDER | agent::PLAINLY_CHANGE => a
                    .about
                    .strip_prefix("Reading ")
                    .and_then(|t| t.strip_suffix(" plainly"))
                    .map(str::to_string),
                _ => path,
            };
            if taken
                && working
                && let (Some(stage), Some(path)) = (stage, what)
            {
                hand = Some(StepAt { path, stage });
            }
        }
        match at.get(&key) {
            Some(&i) => {
                let (item, now) = &mut items[i];
                item.asks.push(a.id);
                // Each file once: a big one read in parts, by several
                // agents, is as far as its least done part.
                if let Some(h) = hand {
                    match item.hand.iter_mut().find(|x| x.path == h.path) {
                        Some(x) => x.stage = x.stage.min(h.stage),
                        None => item.hand.push(h),
                    }
                }
                *now |= taken;
            }
            None => {
                at.insert(key.clone(), items.len());
                items.push((
                    ListItem {
                        key,
                        kind: kind.into(),
                        title,
                        sub,
                        opened: a.opened,
                        idea: a.idea.clone().filter(|_| kind == "entry"),
                        note,
                        left: None,
                        asked_in: a.opened.and_then(|id| titles.get(&id).cloned()),
                        asks: vec![a.id],
                        of: None,
                        hand: hand.into_iter().collect(),
                    },
                    taken,
                ));
            }
        }
    }
    for (item, _) in &mut items {
        if let Some((files, pages)) = left.get(&item.key) {
            item.left = Some(u32::try_from(files.len()).unwrap_or(u32::MAX) + pages);
        }
        let Some(opened) = item.opened else { continue };
        match item.kind.as_str() {
            "reading" | "pages" if item.title.is_empty() => {
                let Ok(was) = reader::opened(app, opened).await else {
                    continue;
                };
                let way: std::collections::HashSet<String> = crate::way::view(app, &was)
                    .await
                    .map(|w| w.order.into_iter().map(|s| s.path).collect())
                    .unwrap_or_default();
                let on_way = reading_files
                    .get(&item.key)
                    .is_some_and(|files| files.iter().any(|f| way.contains(f)));
                let change = was.kind() == kelve_core::views::OpenKind::Change;
                (item.title, item.sub) = match (item.kind.as_str(), on_way, way.is_empty(), change)
                {
                    ("reading", true, _, _) => ("Reading the way in".into(), None),
                    ("reading", ..) => ("Reading its files".into(), None),
                    (_, _, _, true) => ("Pages for the change".into(), Some(PAGES_SAID.into())),
                    (_, _, false, _) => (
                        "Folder pages for the way in".into(),
                        Some(PAGES_SAID.into()),
                    ),
                    _ => ("Folder pages".into(), Some(PAGES_SAID.into())),
                };
            }
            "pages" if item.key.starts_with("rest:") => {
                // How many in all: those written as well as those to go.
                let mut conn = app.db.acquire().await?;
                let done =
                    kelve_store::asks::answered_later(&mut conn, opened, agent::PLAINLY_FOLDER)
                        .await?;
                item.of = Some(u32::try_from(done).unwrap_or(0) + item.left.unwrap_or(0));
            }
            _ => {}
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
        if let Ok(open) = reader::opened(app, id).await {
            for r in crate::ready::list(app, &open).await? {
                ready.push(match r {
                    ReadyItem::Entry { key, name } => ListItem {
                        key: format!("idea:{key}"),
                        kind: "entry".into(),
                        title: name,
                        sub: None,
                        opened: Some(id),
                        idea: Some(key),
                        note: None,
                        left: None,
                        asked_in: None,
                        asks: Vec::new(),
                        of: None,
                        hand: Vec::new(),
                    },
                });
            }
        }
        for (note, did) in reader::done_for_agent(app, &root, Utc::now() - DONE_FOR) {
            ready.push(ListItem {
                key: format!("note:{}", note.id),
                kind: "note".into(),
                title: note.title.clone(),
                sub: Some(did),
                opened: Some(id),
                idea: None,
                note: Some(note.id),
                left: None,
                asked_in: None,
                asks: Vec::new(),
                of: None,
                hand: Vec::new(),
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
        paused,
    })
}
