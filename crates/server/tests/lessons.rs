//! Lessons, as the person's agent meets them: the best one about a file as it
//! reads it, the one that answers a problem as something fails, what came of
//! each, and, when something failed and then passed, a job to write what it
//! learned, which waits before it leaves.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use axum::Json;
use axum::extract::State;
use common::kelve;
use kelve_core::ideas::IdeaKind;
use kelve_core::lesson::{Lesson, Record, seen_as};
use kelve_graph_api::LessonState;
use kelve_graph_api::stand_in::StandIn;
use kelve_server::auth::Agent;
use kelve_server::lessons::{self, MomentIn};
use kelve_server::state::Shared;
use serde_json::json;

async fn moment(app: &Shared, what: &str, f: impl FnOnce(&mut MomentIn)) -> String {
    let mut m = MomentIn {
        what: what.into(),
        session: "session-1".into(),
        ..MomentIn::default()
    };
    f(&mut m);
    lessons::moment(State(app.clone()), Agent, Json(m))
        .await
        .unwrap()
        .0
        .text
}

#[tokio::test]
async fn a_lesson_reaches_the_agent_holds_and_what_it_learns_waits_then_leaves() {
    let g = StandIn::start().await.unwrap();
    let rand = g.add("rand", "A Rust library for random numbers.", IdeaKind::Tool);
    let theirs = g.learned(
        Lesson {
            about: vec![rand.clone()],
            says: "rand 0.9 renamed `thread_rng()` to `rng()`; the old name warns, then fails."
                .into(),
            act: "Call `rand::rng()` instead of `rand::thread_rng()`.".into(),
            example: Some("let mut r = rand::rng();".into()),
            holds_when: "rand 0.9 and later.".into(),
            backed_by: None,
            seen_as: Some(seen_as(
                "error[E0425]: cannot find function `thread_rng` in crate `rand`",
            )),
        },
        Record::default(),
    );
    let app = kelve(Some(&g)).await;
    let root = common::folder(&[("src/dice.rs", "fn roll() -> u32 { 4 }\n")]);
    let root_s = root.display().to_string();
    let file = root.join("src/dice.rs").display().to_string();
    // kelve read the file, and marked rand in it.
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::entries::keep_idea(
            &mut conn,
            &kelve_store::entries::Idea {
                key: rand.clone(),
                name: "rand".into(),
                definition: "A Rust library for random numbers.".into(),
                kind: "tool".into(),
                became: None,
            },
        )
        .await
        .unwrap();
        kelve_store::reader::read_file(
            &mut conn,
            &root_s,
            "src/dice.rs",
            &json!([{ "key": rand, "name": "rand", "area": "rust", "depth": "common" }])
                .to_string(),
            "h",
        )
        .await
        .unwrap();
    }
    lessons::refresh(&app, std::slice::from_ref(&rand))
        .await
        .unwrap();

    // As the agent reads the file: nothing in it is what the lesson is about.
    let text = moment(&app, "read", |m| {
        m.cwd = root_s.clone();
        m.file = Some(file.clone());
    })
    .await;
    assert_eq!(text, "", "most moments give nothing");
    // As it edits the file to use the old name: the lesson about rand, as
    // not confirmed, with a check first, since kelve can't tell the version.
    // Given to Codex, though another agent was seen since.
    kelve_server::agent::hello(
        &app,
        &kelve_server::agent::Caller {
            agent: "claude-code".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let text = moment(&app, "edit", |m| {
        m.cwd = root_s.clone();
        m.file = Some(file.clone());
        m.agent = Some("codex".into());
        m.written = Some("let mut r = rand::thread_rng();".into());
    })
    .await;
    assert!(
        text.contains("a lesson about rand, not confirmed yet"),
        "{text}"
    );
    assert!(text.contains(&theirs));
    assert!(text.contains("Check first that it holds here"));
    let again = moment(&app, "edit", |m| {
        m.cwd = root_s.clone();
        m.file = Some(file.clone());
    })
    .await;
    assert_eq!(again, "", "never twice in a session for the same file");

    // An agent that can't be given text as a tool runs (Antigravity) is
    // given it as it next calls its model.
    let held = lessons::moment(
        State(app.clone()),
        Agent,
        Json(MomentIn {
            what: "failed".into(),
            session: "gravity".into(),
            cwd: root_s.clone(),
            command: Some("cargo check".into()),
            problem: Some("error[E0425]: cannot find function `thread_rng` in crate `rand`".into()),
            hold: true,
            ..MomentIn::default()
        }),
    )
    .await
    .unwrap()
    .0
    .text;
    assert_eq!(held, "", "kept for later");
    let next = lessons::moment(
        State(app.clone()),
        Agent,
        Json(MomentIn {
            what: "next".into(),
            session: "gravity".into(),
            cwd: root_s.clone(),
            ..MomentIn::default()
        }),
    )
    .await
    .unwrap()
    .0
    .text;
    assert!(next.contains(&theirs), "{next}");

    // Something fails that it answers, then the same command passes: it held.
    let text = moment(&app, "failed", |m| {
        m.cwd = root_s.clone();
        m.command = Some("cargo test".into());
        m.problem = Some(format!(
            "error[E0425]: cannot find function `thread_rng` in crate `rand`\n --> {file}:1:5"
        ));
    })
    .await;
    assert!(text.contains(&theirs), "the lesson that answers it: {text}");
    assert!(!text.contains(&root_s), "nothing from the work");
    assert_eq!(
        moment(&app, "ran", |m| {
            m.cwd = root_s.clone();
            m.command = Some("cargo test".into());
        })
        .await,
        "",
        "it held, so there's nothing to write"
    );
    let shown = lessons::about_files(&app, &root_s, &["src/dice.rs".into()])
        .await
        .unwrap();
    assert_eq!(shown.used.len(), 1);
    assert!(
        shown.used[0]
            .used
            .iter()
            .any(|u| u.line == "held: the error didn't come back, and the next run passed"),
        "{:?}",
        shown.used[0].used
    );
    assert!(
        shown.used[0]
            .used
            .iter()
            .any(|u| u.line == "given to Codex; nothing showed yet whether it held"),
        "named for the agent it was given to: {:?}",
        shown.used[0].used
    );

    // Something fails that no lesson answers, and one fix makes it pass:
    // its error led to the fix, so there's nothing to write.
    moment(&app, "failed", |m| {
        m.cwd = root_s.clone();
        m.command = Some("cargo check".into());
        m.problem = Some("error[E0425]: cannot find value `x` in this scope".into());
    })
    .await;
    moment(&app, "ran", |m| {
        m.cwd = root_s.clone();
        m.command = Some("cargo check".into());
    })
    .await;
    let text = moment(&app, "end", |m| m.cwd = root_s.clone()).await;
    assert_eq!(text, "", "one failure that one edit fixed");
    // It fails twice, with two errors, the second run piped: a turn that
    // ends still failing isn't asked, and waits for the one that gets past.
    for problem in [
        "error[E0599]: no method named `gen_range` found for struct `ThreadRng`",
        "error[E0277]: the trait bound `Range<u32>: SampleRange<_>` is not satisfied",
    ] {
        moment(&app, "failed", |m| {
            m.cwd = root_s.clone();
            m.command = Some("cargo build".into());
            m.problem = Some(problem.into());
        })
        .await;
    }
    let still = moment(&app, "end", |m| m.cwd = root_s.clone()).await;
    assert_eq!(still, "", "still failing: nothing to learn yet");
    // Then something passes, whatever the command: hard won, so as the turn
    // ends, kelve asks what the agent learned, once.
    let passing = moment(&app, "ran", |m| {
        m.cwd = root_s.clone();
        m.command = Some("cargo run --example dice".into());
    })
    .await;
    assert_eq!(passing, "", "nothing asked mid-turn");
    let text = moment(&app, "end", |m| m.cwd = root_s.clone()).await;
    assert!(text.contains("write it as a lesson"), "{text}");
    assert!(text.contains("this turn took 2 failing runs"), "{text}");
    assert_eq!(
        moment(&app, "end", |m| m.cwd = root_s.clone()).await,
        "",
        "once"
    );
    let given = common::work(&app, |job| {
        assert_eq!(job.kind, "lesson");
        assert!(
            job.material["lessons"].as_str().unwrap().contains(&theirs),
            "what's known, first"
        );
        let failed = job.material["failed"].as_str().unwrap();
        assert!(failed.contains("E0599"));
        assert!(
            failed.contains("It took 2 failing runs in under a minute; the errors were:"),
            "{failed}"
        );
        json!({
            "lessons": [{
                "about": ["rand"],
                "says": "rand 0.9 renamed `gen_range` to `random_range`; the old name is gone.",
                "do": "Call `random_range` where `gen_range` was.",
                "holdsWhen": "rand 0.9 and later.",
                "seenAs": "error[E0599]: no method named `gen_range` found for struct `ThreadRng`",
            }],
            "same": [],
        })
    })
    .await;
    assert_eq!(given.len(), 1);
    let settings = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::lessons::learned(&mut conn, 10).await.unwrap()
    };
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].state, "waiting", "held a day first");
    assert!(g.lessons().len() == 1, "nothing's gone yet");

    // The person lets it go now; it reaches the graph, live, not confirmed.
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::lessons::send_now(&mut conn, settings[0].id)
            .await
            .unwrap();
    }
    common::jobs(&app).await;
    let held = g.lessons();
    assert_eq!(held.len(), 2);
    assert_eq!(held[1].state, LessonState::Live);
    assert!(held[1].lesson.says.contains("random_range"));
    let mine = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::lessons::learned_one(&mut conn, settings[0].id)
            .await
            .unwrap()
            .unwrap()
    };
    assert_eq!(mine.state, "live");

    // What came of the first goes to the graph, with nothing else.
    assert_eq!(lessons::send_outcomes(&app).await.unwrap(), 1);
    let page = g.lessons().into_iter().find(|l| l.id == theirs).unwrap();
    assert_eq!(page.record.held, 1);
}

