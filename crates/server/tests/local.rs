//! kelve's local service, end to end through its router, on a store of its
//! own: signing a browser in the way `kelve open` does, what the command
//! line's secret may and mayn't do, settings, and signing in to the shared
//! graph.

#![allow(clippy::expect_used, clippy::unwrap_used)]

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
    /// The sign-in links kelve gave its person.
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
            false
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
    ) -> (StatusCode, Vec<u8>, Option<String>) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost:4600")
            .header("sec-fetch-site", "same-origin");
        if as_agent {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", self.app.secret));
        }
        if let Some(cookie) = &self.cookie {
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
            .to_bytes()
            .to_vec();
        (status, bytes, cookie)
    }

    async fn json(&self, method: &str, path: &str, body: Option<Value>) -> Value {
        let (status, bytes, _) = self.call(method, path, body, false).await;
        assert!(
            status.is_success(),
            "{method} {path}: {status} {}",
            String::from_utf8_lossy(&bytes)
        );
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("JSON")
        }
    }

    /// Signs in, as `kelve open` does: kelve gives the link to the person,
    /// here the test, and only says whether a browser opened.
    async fn sign_in(&mut self) {
        let (status, bytes, _) = self.call("POST", "/api/signin-link", None, true).await;
        assert_eq!(status, StatusCode::OK);
        let said: Value = serde_json::from_slice(&bytes).expect("JSON");
        assert_eq!(
            said,
            json!({ "opened": false }),
            "the link never comes back"
        );
        let link = self
            .links
            .lock()
            .expect("links")
            .last()
            .cloned()
            .expect("a link");
        let path = link.trim_start_matches("http://localhost:4600").to_string();
        let (status, _, cookie) = self.call("GET", &path, None, false).await;
        assert!(status.is_redirection(), "signing in redirects: {status}");
        self.cookie = Some(cookie.expect("a session cookie"));
        // A link works once.
        let (status, _, _) = self.call("GET", &path, None, false).await;
        assert!(!status.is_success() || status.is_redirection());
    }
}

