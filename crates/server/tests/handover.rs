//! What the agent handed over, and what the person changed in it after: as
//! the agent shows its work in kelve, the change is kept as kelve read it;
//! when it commits later, what the person changed in the files handed over
//! is kept as evidence for their skills, and what the agent changed itself
//! isn't. And what the person says to the agent is kept, short.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use axum::Json;
use axum::extract::State;
use kelve_server::api::agent::{OpenIn, open};
use kelve_server::auth::Agent;
use kelve_server::moment::{MomentIn, moment};
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

async fn at(app: &Shared, m: MomentIn) -> String {
    moment(State(app.clone()), Agent, Json(m))
        .await
        .unwrap()
        .0
        .text
}

const RETRY: &str = "pub fn again(tries: u32) -> bool {
    tries < 5
}

pub fn wait(tries: u32) -> u64 {
    1 << tries
}
";

#[tokio::test]
async fn what_the_person_changed_after_handover_is_kept_and_the_agents_own_edits_arent() {
    let dir = common::folder(&[
        ("src/retry.rs", "pub fn again() {}\n"),
        ("src/fetch.rs", "pub fn fetch() {}\n"),
        ("README.md", "# Fetch\n"),
    ]);
    let dir_s = dir.display().to_string();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "Start"]);
    git(&dir, &["checkout", "-q", "-b", "retry"]);
    // The agent's work: one commit, and more not committed yet.
    std::fs::write(dir.join("src/retry.rs"), RETRY).unwrap();
    git(&dir, &["commit", "-qam", "Retry with a growing wait"]);
    std::fs::write(dir.join("src/fetch.rs"), "pub fn fetch() { again(0); }\n").unwrap();
    std::fs::write(
        dir.join(".env"),
        "TOKEN=ghp_abcdefghijklmnopqrstuvwxyz0123456789\n",
    )
    .unwrap();
    let app = common::kelve(None).await;

    // It shows its work: handed over, as kelve read it.
    let asked: OpenIn = serde_json::from_value(json!({
        "what": "worktree",
        "path": dir_s,
        "base": "main",
        "title": "Retry failed downloads",
        "said": "Downloads retry, waiting longer each time.",
        "agent": "stand-in-agent",
    }))
    .unwrap();
    let id = open(State(app.clone()), Agent, Json(asked))
        .await
        .unwrap()
        .0
        .id;
    let handover = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::handovers::latest_in(&mut conn, &dir_s, 0)
            .await
            .unwrap()
            .expect("handed over")
    };
    let head = {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(handover.opened, Some(id));
    assert_eq!(
        (
            handover.branch.as_str(),
            handover.base.as_str(),
            handover.change.as_str()
        ),
        ("retry", "main", "since main")
    );
    assert_eq!(handover.head, head);
    assert!(handover.uncommitted);
    assert_eq!(
        (handover.agent.as_str(), handover.title.as_str()),
        ("stand-in-agent", "Retry failed downloads")
    );
    assert_eq!(handover.said, "Downloads retry, waiting longer each time.");
    assert!(handover.session.is_empty(), "till the hook says whose");
    let files = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::handovers::files(&mut conn, handover.id)
            .await
            .unwrap()
    };
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec![".env", "src/fetch.rs", "src/retry.rs"]);
    assert_eq!(files[0].skip.as_deref(), Some("secret"));
    assert_eq!(files[0].text, None, "a secret's file is never kept");
    assert_eq!(files[2].text.as_deref(), Some(RETRY));
    assert!(files[2].hunks.contains("tries < 5"), "{}", files[2].hunks);
    assert_eq!(files[2].status, "modified");

    // The hook sees it open it: the handover is its session's.
    let opened = MomentIn {
        what: "opened".into(),
        session: "s".into(),
        cwd: dir_s.clone(),
        opened: Some(id),
        ..MomentIn::default()
    };
    at(&app, opened).await;
    {
        let mut conn = app.db.acquire().await.unwrap();
        let h = kelve_store::handovers::get(&mut conn, handover.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(h.session, "s");
    }

    // The person corrects it: their message is kept, short.
    let said = MomentIn {
        what: "prompt".into(),
        session: "s".into(),
        cwd: dir_s.clone(),
        agent: Some("claude-code".into()),
        prompt: Some("Give up after 3 tries, not 5. Then commit it.".into()),
        ..MomentIn::default()
    };
    at(&app, said).await;
    {
        let mut conn = app.db.acquire().await.unwrap();
        let kept = kelve_store::handovers::messages(&mut conn, "s", 0)
            .await
            .unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].root, dir_s);
        assert!(kept[0].text.starts_with("Give up after 3 tries"));
    }

    // The person changes a line the agent wrote; the agent edits another
    // file itself; then it commits it all.
    std::fs::write(
        dir.join("src/retry.rs"),
        RETRY.replace("tries < 5", "tries < 3"),
    )
    .unwrap();
    std::fs::write(
        dir.join("src/fetch.rs"),
        "pub fn fetch() { again(0); wait(0); }\n",
    )
    .unwrap();
    let edit = MomentIn {
        what: "edit".into(),
        session: "s".into(),
        cwd: dir_s.clone(),
        file: Some(dir.join("src/fetch.rs").display().to_string()),
        written: Some("wait(0);".into()),
        ..MomentIn::default()
    };
    at(&app, edit).await;
    std::fs::remove_file(dir.join(".env")).unwrap();
    git(&dir, &["commit", "-qam", "Give up sooner"]);
    let commit = MomentIn {
        what: "ran".into(),
        session: "s".into(),
        cwd: dir_s.clone(),
        command: Some("git commit -qam".into()),
        ..MomentIn::default()
    };
    at(&app, commit).await;

    let (edits, h) = {
        let mut conn = app.db.acquire().await.unwrap();
        (
            kelve_store::candidates::edits_after(&mut conn, handover.id)
                .await
                .unwrap(),
            kelve_store::handovers::get(&mut conn, handover.id)
                .await
                .unwrap()
                .unwrap(),
        )
    };
    assert!(h.compared_at.is_some());
    let paths: Vec<&str> = edits.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["src/retry.rs"],
        "the agent's own edit isn't the person's"
    );
    assert!(
        edits[0].text.contains("-    tries < 5"),
        "{}",
        edits[0].text
    );
    assert!(
        edits[0].text.contains("+    tries < 3"),
        "{}",
        edits[0].text
    );
    assert_eq!(edits[0].session, "s");
    assert_eq!(edits[0].kind, "edit");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_commit_with_nothing_handed_over_or_nothing_changed_keeps_nothing() {
    let dir = common::folder(&[("src/lib.rs", "pub fn a() {}\n")]);
    let dir_s = dir.display().to_string();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "Start"]);
    let app = common::kelve(None).await;

    // Nothing handed over here.
    std::fs::write(dir.join("src/lib.rs"), "pub fn a() { b(); }\n").unwrap();
    git(&dir, &["commit", "-qam", "More"]);
    assert_eq!(
        kelve_server::handover::on_commit(&app, "s", &dir_s)
            .await
            .unwrap(),
        0
    );

    // Handed over uncommitted, then committed as it was: nothing changed.
    std::fs::write(dir.join("src/lib.rs"), "pub fn a() { c(); }\n").unwrap();
    let asked: OpenIn = serde_json::from_value(json!({
        "what": "change",
        "path": dir_s,
        "title": "Call c",
        "agent": "stand-in-agent",
    }))
    .unwrap();
    let _opened = open(State(app.clone()), Agent, Json(asked)).await.unwrap();
    let handover = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::handovers::latest_in(&mut conn, &dir_s, 0)
            .await
            .unwrap()
            .expect("handed over")
    };
    assert_eq!(
        (
            handover.change.as_str(),
            handover.uncommitted,
            handover.branch.as_str()
        ),
        // The branch, read from git, though the change's words don't say.
        ("uncommitted", true, "main")
    );
    git(&dir, &["commit", "-qam", "Call c"]);
    assert_eq!(
        kelve_server::handover::on_commit(&app, "s", &dir_s)
            .await
            .unwrap(),
        0
    );
    // Outside a repository: nothing to compare.
    let elsewhere = common::folder(&[("a.txt", "a\n")]);
    assert_eq!(
        kelve_server::handover::on_commit(&app, "s", &elsewhere.display().to_string())
            .await
            .unwrap(),
        0
    );

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&elsewhere);
}
