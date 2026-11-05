//! Work shown as it's done: kelve's hook has the agent show its work as its
//! turn ends, when it edited files, and when it commits, pushes or opens a
//! pull request, once for each commit, unless kelve has shown it since.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use kelve_core::hook::{Agent, Handover, moment};
use kelve_server::handoff::{after, done};
use serde_json::json;

/// The work a command hands over, as kelve's hook reads it after Claude
/// Code ran it in `cwd`.
fn handed(cwd: &str, command: &str) -> Option<Handover> {
    let event = json!({ "session_id": "s", "cwd": cwd, "tool_name": "Bash", "tool_input": { "command": command }, "tool_response": { "stdout": "", "stderr": "" } });
    moment(Agent::Claude, "PostToolUse", &event)?.handover
}

fn git(dir: &Path, args: &[&str], date: Option<&str>) {
    let mut cmd = std::process::Command::new("git");
    cmd.args([
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
    .env("GIT_CONFIG_NOSYSTEM", "1");
    if let Some(d) = date {
        cmd.env("GIT_COMMITTER_DATE", d).env("GIT_AUTHOR_DATE", d);
    }
    let ran = cmd.output().unwrap();
    assert!(
        ran.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

#[tokio::test]
async fn work_handed_over_and_not_shown_is_shown_once() {
    let main = common::folder(&[("src/retry.rs", "pub fn again() {}\n")]);
    git(&main, &["init", "-q", "-b", "main"], None);
    git(&main, &["add", "."], None);
    git(&main, &["commit", "-q", "-m", "Start"], None);
    let wt = main.with_file_name(format!(
        "{}-fix",
        main.file_name().unwrap().to_string_lossy()
    ));
    // Beside it, by a path from it: git can't make folders under Windows's
    // `\\?\` form of a path.
    let beside = format!("../{}", wt.file_name().unwrap().to_string_lossy());
    git(
        &main,
        &["worktree", "add", "-q", "-b", "fix", &beside],
        None,
    );
    std::fs::write(wt.join("src/retry.rs"), "pub fn again() { wait(); }\n").unwrap();
    git(&wt, &["commit", "-qam", "Wait between tries"], None);
    let app = common::kelve(None).await;
    let wt_s = wt.display().to_string();
    let main_s = main.display().to_string();

    // Committed and not shown: the agent is told to show it, once.
    let commit = handed(
        &wt_s,
        "git -c user.name='Ada Lovelace' commit -qam 'Wait between tries' && git log -1",
    )
    .unwrap();
    let said = after(&app, "s", &commit).await.unwrap();
    assert!(said.contains("show the person it in kelve"), "{said}");
    assert!(said.contains("what: worktree"));
    let push = handed(&wt_s, "git push -u origin fix 2>&1 | tail -3; echo pushed").unwrap();
    assert_eq!(
        after(&app, "s", &push).await.unwrap(),
        "",
        "once for each commit"
    );
    // Not handing anything over: nothing.
    assert_eq!(handed(&wt_s, "cargo test"), None);

    // More work, then shown in kelve as a worktree: pushing it says nothing.
    std::fs::write(
        wt.join("src/retry.rs"),
        "pub fn again() { wait(); jitter(); }\n",
    )
    .unwrap();
    git(
        &wt,
        &["commit", "-qam", "And spread them out"],
        Some("2001-01-01T00:00:00Z"),
    );
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::reader::open(&mut conn, "change", &wt_s, "", "since main", "Retries")
            .await
            .unwrap();
    }
    let push = handed(&main_s, &format!("cd '{wt_s}' && git push")).unwrap();
    assert_eq!(after(&app, "s", &push).await.unwrap(), "");

    // A commit made after it was shown is new work: shown again, when a pull
    // request is opened for it.
    std::fs::write(
        wt.join("src/retry.rs"),
        "pub fn again() { wait(); jitter(); cap(); }\n",
    )
    .unwrap();
    git(
        &wt,
        &["commit", "-qam", "Cap the wait"],
        Some("2099-01-01T00:00:00Z"),
    );
    // Opened from the main folder, for the branch: that branch's work. A
    // branch that isn't there is nothing kelve can show.
    let none = handed(&main_s, "gh pr create --head elsewhere --fill").unwrap();
    assert_eq!(after(&app, "s", &none).await.unwrap(), "");
    let pr = handed(&main_s, "gh pr create --head fix --fill").unwrap();
    let said = after(&app, "s", &pr).await.unwrap();
    assert!(said.contains("show the person it in kelve"), "{said}");

    let _ = std::fs::remove_dir_all(&wt);
    let _ = std::fs::remove_dir_all(&main);
}

#[tokio::test]
async fn work_done_is_shown_once_and_the_same_thing_is_brought_up_to_date() {
    let dir = common::folder(&[("src/retry.rs", "pub fn again() {}\n")]);
    git(&dir, &["init", "-q", "-b", "main"], None);
    git(&dir, &["add", "."], None);
    git(&dir, &["commit", "-q", "-m", "Start"], None);
    git(&dir, &["checkout", "-q", "-b", "fix"], None);
    let app = common::kelve(None).await;
    let dir_s = dir.display().to_string();
    let edit = |s: &'static str| {
        let (app, dir_s) = (app.clone(), dir_s.clone());
        async move {
            let mut conn = app.db.acquire().await.unwrap();
            kelve_store::lessons::touch(&mut conn, s, &dir_s, "src/retry.rs", true)
                .await
                .unwrap();
        }
    };

    // A turn that only read gives nothing to show.
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::lessons::touch(&mut conn, "s", &dir_s, "src/retry.rs", false)
            .await
            .unwrap();
    }
    assert_eq!(done(&app, "s").await.unwrap(), "");

    // It edits, and its turn ends: it's asked to show the work if it's done,
    // the same thing each time, and asked once.
    std::fs::write(dir.join("src/retry.rs"), "pub fn again() { wait(); }\n").unwrap();
    edit("s").await;
    let said = done(&app, "s").await.unwrap();
    assert!(
        said.contains("If you've finished what you were doing"),
        "{said}"
    );
    assert!(said.contains("what: worktree, with the branch it'll merge into"));
    assert_eq!(done(&app, "s").await.unwrap(), "", "once for its last edit");

    // It shows it, then commits, pushes and asks for it to be merged:
    // nothing changed since it was shown, so nothing's opened again.
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::reader::open(&mut conn, "change", &dir_s, "", "since main", "Retries")
            .await
            .unwrap();
    }
    git(&dir, &["commit", "-qam", "Wait between tries"], None);
    for command in [
        "git commit -qam 'Wait between tries'",
        "git push -u origin fix",
        "gh pr create --fill",
    ] {
        let handover = handed(&dir_s, command).unwrap();
        assert_eq!(after(&app, "s", &handover).await.unwrap(), "", "{command}");
    }

    // Edited again after it was shown: shown again, as the same thing.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    std::fs::write(
        dir.join("src/retry.rs"),
        "pub fn again() { wait(); cap(); }\n",
    )
    .unwrap();
    edit("s").await;
    assert!(
        done(&app, "s")
            .await
            .unwrap()
            .contains("brought up to date")
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn work_outside_a_repository_is_shown_as_its_files_one_thing_each_time() {
    use axum::Json;
    use axum::extract::State;
    use kelve_server::api::agent::{OpenIn, open};
    use kelve_server::auth::Agent;

    let dir = common::folder(&[
        ("notes/plan.md", "# Plan\n"),
        ("src/retry.py", "def again(): pass\n"),
    ]);
    let dir = std::fs::canonicalize(&dir).unwrap();
    let dir_s = dir.display().to_string();
    let app = common::kelve(None).await;
    {
        // A folder the hook met the agent in, walked for what it imports.
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::lessons::start_walk(&mut conn, &dir_s)
            .await
            .unwrap();
        kelve_store::lessons::touch(&mut conn, "t", &dir_s, "src/retry.py", true)
            .await
            .unwrap();
    }

    // Its turn ends: it's told to show the files it changed, not a branch.
    let said = done(&app, "t").await.unwrap();
    assert!(said.contains("what: files"), "{said}");
    assert!(!said.contains("what: worktree"), "{said}");

    // Shown once, then again with more: the same thing in kelve, in the
    // folder kelve knows, though the files first shown are all in src.
    let show = |paths: Vec<String>| {
        let app = app.clone();
        async move {
            let asked: OpenIn = serde_json::from_value(json!({
                "what": "files",
                "paths": paths,
                "title": "Retries",
                "agent": "stand-in-agent",
            }))
            .unwrap();
            open(State(app), Agent, Json(asked)).await.unwrap().0.id
        }
    };
    let first = show(vec![dir.join("src/retry.py").display().to_string()]).await;
    let again = show(vec![
        dir.join("src/retry.py").display().to_string(),
        dir.join("notes/plan.md").display().to_string(),
    ])
    .await;
    assert_eq!(first, again, "the same thing, brought up to date");
    let opened = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::reader::lately(&mut conn, 5).await.unwrap()
    };
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0].root, dir_s);
    assert_eq!(
        done(&app, "t").await.unwrap(),
        "",
        "shown since its last edit"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn what_another_agent_writes_around_a_worktree_isnt_this_ones_work() {
    use axum::Json;
    use axum::extract::State;
    use kelve_server::auth::Agent;
    use kelve_server::moment::MomentIn;

    let main = common::folder(&[("README.md", "# Ledger\n")]);
    let main = std::fs::canonicalize(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"], None);
    git(&main, &["add", "."], None);
    git(&main, &["commit", "-q", "-m", "Start"], None);
    // A worktree inside the project, as agents' apps make them.
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "fix",
            ".claude/worktrees/fix",
        ],
        None,
    );
    let wt = main.join(".claude/worktrees/fix");
    let (main_s, wt_s) = (main.display().to_string(), wt.display().to_string());
    let app = common::kelve(None).await;
    {
        // The person opened the whole project, and kelve read its README.
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::reader::read_file(&mut conn, &main_s, "README.md", "[]", "h")
            .await
            .unwrap();
    }
    let say = |what: &str, command: &str| {
        let m = MomentIn {
            what: what.into(),
            session: "w".into(),
            cwd: wt_s.clone(),
            command: Some(command.into()),
            ..MomentIn::default()
        };
        let app = app.clone();
        async move {
            kelve_server::moment::moment(State(app), Agent, Json(m))
                .await
                .unwrap()
                .0
                .text
        }
    };
    let pause = || tokio::time::sleep(std::time::Duration::from_millis(20));

    // Another agent edits the project's README while this one, in the
    // worktree, runs a command: it isn't this agent's, so nothing to show.
    say("prompt", "").await;
    pause().await;
    std::fs::write(main.join("README.md"), "# Ledger\n\nRetries.\n").unwrap();
    say("ran", "cargo test").await;
    assert_eq!(say("end", "").await, "");

    // What it writes in its own worktree is.
    pause().await;
    std::fs::write(wt.join("README.md"), "# Ledger\n\nWaits between tries.\n").unwrap();
    say("ran", "cat > README.md <<'EOF'").await;
    let said = say("end", "").await;
    assert!(said.contains("show them it in kelve"), "{said}");

    let _ = std::fs::remove_dir_all(&main);
}