#[tokio::test]
async fn signing_in_to_the_shared_graph_keeps_its_key_sealed_until_signing_out() {
    use kelve_graph_api::stand_in::{StandIn, TOKEN};

    let graph = StandIn::start().await.unwrap();
    let mut config = test_config();
    config.endpoints.graph = graph.url.clone();
    let mut k = Kelve::with(config).await;
    k.sign_in().await;
    // Not agreed yet: signing in agrees.
    graph.unagree();
    let account = k.json("GET", "/api/graph/account", None).await;
    assert_eq!(
        account,
        json!({ "graph": true, "from": null, "handle": null, "waiting": false, "agreed": null })
    );
    assert!(k.app.graph_writer().is_none(), "nothing to write as yet");
    let cookie = k.cookie.take();
    let (status, _, _) = k.call("POST", "/api/graph/signin", None, true).await;
    k.cookie = cookie;
    assert!(
        status.is_client_error(),
        "only the person signs in: {status}"
    );

    let address = k.json("POST", "/api/graph/signin", None).await["address"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        k.json("GET", "/api/graph/account", None).await["waiting"],
        true
    );
    // The browser goes to the graph, which sends it back to kelve.
    let browser = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let back = browser.get(&address).send().await.unwrap();
    let back = back.headers()["location"].to_str().unwrap().to_string();
    assert!(back.starts_with("http://127.0.0.1:"), "{back}");
    let forged = back.replace("state=", "state=not-");
    let refused = browser.get(&forged).send().await.unwrap();
    assert_eq!(refused.status(), 400, "only the state kelve made");
    let page = browser.get(&back).send().await.unwrap();
    assert_eq!(page.status(), 200);
    let said = page.text().await.unwrap();
    assert!(
        said.contains("signed in to kelve’s shared graph as contributor-one"),
        "{said}"
    );
    assert!(!said.contains(TOKEN));

    let mut account = Value::Null;
    for _ in 0..50 {
        account = k.json("GET", "/api/graph/account", None).await;
        if account["waiting"] == false {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(
        account,
        json!({ "graph": true, "from": "app", "handle": "contributor-one", "waiting": false, "agreed": true })
    );
    assert_eq!(
        graph.agreed().as_deref(),
        Some(kelve_core::terms::VERSION),
        "signing in agreed"
    );
    // The terms, and agreeing to them, go through to the graph.
    let terms = k.json("GET", "/api/graph/terms", None).await;
    assert_eq!(terms["version"], kelve_core::terms::VERSION);
    graph.unagree();
    assert_eq!(
        k.json("GET", "/api/graph/account", None).await["agreed"],
        false
    );
    let (status, said, _) = k
        .call(
            "POST",
            "/api/graph/agree",
            Some(json!({ "version": "1999-01-01" })),
            false,
        )
        .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "{}",
        String::from_utf8_lossy(&said)
    );
    let agreed = k
        .json(
            "POST",
            "/api/graph/agree",
            Some(json!({ "version": kelve_core::terms::VERSION })),
        )
        .await;
    assert_eq!(agreed["agreed"], true);
    assert_eq!(graph.agreed().as_deref(), Some(kelve_core::terms::VERSION));
    assert!(k.app.graph_writer().unwrap().can_write());
    let mut conn = k.app.db.acquire().await.unwrap();
    let kept = kelve_store::settings::secret(&mut conn, "keys.graph")
        .await
        .unwrap()
        .unwrap();
    drop(conn);
    assert!(
        !String::from_utf8_lossy(&kept).contains(TOKEN),
        "kept sealed"
    );
    k.app.reload().await.unwrap();
    assert!(k.app.graph_writer().is_some(), "and opened again");

    let account = k.json("DELETE", "/api/graph/account", None).await;
    assert_eq!(account["from"], Value::Null);
    assert_eq!(graph.signed_out(), 1, "the graph forgets the key too");
    assert!(k.app.graph_writer().is_none());

    // Deleting the account: the graph is told, and the key forgotten here.
    let (status, _, _) = k
        .call("POST", "/api/graph/account/delete", None, false)
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "no account to delete");
    k.app
        .hold_graph_key(Some((TOKEN, "contributor-one")))
        .await
        .unwrap();
    let account = k.json("POST", "/api/graph/account/delete", None).await;
    assert_eq!(account["from"], Value::Null);
    assert_eq!(graph.deleted(), 1);
    assert!(k.app.graph_writer().is_none());
}

