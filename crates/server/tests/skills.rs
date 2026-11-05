//! The skills pages: skills read from disk, the person's own and a project's,
//! in their groups; editing one, with what's wrong said in sentences; what
//! can be done with a part, and undoing it; suggested skills; checking each
//! part on past work, the agent redoing it with the part and without; and
//! Fits you, picked blind.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::Path;

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use kelve_core::skills::SkillKind;
use kelve_core::views::{PartCheck, SkillView};
use kelve_server::api::skills::{self, CheckIn, DecideIn, MarkdownIn, PartIn, PickIn, UndoIn};
use kelve_server::auth::Me;
use kelve_server::state::Shared;
use kelve_store::checks::{self, NewTask};
use kelve_store::skills::{self as kept, NewSkill};
use serde_json::{Value, json};

const RUST: &str = "---
name: rust
description: How Sam wants Rust written. Use when reading, writing or reviewing Rust.
---

# Rust

Rust that fails clearly and reads plainly, in any project.

## How you like it

### Errors are one enum, with a message for each
A string passed around loses what went wrong.

### Bad arguments exit 1, the tool's own failures exit 2
So scripts can tell \"you called it wrong\" from \"it broke\".

## Watch out for

### Never unwrap outside tests
Return the error instead.
";

const FETCH: &str = "---
name: fetch
description: How fetch is worked on. Use in the fetch repository.
---

# fetch

## Watch out for

### The test server runs on port 4810
Never 8080, which the docs server uses.
";

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&ran.stdout).trim().to_string()
}

/// A kelve with the person's Rust skill on disk.
async fn with_rust() -> Shared {
    let app = common::kelve(None).await;
    kelve_server::skills_home::write(&app.config.skills_dir, "rust", RUST).unwrap();
    app
}

async fn one(app: &Shared, id: &str) -> SkillView {
    skills::one(State(app.clone()), Me, UrlPath(id.into()))
        .await
        .unwrap()
        .0
}

fn part<'a>(view: &'a SkillView, heading: &str) -> &'a kelve_core::views::SkillPartView {
    view.sections
        .iter()
        .flat_map(|s| s.parts.iter())
        .find(|p| p.heading.starts_with(heading))
        .unwrap_or_else(|| panic!("no part {heading}"))
}

