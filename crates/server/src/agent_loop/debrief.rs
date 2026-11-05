//! The debrief of a piece of work (docs/skills.md, "When there's a debrief").
//! As the agent hands its work over, kelve decides from the change itself
//! whether it's worth one (`core::due`), as the person chose in settings.
//! When it is, kelve asks for it at once, and the open tool's reply tells
//! the agent to write it now, while it remembers why. One piece of work has
//! one debrief till the person sends it: more on the same work brings it up
//! to date. The agent's debrief is checked against the change as it was
//! handed over (`core::debrief::read`): one that fails goes back to it once,
//! with what to fix, then home says it failed, with Ask it again.

use std::path::Path;

use chrono::Utc;
use kelve_core::debrief::{self, Changed, Range};
use kelve_core::due::{self, DebriefsWhen};
use kelve_core::fence::{fence, fence_with};
use kelve_core::skills::work::kind_of;
use kelve_store::checks::NewTask;
use kelve_store::handovers::{self, HandedFile, Handover};
use kelve_store::{agent_loop as store, asks, debriefs};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{fail, files};
use crate::agent::{self, Ask, Priority, Resume};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{self, Then};
use crate::reader::Open;
use crate::state::Shared;
use crate::v2jobs;

/// The most of the change given in a debrief's material, in characters:
/// the agent has the rest in front of it.
const CHANGE: usize = 60_000;
/// The most of each skill and instruction file given, in characters.
const SKILL: usize = 8_000;
/// The most of all skills given, in characters.
const SKILLS: usize = 24_000;
/// The project's instruction files for agents, read for the debrief.
const INSTRUCTIONS: [&str; 4] = [
    "AGENTS.md",
    "CLAUDE.md",
    ".github/copilot-instructions.md",
    "GEMINI.md",
];
/// More files than this, or lines, and the change is large: the debrief
/// groups its decisions by part.
const LARGE_FILES: usize = 20;
const LARGE_LINES: i64 = 1_000;

/// What the agent is told of a debrief as it hands work over.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handed {
    /// The debrief's id, when one is asked for.
    pub debrief: Option<i64>,
    /// Whether one is asked for now.
    pub due: bool,
    /// Why, or why not, in a sentence for the person.
    pub reason: String,
    /// Whether this brings a debrief not yet sent up to date.
    pub again: bool,
}