#[tokio::test]
async fn a_lesson_that_names_the_work_is_sent_back_to_fix() {
    let material = json!({
        "known": [],
        "offered": [{ "key": "kelve:i3k9xq2m7twa8", "name": "rand" }],
        "work": ["acme-dice", "dice.rs"],
    });
    let answer = json!({
        "lessons": [{
            "about": ["rand"],
            "says": "In acme-dice the roll uses `gen_range`.",
            "do": "Call `random_range`.",
            "holdsWhen": "rand 0.9 and later.",
        }],
        "same": [],
    });
    let why = lessons::shape(&material, &answer).unwrap_err();
    assert!(why.contains("names `acme-dice`"), "{why}");
    let not_listed = json!({ "lessons": [], "same": ["kelve:l99"] });
    assert!(lessons::shape(&material, &not_listed).is_err());
    assert!(
        lessons::shape(&material, &json!({ "lessons": [], "same": [] })).is_ok(),
        "none is an answer"
    );
}

#[tokio::test]
async fn a_lesson_held_back_is_neither_given_nor_shown_but_what_came_of_it_goes() {
    let g = StandIn::start().await.unwrap();
    let rand = g.add("rand", "A Rust library for random numbers.", IdeaKind::Tool);
    let theirs = g.learned(
        Lesson {
            about: vec![rand.clone()],
            says: "rand 0.9 renamed `thread_rng()` to `rng()`.".into(),
            act: "Call `rand::rng()`.".into(),
            example: None,
            holds_when: "rand 0.9 and later.".into(),
            backed_by: None,
            seen_as: Some(seen_as(
                "error[E0425]: cannot find function `thread_rng` in crate `rand`",
            )),
        },
        Record::default(),
    );
    // Every lesson that fits, once it's widely used, is held back.
    g.given(&theirs, kelve_core::lesson::HOLD_BACK_FROM);
    let mut config = kelve_server::config::test_config();
    config.endpoints.graph = g.url.clone();
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    config.hold_back = 1000;
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate(&pool).await.unwrap();
    let app = kelve_server::state::App::new(config, pool).await.unwrap();
    let root = common::folder(&[("src/dice.rs", "fn roll() {}\n")]);
    let root_s = root.display().to_string();
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::reader::read_file(
            &mut conn,
            &root_s,
            "src/dice.rs",
            &json!([{ "key": rand, "name": "rand" }]).to_string(),
            "h",
        )
        .await
        .unwrap();
    }
    lessons::refresh(&app, std::slice::from_ref(&rand))
        .await
        .unwrap();
    let failed = |what: &str| {
        let mut m = MomentIn {
            what: what.into(),
            session: "s".into(),
            cwd: root_s.clone(),
            command: Some("cargo build".into()),
            ..MomentIn::default()
        };
        if what == "failed" {
            m.problem =
                Some("error[E0425]: cannot find function `thread_rng` in crate `rand`".into());
        }
        m
    };
    let text = lessons::moment(State(app.clone()), Agent, Json(failed("failed")))
        .await
        .unwrap()
        .0
        .text;
    assert_eq!(text, "", "held back");
    let passed = lessons::moment(State(app.clone()), Agent, Json(failed("ran")))
        .await
        .unwrap()
        .0
        .text;
    assert!(
        !passed.contains(&theirs),
        "the lesson held back is never named: {passed}"
    );
    let shown = lessons::about_files(&app, &root_s, &["src/dice.rs".into()])
        .await
        .unwrap();
    assert!(shown.used.is_empty(), "the reader never says it was used");
    assert_eq!(
        lessons::send_outcomes(&app).await.unwrap(),
        1,
        "what came of the moment goes"
    );
    let page = g.lessons().into_iter().find(|l| l.id == theirs).unwrap();
    assert_eq!(
        page.record.held, 0,
        "and never counts for or against the lesson"
    );
}

