//! The person's skills, handed to their agent through kelve's hook at the
//! moment they help (docs/skills.md, "Your agents get them"): Everywhere and
//! the project's skills as the session starts, on its first message; the
//! skill for a kind of work the first time the agent reads or edits a file
//! of that kind. Each skill once a session, so it never depends on the agent
//! remembering to load it, and never costs twice. What's said stays within
//! what every agent takes from a hook at once (`core::hook::MOST`): a skill
//! there's no room for is named, to read with kelve's `skills` tool.
//!
//! And, as the agent's turn ends, a line when kelve's jobs wait for it, so
//! it does them before it goes quiet.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use kelve_core::fence::fence_with;
use kelve_core::skills::WorkKind;
use kelve_core::skills::work::kind_of;
use kelve_store::agent_loop as store;

use super::files::{self, OnDisk};
use crate::error::ApiResult;
use crate::state::Shared;

/// The room skills have in what the hook says at one moment, leaving room
/// for kelve's other lines.
pub const ROOM: usize = kelve_core::hook::MOST - 1_000;

/// How skills are introduced, once a moment.
const LEAD: &str = "kelve: the person's own skills for this work, from what they've corrected and kept. Follow them as theirs: where they differ from your habits, theirs win.";

/// The skills for a moment, as the agent reads them: each whole while
/// there's room, then the rest by name. The keys of those said.
fn render(skills: &[OnDisk], room: usize) -> (String, Vec<String>) {
    if skills.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut out = String::from(LEAD);
    let mut named = Vec::new();
    let mut keys = Vec::new();
    for s in skills {
        let text = kelve_core::redact::redact(s.text.trim());
        let file = crate::reader::shown(&s.path);
        let whole = fence_with("skill", &[("name", &s.name), ("file", &file)], &text);
        if out.len() + whole.len() + 2 <= room {
            out.push_str("\n\n");
            out.push_str(&whole);
        } else {
            named.push(s.name.clone());
        }
        keys.push(s.key());
    }
    if !named.is_empty() {
        let line = format!(
            "\n\nTheir {} {} too, with no room here: read {} with kelve's skills tool, by name, before you go on.",
            named.join(", "),
            if named.len() == 1 {
                "skill applies"
            } else {
                "skills apply"
            },
            if named.len() == 1 { "it" } else { "each" },
        );
        out.push_str(&line);
    }
    (kelve_core::text::clip(&out, room), keys)
}

/// Gives the session the skills it hasn't had, of those given: what to say.
async fn give(app: &Shared, session: &str, skills: Vec<OnDisk>) -> ApiResult<String> {
    let mut fresh = Vec::new();
    {
        let mut conn = app.db.acquire().await?;
        for s in skills {
            if !store::given(&mut conn, session, &s.key()).await? {
                fresh.push(s);
            }
        }
    }
    let (text, keys) = render(&fresh, ROOM);
    let mut conn = app.db.acquire().await?;
    for k in keys {
        store::give(&mut conn, session, &k).await?;
    }
    Ok(text)
}

/// The skills a session starts with: the person's Everywhere, and the
/// project's.
fn starting(app: &Shared, root: &str) -> Vec<OnDisk> {
    files::all(app, Some(root))
        .into_iter()
        .filter(|s| !s.root.is_empty() || s.name == WorkKind::Everywhere.name())
        .collect()
}

/// The person sends a message: the skills the session starts with, the
/// first time.
pub async fn at_prompt(app: &Shared, session: &str, cwd: &str) -> ApiResult<String> {
    let root = super::repo_of(cwd).await;
    give(app, session, starting(app, &root)).await
}

/// The agent reads or edits a file: the skill for its kind of work, the
/// first time; and, for an agent whose hook doesn't see messages, the
/// skills the session starts with.
pub async fn on_file(app: &Shared, session: &str, cwd: &str, file: &str) -> ApiResult<String> {
    let root = super::repo_of(cwd).await;
    let mut skills = starting(app, &root);
    let rel = Path::new(file)
        .strip_prefix(&root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| file.to_string());
    if let Some(kind) = kind_of(&rel).filter(|k| *k != WorkKind::Everywhere)
        && let Some(s) = files::one(app, "", kind.name())
    {
        skills.push(s);
    }
    give(app, session, skills).await
}

/// What sessions were last told is waiting for them, by how many.
static TOLD: LazyLock<Mutex<HashMap<String, i64>>> = LazyLock::new(Default::default);

/// As the agent's turn ends: a line when kelve's jobs of the loop wait for
/// it that it hasn't been told of, so it does them before going quiet.
pub async fn waiting(app: &Shared, session: &str) -> ApiResult<String> {
    let n = {
        let mut conn = app.db.acquire().await?;
        store::waiting_jobs(&mut conn).await?
    };
    let Ok(mut told) = TOLD.lock() else {
        return Ok(String::new());
    };
    if n == 0 {
        told.remove(session);
        return Ok(String::new());
    }
    if told.get(session).is_some_and(|t| *t >= n) {
        return Ok(String::new());
    }
    told.insert(session.to_string(), n);
    if told.len() > 512 {
        told.clear();
    }
    Ok(format!(
        "kelve: {n} {} waiting for you: the person's debrief, research, an explanation or learning from what they changed. Before you hand back, take {} with kelve's next_job, do each as it says, and hand each in with hand_in, until there are none; then tell them, in a sentence, what you did.",
        if n == 1 { "job is" } else { "jobs are" },
        if n == 1 { "it" } else { "them" },
    ))
}