#[tokio::test]
async fn a_browser_signs_in_once_and_the_secret_only_asks_for_links() {
    let mut k = Kelve::new().await;
    // Nobody signed in reads nothing.
    let (status, _, _) = k.call("GET", "/api/settings", None, false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // The command line's secret can't read settings either.
    let (status, _, _) = k.call("GET", "/api/settings", None, true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    k.sign_in().await;
    let me = k.json("GET", "/api/me", None).await;
    assert_eq!(me["agent"], json!(null));
    assert_eq!(me["agents"], json!([]));
    assert!(me["found"].is_array(), "the agents installed here");
    k.app.stop();
}

#[tokio::test]
async fn settings_change_and_deleting_everything_puts_them_back() {
    let mut k = Kelve::new().await;
    k.sign_in().await;
    let mut settings = k.json("GET", "/api/settings", None).await;
    // Notes kept only for you: kelve's own folder, until another's chosen.
    assert!(std::path::Path::new(settings["notesDir"].as_str().unwrap()).ends_with("notes"));
    assert_eq!(settings["notesChosen"], false);
    settings.as_object_mut().unwrap().remove("notesDir");
    assert_eq!(
        settings,
        json!({ "contribute": true, "shareCounts": false, "countsOff": null, "notesChosen": false, "agent": "claudeCode", "codeSize": "medium", "textSize": "medium", "highlighting": "full", "keepReadInRepo": true, "newNote": "you", "hints": [], "notesHook": false, "hooksElsewhere": [], "askToAdd": true })
    );
    let changed = k
        .json(
            "POST",
            "/api/settings",
            Some(json!({ "codeSize": "large" })),
        )
        .await;
    let mut changed = changed;
    changed.as_object_mut().unwrap().remove("notesDir");
    assert_eq!(
        changed,
        json!({ "contribute": true, "shareCounts": false, "countsOff": null, "notesChosen": false, "agent": "claudeCode", "codeSize": "large", "textSize": "medium", "highlighting": "full", "keepReadInRepo": true, "newNote": "you", "hints": [], "notesHook": false, "hooksElsewhere": [], "askToAdd": true })
    );
    let (status, bytes, _) = k
        .call(
            "POST",
            "/api/settings",
            Some(json!({ "notesDir": "/no/such/folder" })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&bytes).contains("folder"));
    // Deleting everything puts the settings back, and keeps the browser in.
    let (status, _, _) = k.call("DELETE", "/api/everything", None, false).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let settings = k.json("GET", "/api/settings", None).await;
    assert_eq!(settings["contribute"], true);
    k.app.stop();
}

#[tokio::test]
async fn the_weekly_count_goes_only_when_shared_and_only_once() {
    use kelve_graph_api::stand_in::StandIn;

    let graph = StandIn::start().await.unwrap();
    let mut config = test_config();
    config.endpoints.graph = graph.url.clone();
    // Reading the graph is for someone signed in.
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    let mut k = Kelve::with(config).await;
    k.sign_in().await;
    let week = kelve_server::counts::monday(k.app.config.today()) - chrono::Duration::days(7);
    let tuesday = (week + chrono::Duration::days(1)).to_string();
    {
        let mut conn = k.app.db.acquire().await.unwrap();
        for what in ["opened.change", "opened.change", "links.followed"] {
            kelve_store::counts::add(&mut conn, &tuesday, what)
                .await
                .unwrap();
        }
        for key in ["a", "b"] {
            kelve_store::counts::add_once(&mut conn, &tuesday, "links.followed", key)
                .await
                .unwrap();
        }
    }

    assert_eq!(kelve_server::counts::send_due(&k.app).await.unwrap(), None);
    assert!(
        graph.counts().is_empty(),
        "nothing goes until the person says"
    );

    k.json(
        "POST",
        "/api/settings",
        Some(json!({ "shareCounts": true })),
    )
    .await;
    let sent = kelve_server::counts::send_due(&k.app).await.unwrap();
    assert_eq!(sent, Some(week.to_string()));
    let got = graph.counts();
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].days, got[0].deep_days), (1, 1));
    assert_eq!(got[0].counts["opened.change"], 2);
    assert_eq!(got[0].counts["links.followed"], 3);
    assert_eq!(
        got[0].by_day[&tuesday]["opened.change"], 2,
        "a day at a time too"
    );
    assert_eq!(got[0].by_day.len(), 1, "only days with use");
    assert!(got[0].agent.starts_with("kelve/"));
    assert!(!got[0].version.is_empty());
    let install = got[0].install.clone();
    assert_eq!(install.len(), 32, "the install's random ID, made on yes");
    let settings = k.json("GET", "/api/settings", None).await;
    assert!(
        !settings.to_string().contains(&install),
        "never shown beside anything else"
    );

    assert_eq!(
        kelve_server::counts::send_due(&k.app).await.unwrap(),
        None,
        "a week goes once"
    );
    assert_eq!(graph.counts().len(), 1);
    let shown = k.json("GET", "/api/counts", None).await;
    assert_eq!(shown["sent"][0]["week"], week.to_string());
    let as_sent: kelve_graph_api::Counts =
        serde_json::from_str(shown["sent"][0]["report"].as_str().unwrap()).unwrap();
    assert_eq!(as_sent, got[0], "exactly what went");

    // Stopping deletes the ID; saying yes again makes a new one.
    k.json(
        "POST",
        "/api/settings",
        Some(json!({ "shareCounts": false })),
    )
    .await;
    assert_eq!(k.app.settings().install, None);
    k.json(
        "POST",
        "/api/settings",
        Some(json!({ "shareCounts": true })),
    )
    .await;
    assert!(
        k.app
            .settings()
            .install
            .as_deref()
            .is_some_and(|i| i != install)
    );
    k.app.stop();
}

