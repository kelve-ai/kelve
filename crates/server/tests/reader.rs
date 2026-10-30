//! The reader's routes, end to end through kelve's router, on a store and
//! folders of their own: opening from the command line, a folder and its
//! files, a change, notes, and search. No model here: what's marked is
//! `marking.rs`'s.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use kelve_server::config::test_config;
use kelve_server::state::{App, Shared};
use serde_json::{Value, json};
use tower::ServiceExt;

struct Kelve {
    app: Shared,
    router: Router,
    cookie: Option<String>,
    links: Arc<Mutex<Vec<String>>>,
    /// The overviews the stand-in has written, by their job's about.
    overviews: Mutex<std::collections::HashSet<String>>,
}

impl Kelve {
    async fn new() -> Kelve {
        Kelve::with(test_config()).await
    }

    async fn with(config: kelve_server::config::Config) -> Kelve {
        let pool = kelve_store::open(&config.store).await.expect("a store");
        kelve_store::migrate(&pool).await.expect("migrated");
        let app = App::new(config, pool).await.expect("kelve");
        let links: Arc<Mutex<Vec<String>>> = Arc::default();
        let keep = links.clone();
        app.set_hand_off(Arc::new(move |link: &str| {
            keep.lock().expect("links").push(link.to_string());
            true
        }));
        let router = kelve_server::router(app.clone());
        Kelve {
            app,
            router,
            cookie: None,
            links,
            overviews: Mutex::default(),
        }
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        as_agent: bool,
    ) -> (StatusCode, Value, Option<String>) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost:4600")
            .header("sec-fetch-site", "same-origin");
        if as_agent {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", self.app.secret));
        } else if let Some(cookie) = &self.cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let request = match body {
            Some(b) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("a request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("a response");
        let status = response.status();
        let location = response
            .headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(str::to_string);
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value, cookie.or(location))
    }

    async fn get(&self, path: &str) -> Value {
        let (status, value, _) = self.call("GET", path, None, false).await;
        assert!(status.is_success(), "GET {path}: {status} {value}");
        value
    }

    /// `kelve open <path>`: the command line learns only whether a browser
    /// opened. The link kelve opened is the person's: it's returned here,
    /// with the id of what's open, for the test to follow.
    async fn open(&mut self, body: Value) -> (i64, String) {
        let (status, said, _) = self.call("POST", "/api/open", Some(body), true).await;
        assert_eq!(status, StatusCode::OK, "{said}");
        assert_eq!(said, json!({ "browser": true }), "nothing else comes back");
        let link = self
            .links
            .lock()
            .expect("links")
            .last()
            .cloned()
            .expect("a link");
        let path = link.trim_start_matches("http://localhost:4600").to_string();
        let id = path
            .split("opened=")
            .nth(1)
            .and_then(|s| s.parse().ok())
            .expect("the link names what's open");
        (id, path)
    }

    /// Follows a sign-in link: where it lands, and the session it sets.
    async fn land(&self, path: &str) -> (StatusCode, Option<String>, Option<String>) {
        let request = Request::builder()
            .uri(path)
            .header(header::HOST, "localhost:4600")
            .body(Body::empty())
            .expect("a request");
        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("a response");
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|v: &header::HeaderValue| v.to_str().ok())
                .map(str::to_string)
        };
        let location = header(header::LOCATION);
        let cookie =
            header(header::SET_COOKIE).and_then(|c| c.split(';').next().map(str::to_string));
        (response.status(), location, cookie)
    }
}