#[tokio::test]
async fn told_to_hold_lessons_a_short_while_what_was_learned_leaves_by_itself() {
    let g = StandIn::start().await.unwrap();
    let rand = g.add("rand", "A Rust library for random numbers.", IdeaKind::Tool);
    let mut config = kelve_server::config::test_config();
    config.endpoints.graph = g.url.clone();
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    config.lesson_hold = 0;
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate(&pool).await.unwrap();
    let app = kelve_server::state::App::new(config, pool).await.unwrap();
    let root = common::folder(&[
        ("src/dice.rs", "fn roll() {}\n"),
        ("Cargo.toml", "[dependencies]\nrand = \"0.9\"\n"),
    ]);
    let root_s = root.display().to_string();
    // The file the agent works in marks only an idea the graph hasn't got;
    // rand is marked where the work says it uses it.
    {
        let mut conn = app.db.acquire().await.unwrap();
        for (path, ideas) in [
            (
                "src/dice.rs",
                json!([{ "key": "new:dice rolls", "name": "Dice rolls" }]),
            ),
            ("Cargo.toml", json!([{ "key": rand, "name": "rand" }])),
        ] {
            kelve_store::reader::read_file(&mut conn, &root_s, path, &ideas.to_string(), "h")
                .await
                .unwrap();
        }
    }
    let file = root.join("src/dice.rs").display().to_string();
    moment(&app, "edit", |m| {
        m.cwd = root_s.clone();
        m.file = Some(file.clone());
    })
    .await;
    // Two failing runs: once with the whole command, once with another
    // form of it, the same problem each time.
    for command in ["cargo test", "cargo test --lib"] {
        moment(&app, "failed", |m| {
            m.cwd = root_s.clone();
            m.command = Some(command.into());
            m.problem = Some("thread 'tests::seeded' panicked: left: 3, right: 5".into());
        })
        .await;
    }
    // Passed in another form again, piped: the turn's end asks.
    moment(&app, "ran", |m| {
        m.cwd = root_s.clone();
        m.command = Some("cargo test -p dice 2>&1 | tail -20".into());
    })
    .await;
    let text = moment(&app, "end", |m| m.cwd = root_s.clone()).await;
    assert!(text.contains("write it as a lesson"), "{text}");
    assert!(text.contains("this turn took 2 failing runs"), "{text}");
    assert!(text.contains("anyone using rand"), "{text}");
    common::work(&app, |job| {
        let offered: Vec<&str> = job.material["offered"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| i["name"].as_str())
            .collect();
        assert_eq!(offered, ["rand"], "only what the graph has");
        json!({
            "lessons": [{
                "about": ["rand"],
                "says": "rand's `StdRng` may give other numbers after an upgrade, so a test seeded with it can fail with nothing about rand in the failure.",
                "do": "Seed a named generator, such as `ChaCha8Rng`, where a seed must give the same numbers.",
                "holdsWhen": "Any rand version.",
            }],
            "same": [],
        })
    })
    .await;
    common::jobs(&app).await;
    let learned = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::lessons::learned(&mut conn, 10).await.unwrap()
    };
    assert_eq!(learned.len(), 1);
    assert_eq!(learned[0].state, "live", "it left once its hold was over");
    assert_eq!(g.lessons().len(), 1);
}