/// A hunk's line, as a handover keeps it.
#[derive(Debug, Clone, Deserialize)]
struct Line {
    kind: String,
    old: Option<u32>,
    new: Option<u32>,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Hunk {
    lines: Vec<Line>,
}

fn hunks_of(f: &HandedFile) -> Vec<Hunk> {
    serde_json::from_str(&f.hunks).unwrap_or_default()
}

/// Whether a file's lines name what `due` reads in them: manifests and
/// files that run commands.
fn listed(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or_default().to_lowercase();
    [
        "cargo.toml",
        "package.json",
        "pyproject.toml",
        "pipfile",
        "setup.py",
        "setup.cfg",
        "go.mod",
        "gemfile",
        "composer.json",
        "deno.json",
        "makefile",
        "justfile",
        "taskfile.yml",
        "taskfile.yaml",
    ]
    .contains(&name.as_str())
        || name.starts_with("requirements")
}

/// A handover's files, as `due` reads them.
pub fn due_files(handed: &[HandedFile]) -> Vec<due::File> {
    handed
        .iter()
        .map(|f| {
            let status = match (f.status.as_str(), &f.moved_from) {
                (_, Some(_)) => due::Status::Renamed,
                ("added", _) => due::Status::Added,
                ("deleted", _) => due::Status::Deleted,
                _ => due::Status::Modified,
            };
            let mut file = due::File::new(
                &f.path,
                status,
                u32::try_from(f.added).unwrap_or(u32::MAX),
                u32::try_from(f.removed).unwrap_or(u32::MAX),
            );
            let hunks = hunks_of(f);
            let squeeze = |kind: &str| -> String {
                hunks
                    .iter()
                    .flat_map(|h| &h.lines)
                    .filter(|l| l.kind == kind)
                    .flat_map(|l| l.text.chars().filter(|c| !c.is_whitespace()))
                    .collect()
            };
            file.whitespace_only = (f.added + f.removed) > 0
                && !hunks.is_empty()
                && squeeze("added") == squeeze("removed");
            if listed(&f.path) {
                for l in hunks.iter().flat_map(|h| &h.lines) {
                    match l.kind.as_str() {
                        "added" => file.added_lines.push(l.text.clone()),
                        "removed" => file.removed_lines.push(l.text.clone()),
                        _ => {}
                    }
                }
            }
            file
        })
        .collect()
}

/// A handover's files, each with the runs of lines it added, on its new
/// side: what a debrief's lines are checked against.
pub fn changed_of(handed: &[HandedFile]) -> Vec<Changed> {
    handed
        .iter()
        .map(|f| {
            let mut ranges: Vec<Range> = Vec::new();
            for l in hunks_of(f).iter().flat_map(|h| &h.lines) {
                let (true, Some(n)) = (l.kind == "added", l.new) else {
                    continue;
                };
                match ranges.last_mut() {
                    Some(r) if r.to + 1 == n => r.to = n,
                    _ => ranges.push(Range::new(n, n)),
                }
            }
            Changed {
                path: f.path.clone(),
                ranges,
            }
        })
        .collect()
}

/// The change as the agent reads it: each file with what happened to it,
/// and its hunks with their line numbers as the file is now. Secrets were
/// masked as it was read.
pub fn change_text(handed: &[HandedFile], most: usize) -> String {
    let mut out = String::new();
    let mut left = 0;
    for f in handed {
        if out.len() >= most {
            left += 1;
            continue;
        }
        let moved = f
            .moved_from
            .as_deref()
            .map(|m| format!(", moved from {m}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "--- {} ({}{moved}, +{} -{})\n",
            f.path, f.status, f.added, f.removed
        ));
        if let Some(skip) = &f.skip {
            out.push_str(&format!("(not shown: {skip})\n"));
            continue;
        }
        for h in hunks_of(f) {
            out.push_str("@@\n");
            for l in &h.lines {
                match l.kind.as_str() {
                    "added" => out.push_str(&format!("{:>5} + {}\n", l.new.unwrap_or(0), l.text)),
                    "removed" => out.push_str(&format!(
                        "{:>5} - {}\n",
                        l.old.map(|o| format!("({o})")).unwrap_or_default(),
                        l.text
                    )),
                    _ => out.push_str(&format!("{:>5}   {}\n", l.new.unwrap_or(0), l.text)),
                }
            }
        }
    }
    if left > 0 {
        out.push_str(&format!(
            "… and {left} more files, not shown here: read them in the repository.\n"
        ));
    }
    out
}

/// The answer's shape: one markdown file.
fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "debrief": { "type": "string", "description": "The debrief: one markdown file, starting with its front matter (kelve: debrief/1, title)." },
        },
        "required": ["debrief"],
    })
}

/// Whether the agent's debrief is sound: read against the change as it was
/// handed over, kept with the job.
pub fn shape(material: &Value, value: &Value) -> Result<(), String> {
    let text = value["debrief"].as_str().unwrap_or_default();
    if text.trim().is_empty() {
        return Err(
            "Hand in the debrief as debrief: one markdown file, starting with its front matter."
                .into(),
        );
    }
    if kelve_core::redact::redact(text) != text {
        return Err(
            "The debrief seems to hold a secret. Take it out, and hand it in again.".into(),
        );
    }
    let changed: Vec<Changed> =
        serde_json::from_value(material["changed"].clone()).unwrap_or_default();
    debrief::read(text, &changed)
        .map(|_| ())
        .map_err(|problems| {
            format!(
                "Fix the debrief, then hand it in again: {}",
                problems.join(" ")
            )
        })
}

/// How often the person wants a debrief.
fn when(app: &Shared) -> DebriefsWhen {
    app.settings().debriefs
}

