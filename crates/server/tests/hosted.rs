//! kelve.ai from the app, through kelve's router, against the stand-in
//! service: signing in, a page's address, a verified run from its offer to
//! its score, publishing it, the provider's key kept sealed, and deleting
//! the account.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use kelve_graph_api::stand_in::{StandIn, TOKEN};
use kelve_server::config::test_config;
use kelve_server::state::{App, Shared};
use serde_json::{Value, json};
use tower::ServiceExt;

struct Kelve {
    app: Shared,
    router: Router,
    cookie: Option<String>,
    links: Arc<Mutex<Vec<String>>>,
    graph: StandIn,
}

impl Kelve {
    /// A kelve beside the stand-in service, its person's skills in a
    /// folder of their own: one with a secret and a path in it, and the
    /// one the stand-in's runs come out better with.
    async fn new() -> Kelve {
        let graph = StandIn::start().await.unwrap();
        let mut config = test_config();
        config.endpoints.graph = graph.url.clone();
        let skills = &config.skills_dir;
        std::fs::create_dir_all(skills.join("rust/references")).unwrap();
        std::fs::create_dir_all(skills.join("standin-expert")).unwrap();
        std::fs::write(
            skills.join("rust/SKILL.md"),
            "---\nname: rust\ndescription: How you like Rust.\n---\n# Rust\n\nNotes in /Users/sam/acme/notes.md\n",
        )
        .unwrap();
        std::fs::write(
            skills.join("rust/references/errors.md"),
            "token ghp_abcdefghijklmnopqrstuvwxyz0123456789\n",
        )
        .unwrap();
        std::fs::write(
            skills.join("standin-expert/SKILL.md"),
            "---\nname: standin-expert\ndescription: Does it well.\n---\n",
        )
        .unwrap();
        let pool = kelve_store::open(&config.store).await.unwrap();
        kelve_store::migrate(&pool).await.unwrap();
        let app = App::new(config, pool).await.unwrap();
        let links: Arc<Mutex<Vec<String>>> = Arc::default();
        let keep = links.clone();
        app.set_hand_off(Arc::new(move |link: &str| {
            keep.lock().unwrap().push(link.to_string());
            false
        }));
        let router = kelve_server::router(app.clone());
        let mut k = Kelve {
            app,
            router,
            cookie: None,
            links,
            graph,
        };
        k.pair().await;
        k
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
        // The command line has no cookie: only its secret.
        if let Some(cookie) = self.cookie.as_ref().filter(|_| !as_agent) {
            request = request.header(header::COOKIE, cookie);
        }
        let request = match body {
            Some(b) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
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
            .unwrap()
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
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    /// What kelve said no with: its status and its sentence, and the rest.
    async fn refused(&self, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let (status, bytes, _) = self.call(method, path, body, false).await;
        assert!(!status.is_success(), "{method} {path} was taken");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Pairs this browser with kelve, as `kelve open` does.
    async fn pair(&mut self) {
        let (status, _, _) = self.call("POST", "/api/signin-link", None, true).await;
        assert_eq!(status, StatusCode::OK);
        let link = self.links.lock().unwrap().last().cloned().unwrap();
        let path = link.trim_start_matches("http://localhost:4600").to_string();
        let (_, _, cookie) = self.call("GET", &path, None, false).await;
        self.cookie = Some(cookie.unwrap());
    }

    /// Signs in to kelve.ai, as the person's browser does: to the stand-in,
    /// which sends it straight back to kelve. What the tab said.
    async fn sign_in(&self) -> String {
        let address = self
            .json(
                "POST",
                "/api/hosted/signin",
                Some(json!({ "provider": "github" })),
            )
            .await["address"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(address.contains("/v2/signin"), "{address}");
        assert_eq!(self.json("GET", "/api/hosted", None).await["waiting"], true);
        let browser = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let back = browser.get(&address).send().await.unwrap();
        let back = back.headers()["location"].to_str().unwrap().to_string();
        assert!(back.starts_with("http://127.0.0.1:"), "{back}");
        let forged = back.replace("state=", "state=not-");
        assert_eq!(browser.get(&forged).send().await.unwrap().status(), 400);
        let page = browser.get(&back).send().await.unwrap();
        assert_eq!(page.status(), 200);
        let said = page.text().await.unwrap();
        for _ in 0..50 {
            if self.json("GET", "/api/hosted", None).await["waiting"] == false {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        said
    }

    async fn sealed(&self, name: &str) -> Option<Vec<u8>> {
        let mut conn = self.app.db.acquire().await.unwrap();
        kelve_store::settings::secret(&mut conn, name)
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn signing_in_choosing_an_address_and_a_run_to_a_published_score() {
    let k = Kelve::new().await;
    let out = k.json("GET", "/api/hosted", None).await;
    assert_eq!(
        (out["available"].clone(), out["signedIn"].clone()),
        (json!(true), json!(false))
    );
    let (status, _) = k.refused("GET", "/api/hosted/offer", None).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "signed out, nothing to offer"
    );
    // Only the person signs in: never the command line.
    let (status, _, _) = k.call("POST", "/api/hosted/signin", None, true).await;
    assert!(status.is_client_error());

    let said = k.sign_in().await;
    assert!(said.contains("You’re signed in"), "{said}");
    assert!(said.contains("<svg"), "the agent is on the page");
    assert!(!said.contains(TOKEN));
    let account = k.json("GET", "/api/hosted", None).await;
    assert_eq!(account["signedIn"], true);
    assert_eq!(account["handle"], Value::Null);
    assert_eq!(account["runs"]["left"], 1);
    let kept = k.sealed("keys.hosted").await.unwrap();
    assert!(
        !String::from_utf8_lossy(&kept).contains(TOKEN),
        "kept sealed"
    );

    // The address: the rules first, then kelve.ai, with others offered.
    let c = k
        .json(
            "POST",
            "/api/hosted/handle/check",
            Some(json!({ "handle": "-x" })),
        )
        .await;
    assert_eq!(c["available"], false);
    assert_eq!(c["why"], "Start and end with a letter or a number.");
    let c = k
        .json(
            "POST",
            "/api/hosted/handle/check",
            Some(json!({ "handle": "alex" })),
        )
        .await;
    assert_eq!(c["why"], "Someone has that one.");
    assert_eq!(c["instead"], json!(["alex-agent", "alexbuilds"]));
    let (status, said) = k
        .refused(
            "POST",
            "/api/hosted/handle",
            Some(json!({ "handle": "alex" })),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(said["error"], "Someone has that one.");
    let account = k
        .json(
            "POST",
            "/api/hosted/handle",
            Some(json!({ "handle": "@Sam" })),
        )
        .await;
    assert_eq!(account["handle"], "sam");
    assert_eq!(account["page"], "https://kelve.ai/@sam");
    assert_eq!(account["public"], false, "nothing is public yet");

    // What goes, shown before it goes: every file of every skill.
    let offer = k.json("GET", "/api/hosted/offer", None).await;
    assert_eq!(offer["kinds"][0]["kind"], "rust");
    let paths: Vec<&str> = offer["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        [
            "rust/SKILL.md",
            "rust/references/errors.md",
            "standin-expert/SKILL.md"
        ]
    );

    let run = json!({ "kind": "rust", "agent": "claude-code", "model": "claude-sonnet-5-6" });
    let e = k.json("POST", "/api/hosted/runs", Some(run)).await;
    let id = e["run"].as_str().unwrap().to_string();
    assert_eq!(e["dollars"], 60.0);
    assert_eq!(e["providerName"], "Anthropic");
    assert_eq!(e["kept"], json!([]));
    assert_eq!(e["agentRuns"], 36);
    // A service run locally takes a Claude subscription too.
    assert_eq!(e["accepts"], json!(["subscription", "apiKey"]));
    assert_eq!(e["skills"].as_array().unwrap().len(), 3);

    // The provider's key: asked for the first time, and turned away when
    // the provider won't take it, then kept, sealed, once a run starts.
    let confirm = format!("/api/hosted/runs/{id}/confirm");
    let (_, said) = k.refused("POST", &confirm, Some(json!({}))).await;
    assert_eq!(said["error"], "Paste your Anthropic key to start the run.");
    assert_eq!(said["key"], true);
    let (_, said) = k
        .refused(
            "POST",
            &confirm,
            Some(json!({ "key": "sk-ant-not-a-test" })),
        )
        .await;
    assert!(
        said["error"]
            .as_str()
            .unwrap()
            .contains("didn’t accept that key")
    );
    assert!(!said.to_string().contains("sk-ant-not"), "never said back");
    assert_eq!(
        k.sealed("keys.provider.anthropic").await,
        None,
        "only a key that worked"
    );
    let (_, said) = k
        .refused(
            "POST",
            &confirm,
            Some(json!({ "kind": "subscription", "key": "sk-ant-api03-x" })),
        )
        .await;
    assert!(
        said["error"]
            .as_str()
            .unwrap()
            .starts_with("That’s an API key"),
        "{said}"
    );
    let started = k
        .json("POST", &confirm, Some(json!({ "key": " sk-test-1 " })))
        .await;
    assert_eq!(started["state"], "queued");
    assert_eq!(started["squares"].as_array().unwrap().len(), 12);
    let sealed = k.sealed("keys.provider.anthropic").await.unwrap();
    assert!(!String::from_utf8_lossy(&sealed).contains("sk-test-1"));
    let account = k.json("GET", "/api/hosted", None).await;
    assert_eq!(
        account["keys"],
        json!([{ "provider": "anthropic", "name": "Anthropic", "kind": "apiKey" }])
    );
    assert_eq!(account["going"], id.as_str(), "a run to go back to");
    assert_eq!(account["runs"]["left"], 0);

    let now = k.json("GET", &format!("/api/hosted/runs/{id}"), None).await;
    assert_eq!(now["state"], "running");
    assert_eq!(now["squares"][0], "running");
    assert_eq!(now["kindName"], "Rust");
    assert_eq!(now["pausedUntil"], Value::Null);
    // Paused for the Claude plan's limit: the page says until when.
    let until = kelve_core::time::Timestamp(chrono::Utc::now() + chrono::Duration::hours(2));
    k.graph.hosted.pause(&id, Some(until));
    let paused = k.json("GET", &format!("/api/hosted/runs/{id}"), None).await;
    assert_eq!(paused["state"], "running");
    assert_eq!(paused["pausedUntil"], json!(until));
    assert_eq!(paused["squares"], now["squares"], "it waits");
    k.graph.hosted.finish(&id);
    let done = k.json("GET", &format!("/api/hosted/runs/{id}"), None).await;
    assert_eq!(done["state"], "finished");
    let score = &done["score"];
    assert_eq!(
        (score["better"].clone(), score["of"].clone()),
        (json!(10), json!(12))
    );
    assert_eq!(score["verified"], true);
    assert_eq!(score["shown"], false);
    assert_eq!(
        k.json("GET", "/api/hosted", None).await["going"],
        Value::Null
    );

    // Published: the page goes up, with the agent's character.
    let sid = score["id"].as_str().unwrap();
    let shown = k
        .json(
            "POST",
            &format!("/api/hosted/scores/{sid}"),
            Some(json!({ "shown": true })),
        )
        .await;
    assert_eq!(shown["shown"], true);
    assert!(k.graph.hosted.page(TOKEN).unwrap().2, "the page is up");
    let hosted =
        kelve_graph_api::v2::client::Client::new(&k.graph.url, Some(TOKEN.into())).unwrap();
    let character = hosted.character().await.unwrap().expect("a character");
    assert!(character.check().is_ok());
    let scores = k.json("GET", "/api/hosted/scores", None).await;
    assert_eq!(scores[0]["shown"], true);

    // Taken down, then the key forgotten.
    let down = k
        .json("POST", "/api/hosted/page", Some(json!({ "public": false })))
        .await;
    assert_eq!(down["public"], false);
    let account = k
        .json("DELETE", "/api/hosted/keys/api-key/anthropic", None)
        .await;
    assert_eq!(account["keys"], json!([]));
    assert_eq!(k.sealed("keys.provider.anthropic").await, None);
    // A subscription's token, kept in its own place.
    let sub = json!({ "provider": "anthropic", "kind": "subscription", "key": "sk-test-sub" });
    let account = k.json("POST", "/api/hosted/keys", Some(sub)).await;
    assert_eq!(account["keys"][0]["kind"], "subscription");
    let sealed = k.sealed("keys.subscription.anthropic").await.unwrap();
    assert!(!String::from_utf8_lossy(&sealed).contains("sk-test-sub"));

    // A second run today is one too many.
    let again = json!({ "kind": "rust", "agent": "claude-code", "model": "claude-sonnet-5-6" });
    let (status, said) = k.refused("POST", "/api/hosted/runs", Some(again)).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(
        said["error"]
            .as_str()
            .unwrap()
            .contains("today’s verified runs")
    );

    // Deleting the account takes the page with it, and signs kelve out.
    let gone = k.json("POST", "/api/hosted/account/delete", None).await;
    assert_eq!(gone["signedIn"], false);
    assert_eq!(k.graph.hosted.page(TOKEN), None);
    assert_eq!(k.sealed("keys.hosted").await, None);
}

#[tokio::test]
async fn signing_out_forgets_the_key_and_a_refused_sign_in_says_what_to_do() {
    let k = Kelve::new().await;
    k.sign_in().await;
    let out = k.json("POST", "/api/hosted/signout", None).await;
    assert_eq!(out["signedIn"], false);
    assert_eq!(k.sealed("keys.hosted").await, None);

    // The person said no at GitHub.
    let address = k.json("POST", "/api/hosted/signin", None).await["address"]
        .as_str()
        .unwrap()
        .to_string();
    let browser = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let back = browser.get(&address).send().await.unwrap();
    let back = back.headers()["location"].to_str().unwrap().to_string();
    let (start, query) = back.split_once('?').unwrap();
    let state = query
        .split('&')
        .find_map(|p| p.strip_prefix("state="))
        .unwrap();
    let said = browser
        .get(format!("{start}?error=access_denied&state={state}"))
        .send()
        .await
        .unwrap();
    assert_eq!(said.status(), 400);
    let said = said.text().await.unwrap();
    assert!(said.contains("That didn’t work"), "{said}");
    assert!(
        said.contains("GitHub didn’t finish signing you in."),
        "{said}"
    );
    assert_eq!(k.json("GET", "/api/hosted", None).await["signedIn"], false);

    // A sign-in given up on stops waiting.
    k.json("POST", "/api/hosted/signin", None).await;
    let stopped = k.json("POST", "/api/hosted/signin/stop", None).await;
    assert_eq!(stopped["waiting"], false);
}

#[tokio::test]
async fn without_kelve_ai_there_is_nothing_to_sign_in_to() {
    let graph = StandIn::start().await.unwrap();
    drop(graph);
    let mut config = test_config();
    config.endpoints.graph = String::new();
    let pool = kelve_store::open(&config.store).await.unwrap();
    kelve_store::migrate(&pool).await.unwrap();
    let app = App::new(config, pool).await.unwrap();
    let view = kelve_server::hosted::view(&app).await.unwrap();
    assert!(!view.available && !view.signed_in);
}
