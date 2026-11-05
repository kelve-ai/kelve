//! Checking each part of the person's skills on their own past work, and Fits
//! you (docs/skills.md). kelve never starts the agent: it keeps a `compare`
//! job for it, and the agent redoes a past task twice, as two subagents in
//! fresh worktrees, and hands in both changes. For a part, one run has the
//! skill with the part and the other without it; for Fits you, one has every
//! skill and the other none. What the answer means is kelve's to say, in
//! `core::skills::compare`: how close each run came to what the person kept,
//! and whether the part works, makes no difference or failed.
//!
//! Checks cost runs of the agent, so they're batched, with a rough count of
//! the runs first (`offer`), and run as the person set them in settings:
//! when they agree to each, by themselves for new and changed parts when the
//! agent is free (`auto`), or never; and never more runs a week than they
//! allow.

use std::path::Path;

use kelve_core::fence::fence_with;
use kelve_core::instructions::COMPARE;
use kelve_core::skills::compare::{self, Line, Outcome};
use kelve_core::skills::{PartKey, Skill};
use kelve_core::views::{CheckOffer, CheckRun, ChecksWhen, OfferedPart, PartCheck, ServerEvent};
use kelve_store::checks::{self, PastTask};
use kelve_store::{asks, checking, skills};
use serde_json::{Value, json};

use crate::agent::{self, Ask, Priority, Resume};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{self, Then};
use crate::skills_home;
use crate::skills_sync::dir_of;
use crate::state::Shared;
use crate::v2jobs::{self, CHECK};

/// Redo a past task for Fits you: with every skill, and with none.
pub const FITS: &str = "v2.fits";
/// The past tasks each part is redone on.
pub const TASKS: i64 = 2;
/// The past tasks Fits you redoes at once.
pub const PAIRS: i64 = 3;

/// How lately an agent with no hook must have called kelve to count as one
/// that could take a check: a week.
const AGENT_LATELY_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// The ask's key for a part's check on one past task.
fn check_key(check: i64) -> String {
    format!("compare:check:{check}")
}

/// The ask's key for one Fits you pair.
fn fits_key(pair: i64) -> String {
    format!("compare:fits:{pair}")
}

/// Whether a `compare` answer is in the shape the job asked for: why not,
/// in a sentence the agent can act on.
pub fn shape(material: &Value, value: &Value) -> Result<(), String> {
    compare::read(value, material.get("part").is_some()).map(|_| ())
}

/// The lines the person kept of a past task: from the commit it started on
/// to what they kept. None when that can't be read, such as when the
/// project has moved.
async fn kept_lines(task: &PastTask) -> Option<Vec<Line>> {
    let kept = task.kept.clone()?;
    let (root, start) = (task.root.clone(), task.start.clone());
    let change = tokio::task::spawn_blocking(move || {
        kelve_engine::git::between_commits(Path::new(&root), &start, &kept)
    })
    .await
    .ok()?;
    match change {
        Ok(change) => Some(
            change
                .files
                .iter()
                .flat_map(|f| {
                    f.hunks.iter().flat_map(move |h| {
                        h.lines.iter().filter_map(move |l| {
                            let added = match l.kind {
                                kelve_engine::git::LineKind::Added => true,
                                kelve_engine::git::LineKind::Removed => false,
                                kelve_engine::git::LineKind::Context => return None,
                            };
                            (!l.text.trim().is_empty()).then(|| Line::new(&f.path, added, &l.text))
                        })
                    })
                })
                .collect(),
        ),
        Err(e) => {
            tracing::info!(error = %e, task = task.id, "couldn't read what was kept of a past task");
            None
        }
    }
}

/// Checks are started one at a time, so two never take the same runs.
static STARTING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Said where a check would be offered, when checks are off.
const OFF: &str =
    "Checks are off. To check new parts, turn them on in Settings, under Skill checks.";

/// Runs of the agent the person's checks have used this week: two for each
/// check asked for, but for those stopped before they were done.
pub async fn runs_this_week(app: &Shared) -> ApiResult<u32> {
    let mut conn = app.db.acquire().await?;
    let checks =
        kelve_store::your_agent::checks_since(&mut conn, crate::your_agent::week_start(app))
            .await?;
    Ok(u32::try_from(checks.saturating_mul(2)).unwrap_or(u32::MAX))
}

/// Runs of the agent left this week, of the most the person allows.
pub(crate) async fn runs_left(app: &Shared) -> ApiResult<u32> {
    let most = app.settings().checks_a_week;
    Ok(most.saturating_sub(runs_this_week(app).await?))
}

