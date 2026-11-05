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
        Some(format!("/#/o/{id}?shown").as_str()),
        "it lands on what was opened, marked as kelve showing it, so it takes the place of a tab showing it already"
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
    assert_eq!(file["lang"], "rust");
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
    // Every file beside it, as a list.
    let every = k.get(&format!("/api/opened/{id}/folder")).await;
    assert_eq!(every["files"][0]["path"], "src/ledger.rs");
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
    let shown = k.get("/api/counts").await;
    let text = shown["thisWeek"].as_str().unwrap();
    let week: Value = serde_json::from_str(text).unwrap();
    assert_eq!(week["days"], 1);
    assert_eq!(week["counts"]["opened.folder"], 1);
    assert_eq!(week["counts"]["notes.work"], 1);
    for private in ["ledger", "Spending", root.to_str().unwrap()] {
        assert!(!text.contains(private), "{private} never goes in a count");
    }
    assert_eq!(shown["sent"], json!([]));
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
    run(&k).await;

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

    std::fs::remove_dir_all(&a).unwrap();
    std::fs::remove_dir_all(&b).unwrap();
}
