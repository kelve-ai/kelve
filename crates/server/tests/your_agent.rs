//! Your agent, through kelve's router on a store of its own: its name and
//! look, kept for the person; what it draws when they ask, checked before
//! it's kept; the Your agent page; and the settings for it and for checks.

#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use kelve_server::agent::{self, HandedIn};
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
        let config = test_config();
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
        };
        k.sign_in().await;
        k
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
        .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(str::to_string);
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value, cookie)
    }

    async fn json(&self, method: &str, path: &str, body: Option<Value>) -> Value {
        let (status, value, _) = self.call(method, path, body, false).await;
        assert!(status.is_success(), "{method} {path}: {status} {value}");
        value
    }

    /// What kelve says is wrong with a change: its status and sentence.
    async fn refused(&self, method: &str, path: &str, body: Value) -> (StatusCode, String) {
        let (status, value, _) = self.call(method, path, Some(body), false).await;
        assert!(
            status.is_client_error(),
            "{method} {path}: {status} {value}"
        );
        (
            status,
            value["detail"]
                .as_str()
                .or(value["error"].as_str())
                .unwrap_or_default()
                .to_string(),
        )
    }

    async fn sign_in(&mut self) {
        let (status, _, _) = self.call("POST", "/api/signin-link", None, true).await;
        assert_eq!(status, StatusCode::OK);
        let link = self.links.lock().unwrap().last().cloned().unwrap();
        let path = link.trim_start_matches("http://localhost:4600").to_string();
        let (_, _, cookie) = self.call("GET", &path, None, false).await;
        self.cookie = Some(cookie.expect("a session cookie"));
    }
}