/// Why parts not checked yet wait for this week's runs, in a sentence: none
/// of them fit, or `fit` do and `waiting` more don't.
fn over_limit(fit: usize, waiting: usize) -> String {
    let raise = "unless you raise the limit in Settings, under Skill checks.";
    match (fit, waiting) {
        (0, _) => format!("This week’s check runs are used. New parts wait till Monday, {raise}"),
        (fit, 1) => format!(
            "This week’s runs are enough for {}. One more waits till Monday, {raise}",
            parts_said(fit)
        ),
        (fit, n) => format!(
            "This week’s runs are enough for {}. {n} more wait till Monday, {raise}",
            parts_said(fit)
        ),
    }
}

fn parts_said(n: usize) -> String {
    if n == 1 {
        "one part".into()
    } else {
        format!("{n} parts")
    }
}

/// What a check of a skill's parts not checked yet would take: as many as
/// this week's runs allow, and why the rest wait.
pub async fn offer(app: &Shared, skill: &skills::Skill) -> ApiResult<CheckOffer> {
    let when = app.settings().checks;
    let left = runs_left(app).await?;
    let mut conn = app.db.acquire().await?;
    let mut parts: Vec<OfferedPart> = skills::parts(&mut conn, skill.id, false)
        .await?
        .into_iter()
        .filter(|p| p.state == "unchecked")
        .map(|p| OfferedPart {
            key: PartKey::new(&skill.name, &p.key).to_string(),
            heading: p.heading,
        })
        .collect();
    let tasks = checking::tasks_known(&mut conn).await?.min(TASKS) as u32;
    let fit = match when {
        ChecksWhen::Never => 0,
        _ if tasks == 0 => parts.len(),
        _ => (left / (2 * tasks)) as usize,
    };
    let waiting = parts.len().saturating_sub(fit);
    let held = (waiting > 0).then(|| match when {
        ChecksWhen::Never => OFF.to_string(),
        _ => over_limit(fit, waiting),
    });
    parts.truncate(fit);
    let runs = 2 * tasks * parts.len() as u32;
    Ok(CheckOffer {
        parts,
        tasks,
        runs,
        when,
        left,
        waiting: waiting as u32,
        held,
    })
}

/// Asks for each part's check on each past task, and keeps the agent's jobs
/// for them: how many checks.
async fn begin(app: &Shared, parts: &[skills::Part], tasks: &[PastTask]) -> ApiResult<usize> {
    let mut conn = app.db.acquire().await?;
    let batch = kelve_store::now();
    let mut ids = Vec::new();
    for part in parts {
        for task in tasks {
            ids.push(checking::ask_in(&mut conn, part.id, task.id, batch).await?);
        }
        skills::checked(&mut conn, part.id, "running", None, None).await?;
        skills::tried(&mut conn, part.id).await?;
    }
    drop(conn);
    for id in &ids {
        v2jobs::queue(app, CHECK, &id.to_string(), json!({ "check": id }))
            .await
            .map_err(ApiError::internal)?;
    }
    app.wake.notify_one();
    app.publish(ServerEvent::Skills);
    Ok(ids.len())
}

/// When checks run by themselves, checks each skill's new and changed parts
/// on past work, as many as this week's runs allow, for the agent to do when
/// it's free. A part whose check was stopped, or couldn't be done, waits for
/// the person till its text changes. How many runs it asked for.
pub async fn auto(app: &Shared) -> ApiResult<u32> {
    if app.settings().checks != ChecksWhen::WhenFree {
        return Ok(0);
    }
    let _one = STARTING.lock().await;
    let mut left = runs_left(app).await?;
    let mut conn = app.db.acquire().await?;
    let made = skills::all(&mut conn, "made").await?;
    drop(conn);
    let mut asked = 0;
    for skill in &made {
        let mut conn = app.db.acquire().await?;
        let mut parts: Vec<skills::Part> = skills::parts(&mut conn, skill.id, false)
            .await?
            .into_iter()
            .filter(|p| p.state == "unchecked" && p.tried.as_deref() != Some(p.body_hash.as_str()))
            .collect();
        if parts.is_empty() {
            continue;
        }
        let root = (!skill.root.is_empty()).then_some(skill.root.as_str());
        let tasks = checking::tasks_for(&mut conn, root, TASKS).await?;
        drop(conn);
        if tasks.is_empty() {
            continue;
        }
        let each = 2 * tasks.len() as u32;
        parts.truncate((left / each) as usize);
        if parts.is_empty() {
            break;
        }
        let runs = 2 * begin(app, &parts, &tasks).await? as u32;
        left = left.saturating_sub(runs);
        asked += runs;
    }
    Ok(asked)
}