/// The agent handed its work over, as `open` kept it: whether a debrief is
/// due, and, when it is, or when the same work has one not yet sent, the
/// debrief brought up to date and asked for at once. `asked` is the person
/// asking for one, as they do with "debrief this".
pub async fn on_handover(
    app: &Shared,
    open: &Open,
    handover: i64,
    asked: bool,
) -> ApiResult<Handed> {
    let (h, handed) = {
        let mut conn = app.db.acquire().await?;
        let h = handovers::get(&mut conn, handover)
            .await?
            .ok_or_else(|| ApiError::internal("a handover not kept"))?;
        let files = handovers::files(&mut conn, handover).await?;
        (h, files)
    };
    // The branch, from git, when the change's words didn't say.
    let branch = if h.branch.is_empty() {
        let root = open.root.clone();
        let found = tokio::task::spawn_blocking(move || kelve_engine::git::branch(&root))
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        if !found.is_empty() {
            let mut conn = app.db.acquire().await?;
            store::set_branch(&mut conn, handover, &found).await?;
        }
        found
    } else {
        h.branch.clone()
    };
    let decided = due::due(
        &due_files(&handed),
        &due::Handover {
            said: format!("{} {}", h.title, h.said),
            said_exactly: false,
        },
        if asked {
            DebriefsWhen::Every
        } else {
            when(app)
        },
    );
    let decided = if asked && decided.due {
        due::Due {
            due: true,
            reason: "You asked for one.".into(),
        }
    } else {
        decided
    };
    let mut conn = app.db.acquire().await?;
    store::set_due(&mut conn, handover, decided.due, &decided.reason).await?;
    let open_one = store::open_debrief(&mut conn, &h.root, &branch, &h.session).await?;
    if !decided.due && open_one.is_none() {
        // Not worth one: home shows it as one quiet line, with Debrief it.
        drop(conn);
        keep_task(app, &h, None, &open.root).await?;
        return Ok(Handed {
            debrief: None,
            due: false,
            reason: decided.reason,
            again: false,
        });
    }
    let (id, new) =
        store::debrief_for_work(&mut conn, handover, &h.root, &branch, &h.session).await?;
    // What it learned from the person since, in this repository, shows in it.
    for c in store::unshown_in(&mut conn, &h.root).await? {
        store::show_in(&mut conn, c, id).await?;
    }
    drop(conn);
    keep_task(app, &h, Some(id), &open.root).await?;
    ask(app, id).await.map_err(ApiError::from)?;
    Ok(Handed {
        debrief: Some(id),
        due: true,
        reason: if decided.due {
            decided.reason
        } else {
            "It's more on work that has a debrief not yet sent.".into()
        },
        again: !new,
    })
}

/// The person asked for a debrief of a handover anyway (Debrief it, on
/// home): asked for now, whatever their setting. Its id.
pub async fn anyway(app: &Shared, handover: i64) -> ApiResult<i64> {
    let opened = {
        let mut conn = app.db.acquire().await?;
        handovers::get(&mut conn, handover)
            .await?
            .ok_or_else(|| ApiError::not_found("That work isn't kept any more."))?
            .opened
            .ok_or_else(|| ApiError::not_found("That work isn't open in kelve any more."))?
    };
    let open = crate::reader::opened(app, opened).await?;
    on_handover(app, &open, handover, true)
        .await?
        .debrief
        .ok_or_else(|| {
            ApiError::conflict("That work changed nothing, so there's nothing to debrief.")
        })
}

/// Ask it again, for a debrief that failed its checks: waiting for the
/// agent, with its tries forgotten.
pub async fn again(app: &Shared, debrief: i64) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    if !store::debrief_again(&mut conn, debrief).await? {
        return Err(ApiError::conflict(
            "That debrief was sent, so there's nothing to ask again.",
        ));
    }
    // The job put aside after its second try is asked afresh, as asking
    // again under its key does.
    drop(conn);
    ask(app, debrief).await.map_err(ApiError::from)?;
    Ok(())
}

/// The piece of work as a past task, for checks: what was asked, where it
/// started, and what was kept, when it's committed.
async fn keep_task(app: &Shared, h: &Handover, debrief: Option<i64>, root: &Path) -> ApiResult<()> {
    let which = crate::reader::which(&h.change);
    let start = match which {
        Some(which) => {
            let root = root.to_path_buf();
            tokio::task::spawn_blocking(move || kelve_engine::git::start_of(&root, &which))
                .await
                .ok()
                .flatten()
                .unwrap_or_default()
        }
        None => String::new(),
    };
    if start.is_empty() {
        return Ok(());
    }
    let request = request_of(app, &h.session).await?;
    let mut conn = app.db.acquire().await?;
    store::keep_task(
        &mut conn,
        &NewTask {
            request: &request,
            root: &h.root,
            start: &start,
            kept: (!h.uncommitted && !h.head.is_empty()).then_some(h.head.as_str()),
            handover: Some(h.id),
            debrief,
        },
    )
    .await?;
    Ok(())
}

