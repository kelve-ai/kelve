//! Marking a file, end to end, with a stand-in for the person's agent and
//! for the shared graph: what's marked, what's dropped, which idea each
//! mark finds, that an unchanged passage is never read twice, and that
//! what's read is kept in the project for everyone working in it.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use common::{caller, folder, kelve, unfenced, work};
use kelve_core::ideas::IdeaKind;
use kelve_graph_api::stand_in::StandIn;
use kelve_server::agent::{self, HandedIn, Job, Priority, Resume};
use kelve_server::pipeline::mark::{self, For, Waiting};
use kelve_server::state::Shared;
use serde_json::{Value, json};

const LEDGER: &str = "use sqlx::PgPool;

/// Holds back the most a call could cost, before it's made.
pub async fn reserve(tx: &mut Transaction<'_>, key: i64) -> Result<i64> {
    // Take the lock in the transaction, so it's released on commit.
    sqlx::query(\"select pg_advisory_xact_lock($1)\").bind(key).execute(&mut **tx).await?;
    let spent = total(tx).await?;
    Ok(spent)
}

/// Settles a call at what it cost, once the provider has said.
/// A call that failed costs nothing.
pub async fn settle(tx: &mut Transaction<'_>, id: i64, cost: f64) -> Result<()> {
    sqlx::query(\"update ledger set spent = $2 where id = $1\").bind(id).bind(cost).execute(&mut **tx).await?;
    Ok(())
}

/// What the day has cost so far, settled and held.
pub async fn today(tx: &mut Transaction<'_>, day: &str) -> Result<(f64, f64)> {
    let row = sqlx::query(\"select total(spent), total(reserved) from ledger where day = $1\")
        .bind(day)
        .fetch_one(&mut **tx)
        .await?;
    let spent: f64 = row.get(0);
    let held: f64 = row.get(1);
    Ok((spent, held))
}
";

/// Marking asked for directly, as a test does: nothing to run again.
fn asked() -> For {
    For {
        opened: None,
        priority: Priority::Now,
        resume: Resume::job("test", "test", json!({})),
    }
}

/// The stand-in agent's reading: in the passage that takes the lock, a mark
/// by its candidate, one by a quote of the comment above it, an invented
/// quote, and one too basic to keep.
fn read(job: &Job) -> Value {
    let mark = |candidate: &str, line: u32, quote: &str, name: &str| {
        json!({
            "candidate": candidate, "line": line, "quote": quote, "lastLine": 0,
            "name": name, "definition": "A lock a program names, and the database keeps.",
            "why": "It's why two calls can't both pass.",
        })
    };
    let passages: Vec<Value> = job.material["passages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let candidates = unfenced(&p["candidates"]);
            let lock = candidates
                .lines()
                .find(|l| l.contains("pg_advisory_xact_lock"))
                .and_then(|l| l.split_whitespace().next());
            let marks = match lock {
                Some(c) => vec![
                    mark(c, 0, "", "Advisory locks"),
                    mark("", 5, "released on commit", "Transaction-scoped locks"),
                    mark("", 5, "freed at the end", "Invented"),
                    mark("", 4, "pub async fn", "Function"),
                ],
                None => vec![],
            };
            json!({ "passage": p["id"], "marks": marks })
        })
        .collect();
    json!({ "passages": passages })
}

/// The stand-in agent's checking: only the first mark matters, and it's the
/// graph's first idea.
fn choose(job: &Job) -> Value {
    let n = job.material["marks"].as_array().unwrap().len();
    let checks: Vec<Value> = (1..=n)
        .map(|n| {
            json!({
                "mark": n, "named": true, "instance": true,
                "matters": n == 1, "idea": if n == 1 { 1 } else { 0 },
            })
        })
        .collect();
    json!({ "checks": checks })
}

/// The stand-in agent, for every job these tests give it.
fn agent(job: &Job) -> Value {
    match job.kind.as_str() {
        "read" => read(job),
        "choose" => {
            // What the marks were offered is kelve's own: the agent has
            // it in each mark.
            assert!(job.material["offered"].is_null(), "{}", job.material);
            choose(job)
        }
        "builtOn" => json!({ "chosen": [
            { "n": 1, "why": "How two calls never both pass the ceiling." },
            { "n": 9, "why": "Not one it was given." },
        ] }),
        "relate" => {
            let idea = unfenced(&job.material["idea"]);
            let rests_on = if idea.contains("The idea: Advisory locks") {
                "Database transaction"
            } else if idea.contains("The idea: Database transaction") {
                "Atomicity"
            } else {
                "State"
            };
            json!({
                "question": "What does it answer?",
                "restsOn": [{ "name": rests_on, "definition": "What it rests on.", "why": "It takes this from it." }],
                "kindOf": [], "answers": [], "alternatives": [], "usedWith": [],
            })
        }
        "way" => common::way(&job.material),
        "path" => json!({ "parts": [] }),
        "folders" => {
            let folders: Vec<Value> = job.material["folders"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| json!({ "path": p, "line": "Where the money is kept." }))
                .collect();
            json!({ "folders": folders })
        }
        other => common::plainly(other, &job.material)
            .unwrap_or_else(|| panic!("a job these tests don't give: {other}")),
    }
}

/// Marks a file, having the agent do what it's asked, until it's marked:
/// the marks, and the jobs the agent was given.
async fn marked(app: &Shared, root: &std::path::Path, rel: &str) -> (mark::Marked, Vec<Job>) {
    let mut given = Vec::new();
    for _ in 0..5 {
        let m = mark::file(app, root, rel, &asked()).await.unwrap();
        if m.waiting.is_none() {
            return (m, given);
        }
        given.extend(work(app, agent).await);
    }
    panic!("never marked");
}

/// How many files the path through what's open leaves off, once kelve has
/// found them in the background.
async fn left_off(app: &Shared, open: &kelve_server::reader::Open) -> u32 {
    for _ in 0..100 {
        let way = kelve_server::way::shown(app, open).await.unwrap();
        if way.left_off > 0 {
            return way.left_off;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    0
}

async fn open_folder(app: &Shared, root: &std::path::Path) -> i64 {
    let mut conn = app.db.acquire().await.unwrap();
    kelve_store::reader::open(
        &mut conn,
        "folder",
        &root.display().to_string(),
        "",
        "",
        "ledger",
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn a_file_is_read_once_by_the_agent_sparingly_with_the_graphs_ideas() {
    let graph = StandIn::start().await.unwrap();
    let locks = graph.add(
        "Advisory locks",
        "A lock an application names and takes through the database.",
        IdeaKind::Concept,
    );
    let app = kelve(Some(&graph)).await;
    let root = folder(&[("src/ledger.rs", LEDGER)]);

    let (marked, given) = marked(&app, &root, "src/ledger.rs").await;
    // Two jobs for the whole file: reading every passage, then checking
    // what was marked and choosing each one's idea.
    let kinds: Vec<&str> = given.iter().map(|j| j.kind.as_str()).collect();
    assert_eq!(kinds, ["read", "choose"]);
    let parts = kelve_engine::passages::passages("src/ledger.rs", LEDGER).len();
    assert!(parts > 1);
    assert_eq!(
        given[0].material["passages"].as_array().unwrap().len(),
        parts
    );
    assert_eq!(given[0].about, "Reading ledger.rs");
    // What it's told comes from the graph, and the file is material, fenced.
    let served = kelve_core::instructions::get("read").unwrap();
    assert_eq!(given[0].instructions, served.text);
    assert_eq!(given[0].version, format!("read@{}", served.version));
    assert!(
        given[0].material["passages"][0]["passage"]
            .as_str()
            .unwrap()
            .starts_with("<passage>")
    );
    // The invented quote and the basic idea never get to be checked.
    let checked: Vec<&str> = given[1].material["marks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap())
        .collect();
    assert!(!checked.is_empty());
    assert!(
        checked
            .iter()
            .all(|m| !m.contains("Invented") && !m.contains("name=\"Function\"")),
        "{checked:?}"
    );

    assert_eq!(marked.marks.len(), 1, "{marked:?}");
    let m = &marked.marks[0];
    assert_eq!(m.name, "Advisory locks");
    assert_eq!(m.idea.as_ref().map(|i| i.id.as_str()), Some(locks.as_str()));
    let span = m.span.clone().expect("words");
    let words: String = LEDGER
        .lines()
        .nth(span.line as usize - 1)
        .unwrap()
        .chars()
        .skip(span.from as usize)
        .take((span.to - span.from) as usize)
        .collect();
    assert_eq!(words, "\"select pg_advisory_xact_lock($1)\"");

    // Unchanged, it's never read again. Moved down the file, it keeps its
    // marks, moved with it.
    let again = mark::file(&app, &root, "src/ledger.rs", &asked())
        .await
        .unwrap();
    assert_eq!(again, marked);
    std::fs::write(root.join("src/ledger.rs"), format!("\n\n{LEDGER}")).unwrap();
    let moved = mark::file(&app, &root, "src/ledger.rs", &asked())
        .await
        .unwrap();
    assert!(
        agent::next(&app, &caller()).await.unwrap().is_none(),
        "nothing asked again"
    );
    assert_eq!(
        moved.marks[0].span.as_ref().map(|s| s.line),
        Some(span.line + 2)
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn until_the_agent_answers_nothing_is_marked_and_the_job_waits_for_it() {
    // No graph: the instructions are the ones kelve was built with.
    let app = kelve(None).await;
    let root = folder(&[("src/ledger.rs", LEDGER)]);
    let marked = mark::file(&app, &root, "src/ledger.rs", &asked())
        .await
        .unwrap();
    assert_eq!(marked.marks, vec![]);
    assert_eq!(marked.waiting, Some(Waiting::Agent));
    // Asking again is the same job.
    mark::file(&app, &root, "src/ledger.rs", &asked())
        .await
        .unwrap();
    let job = agent::next(&app, &caller()).await.unwrap().expect("a job");
    assert_eq!(job.kind, "read");
    assert_eq!(job.waiting, 0);
    assert_eq!(
        job.instructions,
        kelve_core::instructions::get("read").unwrap().text
    );
    assert!(agent::next(&app, &caller()).await.unwrap().is_none());
    assert!(
        mark::file(&app, &root, "../elsewhere.rs", &asked())
            .await
            .is_err()
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn an_answer_in_the_wrong_shape_is_handed_back_then_put_aside() {
    let app = kelve(None).await;
    let root = folder(&[("src/ledger.rs", LEDGER)]);
    mark::file(&app, &root, "src/ledger.rs", &asked())
        .await
        .unwrap();
    let job = agent::next(&app, &caller()).await.unwrap().unwrap();
    let wrong = json!({ "passages": [{ "passage": "p99", "marks": [] }] });
    let first = agent::hand_in(&app, &caller(), job.job, wrong.clone())
        .await
        .unwrap();
    let HandedIn::No { why, put_aside } = first else {
        panic!("taken: {first:?}");
    };
    assert!(why.contains("p99") && !put_aside, "{why}");
    // It stays with the agent fixing it, not offered to another; wrong
    // again, it's put aside.
    assert!(agent::next(&app, &caller()).await.unwrap().is_none());
    let second = agent::hand_in(&app, &caller(), job.job, wrong)
        .await
        .unwrap();
    assert!(matches!(
        second,
        HandedIn::No {
            put_aside: true,
            ..
        }
    ));
    assert!(agent::next(&app, &caller()).await.unwrap().is_none());
    // An answer to a job that's gone is refused, with what to do.
    let gone = agent::hand_in(&app, &caller(), job.job, json!({ "passages": [] })).await;
    assert!(gone.is_err());
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn without_the_graph_marks_stand_but_wait_for_their_ideas() {
    let app = kelve(None).await;
    let root = folder(&[("src/ledger.rs", LEDGER)]);
    let (marked, _) = marked(&app, &root, "src/ledger.rs").await;
    assert_eq!(marked.marks.len(), 1);
    assert_eq!(
        marked.marks[0].idea, None,
        "a new idea, not one of the graph's"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn opening_a_file_has_it_read_and_the_folder_is_built_on_what_it_found() {
    let app = kelve(None).await;
    let root = folder(&[("src/ledger.rs", LEDGER)]);
    let id = open_folder(&app, &root).await;
    let mut events = app.events.subscribe();
    let payload = json!({ "opened": id, "path": "src/ledger.rs", "priority": 0 });
    kelve_server::reader::read_job(&app, &payload, false)
        .await
        .unwrap();
    // Until the agent answers, it isn't read.
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    assert!(
        kelve_server::reader::files_read(&app, &open)
            .await
            .unwrap()
            .is_empty()
    );
    let given = work(&app, agent).await;
    let kinds: Vec<&str> = given.iter().map(|j| j.kind.as_str()).collect();
    // Read, then read plainly, as part of reading it.
    assert_eq!(kinds, ["read", "choose", "plainly"]);
    // The reader heard as the marks came in.
    let mut heard = Vec::new();
    while let Ok(e) = events.try_recv() {
        heard.push(e);
    }
    assert!(heard.contains(&kelve_core::views::ServerEvent::Marks {
        opened: id,
        path: "src/ledger.rs".into()
    }));
    let text = std::fs::read_to_string(root.join("src/ledger.rs")).unwrap();
    let (marks, whole) = mark::kept(&app, "src/ledger.rs", &text, None)
        .await
        .unwrap();
    assert!(whole && marks.len() == 1, "{marks:?}");
    let key = kelve_server::reader::key_of(&marks[0]);
    let mut conn = app.db.acquire().await.unwrap();
    assert!(
        kelve_store::entries::idea(&mut conn, &key)
            .await
            .unwrap()
            .is_some()
    );
    drop(conn);

    // What the folder is built on, chosen by the agent from what was found:
    // only ideas it was given.
    let asked_for = kelve_server::reader::folder_built_on(&app, &open, false)
        .await
        .unwrap();
    assert!(asked_for.is_empty(), "asked for, not there yet");
    let given = work(&app, agent).await;
    assert_eq!(
        given.iter().map(|j| j.kind.as_str()).collect::<Vec<_>>(),
        ["builtOn"]
    );
    let built = kelve_server::reader::folder_built_on(&app, &open, false)
        .await
        .unwrap();
    assert_eq!(built.len(), 1);
    assert_eq!(built[0].name, "Advisory locks");
    assert_eq!(built[0].why, "How two calls never both pass the ceiling.");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_built_on_answer_given_while_files_are_still_read_is_kept_not_asked_again() {
    // With several agents at once, files are read between the asking and the
    // answer. The answer must be taken against the ideas it was given, not
    // asked for again because the folder moved on.
    let app = kelve(None).await;
    let root = folder(&[("src/ledger.rs", LEDGER), ("src/copy.rs", LEDGER)]);
    let id = open_folder(&app, &root).await;
    let read = |path: &str| json!({ "opened": id, "path": path, "priority": 0 });
    kelve_server::reader::read_job(&app, &read("src/ledger.rs"), false)
        .await
        .unwrap();
    work(&app, agent).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    kelve_server::reader::folder_built_on(&app, &open, false)
        .await
        .unwrap();
    common::jobs(&app).await;
    let asked = agent::next(&app, &caller()).await.unwrap().unwrap();
    assert_eq!(asked.kind, "builtOn");

    // Another file read before the answer comes back.
    kelve_server::reader::read_job(&app, &read("src/copy.rs"), false)
        .await
        .unwrap();
    let handed = agent::hand_in(&app, &caller(), asked.job, agent(&asked))
        .await
        .unwrap();
    assert_eq!(handed, HandedIn::Yes);
    let after = work(&app, agent).await;
    assert!(
        after.iter().all(|j| j.kind != "builtOn"),
        "the answer was taken, not asked for again: {:?}",
        after.iter().map(|j| &j.about).collect::<Vec<_>>()
    );
    let built = kelve_server::reader::folder_built_on(&app, &open, true)
        .await
        .unwrap();
    assert_eq!(
        built.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
        ["Advisory locks"]
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_mark_named_like_an_idea_kelve_has_is_offered_it_and_joins_it() {
    // Several agents name one idea several ways; the graph hasn't taken
    // kelve's new ideas in yet, so the check offers them too.
    let app = kelve(None).await;
    let other = LEDGER.replace("Holds back", "Keeps back");
    let root = folder(&[("src/ledger.rs", LEDGER), ("src/other.rs", &other)]);
    let id = open_folder(&app, &root).await;
    let read = |path: &str| json!({ "opened": id, "path": path, "priority": 0 });
    kelve_server::reader::read_job(&app, &read("src/ledger.rs"), false)
        .await
        .unwrap();
    work(&app, agent).await;

    kelve_server::reader::read_job(&app, &read("src/other.rs"), false)
        .await
        .unwrap();
    let offered = std::sync::Mutex::new(String::new());
    work(&app, |job: &Job| {
        let mut answer = agent(job);
        if job.kind == "read" {
            // This agent says it another way.
            let text = answer
                .to_string()
                .replace("Advisory locks", "Advisory locking");
            answer = serde_json::from_str(&text).unwrap();
        }
        if job.kind == "choose" {
            *offered.lock().unwrap() = job.material["marks"].to_string();
        }
        answer
    })
    .await;
    assert!(
        offered
            .lock()
            .unwrap()
            .contains("Advisory locks (marked here before, not in the shared graph yet)"),
        "{}",
        offered.lock().unwrap()
    );
    let (marks, _) = mark::kept(&app, "src/other.rs", &other, None)
        .await
        .unwrap();
    assert_eq!(
        marks.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
        ["Advisory locks"],
        "chosen, it joins the idea kelve has"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn while_the_graph_cant_be_asked_marks_wait_rather_than_choose_from_nothing() {
    let mut config = kelve_server::config::test_config();
    // Nothing listens here.
    config.endpoints.graph = "http://127.0.0.1:9".into();
    // Reading the graph is for someone signed in.
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate(&pool).await.unwrap();
    let app = kelve_server::state::App::new(config, pool).await.unwrap();
    let root = folder(&[("src/ledger.rs", LEDGER)]);
    let text = std::fs::read_to_string(root.join("src/ledger.rs")).unwrap();
    let first = mark::file(&app, &root, "src/ledger.rs", &asked())
        .await
        .unwrap();
    assert_eq!(first.waiting, Some(Waiting::Agent), "read first");
    let job = agent::next(&app, &caller()).await.unwrap().unwrap();
    agent::hand_in(&app, &caller(), job.job, agent(&job))
        .await
        .unwrap();
    assert!(
        mark::file(&app, &root, "src/ledger.rs", &asked())
            .await
            .is_err(),
        "the job fails, to be tried again"
    );
    assert!(
        agent::next(&app, &caller()).await.unwrap().is_none(),
        "no check with nothing to choose from"
    );
    let (marks, whole) = mark::kept(&app, "src/ledger.rs", &text, None)
        .await
        .unwrap();
    assert!(marks.is_empty() && !whole);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn asking_again_for_what_waits_on_the_agent_tells_the_reader_nothing() {
    // The reader asks again for what it hears changed: telling it of work
    // that's only asked for again made it ask without end.
    let app = kelve(None).await;
    let root = folder(&[("src/ledger.rs", LEDGER)]);
    let id = open_folder(&app, &root).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    // Its way in, as the agent gave it: asking for one is news.
    let step = kelve_server::way::StepIn {
        path: "src/ledger.rs".into(),
        why: "All there is.".into(),
    };
    kelve_server::way::given(&app, &open, &[step], &[])
        .await
        .unwrap();
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let payload = json!({ "opened": id, "path": "src/ledger.rs", "priority": 0 });
    kelve_server::reader::read_job(&app, &payload, false)
        .await
        .unwrap();
    let mut events = app.events.subscribe();
    for _ in 0..3 {
        kelve_server::reader::forget_ranking(&open.root);
        kelve_server::reader::read_folder_job(&app, &json!({ "opened": id }))
            .await
            .unwrap();
        kelve_server::reader::read_job(&app, &payload, false)
            .await
            .unwrap();
    }
    assert!(
        events.try_recv().is_err(),
        "nothing came in, so nothing to hear"
    );
    // Once the agent answers, the reader hears.
    work(&app, agent).await;
    assert!(events.try_recv().is_ok());
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn opening_a_folder_has_the_agent_read_every_file_the_ones_the_rest_lean_on_first() {
    let app = kelve(None).await;
    let root = folder(&[
        ("README.md", "# Ledger\n\nWhat calls cost.\n"),
        ("src/ledger.rs", LEDGER),
        (
            "src/spend.rs",
            "use crate::ledger;\n\npub fn spend() { ledger::reserve(); }\n",
        ),
        ("data.dat", "\0\0\0binary"),
        ("docs/notes.md", "# Notes\n"),
        ("src/other.rs", "pub fn other() {}\n"),
        ("tests/ledger_test.rs", "use kelve::ledger;\n"),
    ]);
    let id = open_folder(&app, &root).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let reading = kelve_server::reader::reading(&app, &open, 0, 7)
        .await
        .unwrap();
    assert_eq!(reading.marking, kelve_core::views::Marking::Marking);
    let given = work(&app, agent).await;
    let reads: Vec<&str> = given
        .iter()
        .filter(|j| j.kind == "read")
        .map(|j| j.about.as_str())
        .collect();
    // The way in and the file the rest lean on come first.
    assert!(reads.len() >= 3, "{reads:?}");
    assert!(
        reads[..2].contains(&"Reading README.md") && reads[..2].contains(&"Reading ledger.rs"),
        "{reads:?}"
    );
    // Every file on the way in: the text files, as the way in lists only
    // those; data kelve can't read isn't on it, so it's never tried.
    let all = kelve_server::reader::files_read(&app, &open).await.unwrap();
    assert_eq!(all.len(), 6, "{all:?}");
    assert!(!all.contains_key("data.dat"));
    assert!(!all["src/ledger.rs"].ideas.is_empty());
    // Every folder has a line on what it's for, from the agent's way in.
    assert_eq!(given[0].kind, "way", "the way in first");
    let kept = kelve_server::way::kept(&app, &open.root).await;
    assert_eq!(
        kept.folders.keys().collect::<Vec<_>>(),
        ["docs", "src", "tests"]
    );

    // Unchanged, nothing's read again; a file that changed is, and only its
    // passages that changed go to the agent.
    kelve_server::reader::forget_ranking(&open.root);
    kelve_server::reader::read_folder_job(&app, &json!({ "opened": id }))
        .await
        .unwrap();
    let again: Vec<String> = work(&app, agent)
        .await
        .iter()
        .map(|j| format!("{} {}", j.kind, j.about))
        .collect();
    assert!(again.is_empty(), "{again:?}");
    std::fs::write(
        root.join("src/ledger.rs"),
        format!("{LEDGER}\n/// And one more.\npub fn refund() {{}}\n"),
    )
    .unwrap();
    kelve_server::reader::forget_ranking(&open.root);
    kelve_server::reader::read_folder_job(&app, &json!({ "opened": id }))
        .await
        .unwrap();
    let again = work(&app, agent).await;
    let passages = again[0].material["passages"].as_array().unwrap().len();
    assert!((1..3).contains(&passages), "only what changed: {passages}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn what_is_read_is_kept_in_the_project_so_no_one_reads_it_twice() {
    let root = folder(&[
        ("src/ledger.rs", LEDGER),
        ("src/other.rs", "pub fn other() {}\n"),
    ]);
    let git = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(git.success());
    let app = kelve(None).await;
    let id = open_folder(&app, &root).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    kelve_server::reader::read_folder_job(&app, &json!({ "opened": id }))
        .await
        .unwrap();
    assert!(!work(&app, agent).await.is_empty());
    // Kept in the project, under git's id for each file's text.
    let blob = kelve_engine::git::blob_id(LEDGER.as_bytes()).unwrap();
    let kept = root
        .join(".kelve/read")
        .join(&blob[..2])
        .join(format!("{blob}.json"));
    let read: kelve_server::read::Read =
        serde_json::from_str(&std::fs::read_to_string(&kept).unwrap()).unwrap();
    assert_eq!(read.path, "src/ledger.rs");
    assert!(read.version.starts_with("read@"));
    assert!(!read.passages.is_empty());
    drop(open);

    // Someone else's kelve, on the same project as it's pulled: nothing to
    // read, and the marks are there.
    let theirs = kelve(None).await;
    let id = open_folder(&theirs, &root).await;
    kelve_server::reader::read_folder_job(&theirs, &json!({ "opened": id }))
        .await
        .unwrap();
    // Nothing read twice: only read plainly, the files and their folders'
    // pages, which each kelve keeps itself.
    let given = work(&theirs, agent).await;
    assert!(
        given.iter().all(|j| j.kind.starts_with("plainly")),
        "nothing read twice: {:?}",
        given.iter().map(|j| &j.kind).collect::<Vec<_>>()
    );
    let (marks, whole) = mark::kept(&theirs, "src/ledger.rs", LEDGER, None)
        .await
        .unwrap();
    assert!(whole && marks.len() == 1, "{marks:?}");

    // A file that changes is kept again, and its old version's file goes.
    let changed = format!("{LEDGER}\npub fn refund() {{}}\n");
    std::fs::write(root.join("src/ledger.rs"), &changed).unwrap();
    let open = kelve_server::reader::opened(&theirs, id).await.unwrap();
    kelve_server::reader::forget_ranking(&open.root);
    kelve_server::reader::read_folder_job(&theirs, &json!({ "opened": id }))
        .await
        .unwrap();
    work(&theirs, agent).await;
    let new_blob = kelve_engine::git::blob_id(changed.as_bytes()).unwrap();
    assert!(
        root.join(".kelve/read")
            .join(&new_blob[..2])
            .join(format!("{new_blob}.json"))
            .exists()
    );
    assert!(!kept.exists(), "the old version's file went");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_kept_file_grows_for_whoever_asks_for_more_and_files_with_one_text_share_it() {
    const TWIN: &str = "/// Holds funds for a while, with an advisory lock.\npub fn hold() {}\n";
    let root = folder(&[("src/a.rs", TWIN), ("src/b.rs", TWIN)]);
    let git = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(git.success());
    let app = kelve(None).await;
    let id = open_folder(&app, &root).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let read_all = || async {
        kelve_server::reader::forget_ranking(&open.root);
        kelve_server::reader::read_folder_job(&app, &json!({ "opened": id }))
            .await
            .unwrap();
        work(&app, agent).await
    };
    read_all().await;
    let kept_for = |text: &str| {
        let blob = kelve_engine::git::blob_id(text.as_bytes()).unwrap();
        root.join(".kelve/read")
            .join(&blob[..2])
            .join(format!("{blob}.json"))
    };
    let kept = |file: &std::path::Path| -> kelve_server::read::Read {
        serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
    };
    // One file for both: they share the text, so they share the marks.
    let twin = kept_for(TWIN);
    let read = kept(&twin);
    assert_eq!(
        (read.path.as_str(), read.also.as_slice()),
        ("src/a.rs", ["src/b.rs".to_string()].as_slice())
    );
    let version = mark::read_version(&app).await;
    let wanted = mark::lens_for(&app).0;
    assert!(kelve_server::read::kept_as_is(
        &open, "src/a.rs", &version, &wanted
    ));
    assert!(
        !kelve_server::read::kept_as_is(&open, "src/a.rs", "read@0", &wanted),
        "read under other instructions, it isn't read as reading is now"
    );

    // Someone asking for more: only that is read, and kept with what was
    // found before, under the same instructions.
    let more = kelve_core::spec::Spec {
        mark: kelve_core::lens::Level::New,
        ..Default::default()
    };
    kelve_server::spec::save(&app, more, "you").unwrap();
    let asks_more = mark::lens_for(&app).0;
    assert!(!kelve_server::read::kept_as_is(
        &open, "src/a.rs", &version, &asks_more
    ));
    let topped = read_all().await;
    let read_job = topped
        .iter()
        .find(|j| j.kind == "read")
        .expect("read again");
    assert!(
        read_job.material["lookFor"]
            .as_str()
            .unwrap()
            .contains("the basics too")
    );
    assert_eq!(kept(&twin).version, version);
    assert!(kelve_server::read::kept_as_is(
        &open, "src/a.rs", &version, &asks_more
    ));
    assert!(
        kelve_server::read::kept_as_is(&open, "src/a.rs", &version, &wanted),
        "and it still serves whoever asked for less"
    );

    // One of the two changes: the shared file stays for the other.
    let changed = format!("{TWIN}pub fn release() {{}}\n");
    std::fs::write(root.join("src/a.rs"), &changed).unwrap();
    read_all().await;
    assert!(kept_for(&changed).exists());
    let read = kept(&twin);
    assert_eq!((read.path.as_str(), read.also.len()), ("src/b.rs", 0));
    // The other changes too: no file has that text, so its file goes.
    std::fs::write(root.join("src/b.rs"), &changed).unwrap();
    read_all().await;
    assert!(!twin.exists(), "no file has that text any more");
    let read = kept(&kept_for(&changed));
    assert_eq!(
        (read.path.as_str(), read.also.as_slice()),
        ("src/a.rs", ["src/b.rs".to_string()].as_slice())
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_graph_that_never_answers_slows_kelve_once_at_most() {
    // A graph that takes the connection and never says anything, as one
    // that isn't there can seem to.
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", silent.local_addr().unwrap());
    let held = tokio::spawn(async move {
        let mut open = Vec::new();
        while let Ok((socket, _)) = silent.accept().await {
            open.push(socket);
        }
    });
    let mut config = kelve_server::config::test_config();
    config.endpoints.graph = url;
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate(&pool).await.unwrap();
    let app = kelve_server::state::App::new(config, pool).await.unwrap();

    // The first ask waits for the graph, a little; then kelve's own are used.
    let first = std::time::Instant::now();
    assert_eq!(
        agent::version(&app, "read").await,
        format!(
            "read@{}",
            kelve_core::instructions::get("read").unwrap().version
        )
    );
    assert!(first.elapsed() < std::time::Duration::from_secs(5));
    // Every ask after that is at once, however many a page makes.
    let after = std::time::Instant::now();
    for _ in 0..400 {
        agent::version(&app, "read").await;
    }
    assert!(
        after.elapsed() < std::time::Duration::from_millis(200),
        "{:?}",
        after.elapsed()
    );
    held.abort();
}

#[tokio::test]
async fn a_lot_to_read_with_no_way_in_is_read_along_a_path_the_agent_gives() {
    let mut files: Vec<(String, String)> = vec![(
        "README.md".into(),
        "# ledger\n\nMoney in, money out.\n".into(),
    )];
    for n in 1..=24 {
        files.push((
            format!("src/f{n:02}.rs"),
            format!("pub fn f{n:02}() {{}}\n"),
        ));
    }
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let root = folder(&named);
    let app = kelve(None).await;
    let id = open_folder(&app, &root).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    // Opened with a lot to read and no way in: reading starts, and the
    // agent's first job is finding one.
    kelve_server::reader::read_ahead(&app, &open).await.unwrap();
    kelve_server::way::ask_way(&app, id).await.unwrap();
    let way = kelve_server::way::view(&app, &open).await.unwrap();
    assert!(way.finding && way.order.is_empty(), "finding a way in");
    let read_of = |given: &[kelve_server::agent::Job]| -> Vec<String> {
        given
            .iter()
            .filter(|j| j.kind == "read")
            .map(|j| j.about.trim_start_matches("Reading ").to_string())
            .collect()
    };
    let given = work(&app, |job| match job.kind.as_str() {
        "way" => {
            let files = job.material["files"].as_array().unwrap();
            assert_eq!(files.len(), 25, "every file to read, listed");
            assert!(unfenced(&job.material["about"]).contains("Money in, money out."));
            // Its folders, to say what each is for, with what's in them.
            assert_eq!(job.material["folders"], json!(["src"]));
            assert!(unfenced(&job.material["inFolders"]).contains("src/: f01.rs"));
            let late: Vec<String> = (12..=22).rev().map(|n| format!("src/f{n:02}.rs")).collect();
            let early: Vec<String> = (1..=11).map(|n| format!("src/f{n:02}.rs")).collect();
            json!({
                "order": [
                    { "path": "README.md", "why": "What ledger is, in its own words." },
                    { "path": "src/f24.rs", "why": "Where it starts." },
                    { "path": "src/f23.rs", "why": "What it calls first." },
                ],
                "parts": [
                    { "name": "The end first", "why": "It leads on from the way in.", "paths": late },
                    { "name": "Then the start", "why": "What the end was built on.", "paths": early },
                ],
                "folders": [{ "path": "src", "line": "Every function the ledger has." }],
            })
        }
        "path" => panic!("the rest of the path comes with the way in"),
        "folders" => panic!("each folder's line comes with the way in"),
        _ => agent(job),
    })
    .await;
    assert_eq!(given[0].kind, "way", "the way in comes first");
    // Only the way in is read, in its order: the rest waits for the person.
    assert_eq!(read_of(&given), ["README.md", "f24.rs", "f23.rs"]);
    assert!(
        given.iter().any(|j| j.kind == "plainlyFolder"),
        "the pages of the folders its files are in"
    );
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let way = kelve_server::way::view(&app, &open).await.unwrap();
    assert!(!way.finding);
    assert_eq!(way.order.len(), 3);
    assert_eq!(
        way.parts
            .iter()
            .map(|p| (p.name.as_str(), p.queued))
            .collect::<Vec<_>>(),
        [("The end first", false), ("Then the start", false)]
    );
    assert_eq!(
        way.folders[0].line, "Every function the ledger has.",
        "from the way in, so Files has it at once"
    );

    // The person asks for a part: it's read, in its order, and no more.
    kelve_server::way::read_part(&app, &open, "Then the start")
        .await
        .unwrap();
    let given = work(&app, agent).await;
    let early: Vec<String> = (1..=11).map(|n| format!("f{n:02}.rs")).collect();
    assert_eq!(read_of(&given), early);
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let way = kelve_server::way::view(&app, &open).await.unwrap();
    assert!(way.parts[1].queued && !way.parts[0].queued);
    assert!(
        kelve_server::way::read_part(&app, &open, "Not a part")
            .await
            .is_err()
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_way_in_given_with_a_lot_to_read_has_the_rest_of_the_path_asked_at_once() {
    let mut files: Vec<(String, String)> = Vec::new();
    for n in 1..=24 {
        files.push((
            format!("src/f{n:02}.rs"),
            format!("pub fn f{n:02}() {{}}\n"),
        ));
    }
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let root = folder(&named);
    let app = kelve(None).await;
    let id = open_folder(&app, &root).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    // As the agent opens it with a way in: the rest of the path is asked
    // for before anything's read.
    let step = |p: &str| kelve_server::way::StepIn {
        path: p.into(),
        why: "Where it starts.".into(),
    };
    kelve_server::way::given(&app, &open, &[step("src/f01.rs"), step("src/f02.rs")], &[])
        .await
        .unwrap();
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    assert!(kelve_server::way::has_way(&open).await);
    kelve_server::way::ask_rest(&app, id).await.unwrap();
    kelve_server::reader::read_ahead(&app, &open).await.unwrap();
    let given = work(&app, |job| match job.kind.as_str() {
        "path" => {
            let files = job.material["files"].as_array().unwrap();
            assert_eq!(files.len(), 22, "only what the way in left");
            // Only what helps to understand it: the rest is left off the path.
            let rest: Vec<String> = (3..=12).map(|n| format!("src/f{n:02}.rs")).collect();
            json!({ "parts": [{ "name": "The rest", "why": "Everything after it.", "paths": rest }] })
        }
        _ => agent(job),
    })
    .await;
    let first_read = given.iter().position(|j| j.kind == "read").unwrap();
    assert!(
        given.iter().position(|j| j.kind == "path").unwrap() < first_read,
        "the rest of the path is asked for first"
    );
    let read: Vec<&str> = given
        .iter()
        .filter(|j| j.kind == "read")
        .map(|j| j.about.trim_start_matches("Reading "))
        .collect();
    assert_eq!(read, ["f01.rs", "f02.rs"], "only the way in");
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let way = kelve_server::way::shown(&app, &open).await.unwrap();
    assert_eq!(way.parts[0].steps.len(), 10);
    assert!(!way.parts[0].queued);
    assert_eq!(
        left_off(&app, &open).await,
        12,
        "the files no part names, left off the path"
    );
    // Opened again as it is, the path stands.
    assert_eq!(open_folder(&app, &root).await, id);
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    kelve_server::reader::read_ahead(&app, &open).await.unwrap();
    let again = work(&app, agent).await;
    assert!(
        again.iter().all(|j| j.kind != "path"),
        "nothing new: the rest isn't asked again"
    );
    // Files added since: opened again, the rest of the path is found again,
    // with the new one to place; the way in stands.
    std::fs::write(root.join("src/f25.rs"), "pub fn f25() {}\n").unwrap();
    assert_eq!(open_folder(&app, &root).await, id);
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    kelve_server::reader::read_ahead(&app, &open).await.unwrap();
    let found = work(&app, |job| match job.kind.as_str() {
        "path" => {
            let files = job.material["files"].as_array().unwrap();
            assert!(files.iter().any(|f| f == "src/f25.rs"), "the new file is placed");
            json!({ "parts": [{ "name": "The rest", "why": "Everything after it.", "paths": ["src/f25.rs"] }] })
        }
        _ => agent(job),
    })
    .await;
    assert!(
        found.iter().any(|j| j.kind == "path"),
        "the rest of the path is found again"
    );
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let way = kelve_server::way::shown(&app, &open).await.unwrap();
    assert_eq!(way.order.len(), 2, "the way in stands");
    assert_eq!(way.parts[0].steps.len(), 1);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn reading_asked_for_before_a_way_in_is_let_go() {
    let mut files: Vec<(String, String)> = Vec::new();
    for n in 1..=24 {
        files.push((
            format!("src/f{n:02}.rs"),
            format!("pub fn f{n:02}() {{}}\n"),
        ));
    }
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let root = folder(&named);
    let app = kelve(None).await;
    let id = open_folder(&app, &root).await;
    // Every file asked for, as kelve did before it read only the way in.
    for (path, _) in &files {
        kelve_server::reader::read_job(
            &app,
            &json!({ "opened": id, "path": path, "priority": 2 }),
            false,
        )
        .await
        .unwrap();
    }
    // Then a way in.
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let step = |p: &str| kelve_server::way::StepIn {
        path: p.into(),
        why: "Where it starts.".into(),
    };
    kelve_server::way::given(&app, &open, &[step("src/f01.rs"), step("src/f02.rs")], &[])
        .await
        .unwrap();
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    kelve_server::reader::read_ahead(&app, &open).await.unwrap();
    let given = work(&app, agent).await;
    // Only the way in is read: what was asked for before is let go.
    let read: Vec<&str> = given
        .iter()
        .filter(|j| j.kind == "read")
        .map(|j| j.about.trim_start_matches("Reading "))
        .collect();
    assert_eq!(read, ["f01.rs", "f02.rs"]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_file_on_the_way_in_says_how_far_the_agent_is_with_it_until_it_reads_plainly() {
    use kelve_core::views::WorkStage;
    let root = folder(&[
        ("src/ledger.rs", LEDGER),
        ("README.md", "# ledger\n\nMoney in, money out.\n"),
    ]);
    let app = kelve(None).await;
    let id = open_folder(&app, &root).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let step = |p: &str| kelve_server::way::StepIn {
        path: p.into(),
        why: "Where it starts.".into(),
    };
    kelve_server::way::given(&app, &open, &[step("src/ledger.rs")], &[])
        .await
        .unwrap();
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let at = |app: Shared| async move {
        let open = kelve_server::reader::opened(&app, id).await.unwrap();
        kelve_server::way::shown(&app, &open)
            .await
            .unwrap()
            .reading
            .into_iter()
            .find(|s| s.path == "src/ledger.rs")
            .map(|s| s.stage)
    };
    kelve_server::reader::read_ahead(&app, &open).await.unwrap();
    // Not started: nothing to say.
    common::jobs(&app).await;
    assert_eq!(at(app.clone()).await, None);
    // Taken: being read.
    let mut job = loop {
        let job = agent::next(&app, &caller()).await.unwrap().expect("a job");
        if job.kind == "read" && job.about.contains("ledger.rs") {
            break job;
        }
        agent::hand_in(&app, &caller(), job.job, agent(&job))
            .await
            .unwrap();
        common::jobs(&app).await;
    };
    assert_eq!(at(app.clone()).await, Some(WorkStage::Reading));
    // Handed in, it stays at work, a stage on, until it reads plainly.
    let mut seen = Vec::new();
    loop {
        agent::hand_in(&app, &caller(), job.job, agent(&job))
            .await
            .unwrap();
        common::jobs(&app).await;
        if let Some(stage) = at(app.clone()).await {
            seen.push(stage);
        }
        match agent::next(&app, &caller()).await.unwrap() {
            Some(next) => job = next,
            None => break,
        }
    }
    assert!(!seen.is_empty(), "at work after it's read");
    assert!(
        seen.windows(2).all(|w| w[0] <= w[1]),
        "only ever on: {seen:?}"
    );
    assert_eq!(at(app.clone()).await, None, "read plainly: done");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn with_the_way_in_taken_off_the_list_only_what_the_person_opens_is_read() {
    let mut files: Vec<(String, String)> = Vec::new();
    for n in 1..=24 {
        files.push((
            format!("src/f{n:02}.rs"),
            format!("pub fn f{n:02}() {{}}\n"),
        ));
    }
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let root = folder(&named);
    let app = kelve(None).await;
    let id = open_folder(&app, &root).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    kelve_server::way::ask_way(&app, id).await.unwrap();
    // The person takes the way in off their agent's list.
    {
        let mut conn = app.db.acquire().await.unwrap();
        let way = kelve_store::asks::open(&mut conn)
            .await
            .unwrap()
            .into_iter()
            .find(|a| a.kind == "way")
            .expect("the way in asked for");
        assert!(kelve_store::asks::drop(&mut conn, way.id).await.unwrap());
    }
    // Nothing is read in the background: never everything.
    kelve_server::reader::read_ahead(&app, &open).await.unwrap();
    let given = work(&app, agent).await;
    assert!(
        given.iter().all(|j| j.kind != "read"),
        "{:?}",
        given.iter().map(|j| &j.about).collect::<Vec<_>>()
    );
    // A file the person opens is read.
    kelve_server::reader::read_job(
        &app,
        &json!({ "opened": id, "path": "src/f07.rs", "priority": 0 }),
        false,
    )
    .await
    .unwrap();
    let given = work(&app, agent).await;
    let read: Vec<&str> = given
        .iter()
        .filter(|j| j.kind == "read")
        .map(|j| j.about.trim_start_matches("Reading "))
        .collect();
    assert_eq!(read, ["f07.rs"]);
    std::fs::remove_dir_all(&root).unwrap();
}

/// A way-in answer for files `src/fNN.rs`, with a line for `src`.
fn way_with_lines(job: &Job) -> Value {
    let files: Vec<&str> = job.material["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    json!({
        "order": [{ "path": files[0], "why": "Where it starts." }],
        "parts": [{ "name": "The rest", "why": "What follows.", "paths": files[1..files.len().min(16)] }],
        "folders": [{ "path": "src", "line": "Every function there is." }],
    })
}

#[tokio::test]
async fn a_big_folder_opened_in_the_reader_has_its_way_in_and_folders_lines_first() {
    let files: Vec<(String, String)> = (1..=24)
        .map(|n| {
            (
                format!("src/f{n:02}.rs"),
                format!("pub fn f{n:02}() {{}}\n"),
            )
        })
        .collect();
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let root = folder(&named);
    let app = kelve(None).await;
    let id = open_folder(&app, &root).await;
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    // Opened in the reader: its reading starts, and the way in comes first.
    kelve_server::reader::read_ahead(&app, &open).await.unwrap();
    let given = work(&app, |job| match job.kind.as_str() {
        "way" => way_with_lines(job),
        "folders" => panic!("each folder's line comes with the way in"),
        _ => agent(job),
    })
    .await;
    assert_eq!(given[0].kind, "way");
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let way = kelve_server::way::view(&app, &open).await.unwrap();
    assert_eq!(way.folders[0].line, "Every function there is.");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_big_change_has_its_way_in_and_folders_lines_first() {
    let root = folder(&[("README.md", "# ledger\n")]);
    let git = |args: &[&str]| {
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
            .current_dir(&root)
            .output()
            .unwrap();
        assert!(
            ran.status.success(),
            "{}",
            String::from_utf8_lossy(&ran.stderr)
        );
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "Start"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    for n in 1..=24 {
        std::fs::write(
            root.join(format!("src/f{n:02}.rs")),
            format!("pub fn f{n:02}() {{}}\n"),
        )
        .unwrap();
    }
    git(&["add", "."]);
    let app = kelve(None).await;
    let id = {
        let mut conn = app.db.acquire().await.unwrap();
        kelve_store::reader::open(
            &mut conn,
            "change",
            &root.display().to_string(),
            "",
            "uncommitted",
            "A lot changed",
        )
        .await
        .unwrap()
    };
    let open = kelve_server::reader::opened(&app, id).await.unwrap();
    let change = kelve_server::reader::change(&open).unwrap();
    kelve_server::reader::mark_change(&app, &open, &change)
        .await
        .unwrap();
    let given = work(&app, |job| match job.kind.as_str() {
        "way" => way_with_lines(job),
        "folders" => panic!("each folder's line comes with the way in"),
        _ => agent(job),
    })
    .await;
    assert_eq!(given[0].kind, "way", "the way in comes first");
    std::fs::remove_dir_all(&root).unwrap();
}