#[tokio::test]
async fn an_agent_that_opened_something_is_asked_once_to_finish_reading_it() {
    use axum::Json;
    use axum::extract::State;
    use kelve_server::auth::Agent;
    use kelve_server::moment::MomentIn;

    let root = common::folder(&[("src/retry.rs", "pub fn again() {}\n")]);
    let root_s = root.display().to_string();
    let app = common::kelve(None).await;
    let (retries, theirs) = {
        let mut conn = app.db.acquire().await.unwrap();
        // An agent opened the project; the person, one file of it.
        let retries = kelve_store::reader::open(&mut conn, "folder", &root_s, "", "", "ledger")
            .await
            .unwrap();
        kelve_store::reader::opened_by(&mut conn, retries, "claude-code", "Retries", "", None)
            .await
            .unwrap();
        let theirs =
            kelve_store::reader::open(&mut conn, "file", &root_s, "src/retry.rs", "", "retry.rs")
                .await
                .unwrap();
        // Neither has its way in found yet.
        for id in [retries, theirs] {
            kelve_store::asks::add(
                &mut conn,
                &kelve_store::asks::New {
                    key: format!("way:{id}"),
                    kind: "way".into(),
                    about: "Finding the way in".into(),
                    idea: None,
                    opened: Some(id),
                    material: "{}".into(),
                    schema: "{}".into(),
                    resume_kind: "way.find".into(),
                    resume_key: Some(format!("way.find:{id}")),
                    resume_payload: json!({ "opened": id }).to_string(),
                    priority: 0,
                    follows: None,
                },
            )
            .await
            .unwrap();
        }
        (retries, theirs)
    };
    let say = |what: &str, session: &str, opened: Option<i64>| {
        let m = MomentIn {
            what: what.into(),
            session: session.into(),
            cwd: root_s.clone(),
            opened,
            ..MomentIn::default()
        };
        let app = app.clone();
        async move {
            kelve_server::moment::moment(State(app), Agent, Json(m))
                .await
                .unwrap()
                .0
                .text
        }
    };
    let pause = || tokio::time::sleep(std::time::Duration::from_millis(5));

    // The agent's hook saw it open the project; and, by a number it gave,
    // the person's file, which isn't the agent's to finish.
    assert_eq!(say("opened", "s", Some(retries)).await, "");
    say("opened", "s", Some(theirs)).await;
    // As its turn ends, it's told to finish reading what it opened, by its
    // name and number.
    let said = say("end", "s", None).await;
    assert!(
        said.contains(&format!("You opened Retries in kelve (number {retries})")),
        "{said}"
    );
    assert!(said.contains(&format!("giving for: {retries}")), "{said}");
    assert!(
        said.contains("Unless the person asked you not to"),
        "{said}"
    );
    assert!(said.contains("subagents"), "{said}");
    assert!(!said.contains(&format!("number {theirs}")), "{said}");
    // Once: as its next turn ends, nothing; nor for another agent, which
    // opened nothing.
    assert_eq!(say("end", "s", None).await, "");
    assert_eq!(say("end", "t", None).await, "");

    // Opened again, it's told again, while its reading isn't finished.
    pause().await;
    say("opened", "s", Some(retries)).await;
    assert!(say("end", "s", None).await.contains("number"));
    // Finished, it isn't.
    {
        let mut conn = app.db.acquire().await.unwrap();
        sqlx::query("delete from asks where opened = ?")
            .bind(retries)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    pause().await;
    say("opened", "s", Some(retries)).await;
    assert_eq!(say("end", "s", None).await, "");

    let _ = std::fs::remove_dir_all(&root);
}