#[tokio::test]
async fn a_lesson_about_a_library_a_file_imports_is_found_though_kelve_never_marked_it() {
    let g = StandIn::start().await.unwrap();
    let rand = g.add("rand", "A Rust library for random numbers.", IdeaKind::Tool);
    let theirs = g.learned(
        Lesson {
            about: vec![rand.clone()],
            says: "Rust 2024 reserves `gen`, so `rng.gen()` no longer parses; rand 0.9 names it `random`.".into(),
            act: "Call `rng.random()` where `rng.gen()` was.".into(),
            example: None,
            holds_when: "rand 0.9 and later, in the 2024 edition.".into(),
            backed_by: None,
            seen_as: Some(seen_as(
                "error: expected identifier, found reserved keyword `gen`",
            )),
        },
        Record::default(),
    );
    let app = kelve(Some(&g)).await;
    let text = "use rand::Rng;\n\npub fn coin() -> bool {\n    rand::rng().gen::<bool>()\n}\n";
    let root = common::folder(&[("src/lib.rs", text)]);
    let root_s = root.display().to_string();
    // kelve read the file and marked what it does, never rand; it keeps the
    // libraries it imports, and finds rand in the graph by its name.
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::reader::read_file(
            &mut conn,
            &root_s,
            "src/lib.rs",
            &json!([{ "key": "new:coin flips", "name": "Coin flips" }]).to_string(),
            "h",
        )
        .await
        .unwrap();
        let used = kelve_engine::candidates::libraries("src/lib.rs", text);
        assert_eq!(used, ["rand"]);
        // The work's own packages are never looked up.
        std::fs::create_dir_all(root.join("shop")).unwrap();
        assert_eq!(
            lessons::not_own(&root, vec!["rand".into(), "shop".into()]),
            ["rand"]
        );
        kelve_store::lessons::keep_uses(&mut conn, &root_s, "src/lib.rs", &used)
            .await
            .unwrap();
    }
    lessons::look_up(&app, &["rand".into()]).await.unwrap();
    {
        let mut conn = app.db.acquire().await.unwrap();
        let used = kelve_store::lessons::used_in(&mut conn, &root_s, &["src/lib.rs".into()])
            .await
            .unwrap();
        assert_eq!(
            used,
            [(rand.clone(), "rand".to_string())],
            "found in the graph"
        );
    }

    // The tests fail as the lesson says they would: it's given, as material.
    let said = moment(&app, "failed", |m| {
        m.cwd = root_s.clone();
        m.command = Some("cargo test".into());
        m.problem = Some("error: expected identifier, found reserved keyword `gen`".into());
    })
    .await;
    assert!(said.contains(&theirs), "{said}");
    assert!(said.contains("a lesson about rand"), "{said}");
    // The side lists it with the file's lessons.
    let shown = lessons::about_files(&app, &root_s, &["src/lib.rs".into()])
        .await
        .unwrap();
    assert_eq!(shown.used.len(), 1);
}

