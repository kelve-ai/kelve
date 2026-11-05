//! The agent loop, end to end, with the stand-in agent: work handed over is
//! debriefed at once when it's worth it, as the person chose; a debrief
//! that fails its checks goes back once, then fails; research and
//! explanations are asked and kept; what the person corrects, changes after
//! handover (however they commit it) and sends from a debrief is learned,
//! written into their skills and can be undone; and their skills reach the
//! agent through the hook, once a session each.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use axum::Json;
use axum::extract::State;
use kelve_core::due::DebriefsWhen;
use kelve_server::agent::{self, Caller, HandedIn, Job};
use kelve_server::api::agent::{OpenIn, Opened, open};
use kelve_server::auth::Agent;
use kelve_server::moment::{MomentIn, moment};
use kelve_server::state::Shared;
use serde_json::{Value, json};

fn git(dir: &Path, args: &[&str]) {
    let ran = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "maintenance.auto=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        ran.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

async fn at(app: &Shared, m: MomentIn) -> String {
    moment(State(app.clone()), Agent, Json(m))
        .await
        .unwrap()
        .0
        .text
}

/// Retries, as the agent wrote them: more than twenty lines.
fn retry() -> String {
    let mut out = String::from(
        "/// Tries a call again while it might pass.\npub fn with_retries(tries: u32) -> bool {\n    tries < 5\n}\n\n",
    );
    for n in 0..24 {
        out.push_str(&format!("pub fn wait_{n}() -> u64 {{\n    1 << {n}\n}}\n"));
    }
    out
}

/// A repository with a ledger on main, and the agent's retries on a branch,
/// committed.
fn repo() -> std::path::PathBuf {
    let dir = common::folder(&[
        ("src/lib.rs", "pub mod retry;\n"),
        ("src/retry.rs", "pub fn with_retries() {}\n"),
        ("README.md", "# Ledger\n"),
    ]);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "Start"]);
    git(&dir, &["checkout", "-q", "-b", "retry"]);
    std::fs::write(dir.join("src/retry.rs"), retry()).unwrap();
    git(&dir, &["commit", "-qam", "Retry while it might pass"]);
    dir
}

async fn show(app: &Shared, dir: &Path, what: &str, extra: Value) -> Opened {
    let mut asked = json!({
        "what": what,
        "path": dir.display().to_string(),
        "base": "main",
        "title": "Retry failed calls",
        "said": "Calls retry while they might pass. I chose 5 tries over 3.",
        "agent": "stand-in-agent",
    });
    if let (Some(a), Some(e)) = (asked.as_object_mut(), extra.as_object()) {
        a.extend(e.clone());
    }
    let asked: OpenIn = serde_json::from_value(asked).unwrap();
    let opened = open(State(app.clone()), Agent, Json(asked))
        .await
        .unwrap()
        .0;
    // Reading the change, as kelve did before debriefs, isn't what's tested
    // here: it's taken off the list.
    let mut conn = app.db.acquire().await.unwrap();
    kelve_store::asks::drop_reading(&mut conn, opened.id)
        .await
        .unwrap();
    opened
}

fn only(id: i64) -> Caller {
    Caller {
        only: Some(id),
        ..common::caller()
    }
}

/// The agent loop's jobs: anything else waiting, as reading the change, is
/// taken off the list.
const LOOP: [&str; 4] = ["debrief", "research", "explain", "learn"];

/// The next of the loop's jobs for the agent, after the background work.
async fn next_loop(app: &Shared, caller: &Caller) -> Option<Job> {
    loop {
        common::jobs(app).await;
        let job = agent::next(app, caller).await.unwrap()?;
        if LOOP.contains(&job.kind.as_str()) {
            return Some(job);
        }
        agent::put_aside(app, job.job).await.unwrap();
    }
}