#[tokio::test]
async fn its_name_is_yours_until_you_give_it_one_and_its_look_is_kept() {
    let k = Kelve::new().await;
    let shown = k.json("GET", "/api/character", None).await;
    assert_eq!(shown["name"], "Your agent", "no first name to go on");
    assert_eq!(shown["named"], false);
    assert_eq!(shown["look"]["character"], "robot");
    assert!(
        shown["svg"]
            .as_str()
            .unwrap()
            .contains("aria-label=\"Your agent\"")
    );
    assert_eq!(shown["made"], json!([]));

    // The person's first name, from their git config.
    std::fs::create_dir_all(&k.app.config.agents_home).unwrap();
    std::fs::write(
        k.app.config.agents_home.join(".gitconfig"),
        "[user]\n\tname = Sam Clark\n",
    )
    .unwrap();
    let shown = k.json("GET", "/api/character", None).await;
    assert_eq!(shown["name"], "Sam’s agent");

    let named = k
        .json("POST", "/api/character", Some(json!({ "name": "  Bolt " })))
        .await;
    assert_eq!(named["name"], "Bolt");
    assert_eq!(named["named"], true);
    assert_eq!(
        k.json("GET", "/api/home", None).await["name"],
        "Bolt",
        "home says it too"
    );
    let (status, why) = k
        .refused("POST", "/api/character", json!({ "name": "x".repeat(30) }))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(why.contains("the most is 24"), "{why}");
    let (_, why) = k
        .refused("POST", "/api/character", json!({ "name": "<b>Bolt</b>" }))
        .await;
    assert!(why.contains("markup"), "{why}");
    assert_eq!(
        k.json("GET", "/api/character", None).await["name"],
        "Bolt",
        "kept as it was"
    );
    let unnamed = k
        .json("POST", "/api/character", Some(json!({ "name": "" })))
        .await;
    assert_eq!(unnamed["name"], "Sam’s agent");
    assert_eq!(unnamed["named"], false);

    // A look: the cat, in the colours the person chose.
    let mut look = shown["look"].clone();
    look["character"] = json!("cat");
    look["robot"]["colours"]["body"] = json!("#f6b26b");
    look["background"] = json!("#fff1d6");
    let worn = k
        .json("POST", "/api/character", Some(json!({ "look": look })))
        .await;
    assert_eq!(worn["look"]["character"], "cat");
    assert!(worn["svg"].as_str().unwrap().contains("#f6b26b"));
    assert_eq!(
        k.app.settings().look.background,
        "#fff1d6",
        "kept with the settings"
    );
    // Where kelve.ai's page reads them: `look` and `agentName` in settings.
    k.json("POST", "/api/character", Some(json!({ "name": "Bolt" })))
        .await;
    let (on_page, named) = kelve_server::hosted::look_and_name(&k.app).await;
    assert_eq!(on_page.character, "cat");
    assert_eq!(named.as_deref(), Some("Bolt"));
    let mut wrong = look.clone();
    wrong["robot"]["parts"]["hat"] = json!("a hat no one drew");
    let (_, why) = k
        .refused("POST", "/api/character", json!({ "look": wrong }))
        .await;
    assert!(why.contains("robot's hat"), "{why}");
    let mut wrong = look.clone();
    wrong["robot"]["colours"]["eyes"] = json!("red");
    let (_, why) = k
        .refused("POST", "/api/character", json!({ "look": wrong }))
        .await;
    assert!(why.contains("#rrggbb"), "{why}");

    // Only the person: not signed in, or as the command line, it's refused.
    let (status, _, _) = k.call("GET", "/api/character", None, true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = k
        .call(
            "POST",
            "/api/character/draw",
            Some(json!({ "asked": "a hat" })),
            true,
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

const HAT: &str = r##"{
  "format": "kelve-part/1",
  "name": "Party hat",
  "slot": "hat",
  "at": [9, 0],
  "colours": { "a": "light", "g": "#f4c84a", "w": "#ffffff" },
  "pixels": ["..ww..", "..gg..", ".aaaa.", "agggga"]
}"##;

#[tokio::test]
async fn your_agent_draws_what_you_ask_for_and_kelve_checks_it_first() {
    let k = Kelve::new().await;
    let asked = k
        .json(
            "POST",
            "/api/character/draw",
            Some(json!({ "asked": "  a party hat " })),
        )
        .await;
    assert_eq!(asked["state"], "waiting");
    assert_eq!(asked["asked"], "a party hat");
    let (_, why) = k
        .refused("POST", "/api/character/draw", json!({ "asked": "  " }))
        .await;
    assert!(why.contains("Say what to draw"), "{why}");

    // kelve asks the agent, with what was asked fenced, and the colours chosen.
    common::jobs(&k.app).await;
    let job = agent::next(&k.app, &common::caller())
        .await
        .unwrap()
        .expect("the draw job");
    assert_eq!(job.kind, "draw");
    assert_eq!(job.about, "Drawing a party hat");
    assert_eq!(job.material["asked"], "<asked>\na party hat\n</asked>");
    assert_eq!(job.material["colours"]["light"], "#ff7a59");
    let shown = k.json("GET", "/api/character/drawings", None).await;
    assert_eq!(shown[0]["state"], "drawing");

    // A part outside its slot goes back to the agent, with what to change.
    let mut wide: Value = serde_json::from_str(HAT).unwrap();
    wide["at"] = json!([20, 0]);
    let handed = agent::hand_in(
        &k.app,
        &common::caller(),
        job.job,
        json!({ "drawing": wide }),
    )
    .await
    .unwrap();
    let HandedIn::No { why, put_aside } = handed else {
        panic!("a part outside its box was taken");
    };
    assert!(why.contains("outside its slot's box"), "{why}");
    assert!(!put_aside, "it gets another go");
    // Its second go fits: kept, for the person to try on.
    let handed = agent::hand_in(
        &k.app,
        &common::caller(),
        job.job,
        json!({ "drawing": HAT }),
    )
    .await
    .unwrap();
    assert_eq!(handed, HandedIn::Yes);
    common::jobs(&k.app).await;
    let shown = k.json("GET", "/api/character/drawings", None).await;
    assert_eq!(shown[0]["state"], "done");
    let key = shown[0]["made"].as_str().unwrap().to_string();
    let character = k.json("GET", "/api/character", None).await;
    let made = &character["made"][0];
    assert_eq!(made["key"], key.as_str());
    assert_eq!(made["name"], "Party hat");
    assert_eq!(made["slot"], "hat");
    assert_eq!(made["part"]["pixels"][3], "agggga");
    assert!(made["svg"].as_str().unwrap().contains("fill=\"#f4c84a\""));
    // Worn.
    let mut look = character["look"].clone();
    look["robot"]["parts"]["hat"] = json!(key);
    let worn = k
        .json("POST", "/api/character", Some(json!({ "look": look })))
        .await;
    assert_eq!(worn["look"]["robot"]["parts"]["hat"], key.as_str());

    // A drawing that never fits is dropped, and says why.
    k.json(
        "POST",
        "/api/character/draw",
        Some(json!({ "asked": "a cat" })),
    )
    .await;
    common::jobs(&k.app).await;
    let job = agent::next(&k.app, &common::caller())
        .await
        .unwrap()
        .expect("the cat");
    for _ in 0..2 {
        agent::hand_in(
            &k.app,
            &common::caller(),
            job.job,
            json!({ "drawing": { "format": "kelve-character/1", "name": "Cat" } }),
        )
        .await
        .unwrap();
    }
    assert!(
        agent::next(&k.app, &common::caller())
            .await
            .unwrap()
            .is_none(),
        "dropped after two tries"
    );
    let shown = k.json("GET", "/api/character/drawings", None).await;
    assert_eq!(shown[0]["asked"], "a cat");
    assert_eq!(shown[0]["state"], "failed");
    assert!(
        shown[0]["why"].as_str().unwrap().contains("layers"),
        "{}",
        shown[0]
    );
    assert_eq!(shown[1]["state"], "done");
}

#[tokio::test]
async fn the_page_shows_what_it_knows_and_how_it_fits_you() {
    let k = Kelve::new().await;
    let page = k.json("GET", "/api/your-agent", None).await;
    assert_eq!(page["knows"], json!([]));
    assert_eq!(page["fits"], Value::Null);
    assert_eq!(page["scores"], json!([]));
    assert_eq!(page["page"], Value::Null);

    // Two skills, and what each part's checks found.
    let mut conn = k.app.db.acquire().await.unwrap();
    use kelve_store::skills::{NewSkill, SeenPart, add, checked, seen};
    let rust = add(
        &mut conn,
        &NewSkill {
            name: "rust",
            kind: "like",
            work: "rust",
            ..NewSkill::default()
        },
    )
    .await
    .unwrap();
    let everywhere = add(
        &mut conn,
        &NewSkill {
            name: "everywhere",
            kind: "like",
            work: "everywhere",
            ..NewSkill::default()
        },
    )
    .await
    .unwrap();
    let part = |key: &'static str| SeenPart {
        key,
        heading: key,
        section: "like",
        body_hash: "h",
    };
    let ids = seen(
        &mut conn,
        rust,
        &[part("a"), part("b"), part("c"), part("d"), part("e")],
    )
    .await
    .unwrap();
    for (id, state) in ids
        .iter()
        .zip(["works", "failed", "running", "no_difference", "unchecked"])
    {
        checked(&mut conn, *id, state, None, None).await.unwrap();
    }
    seen(&mut conn, everywhere, &[part("a")]).await.unwrap();
    // Three blind picks: their own agent's work twice, and the standard's once.
    use kelve_store::checks::{NewTask, add_pick, add_task, pick};
    let task = add_task(
        &mut conn,
        &NewTask {
            request: "a flag",
            root: "/r",
            start: "abc",
            ..NewTask::default()
        },
    )
    .await
    .unwrap();
    for (side, picked) in [("left", "left"), ("right", "left"), ("right", "right")] {
        let id = add_pick(&mut conn, task, side).await.unwrap();
        pick(&mut conn, id, picked).await.unwrap();
    }
    add_pick(&mut conn, task, "left").await.unwrap();
    drop(conn);

    let page = k.json("GET", "/api/your-agent", None).await;
    assert_eq!(page["knows"][0]["title"], "Everywhere", "everywhere first");
    assert_eq!(page["knows"][0]["parts"], json!(["notChecked"]));
    assert_eq!(page["knows"][1]["name"], "rust");
    assert_eq!(page["knows"][1]["title"], "Rust");
    assert_eq!(page["knows"][1]["kind"], "like");
    assert_eq!(
        page["knows"][1]["parts"],
        json!(["works", "failed", "waiting", "noDifference", "notChecked"])
    );
    assert_eq!(
        page["fits"],
        json!({ "picked": 2, "of": 3, "tests": [true, false, true] })
    );
    assert_eq!(page["character"]["name"], "Your agent");
}

/// On by default, the skills folder really is on: kelve's links follow it,
/// and what the person says, in setup or in settings, holds across restarts.
#[cfg(unix)]
#[tokio::test]
async fn the_skills_folder_is_on_by_default_until_the_person_says_no() {
    use kelve_core::hook::Agent;
    use kelve_server::agent_loop::links;

    let k = Kelve::new().await;
    let c = &k.app.config;
    let (home, claude, skills) = (&c.agents_home, &c.claude_dir, &c.skills_dir);
    let linked = || claude.join("skills/rust").symlink_metadata().is_ok();
    kelve_server::skills_home::write(skills, "rust", "---\nname: rust\n---\n").unwrap();

    // As kelve starts, with no agent connected: nothing to link, and it stays on.
    links::keep_in_step(&k.app);
    links::sync_setting(&k.app).await;
    assert!(k.app.settings().skills_folder);
    assert!(!linked());

    // An agent connects, and the links come as kelve next starts.
    kelve_server::hooks::set(Agent::Claude, true, home, claude).unwrap();
    links::keep_in_step(&k.app);
    links::sync_setting(&k.app).await;
    assert!(linked());
    assert!(k.app.settings().skills_folder);

    // Turned off in settings: they go, and don't come back on a restart.
    k.json(
        "POST",
        "/api/settings/agent",
        Some(json!({ "skillsFolder": false })),
    )
    .await;
    assert!(!linked());
    links::keep_in_step(&k.app);
    links::sync_setting(&k.app).await;
    assert!(!linked());
    assert!(!k.app.settings().skills_folder);

    // Turned on again: back, and they hold.
    k.json(
        "POST",
        "/api/settings/agent",
        Some(json!({ "skillsFolder": true })),
    )
    .await;
    assert!(linked());
    links::sync_setting(&k.app).await;
    assert!(k.app.settings().skills_folder);

    // No, as setup says it, comes to the setting; yes likewise.
    links::unlink(Agent::Claude, home, claude, skills).unwrap();
    links::sync_setting(&k.app).await;
    assert!(!k.app.settings().skills_folder);
    links::keep_in_step(&k.app);
    assert!(!linked(), "a no isn't undone");
    links::link(Agent::Claude, home, claude, skills).unwrap();
    links::sync_setting(&k.app).await;
    assert!(k.app.settings().skills_folder);
    assert!(linked());

    // On with no agent to link: it stays on through a restart.
    kelve_server::hooks::set(Agent::Claude, false, home, claude).unwrap();
    links::unlink(Agent::Claude, home, claude, skills).unwrap();
    k.json(
        "POST",
        "/api/settings/agent",
        Some(json!({ "skillsFolder": true })),
    )
    .await;
    links::sync_setting(&k.app).await;
    assert!(k.app.settings().skills_folder);
}

#[tokio::test]
async fn settings_for_your_agent_and_for_checks() {
    let k = Kelve::new().await;
    let s = k.json("GET", "/api/settings/agent", None).await;
    assert_eq!(
        s,
        json!({
            "connected": [], "debriefs": "worthOne", "skillsFolder": true,
            "checks": "ask", "checksAWeek": 10, "checksThisWeek": 0,
        })
    );
    let s = k
        .json(
            "POST",
            "/api/settings/agent",
            Some(json!({ "debriefs": "onlyAsked", "skillsFolder": false, "checks": "never", "checksAWeek": 20 })),
        )
        .await;
    assert_eq!(s["debriefs"], "onlyAsked");
    assert_eq!(s["skillsFolder"], false);
    assert_eq!(s["checks"], "never");
    assert_eq!(s["checksAWeek"], 20);
    assert_eq!(
        k.app.settings().debriefs,
        kelve_core::due::DebriefsWhen::OnlyAsked,
        "kept"
    );
    let (_, why) = k
        .refused("POST", "/api/settings/agent", json!({ "checksAWeek": 0 }))
        .await;
    assert!(why.contains("between 1 and 100"), "{why}");

    // Connected: the agents whose settings hold kelve's hook. Disconnecting
    // takes it out, and leaves the rest of their settings be.
    let claude = &k.app.config.claude_dir;
    std::fs::create_dir_all(claude).unwrap();
    std::fs::write(claude.join("settings.json"), r#"{ "theme": "dark" }"#).unwrap();
    kelve_server::hooks::set(
        kelve_core::hook::Agent::Claude,
        true,
        &k.app.config.agents_home,
        claude,
    )
    .unwrap();
    let s = k.json("GET", "/api/settings/agent", None).await;
    assert_eq!(
        s["connected"],
        json!([{ "agent": "claude", "name": "Claude Code" }])
    );
    let s = k
        .json(
            "POST",
            "/api/settings/agent",
            Some(json!({ "disconnect": "claude" })),
        )
        .await;
    assert_eq!(s["connected"], json!([]));
    let left: Value =
        serde_json::from_str(&std::fs::read_to_string(claude.join("settings.json")).unwrap())
            .unwrap();
    assert_eq!(left["theme"], "dark");
    let (_, why) = k
        .refused(
            "POST",
            "/api/settings/agent",
            json!({ "disconnect": "nobody" }),
        )
        .await;
    assert!(why.contains("doesn't know that agent"), "{why}");
}