#[tokio::test]
async fn replaced_work_asks_for_a_better_way_given_as_the_usual_way_is_written_and_held_if_it_stays()
 {
    let g = StandIn::start().await.unwrap();
    let tokio = g.add("tokio", "An asynchronous runtime for Rust.", IdeaKind::Tool);
    let app = kelve(Some(&g)).await;
    let usual = "    tokio::time::sleep(Duration::from_secs(1)).await;\n    tokio::time::sleep(Duration::from_secs(2)).await;\n    tokio::time::sleep(Duration::from_secs(4)).await;\n";
    let better = "    tokio::time::advance(Duration::from_secs(7)).await;\n";
    let root = common::folder(&[
        ("tests/retry.rs", "use tokio::time::Duration;\n"),
        ("tests/poll.rs", "\n"),
    ]);
    let root_s = root.display().to_string();
    let file = root.join("tests/retry.rs");
    let file_s = file.display().to_string();
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::reader::read_file(
            &mut conn,
            &root_s,
            "tests/retry.rs",
            &json!([{ "key": tokio, "name": "tokio" }]).to_string(),
            "h",
        )
        .await
        .unwrap();
        // A file with nothing marked in it yet.
        kelve_store::reader::read_file(&mut conn, &root_s, "tests/poll.rs", "[]", "h")
            .await
            .unwrap();
    }
    let edit = |session: &str, text: &str, old: Option<&str>| MomentIn {
        what: "edit".into(),
        session: session.into(),
        cwd: root_s.clone(),
        file: Some(file_s.clone()),
        written: Some(text.into()),
        replaced: old.map(str::to_string),
        ..MomentIn::default()
    };
    let say = |m: MomentIn| {
        let app = app.clone();
        async move {
            lessons::moment(State(app), Agent, Json(m))
                .await
                .unwrap()
                .0
                .text
        }
    };
    // The agent writes the usual way, then, asked, replaces it with its
    // own better way: kelve asks for a lesson at the person's next message.
    std::fs::write(&file, usual).unwrap();
    say(edit("a", usual, None)).await;
    std::fs::write(&file, better).unwrap();
    assert_eq!(say(edit("a", better, Some(usual))).await, "");
    let prompt = || MomentIn {
        what: "prompt".into(),
        session: "a".into(),
        cwd: root_s.clone(),
        ..MomentIn::default()
    };
    let asked = say(prompt()).await;
    assert!(
        asked.contains("you replaced what you wrote in tests/retry.rs"),
        "{asked}"
    );
    assert_eq!(say(prompt()).await, "", "once");
    let jobs = common::work(&app, |job| {
        let replaced = job.material["replaced"].as_str().unwrap();
        assert!(replaced.contains("tokio::time::sleep") && replaced.contains("tokio::time::advance"));
        json!({
            "lessons": [{
                "about": ["tokio"],
                "says": "A test that waits for real is as slow as its waits; with the clock paused, the same waits pass at once.",
                "do": "Start tests that wait with the clock paused, and advance it.",
                "holdsWhen": "tokio 1, with test-util.",
                "seenAs": "tokio::time::sleep(Duration::from_secs(1)).await;",
            }],
            "same": [],
        })
    })
    .await;
    assert_eq!(jobs.len(), 1);
    let learned = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::lessons::learned(&mut conn, 10).await.unwrap()
    };
    assert_eq!(learned[0].first.as_deref(), Some(usual.trim_end()));
    assert_eq!(learned[0].path.as_deref(), Some("tests/retry.rs"));

    // Another session's agent, given it as it writes the usual way: what
    // it reads says what to replace. Done its way and committed, it held;
    // changed back, it didn't.
    let page = g.learned(
        Lesson {
            about: vec![tokio.clone()],
            says: "A test that waits for real is as slow as its waits.".into(),
            act: "Start tests that wait with the clock paused.".into(),
            // Its own way, as the paused clock's tests write it.
            example: Some("tokio::time::sleep(paused_by(wait)).await;".into()),
            holds_when: "tokio 1, with test-util.".into(),
            backed_by: None,
            seen_as: Some(seen_as("tokio::time::sleep(Duration::from_secs(1)).await;")),
        },
        Record::default(),
    );
    {
        let mut conn = app.db.acquire().await.unwrap();
        sqlx::query("delete from lessons_fetched")
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    lessons::refresh(&app, std::slice::from_ref(&tokio))
        .await
        .unwrap();
    for (session, back) in [("b1", false), ("b2", true)] {
        std::fs::write(&file, usual).unwrap();
        let given = say(edit(session, usual, None)).await;
        assert!(given.contains(&page), "{given}");
        assert!(
            given.contains("Instead of: `tokio::time::sleep(Duration::from_secs(<n>)).await;`"),
            "{given}"
        );
        std::fs::write(&file, better).unwrap();
        say(edit(session, better, None)).await;
        if back {
            std::fs::write(&file, usual).unwrap();
            say(edit(session, usual, None)).await;
        } else {
            // The agent says it held before it commits: what the commit
            // shows outweighs that.
            let mut conn = app.db.acquire().await.unwrap();
            let given = kelve_store::lessons::given_in(&mut conn, session)
                .await
                .unwrap();
            kelve_store::lessons::came_of(&mut conn, given[0].id, "held", "said", 1, "")
                .await
                .unwrap();
        }
        say(MomentIn {
            what: "ran".into(),
            session: session.into(),
            cwd: root_s.clone(),
            command: Some("git add -A && git commit -m done".into()),
            ..MomentIn::default()
        })
        .await;
    }
    let shown = lessons::about_files(&app, &root_s, &["tests/retry.rs".into()])
        .await
        .unwrap();
    let view = shown.used.iter().find(|l| l.id == page).unwrap();
    let lines: Vec<&str> = view.used.iter().map(|u| u.line.as_str()).collect();
    assert!(
        lines.contains(&"held: it was done this way, and it stayed"),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"didn't hold: it was changed back"),
        "{lines:?}"
    );
    assert_eq!(
        view.instead.as_deref(),
        Some("tokio::time::sleep(Duration::from_secs(<n>)).await;")
    );

    // In a file with nothing marked in it, the usual way written, however
    // it's filled in, is enough: the lesson's idea is elsewhere in the folder.
    let poll = root.join("tests/poll.rs");
    let mine = "    tokio::time::sleep(every).await;\n";
    std::fs::write(&poll, mine).unwrap();
    let given = say(MomentIn {
        what: "edit".into(),
        session: "d".into(),
        cwd: root_s.clone(),
        file: Some(poll.display().to_string()),
        written: Some(mine.into()),
        ..MomentIn::default()
    })
    .await;
    assert!(given.contains(&page), "{given}");
    assert!(given.contains("Instead of:"), "{given}");

    // Written its own way already, the cue alone isn't enough: nothing to
    // give, and no "Instead of".
    let theirs = "    tokio::time::sleep(paused_by(every)).await;\n";
    std::fs::write(&poll, theirs).unwrap();
    let given = say(MomentIn {
        what: "edit".into(),
        session: "e".into(),
        cwd: root_s.clone(),
        file: Some(poll.display().to_string()),
        written: Some(theirs.into()),
        ..MomentIn::default()
    })
    .await;
    assert_eq!(given, "", "already the lesson's way");

    // A file written over whole replaces what the agent last wrote there;
    // when the turn's end also asks about what failed, one job has both.
    std::fs::write(&file, usual).unwrap();
    say(edit("c", usual, None)).await;
    std::fs::write(&file, better).unwrap();
    say(edit("c", better, None)).await;
    for _ in 0..2 {
        say(MomentIn {
            what: "failed".into(),
            session: "c".into(),
            cwd: root_s.clone(),
            command: Some("cargo test".into()),
            problem: Some("test result: FAILED. 1 passed; 1 failed".into()),
            ..MomentIn::default()
        })
        .await;
    }
    say(MomentIn {
        what: "ran".into(),
        session: "c".into(),
        cwd: root_s.clone(),
        command: Some("cargo test".into()),
        ..MomentIn::default()
    })
    .await;
    let asked = say(MomentIn {
        what: "end".into(),
        session: "c".into(),
        cwd: root_s.clone(),
        ..MomentIn::default()
    })
    .await;
    assert!(
        asked.contains("and you replaced what you wrote in tests/retry.rs on the way"),
        "{asked}"
    );
    assert_eq!(asked.matches("kelve:").count(), 1, "one question: {asked}");
    let both = std::sync::atomic::AtomicUsize::new(0);
    common::work(&app, |job| {
        let replaced = job.material["replaced"].as_str().unwrap();
        let failed = job.material["failed"].as_str().unwrap();
        if failed.contains("FAILED")
            && replaced.contains("tokio::time::sleep")
            && replaced.contains("tokio::time::advance")
        {
            both.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        json!({ "lessons": [], "same": [] })
    })
    .await;
    assert_eq!(both.into_inner(), 1);
}

/// Runs git for a test's fixture: no global or system configuration, and a
/// fixed author.
fn git(dir: &std::path::Path, args: &[&str]) {
    let ran = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(ran.status.success(), "git {args:?}");
}

