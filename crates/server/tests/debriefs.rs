//! Home and the debrief, as the person uses them: the agent hands its work
//! over and writes the debrief; home asks the person to read it; they agree,
//! ask for a decision another way, have their agent look into one, ask for
//! an idea to be explained or say they know it, and send it, which leaves
//! their agent a note and asks it to learn. A small change can be debriefed
//! anyway, and a debrief that failed asked for again. The change view
//! shows each file whole, each decision beside its lines.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use axum::Json;
use axum::extract::{Path as UrlPath, Query, State};
use kelve_core::views::{
    AnswerState, DebriefLineKind, DebriefState, DecisionChoice, DecisionStatus, HomeAskKind,
    HomeState, SendKind, TimelineKind,
};
use kelve_server::api::agent::{OpenIn, open};
use kelve_server::api::debriefs::{self as api, DecideIn, IdeaIn, LookIn, PathIn, RememberIn};
use kelve_server::auth::{Agent, Me};
use kelve_server::state::Shared;
use serde_json::json;

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

const RETRY: &str = "const MOST_TRIES: u32 = 5;

pub fn again(tries: u32) -> bool {
    tries < MOST_TRIES
}

pub fn wait(tries: u32) -> u64 {
    1 << tries
}
";

const DEBRIEF: &str = "---
kelve: debrief/1
title: Retry failed downloads
---

## What it did

When a download fails, fetch tries again, up to 5 times.

## How it works

Every download asks `again()`, in `src/retry.rs`, whether to try once more.

## Decisions

### Stop after 5 tries
- from: its own call
- lines: src/retry.rs 1-5
- could have: a flag for how many
- rests on: Exponential backoff

Long enough to ride out a short outage.

### Double the wait each time
- from: what you said
- lines: src/retry.rs 7-9
- rests on: Exponential backoff, Jitter

Doubling gives a struggling server room to recover.

### Fetch asks before it tries again
- from: its own call
- lines: src/fetch.rs 1

So a failure that won't pass fails at once.

## What it rests on

- **Exponential backoff:** waiting longer after each failure.
- **Jitter:** a little randomness added to a wait.
";

/// A repository with the agent's work on a branch, handed over in kelve:
/// the handover's id.
async fn handed_over(app: &Shared) -> (std::path::PathBuf, i64) {
    let dir = common::folder(&[
        ("src/fetch.rs", "pub fn fetch() {}\n"),
        ("README.md", "# Fetch\n"),
    ]);
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "Start"]);
    git(&dir, &["checkout", "-q", "-b", "retry"]);
    std::fs::write(dir.join("src/retry.rs"), RETRY).unwrap();
    std::fs::write(dir.join("src/fetch.rs"), "pub fn fetch() { again(0); }\n").unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "Retry"]);
    let asked: OpenIn = serde_json::from_value(json!({
        "what": "worktree",
        "path": dir.display().to_string(),
        "base": "main",
        "title": "Retry failed downloads",
        "said": "Downloads retry.",
        "agent": "stand-in-agent",
    }))
    .unwrap();
    let _ = open(State(app.clone()), Agent, Json(asked)).await.unwrap();
    let mut conn = app.db.acquire().await.unwrap();
    let h = kelve_store::handovers::latest_in(&mut conn, &dir.display().to_string(), 0)
        .await
        .unwrap()
        .expect("handed over");
    (dir, h.id)
}

/// The agent's debrief of a handover, written and kept: its id.
async fn written(app: &Shared, handover: i64, root: &Path) -> i64 {
    let mut conn = app.db.acquire().await.unwrap();
    let id =
        kelve_store::debriefs::for_handover(&mut conn, handover, &root.display().to_string(), "s")
            .await
            .unwrap();
    kelve_store::debriefs::written(&mut conn, id, "Retry failed downloads", "small", DEBRIEF)
        .await
        .unwrap();
    id
}

async fn jobs(app: &Shared) -> Vec<(String, String)> {
    let mut conn = app.db.acquire().await.unwrap();
    sqlx::query_as("select kind, key from jobs where kind like 'v2.%' order by id")
        .fetch_all(&mut *conn)
        .await
        .unwrap()
}