/// A project with a past task: what was asked, the commit it started from,
/// and the commit the person kept. The repo's root.
async fn past_task(app: &Shared) -> std::path::PathBuf {
    let root = common::folder(&[(
        "src/fetch.rs",
        "pub fn fetch(url: &str) -> Result<String, String> {\n    todo!()\n}\n",
    )]);
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "fetch"]);
    let start = git(&root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("src/fetch.rs"), KEPT_FILE).unwrap();
    git(&root, &["commit", "-q", "-am", "errors"]);
    let kept_at = git(&root, &["rev-parse", "HEAD"]);
    let mut conn = app.db.acquire().await.unwrap();
    checks::add_task(
        &mut conn,
        &NewTask {
            request: "Make fetch's errors say what went wrong.",
            root: root.to_str().unwrap(),
            start: &start,
            kept: Some(&kept_at),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    root
}

const KEPT_FILE: &str = "pub enum FetchError {\n    Timeout,\n    Status(u16),\n}\n\npub fn fetch(url: &str) -> Result<String, FetchError> {\n    todo!()\n}\n";

/// The change the person kept, as `git diff` prints it.
const KEPT_DIFF: &str = "diff --git a/src/fetch.rs b/src/fetch.rs
--- a/src/fetch.rs
+++ b/src/fetch.rs
@@ -1,3 +1,8 @@
-pub fn fetch(url: &str) -> Result<String, String> {
+pub enum FetchError {
+    Timeout,
+    Status(u16),
+}
+
+pub fn fetch(url: &str) -> Result<String, FetchError> {
     todo!()
 }
";

const STRING_DIFF: &str = "diff --git a/src/fetch.rs b/src/fetch.rs
--- a/src/fetch.rs
+++ b/src/fetch.rs
@@ -1,3 +1,3 @@
-pub fn fetch(url: &str) -> Result<String, String> {
+pub fn fetch(url: &str) -> Result<String, Box<dyn std::error::Error>> {
     todo!()
 }
";

#[tokio::test]
async fn skills_on_disk_are_listed_in_their_groups_with_each_parts_state() {
    let app = with_rust().await;
    kelve_server::skills_home::write(&app.config.skills_dir, "adding-a-flag", "---\nname: adding-a-flag\ndescription: The steps for a flag. Use when adding a flag.\n---\n\n# Adding a command-line flag\n\n## Steps\n\n### 1. Add it to the Args struct\nThe comment is what --help shows.\n").unwrap();
    // A project kelve knows, with a skill of its own.
    let project = common::folder(&[(".kelve/skills/fetch/SKILL.md", FETCH)]);
    let mut conn = app.db.acquire().await.unwrap();
    checks::add_task(
        &mut conn,
        &NewTask {
            request: "x",
            root: project.to_str().unwrap(),
            start: "abc",
            ..Default::default()
        },
    )
    .await
    .unwrap();
    drop(conn);

    let list = skills::list(State(app.clone()), Me).await.unwrap().0;
    assert_eq!(list.like.len(), 1);
    assert_eq!(list.like[0].title, "Rust");
    assert_eq!(list.like[0].parts, 3);
    assert_eq!(list.doing[0].title, "Adding a command-line flag");
    assert_eq!(list.project[0].title, "fetch");
    assert_eq!(
        list.project[0].project.as_deref(),
        project.file_name().and_then(|n| n.to_str())
    );
    assert_eq!(list.project[0].kind, SkillKind::Project);
    // A project's skill is addressed by its number: names repeat across projects.
    assert!(list.project[0].id.parse::<i64>().is_ok());

    let rust = one(&app, "rust").await;
    assert_eq!(rust.title, "Rust");
    assert_eq!(
        rust.intro,
        "Rust that fails clearly and reads plainly, in any project."
    );
    assert_eq!(rust.sections.len(), 2);
    let errors = part(&rust, "Errors are one enum");
    assert_eq!(errors.check, PartCheck::NotChecked);
    assert_eq!(errors.line, 12);
    assert!(rust.problems.is_empty(), "{:?}", rust.problems);
    assert_eq!(rust.offer.parts.len(), 3);
    // No past work kept yet: nothing to check it on.
    assert_eq!((rust.offer.tasks, rust.offer.runs), (0, 0));
}

#[tokio::test]
async fn a_skill_edited_by_hand_is_read_again_and_a_changed_part_isnt_checked() {
    let app = with_rust().await;
    let rust = one(&app, "rust").await;
    let mut conn = app.db.acquire().await.unwrap();
    let skill = kept::by_name(&mut conn, "", "rust").await.unwrap().unwrap();
    for p in kept::parts(&mut conn, skill.id, false).await.unwrap() {
        kept::checked(
            &mut conn,
            p.id,
            "works",
            Some("Better on 2 of 2 past tasks"),
            None,
        )
        .await
        .unwrap();
    }
    drop(conn);
    assert_eq!(
        part(&one(&app, "rust").await, "Never unwrap").check,
        PartCheck::Works
    );

    // The person changes one part's words, and adds one, in their editor.
    let edited = rust
        .markdown
        .replace("Return the error instead.", "Return the error, with `?`.")
        .replace(
            "## Watch out for\n",
            "## Watch out for\n\n### Fix clippy warnings, never quiet them\nChange the code itself.\n",
        );
    kelve_server::skills_home::write(&app.config.skills_dir, "rust", &edited).unwrap();
    let again = one(&app, "rust").await;
    assert_eq!(part(&again, "Never unwrap").check, PartCheck::NotChecked);
    assert_eq!(part(&again, "Fix clippy").check, PartCheck::NotChecked);
    assert_eq!(part(&again, "Errors are one enum").check, PartCheck::Works);
    assert_eq!(
        part(&again, "Errors are one enum").hint,
        "Better on 2 of 2 past tasks"
    );
    // kelve never writes it back.
    assert_eq!(
        kelve_server::skills_home::read(&app.config.skills_dir, "rust")
            .unwrap()
            .as_deref(),
        Some(edited.as_str())
    );
}

#[tokio::test]
async fn saving_a_skill_says_whats_wrong_in_sentences_and_writes_nothing_till_its_right() {
    let app = with_rust().await;
    let save = |text: &str| {
        let app = app.clone();
        let text = text.to_string();
        async move {
            skills::save(
                State(app),
                Me,
                UrlPath("rust".into()),
                Json(MarkdownIn { markdown: text }),
            )
            .await
            .unwrap()
            .0
        }
    };
    let broken = save("# Rust\n\n### A part with nowhere to go\n").await;
    assert!(!broken.saved);
    assert!(
        broken
            .problems
            .iter()
            .any(|p| p.starts_with("SKILL.md has no front matter."))
    );
    assert!(
        broken
            .problems
            .iter()
            .any(|p| p.contains("isn't under a section"))
    );
    assert_eq!(
        kelve_server::skills_home::read(&app.config.skills_dir, "rust")
            .unwrap()
            .as_deref(),
        Some(RUST)
    );

    let renamed = save(&RUST.replace("name: rust", "name: rusty")).await;
    assert!(!renamed.saved);
    assert!(renamed.problems[0].starts_with("The name has to stay “rust”"));

    let added = RUST.replace(
        "## Watch out for\n",
        "## Watch out for\n\n### A duration is a Duration\nA u64 doesn't say which unit.\n",
    );
    let saved = save(&added).await;
    assert!(saved.saved, "{:?}", saved.problems);
    assert_eq!(saved.unchecked, 4);

    // A new skill of the person's own.
    let made = skills::create(
        State(app.clone()),
        Me,
        Json(MarkdownIn {
            markdown: "---\nname: words\ndescription: How Sam writes app copy. Use when writing words people see.\n---\n\n# Words\n\n## How you like it\n\n### Buttons say what they do\nLike Delete note, never OK.\n".into(),
        }),
    )
    .await
    .unwrap()
    .0;
    assert!(made.saved);
    assert_eq!(made.id, "words");
    assert_eq!(one(&app, "words").await.kind, SkillKind::Like);
    let again = skills::create(
        State(app.clone()),
        Me,
        Json(MarkdownIn {
            markdown: RUST.into(),
        }),
    )
    .await
    .unwrap()
    .0;
    assert!(again.problems[0].starts_with("You have a skill called “rust” already."));
}

#[tokio::test]
async fn a_part_can_be_kept_reworded_or_taken_out_and_each_undone() {
    let app = with_rust().await;
    let view = one(&app, "rust").await;
    let key = part(&view, "Never unwrap").key.clone();
    let act = |act: &str, markdown: Option<&str>| {
        let app = app.clone();
        let input = PartIn {
            key: key.clone(),
            act: act.into(),
            markdown: markdown.map(str::to_string),
        };
        async move {
            skills::part(State(app), Me, UrlPath("rust".into()), Json(input))
                .await
                .map(|j| j.0)
        }
    };

    let removed = act("remove", None).await.unwrap();
    assert_eq!(removed.said, "Removed from Rust.");
    assert!(
        !kelve_server::skills_home::read(&app.config.skills_dir, "rust")
            .unwrap()
            .unwrap()
            .contains("Never unwrap")
    );
    let _ = skills::undo(
        State(app.clone()),
        Me,
        UrlPath("rust".into()),
        Json(UndoIn {
            before: removed.before.clone().unwrap(),
            after: removed.after.clone().unwrap(),
        }),
    )
    .await
    .unwrap();
    assert_eq!(
        kelve_server::skills_home::read(&app.config.skills_dir, "rust")
            .unwrap()
            .as_deref(),
        Some(RUST)
    );
    // Undone twice, it says the skill changed since.
    let twice = skills::undo(
        State(app.clone()),
        Me,
        UrlPath("rust".into()),
        Json(UndoIn {
            before: removed.before.unwrap(),
            after: removed.after.unwrap(),
        }),
    )
    .await
    .unwrap_err();
    assert!(twice.message.starts_with("The skill changed since"));

    // It made no difference: kept anyway.
    let mut conn = app.db.acquire().await.unwrap();
    let skill = kept::by_name(&mut conn, "", "rust").await.unwrap().unwrap();
    let row = kept::parts(&mut conn, skill.id, false)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.heading.starts_with("Never unwrap"))
        .unwrap();
    kept::checked(&mut conn, row.id, "failed", Some("Not followed on 1 of 2 past tasks"), Some("### Return errors with `?`, never unwrap\nOutside tests, a panic takes the whole tool down."))
        .await
        .unwrap();
    drop(conn);
    let failed = part(&one(&app, "rust").await, "Never unwrap")
        .failed
        .clone()
        .unwrap();
    assert!(failed.wording.starts_with("### Return errors with `?`"));

    let reworded = act("wording", None).await.unwrap();
    assert_eq!(reworded.said, "Rust has the clearer wording.");
    let view = one(&app, "rust").await;
    // A new heading is a new part, not checked yet.
    assert_eq!(
        part(&view, "Return errors with").check,
        PartCheck::NotChecked
    );
    assert!(
        view.markdown
            .contains("Outside tests, a panic takes the whole tool down.")
    );

    let key = part(&view, "Return errors with").key.clone();
    let own = skills::part(
        State(app.clone()),
        Me,
        UrlPath("rust".into()),
        Json(PartIn {
            key: key.clone(),
            act: "own".into(),
            markdown: Some("No heading here".into()),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(
        own.message,
        "The wording must start with a `###` heading that is the rule itself."
    );
    let own = skills::part(
        State(app.clone()),
        Me,
        UrlPath("rust".into()),
        Json(PartIn {
            key,
            act: "own".into(),
            markdown: Some("### Never unwrap, ever\nNot even in tests.".into()),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(own.said, "Saved to Rust.");
    assert!(
        one(&app, "rust")
            .await
            .markdown
            .contains("### Never unwrap, ever\nNot even in tests.")
    );
}

/// The stand-in agent's answer to a check: with the part, it did what the
/// part says and what the person kept; without it, it kept errors strings.
fn redone(job: &kelve_server::agent::Job) -> Value {
    assert_eq!(job.kind, "compare");
    if job.material.get("part").is_some() {
        json!({
            "with": { "diff": KEPT_DIFF, "said": "Errors are one enum now.", "followed": true, "shows": "pub enum FetchError" },
            "without": { "diff": STRING_DIFF, "said": "Errors are boxed.", "followed": false, "shows": "Box<dyn Error>" }
        })
    } else {
        json!({
            "with": { "diff": KEPT_DIFF, "said": "Errors are one enum now." },
            "without": { "diff": STRING_DIFF, "said": "Errors are boxed." }
        })
    }
}

#[tokio::test]
async fn a_part_is_checked_on_past_work_redone_with_it_and_without_it() {
    let app = with_rust().await;
    // Nothing to check it on yet.
    let none = skills::check(
        State(app.clone()),
        Me,
        UrlPath("rust".into()),
        Json(CheckIn::default()),
    )
    .await
    .unwrap_err();
    assert!(
        none.message
            .starts_with("There’s no past work to check it on yet.")
    );

    past_task(&app).await;
    let view = one(&app, "rust").await;
    assert_eq!(
        (view.offer.parts.len(), view.offer.tasks, view.offer.runs),
        (3, 1, 6)
    );
    let key = part(&view, "Errors are one enum").key.clone();
    let started = skills::check(
        State(app.clone()),
        Me,
        UrlPath("rust".into()),
        Json(CheckIn { keys: vec![key] }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!((started.parts, started.done, started.runs), (1, 0, 2));
    let view = one(&app, "rust").await;
    assert_eq!(
        part(&view, "Errors are one enum").check,
        PartCheck::Checking
    );
    assert_eq!(view.running.map(|r| (r.done, r.runs)), Some((0, 2)));
    let list = skills::list(State(app.clone()), Me).await.unwrap().0;
    assert!(list.like[0].checking);

    let given = common::work(&app, redone).await;
    assert_eq!(given.len(), 1);
    let job = &given[0];
    assert_eq!(
        job.about,
        "Checking “Errors are one enum, with a message for each” on past work"
    );
    // The agent is given the request, where it started, and the skill with the part and without it.
    assert!(
        common::unfenced(&job.material["request"])
            .contains("Make fetch's errors say what went wrong.")
    );
    assert!(common::unfenced(&job.material["with"]).contains("### Errors are one enum"));
    assert!(!common::unfenced(&job.material["without"]).contains("### Errors are one enum"));
    assert!(common::unfenced(&job.material["without"]).contains("### Never unwrap"));
    assert_eq!(job.material["start"].as_str().map(str::len), Some(40));

    let view = one(&app, "rust").await;
    let errors = part(&view, "Errors are one enum");
    assert_eq!(errors.check, PartCheck::Works);
    assert_eq!(errors.hint, "Better on 1 of 1 past task");
    assert_eq!(view.running, None);
    let mut conn = app.db.acquire().await.unwrap();
    let skill = kept::by_name(&mut conn, "", "rust").await.unwrap().unwrap();
    let row = kept::parts(&mut conn, skill.id, false)
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.heading.starts_with("Errors"))
        .unwrap();
    let check = &checks::checks_of(&mut conn, row.id).await.unwrap()[0];
    assert_eq!(check.verdict.as_deref(), Some("works"));
    assert_eq!(
        check.notes.as_deref(),
        Some("With the part, your agent did what it says. Without it, it didn't.")
    );
}

#[tokio::test]
async fn a_part_the_agent_still_got_wrong_failed_with_clearer_words_and_a_check_can_stop() {
    let app = with_rust().await;
    past_task(&app).await;
    let view = one(&app, "rust").await;
    let key = part(&view, "Never unwrap").key.clone();
    let _ = skills::check(
        State(app.clone()),
        Me,
        UrlPath("rust".into()),
        Json(CheckIn {
            keys: vec![key.clone()],
        }),
    )
    .await
    .unwrap();
    // A wrong answer: it didn't follow the part, and gave no wording.
    common::jobs(&app).await;
    let job = kelve_server::agent::next(&app, &common::caller())
        .await
        .unwrap()
        .unwrap();
    let wrong = json!({
        "with": { "diff": STRING_DIFF, "said": "Done.", "followed": false, "shows": "an unwrap in main" },
        "without": { "diff": STRING_DIFF, "said": "Done.", "followed": false, "shows": "an unwrap in main" }
    });
    let handed = kelve_server::agent::hand_in(&app, &common::caller(), job.job, wrong.clone())
        .await
        .unwrap();
    assert!(
        matches!(handed, kelve_server::agent::HandedIn::No { ref why, .. } if why.contains("give wording"))
    );
    let mut right = wrong;
    right["wording"] = json!(
        "### Return errors with `?`, never unwrap\nOutside tests, a panic takes the whole tool down."
    );
    let handed = kelve_server::agent::hand_in(&app, &common::caller(), job.job, right)
        .await
        .unwrap();
    assert_eq!(handed, kelve_server::agent::HandedIn::Yes);
    common::jobs(&app).await;
    let view = one(&app, "rust").await;
    let unwrap = part(&view, "Never unwrap");
    assert_eq!(unwrap.check, PartCheck::Failed);
    assert_eq!(unwrap.hint, "Not followed on 1 of 1 past task");
    let failed = unwrap.failed.clone().unwrap();
    assert_eq!(
        failed.what,
        "With the part, your agent still didn't do what it says: An unwrap in main. Clearer wording:"
    );
    assert!(failed.wording.starts_with("### Return errors with `?`"));

    // Checking the rest, then stopping: they aren't checked, and the agent's jobs go.
    let _ = skills::check(
        State(app.clone()),
        Me,
        UrlPath("rust".into()),
        Json(CheckIn::default()),
    )
    .await
    .unwrap();
    common::jobs(&app).await;
    assert!(one(&app, "rust").await.running.is_some());
    let stopped = skills::stop(State(app.clone()), Me, UrlPath("rust".into()))
        .await
        .unwrap()
        .0;
    assert_eq!(stopped.said, "Stopped. Those parts aren’t checked yet.");
    let view = one(&app, "rust").await;
    assert_eq!(view.running, None);
    assert_eq!(
        part(&view, "Errors are one enum").check,
        PartCheck::NotChecked
    );
    assert_eq!(part(&view, "Never unwrap").check, PartCheck::Failed);
    assert!(
        kelve_server::agent::next(&app, &common::caller())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn a_suggested_skill_is_made_added_to_one_you_have_or_left_for_now() {
    let app = with_rust().await;
    let draft = "---\nname: retrying-network-calls\ndescription: How Sam retries network calls. Use when a call over the network can fail.\n---\n\n# Retrying network calls\n\n## How you like it\n\n### Retry only failures that might pass\nA 4xx won't, so don't retry it.\n";
    let mut conn = app.db.acquire().await.unwrap();
    let id = kept::add(
        &mut conn,
        &NewSkill {
            name: "retrying-network-calls",
            kind: "do",
            suggested: Some((draft, "A new skill from five of your decisions.")),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    drop(conn);
    let list = skills::list(State(app.clone()), Me).await.unwrap().0;
    assert_eq!(list.suggested.len(), 1);
    assert_eq!(list.suggested[0].title, "Retrying network calls");
    let s = skills::suggested(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    assert_eq!(s.why, "A new skill from five of your decisions.");
    assert_eq!(s.sections[0].parts[0].check, PartCheck::NotChecked);
    assert_eq!(s.close_to, None);

    let made = skills::decide(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(DecideIn {
            act: "create".into(),
            to: None,
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(made.id, "retrying-network-calls");
    assert_eq!(
        one(&app, "retrying-network-calls").await.kind,
        SkillKind::Do
    );
    assert!(
        skills::list(State(app.clone()), Me)
            .await
            .unwrap()
            .0
            .suggested
            .is_empty()
    );

    // Another, close to Rust: its parts go into Rust instead.
    let close = "---\nname: rust-errors\ndescription: How Sam wants Rust errors written. Use when reading, writing or reviewing Rust.\n---\n\n# Rust errors\n\n## How you like it\n\n### Say what to do next in every error\nIn a full sentence.\n";
    let mut conn = app.db.acquire().await.unwrap();
    let id = kept::add(
        &mut conn,
        &NewSkill {
            name: "rust-errors",
            kind: "do",
            suggested: Some((close, "Three of your decisions.")),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    drop(conn);
    let s = skills::suggested(State(app.clone()), Me, UrlPath(id))
        .await
        .unwrap()
        .0;
    assert_eq!(s.close_to.map(|c| c.name), Some("rust".into()));
    let added = skills::decide(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(DecideIn {
            act: "add".into(),
            to: Some("rust".into()),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(added.id, "rust");
    let rust = one(&app, "rust").await;
    assert_eq!(
        part(&rust, "Say what to do next").check,
        PartCheck::NotChecked
    );
    assert_eq!(rust.sections[0].parts.len(), 3);

    let mut conn = app.db.acquire().await.unwrap();
    let id = kept::add(
        &mut conn,
        &NewSkill {
            name: "later",
            kind: "do",
            suggested: Some((draft, "Why.")),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    drop(conn);
    let _ = skills::decide(
        State(app.clone()),
        Me,
        UrlPath(id),
        Json(DecideIn {
            act: "not_now".into(),
            to: None,
        }),
    )
    .await
    .unwrap();
    assert!(
        skills::list(State(app.clone()), Me)
            .await
            .unwrap()
            .0
            .suggested
            .is_empty()
    );
}

#[tokio::test]
async fn fits_you_shows_two_results_blind_and_counts_the_picks() {
    let app = with_rust().await;
    let none = skills::fits_start(State(app.clone()), Me)
        .await
        .unwrap_err();
    assert!(
        none.message
            .starts_with("There’s no past work left to redo.")
    );
    past_task(&app).await;
    let started = skills::fits_start(State(app.clone()), Me).await.unwrap().0;
    assert_eq!(started.waiting, 1);
    assert_eq!(started.pick, None);

    let given = common::work(&app, redone).await;
    assert_eq!(given.len(), 1);
    assert_eq!(given[0].about, "Redoing past work for Fits you");
    assert!(
        given[0].material["with"]
            .as_str()
            .unwrap()
            .contains("### Errors are one enum")
    );
    assert_eq!(given[0].material["without"], json!(""));

    let view = skills::fits(State(app.clone()), Me).await.unwrap().0;
    let pick = view.pick.unwrap();
    assert_eq!(pick.request, "Make fetch's errors say what went wrong.");
    let mine_left = pick.left.contains("FetchError");
    assert_ne!(mine_left, pick.right.contains("FetchError"));
    let picked = skills::fits_pick(
        State(app.clone()),
        Me,
        UrlPath(pick.id),
        Json(PickIn {
            picked: if mine_left { "left" } else { "right" }.into(),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(picked.mine, if mine_left { "left" } else { "right" });
    assert_eq!(picked.skills_close, Some(100));
    assert_eq!(picked.standard_close, Some(25));
    let again = skills::fits_pick(
        State(app.clone()),
        Me,
        UrlPath(pick.id),
        Json(PickIn {
            picked: "same".into(),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(again.message, "You picked one of this pair already.");

    let view = skills::fits(State(app.clone()), Me).await.unwrap().0;
    assert_eq!((view.tally.picked, view.tally.of), (1, 1));
    assert_eq!(view.tally.tests, [true]);
    assert_eq!(
        view.closeness.map(|c| (c.skills, c.standard)),
        Some((100, 25))
    );
    assert_eq!(view.pick, None);
}