/// Does the loop's jobs with `answer` until there are none: those given.
async fn work(app: &Shared, answer: impl Fn(&Job) -> Value) -> Vec<Job> {
    let mut given = Vec::new();
    while let Some(job) = next_loop(app, &common::caller()).await {
        let handed = agent::hand_in(app, &common::caller(), job.job, answer(&job))
            .await
            .unwrap();
        assert_eq!(handed, HandedIn::Yes, "{} wasn't taken", job.about);
        given.push(job);
    }
    common::jobs(app).await;
    given
}

/// The first run of lines a file adds, as the job's change shows it.
fn first_added(job: &Job, path: &str) -> (u32, u32) {
    let change = job.material["change"].as_str().unwrap();
    let mut inside = false;
    let mut run: Option<(u32, u32)> = None;
    for line in change.lines() {
        if let Some(rest) = line.strip_prefix("--- ") {
            inside = rest.starts_with(path);
            if run.is_some() {
                break;
            }
            continue;
        }
        let added = line.get(6..8) == Some("+ ");
        let n: Option<u32> = line.get(..5).and_then(|n| n.trim().parse().ok());
        match (inside, added, n, run) {
            (true, true, Some(n), None) => run = Some((n, n)),
            (true, true, Some(n), Some((a, b))) if b + 1 == n => run = Some((a, n)),
            (_, _, _, Some(_)) if inside => break,
            _ => {}
        }
    }
    run.expect("an added run")
}