async fn decide(app: &Shared, id: i64, n: u32, choice: Option<DecisionChoice>, note: Option<&str>) {
    let _ = api::decide(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(DecideIn {
            n,
            choice,
            note: note.map(str::to_string),
        }),
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn home_starts_empty_then_asks_for_the_debrief_in_the_agents_words() {
    let app = common::kelve(None).await;
    let home = api::home(State(app.clone()), Me).await.unwrap().0;
    assert_eq!(home.state, HomeState::Empty);
    assert_eq!(home.status, "New here");
    assert!(home.say.starts_with("Hello. I’m your agent."));
    assert!(home.days.is_empty());

    let (dir, h) = handed_over(&app).await;
    // Handed over with no debrief: a small change, to debrief anyway.
    let home = api::home(State(app.clone()), Me).await.unwrap().0;
    assert_eq!(home.state, HomeState::Normal);
    assert_eq!(home.days[0].label, "Today");
    let small = &home.days[0].items[0];
    assert_eq!(
        (small.kind, small.title.as_str()),
        (TimelineKind::Small, "Retry failed downloads")
    );

    let id = written(&app, h, &dir).await;
    let home = api::home(State(app.clone()), Me).await.unwrap().0;
    assert_eq!(home.say, "One thing for you.");
    assert_eq!(home.status, "Waiting for you");
    let ask = &home.asks[0];
    assert_eq!(ask.kind, HomeAskKind::Debrief);
    assert_eq!(
        ask.text,
        "I finished Retry failed downloads. I made 3 decisions you might want a say in."
    );
    assert_eq!(ask.to.as_deref(), Some(format!("debrief/{id}").as_str()));
    assert!(ask.main);
    let line = &home.days[0].items[0];
    assert_eq!(line.kind, TimelineKind::Debrief);
    assert!(
        home.days
            .iter()
            .flat_map(|d| &d.items)
            .all(|i| i.kind != TimelineKind::Small),
        "the work has its debrief"
    );

    // The person asks for more: their agent is at work, with a clock.
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::handovers::keep_message(
            &mut conn,
            "s",
            "claude",
            &dir.display().to_string(),
            "Retry 429 too.",
        )
        .await
        .unwrap();
    }
    // kelve's hook told it of that message, but isn't in the agent's
    // settings now: home says so first.
    let home = api::home(State(app.clone()), Me).await.unwrap().0;
    assert_eq!(home.state, HomeState::Attention);
    assert_eq!(home.say, "Something needs fixing.");
    assert_eq!(
        home.asks[0].text,
        "kelve’s hook is gone from Claude Code’s settings, so I can’t send you debriefs."
    );
    assert_eq!(home.asks[0].alt.as_deref(), Some("Not now"));
    // Not now: asked again tomorrow.
    let home = api::later(
        State(app.clone()),
        Me,
        Json(api::LaterIn { ask: "hook".into() }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(home.state, HomeState::Working);
    assert_eq!(home.say, "I’m working on what you asked.");
    assert!(home.working_since.is_some());
}

#[tokio::test]
async fn a_debrief_reads_from_its_markdown_and_the_change_handed_over() {
    let app = common::kelve(None).await;
    let (dir, h) = handed_over(&app).await;
    let id = written(&app, h, &dir).await;
    let v = api::one(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    assert_eq!(v.state, DebriefState::Ready);
    assert_eq!(v.title, "Retry failed downloads");
    assert_eq!((v.files, v.left), (2, 3));
    assert!(v.did.starts_with("When a download fails"));
    let d = &v.decisions[0];
    assert_eq!(d.title, "Stop after 5 tries");
    assert_eq!(d.place, "src/retry.rs, lines 1 to 5");
    assert_eq!(d.status, DecisionStatus::YourCall);
    assert_eq!(d.lang.as_deref(), Some("rust"));
    assert_eq!(d.code.len(), 5);
    assert!(d.code.iter().all(|l| l.kind == DebriefLineKind::Added));
    let skill = d.skill.as_ref().unwrap();
    assert_eq!((skill.title.as_str(), skill.new), ("Rust", true));
    assert_eq!(v.ideas.len(), 2);
    assert!(v.ideas.iter().all(|i| i.new_to_you));
    // Files by folder, those with decisions first.
    assert_eq!(v.folders[0].name, "src/");
    assert_eq!(v.folders[0].files[0].path, "src/fetch.rs");
    assert_eq!(v.folders[0].files[1].decisions, [1, 2]);
    assert_eq!(v.on_send[0].text, "Decisions 1, 2 and 3 stay open.");

    // The change view: every file, whole, each decision beside its lines.
    let c = api::change(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    assert_eq!(c.folders.iter().map(|f| f.files.len()).sum::<usize>(), 2);
    let f = api::diff(
        State(app.clone()),
        Me,
        UrlPath(id),
        Query(PathIn {
            path: "src/fetch.rs".into(),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(f.kind, "changed");
    let kinds: Vec<DebriefLineKind> = f.lines.iter().map(|l| l.kind).collect();
    assert_eq!(kinds, [DebriefLineKind::Removed, DebriefLineKind::Added]);
    assert_eq!((f.notes[0].n, f.notes[0].at), (3, 2));
    let r = api::diff(
        State(app.clone()),
        Me,
        UrlPath(id),
        Query(PathIn {
            path: "src/retry.rs".into(),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(r.kind, "new");
    assert_eq!(
        r.notes.iter().map(|n| (n.n, n.at)).collect::<Vec<_>>(),
        [(1, 1), (2, 7)]
    );
    assert!(
        api::diff(
            State(app.clone()),
            Me,
            UrlPath(id),
            Query(PathIn {
                path: "nope".into()
            })
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn the_person_answers_each_decision_and_sends_it() {
    let app = common::kelve(None).await;
    let (dir, h) = handed_over(&app).await;
    let id = written(&app, h, &dir).await;

    // Agreeing with what came from them keeps it for every time.
    decide(&app, id, 2, Some(DecisionChoice::Agree), None).await;
    let v = api::one(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    let d = &v.decisions[1];
    assert_eq!(d.status, DecisionStatus::Agreed);
    assert!(d.can_remember && d.remember);
    assert_eq!(v.left, 2);
    // And they can say not.
    let v = api::remember(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(RememberIn { n: 2, on: false }),
    )
    .await
    .unwrap()
    .0;
    assert!(!v.decisions[1].remember);

    // Change it: a note for their agent, left as they send it.
    decide(
        &app,
        id,
        1,
        Some(DecisionChoice::Change),
        Some("Add a --retries flag, defaulting to 5."),
    )
    .await;
    // Look into it: research waits for the agent.
    let v = api::look_into(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(LookIn {
            n: 3,
            question: "Should fetch retry 429 too?".into(),
        }),
    )
    .await
    .unwrap()
    .0;
    let d = &v.decisions[2];
    assert_eq!(d.status, DecisionStatus::Researching);
    assert_eq!(d.question, "Should fetch retry 429 too?");
    assert_eq!(d.research.as_ref().unwrap().state, AnswerState::Asked);
    assert!(
        api::look_into(
            State(app.clone()),
            Me,
            UrlPath(id),
            Json(LookIn {
                n: 3,
                question: " ".into()
            })
        )
        .await
        .is_err(),
        "a question is needed"
    );
    // Home says what waits for the agent.
    let home = api::home(State(app.clone()), Me).await.unwrap().0;
    assert!(
        home.say
            .ends_with("Next time I work, I’ll look into your question."),
        "{}",
        home.say
    );

    // Explain: once for the same code.
    let v = api::explain(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(IdeaIn {
            idea: "Jitter".into(),
        }),
    )
    .await
    .unwrap()
    .0;
    let jitter = v.ideas.iter().find(|i| i.name == "Jitter").unwrap();
    assert_eq!(
        jitter.explanation.as_ref().unwrap().state,
        AnswerState::Asked
    );
    let _ = api::explain(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(IdeaIn {
            idea: "jitter".into(),
        }),
    )
    .await
    .unwrap();
    // I know this: in marks.md, so it's new to them nowhere.
    let v = api::know(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(IdeaIn {
            idea: "Exponential backoff".into(),
        }),
    )
    .await
    .unwrap()
    .0;
    assert!(!v.ideas[0].new_to_you);
    assert!(
        kelve_server::spec::read(&app)
            .never
            .contains(&"Exponential backoff".to_string())
    );

    let kinds: Vec<String> = jobs(&app).await.into_iter().map(|(k, _)| k).collect();
    assert_eq!(kinds, ["v2.research", "v2.explain"]);

    // What happens when it's sent.
    let v = api::remember(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(RememberIn { n: 1, on: true }),
    )
    .await
    .unwrap()
    .0;
    let lines: Vec<(SendKind, &str)> = v
        .on_send
        .iter()
        .map(|l| (l.kind, l.text.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            (SendKind::Redo, "Your agent redoes decision 1."),
            (
                SendKind::Research,
                "Your agent keeps looking into decision 3."
            ),
            (SendKind::NewSkill, "Decision 1 starts a new skill, Rust."),
        ]
    );

    // Agree with the rest: none left; then send.
    let v = api::agree_rest(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    assert_eq!(v.left, 0);
    assert_eq!(
        v.decisions[2].status,
        DecisionStatus::Researching,
        "asked, it stays asked"
    );
    let v = api::send(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    assert_eq!(v.state, DebriefState::Sent);
    assert!(v.sent_at.is_some());
    assert_eq!(
        v.on_send[0].text,
        "Your agent redoes decision 1 when you next message it."
    );
    assert_eq!(v.on_send[1].action.as_deref(), Some("Open the debrief"));
    let keys: Vec<String> = jobs(&app).await.into_iter().map(|(_, k)| k).collect();
    assert!(keys.contains(&format!("v2.learn:debrief:{id}")), "{keys:?}");

    // The note waits for the agent, on the decision's lines.
    let notes = kelve_server::reader::waiting_for_agent(&app, &dir)
        .await
        .unwrap();
    assert_eq!(notes.len(), 1);
    // Sent, it can't change.
    let again = api::decide(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(DecideIn {
            n: 2,
            choice: None,
            note: None,
        }),
    )
    .await;
    assert!(again.is_err());
    // On home, it's done.
    let home = api::home(State(app.clone()), Me).await.unwrap().0;
    assert!(home.asks.is_empty());
    assert_eq!(home.days[0].items[0].kind, TimelineKind::Done);
}

#[tokio::test]
async fn a_small_change_can_be_debriefed_and_a_failed_one_asked_for_again() {
    let app = common::kelve(None).await;
    let (_, h) = handed_over(&app).await;
    let asked = api::debrief_it(State(app.clone()), Me, UrlPath(h))
        .await
        .unwrap()
        .0;
    let id: i64 = asked.id.parse().unwrap();
    let v = api::one(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    assert_eq!(v.state, DebriefState::Waiting);
    assert_eq!(v.steps[0].text, "Waiting for your agent");
    assert_eq!(v.steps[1].text, "Reading the 2 files");
    assert_eq!(
        jobs(&app).await,
        [("v2.debrief".to_string(), format!("v2.debrief:{h}"))]
    );
    // Asked twice, it's the same debrief.
    let twice = api::debrief_it(State(app.clone()), Me, UrlPath(h))
        .await
        .unwrap()
        .0;
    assert_eq!(twice.id, asked.id);

    // The agent got it wrong twice: home needs the person.
    {
        let mut conn = app.db.acquire().await.unwrap();
        for _ in 0..kelve_store::debriefs::MOST_TRIES {
            kelve_store::debriefs::wrong(&mut conn, id, "The debrief has no title.")
                .await
                .unwrap();
        }
    }
    let home = api::home(State(app.clone()), Me).await.unwrap().0;
    assert_eq!(home.state, HomeState::Attention);
    let ask = &home.asks[0];
    assert_eq!(ask.kind, HomeAskKind::Stalled);
    assert_eq!(
        ask.text,
        "I stopped before finishing the debrief for Retry failed downloads."
    );
    assert_eq!(ask.id, format!("again:{id}"));
    let v = api::one(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    assert_eq!(v.state, DebriefState::Failed);
    assert_eq!(v.problems, ["The debrief has no title."]);
    let v = api::again(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    assert_eq!(v.state, DebriefState::Waiting);
    let home = api::home(State(app.clone()), Me).await.unwrap().0;
    assert_eq!(home.state, HomeState::Normal);
    assert!(
        home.say.contains("I’ll write up Retry failed downloads."),
        "{}",
        home.say
    );
}