/// The parts of a skill an agent really is checking now: a job an agent took
/// and stopped answering doesn't count.
pub async fn being_done(conn: &mut kelve_store::Conn, skill: i64) -> kelve_store::Result<Vec<i64>> {
    asks::let_go_of_lapsed(conn).await?;
    checking::being_done(conn, skill).await
}

/// A skill's check, while it's asked for: how far it's got, and whether an
/// agent is doing it, or could.
pub async fn running(app: &Shared, skill: i64) -> ApiResult<Option<CheckRun>> {
    let mut conn = app.db.acquire().await?;
    let Some((parts, done, all)) = checking::running_for(&mut conn, skill).await? else {
        return Ok(None);
    };
    let waiting = being_done(&mut conn, skill).await?.is_empty();
    Ok(Some(CheckRun {
        parts: parts as u32,
        done: 2 * done as u32,
        runs: 2 * all as u32,
        waiting,
        agent: an_agent_could(app).await?,
    }))
}

/// Whether any agent could take a job: one has kelve's hook, or called kelve
/// lately.
async fn an_agent_could(app: &Shared) -> ApiResult<bool> {
    let mut conn = app.db.acquire().await?;
    let seen = asks::latest_agent(&mut conn)
        .await?
        .is_some_and(|a| kelve_store::now() - a.last_seen_at < AGENT_LATELY_MS);
    Ok(seen || !crate::your_agent::connected(app).is_empty())
}

/// Starts checking a skill's parts not checked yet, or those of them named
/// by `keys`, each on the latest past tasks, as many as this week's runs
/// allow: the check as it starts.
pub async fn start(app: &Shared, skill: &skills::Skill, keys: &[String]) -> ApiResult<CheckRun> {
    if app.settings().checks == ChecksWhen::Never {
        return Err(ApiError::conflict(OFF));
    }
    let _one = STARTING.lock().await;
    let left = runs_left(app).await?;
    let mut conn = app.db.acquire().await?;
    let mut parts: Vec<skills::Part> = skills::parts(&mut conn, skill.id, false)
        .await?
        .into_iter()
        .filter(|p| p.state == "unchecked")
        .filter(|p| {
            keys.is_empty()
                || keys
                    .iter()
                    .any(|k| *k == p.key || *k == PartKey::new(&skill.name, &p.key).to_string())
        })
        .collect();
    if parts.is_empty() {
        return Err(ApiError::conflict(
            "Every part of it is checked already. Change a part, or add one, to check it.",
        ));
    }
    let root = (!skill.root.is_empty()).then_some(skill.root.as_str());
    let tasks = checking::tasks_for(&mut conn, root, TASKS).await?;
    if tasks.is_empty() {
        return Err(ApiError::conflict(
            "There’s no past work to check it on yet. Once your agent hands over work and you keep it, kelve can check each part on it.",
        ));
    }
    drop(conn);
    let fit = (left / (2 * tasks.len() as u32)) as usize;
    if fit == 0 {
        return Err(ApiError::conflict(over_limit(0, parts.len())));
    }
    parts.truncate(fit);
    let asked = begin(app, &parts, &tasks).await?;
    Ok(CheckRun {
        parts: parts.len() as u32,
        done: 0,
        runs: 2 * asked as u32,
        waiting: true,
        agent: an_agent_could(app).await?,
    })
}

/// Stops a skill's check: what's not done yet is dropped, from kelve and from
/// the agent's list, and its parts aren't checked. How many checks stopped.
pub async fn stop(app: &Shared, skill: &skills::Skill) -> ApiResult<u64> {
    let mut conn = app.db.acquire().await?;
    let open = checking::open_checks(&mut conn, skill.id).await?;
    let mut stopped = 0;
    let mut parts: Vec<i64> = open.iter().map(|(_, part)| *part).collect();
    parts.sort_unstable();
    parts.dedup();
    for (check, _) in &open {
        if let Some(ask) = asks::by_key(&mut conn, &check_key(*check)).await? {
            asks::drop(&mut conn, ask.id).await?;
        }
    }
    for part in parts {
        stopped += checks::stop(&mut conn, part).await?;
        settle(&mut conn, part).await?;
    }
    drop(conn);
    app.publish(ServerEvent::Skills);
    app.publish(ServerEvent::Asks);
    Ok(stopped)
}

