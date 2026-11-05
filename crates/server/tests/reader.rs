//! The reader's routes, end to end through kelve's router, on a store and
//! folders of their own: opening from the command line, a folder and its
//! files, and a change. No model here.

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
    std::fs::create_dir_all(root.join(".kelve/skills/rust")).unwrap();
    std::fs::write(root.join(".kelve/way.json"), "{}\n").unwrap();
    std::fs::write(root.join(".kelve/skills/rust/SKILL.md"), "# Rust\n").unwrap();
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
async fn the_hook_goes_into_claude_codes_settings_only_when_asked_and_comes_out_alone() {
    let mut k = Kelve::new().await;
    let root = folder(&[("a.md", "# A\n")]);
    let _ = signed_in(&mut k, &root).await;
    let file = k.app.config.claude_dir.join("settings.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, r#"{ "model": "opus" }"#).unwrap();
    let before = k.get("/api/settings").await;
    assert_eq!(before["hook"], false);
    let (status, after, _) = k
        .call(
            "POST",
            "/api/settings",
            Some(json!({ "hook": true })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{after}");
    assert_eq!(after["hook"], true);
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
    assert_eq!(codex["hook"], false, "{codex}");
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
            Some(json!({ "hook": false })),
            false,
        )
        .await;
    assert_eq!(after["hook"], false);
    let left: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(left, json!({ "model": "opus" }));
    std::fs::remove_dir_all(&root).unwrap();
}

#[tokio::test]
async fn kelve_counts_how_it_was_used_in_numbers_and_never_what() {
    let mut k = Kelve::new().await;
    let root = folder(&[("src/ledger.rs", "pub fn reserve() {}\n")]);
    signed_in(&mut k, &root).await;
    let shown = k.get("/api/counts").await;
    let text = shown["thisWeek"].as_str().unwrap();
    let week: Value = serde_json::from_str(text).unwrap();
    assert_eq!(week["days"], 1);
    assert_eq!(week["counts"]["opened.folder"], 1);
    for private in ["ledger", root.to_str().unwrap()] {
        assert!(!text.contains(private), "{private} never goes in a count");
    }
    assert_eq!(shown["sent"], json!([]));
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