fn folder(files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kelve-reader-{}", uuid::Uuid::now_v7().simple()));
    for (rel, text) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();
    std::fs::canonicalize(root).unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .expect("git runs");
    assert!(
        ok.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
}

/// Opens a folder from the command line, and signs the browser in on it.
async fn signed_in(k: &mut Kelve, root: &Path) -> i64 {
    let (id, link) = k.open(json!({ "path": root.display().to_string() })).await;
    let (_, _, cookie) = k.land(&link).await;
    k.cookie = Some(cookie.expect("a session cookie"));
    id
}

#[tokio::test]
async fn opening_lands_the_browser_on_it_and_tells_the_command_line_nothing_more() {
    let mut k = Kelve::new().await;
    let root = folder(&[("src/ledger.rs", "pub fn reserve() {}\n")]);
    let (id, link) = k.open(json!({ "path": root.display().to_string() })).await;
    let (status, location, cookie) = k.land(&link).await;
    assert!(status.is_redirection());
    assert_eq!(
        location.as_deref(),
        Some(format!("/#/o/{id}").as_str()),
        "it lands on what was opened"
    );
    assert!(cookie.is_some(), "signed in");
    let (status, _, _) = k.land(&link).await;
    assert!(!status.is_redirection(), "the link works once");
    // The command line can't read what's open.
    let (status, _, _) = k
        .call("GET", &format!("/api/opened/{id}/folder"), None, true)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_folder_lists_its_files_and_keeps_secrets_unread() {
    let mut k = Kelve::new().await;
    let root = folder(&[
        ("src/ledger.rs", "pub fn reserve() {}\n"),
        (
            ".env",
            "KEY=sk-ant-api03-AbCdEf0123456789_AbCdEf0123456789-xyz\n",
        ),
        ("README.md", "# Ledger\n"),
    ]);
    let id = signed_in(&mut k, &root).await;
    let f = k.get(&format!("/api/opened/{id}/folder")).await;
    let paths: Vec<&str> = f["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, [".env", "README.md", "src/ledger.rs"]);
    assert!(f["files"][0]["skip"].as_str().unwrap().contains("secrets"));
    assert_eq!(f["opened"]["kind"], "folder");

    let file = k
        .get(&format!("/api/opened/{id}/file?path=src/ledger.rs"))
        .await;
    assert_eq!(file["text"], "pub fn reserve() {}\n");
    assert_eq!(
        file["marking"], "marking",
        "with the agent, until it reads it"
    );
    let secret = k.get(&format!("/api/opened/{id}/file?path=.env")).await;
    assert_eq!(secret["text"], Value::Null);
    let (status, _, _) = k
        .call(
            "GET",
            &format!("/api/opened/{id}/file?path=../../etc/passwd"),
            None,
            false,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let lately = k.get("/api/opened").await;
    assert_eq!(lately[0]["id"], id);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_change_lists_its_files_and_their_lines() {
    let mut k = Kelve::new().await;
    let root = folder(&[("src/ledger.rs", "pub fn reserve() {}\n")]);
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "first"]);
    std::fs::write(
        root.join("src/ledger.rs"),
        "pub fn reserve() {}\npub fn settle() {}\n",
    )
    .unwrap();
    // kelve's own folder, beside the work, isn't part of the change.
    std::fs::create_dir_all(root.join(".kelve/notes")).unwrap();
    std::fs::write(root.join(".kelve/way.json"), "{}\n").unwrap();
    std::fs::write(root.join(".kelve/notes/retries.md"), "# Retries\n").unwrap();
    let _ = signed_in(&mut k, &root).await;
    let (status, said, _) = k
        .call(
            "POST",
            "/api/open",
            Some(json!({ "path": root.display().to_string(), "change": "uncommitted" })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    let id = said["opened"]["id"].as_i64().unwrap();
    assert_eq!(said["opened"]["title"], "Uncommitted changes");
    let c = k.get(&format!("/api/opened/{id}/change")).await;
    assert_eq!(c["files"].as_array().unwrap().len(), 1, "{c}");
    assert_eq!(c["files"][0]["path"], "src/ledger.rs");
    assert_eq!(c["files"][0]["added"], 1);
    let d = k
        .get(&format!("/api/opened/{id}/diff?path=src/ledger.rs"))
        .await;
    // The file whole: what it kept, and what the change added.
    let lines: Vec<(&str, &str)> = d["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| (l["kind"].as_str().unwrap(), l["text"].as_str().unwrap()))
        .collect();
    assert_eq!(
        lines,
        [
            ("context", "pub fn reserve() {}"),
            ("added", "pub fn settle() {}")
        ]
    );
    // Every file beside it, as a list: nothing is read for it.
    let every = k.get(&format!("/api/opened/{id}/folder?list=true")).await;
    assert_eq!(every["files"][0]["path"], "src/ledger.rs");
    assert_eq!(every["builtOn"], json!([]));
    let (status, _, _) = k
        .call(
            "POST",
            "/api/open",
            Some(json!({ "path": "/", "change": "whatever" })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_worktree_reads_as_one_change_since_its_base_committed_or_not() {
    let mut k = Kelve::new().await;
    let root = folder(&[("src/ledger.rs", "pub fn reserve() {}\n")]);
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "first"]);
    git(&root, &["checkout", "-q", "-b", "agent/retries"]);
    std::fs::write(root.join("src/retry.rs"), "pub fn wait() {}\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "retry: wait between tries"]);
    std::fs::write(
        root.join("src/ledger.rs"),
        "pub fn reserve() {}\npub fn settle() {}\n",
    )
    .unwrap();
    git(&root, &["commit", "-q", "-am", "ledger: settle after"]);
    std::fs::write(root.join("src/timeout.rs"), "pub fn timeout() {}\n").unwrap();
    let _ = signed_in(&mut k, &root).await;
    let (status, said, _) = k
        .call(
            "POST",
            "/api/open",
            Some(json!({ "path": root.display().to_string(), "change": "since main" })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    let id = said["opened"]["id"].as_i64().unwrap();

    // Everything since it left its base, committed or not, as one change.
    let all = k.get(&format!("/api/opened/{id}/change")).await;
    assert_eq!(all["files"].as_array().unwrap().len(), 3);
    // Read once, it's used again only while it holds: an edit, a file
    // added and a commit each show at once.
    let diff = format!("/api/opened/{id}/diff?path=src/timeout.rs");
    let text = |d: &Value| d["lines"][1]["text"].as_str().map(str::to_string);
    assert_eq!(text(&k.get(&diff).await), None);
    std::fs::write(
        root.join("src/timeout.rs"),
        "pub fn timeout() {}\npub fn each() {}\n",
    )
    .unwrap();
    assert_eq!(
        text(&k.get(&diff).await).as_deref(),
        Some("pub fn each() {}")
    );
    std::fs::write(root.join("src/jitter.rs"), "pub fn jitter() {}\n").unwrap();
    let all = k.get(&format!("/api/opened/{id}/change")).await;
    assert_eq!(all["files"].as_array().unwrap().len(), 4);
    git(&root, &["add", "."]);
    git(
        &root,
        &["commit", "-q", "-m", "retry: a timeout on each try"],
    );
    git(&root, &["rm", "-q", "src/jitter.rs"]);
    git(&root, &["commit", "-q", "-m", "retry: no jitter yet"]);
    let all = k.get(&format!("/api/opened/{id}/change")).await;
    assert_eq!(all["files"].as_array().unwrap().len(), 3);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn notes_are_markdown_files_in_their_places() {
    let mut k = Kelve::new().await;
    let root = folder(&[("src/ledger.rs", "pub fn reserve() {}\n")]);
    let id = signed_in(&mut k, &root).await;
    let (status, work, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({ "place": "work", "text": "# Spending ledger\n\nKeep the reservation immediate." })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{work}");
    assert_eq!(work["id"], "work/spending-ledger.md");
    assert!(
        root.join(".kelve/notes/spending-ledger.md").exists(),
        "beside the code"
    );
    // A note for the agent, on lines: kept with yours, out of the project,
    // waiting for it.
    let (status, agents, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({
                "place": "agent",
                "text": "# Make it settle too\n",
                "on": { "path": "src/ledger.rs", "start": 1, "end": 1 },
            })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{agents}");
    assert_eq!(agents["id"], "agent/make-it-settle-too.md");
    assert_eq!(agents["state"], "waiting");
    assert!(!root.join(".kelve/notes/make-it-settle-too.md").exists());
    // The agent reads it, fenced, with its lines as they are now.
    let at = urlencode(&root.display().to_string());
    let (status, waiting, _) = k
        .call(
            "GET",
            &format!("/api/agent/notes?path={at}/src"),
            None,
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{waiting}");
    assert_eq!(waiting[0]["id"], "agent/make-it-settle-too.md");
    assert!(waiting[0]["note"].as_str().unwrap().starts_with("<note"));
    assert!(
        waiting[0]["textNow"]
            .as_str()
            .unwrap()
            .contains("pub fn reserve()")
    );
    let (status, _, _) = k
        .call("GET", &format!("/api/agent/notes?path={at}"), None, false)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "only the agent");
    // It says it's done, for the words it read, and what it did; it waits
    // no more. For other words, or none, it isn't done.
    let version = waiting[0]["version"].as_str().unwrap().to_string();
    let done_with = |version: &str| json!({ "path": root.display().to_string(), "note": "agent/make-it-settle-too.md", "version": version, "did": "Added `settle`, which takes what was held." });
    let (status, said, _) = k
        .call("POST", "/api/agent/note-done", Some(done_with("")), true)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{said}");
    let (status, said, _) = k
        .call(
            "POST",
            "/api/agent/note-done",
            Some(done_with("0123456789abcdef")),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{said}");
    assert!(
        said.to_string().contains("read it again with notes"),
        "{said}"
    );
    let (status, said, _) = k
        .call(
            "POST",
            "/api/agent/note-done",
            Some(done_with(&version)),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    let (_, waiting, _) = k
        .call("GET", &format!("/api/agent/notes?path={at}"), None, true)
        .await;
    assert_eq!(waiting, json!([]));
    let done = k
        .get(&format!(
            "/api/opened/{id}/note?note=agent/make-it-settle-too.md"
        ))
        .await;
    assert_eq!(done["state"], "done");
    assert_eq!(done["did"], "Added `settle`, which takes what was held.");
    // Kept again as it was, it stays done; written in afresh, it waits for
    // the agent again, with what it did before and what's new; cleared for
    // fresh notes, what it did goes.
    let note = "agent/make-it-settle-too.md";
    let save = |text: String| json!({ "id": note, "place": "agent", "text": text });
    let (_, same, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(save(done["text"].as_str().unwrap().to_string())),
            false,
        )
        .await;
    assert_eq!(same["state"], "done", "{same}");
    let (_, again, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(save("And refunds, the same way.".into())),
            false,
        )
        .await;
    assert_eq!(again["state"], "waiting", "{again}");
    assert_eq!(
        again["did"], "Added `settle`, which takes what was held.",
        "{again}"
    );
    let (_, waiting, _) = k
        .call("GET", &format!("/api/agent/notes?path={at}"), None, true)
        .await;
    assert_eq!(waiting[0]["id"], note, "the agent sees it again");
    assert_ne!(waiting[0]["version"], version.as_str());
    assert!(
        waiting[0]["doneBefore"]["text"]
            .as_str()
            .unwrap()
            .contains("Make it settle too"),
        "{waiting}"
    );
    assert_eq!(
        waiting[0]["doneBefore"]["did"],
        "Added `settle`, which takes what was held."
    );
    assert_eq!(waiting[0]["added"], json!(["And refunds, the same way."]));
    let (_, cleared, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(save(String::new())),
            false,
        )
        .await;
    assert_eq!(cleared["id"], note, "cleared, not deleted: {cleared}");
    assert!(cleared["state"].is_null(), "empty, it waits for nothing");
    assert_eq!(cleared["did"], "", "{cleared}");
    let (_, waiting, _) = k
        .call("GET", &format!("/api/agent/notes?path={at}"), None, true)
        .await;
    assert_eq!(waiting, json!([]));
    // Written in again, as the rest of this test has it.
    k.call(
        "POST",
        &format!("/api/opened/{id}/notes"),
        Some(save(done["text"].as_str().unwrap().to_string())),
        false,
    )
    .await;
    let (_, mine, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({ "place": "you", "text": "Things to read" })),
            false,
        )
        .await;
    assert!(
        !root
            .join(".kelve/notes")
            .join(mine["id"].as_str().unwrap().trim_start_matches("you/"))
            .exists(),
        "yours stay out of the project"
    );

    let list = k.get(&format!("/api/opened/{id}/notes")).await;
    let titles: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        ["Make it settle too", "Spending ledger", "Things to read"]
    );
    let read = k
        .get(&format!(
            "/api/opened/{id}/note?note=work/spending-ledger.md"
        ))
        .await;
    assert!(read["text"].as_str().unwrap().contains("immediate"));
    let edited = json!({ "id": "work/spending-ledger.md", "place": "work", "text": "# Spending ledger\n\nEdited." });
    let (status, _, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(edited),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    for bad in [
        "work/../../x.md",
        "work/a%20b.md",
        "agent/other.md",
        "you/../../x.md",
        "elsewhere/x.md",
    ] {
        let (status, _, _) = k
            .call(
                "GET",
                &format!("/api/opened/{id}/note?note={bad}"),
                None,
                false,
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{bad}");
    }

    let found = k.get(&format!("/api/search?q=ledger&opened={id}")).await;
    assert_eq!(found["files"], json!(["src/ledger.rs"]));
    assert_eq!(found["notes"][0]["title"], "Spending ledger");
    // What files say, too, line by line, as the reader shows them.
    let found = k.get(&format!("/api/search?q=pub%20fn&opened={id}")).await;
    let lines = found["lines"].as_array().unwrap();
    assert!(!lines.is_empty(), "{found}");
    assert!(lines.iter().all(
        |l| l["text"].as_str().unwrap().contains("pub fn") && l["line"].as_u64().unwrap() >= 1
    ));

    // Your own notes go to a folder you choose, a folder for the project,
    // named as it is; the notes kept already go with them.
    let chosen = folder(&[]);
    let (status, _, _) = k
        .call(
            "POST",
            "/api/settings",
            Some(json!({ "notesDir": "/nowhere/at/all" })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a folder that's there");
    let (status, set, _) = k
        .call(
            "POST",
            "/api/settings",
            Some(json!({ "notesDir": chosen.display().to_string() })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{set}");
    assert_eq!(set["notesChosen"], true);
    let name = root.file_name().unwrap().to_string_lossy().into_owned();
    let file = mine["id"].as_str().unwrap().trim_start_matches("you/");
    assert!(
        chosen.join(&name).join(file).exists(),
        "moved, by the project's name"
    );
    let list = k.get(&format!("/api/opened/{id}/notes")).await;
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == mine["id"]),
        "still found"
    );
    // Another project of the same name gets a folder of its own.
    let twin = std::env::temp_dir()
        .join(format!("kelve-twin-{}", uuid::Uuid::now_v7().simple()))
        .join(&name);
    std::fs::create_dir_all(&twin).unwrap();
    std::fs::write(twin.join("a.md"), "# A\n").unwrap();
    let other = signed_in(&mut k, &twin).await;
    let (_, theirs, _) = k
        .call(
            "POST",
            &format!("/api/opened/{other}/notes"),
            Some(json!({ "place": "you", "text": "# Theirs\n" })),
            false,
        )
        .await;
    assert_eq!(theirs["id"], "you/theirs.md");
    let above = twin
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(
        chosen
            .join(format!("{name} ({above})"))
            .join("theirs.md")
            .exists()
    );
    // Yours become the agent's when you say so, and wait for it.
    let moved = json!({ "id": theirs["id"], "place": "agent", "text": "# Theirs\n" });
    let (_, moved, _) = k
        .call(
            "POST",
            &format!("/api/opened/{other}/notes"),
            Some(moved),
            false,
        )
        .await;
    assert_eq!(moved["id"], "agent/theirs.md");
    assert_eq!(moved["state"], "waiting");
    // Deleted, it's gone.
    let (status, _, _) = k
        .call(
            "DELETE",
            &format!("/api/opened/{other}/notes?note=agent/theirs.md"),
            None,
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let list = k.get(&format!("/api/opened/{other}/notes")).await;
    assert_eq!(list, json!([]));
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::remove_dir_all(&chosen).unwrap();
    std::fs::remove_dir_all(twin.parent().unwrap()).unwrap();
}

/// A path, as it goes in an address.
/// A note's file, left alone long enough to be on the agent's list.
fn settled(note: &Value) {
    let path = kelve_server::reader::unshown(note["file"].as_str().unwrap());
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(60))
        .unwrap();
}

/// The agent's next job, as an agent working in `path` takes it.
async fn next_in(k: &Kelve, path: &Path) -> Option<kelve_server::agent::Job> {
    let caller = kelve_server::agent::Caller {
        agent: "codex-mcp-client".into(),
        model: None,
        plan: None,
        path: Some(path.display().to_string()),
        only: None,
    };
    kelve_server::agent::next(&k.app, &caller).await.unwrap()
}

/// Puts aside every job that isn't a note's, so the agent is given notes.
async fn only_notes(k: &Kelve) {
    let mut conn = k.app.db.acquire().await.unwrap();
    for a in kelve_store::asks::open(&mut conn).await.unwrap() {
        if a.kind != "note" {
            kelve_store::asks::drop(&mut conn, a.id).await.unwrap();
        }
    }
}

#[tokio::test]
async fn a_note_is_done_for_the_words_the_agent_was_given_and_waits_again_with_whats_new() {
    let mut k = Kelve::new().await;
    let root = folder(&[(
        "src/retry.rs",
        "const MAX_WAIT: u64 = 30;\nconst FLOOR: u64 = 1;\n",
    )]);
    let id = signed_in(&mut k, &root).await;
    let keep = |text: &str, note: Option<&str>| {
        let mut body = json!({
            "place": "agent",
            "text": text,
            "on": { "path": "src/retry.rs", "start": 1, "end": 2 },
        });
        if let Some(n) = note {
            body["id"] = json!(n);
        }
        body
    };
    let (_, cap, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(keep("# Make the cap a setting\n", None)),
            false,
        )
        .await;
    let (_, floor, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({
                "place": "agent",
                "text": "# And the floor\n",
                "on": { "path": "src/retry.rs", "start": 2, "end": 2 },
            })),
            false,
        )
        .await;
    let note = cap["id"].as_str().unwrap().to_string();
    settled(&cap);
    settled(&floor);
    only_notes(&k).await;

    // Notes on the same lines name each other.
    let at = urlencode(&root.display().to_string());
    let (_, waiting, _) = k
        .call("GET", &format!("/api/agent/notes?path={at}"), None, true)
        .await;
    let mine = |w: &Value, id: &str| {
        w.as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .cloned()
            .unwrap()
    };
    assert_eq!(
        mine(&waiting, &note)["onTheSameLines"],
        json!([floor["id"]])
    );
    assert!(mine(&waiting, &note)["linesChanged"].is_null());

    // An agent in another project is given none of these.
    let elsewhere = folder(&[("b.md", "# B\n")]);
    let _ = k
        .open(json!({ "path": elsewhere.display().to_string() }))
        .await;
    only_notes(&k).await;
    assert!(
        next_in(&k, &elsewhere).await.is_none(),
        "not this project's"
    );

    // Taken, then changed while the agent does it: its job stays with the
    // agent, and its answer is kept for the words it was given.
    let mut job = next_in(&k, &root).await.unwrap();
    while !job.about.contains("Make the cap") {
        job = next_in(&k, &root).await.unwrap();
    }
    let (_, changed, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(keep(
                "# Make the cap a setting\n\nAnd log when it's reached.\n",
                Some(&note),
            )),
            false,
        )
        .await;
    assert_eq!(changed["state"], "waiting");
    // While it's written, the list doesn't pull the job from the agent.
    kelve_server::list::sync_notes(&k.app).await.unwrap();
    let handed = kelve_server::agent::hand_in(
        &k.app,
        &kelve_server::agent::Caller {
            agent: "codex-mcp-client".into(),
            ..Default::default()
        },
        job.job,
        json!({ "did": "Moved the cap to `Config`." }),
    )
    .await
    .unwrap();
    assert_eq!(handed, kelve_server::agent::HandedIn::Yes);
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let read = k.get(&format!("/api/opened/{id}/note?note={note}")).await;
    assert_eq!(read["state"], "waiting", "{read}");
    assert_eq!(read["did"], "Moved the cap to `Config`.");
    // The new words are asked, with what was done before and what's new.
    settled(&read);
    let mut job = next_in(&k, &root).await.unwrap();
    while !job.about.contains("Make the cap") {
        job = next_in(&k, &root).await.unwrap();
    }
    assert_eq!(
        job.material["doneBefore"]["did"],
        "Moved the cap to `Config`."
    );
    assert_eq!(job.material["added"], json!(["And log when it's reached."]));
    assert!(job.instructions.contains("Do only what's new"));
    kelve_server::agent::hand_in(
        &k.app,
        &kelve_server::agent::Caller {
            agent: "codex-mcp-client".into(),
            ..Default::default()
        },
        job.job,
        json!({ "did": "Logged it, once." }),
    )
    .await
    .unwrap();
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let read = k.get(&format!("/api/opened/{id}/note?note={note}")).await;
    assert_eq!(read["state"], "done", "{read}");

    // Put back as it was first done, outside kelve: it waits, and its job
    // is asked again, though it was answered before.
    let path = kelve_server::reader::unshown(read["file"].as_str().unwrap());
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        text.replace("\n\nAnd log when it's reached.\n", "\n"),
    )
    .unwrap();
    settled(&read);
    let mut job = next_in(&k, &root).await;
    while job
        .as_ref()
        .is_some_and(|j| !j.about.contains("Make the cap"))
    {
        job = next_in(&k, &root).await;
    }
    let job = job.expect("asked again");
    assert_eq!(job.material["doneBefore"]["did"], "Logged it, once.");

    // The lines it's on change: it says so.
    std::fs::write(root.join("src/retry.rs"), "const MAX_WAIT: u64 = 60;\n").unwrap();
    let (_, waiting, _) = k
        .call("GET", &format!("/api/agent/notes?path={at}"), None, true)
        .await;
    assert_eq!(
        mine(&waiting, &note)["linesChanged"],
        "These lines have changed since the note was left."
    );
    assert!(mine(&waiting, &note)["textNow"].is_null());
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&elsewhere).ok();
}

#[tokio::test]
async fn the_hook_counts_only_notes_its_agent_hasnt_been_told_of() {
    let mut k = Kelve::new().await;
    let root = folder(&[("a.md", "# A\n")]);
    let id = signed_in(&mut k, &root).await;
    let save = |text: &str, note: Option<&str>| {
        let mut body = json!({ "place": "agent", "text": text });
        if let Some(n) = note {
            body["id"] = json!(n);
        }
        body
    };
    let (_, one, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(save("# Tidy the intro\n", None)),
            false,
        )
        .await;
    k.call(
        "POST",
        &format!("/api/opened/{id}/notes"),
        Some(save("# Add a summary\n", None)),
        false,
    )
    .await;
    let told =
        |agent: &str| json!({ "path": root.join("sub").display().to_string(), "agent": agent });
    let new = |v: Value| v["new"].as_u64().unwrap();
    let (status, said, _) = k
        .call("POST", "/api/agent/notes/new", Some(told("claude")), true)
        .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    assert_eq!(new(said), 2);
    let (_, said, _) = k
        .call("POST", "/api/agent/notes/new", Some(told("claude")), true)
        .await;
    assert_eq!(new(said), 0, "told already");
    let (_, said, _) = k
        .call("POST", "/api/agent/notes/new", Some(told("codex")), true)
        .await;
    assert_eq!(new(said), 2, "another agent is told for itself");
    k.call(
        "POST",
        &format!("/api/opened/{id}/notes"),
        Some(save("# Tidy the intro\n\nShorter.\n", one["id"].as_str())),
        false,
    )
    .await;
    let (_, said, _) = k
        .call("POST", "/api/agent/notes/new", Some(told("claude")), true)
        .await;
    assert_eq!(new(said), 1, "changed since");
    let (status, _, _) = k
        .call("POST", "/api/agent/notes/new", Some(told("claude")), false)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "only the agent");
    std::fs::remove_dir_all(&root).ok();
}

fn urlencode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

#[tokio::test]
async fn the_hook_goes_into_claude_codes_settings_only_when_asked_and_comes_out_alone() {
    let mut k = Kelve::new().await;
    let root = folder(&[("a.md", "# A\n")]);
    let _ = signed_in(&mut k, &root).await;
    let file = k.app.config.claude_dir.join("settings.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, r#"{ "model": "opus" }"#).unwrap();
    let before = k.get("/api/settings").await;
    assert_eq!(before["notesHook"], false);
    let (status, after, _) = k
        .call(
            "POST",
            "/api/settings",
            Some(json!({ "notesHook": true })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["notesHook"], true);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("kelve moment")
            && text.contains("PostToolUseFailure")
            && text.contains("opus")
    );
    assert_eq!(after["hooksElsewhere"], json!([]));
    // Switched to another agent: its own hook, and Claude Code's said.
    let (_, codex, _) = k
        .call(
            "POST",
            "/api/settings",
            Some(json!({ "agent": "codex" })),
            false,
        )
        .await;
    assert_eq!(codex["notesHook"], false, "{codex}");
    assert_eq!(codex["hooksElsewhere"], json!(["Claude Code"]));
    k.call(
        "POST",
        "/api/settings",
        Some(json!({ "agent": "claudeCode" })),
        false,
    )
    .await;
    let (_, after, _) = k
        .call(
            "POST",
            "/api/settings",
            Some(json!({ "notesHook": false })),
            false,
        )
        .await;
    assert_eq!(after["notesHook"], false);
    let left: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(left, json!({ "model": "opus" }));
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn an_idea_kelve_doesnt_know_is_a_plain_not_found() {
    let mut k = Kelve::new().await;
    let root = folder(&[("a.md", "# A\n")]);
    let _ = signed_in(&mut k, &root).await;
    let (status, said, _) = k.call("GET", "/api/ideas/new:nothing", None, false).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        said["error"]
            .as_str()
            .unwrap()
            .starts_with("kelve doesn't know")
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_note_on_lines_keeps_them_and_follows_them_as_the_file_changes() {
    let mut k = Kelve::new().await;
    let ledger = "pub fn reserve() {\n    take_lock();\n    spend();\n}\n";
    let root = folder(&[("src/ledger.rs", ledger)]);
    let id = signed_in(&mut k, &root).await;
    let (status, note, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({
                "place": "work",
                "text": "# The lock\n\nTaken first.",
                "on": { "path": "src/ledger.rs", "start": 2, "end": 3 },
                "ideas": ["new:advisory-locks"],
            })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{note}");
    assert_eq!(note["text"], "# The lock\n\nTaken first.", "the body alone");
    assert_eq!(note["ideas"], json!(["new:advisory-locks"]));
    assert_eq!(
        note["on"],
        json!({ "path": "src/ledger.rs", "start": 2, "end": 3, "found": true })
    );
    let file = std::fs::read_to_string(root.join(".kelve/notes/the-lock.md")).unwrap();
    assert!(file.starts_with("---\nideas: [\"new:advisory-locks\"]\npassage: {\"path\":\"src/ledger.rs\",\"lines\":\"2-3\",\"fingerprint\":"), "{file}");

    // Editing the body keeps what the front matter says.
    let (_, edited, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({ "id": "work/the-lock.md", "place": "work", "text": "# The lock\n\nTaken first, always." })),
            false,
        )
        .await;
    assert_eq!(edited["on"]["start"], 2);
    let file = std::fs::read_to_string(root.join(".kelve/notes/the-lock.md")).unwrap();
    assert!(file.contains("\"lines\":\"2-3\"") && file.ends_with("Taken first, always."));

    // Lines added above: the note follows its passage.
    std::fs::write(
        root.join("src/ledger.rs"),
        format!("use std::sync::Mutex;\n\n{ledger}"),
    )
    .unwrap();
    let notes = k.get(&format!("/api/opened/{id}/notes")).await;
    assert_eq!(
        notes[0]["on"],
        json!({ "path": "src/ledger.rs", "start": 4, "end": 5, "found": true })
    );
    // Its text changed: where it was, and not found.
    std::fs::write(root.join("src/ledger.rs"), ledger.replace("spend", "pay")).unwrap();
    let notes = k.get(&format!("/api/opened/{id}/notes")).await;
    assert_eq!(notes[0]["on"]["found"], false);

    // Lines that aren't there are refused.
    let (status, _, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({
                "place": "you",
                "text": "x",
                "on": { "path": "src/ledger.rs", "start": 40, "end": 41 },
            })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn kelve_counts_how_it_was_used_in_numbers_and_never_what() {
    let mut k = Kelve::new().await;
    let root = folder(&[("src/ledger.rs", "pub fn reserve() {}\n")]);
    let id = signed_in(&mut k, &root).await;
    let (status, _, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({ "place": "work", "text": "# Spending ledger\n\nKeep it immediate." })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    {
        let mut conn = k.app.db.acquire().await.unwrap();
        kelve_store::entries::keep_idea(
            &mut conn,
            &kelve_store::entries::Idea {
                key: "new:advisory-locks".into(),
                name: "Advisory locks".into(),
                definition: "A lock a program names, and the database keeps.".into(),
                kind: "concept".into(),
                became: None,
            },
        )
        .await
        .unwrap();
    }
    // Followed from a link, and the page asked for again: one link.
    k.get("/api/ideas/new:advisory-locks?followed=true").await;
    k.get("/api/ideas/new:advisory-locks?followed=true").await;
    k.get("/api/ideas/new:advisory-locks").await;

    let shown = k.get("/api/counts").await;
    let text = shown["thisWeek"].as_str().unwrap();
    let week: Value = serde_json::from_str(text).unwrap();
    assert_eq!(week["days"], 1);
    assert_eq!(week["counts"]["opened.folder"], 1);
    assert_eq!(week["counts"]["notes.work"], 1);
    assert_eq!(week["counts"]["links.followed"], 1);
    assert!(
        week["counts"].get("links.written").is_none(),
        "its entry wasn't written yet"
    );
    for private in ["ledger", "advisory", "Spending", root.to_str().unwrap()] {
        assert!(!text.contains(private), "{private} never goes in a count");
    }
    assert_eq!(shown["sent"], json!([]));
    std::fs::remove_dir_all(&root).unwrap();
}

/// A stand-in graph holding how prompt caching comes down to sets, by
/// relations, and a kelve reading from it.
async fn related() -> (kelve_graph_api::stand_in::StandIn, Vec<String>) {
    use kelve_core::ideas::IdeaKind::Concept;
    use kelve_core::relations::RelationKind::{Alternative, KindOf, RestsOn};
    use kelve_graph_api::{Change, Changeset};
    let g = kelve_graph_api::stand_in::StandIn::start()
        .await
        .expect("a graph");
    let ids: Vec<String> = [
        "Prompt caching",
        "Transformer key-value cache",
        "Tokenization",
        "Attention",
        "Set",
        "Caching",
        "Retrieval-augmented generation",
    ]
    .iter()
    .map(|n| g.add(n, &format!("What {n} is, in a line."), Concept))
    .collect();
    let relate = |a: usize, kind, b: usize, rank: Option<u8>| Change::Relate {
        from: ids[a].clone(),
        kind,
        to: ids[b].clone(),
        why: format!("{} needs {} to work.", a, b),
        rank,
        model: None,
    };
    let cs = Changeset {
        agent: "test".into(),
        comment: String::new(),
        changes: vec![
            relate(0, RestsOn, 1, Some(1)),
            relate(0, RestsOn, 2, Some(2)),
            relate(1, RestsOn, 3, Some(1)),
            relate(3, RestsOn, 4, Some(1)),
            relate(0, KindOf, 5, None),
            relate(0, Alternative, 6, None),
        ],
        written_by: None,
    };
    let applied = g.client(true).apply(&cs, "relations").await.expect("taken");
    assert!(applied.skipped.is_empty(), "{:?}", applied.skipped);
    (g, ids)
}

#[tokio::test]
async fn an_idea_is_drawn_down_to_the_ground_the_open_way_followed() {
    let (g, ids) = related().await;
    let mut config = test_config();
    config.endpoints.graph = g.url.clone();
    // Reading the graph is for someone signed in.
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    let mut k = Kelve::with(config).await;
    let root = folder(&[("src/llm.rs", "fn body() {}\n")]);
    signed_in(&mut k, &root).await;
    {
        let mut conn = k.app.db.acquire().await.unwrap();
        kelve_store::entries::keep_idea(
            &mut conn,
            &kelve_store::entries::Idea {
                key: ids[0].clone(),
                name: "Prompt caching".into(),
                definition: "Reusing a prompt's start.".into(),
                kind: "concept".into(),
                became: None,
            },
        )
        .await
        .unwrap();
    }
    let o = k.get(&format!("/api/ideas/{}/outline", ids[0])).await;
    let rows: Vec<Vec<(&str, bool)>> = o["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| (i["name"].as_str().unwrap(), i["on"].as_bool().unwrap()))
                .collect()
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            vec![
                ("Transformer key-value cache", true),
                ("Tokenization", false)
            ],
            vec![("Attention", true)],
            vec![("Set", true)],
        ],
        "the most needed, followed down to the ground"
    );
    assert_eq!(
        o["rows"][0]["items"][1]["traced"], false,
        "nothing known below it"
    );
    assert_eq!(o["rows"][2]["items"][0]["foundation"], true);
    assert_eq!(o["kindOf"][0]["name"], "Caching");
    assert_eq!(
        o["alternatives"][0]["name"],
        "Retrieval-augmented generation"
    );
    assert_eq!(o["grounds"][0]["name"], "Set");
    assert_eq!(o["rows"][0]["items"][0]["why"], "0 needs 1 to work.");

    // Another way down, chosen.
    let o = k
        .get(&format!("/api/ideas/{}/outline?path={}", ids[0], ids[2]))
        .await;
    assert_eq!(o["rows"].as_array().unwrap().len(), 1);
    assert_eq!(o["rows"][0]["items"][1]["on"], true);

    // A first principle, with what rests on it above.
    {
        let mut conn = k.app.db.acquire().await.unwrap();
        kelve_store::entries::keep_idea(
            &mut conn,
            &kelve_store::entries::Idea {
                key: ids[4].clone(),
                name: "Set".into(),
                definition: "A collection of distinct things.".into(),
                kind: "concept".into(),
                became: None,
            },
        )
        .await
        .unwrap();
    }
    let o = k.get(&format!("/api/ideas/{}/outline", ids[4])).await;
    assert_eq!(o["foundation"], true);
    assert_eq!(o["proposing"], false, "a first principle rests on nothing");
    assert_eq!(o["above"][0]["name"], "Attention");

    // A reader says a relation doesn't help: the graph's judges hear
    // which, by the graph's IDs, and why.
    let (status, said, _) = k
        .call(
            "POST",
            &format!("/api/ideas/{}/flag", ids[0]),
            Some(json!({ "why": "Tokens aren't what this rests on.", "relation": { "kind": "restsOn", "to": ids[2] } })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    let flags = g.flags();
    let r = flags[0].relation.as_ref().expect("about a relation");
    assert_eq!(
        (r.from.as_str(), r.to.as_str()),
        (ids[0].as_str(), ids[2].as_str())
    );
    assert!(flags[0].revision.is_empty(), "no entry needed");
}

#[tokio::test]
async fn an_idea_found_to_be_two_offers_both_and_its_marks_are_made_again() {
    use kelve_core::ideas::IdeaKind::Concept;
    let g = kelve_graph_api::stand_in::StandIn::start()
        .await
        .expect("a graph");
    let norm = g.add(
        "Normalization",
        "Making things comparable by a standard form.",
        Concept,
    );
    let mut config = test_config();
    config.endpoints.graph = g.url.clone();
    // Reading the graph is for someone signed in.
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    let mut k = Kelve::with(config).await;
    let root = folder(&[("src/text.rs", "fn fold() {}\n")]);
    signed_in(&mut k, &root).await;
    let mark = serde_json::json!([{
        "start": 1, "end": 1, "span": null, "name": "Normalization",
        "definition": "Making things comparable.",
        "idea": { "id": norm, "name": "Normalization" },
    }])
    .to_string();
    {
        let mut conn = k.app.db.acquire().await.unwrap();
        kelve_store::entries::keep_idea(
            &mut conn,
            &kelve_store::entries::Idea {
                key: norm.clone(),
                name: "Normalization".into(),
                definition: "Making things comparable by a standard form.".into(),
                kind: "concept".into(),
                became: None,
            },
        )
        .await
        .unwrap();
        kelve_store::marks::put(&mut conn, "passage", "v1", "model", &mark)
            .await
            .unwrap();
        kelve_store::marks::put(&mut conn, "other", "v1", "model", "[]")
            .await
            .unwrap();
    }
    let two = g.split(
        &norm,
        [
            (
                "Unicode normalization",
                "Making text that looks the same compare as the same.",
            ),
            (
                "Normalization (statistics)",
                "Rescaling values to a common range.",
            ),
        ],
    );

    // Reading how it relates is when kelve hears.
    k.get(&format!("/api/ideas/{norm}/outline")).await;
    let page = k.get(&format!("/api/ideas/{norm}")).await;
    let offered: Vec<&str> = page["split"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        offered,
        ["Unicode normalization", "Normalization (statistics)"]
    );
    assert_eq!(page["split"][0]["id"], two[0]);
    assert!(page["entry"].is_null(), "nothing is written for it");
    // Each of the two opens.
    let one = k.get(&format!("/api/ideas/{}", two[1])).await;
    assert_eq!(one["name"], "Normalization (statistics)");

    let mut conn = k.app.db.acquire().await.unwrap();
    assert_eq!(
        kelve_store::marks::get(&mut conn, "passage", "v1", "model")
            .await
            .unwrap(),
        None,
        "its passage is marked again"
    );
    assert!(
        kelve_store::marks::get(&mut conn, "other", "v1", "model")
            .await
            .unwrap()
            .is_some(),
        "and only its"
    );
}

#[tokio::test]
async fn the_persons_agent_opens_what_they_asked_for_and_does_kelves_work() {
    let mut k = Kelve::new().await;
    let root = folder(&[
        (
            "notes/heat-pump.md",
            "# Heat pump\n\nA COP of 3.2 at 45 °C flow.\n",
        ),
        (
            "notes/loft.md",
            "# Loft\n\nInsulate to a U-value of 0.16.\n",
        ),
        ("notes/other.md", "# Other\n"),
    ]);
    // Only with this computer's secret.
    let (status, _, _) = k
        .call(
            "POST",
            "/api/agent/next",
            Some(json!({ "agent": "codex" })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // Some files: their folder, with only them.
    let body = json!({
        "what": "files", "agent": "codex", "said": "Notes on a heat pump.", "title": "Heating the house",
        "paths": [
            root.join("notes/heat-pump.md").display().to_string(),
            root.join("notes/loft.md").display().to_string(),
        ],
    });
    let (status, opened, _) = k.call("POST", "/api/agent/open", Some(body), true).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    // The agent's title, in place of the folder's name.
    assert_eq!(opened["title"], "Heating the house");
    // Both files; their folder's page is asked for once the way in is found.
    assert_eq!(
        (
            opened["files"].as_u64(),
            opened["folders"].as_u64(),
            opened["toRead"].as_u64()
        ),
        (Some(2), Some(0), Some(2))
    );
    assert_eq!(opened["shown"], true);
    let link = k.links.lock().unwrap().last().cloned().unwrap();
    let (_, _, cookie) = k.land(&link).await;
    k.cookie = Some(cookie.expect("a session cookie"));
    let lately = k.get("/api/opened").await;
    assert_eq!(lately[0]["said"], "Notes on a heat pump.");
    assert_eq!(lately[0]["agent"], "codex");
    assert_eq!(lately[0]["only"], json!(["heat-pump.md", "loft.md"]));
    let id = lately[0]["id"].as_i64().unwrap();
    let f = k.get(&format!("/api/opened/{id}/folder")).await;
    assert_eq!(
        f["files"].as_array().unwrap().len(),
        2,
        "only what was opened"
    );

    // The agent takes a job; the browser can't.
    let (status, _, _) = k
        .call(
            "POST",
            "/api/agent/next",
            Some(json!({ "agent": "codex" })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    kelve_server::pipeline::run_once(&k.app).await.unwrap();
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let (status, way, _) = k
        .call(
            "POST",
            "/api/agent/next",
            Some(json!({ "agent": "codex", "model": "gpt-6" })),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{way}");
    // The way in first, then its files.
    assert_eq!(way["kind"], "way", "{way}");
    let (_, taken, _) = k
        .call(
            "POST",
            "/api/agent/answer",
            Some(json!({ "job": way["job"], "answer": stand_in_way(&way), "agent": "codex", "model": "gpt-6" })),
            true,
        )
        .await;
    assert_eq!(taken["taken"], "yes", "{taken}");
    // Its overview first, then its files.
    let overview = taken["next"].clone();
    assert_eq!(overview["kind"], "plainlyFolder", "{overview}");
    let (_, taken, _) = k
        .call(
            "POST",
            "/api/agent/answer",
            Some(json!({ "job": overview["job"], "answer": stand_in_overview(&overview), "agent": "codex", "model": "gpt-6" })),
            true,
        )
        .await;
    assert_eq!(taken["taken"], "yes", "{taken}");
    let job = taken["next"].clone();
    assert_eq!(job["kind"], "read", "{job}");
    assert!(job["about"].as_str().unwrap().starts_with("Reading "));
    // The person sees it's with their agent.
    let kept = k.get("/api/kept").await;
    assert!(kept.as_array().unwrap().iter().any(|j| j["taken"] == true));
    let me = k.get("/api/me").await;
    let agent = &me["agent"];
    assert_eq!(
        (agent["name"].as_str(), agent["model"].as_str()),
        (Some("codex"), Some("gpt-6"))
    );
    // Every agent seen lately, the latest first, each with when it last
    // asked.
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    k.call(
        "POST",
        "/api/agent/hello",
        Some(json!({ "agent": "claude-code" })),
        true,
    )
    .await;
    let agents = k.get("/api/me").await["agents"].clone();
    let names: Vec<&str> = agents
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["claude-code", "codex"]);
    assert!(agents[1]["lastSeenAt"].is_string());

    // A wrong answer comes back with why; a right one is taken, with the
    // next job.
    let (status, wrong, _) = k
        .call(
            "POST",
            "/api/agent/answer",
            Some(json!({ "job": job["job"], "answer": { "marks": [] }, "agent": "codex" })),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(wrong["taken"], "no");
    assert!(!wrong["why"].as_str().unwrap().is_empty());
    // It stays with the agent that handed it in, to fix.
    let (_, taken, _) = k
        .call(
            "POST",
            "/api/agent/answer",
            Some(json!({ "job": job["job"], "answer": { "passages": [] }, "agent": "codex" })),
            true,
        )
        .await;
    assert_eq!(taken["taken"], "yes", "{taken}");
    assert!(taken["next"]["job"].is_number(), "{taken}");

    // The person takes a job off the list.
    let next = taken["next"]["job"].as_i64().unwrap();
    let (status, left, _) = k
        .call("DELETE", &format!("/api/kept/{next}"), None, false)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(left.as_array().unwrap().iter().all(|j| j["id"] != next));
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn an_idea_opened_is_read_from_then_on_and_nothing_else_is() {
    let (g, ids) = related().await;
    let mut config = test_config();
    config.endpoints.graph = g.url.clone();
    // Reading the graph is for someone signed in.
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    let mut k = Kelve::with(config).await;
    let root = folder(&[("src/llm.rs", "fn body() {}\n")]);
    signed_in(&mut k, &root).await;
    assert_eq!(k.get("/api/read").await, json!([]));
    let _ = k.get(&format!("/api/ideas/{}", ids[0])).await;
    let _ = k.get(&format!("/api/ideas/{}", ids[0])).await;
    // Its outline is how it relates, not reading it.
    let _ = k.get(&format!("/api/ideas/{}/outline", ids[0])).await;
    assert_eq!(k.get("/api/read").await, json!([ids[0]]));
    // Deleting everything forgets it.
    let (status, _, _) = k.call("DELETE", "/api/everything", None, false).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(k.get("/api/read").await, json!([]));
    std::fs::remove_dir_all(&root).unwrap();
}

/// A read job's answer that marks nothing in any passage.
fn none_marked(job: &Value) -> Value {
    let passages: Vec<Value> = job["material"]["passages"]
        .as_array()
        .map(|ps| {
            ps.iter()
                .map(|p| json!({ "passage": p["id"], "marks": [] }))
                .collect()
        })
        .unwrap_or_default();
    json!({ "passages": passages })
}

/// The stand-in agent's way in: every file it's given, in kelve's order,
/// up to the most a way in has; no parts, and no folder lines.
fn stand_in_way(job: &Value) -> Value {
    let order: Vec<Value> = job["material"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .take(40)
        .map(|p| json!({ "path": p, "why": "Where it fits." }))
        .collect();
    json!({ "order": order, "parts": [], "folders": [] })
}

/// Takes the agent's jobs until one of `kind` comes, answering the others
/// as an agent with nothing to say would. Handing one in brings the next.
async fn job_of(k: &Kelve, kind: &str) -> Value {
    let mut next = Value::Null;
    loop {
        let job = if next.is_null() {
            while kelve_server::pipeline::run_once(&k.app)
                .await
                .unwrap()
                .is_some()
            {}
            let (status, job, _) = k
                .call(
                    "POST",
                    "/api/agent/next",
                    Some(json!({ "agent": "claude-code" })),
                    true,
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{job}");
            past_overview(k, job).await
        } else {
            past_overview(k, std::mem::take(&mut next)).await
        };
        assert!(!job.is_null(), "no {kind} job came");
        if job["kind"] == kind {
            return job;
        }
        let answer = match job["kind"].as_str() {
            Some("read") => none_marked(&job),
            Some("builtOn") => json!({ "chosen": [] }),
            Some("folders") => json!({ "folders": [] }),
            Some("way") => stand_in_way(&job),
            Some("path") => json!({ "parts": [] }),
            other => panic!("a {other:?} job: {job}"),
        };
        let (_, taken, _) = k
            .call(
                "POST",
                "/api/agent/answer",
                Some(json!({ "job": job["job"], "answer": answer, "agent": "claude-code" })),
                true,
            )
            .await;
        assert_eq!(taken["taken"], "yes", "{taken}");
        next = taken["next"].clone();
    }
}

#[tokio::test]
async fn the_way_in_the_agent_finds_is_kept_with_the_project() {
    let mut k = Kelve::new().await;
    let root = folder(&[
        ("README.md", "# ledger\n"),
        ("src/main.rs", "use crate::bank::Bank;\nfn main() {}\n"),
        (
            "src/bank/mod.rs",
            "use crate::retry::with_retries;\npub struct Bank;\n",
        ),
        (
            "src/retry.rs",
            "const MAX_ATTEMPTS: u32 = 6;\n\npub fn with_retries() {}\n",
        ),
    ]);
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(
        &root,
        &[
            "commit",
            "-q",
            "-m",
            "Retry in the client, where we know what's safe",
        ],
    );

    // Opened, the agent's first job is finding the way in.
    let body = json!({
        "what": "folder", "agent": "claude-code", "path": root.display().to_string(), "title": "ledger",
    });
    let (status, opened, _) = k.call("POST", "/api/agent/open", Some(body), true).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    let link = k.links.lock().unwrap().last().cloned().unwrap();
    let (_, _, cookie) = k.land(&link).await;
    k.cookie = Some(cookie.expect("a session cookie"));
    let id = k.get("/api/opened").await[0]["id"].as_i64().unwrap();
    let way = next_job(&k).await;
    assert_eq!(way["kind"], "way", "{way}");
    let said = hand_in(
        &k,
        &way,
        json!({
            "order": [
                { "path": "README.md", "why": "What ledger is for, in its own words." },
                { "path": "src/main.rs", "why": "Where it starts." },
            ],
            "parts": [],
            "folders": [{ "path": "src/bank", "line": "Talking to the bank, and nothing else." }],
        }),
    )
    .await;
    assert_eq!(said["taken"], "yes", "{said}");
    let o = k.get(&format!("/api/opened/{id}")).await;
    let order: Vec<&str> = o["way"]["order"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["path"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["README.md", "src/main.rs"]);
    assert_eq!(
        o["way"]["folders"],
        json!([{ "path": "src/bank", "line": "Talking to the bank, and nothing else." }])
    );
    // Kept with the project, for everyone with the repo.
    let kept: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join(".kelve/way.json")).unwrap())
            .unwrap();
    assert_eq!(kept["order"][1]["path"], "src/main.rs");
    assert_eq!(
        kept["folders"]["src/bank"],
        "Talking to the bank, and nothing else."
    );

    // A folder's page, while its plainly isn't written: its parts, each
    // folder with its line, and a file with what it's for once it's read
    // plainly.
    let part = k.get(&format!("/api/opened/{id}/part?path=src")).await;
    let parts: Vec<&str> = part["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["path"].as_str().unwrap())
        .collect();
    let at = |p: &str| parts.iter().position(|x| *x == p).unwrap();
    assert_eq!(
        part["parts"][at("src/bank")]["line"],
        "Talking to the bank, and nothing else."
    );
    assert_eq!(part["parts"][at("src/main.rs")]["line"], Value::Null);
    assert_eq!(part["parts"][at("src/main.rs")]["folder"], false);
    let (status, _, _) = k
        .call(
            "GET",
            &format!("/api/opened/{id}/part?path=../.."),
            None,
            false,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    std::fs::remove_dir_all(&root).unwrap();
}

/// Takes the agent's next job, after kelve's own work: none when there's none.
async fn next_job(k: &Kelve) -> Value {
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let (status, job, _) = k
        .call(
            "POST",
            "/api/agent/next",
            Some(json!({ "agent": "claude-code" })),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{job}");
    past_overview(k, job).await
}

/// The stand-in agent writes what's open's overview, the first job after
/// the way in, as it comes, and is given the job after it: the tests are
/// about what follows. Only the first page of its kind for a thing is the
/// overview; the top page written again later is the folder's own.
async fn past_overview(k: &Kelve, job: Value) -> Value {
    let kind = job["kind"].as_str().unwrap_or_default();
    // A folder's top page; in a change, the change's own.
    let overview = (kind == "plainlyFolder"
        && job["material"]["path"] == ""
        && job["material"]["changed"].is_null())
        || kind == "plainlyChange";
    let about = job["about"].as_str().unwrap_or_default().to_string();
    if !overview || !k.overviews.lock().unwrap().insert(about) {
        return job;
    }
    let said = hand_in(k, &job, stand_in_overview(&job)).await;
    assert_eq!(said["taken"], "yes", "the overview wasn't taken: {said}");
    // Handing it in brought the next, the agent's now.
    match &said["next"] {
        Value::Null => Box::pin(next_job(k)).await,
        next => Box::pin(past_overview(k, next.clone())).await,
    }
}

/// The stand-in's overview, by what the job is.
fn stand_in_overview(job: &Value) -> Value {
    if job["kind"] == "plainlyChange" {
        let files: Vec<Value> = job["material"]["files"]
            .as_array()
            .into_iter()
            .flatten()
            .take(7)
            .map(|f| json!({ "mark": "changed", "goal": "Changes a file", "says": "What it changes.", "files": [f["path"]] }))
            .collect();
        json!({ "what": "What the change does, as the stand-in reads it.", "kind": "updates", "parts": files })
    } else {
        json!({ "what": "The whole of it, as the stand-in reads it.", "shape": "parts", "parts": [
            { "part": job["material"]["parts"][0]["path"], "goal": "Holds it", "says": "All of it." }
        ] })
    }
}

/// The agent finds the way in, kelve's first job for anything opened with
/// files in it, as the stand-in does, and takes no other.
async fn find_way(k: &Kelve) {
    find_way_as(k, "claude-code").await;
}

/// The same, as an agent of another name.
async fn find_way_as(k: &Kelve, agent: &str) {
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let (status, job, _) = k
        .call(
            "POST",
            "/api/agent/next",
            Some(json!({ "agent": agent })),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{job}");
    assert_eq!(job["kind"], "way", "the way in first: {job}");
    let (_, said, _) = k
        .call(
            "POST",
            "/api/agent/answer",
            Some(json!({ "job": job["job"], "agent": agent, "answer": stand_in_way(&job), "more": false })),
            true,
        )
        .await;
    assert_eq!(said["taken"], "yes", "{said}");
    // Then its overview, the first job after it.
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let (_, job, _) = k
        .call(
            "POST",
            "/api/agent/next",
            Some(json!({ "agent": agent })),
            true,
        )
        .await;
    assert!(
        job["kind"] == "plainlyFolder" || job["kind"] == "plainlyChange",
        "the overview after the way in: {job}"
    );
    k.overviews
        .lock()
        .unwrap()
        .insert(job["about"].as_str().unwrap_or_default().to_string());
    let (_, said, _) = k
        .call(
            "POST",
            "/api/agent/answer",
            Some(json!({ "job": job["job"], "agent": agent, "answer": stand_in_overview(&job), "more": false })),
            true,
        )
        .await;
    assert_eq!(said["taken"], "yes", "{said}");
}

/// The job kelve gave with the last answer, or else the next.
async fn take_next(k: &Kelve, given: &mut Value) -> Value {
    if given.is_null() {
        next_job(k).await
    } else {
        past_overview(k, std::mem::take(given)).await
    }
}

/// Hands in an answer to a job: what kelve said of it, with the next job,
/// the agent's now, when it was taken.
async fn hand_in(k: &Kelve, job: &Value, answer: Value) -> Value {
    let (_, said, _) = k
        .call(
            "POST",
            "/api/agent/answer",
            Some(json!({ "job": job["job"], "agent": "claude-code", "answer": answer })),
            true,
        )
        .await;
    said
}

#[tokio::test]
async fn every_file_is_read_plainly_once_its_reading_is_handed_in() {
    let mut k = Kelve::new().await;
    let retry =
        "use std::time::Duration;\n\nconst MAX_ATTEMPTS: u32 = 6;\n\npub fn with_retries() {}\n";
    let root = folder(&[
        ("README.md", "# ledger\n\nIt keeps the books.\n"),
        ("src/retry.rs", retry),
    ]);
    let id = signed_in(&mut k, &root).await;
    let plainly = format!("/api/opened/{id}/plainly?path=src/retry.rs");

    // Opening the folder has its files read: as each is handed in, it's
    // read plainly, code and text alike, as part of reading it.
    k.get(&format!("/api/opened/{id}/folder")).await;
    let (mut job, mut readme, mut given) = (Value::Null, false, Value::Null);
    while job.is_null() || !readme {
        let next = take_next(&k, &mut given).await;
        assert!(!next.is_null(), "both files weren't read plainly");
        let answer = match next["kind"].as_str() {
            Some("read") => none_marked(&next),
            Some("folders") => json!({ "folders": [] }),
            Some("way") => stand_in_way(&next),
            Some("path") => json!({ "parts": [] }),
            Some("plainly") if next["material"]["path"] == "src/retry.rs" => {
                job = next;
                continue;
            }
            Some("plainly") => {
                assert_eq!(next["material"]["path"], "README.md");
                assert_eq!(next["material"]["code"], false, "text reads plainly too");
                readme = true;
                json!({ "what": "Says what the ledger is.", "parts": [
                    { "from": 1, "to": 3, "goal": "Names the ledger", "says": "It keeps the books." }
                ] })
            }
            other => panic!("a {other:?} job: {next}"),
        };
        let said = hand_in(&k, &next, answer).await;
        assert_eq!(said["taken"], "yes", "{said}");
        given = said["next"].clone();
    }

    // Its ideas checked, but not yet read plainly, a file isn't read yet,
    // though it fills the line, lighter; the folder's own page is last.
    let folder_reading = || async {
        let view = k.get(&format!("/api/opened/{id}/folder")).await;
        let r = &view["reading"];
        (r["read"].clone(), r["of"].clone(), r["checking"].clone())
    };
    // Two files, then two folders' pages: the top's, its overview written
    // first, and src's.
    assert_eq!(folder_reading().await, (json!(2), json!(4), json!(1)));

    // On its list as part of reading, and while it's with the agent, the
    // file says so.
    let p = k.get(&plainly).await;
    assert!(p["parts"].is_null(), "{p}");
    assert_eq!(p["code"], true);
    assert_eq!(p["waiting"]["writing"], true, "{p}");
    assert_eq!(p["waiting"]["before"], 0, "{p}");
    assert!(
        p["waiting"]["ask"].is_number(),
        "its job, to move it up: {p}"
    );
    let list = k.get("/api/list").await;
    assert!(
        list["now"]
            .as_array()
            .unwrap()
            .iter()
            .chain(list["next"].as_array().unwrap())
            .any(|i| i["kind"] == "reading"
                && i["hand"]
                    .as_array()
                    .is_some_and(|h| h.iter().any(|f| f["path"] == "src/retry.rs"))),
        "{list}"
    );
    assert!(
        job["instructions"]
            .as_str()
            .unwrap()
            .contains("never goes to the shared graph"),
        "{job}"
    );
    assert_eq!(job["version"], "plainly@2");
    assert_eq!(job["material"]["lineCount"], 5);
    assert_eq!(job["material"]["blankLines"], json!([2, 4]));
    assert_eq!(job["material"]["code"], true);
    assert!(job["material"]["change"].is_null());
    assert!(
        job["material"]["text"]
            .as_str()
            .unwrap()
            .contains("\n3  const MAX_ATTEMPTS"),
        "fenced, with its lines numbered no wider than the last: {job}"
    );

    // An answer well over a limit comes back with what to do; lines left
    // out are set right by kelve, not sent back.
    let part = |from: u32, to: u32, goal: &str, says: &str| json!({ "from": from, "to": to, "goal": goal, "says": says });
    let wrong = hand_in(
        &k,
        &job,
        json!({ "what": "Tries a call again.", "parts": [part(1, 3, "Sets the limit on how many times a call is tried", "Six tries.")] }),
    )
    .await;
    assert_eq!(wrong["taken"], "no", "{wrong}");
    assert!(
        wrong["why"].as_str().unwrap().contains("at most 6"),
        "{wrong}"
    );
    // It stays with the agent fixing it: the fixed answer goes to the same job.
    let taken = hand_in(
        &k,
        &job,
        json!({ "what": "Tries a call to the bank again, a few times at most.", "parts": [
            part(5, 5, "Tries again", "Tries a call again until it works, as `with_retries`."),
            part(3, 3, "Sets the limit", "A call is tried **at most six times**."),
            json!({ "from": 1, "to": 1, "goal": "Brings in time", "says": "Lengths of time.", "also": true }),
        ] }),
    )
    .await;
    assert_eq!(taken["taken"], "yes", "{taken}");
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let p = k.get(&plainly).await;
    assert_eq!(p["parts"].as_array().map(Vec::len), Some(3), "{p}");
    assert_eq!(
        p["what"],
        "Tries a call to the bank again, a few times at most."
    );
    assert_eq!(p["parts"][0]["from"], 5, "in the order things happen");
    assert_eq!(
        p["parts"][1]["says"],
        "A call is tried **at most six times**."
    );
    assert_eq!(p["parts"][1]["mark"], "asBefore");
    assert_eq!(p["parts"][2]["also"], true);
    assert!(p["waiting"].is_null());
    // Both files read, and the overview; the folders' pages are still to come.
    assert_eq!(folder_reading().await, (json!(3), json!(4), json!(0)));

    // A text file reads plainly too.
    let readme = k
        .get(&format!("/api/opened/{id}/plainly?path=README.md"))
        .await;
    assert_eq!(readme["code"], false, "{readme}");
    assert_eq!(readme["parts"][0]["goal"], "Names the ledger", "{readme}");

    // A new text is read plainly again; the text it had is kept.
    std::fs::write(root.join("src/retry.rs"), format!("{retry}// More.\n")).unwrap();
    assert!(k.get(&plainly).await["parts"].is_null());
    std::fs::write(root.join("src/retry.rs"), retry).unwrap();
    assert_eq!(k.get(&plainly).await["parts"], p["parts"]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_folder_is_read_with_each_folder_in_it_bottom_up() {
    let mut k = Kelve::new().await;
    let root = folder(&[
        ("README.md", "# ledger\n"),
        ("src/main.rs", "fn main() {}\n"),
        ("src/api/charges.rs", "pub fn charge() {}\n"),
        // A folder that holds only one folder is that folder's page.
        ("deep/only/one.rs", "pub fn one() {}\n"),
        // Tooling's dot-folders, and what tools made, have no page.
        (".github/workflows/ci.yml", "on: push\n"),
    ]);
    std::fs::create_dir_all(root.join("shots")).unwrap();
    std::fs::write(root.join("shots/a.png"), [0x89, b'P', b'N', b'G', 0, 1, 2]).unwrap();
    let id = signed_in(&mut k, &root).await;
    async fn reading_of(k: &Kelve, id: i64) -> (Value, Value) {
        let view = k.get(&format!("/api/opened/{id}/folder")).await;
        let r = &view["reading"];
        (r["read"].clone(), r["of"].clone())
    }
    let reading = || reading_of(&k, id);
    // Its way in first, then the five text files on it, the picture not
    // among them, then four folders' pages.
    reading().await;
    find_way(&k).await;
    // Its overview written first, then the five files and four folders' pages.
    assert_eq!(reading().await, (json!(1), json!(9)));
    let mut given = Value::Null;
    let mut folders: Vec<String> = Vec::new();
    loop {
        let job = take_next(&k, &mut given).await;
        assert!(!job.is_null(), "the top's page didn't come: {folders:?}");
        let answer = match job["kind"].as_str() {
            Some("read") => none_marked(&job),
            Some("folders") => json!({ "folders": [] }),
            Some("way") => stand_in_way(&job),
            Some("path") => json!({ "parts": [] }),
            Some("plainly") => json!({ "what": "A part of the ledger.", "parts": [
                { "from": 1, "to": job["material"]["lineCount"], "goal": "Does its part", "says": "What it does." }
            ] }),
            Some("plainlyFolder") => {
                let path = job["material"]["path"].as_str().unwrap().to_string();
                if path.is_empty() {
                    // Written from the pages of the folders in it.
                    let listed = job["material"]["listed"].as_str().unwrap();
                    assert!(
                        listed.contains("src/ (folder): The src folder."),
                        "{listed}"
                    );
                    assert!(
                        listed.contains("deep/only/ (folder): The deep/only folder."),
                        "{listed}"
                    );
                }
                folders.push(path.clone());
                json!({ "what": format!("The {path} folder."), "shape": "parts", "parts": [
                    { "part": job["material"]["parts"][0]["path"], "goal": "Holds the code", "says": "All of it." }
                ] })
            }
            other => panic!("a {other:?} job: {job}"),
        };
        let said = hand_in(&k, &job, answer).await;
        assert_eq!(said["taken"], "yes", "{said}");
        given = said["next"].clone();
        if folders.last().is_some_and(String::is_empty) {
            break;
        }
    }
    // Bottom up: each after the folders in it, the top last.
    let at = |p: &str| folders.iter().position(|f| f == p).unwrap();
    assert_eq!(folders.len(), 4, "{folders:?}");
    assert!(at("src/api") < at("src"), "{folders:?}");
    assert_eq!(at(""), 3, "{folders:?}");
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    assert_eq!(reading().await, (json!(9), json!(9)));

    // Opened again as it is, nothing is asked again.
    k.open(json!({ "path": root.display().to_string() })).await;
    assert_eq!(reading_of(&k, id).await, (json!(9), json!(9)));
    let next = next_job(&k).await;
    assert!(
        next.is_null()
            || !next["kind"]
                .as_str()
                .unwrap_or_default()
                .starts_with("plainly"),
        "{next}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_page_waiting_says_its_place_as_the_list_shows_it() {
    let mut k = Kelve::new().await;
    let files: Vec<(String, String)> = (1..=6)
        .map(|i| (format!("src/f{i}.rs"), format!("pub fn f{i}() {{}}\n")))
        .collect();
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let root = folder(&named);
    let id = signed_in(&mut k, &root).await;
    k.get(&format!("/api/opened/{id}/folder")).await;
    find_way(&k).await;
    let run = || async {
        while kelve_server::pipeline::run_once(&k.app)
            .await
            .unwrap()
            .is_some()
        {}
    };
    run().await;
    // Every file read at once, by agents side by side: each file's
    // plainly is asked where its reading was, many jobs in all.
    let caller = kelve_server::agent::Caller {
        agent: "claude-code".into(),
        model: None,
        plan: None,
        path: None,
        only: None,
    };
    let reads: Vec<kelve_store::asks::Ask> = {
        let mut conn = k.app.db.acquire().await.unwrap();
        kelve_store::asks::open(&mut conn)
            .await
            .unwrap()
            .into_iter()
            .filter(|a| a.kind == "read")
            .collect()
    };
    assert_eq!(reads.len(), 6);
    for read in &reads {
        let job = json!({ "material": serde_json::from_str::<Value>(&read.material).unwrap() });
        kelve_server::agent::hand_in(&k.app, &caller, read.id, none_marked(&job))
            .await
            .unwrap();
    }
    run().await;
    let kinds: Vec<String> = {
        let mut conn = k.app.db.acquire().await.unwrap();
        kelve_store::asks::open(&mut conn)
            .await
            .unwrap()
            .into_iter()
            .map(|a| a.kind)
            .collect()
    };
    assert_eq!(
        kinds.iter().filter(|k| *k == "plainly").count(),
        6,
        "{kinds:?}"
    );
    // The last file's page: five files before it, as the list counts them.
    let p = k
        .get(&format!("/api/opened/{id}/plainly?path=src/f6.rs"))
        .await;
    assert_eq!(p["waiting"]["writing"], false, "{p}");
    assert_eq!(p["waiting"]["before"], 5, "{p}");
    // The first file's: none, so next.
    let p = k
        .get(&format!("/api/opened/{id}/plainly?path=src/f1.rs"))
        .await;
    assert_eq!(p["waiting"]["before"], 0, "{p}");

    // An entry asked for meanwhile: its row comes after the one row for
    // all of the reading, whatever number of jobs that is.
    let idea = kelve_store::entries::Idea {
        key: "new:advisory locks".into(),
        name: "Advisory locks".into(),
        definition: "A lock a program names, and the database keeps.".into(),
        kind: "concept".into(),
        became: None,
    };
    {
        let mut conn = k.app.db.acquire().await.unwrap();
        kelve_store::entries::keep_idea(&mut conn, &idea)
            .await
            .unwrap();
        kelve_store::asks::add(
            &mut conn,
            &kelve_store::asks::New {
                key: "write:advisory locks".into(),
                kind: "write".into(),
                about: "Writing the entry for Advisory locks".into(),
                idea: Some(idea.key.clone()),
                opened: None,
                material: "{}".into(),
                schema: "{}".into(),
                resume_kind: "entry.write".into(),
                resume_key: None,
                resume_payload: "{}".into(),
                priority: 0,
                follows: None,
            },
        )
        .await
        .unwrap();
    }
    let list = k.get("/api/list").await;
    let rows: Vec<&Value> = ["now", "next", "elsewhere"]
        .iter()
        .flat_map(|g| list[*g].as_array().unwrap())
        .collect();
    let keys: Vec<&str> = rows.iter().map(|r| r["key"].as_str().unwrap()).collect();
    assert_eq!(
        keys,
        [format!("read:{id}").as_str(), "idea:new:advisory locks"],
        "the second row you see: {list}"
    );
    // The reading row counts what's left as the pages do: six files, not
    // their jobs and the folders' lines.
    assert_eq!(rows[0]["left"], 6, "{list}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_file_opened_alone_has_no_pages() {
    let k = Kelve::new().await;
    let root = folder(&[("src/retry.rs", "pub fn retry() {}\n")]);
    let body = json!({ "what": "file", "agent": "codex", "path": root.join("src/retry.rs").display().to_string() });
    let (status, opened, _) = k.call("POST", "/api/agent/open", Some(body), true).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    assert_eq!(
        (
            opened["files"].as_u64(),
            opened["folders"].as_u64(),
            opened["toRead"].as_u64()
        ),
        (Some(1), Some(0), Some(1))
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_file_read_as_code_is_read_plainly_only_when_shown() {
    let mut k = Kelve::new().await;
    let root = folder(&[
        ("README.md", "# ledger\n\nIt keeps the books.\n"),
        ("src/retry.rs", "const MAX_ATTEMPTS: u32 = 6;\n"),
    ]);
    let id = signed_in(&mut k, &root).await;
    // Rust reads as code, as What kelve marks says.
    let spec = kelve_core::spec::Spec {
        areas: vec![kelve_core::spec::AreaLevel {
            area: "Rust".into(),
            level: kelve_core::lens::Level::Working,
            read: Some(kelve_core::spec::ReadAs::Code),
        }],
        ..Default::default()
    };
    kelve_server::spec::save(&k.app, spec, "you").unwrap();

    // Both files are read; only the text is read plainly, and the folder
    // doesn't wait for the code.
    k.get(&format!("/api/opened/{id}/folder")).await;
    let (mut given, mut readme) = (Value::Null, false);
    loop {
        let next = take_next(&k, &mut given).await;
        assert!(!next.is_null(), "the folder's plainly didn't come");
        let answer = match next["kind"].as_str() {
            Some("plainlyFolder") => break,
            Some("read") => none_marked(&next),
            Some("folders") => json!({ "folders": [] }),
            Some("way") => stand_in_way(&next),
            Some("path") => json!({ "parts": [] }),
            Some("plainly") => {
                assert_eq!(next["material"]["path"], "README.md", "{next}");
                readme = true;
                json!({ "what": "Says what the ledger is.", "parts": [
                    { "from": 1, "to": 3, "goal": "Names the ledger", "says": "It keeps the books." }
                ] })
            }
            other => panic!("a {other:?} job: {next}"),
        };
        given = hand_in(&k, &next, answer).await["next"].clone();
    }
    assert!(readme, "the text wasn't read plainly");

    // Only looked at, ahead of opening it, nothing is asked.
    let p = k
        .get(&format!(
            "/api/opened/{id}/plainly?path=src/retry.rs&ahead=true"
        ))
        .await;
    assert!(p["parts"].is_null(), "{p}");
    let key = format!("{}:{id}:src/retry.rs", kelve_server::plainly::JOB);
    let asked = || async {
        let mut conn = k.app.db.acquire().await.unwrap();
        kelve_store::jobs::due(&mut conn, &key)
            .await
            .unwrap()
            .is_some()
    };
    assert!(!asked().await, "looking asked for it");
    // Shown plainly anyway, it's asked for then.
    let p = k
        .get(&format!("/api/opened/{id}/plainly?path=src/retry.rs"))
        .await;
    assert!(p["parts"].is_null(), "{p}");
    assert!(!p["waiting"].is_null(), "{p}");
    assert!(asked().await, "showing it didn't ask for it");
    let job = job_of(&k, "plainly").await;
    assert_eq!(job["material"]["path"], "src/retry.rs");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_folder_reads_plainly_by_steps_that_really_lead_on() {
    let mut k = Kelve::new().await;
    let root = folder(&[
        ("src/main.rs", "use crate::bank::Bank;\nfn main() {}\n"),
        (
            "src/bank/mod.rs",
            "use crate::retry::with_retries;\npub struct Bank;\n",
        ),
        ("src/retry.rs", "pub fn with_retries() {}\n"),
    ]);
    let id = signed_in(&mut k, &root).await;
    k.get(&format!("/api/opened/{id}/folder")).await;
    find_way(&k).await;
    let src = format!("/api/opened/{id}/plainly/folder?path=src");
    let f = k.get(&src).await;
    assert!(f["what"].is_null(), "{f}");
    assert!(f["waiting"].is_object(), "{f}");
    // Its files are read, and read plainly, first.
    let mut given = Value::Null;
    let job = loop {
        let job = take_next(&k, &mut given).await;
        assert!(!job.is_null(), "no folder's plainly came");
        let answer = match job["kind"].as_str() {
            Some("read") => none_marked(&job),
            Some("folders") => json!({ "folders": [] }),
            Some("way") => stand_in_way(&job),
            Some("path") => json!({ "parts": [] }),
            Some("plainly") => json!({ "what": "A part of the ledger.", "parts": [
                { "from": 1, "to": job["material"]["lineCount"], "goal": "Does its part", "says": "What it does." }
            ] }),
            Some("plainlyFolder") if job["material"]["path"] == "src" => break job,
            Some("plainlyFolder") => json!({ "what": "The ledger.", "shape": "parts", "parts": [
                { "part": job["material"]["parts"][0]["path"], "goal": "Holds the code", "says": "All of it." }
            ] }),
            other => panic!("a {other:?} job: {job}"),
        };
        let said = hand_in(&k, &job, answer).await;
        assert_eq!(said["taken"], "yes", "{said}");
        given = said["next"].clone();
    };
    assert_eq!(job["version"], "plainlyFolder@1");
    let listed = job["material"]["listed"].as_str().unwrap();
    assert!(
        listed.contains("src/retry.rs (file): A part of the ledger."),
        "{listed}"
    );
    let step = |part: &str| json!({ "part": part, "goal": "Takes a step", "says": "It hands on." });
    // Steps that don't lead on come back, with what to do.
    let wrong = hand_in(
        &k,
        &job,
        json!({ "what": "The ledger's code.", "shape": "steps", "parts": [step("src/main.rs"), step("src/retry.rs")] }),
    )
    .await;
    assert_eq!(wrong["taken"], "no", "{wrong}");
    let why = wrong["why"].as_str().unwrap();
    assert!(why.contains("neither uses the other"), "{why}");
    assert!(why.contains("give shape parts"), "{why}");
    // It stays with the agent fixing it: the fixed answer goes to the same job.
    let taken = hand_in(
        &k,
        &job,
        json!({ "what": "The ledger's code: a call goes in at main, through the bank, and is tried again.", "shape": "steps",
                "parts": [step("src/main.rs"), step("src/bank"), step("src/retry.rs")],
                "keeps": [{ "says": "Only the bank calls out.", "by": "src/bank" }] }),
    )
    .await;
    assert_eq!(taken["taken"], "yes", "{taken}");
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let f = k.get(&src).await;
    assert_eq!(f["shape"], "steps", "{f}");
    assert_eq!(f["parts"][1]["part"], "src/bank");
    assert_eq!(f["parts"][1]["folder"], true);
    assert_eq!(f["keeps"][0]["by"], "src/bank");
    assert_eq!(f["stale"], false);
    assert!(f["waiting"].is_null(), "{f}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_change_reads_plainly_with_what_tools_made_and_what_moved() {
    let mut k = Kelve::new().await;
    let root = folder(&[
        ("src/wait.rs", "pub fn wait() {}\n\npub fn longer() {}\n"),
        ("src/retry.rs", "pub fn retry() {}\n"),
        ("Cargo.lock", "# lock\n"),
    ]);
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "Start"]);
    std::fs::rename(root.join("src/wait.rs"), root.join("src/backoff.rs")).unwrap();
    std::fs::write(
        root.join("src/retry.rs"),
        "pub fn retry() {}\npub fn twice() {}\n",
    )
    .unwrap();
    std::fs::write(root.join("Cargo.lock"), "# lock, again\n").unwrap();
    std::fs::create_dir_all(root.join("looks")).unwrap();
    std::fs::write(root.join("looks/a.png"), [0x89, b'P', b'N', b'G', 0, 1, 2]).unwrap();
    let (id, link) = k
        .open(json!({ "path": root.display().to_string(), "change": "uncommitted" }))
        .await;
    let (_, _, cookie) = k.land(&link).await;
    k.cookie = Some(cookie.expect("a session cookie"));
    k.get(&format!("/api/opened/{id}/change")).await;
    find_way(&k).await;
    let c = k.get(&format!("/api/opened/{id}/change")).await;
    let moved: Vec<&Value> = c["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["movedFrom"] == "src/wait.rs")
        .collect();
    assert_eq!(moved.len(), 1, "{c}");
    assert_eq!(moved[0]["path"], "src/backoff.rs");
    // Its files, then each folder holding a file it reads, the top too, as
    // part of reading it: not what only tools changed. Two files and two
    // folders to read, then its own page.
    let reading = &c["reading"];
    assert_eq!(
        (reading["read"].clone(), reading["of"].clone()),
        (json!(1), json!(5)),
        "{c}"
    );
    let mut given = Value::Null;
    let mut folders = Vec::new();
    let job = loop {
        let job = take_next(&k, &mut given).await;
        assert!(!job.is_null(), "no change's plainly came");
        let answer = match job["kind"].as_str() {
            Some("read") => none_marked(&job),
            Some("folders") => json!({ "folders": [] }),
            Some("way") => stand_in_way(&job),
            Some("path") => json!({ "parts": [] }),
            Some("plainly") => {
                assert!(job["material"]["change"].is_object(), "{job}");
                json!({ "what": "Waits, and tries.", "parts": [
                    { "from": 1, "to": job["material"]["lineCount"], "goal": "Does its part", "says": "What it does." }
                ] })
            }
            Some("plainlyFolder") => {
                folders.push(job["material"]["path"].as_str().unwrap().to_string());
                json!({ "what": "Where the retries are.", "shape": "parts", "parts": [
                    { "part": job["material"]["parts"][0]["path"], "goal": "Holds the code", "says": "All of it." }
                ] })
            }
            Some("plainlyChange") => break job,
            other => panic!("a {other:?} job: {job}"),
        };
        let said = hand_in(&k, &job, answer).await;
        assert_eq!(said["taken"], "yes", "{said}");
        given = said["next"].clone();
    };
    // Bottom up: a folder after the one in it, the change's own page last.
    assert_eq!(folders, ["src", ""]);
    // Its own page counts as read from its overview, written first, while
    // it's brought up to date.
    let c = k.get(&format!("/api/opened/{id}/change")).await;
    assert_eq!(
        (c["reading"]["read"].clone(), c["reading"]["of"].clone()),
        (json!(5), json!(5)),
        "{c}"
    );
    assert_eq!(job["version"], "plainlyChange@1");
    let tools: Vec<&str> = job["material"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    assert_eq!(tools, ["Cargo.lock", "looks/a.png"]);
    let part = |mark: &str, file: &str| json!({ "mark": mark, "goal": "Does a part", "says": "It does.", "files": [file] });
    let wrong = hand_in(
        &k,
        &job,
        json!({ "what": "Retries try twice.", "kind": "adds", "parts": [part("moved", "src/retry.rs")] }),
    )
    .await;
    assert_eq!(wrong["taken"], "no", "{wrong}");
    assert!(
        wrong["why"].as_str().unwrap().contains("didn't move"),
        "{wrong}"
    );
    // It stays with the agent fixing it: the fixed answer goes to the same job.
    let taken = hand_in(
        &k,
        &job,
        json!({ "what": "Retries try twice, and the wait has a new name.", "kind": "adds",
                "parts": [part("changed", "src/retry.rs"), part("moved", "src/backoff.rs")],
                "touches": ["A call can take twice as long."] }),
    )
    .await;
    assert_eq!(taken["taken"], "yes", "{taken}");
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let p = k.get(&format!("/api/opened/{id}/plainly/change")).await;
    assert_eq!(p["kind"], "adds", "{p}");
    assert_eq!(
        p["tools"],
        json!(["Cargo.lock", "looks/a.png"]),
        "kelve adds what tools made"
    );
    assert_eq!(p["order"], json!(["src/retry.rs", "src/backoff.rs"]));
    assert!(p["waiting"].is_null(), "{p}");
    // A file of it says it moved, and what kind of change it's in.
    let f = k
        .get(&format!("/api/opened/{id}/plainly?path=src/backoff.rs"))
        .await;
    assert_eq!(f["change"]["movedFrom"], "src/wait.rs", "{f}");
    assert_eq!(f["change"]["kind"], "adds");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_folder_in_a_change_waits_only_for_the_files_the_change_reads() {
    let mut k = Kelve::new().await;
    let root = folder(&[
        ("src/retry.rs", "pub fn retry() {}\n"),
        ("src/ledger.rs", "pub fn reserve() {}\n"),
    ]);
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "Start"]);
    std::fs::write(
        root.join("src/retry.rs"),
        "pub fn retry() {}\npub fn twice() {}\n",
    )
    .unwrap();
    let (id, link) = k
        .open(json!({ "path": root.display().to_string(), "change": "uncommitted" }))
        .await;
    let (_, _, cookie) = k.land(&link).await;
    k.cookie = Some(cookie.expect("a session cookie"));
    k.get(&format!("/api/opened/{id}/change")).await;
    let src = format!("/api/opened/{id}/plainly/folder?path=src");
    let f = k.get(&src).await;
    assert!(f["what"].is_null(), "{f}");
    // The change reads only its own file; the folder comes once that's
    // read plainly, without waiting for the file the change left alone.
    let mut given = Value::Null;
    let mut placed = false;
    let job = loop {
        let job = take_next(&k, &mut given).await;
        assert!(!job.is_null(), "the folder's plainly didn't come");
        let answer = match job["kind"].as_str() {
            Some("read") => none_marked(&job),
            Some("folders") => json!({ "folders": [] }),
            Some("way") => stand_in_way(&job),
            Some("path") => json!({ "parts": [] }),
            Some("plainly") => {
                assert_eq!(job["material"]["path"], "src/retry.rs", "{job}");
                json!({ "what": "Tries again.", "parts": [
                    { "from": 1, "to": job["material"]["lineCount"], "goal": "Tries again", "says": "What it does." }
                ] })
            }
            Some("plainlyChange") => json!({ "what": "Retries try twice.", "kind": "adds",
                "parts": [{ "mark": "changed", "goal": "Tries twice", "says": "It does.", "files": ["src/retry.rs"] }] }),
            Some("plainlyFolder") if job["material"]["path"] == "src" => break job,
            Some("plainlyFolder") => json!({ "what": "The ledger.", "shape": "parts", "parts": [
                { "part": "src", "goal": "Holds the code", "says": "All of it." }
            ] }),
            other => panic!("a {other:?} job: {job}"),
        };
        let said = hand_in(&k, &job, answer).await;
        assert_eq!(said["taken"], "yes", "{said}");
        given = said["next"].clone();
        // Its page says where it is, with its job to move it up.
        if !placed {
            let f = k.get(&src).await;
            if f["waiting"]["before"].is_number() {
                assert!(f["waiting"]["ask"].is_number(), "{f}");
                placed = true;
            }
        }
    };
    assert!(placed, "its page never said its place");
    let listed = job["material"]["listed"].as_str().unwrap();
    assert!(
        listed.contains("src/retry.rs (file), +1 −0 in the change: Tries again."),
        "{listed}"
    );
    assert!(
        listed.contains("src/ledger.rs (file)\n"),
        "the file it left is given as it is: {listed}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn a_note_for_the_agent_is_on_its_list_and_comes_to_it_as_a_job() {
    let mut k = Kelve::new().await;
    let root = folder(&[("src/ledger.rs", "pub fn reserve() {}\n")]);
    let id = signed_in(&mut k, &root).await;
    let (status, note, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({
                "place": "agent",
                "text": "# Make it settle too\n",
                "on": { "path": "src/ledger.rs", "start": 1, "end": 1 },
            })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{note}");
    // While it's being written, it isn't on the list yet.
    let list = k.get("/api/list").await;
    assert!(
        list["next"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["kind"] != "note"),
        "{list}"
    );
    // Left alone a while, it is.
    left_alone(&kelve_server::reader::own_notes(&k.app));
    // On the agent's list, as you left it, before anything else of kelve's.
    let list = k.get("/api/list").await;
    let first = &list["next"][0];
    assert_eq!(first["kind"], "note", "{list}");
    assert_eq!(first["title"], "Make it settle too");
    assert_eq!(first["sub"], "On src/ledger.rs 1");
    assert_eq!(list["working"], false, "nothing's called kelve yet");
    assert!(list["workingUntil"].is_null());
    // It comes to the agent as a job, with the note and its lines.
    let (status, job, _) = k
        .call(
            "POST",
            "/api/agent/next",
            Some(json!({ "agent": "claude-code" })),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{job}");
    assert_eq!(job["kind"], "note", "{job}");
    assert!(
        job["material"]["note"]
            .as_str()
            .unwrap()
            .contains("Make it settle too")
    );
    assert!(
        job["material"]["linesNow"]
            .as_str()
            .unwrap()
            .contains("pub fn reserve()")
    );
    let list = k.get("/api/list").await;
    assert_eq!(list["working"], true);
    // Until when it counts as working: ten minutes on, holding the note's job.
    let until = list["workingUntil"].as_i64().unwrap();
    assert!(
        until - chrono::Utc::now().timestamp_millis() > 9 * 60 * 1000,
        "{list}"
    );
    assert_eq!(list["now"][0]["kind"], "note", "{list}");
    // Handed in with what it did, the note is done, and ready for you.
    let (_, taken, _) = k
        .call(
            "POST",
            "/api/agent/answer",
            Some(json!({ "job": job["job"], "answer": { "did": "Added `settle`." }, "agent": "claude-code" })),
            true,
        )
        .await;
    assert_eq!(taken["taken"], "yes", "{taken}");
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let done = k
        .get(&format!(
            "/api/opened/{id}/note?note=agent/make-it-settle-too.md"
        ))
        .await;
    assert_eq!(done["state"], "done");
    assert_eq!(done["did"], "Added `settle`.");
    let list = k.get("/api/list").await;
    assert!(
        list["next"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["kind"] != "note"),
        "{list}"
    );
    let ready = list["ready"].as_array().unwrap();
    assert!(
        ready
            .iter()
            .any(|i| i["kind"] == "note" && i["sub"] == "Added `settle`."),
        "{list}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn the_stream_says_the_page_first_so_a_tab_on_another_build_reloads() {
    let mut k = Kelve::new().await;
    let root = folder(&[("a.rs", "fn a() {}\n")]);
    signed_in(&mut k, &root).await;
    let request = Request::builder()
        .uri("/api/stream")
        .header(header::HOST, "localhost:4600")
        .header("sec-fetch-site", "same-origin")
        .header(header::COOKIE, k.cookie.clone().unwrap())
        .body(Body::empty())
        .unwrap();
    let response = k.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let mut said = Vec::new();
    for _ in 0..1 {
        let frame = body.frame().await.unwrap().unwrap();
        let text = String::from_utf8(frame.into_data().unwrap().to_vec()).unwrap();
        said.push(
            serde_json::from_str::<Value>(text.trim().trim_start_matches("data:").trim()).unwrap(),
        );
    }
    // The page's build, which a tab on another one reloads for.
    assert_eq!(said[0]["type"], "page", "{said:?}");
    assert_eq!(said[0]["build"], kelve_server::web::build(), "{said:?}");
    k.app.stop();
}

#[tokio::test]
async fn opening_opens_a_new_tab_even_with_one_open_so_the_browser_comes_forward() {
    let mut k = Kelve::new().await;
    let root = folder(&[("src/ledger.rs", "pub fn reserve() {}\n")]);
    signed_in(&mut k, &root).await;
    let links = k.links.lock().expect("links").len();
    // A kelve tab is open, on another agent's work, say: it stays where it is.
    let _tab = k.app.events.subscribe();
    let file = root.join("src/ledger.rs").display().to_string();
    let (status, said, _) = k
        .call("POST", "/api/open", Some(json!({ "path": file })), true)
        .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    assert_eq!(said, json!({ "browser": true }));
    assert_eq!(k.links.lock().expect("links").len(), links + 1, "a new tab");
    std::fs::remove_dir_all(&root).unwrap();
}

/// Every note under a folder, as if last changed a minute ago.
fn left_alone(dir: &Path) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let path = e.path();
        if path.is_dir() {
            left_alone(&path);
        } else if path.extension().is_some_and(|x| x == "md") {
            let ago = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(ago)
                .unwrap();
        }
    }
}

#[tokio::test]
async fn reading_what_was_opened_before_is_paused_until_its_opened_again() {
    let mut k = Kelve::new().await;
    let a = folder(&[("a.md", "# A\n\nOne.\n"), ("b.md", "# B\n\nTwo.\n")]);
    let b = folder(&[("c.md", "# C\n\nThree.\n")]);
    let open = |root: &Path, title: &str| json!({ "what": "folder", "agent": "codex", "path": root.display().to_string(), "title": title });
    let (status, said, _) = k
        .call("POST", "/api/agent/open", Some(open(&a, "A")), true)
        .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    let link = k.links.lock().unwrap().last().cloned().unwrap();
    let (_, _, cookie) = k.land(&link).await;
    k.cookie = Some(cookie.expect("a session cookie"));
    let older = k.get("/api/opened").await[0]["id"].as_i64().unwrap();
    find_way_as(&k, "codex").await;
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    let (status, said, _) = k
        .call("POST", "/api/agent/open", Some(open(&b, "B")), true)
        .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    let latest = k.get("/api/opened").await[0]["id"].as_i64().unwrap();
    find_way_as(&k, "codex").await;
    while kelve_server::pipeline::run_once(&k.app)
        .await
        .unwrap()
        .is_some()
    {}
    // The folder opened before is paused, with its files still to read.
    let list = k.get("/api/list").await;
    assert_eq!(list["open"], latest, "{list}");
    let paused = list["paused"].as_array().unwrap();
    assert_eq!(paused.len(), 1, "{list}");
    assert_eq!(paused[0]["opened"], older);
    assert_eq!(paused[0]["kind"], "folder");
    assert_eq!(paused[0]["title"], "A");
    assert!(paused[0]["left"].as_u64().unwrap() >= 2, "{list}");
    assert!(
        list["next"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["opened"] == latest),
        "{list}"
    );
    // The agent is given only what's open's reading.
    async fn next(k: &Kelve) -> Value {
        let (_, job, _) = k
            .call(
                "POST",
                "/api/agent/next",
                Some(json!({ "agent": "codex" })),
                true,
            )
            .await;
        job
    }
    let mut abouts = Vec::new();
    loop {
        let job = next(&k).await;
        if job.is_null() {
            break;
        }
        abouts.push(job["about"].as_str().unwrap_or_default().to_string());
    }
    assert!(abouts.iter().any(|a| a.contains("c.md")), "{abouts:?}");
    assert!(
        !abouts
            .iter()
            .any(|a| a.contains("a.md") || a.contains("b.md")),
        "{abouts:?}"
    );
    // Opened again from the list, it's what's open, and its reading carries on.
    let (status, again, _) = k
        .call("POST", &format!("/api/opened/{older}/again"), None, false)
        .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    let list = k.get("/api/list").await;
    assert_eq!(list["open"], older, "{list}");
    assert!(
        list["paused"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["opened"] != older),
        "{list}"
    );
    let job = next(&k).await;
    let about = job["about"].as_str().unwrap_or_default();
    assert!(about.contains("a.md") || about.contains("b.md"), "{job}");
    std::fs::remove_dir_all(&a).unwrap();
    std::fs::remove_dir_all(&b).unwrap();
}

#[tokio::test]
async fn a_folder_with_data_kelve_doesnt_read_is_read_once_the_rest_is() {
    let mut k = Kelve::new().await;
    // Over 64 KB of JSON is data a tool wrote: kelve doesn't read it, so
    // nothing is asked of it, and it counts as read.
    let data: String = (0..4000)
        .map(|i| {
            format!(
                "{{ \"from\": \"idea-{i}\", \"to\": \"idea-{}\" }},\n",
                i + 1
            )
        })
        .collect();
    let data = format!("[\n{data}{{}}\n]\n");
    assert!(data.len() > 64 * 1024);
    let root = folder(&[
        ("README.md", "# graph\n"),
        ("gold/entries.json", "[{ \"name\": \"Retry\" }]\n"),
        ("gold/relations.json", &data),
        ("Cargo.lock", "# made by cargo\n"),
    ]);
    let id = signed_in(&mut k, &root).await;
    let at = format!("/api/opened/{id}/folder");
    k.get(&at).await;
    find_way(&k).await;
    // The two files on the way in, neither the lockfile nor the data, then
    // two folders' pages.
    let r = &k.get(&at).await["reading"];
    assert_eq!((r["read"].clone(), r["of"].clone()), (json!(1), json!(4)));
    let mut given = Value::Null;
    loop {
        let job = take_next(&k, &mut given).await;
        if job.is_null() {
            break;
        }
        assert_ne!(job["material"]["path"], "gold/relations.json", "{job}");
        let answer = match job["kind"].as_str() {
            Some("read") => none_marked(&job),
            Some("folders") => json!({ "folders": [] }),
            Some("way") => stand_in_way(&job),
            Some("path") => json!({ "parts": [] }),
            Some("builtOn") => json!({ "chosen": [] }),
            Some("plainly") => json!({ "what": "A part of the graph.", "parts": [
                { "from": 1, "to": job["material"]["lineCount"], "goal": "Does its part", "says": "What it does." }
            ] }),
            Some("plainlyFolder") => json!({ "what": "A folder.", "shape": "parts", "parts": [
                { "part": job["material"]["parts"][0]["path"], "goal": "Holds it", "says": "All of it." }
            ] }),
            other => panic!("a {other:?} job: {job}"),
        };
        let said = hand_in(&k, &job, answer).await;
        assert_eq!(said["taken"], "yes", "{said}");
        given = said["next"].clone();
    }
    let view = k.get(&at).await;
    let r = &view["reading"];
    assert_eq!(
        (r["read"].clone(), r["of"].clone(), r["marking"].clone()),
        (json!(4), json!(4), json!("done")),
        "{view}"
    );
    // The data isn't on the way in, so it's never asked of the agent.
    assert!(
        view["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["path"] == "gold/relations.json"),
        "still in Files: {view}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn what_the_person_takes_off_their_agents_list_stays_off() {
    let mut k = Kelve::new().await;
    let a = folder(&[("a.md", "# A\n\nOne.\n"), ("b.md", "# B\n\nTwo.\n")]);
    let b = folder(&[("c.md", "# C\n\nThree.\n")]);
    let open = |root: &Path| json!({ "what": "folder", "agent": "codex", "path": root.display().to_string() });
    let run = |k: &Kelve| {
        let app = k.app.clone();
        async move {
            while kelve_server::pipeline::run_once(&app)
                .await
                .unwrap()
                .is_some()
            {}
        }
    };
    async fn next(k: &Kelve) -> Value {
        let (_, job, _) = k
            .call(
                "POST",
                "/api/agent/next",
                Some(json!({ "agent": "codex" })),
                true,
            )
            .await;
        job
    }
    let (status, said, _) = k
        .call("POST", "/api/agent/open", Some(open(&a)), true)
        .await;
    assert_eq!(status, StatusCode::OK, "{said}");
    let link = k.links.lock().unwrap().last().cloned().unwrap();
    let (_, _, cookie) = k.land(&link).await;
    k.cookie = Some(cookie.expect("a session cookie"));
    let id = k.get("/api/opened").await[0]["id"].as_i64().unwrap();
    find_way_as(&k, "codex").await;
    run(&k).await;
    let list = k.get("/api/list").await;
    let row = list["next"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["key"] == format!("read:{id}"))
        .cloned()
        .unwrap_or_else(|| panic!("{list}"));
    // Only the person takes things off: not the command line.
    let (status, _, _) = k
        .call(
            "POST",
            "/api/list/off",
            Some(json!({ "asks": row["asks"] })),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // The person does: the row is gone, and the agent isn't given it.
    let (status, list, _) = k
        .call(
            "POST",
            "/api/list/off",
            Some(json!({ "asks": row["asks"] })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert!(list["next"].as_array().unwrap().is_empty(), "{list}");
    assert!(next(&k).await.is_null());
    // Opened again, and read again, it stays off.
    let (status, _, _) = k
        .call("POST", "/api/agent/open", Some(open(&a)), true)
        .await;
    assert_eq!(status, StatusCode::OK);
    run(&k).await;
    assert!(
        k.get("/api/list").await["next"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(next(&k).await.is_null());

    // A note's job: the note becomes the person's own, and the agent isn't told of it.
    let (status, note, _) = k
        .call(
            "POST",
            &format!("/api/opened/{id}/notes"),
            Some(json!({ "place": "agent", "text": "# Say it shorter\n" })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{note}");
    left_alone(&kelve_server::reader::own_notes(&k.app));
    let list = k.get("/api/list").await;
    let row = list["next"][0].clone();
    assert_eq!(row["kind"], "note", "{list}");
    let (status, list, _) = k
        .call(
            "POST",
            "/api/list/off",
            Some(json!({ "asks": row["asks"] })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert!(list["next"].as_array().unwrap().is_empty(), "{list}");
    let (_, notes, _) = k
        .call(
            "GET",
            &format!(
                "/api/agent/notes?path={}",
                urlencode(&a.display().to_string())
            ),
            None,
            true,
        )
        .await;
    assert_eq!(notes, json!([]), "the agent isn't told of it");
    let mine = k
        .get(&format!("/api/opened/{id}/note?note=you/say-it-shorter.md"))
        .await;
    assert!(
        mine["text"].as_str().unwrap().contains("Say it shorter"),
        "{mine}"
    );
    assert!(next(&k).await.is_null());

    // What's paused, taken off: opened again, its reading doesn't carry on.
    let (status, _, _) = k
        .call("POST", "/api/agent/open", Some(open(&b)), true)
        .await;
    assert_eq!(status, StatusCode::OK);
    let older = k.get("/api/opened").await[0]["id"].as_i64().unwrap();
    find_way_as(&k, "codex").await;
    run(&k).await;
    let (status, _, _) = k
        .call("POST", "/api/agent/open", Some(open(&a)), true)
        .await;
    assert_eq!(status, StatusCode::OK);
    run(&k).await;
    let list = k.get("/api/list").await;
    assert_eq!(list["paused"][0]["opened"], older, "{list}");
    let (status, list, _) = k
        .call(
            "POST",
            "/api/list/off",
            Some(json!({ "paused": older })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert!(list["paused"].as_array().unwrap().is_empty(), "{list}");
    let (status, _, _) = k
        .call("POST", &format!("/api/opened/{older}/again"), None, false)
        .await;
    assert_eq!(status, StatusCode::OK);
    run(&k).await;
    assert!(next(&k).await.is_null());
    std::fs::remove_dir_all(&a).unwrap();
    std::fs::remove_dir_all(&b).unwrap();
}

#[tokio::test]
async fn an_entry_seen_tells_the_reader_once_so_it_leaves_ready_for_you() {
    let config = kelve_server::config::test_config();
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate(&pool).await.unwrap();
    let app = kelve_server::state::App::new(config, pool).await.unwrap();
    {
        let mut conn = app.db.acquire().await.unwrap();
        let opened = kelve_store::reader::open(&mut conn, "folder", "/p/dice", "", "", "dice")
            .await
            .unwrap();
        kelve_store::reader::ask(&mut conn, opened, "entry", "k1")
            .await
            .unwrap();
    }
    let mut tab = app.events.subscribe();
    kelve_server::ready::seen_entry(&app, "k1").await.unwrap();
    assert!(matches!(
        tab.try_recv(),
        Ok(kelve_core::views::ServerEvent::Asks)
    ));
    // Seen already: nothing changed, so nothing to hear.
    kelve_server::ready::seen_entry(&app, "k1").await.unwrap();
    assert!(tab.try_recv().is_err());
}

#[tokio::test]
async fn opening_anything_with_files_has_the_agent_find_its_way_in_first() {
    let mut k = Kelve::new().await;
    let mut files: Vec<(String, String)> = vec![("README.md".into(), "# ledger\n".into())];
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
    let big = folder(&named);
    let small = folder(&[("README.md", "# notes\n"), ("a.md", "A.\n")]);
    for root in [&big, &small] {
        let body = json!({ "what": "folder", "agent": "claude-code", "path": root.display().to_string(), "title": "ledger" });
        let (status, said, _) = k.call("POST", "/api/agent/open", Some(body), true).await;
        assert_eq!(status, StatusCode::OK, "{said}");
    }
    let link = k.links.lock().unwrap().last().cloned().unwrap();
    let (_, _, cookie) = k.land(&link).await;
    k.cookie = Some(cookie.expect("a session cookie"));
    let opened = k.get("/api/opened").await;
    let id_of = |root: &Path| {
        opened
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["root"] == root.display().to_string())
            .unwrap()["id"]
            .as_i64()
            .unwrap()
    };
    // A lot of files, or a few: the same route, the way in found first.
    for root in [&big, &small] {
        let o = k.get(&format!("/api/opened/{}", id_of(root))).await;
        assert_eq!(o["way"]["finding"], true, "{o}");
        assert_eq!(o["way"]["order"], json!([]));
    }
    std::fs::remove_dir_all(&big).unwrap();
    std::fs::remove_dir_all(&small).unwrap();
}

#[tokio::test]
async fn after_the_way_in_every_other_folder_has_its_page_the_shallowest_first() {
    let mut k = Kelve::new().await;
    let mut files: Vec<(String, String)> = vec![
        ("README.md".into(), "# ledger\n".into()),
        ("src/main.rs".into(), "fn main() {}\n".into()),
        ("tools/check.sh".into(), "echo ok\n".into()),
    ];
    for n in 1..=12 {
        files.push((format!("docs/guide/g{n:02}.md"), format!("# Part {n}\n")));
    }
    for n in 1..=10 {
        files.push((
            format!("src/util/u{n:02}.rs"),
            format!("pub fn u{n:02}() {{}}\n"),
        ));
    }
    let named: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let root = folder(&named);
    let body = json!({
        "what": "folder", "agent": "claude-code", "path": root.display().to_string(), "title": "ledger",
    });
    // Its way in, as the agent finds it: kelve's first job.
    let way = json!({
        "order": [
            { "path": "README.md", "why": "What ledger is for." },
            { "path": "src/main.rs", "why": "Where it starts." },
        ],
        "parts": [],
        "folders": [],
    });
    let (status, opened, _) = k.call("POST", "/api/agent/open", Some(body), true).await;
    assert_eq!(status, StatusCode::OK, "{opened}");
    let link = k.links.lock().unwrap().last().cloned().unwrap();
    let (_, _, cookie) = k.land(&link).await;
    k.cookie = Some(cookie.expect("a session cookie"));
    let id = k.get("/api/opened").await[0]["id"].as_i64().unwrap();
    // Opening it asks for its reading.
    let _ = k.get(&format!("/api/opened/{id}/folder")).await;

    // The rest's pages are a line of their own on the list, after the way
    // in's reading.
    let mut given = Value::Null;
    let first = loop {
        let job = take_next(&k, &mut given).await;
        if job["kind"] == "read" {
            break job;
        }
        let answer = match job["kind"].as_str() {
            Some("way") => way.clone(),
            Some("path") => json!({ "parts": [] }),
            _ => json!({ "folders": [] }),
        };
        let said = hand_in(&k, &job, answer).await;
        assert_eq!(said["taken"], "yes", "{said}");
        given = said["next"].clone();
    };
    let list = k.get("/api/list").await;
    let lines: Vec<Value> = list["now"]
        .as_array()
        .unwrap()
        .iter()
        .chain(list["next"].as_array().unwrap())
        .cloned()
        .collect();
    let keys: Vec<&str> = lines.iter().filter_map(|l| l["key"].as_str()).collect();
    let read = format!("read:{id}");
    let rest = format!("rest:{id}");
    assert!(keys.contains(&read.as_str()), "{list}");
    let rest_line = lines
        .iter()
        .find(|l| l["key"] == rest)
        .unwrap_or_else(|| panic!("no line for the rest's pages: {list}"));
    assert_eq!(rest_line["title"], "Folder pages for the rest");
    assert_eq!(rest_line["left"], 3, "{rest_line}");
    assert_eq!(rest_line["of"], 3, "{rest_line}");
    let reading = lines.iter().find(|l| l["key"] == read).unwrap();
    assert_eq!(reading["title"], "Reading the way in", "{reading}");
    assert_eq!(reading["hand"][0]["stage"], "reading", "{reading}");

    // The way in, then the rest's pages, the shallowest first, each
    // written without waiting for the folders in it.
    given = first;
    let mut pages: Vec<String> = Vec::new();
    loop {
        let job = take_next(&k, &mut given).await;
        if job.is_null() {
            break;
        }
        let answer = match job["kind"].as_str() {
            Some("read") => none_marked(&job),
            Some("folders") => json!({ "folders": [] }),
            Some("way") => stand_in_way(&job),
            Some("path") => json!({ "parts": [] }),
            Some("plainly") => json!({ "what": "A part of the ledger.", "parts": [
                { "from": 1, "to": job["material"]["lineCount"], "goal": "Does its part", "says": "What it does." }
            ] }),
            Some("plainlyFolder") => {
                pages.push(job["material"]["path"].as_str().unwrap().to_string());
                assert!(
                    job["material"]["print"].is_null(),
                    "kelve's own print went to the agent: {job}"
                );
                json!({ "what": "A folder of the ledger.", "shape": "parts", "parts": [
                    { "part": job["material"]["parts"][0]["path"], "goal": "Holds a part", "says": "Some of it." }
                ] })
            }
            other => panic!("a {other:?} job: {job}"),
        };
        let said = hand_in(&k, &job, answer).await;
        assert_eq!(said["taken"], "yes", "{said}");
        given = said["next"].clone();
    }
    let rest_pages: Vec<&str> = pages
        .iter()
        .map(String::as_str)
        .filter(|p| !["", "src"].contains(p))
        .collect();
    assert_eq!(rest_pages, ["tools", "docs/guide", "src/util"], "{pages:?}");
    let at = |p: &str| pages.iter().position(|x| x == p).unwrap();
    assert!(
        at("src") < at("tools"),
        "the way in's pages first: {pages:?}"
    );
    // Only the way in was read: no file of the rest was.
    let r = k.get(&format!("/api/opened/{id}/folder")).await;
    assert_eq!(r["reading"]["pages"], 2, "{r}");
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn an_agent_says_its_last_job_and_is_given_none_with_it() {
    let mut k = Kelve::new().await;
    let root = folder(&[
        ("src/a.rs", "pub fn a() {}\n"),
        ("src/b.rs", "pub fn b() {}\n"),
    ]);
    let id = signed_in(&mut k, &root).await;
    let _ = k.get(&format!("/api/opened/{id}/folder")).await;
    let job = job_of(&k, "read").await;
    let (_, said, _) = k
        .call(
            "POST",
            "/api/agent/answer",
            Some(json!({ "job": job["job"], "agent": "claude-code", "answer": none_marked(&job), "more": false })),
            true,
        )
        .await;
    assert_eq!(said["taken"], "yes", "{said}");
    assert!(said["next"].is_null(), "a next job came: {said}");
    assert!(said["left"].as_i64().unwrap() >= 1, "{said}");
    // Nothing was taken for it: the next is still waiting.
    let list = k.get("/api/list").await;
    assert!(
        list["now"]
            .as_array()
            .unwrap()
            .iter()
            .all(|l| l["hand"].as_array().unwrap().is_empty()),
        "{list}"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