/// What the shell wrote, in a repository or a folder that isn't one: a file
/// written by one heredoc and then replaced by another asks for a lesson; a
/// formatter's whitespace doesn't.
async fn shell_writes_in(repository: bool) {
    let g = StandIn::start().await.unwrap();
    let backoff = g.add(
        "Exponential backoff",
        "Waiting longer after each failure before trying again.",
        IdeaKind::Concept,
    );
    let app = kelve(Some(&g)).await;
    let root = common::folder(&[
        ("retry.py", "def charge(bank):\n    return bank.send()\n"),
        ("format.py", "def fetch(server):\n    return server.get()\n"),
    ]);
    if repository {
        git(&root, &["init", "-q"]);
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "Start"]);
    }
    let root_s = root.display().to_string();
    {
        let mut conn = app.db.acquire().await.unwrap();
        for path in ["retry.py", "format.py"] {
            kelve_store::reader::read_file(
                &mut conn,
                &root_s,
                path,
                &json!([{ "key": backoff, "name": "Exponential backoff" }]).to_string(),
                "h",
            )
            .await
            .unwrap();
        }
    }
    let at = |what: &str, session: &str, command: &str| MomentIn {
        what: what.into(),
        session: session.into(),
        cwd: root_s.clone(),
        command: Some(command.into()),
        ..MomentIn::default()
    };
    let say = |m: MomentIn| {
        let app = app.clone();
        async move {
            lessons::moment(State(app), Agent, Json(m))
                .await
                .unwrap()
                .0
                .text
        }
    };
    let pause = || tokio::time::sleep(std::time::Duration::from_millis(20));
    let first = "import time\n\ndef charge(bank, tries=5):\n    wait = 1\n    for _ in range(tries):\n        try:\n            return bank.send()\n        except Busy:\n            time.sleep(wait)\n            wait *= 2\n";
    let second = "import random\nimport time\n\ndef charge(bank, tries=5):\n    ceiling = 1\n    for _ in range(tries):\n        try:\n            return bank.send()\n        except Busy:\n            time.sleep(random.uniform(0, ceiling))\n            ceiling *= 2\n";

    // The person's message starts the session's moments.
    say(at("prompt", "s", "")).await;
    pause().await;
    std::fs::write(root.join("retry.py"), first).unwrap();
    say(at("ran", "s", "cat > retry.py <<'EOF'")).await;
    pause().await;
    // Written and tested in one command, as agents often do: it passed with
    // the second way, so it stands, and the turn's end asks.
    std::fs::write(root.join("retry.py"), second).unwrap();
    say(at(
        "ran",
        "s",
        "cat > retry.py <<'EOF' && python3 -m unittest",
    ))
    .await;
    let asked = say(at("end", "s", "")).await;
    assert!(
        asked.contains("you replaced what you wrote in retry.py"),
        "{asked}"
    );
    let jobs = common::work(&app, |job| {
        let replaced = job.material["replaced"].as_str().unwrap();
        assert!(replaced.contains("time.sleep(wait)"), "{replaced}");
        assert!(replaced.contains("random.uniform"), "{replaced}");
        json!({ "lessons": [], "same": [] })
    })
    .await;
    assert_eq!(jobs.len(), 1);

    // A formatter, as `cargo fmt` or `black` runs: only whitespace changes.
    let mine = "def fetch(server, tries=4):\n    for n in range(tries):\n        try:\n            return server.get()\n        except Down:\n            time.sleep(2 ** n)\n";
    // Spaces inside lines too, so more than a line or two reads otherwise.
    let formatted = "def fetch(server, tries = 4):\n  for n in range( tries ):\n    try:\n      return server.get( )\n    except Down:\n      time.sleep(2**n)\n\n";
    say(at("prompt", "f", "")).await;
    pause().await;
    std::fs::write(root.join("format.py"), mine).unwrap();
    say(at("ran", "f", "cat > format.py <<'EOF'")).await;
    pause().await;
    std::fs::write(root.join("format.py"), formatted).unwrap();
    say(at("ran", "f", "black format.py")).await;
    pause().await;
    assert_eq!(say(at("ran", "f", "python3 -m unittest")).await, "");
    assert_eq!(say(at("prompt", "f", "")).await, "", "nothing was replaced");

    // A file the agent made through its shell, that kelve has never read,
    // then replaced through it: asked for too.
    say(at("prompt", "n", "")).await;
    pause().await;
    std::fs::write(root.join("pay.py"), first).unwrap();
    say(at("ran", "n", "cat > pay.py <<'EOF'")).await;
    pause().await;
    std::fs::write(root.join("pay.py"), second).unwrap();
    say(at(
        "ran",
        "n",
        "cat > pay.py <<'EOF' && python3 -m unittest",
    ))
    .await;
    let asked = say(at("end", "n", "")).await;
    assert!(
        asked.contains("you replaced what you wrote in pay.py"),
        "{asked}"
    );
}

#[tokio::test]
async fn what_the_shell_wrote_in_a_repository_is_seen_as_written() {
    shell_writes_in(true).await;
}

#[tokio::test]
async fn what_the_shell_wrote_in_a_folder_that_isnt_a_repository_is_seen_as_written() {
    shell_writes_in(false).await;
}