/// A part's state, once every check in its latest start is done: what they
/// add up to, or not checked when none could be done.
async fn settle(conn: &mut kelve_store::Conn, part: i64) -> kelve_store::Result<()> {
    let Some(verdicts) = checking::latest_verdicts(conn, part).await? else {
        return Ok(());
    };
    let outcomes: Vec<Outcome> = verdicts.iter().filter_map(|v| Outcome::of(v)).collect();
    match compare::overall(&outcomes) {
        None => skills::checked(conn, part, "unchecked", None, None).await,
        Some((check, hint)) => {
            let wording = if check == PartCheck::Failed {
                checking::failures(conn, part)
                    .await?
                    .into_iter()
                    .find_map(|(_, w)| w)
            } else {
                None
            };
            let state = match check {
                PartCheck::Works => "works",
                PartCheck::NoDifference => "no_difference",
                PartCheck::Failed => "failed",
                PartCheck::NotChecked | PartCheck::Waiting | PartCheck::Checking => "unchecked",
            };
            skills::checked(conn, part, state, Some(&hint), wording.as_deref()).await
        }
    }
}

/// The past task as the agent is given it: what was asked, where, and from
/// which commit.
fn task_material(task: &PastTask) -> Value {
    json!({
        "request": fence_with("request", &[], &task.request),
        "project": task.root,
        "start": task.start,
    })
}

/// Checks a part on one past task: keeps the `compare` job for the agent,
/// and, once it's answered, what it found.
pub async fn check_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let Some(id) = payload["check"].as_i64() else {
        return Ok(Then::Done);
    };
    let mut conn = app.db.acquire().await?;
    let Some(check) = checks::check(&mut conn, id).await? else {
        return Ok(Then::Done);
    };
    if check.state != "waiting" && check.state != "running" {
        return Ok(Then::Done);
    }
    let part = skills::part(&mut conn, check.part).await?;
    let skill = match &part {
        Some(p) => skills::get(&mut conn, p.skill).await?,
        None => None,
    };
    let task = checks::task(&mut conn, check.task).await?;
    let (Some(part), Some(skill), Some(task)) = (part, skill, task) else {
        checks::check_failed(&mut conn, id, "What it was checking is gone.").await?;
        return Ok(Then::Done);
    };
    drop(conn);
    let text = skills_home::read(&dir_of(app, &skill.root), &skill.name)
        .ok()
        .flatten();
    let found = text
        .as_deref()
        .and_then(|t| Skill::parse(t).part(&part.heading).map(|p| (t, p)));
    let (Some((text, written)), None) = (found, part.removed_at) else {
        let mut conn = app.db.acquire().await?;
        checks::check_failed(
            &mut conn,
            id,
            "The part was changed or taken out before it was checked.",
        )
        .await?;
        settle(&mut conn, part.id).await?;
        drop(conn);
        app.publish(ServerEvent::Skills);
        return Ok(Then::Done);
    };
    let without = compare::without_part(text, &part.heading).unwrap_or_default();
    let mut material = task_material(&task);
    material["skill"] = json!(skill.name);
    material["with"] = json!(fence_with("skill", &[("name", &skill.name)], text));
    material["without"] = json!(fence_with("skill", &[("name", &skill.name)], &without));
    material["part"] = json!(fence_with("part", &[], &written.markdown));
    let key = check_key(id);
    let asked = agent::ask(
        app,
        &key,
        Ask {
            kind: COMPARE,
            about: format!("Checking “{}” on past work", part.heading),
            idea: None,
            opened: None,
            material,
            schema: compare::schema(true),
            priority: Priority::Rest,
            follows: None,
        },
        Resume::job(CHECK, id.to_string(), payload.clone()),
    )
    .await?;
    let Some(answer) = asked else {
        let mut conn = app.db.acquire().await?;
        checks::running(&mut conn, id).await?;
        return Ok(Then::Done);
    };
    let answer = match compare::read(&answer.value, true) {
        Ok(a) => a,
        Err(why) => {
            let mut conn = app.db.acquire().await?;
            checks::check_failed(&mut conn, id, &why).await?;
            settle(&mut conn, part.id).await?;
            return Ok(Then::Done);
        }
    };
    let kept = kept_lines(&task).await;
    let verdict = compare::verdict(kept.as_deref(), &answer);
    let wording = (verdict.outcome == Outcome::Failed)
        .then(|| answer.wording.as_deref().map(str::trim))
        .flatten();
    let mut conn = app.db.acquire().await?;
    checks::check_done(
        &mut conn,
        id,
        verdict.outcome.name(),
        &verdict.what,
        wording,
    )
    .await?;
    if let (Some(w), Some(wo)) = (verdict.with, verdict.without) {
        checking::check_close(&mut conn, id, w.score, wo.score).await?;
    }
    settle(&mut conn, part.id).await?;
    drop(conn);
    app.publish(ServerEvent::Skills);
    Ok(Then::Done)
}

