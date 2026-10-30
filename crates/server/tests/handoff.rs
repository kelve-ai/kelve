//! Work shown as it's handed over: kelve's hook has the agent show its work
//! in kelve when it commits, pushes or opens a pull request, once for each
//! commit, unless kelve has shown it since.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use kelve_server::handoff::after;

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
    let said = after(&app, &wt_s, "git commit -qam 'Wait between tries'")
        .await
        .unwrap();
    assert!(said.contains("show the person it in kelve"), "{said}");
    assert!(said.contains("what: worktree"));
    assert_eq!(
        after(&app, &wt_s, "git push -u origin fix").await.unwrap(),
        "",
        "once for each commit"
    );
    // Not handing anything over: nothing.
    assert_eq!(after(&app, &wt_s, "cargo test").await.unwrap(), "");

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
    assert_eq!(
        after(&app, &main_s, &format!("cd {wt_s} && git push"))
            .await
            .unwrap(),
        ""
    );

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
    let said = after(&app, &wt_s, "gh pr create --fill").await.unwrap();
    assert!(said.contains("show the person it in kelve"), "{said}");

    let _ = std::fs::remove_dir_all(&wt);
    let _ = std::fs::remove_dir_all(&main);
}
