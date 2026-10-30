//! Work shown before it's merged: kelve's hook has the agent show a
//! worktree in kelve before merging it, once for each commit, unless kelve
//! has shown it since.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use kelve_server::handoff::before;

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
async fn a_merge_of_work_not_shown_is_shown_first_once() {
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
    let main_s = main.display().to_string();

    // Not shown: the agent is told to show it first, once.
    let said = before(&app, &main_s, "git merge --ff-only fix")
        .await
        .unwrap();
    assert!(said.contains("Before you merge `fix`"), "{said}");
    assert!(said.contains("what: worktree"));
    assert_eq!(
        before(&app, &main_s, "git merge --ff-only fix")
            .await
            .unwrap(),
        "",
        "once"
    );
    // Not a merge: nothing.
    assert_eq!(before(&app, &main_s, "cargo test").await.unwrap(), "");

    // More work, then shown in kelve as a worktree: merging it goes ahead.
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
        kelve_store::reader::open(
            &mut conn,
            "change",
            &wt.display().to_string(),
            "",
            "since main",
            "Retries",
        )
        .await
        .unwrap();
    }
    assert_eq!(
        before(&app, &main_s, &format!("cd {main_s} && git merge fix"))
            .await
            .unwrap(),
        ""
    );

    // A commit made after it was shown is new work: shown first, again.
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
    let said = before(&app, &wt.display().to_string(), "gh pr merge --squash")
        .await
        .unwrap();
    assert!(said.contains("Before you merge this branch"), "{said}");

    let _ = std::fs::remove_dir_all(&wt);
    let _ = std::fs::remove_dir_all(&main);
}