#[tokio::test]
async fn only_a_lesson_given_a_hundred_times_is_ever_held_back_and_then_one_time_in_twenty() {
    let g = StandIn::start().await.unwrap();
    let rand = g.add("rand", "A Rust library for random numbers.", IdeaKind::Tool);
    let theirs = g.learned(
        Lesson {
            about: vec![rand.clone()],
            says: "rand 0.9 renamed `thread_rng()` to `rng()`.".into(),
            act: "Call `rand::rng()`.".into(),
            example: None,
            holds_when: "rand 0.9 and later.".into(),
            backed_by: None,
            seen_as: Some(seen_as(
                "error[E0425]: cannot find function `thread_rng` in crate `rand`",
            )),
        },
        Record::default(),
    );
    // One in twenty, as people's kelves have it.
    let mut config = kelve_server::config::test_config();
    config.endpoints.graph = g.url.clone();
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    config.hold_back = 50;
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate(&pool).await.unwrap();
    let app = kelve_server::state::App::new(config, pool).await.unwrap();
    let root = common::folder(&[("src/dice.rs", "fn roll() {}\n")]);
    let root_s = root.display().to_string();
    {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::reader::read_file(
            &mut conn,
            &root_s,
            "src/dice.rs",
            &json!([{ "key": rand, "name": "rand" }]).to_string(),
            "h",
        )
        .await
        .unwrap();
    }
    // Each moment in a session of its own, as many agents' would be: how
    // many of them kelve held it back from.
    let held_back = |from: usize, to: usize| {
        let app = app.clone();
        let root_s = root_s.clone();
        async move {
            for n in from..to {
                let _said = lessons::moment(
                    State(app.clone()),
                    Agent,
                    Json(MomentIn {
                        what: "failed".into(),
                        session: format!("s{n}"),
                        cwd: root_s.clone(),
                        command: Some("cargo build".into()),
                        problem: Some(
                            "error[E0425]: cannot find function `thread_rng` in crate `rand`"
                                .into(),
                        ),
                        ..MomentIn::default()
                    }),
                )
                .await
                .unwrap();
            }
            let mut conn = app.db.acquire().await.unwrap();
            let (given, withheld): (i64, i64) = sqlx::query_as(
                "select count(*), count(*) filter (where withheld) from given where session >= ? and session < ?",
            )
            .bind(format!("s{from}"))
            .bind(format!("s{to}"))
            .fetch_one(&mut *conn)
            .await
            .unwrap();
            (given, withheld)
        }
    };
    let fresh = || async {
        let mut conn = app.db.acquire().await.unwrap();
        sqlx::query("delete from lessons_fetched")
            .execute(&mut *conn)
            .await
            .unwrap();
        drop(conn);
        lessons::refresh(&app, std::slice::from_ref(&rand))
            .await
            .unwrap();
    };

    // Given 99 times: a lesson still finding its feet is always given.
    g.given(&theirs, kelve_core::lesson::HOLD_BACK_FROM - 1);
    fresh().await;
    let (given, withheld) = held_back(1000, 1400).await;
    assert_eq!((given, withheld), (400, 0), "never held back");

    // Given 100 times: about one moment in twenty.
    g.given(&theirs, kelve_core::lesson::HOLD_BACK_FROM);
    fresh().await;
    let (given, withheld) = held_back(2000, 3000).await;
    assert_eq!(given, 1000);
    assert!(
        (25..=80).contains(&withheld),
        "about 50 of 1000: {withheld}"
    );
}

#[tokio::test]
async fn a_folder_nobody_opened_is_walked_for_its_imports_and_lessons_reach_the_agent_there() {
    let g = StandIn::start().await.unwrap();
    let rand = g.add("rand", "A Rust library for random numbers.", IdeaKind::Tool);
    let theirs = g.learned(
        Lesson {
            about: vec![rand.clone()],
            says: "rand 0.9 calls the thread's generator `rand::rng()`; `thread_rng()` is the old name.".into(),
            act: "Call `rand::rng()` where `rand::thread_rng()` was.".into(),
            example: Some("let mut rng = rand::rng();".into()),
            holds_when: "rand 0.9 and later.".into(),
            backed_by: None,
            seen_as: Some(seen_as("rand::thread_rng()")),
        },
        Record::default(),
    );
    let app = kelve(Some(&g)).await;
    // A project the person never opened in kelve, in git, its agent working
    // in a folder inside it.
    let root = common::folder(&[
        ("Cargo.toml", "[package]\nname = \"dice\"\n"),
        (
            "src/lib.rs",
            "use rand::Rng;\n\npub fn roll() -> u32 {\n    1\n}\n",
        ),
        ("src/bin/main.rs", "fn main() {}\n"),
    ]);
    git(&root, &["init", "-q"]);
    let root_s = root.display().to_string();
    let inner = root.join("src").display().to_string();
    let at = |what: &str, session: &str| MomentIn {
        what: what.into(),
        session: session.into(),
        cwd: inner.clone(),
        ..MomentIn::default()
    };
    // The hook's first moment there walks the repository, in the background:
    // the moment answers at once.
    let said = lessons::moment(State(app.clone()), Agent, Json(at("prompt", "w")))
        .await
        .unwrap()
        .0
        .text;
    assert_eq!(said, "");
    // Walked, then its libraries looked up in the graph.
    let mut walked = None;
    for _ in 0..200 {
        let mut conn = app.db.acquire().await.unwrap();
        walked = kelve_store::lessons::walked(&mut conn, &root_s)
            .await
            .unwrap();
        let used = kelve_store::lessons::used_in(&mut conn, &root_s, &["src/lib.rs".into()])
            .await
            .unwrap();
        if walked.is_some_and(|(files, _)| files > 0) && !used.is_empty() {
            break;
        }
        drop(conn);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(
        walked,
        Some((3, false)),
        "the repository's root, walked once"
    );
    {
        let mut conn = app.db.acquire().await.unwrap();
        let used = kelve_store::lessons::used_in(&mut conn, &root_s, &["src/lib.rs".into()])
            .await
            .unwrap();
        assert_eq!(
            used,
            [(rand.clone(), "rand".to_string())],
            "found by its name"
        );
        // Nothing the person sees: not opened, nothing read or marked.
        assert!(
            kelve_store::reader::lately(&mut conn, 10)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            kelve_store::reader::read_files(&mut conn, &root_s)
                .await
                .unwrap()
                .is_empty()
        );
    }

    // As the agent writes the usual way there, it's given the lesson.
    let file = root.join("src/lib.rs");
    let written = "    let mut rng = rand::thread_rng();\n";
    std::fs::write(
        &file,
        format!("use rand::Rng;\n\npub fn roll() -> u32 {{\n{written}    1\n}}\n"),
    )
    .unwrap();
    let mut edit = at("edit", "w");
    edit.file = Some(file.display().to_string());
    edit.written = Some(written.into());
    let given = lessons::moment(State(app.clone()), Agent, Json(edit))
        .await
        .unwrap()
        .0
        .text;
    assert!(given.contains(&theirs), "{given}");
    assert!(given.contains("Instead of"), "{given}");
    // Walked once: a second session there doesn't walk it again.
    let _said = lessons::moment(State(app.clone()), Agent, Json(at("prompt", "x")))
        .await
        .unwrap();
    let mut conn = app.db.acquire().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("select count(*) from walked_roots")
            .fetch_one(&mut *conn)
            .await
            .unwrap(),
        1
    );
}