#[tokio::test]
async fn do_not_track_keeps_counts_from_going_whatever_was_chosen() {
    use kelve_graph_api::stand_in::StandIn;

    let graph = StandIn::start().await.unwrap();
    let mut config = test_config();
    config.endpoints.graph = graph.url.clone();
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    config.counts_off = Some("DO_NOT_TRACK".into());
    let mut k = Kelve::with(config).await;
    k.sign_in().await;
    let week = kelve_server::counts::monday(k.app.config.today()) - chrono::Duration::days(7);
    {
        let mut conn = k.app.db.acquire().await.unwrap();
        kelve_store::counts::add(&mut conn, &week.to_string(), "opened.change")
            .await
            .unwrap();
    }
    let settings = k
        .json(
            "POST",
            "/api/settings",
            Some(json!({ "shareCounts": true })),
        )
        .await;
    assert_eq!(settings["countsOff"], "DO_NOT_TRACK", "the line says so");
    assert_eq!(kelve_server::counts::send_due(&k.app).await.unwrap(), None);
    assert!(graph.counts().is_empty());
    k.app.stop();
}

#[tokio::test]
async fn a_wrong_entry_is_flagged_to_the_graph_with_nothing_else() {
    use kelve_graph_api::stand_in::StandIn;

    let graph = StandIn::start().await.unwrap();
    let mut config = test_config();
    config.endpoints.graph = graph.url.clone();
    // Reading the graph is for someone signed in.
    config.env.graph = Some(kelve_graph_api::stand_in::TOKEN.into());
    let mut k = Kelve::with(config).await;
    k.sign_in().await;
    let entry = json!({
        "idea": { "id": "kelve:i0000000000aa", "name": "Advisory locks" },
        "title": "What advisory locks are for", "minutes": 2, "madeAt": "2026-10-15T00:00:00.000Z",
        "blocks": [], "sources": [], "links": []
    });
    {
        let mut conn = k.app.db.acquire().await.unwrap();
        for (key, revision) in [("kelve:i0000000000aa", Some("r7")), ("new:mine", None)] {
            kelve_store::entries::keep_idea(
                &mut conn,
                &kelve_store::entries::Idea {
                    key: key.into(),
                    name: "Advisory locks".into(),
                    definition: "A lock a program names, and the database keeps.".into(),
                    kind: "concept".into(),
                    became: None,
                },
            )
            .await
            .unwrap();
            if let Some(revision) = revision {
                kelve_store::entries::taken(&mut conn, key, &entry.to_string(), revision)
                    .await
                    .unwrap();
            }
        }
    }
    let page = k.json("GET", "/api/ideas/kelve:i0000000000aa", None).await;
    assert_eq!(page["canFlag"], true);
    k.json(
        "POST",
        "/api/ideas/kelve:i0000000000aa/flag",
        Some(json!({
            "why": "Postgres never enforces them; ghp_0123456789abcdefghijABCDEFGHIJ012345",
            "link": "Locks"
        })),
    )
    .await;
    let flags = graph.flags();
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].revision, "r7");
    assert_eq!(flags[0].link.as_deref(), Some("Locks"));
    assert!(flags[0].why.starts_with("Postgres never enforces them"));
    assert!(!flags[0].why.contains("ghp_"), "secrets are stripped first");

    let (status, said, _) = k
        .call(
            "POST",
            "/api/ideas/kelve:i0000000000aa/flag",
            Some(json!({ "why": "no" })),
            false,
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(String::from_utf8_lossy(&said).contains("Say what’s wrong"));
    assert_eq!(
        k.json("GET", "/api/ideas/new:mine", None).await["canFlag"],
        false,
        "an entry the graph hasn't got has no one to tell"
    );
    k.app.stop();
}