/// Starts Fits you on past tasks not redone yet, a few at once: each is
/// redone with the person's skills and without, and shown side by side,
/// their sides chosen at random. How many it started.
pub async fn fits_start(app: &Shared) -> ApiResult<u32> {
    let mut conn = app.db.acquire().await?;
    let paired = checking::tasks_paired(&mut conn).await?;
    let tasks: Vec<PastTask> = checking::tasks_for(&mut conn, None, 50)
        .await?
        .into_iter()
        .filter(|t| !paired.contains(&t.id))
        .take(PAIRS as usize)
        .collect();
    if tasks.is_empty() {
        return Err(ApiError::conflict(
            "There’s no past work left to redo. Once your agent hands over more work and you keep it, there will be.",
        ));
    }
    let mut ids = Vec::new();
    for task in &tasks {
        let side = if crate::crypto::token()
            .bytes()
            .next()
            .is_some_and(|b| b.is_multiple_of(2))
        {
            "left"
        } else {
            "right"
        };
        ids.push(checks::add_pick(&mut conn, task.id, side).await?);
    }
    drop(conn);
    for id in &ids {
        v2jobs::queue(app, FITS, &id.to_string(), json!({ "pair": id }))
            .await
            .map_err(ApiError::internal)?;
    }
    app.wake.notify_one();
    app.publish(ServerEvent::Skills);
    Ok(ids.len() as u32)
}

/// Every skill the person's agent has on a past task's project: their own,
/// and the project's, each fenced with its name.
async fn all_skills(app: &Shared, root: &str) -> pipeline::Result<String> {
    crate::skills_sync::sync(app).await?;
    let mut conn = app.db.acquire().await?;
    let made = skills::all(&mut conn, "made").await?;
    drop(conn);
    let mut out = Vec::new();
    for s in made.iter().filter(|s| s.root.is_empty() || s.root == root) {
        if let Ok(Some(text)) = skills_home::read(&dir_of(app, &s.root), &s.name) {
            out.push(fence_with("skill", &[("name", &s.name)], &text));
        }
    }
    Ok(out.join("\n\n"))
}

/// Redoes a past task for Fits you: keeps the `compare` job for the agent
/// and, once it's answered, both changes and how close each came.
pub async fn fits_job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let Some(id) = payload["pair"].as_i64() else {
        return Ok(Then::Done);
    };
    let mut conn = app.db.acquire().await?;
    let Some(pair) = checking::pair(&mut conn, id).await? else {
        return Ok(Then::Done);
    };
    if pair.skills_diff.is_some() || pair.picked.is_some() {
        return Ok(Then::Done);
    }
    let Some(task) = checks::task(&mut conn, pair.task).await? else {
        checking::forget_pair(&mut conn, id).await?;
        return Ok(Then::Done);
    };
    drop(conn);
    let mut material = task_material(&task);
    material["with"] = json!(all_skills(app, &task.root).await?);
    material["without"] = json!("");
    let asked = agent::ask(
        app,
        &fits_key(id),
        Ask {
            kind: COMPARE,
            about: "Redoing past work for Fits you".into(),
            idea: None,
            opened: None,
            material,
            schema: compare::schema(false),
            priority: Priority::Rest,
            follows: None,
        },
        Resume::job(FITS, id.to_string(), payload.clone()),
    )
    .await?;
    let Some(answer) = asked else {
        return Ok(Then::Done);
    };
    let mut conn = app.db.acquire().await?;
    let Ok(answer) = compare::read(&answer.value, false) else {
        checking::forget_pair(&mut conn, id).await?;
        return Ok(Then::Done);
    };
    let redact = kelve_core::redact::redact;
    checking::pair_diffs(
        &mut conn,
        id,
        &redact(&answer.with.diff),
        &redact(&answer.without.diff),
    )
    .await?;
    drop(conn);
    if let Some(kept) = kept_lines(&task).await {
        let skills = compare::alike(&compare::lines_of(&answer.with.diff), &kept);
        let standard = compare::alike(&compare::lines_of(&answer.without.diff), &kept);
        let mut conn = app.db.acquire().await?;
        checks::close(&mut conn, id, skills.score, standard.score).await?;
    }
    app.publish(ServerEvent::Skills);
    Ok(Then::Done)
}