#[test]
fn the_home_folder_the_root_and_the_systems_folders_are_never_walked() {
    let walk = |p: &std::path::Path| lessons::walk_folder(&p.display().to_string());
    let home = std::env::home_dir().unwrap();
    assert_eq!(walk(&home), None);
    // As the system resolves it: on Windows, after `\\?\`.
    assert_eq!(walk(&std::fs::canonicalize(&home).unwrap()), None);
    // The root: `/`, or the home folder's drive.
    assert_eq!(walk(home.ancestors().last().unwrap()), None);
    let system = if cfg!(windows) {
        std::path::PathBuf::from(std::env::var("SystemRoot").unwrap()).join("System32")
    } else {
        "/usr/lib".into()
    };
    assert_eq!(walk(&system), None);
    assert_eq!(lessons::walk_folder("relative/path"), None);
    // A folder that holds the home folder.
    assert_eq!(walk(home.parent().unwrap()), None);
    // A project folder, outside git: itself.
    let project = common::folder(&[("notes.md", "# Notes\n")]);
    let project_s = project.display().to_string();
    assert_eq!(
        lessons::walk_folder(&project_s).as_deref(),
        Some(project_s.as_str())
    );
}

#[tokio::test]
async fn a_huge_folder_is_walked_up_to_its_cap() {
    let app = kelve(None).await;
    let root = common::folder(&[("README.md", "# Many\n")]);
    for n in 0..lessons::WALK_MOST + 5 {
        let dir = root.join(format!("d{}", n / 1000));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("f{n}.txt")), "").unwrap();
    }
    let root_s = root.display().to_string();
    {
        let mut conn = app.db.acquire().await.unwrap();
        assert!(
            kelve_store::lessons::start_walk(&mut conn, &root_s)
                .await
                .unwrap()
        );
    }
    lessons::walk(&app, &root_s).await.unwrap();
    let mut conn = app.db.acquire().await.unwrap();
    assert_eq!(
        kelve_store::lessons::walked(&mut conn, &root_s)
            .await
            .unwrap(),
        Some((i64::try_from(lessons::WALK_MOST).unwrap(), true))
    );
}

#[tokio::test]
async fn lessons_about_the_tools_an_agent_runs_reach_it_with_no_file_read() {
    let g = StandIn::start().await.unwrap();
    let pgrep = g.add(
        "pgrep",
        "A program that finds processes by name.",
        IdeaKind::Tool,
    );
    let cargo = g.add(
        "cargo",
        "Rust's build tool and package manager.",
        IdeaKind::Tool,
    );
    let waits = g.learned(
        Lesson {
            about: vec![pgrep.clone()],
            says: "`pgrep -f` matches its own shell's command line too, so a loop that waits for a process to go never ends.".into(),
            act: "Match the process by its pid file, or exclude your own shell with `pgrep -f '[p]attern'`.".into(),
            example: Some("until ! pgrep -f '[s]erver --port'; do sleep 1; done".into()),
            holds_when: "Any shell that runs the loop's command line as a process.".into(),
            backed_by: None,
            seen_as: Some(seen_as("pgrep -f 'server --port'")),
        },
        Record::default(),
    );
    let renamed = g.learned(
        Lesson {
            about: vec![cargo.clone()],
            says: "A method a crate renamed in a new major version is reported as missing, not as renamed.".into(),
            act: "Read the crate's changelog for the old name before rewriting the call.".into(),
            example: None,
            holds_when: "Any crate's new major version.".into(),
            backed_by: None,
            seen_as: Some(seen_as("error[E0599]: no method named `gen_range` found for struct `ThreadRng`")),
        },
        Record::default(),
    );
    let app = kelve(Some(&g)).await;
    // A project nobody opened, with nothing in it kelve could mark.
    let root = common::folder(&[("notes.txt", "Nothing to import here.\n")]);
    git(&root, &["init", "-q"]);
    let root_s = root.display().to_string();
    let say = |what: &str, command: &str, problem: Option<&str>| {
        let app = app.clone();
        let m = MomentIn {
            what: what.into(),
            session: "tools".into(),
            cwd: root_s.clone(),
            command: Some(command.into()),
            problem: problem.map(str::to_string),
            ..MomentIn::default()
        };
        async move {
            lessons::moment(State(app), Agent, Json(m))
                .await
                .unwrap()
                .0
                .text
        }
    };
    // The agent builds: kelve learns it runs cargo, and finds cargo in the
    // graph, in the background.
    say("ran", "cargo build", None).await;
    // It writes a wait the usual way: the lesson about pgrep reaches it as
    // it runs it, once kelve has found pgrep in the graph.
    let mut given = String::new();
    for _ in 0..100 {
        given = say(
            "ran",
            "until ! pgrep -f 'server --port'; do sleep 1; done",
            None,
        )
        .await;
        if !given.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(given.contains(&waits), "{given}");
    assert!(given.contains("pgrep"), "{given}");
    // Its own way already: nothing.
    assert_eq!(
        say(
            "ran",
            "until ! pgrep -f '[s]erver --port'; do sleep 1; done",
            None
        )
        .await,
        ""
    );
    // A build fails with what a cargo lesson answers: it's given, though
    // no file was ever read or marked.
    let mut answered = String::new();
    for _ in 0..100 {
        answered = say(
            "failed",
            "cargo test",
            Some("error[E0599]: no method named `random_range` found for struct `ThreadRng`"),
        )
        .await;
        if !answered.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(answered.contains(&renamed), "{answered}");
    // And asked, with the error alone, the agent's own tool finds it too.
    let asked = lessons::know(
        State(app.clone()),
        Agent,
        Json(lessons::KnowIn {
            path: root_s.clone(),
            ideas: Vec::new(),
            problem: Some("error[E0599]: no method named `x` found for struct `Y`".into()),
            write: false,
            agent: None,
        }),
    )
    .await
    .unwrap()
    .0
    .text;
    assert!(asked.contains(&renamed), "{asked}");
}