/// What the person asked for in this piece of work: their messages in the
/// session since its last debrief was sent.
async fn request_of(app: &Shared, session: &str) -> ApiResult<String> {
    if session.is_empty() {
        return Ok(String::new());
    }
    let mut conn = app.db.acquire().await?;
    let since = store::last_sent(&mut conn, session).await?.unwrap_or(0);
    let said = handovers::messages(&mut conn, session, since).await?;
    Ok(said
        .into_iter()
        .map(|m| m.text)
        .collect::<Vec<_>>()
        .join("\n\n"))
}

/// The hook says which session opened what was handed over: the debrief and
/// the past task are that session's, with what the person asked in it.
pub async fn in_session(app: &Shared, opened: i64, session: &str) -> ApiResult<()> {
    let handovers: Vec<(i64,)> =
        sqlx::query_as("select id from handovers where opened = ? and session = ?")
            .bind(opened)
            .bind(session)
            .fetch_all(&mut *app.db.acquire().await?)
            .await?;
    let request = request_of(app, session).await?;
    let mut conn = app.db.acquire().await?;
    for (h,) in handovers {
        if let Some(d) = store::debrief_of_handover(&mut conn, h).await? {
            store::debrief_session(&mut conn, d, session).await?;
            if !request.is_empty() {
                sqlx::query("update past_tasks set request = ? where debrief = ? and request = ''")
                    .bind(kelve_core::text::clip(
                        &kelve_core::redact::redact(&request),
                        kelve_store::checks::REQUEST,
                    ))
                    .bind(d)
                    .execute(&mut *conn)
                    .await?;
            }
        } else if !request.is_empty() {
            sqlx::query("update past_tasks set request = ? where handover = ? and request = ''")
                .bind(kelve_core::text::clip(
                    &kelve_core::redact::redact(&request),
                    kelve_store::checks::REQUEST,
                ))
                .bind(h)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// Text of a file in the project, read with its secrets masked, clipped.
fn project_file(root: &Path, rel: &str) -> Option<String> {
    let read = kelve_engine::read::read(root, rel).ok()?;
    let text = read.text.trim().to_string();
    (!text.is_empty()).then(|| kelve_core::text::clip(&text, SKILL))
}

/// The person's skills that applied to the work: Everywhere, the skill for
/// each kind of work its files are, and the project's, whole.
pub fn skills_for(app: &Shared, root: &str, paths: &[&str]) -> String {
    let kinds: Vec<&str> = paths
        .iter()
        .filter_map(|p| kind_of(p))
        .map(|k| k.name())
        .collect();
    let mut out = String::new();
    for s in files::all(app, Some(root)) {
        let applies =
            !s.root.is_empty() || s.name == "everywhere" || kinds.contains(&s.name.as_str());
        if !applies || out.len() >= SKILLS {
            continue;
        }
        out.push_str(&fence_with(
            "skill",
            &[("name", &s.name)],
            &kelve_core::text::clip(&kelve_core::redact::redact(&s.text), SKILL),
        ));
        out.push('\n');
    }
    out
}

/// The job's material: the change as it was handed over, what was said,
/// the skills that applied, the project's instructions, and the debrief so
/// far when it's brought up to date.
async fn material(app: &Shared, d: &debriefs::Debrief, h: &Handover) -> ApiResult<Value> {
    let handed = {
        let mut conn = app.db.acquire().await?;
        handovers::files(&mut conn, h.id).await?
    };
    let root = Path::new(&h.root).to_path_buf();
    let paths: Vec<&str> = handed.iter().map(|f| f.path.as_str()).collect();
    let instructions: Vec<String> = INSTRUCTIONS
        .iter()
        .filter_map(|rel| {
            project_file(&root, rel).map(|t| fence_with("instructions", &[("file", rel)], &t))
        })
        .collect();
    let said = request_of(app, &h.session).await?;
    let (added, removed): (i64, i64) = handed
        .iter()
        .fold((0, 0), |(a, r), f| (a + f.added, r + f.removed));
    Ok(json!({
        "work": {
            "title": h.title,
            "files": handed.len(),
            "added": added,
            "removed": removed,
            "cut": h.cut,
        },
        "youSaid": (!h.said.is_empty()).then(|| fence("said", &h.said)),
        "change": fence_with("change", &[("shown", &h.change)], &change_text(&handed, CHANGE)),
        "said": (!said.is_empty()).then(|| fence("said", &said)),
        "skills": Some(skills_for(app, &h.root, &paths)).filter(|s| !s.is_empty()),
        "instructions": (!instructions.is_empty()).then(|| instructions.join("\n")),
        "debrief": d.markdown.clone().filter(|_| d.state != "sent").map(|m| fence("debrief", &m)),
        // kelve's own, to check the answer: never given to the agent.
        "changed": changed_of(&handed),
    }))
}

/// The ask's key: the debrief and the handover it's of, so more on the same
/// work asks afresh.
fn key(debrief: i64, handover: i64) -> String {
    format!("debrief:{debrief}:{handover}")
}

/// Asks the agent for a debrief, or, once it's handed in, keeps it.
pub async fn ask(app: &Shared, debrief: i64) -> pipeline::Result<Then> {
    let (d, h) = {
        let mut conn = app.db.acquire().await?;
        let Some(d) = debriefs::get(&mut conn, debrief).await? else {
            return Ok(Then::Done);
        };
        let Some(h) = (match d.handover {
            Some(id) => handovers::get(&mut conn, id).await?,
            None => None,
        }) else {
            return Ok(Then::Done);
        };
        (d, h)
    };
    if d.state == "sent" {
        return Ok(Then::Done);
    }
    let key = key(d.id, h.id);
    // Asks for this debrief of earlier handovers of the work are let go.
    {
        let mut conn = app.db.acquire().await?;
        let older: Vec<(i64,)> = sqlx::query_as(
            "select id from asks where key like ? and key != ? and state in ('waiting', 'taken')",
        )
        .bind(format!("debrief:{}:%", d.id))
        .bind(&key)
        .fetch_all(&mut *conn)
        .await?;
        for (id,) in older {
            asks::drop(&mut conn, id).await?;
        }
    }
    let material = material(app, &d, &h).await.map_err(fail)?;
    let title = if h.title.is_empty() {
        "the work"
    } else {
        &h.title
    };
    let answer = agent::ask(
        app,
        &key,
        Ask {
            kind: kelve_core::instructions::DEBRIEF,
            about: format!("Debriefing {title}"),
            idea: None,
            opened: h.opened,
            material,
            schema: schema(),
            priority: Priority::Now,
            follows: None,
        },
        Resume::job(
            v2jobs::DEBRIEF,
            format!("{}:{}", v2jobs::DEBRIEF, d.id),
            json!({ "debrief": d.id, "opened": h.opened }),
        ),
    )
    .await?;
    let Some(answer) = answer else {
        // Written now, while the agent remembers why: ahead of anything
        // else waiting, as Do next puts a job.
        let mut conn = app.db.acquire().await?;
        if let Some(row) = asks::by_key(&mut conn, &key)
            .await?
            .filter(|r| r.state == "waiting")
        {
            asks::first(&mut conn, &[row.id]).await?;
        }
        return Ok(Then::Done);
    };
    keep(app, &d, &h, &answer.value, &answer.material).await?;
    Ok(Then::Done)
}

/// Keeps the agent's debrief: its text, its size, and its decisions, each
/// keeping the person's call when it's the same decision as before.
async fn keep(
    app: &Shared,
    d: &debriefs::Debrief,
    h: &Handover,
    value: &Value,
    material: &Value,
) -> pipeline::Result<()> {
    let text = value["debrief"].as_str().unwrap_or_default();
    let changed: Vec<Changed> =
        serde_json::from_value(material["changed"].clone()).unwrap_or_default();
    let read = match debrief::read(text, &changed) {
        Ok(read) => read,
        Err(problems) => {
            // Checked as it was handed in: only a change to the checks gets here.
            let mut conn = app.db.acquire().await?;
            store::debrief_failed(&mut conn, d.id, &problems.join(" ")).await?;
            return Ok(());
        }
    };
    let lines = h.added + h.removed;
    let large = h.files as usize > LARGE_FILES
        || lines > LARGE_LINES
        || read.decisions.iter().any(|x| x.part.is_some());
    let decisions: Vec<(String, String, String, String)> = read
        .decisions
        .iter()
        .map(|x| {
            let origin = match x.from {
                debrief::Origin::Own => "own",
                debrief::Origin::Said => "said",
                debrief::Origin::Edit => "edit",
            };
            let lines: Vec<Value> = x
                .lines
                .iter()
                .flat_map(|l| {
                    if l.ranges.is_empty() {
                        vec![json!({ "path": l.path })]
                    } else {
                        l.ranges
                            .iter()
                            .map(|r| json!({ "path": l.path, "from": r.from, "to": r.to }))
                            .collect()
                    }
                })
                .collect();
            (
                x.title.clone(),
                origin.to_string(),
                Value::Array(lines).to_string(),
                x.part.clone().unwrap_or_default(),
            )
        })
        .collect();
    let mut conn = app.db.acquire().await?;
    debriefs::written(
        &mut conn,
        d.id,
        &read.title,
        if large { "large" } else { "small" },
        text,
    )
    .await?;
    let new: Vec<debriefs::NewDecision<'_>> = decisions
        .iter()
        .enumerate()
        .map(|(n, (title, origin, lines, part))| debriefs::NewDecision {
            number: n as i64 + 1,
            title,
            origin,
            lines,
            part,
        })
        .collect();
    debriefs::keep_decisions(&mut conn, d.id, &new).await?;
    drop(conn);
    app.publish(kelve_core::views::ServerEvent::Asks);
    Ok(())
}

/// The pipeline job: asks for the debrief its payload names, or keeps it.
pub async fn job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    match payload["debrief"].as_i64() {
        Some(id) => ask(app, id).await,
        None => Ok(Then::Done),
    }
}

/// Queues a debrief's job, to run now.
pub async fn queue(app: &Shared, debrief: i64, opened: Option<i64>) -> pipeline::Result<()> {
    pipeline::add(
        app,
        v2jobs::DEBRIEF,
        Some(&format!("{}:{debrief}", v2jobs::DEBRIEF)),
        json!({ "debrief": debrief, "opened": opened }),
        Utc::now(),
    )
    .await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn handed(path: &str, status: &str, hunks: Value) -> HandedFile {
        HandedFile {
            path: path.into(),
            status: status.into(),
            moved_from: None,
            skip: None,
            added: 2,
            removed: 1,
            hunks: hunks.to_string(),
            text: None,
        }
    }

    #[test]
    fn a_handovers_lines_are_read_as_the_debrief_is_checked() {
        let hunks = json!([{ "lines": [
            { "kind": "context", "old": 1, "new": 1, "text": "fn a() {" },
            { "kind": "removed", "old": 2, "new": null, "text": "  old()" },
            { "kind": "added", "old": null, "new": 2, "text": "  new()" },
            { "kind": "added", "old": null, "new": 3, "text": "  more()" },
            { "kind": "context", "old": 3, "new": 4, "text": "}" },
        ]}]);
        let files = [handed("src/a.rs", "modified", hunks)];
        let changed = changed_of(&files);
        assert_eq!(changed[0].ranges, vec![Range::new(2, 3)]);
        let text = change_text(&files, CHANGE);
        assert!(text.contains("--- src/a.rs (modified, +2 -1)"), "{text}");
        assert!(text.contains("    2 +   new()"), "{text}");
        assert!(text.contains("  (2) -   old()"), "{text}");
        let due = due_files(&files);
        assert_eq!(due[0].status, due::Status::Modified);
        assert!(!due[0].whitespace_only);
    }

    #[test]
    fn a_debrief_whose_lines_arent_in_the_change_goes_back() {
        let material =
            json!({ "changed": [{ "path": "src/a.rs", "ranges": [{ "from": 2, "to": 3 }] }] });
        let text = |lines: &str| json!({ "debrief": format!("---\nkelve: debrief/1\ntitle: New calls\n---\n\n## What it did\n\nIt calls new.\n\n## How it works\n\n`a()` calls `new()`.\n\n## Decisions\n\n### Call new, not old\n- from: its own call\n- lines: {lines}\n- could have: kept old\n- rests on: Refactoring\n\nOld is going.\n\n## What it rests on\n\n- **Refactoring:** changing code without changing what it does.\n") });
        assert_eq!(shape(&material, &text("src/a.rs 2-3")), Ok(()));
        let wrong = shape(&material, &text("src/a.rs 40-41")).unwrap_err();
        assert!(wrong.starts_with("Fix the debrief"), "{wrong}");
    }
}