/// A debrief, as the stand-in agent writes it, with its decision on lines.
fn debrief(lines: &str) -> Value {
    json!({ "debrief": format!("---
kelve: debrief/1
title: Retry failed calls
---

## What it did

A call that fails because the network dropped now tries again, up to 5 times.

## How it works

`with_retries()`, in `src/retry.rs`, says whether to try again.

## Decisions

### Give up after 5 tries
- from: its own call
- lines: {lines}
- could have: 3 tries; no limit
- rests on: Exponential backoff

Five tries cover a minute of waiting, which most outages are shorter than.

## What it rests on

- **Exponential backoff:** waiting longer after each failure.
") })
}

#[tokio::test]
async fn work_worth_a_debrief_is_debriefed_at_once_and_a_wrong_one_goes_back_once() {
    let dir = repo();
    let app = common::kelve(None).await;
    let opened = show(&app, &dir, "worktree", json!({})).await;
    let handed = opened.debrief.clone().expect("a debrief is decided");
    assert!(handed.due, "{}", handed.reason);
    let id = handed.debrief.expect("asked for");

    // The agent takes it at once, by what it opened.
    let job = agent::next(&app, &only(opened.id)).await.unwrap().unwrap();
    assert_eq!(job.kind, "debrief");
    assert!(job.material.get("changed").is_none(), "kelve's own");
    assert!(
        job.material["change"]
            .as_str()
            .unwrap()
            .starts_with("<change")
    );
    let (from, to) = first_added(&job, "src/retry.rs");
    {
        let mut conn = app.db.acquire().await.unwrap();
        let d = kelve_store::debriefs::get(&mut conn, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(d.state, "writing");
    }

    // Lines that aren't in the change go back, with what to fix.
    let wrong = agent::hand_in(
        &app,
        &common::caller(),
        job.job,
        debrief("src/retry.rs 400-410"),
    )
    .await
    .unwrap();
    let HandedIn::No { why, put_aside } = wrong else {
        panic!("a wrong debrief was taken");
    };
    assert!(!put_aside);
    assert!(why.starts_with("Fix the debrief"), "{why}");
    // Fixed, it's kept, with its decision.
    let good = debrief(&format!("src/retry.rs {from}-{to}"));
    let taken = agent::hand_in(&app, &common::caller(), job.job, good)
        .await
        .unwrap();
    assert_eq!(taken, HandedIn::Yes);
    common::jobs(&app).await;
    let (d, decisions, task) = {
        let mut conn = app.db.acquire().await.unwrap();
        (
            kelve_store::debriefs::get(&mut conn, id)
                .await
                .unwrap()
                .unwrap(),
            kelve_store::debriefs::decisions(&mut conn, id)
                .await
                .unwrap(),
            kelve_store::agent_loop::task_of(&mut conn, id)
                .await
                .unwrap(),
        )
    };
    assert_eq!(
        (d.state.as_str(), d.title.as_str()),
        ("ready", "Retry failed calls")
    );
    assert_eq!(d.size, "small");
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].origin, "own");
    assert!(decisions[0].lines.contains("\"path\":\"src/retry.rs\""));
    let task = task.expect("a past task");
    assert_eq!(task.root, dir.display().to_string());
    assert!(!task.start.is_empty(), "where it started");
    assert!(task.kept.is_some(), "committed as handed over");

    // More on the same work brings the same debrief up to date.
    std::fs::write(dir.join("README.md"), "# Ledger\n\nRetries calls.\n").unwrap();
    let again = show(&app, &dir, "worktree", json!({})).await;
    let handed = again.debrief.unwrap();
    assert_eq!(handed.debrief, Some(id));
    assert!(handed.again);
    let job = agent::next(&app, &only(again.id)).await.unwrap().unwrap();
    assert!(
        job.material["debrief"]
            .as_str()
            .unwrap()
            .contains("Give up after 5 tries")
    );

    // Wrong twice, it fails, and home can ask again.
    for _ in 0..2 {
        agent::hand_in(&app, &common::caller(), job.job, json!({ "debrief": "no" }))
            .await
            .unwrap();
    }
    {
        let mut conn = app.db.acquire().await.unwrap();
        let d = kelve_store::debriefs::get(&mut conn, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(d.state, "failed");
        assert!(d.last_error.unwrap().contains("front matter"));
    }
    kelve_server::agent_loop::debrief::again(&app, id)
        .await
        .unwrap();
    let job = agent::next(&app, &only(again.id))
        .await
        .unwrap()
        .expect("asked again");
    assert_eq!(job.kind, "debrief");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn small_work_isnt_debriefed_unless_the_person_wants_every_one_or_asks() {
    let dir = common::folder(&[("README.md", "# Ledger\nA ledgr.\n")]);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "Start"]);
    std::fs::write(dir.join("README.md"), "# Ledger\nA ledger.\n").unwrap();
    let app = common::kelve(None).await;
    let opened = show(
        &app,
        &dir,
        "change",
        json!({ "base": null, "said": "Fixed a typo." }),
    )
    .await;
    let handed = opened.debrief.unwrap();
    assert!(!handed.due, "a typo isn't worth one");
    assert_eq!(handed.debrief, None);
    assert!(
        agent::next(&app, &only(opened.id))
            .await
            .unwrap()
            .is_none_or(|j| j.kind != "debrief")
    );

    // Only when they ask: asked, it's due.
    let mut settings = (*app.settings()).clone();
    settings.debriefs = DebriefsWhen::OnlyAsked;
    app.save_settings(settings).await.unwrap();
    let asked = show(
        &app,
        &dir,
        "change",
        json!({ "base": null, "debrief": true }),
    )
    .await;
    let handed = asked.debrief.unwrap();
    assert!(handed.due);
    assert_eq!(handed.reason, "You asked for one.");
    let job = agent::next(&app, &only(asked.id)).await.unwrap().unwrap();
    assert_eq!(job.kind, "debrief");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hands in the debrief job waiting for what was opened, written on the
/// change's own lines: the debrief's id.
async fn debriefed(app: &Shared, opened: &Opened) -> i64 {
    let job = next_loop(app, &only(opened.id)).await.unwrap();
    let (from, to) = first_added(&job, "src/retry.rs");
    let taken = agent::hand_in(
        app,
        &common::caller(),
        job.job,
        debrief(&format!("src/retry.rs {from}-{to}")),
    )
    .await
    .unwrap();
    assert_eq!(taken, HandedIn::Yes);
    common::jobs(app).await;
    opened.debrief.as_ref().unwrap().debrief.unwrap()
}

#[tokio::test]
async fn research_and_explanations_are_asked_and_kept() {
    let dir = repo();
    let app = common::kelve(None).await;
    let opened = show(&app, &dir, "worktree", json!({})).await;
    let id = debriefed(&app, &opened).await;
    let decision = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::debriefs::decisions(&mut conn, id)
            .await
            .unwrap()[0]
            .id
    };
    let research =
        kelve_server::agent_loop::research::ask_for(&app, decision, "Is 5 tries too many?")
            .await
            .unwrap();
    let explanation = kelve_server::agent_loop::explain::ask_for(&app, id, "Exponential backoff")
        .await
        .unwrap();
    let given = work(&app, |job| match job.kind.as_str() {
        "research" => {
            assert!(job.material["question"].as_str().unwrap().contains("Is 5 tries too many?"));
            assert!(job.material["lines"].as_str().unwrap().contains("with_retries"));
            json!({ "research": "#### What your agent found\n\nFive is right for a minute of waiting.\n\n| Option | For | Against |\n|---|---|---|\n| **Keep 5** | Covers a minute | Slower to fail |\n| Use 3 | Fails sooner | Misses longer outages |\n\n**It suggests** keeping 5 tries.\n\nSources: [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110)\n" })
        }
        "explain" => {
            assert_eq!(job.material["idea"], "Exponential backoff");
            json!({ "explanation": "Exponential backoff waits longer after each failure. Here, `wait_3()` waits 8 seconds, twice `wait_2()`, so a busy server gets room to recover." })
        }
        other => panic!("a {other} job"),
    })
    .await;
    assert_eq!(given.len(), 2);
    let mut conn = app.db.acquire().await.unwrap();
    let r = kelve_store::agent_loop::research(&mut conn, research)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.state, "ready");
    assert!(r.answer.unwrap().contains("It suggests"));
    let e = kelve_store::agent_loop::explanation(&mut conn, explanation)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(e.state, "ready");
    // Asked again for the same lines, it's the same explanation.
    drop(conn);
    let same = kelve_server::agent_loop::explain::ask_for(&app, id, "exponential  backoff")
        .await
        .unwrap();
    assert_eq!(same, explanation);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The learn job's answer, as the stand-in agent writes it: a part for the
/// first skill it was given.
fn learned(job: &Job, heading: &str) -> Value {
    let skill = job.material["skills"][0]["name"].as_str().unwrap();
    json!({
        "additions": [{
            "skill": skill,
            "section": "howYouLikeIt",
            "heading": heading,
            "body": "So a call that will never pass fails at once.",
            "evidence": "They changed 5 tries to 3.",
        }],
        "same": [],
        "disagrees": [],
    })
}

#[tokio::test]
async fn what_the_person_changes_and_sends_goes_into_their_skills_and_can_be_undone() {
    let dir = repo();
    let dir_s = dir.display().to_string();
    let app = common::kelve(None).await;
    let opened = show(&app, &dir, "worktree", json!({})).await;
    let id = debriefed(&app, &opened).await;
    at(
        &app,
        MomentIn {
            what: "opened".into(),
            session: "s".into(),
            cwd: dir_s.clone(),
            opened: Some(opened.id),
            ..MomentIn::default()
        },
    )
    .await;

    // The person changes the agent's work in their own terminal and commits:
    // kelve sees it the next time it hears from the agent.
    std::fs::write(
        dir.join("src/retry.rs"),
        retry().replace("tries < 5", "tries < 3"),
    )
    .unwrap();
    git(&dir, &["commit", "-qam", "Three tries"]);
    at(
        &app,
        MomentIn {
            what: "prompt".into(),
            session: "s2".into(),
            cwd: dir_s.clone(),
            prompt: Some("What's next?".into()),
            ..MomentIn::default()
        },
    )
    .await;
    common::jobs(&app).await;
    let job = next_loop(&app, &common::caller())
        .await
        .expect("a learn job");
    assert_eq!(job.kind, "learn");
    let edit = job.material["edit"].as_str().unwrap();
    assert!(edit.contains("+    tries < 3"), "{edit}");
    assert_eq!(
        job.material["skills"][0]["name"], "rust",
        "the kind it was caught in"
    );
    assert!(job.material.get("given").is_none(), "kelve's own");
    let taken = agent::hand_in(
        &app,
        &common::caller(),
        job.job,
        learned(&job, "Give up after 3 tries"),
    )
    .await
    .unwrap();
    assert_eq!(taken, HandedIn::Yes);
    common::jobs(&app).await;
    let proposed = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::candidates::in_state(&mut conn, "proposed")
            .await
            .unwrap()
    };
    assert_eq!(proposed.len(), 1);
    assert_eq!(proposed[0].heading, "Give up after 3 tries");
    assert_eq!(proposed[0].work, "rust");
    {
        let mut conn = app.db.acquire().await.unwrap();
        let task = kelve_store::agent_loop::task_of(&mut conn, id)
            .await
            .unwrap()
            .unwrap();
        let head = kelve_engine::git::head(&dir, "HEAD").unwrap().0;
        assert_eq!(
            task.kept.as_deref(),
            Some(head.as_str()),
            "what the person kept"
        );
    }

    // The person sends the debrief, wanting its decision every time: what
    // they agreed goes into their skills, in place.
    {
        let mut conn = app.db.acquire().await.unwrap();
        let decision = kelve_store::debriefs::decisions(&mut conn, id)
            .await
            .unwrap()[0]
            .id;
        kelve_store::debriefs::choose(&mut conn, decision, "agree", None, None)
            .await
            .unwrap();
        sqlx::query("update decisions set every_time = 1 where id = ?")
            .bind(decision)
            .execute(&mut *conn)
            .await
            .unwrap();
        kelve_store::agent_loop::show_in(&mut conn, proposed[0].id, id)
            .await
            .unwrap();
        assert!(kelve_store::debriefs::sent(&mut conn, id).await.unwrap());
    }
    kelve_server::agent_loop::learn::from_debrief(&app, id)
        .await
        .unwrap();
    let given = work(&app, |job| {
        assert_eq!(job.kind, "learn");
        assert!(
            job.material["kept"]
                .as_str()
                .unwrap()
                .contains("Give up after 5 tries")
        );
        learned(job, "Retry only while a call might pass")
    })
    .await;
    assert_eq!(given.len(), 1);
    let rust = std::fs::read_to_string(app.config.skills_dir.join("rust/SKILL.md")).unwrap();
    assert!(rust.contains("### Give up after 3 tries"), "{rust}");
    assert!(
        rust.contains("### Retry only while a call might pass"),
        "{rust}"
    );
    assert!(kelve_core::skills::check(&rust).is_empty(), "{rust}");
    let (parts, edits) = {
        let mut conn = app.db.acquire().await.unwrap();
        let skill = kelve_store::skills::by_name(&mut conn, "", "rust")
            .await
            .unwrap()
            .unwrap();
        (
            kelve_store::skills::parts(&mut conn, skill.id, false)
                .await
                .unwrap(),
            kelve_store::agent_loop::edits(&mut conn, 10).await.unwrap(),
        )
    };
    assert_eq!(parts.len(), 2);
    let evidence = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::candidates::evidence_for_part(&mut conn, parts[0].id)
            .await
            .unwrap()
    };
    assert!(!evidence.is_empty(), "where it came from");

    // Undone, the latest part goes; then the first, and the skill with it.
    kelve_server::agent_loop::files::undo(&app, edits[0].id)
        .await
        .unwrap();
    let rust = std::fs::read_to_string(app.config.skills_dir.join("rust/SKILL.md")).unwrap();
    assert!(!rust.contains("Retry only while"), "{rust}");
    assert!(rust.contains("Give up after 3 tries"), "{rust}");
    kelve_server::agent_loop::files::undo(&app, edits[1].id)
        .await
        .unwrap();
    assert!(!app.config.skills_dir.join("rust").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_correction_while_the_agent_works_is_learned_from() {
    let dir = repo();
    let dir_s = dir.display().to_string();
    let app = common::kelve(None).await;
    let _opened = show(&app, &dir, "worktree", json!({})).await;
    at(
        &app,
        MomentIn {
            what: "edit".into(),
            session: "s".into(),
            cwd: dir_s.clone(),
            file: Some(dir.join("src/retry.rs").display().to_string()),
            ..MomentIn::default()
        },
    )
    .await;
    at(
        &app,
        MomentIn {
            what: "prompt".into(),
            session: "s".into(),
            cwd: dir_s.clone(),
            prompt: Some("No, don't retry a 404: it will never pass.".into()),
            ..MomentIn::default()
        },
    )
    .await;
    common::jobs(&app).await;
    // The debrief of what it handed over comes first; then learning.
    let debrief = next_loop(&app, &common::caller()).await.unwrap();
    assert_eq!(debrief.kind, "debrief");
    agent::put_aside(&app, debrief.job).await.unwrap();
    let job = next_loop(&app, &common::caller())
        .await
        .expect("a learn job");
    assert_eq!(job.kind, "learn");
    let said = job.material["said"].as_str().unwrap();
    assert!(said.contains("don't retry a 404"), "{said}");
    assert!(
        said.contains("src/retry.rs"),
        "what it had just done: {said}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn skills_reach_the_agent_through_the_hook_once_a_session() {
    let dir = repo();
    let dir_s = dir.display().to_string();
    let app = common::kelve(None).await;
    let home = &app.config.skills_dir;
    kelve_server::skills_home::write(
        home,
        "everywhere",
        "---\nname: everywhere\ndescription: Use always.\n---\n\n# Everywhere\n\n## How you like it\n\n### Plain words\nShort sentences.\n",
    )
    .unwrap();
    kelve_server::skills_home::write(
        home,
        "rust",
        "---\nname: rust\ndescription: Use when writing Rust.\n---\n\n# Rust\n\n## How you like it\n\n### Errors are one enum\nWith a message for each.\n",
    )
    .unwrap();
    kelve_server::skills_home::write(
        &kelve_server::skills_home::project_dir(&dir),
        "ledger",
        "---\nname: ledger\ndescription: Use in the ledger.\n---\n\n# Ledger\n\n## Watch out for\n\n### Never charge twice\nEvery call carries a key.\n",
    )
    .unwrap();
    let prompt = |session: &str| MomentIn {
        what: "prompt".into(),
        session: session.into(),
        cwd: dir_s.clone(),
        prompt: Some("Add retries".into()),
        ..MomentIn::default()
    };
    let first = at(&app, prompt("s")).await;
    assert!(first.contains("### Plain words"), "{first}");
    assert!(first.contains("### Never charge twice"), "{first}");
    assert!(!first.contains("Errors are one enum"), "{first}");
    assert!(at(&app, prompt("s")).await.is_empty(), "once a session");
    let read = |file: &str| MomentIn {
        what: "read".into(),
        session: "s".into(),
        cwd: dir_s.clone(),
        file: Some(dir.join(file).display().to_string()),
        ..MomentIn::default()
    };
    assert!(
        !at(&app, read("README.md"))
            .await
            .contains("Errors are one enum")
    );
    let rust = at(&app, read("src/retry.rs")).await;
    assert!(rust.contains("### Errors are one enum"), "{rust}");
    assert!(
        !at(&app, read("src/lib.rs"))
            .await
            .contains("Errors are one enum")
    );
    // Another session has them afresh.
    assert!(at(&app, prompt("t")).await.contains("### Plain words"));

    // And the skills tool, by name or all.
    use axum::extract::Query;
    let all = kelve_server::api::agent_loop::skills(
        State(app.clone()),
        Agent,
        Query(serde_json::from_value(json!({ "path": dir_s })).unwrap()),
    )
    .await
    .unwrap()
    .0;
    let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["everywhere", "ledger", "rust"]);
    assert_eq!(all[1].kind, "project");
    let one = kelve_server::api::agent_loop::skills(
        State(app.clone()),
        Agent,
        Query(serde_json::from_value(json!({ "name": "rust" })).unwrap()),
    )
    .await
    .unwrap()
    .0;
    assert!(
        one[0]
            .markdown
            .as_deref()
            .unwrap()
            .contains("Errors are one enum")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
