//! kelve.ai from the app (docs/skills.md, "Proof: two scores"): the
//! person's account there, their page, and the verified runs that put
//! scores on it. kelve signs in as it once did to the shared graph, and keeps
//! what signing in gives sealed with the install's key: the account's key,
//! and, once a run first needs it, the person's own key for the model's
//! provider, which goes nowhere but a run's confirm.
//!
//! What leaves: the person's skills, for a run they asked for, with
//! secrets and paths stripped and shown to them first; their agent's
//! character and name, for their page, when they publish it. Nothing else.

use std::path::Path;
use std::time::Duration;

use kelve_core::character::{Draw, Look, Move};
use kelve_core::hosted::{
    CredentialKind, HostedView, ProviderKey, RunState, RunsLeft, ScoreView, SkillFileView,
    for_sending, provider_name,
};
use kelve_core::views::{ScoreSquare, ServerEvent};
use kelve_graph_api::v2::client::{Client, Error};
use kelve_graph_api::v2::{self as v2, tar};
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::state::{App, Shared};

/// Where the account's key lives in the store, sealed.
const ACCOUNT: &str = "keys.hosted";

/// How long kelve waits for kelve.ai when it only asks how things stand.
const ASK: Duration = Duration::from_secs(5);

/// How often kelve asks kelve.ai about a run still going, with no one on
/// its page.
const WATCH: Duration = Duration::from_secs(60);

/// What kelve holds for kelve.ai while it runs: a sign-in under way.
#[derive(Debug, Default)]
pub struct Hosted {
    pub(crate) signing_in: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
}

impl Hosted {
    /// Whether a sign-in is waiting for the browser to come back.
    pub fn waiting(&self) -> bool {
        self.signing_in
            .lock()
            .ok()
            .and_then(|w| w.as_ref().map(|t| !t.is_finished()))
            .unwrap_or(false)
    }

    /// Stops waiting for a sign-in.
    pub fn stop_waiting(&self) {
        if let Some(old) = self.signing_in.lock().ok().and_then(|mut w| w.take()) {
            old.abort();
        }
    }
}

/// What signing in gave this kelve: the account's key, and its page's
/// address once it has one.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Account {
    pub key: String,
    pub handle: Option<String>,
}

/// kelve.ai, signed in as no one: for signing in. None when this kelve has
/// none (`KELVE_GRAPH_URL=off`).
pub fn service(app: &App) -> Option<Client> {
    match app.config.endpoints.graph.as_str() {
        "" => None,
        url => Client::new(url, None).ok(),
    }
}

async fn open(app: &App, name: &str) -> ApiResult<Option<String>> {
    let mut conn = app.db.acquire().await?;
    let sealed = kelve_store::settings::secret(&mut conn, name).await?;
    Ok(sealed
        .and_then(|s| app.config.key.open(&s))
        .and_then(|b| String::from_utf8(b).ok()))
}

async fn seal(app: &App, name: &str, secret: Option<&str>) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    match secret {
        Some(s) => {
            let sealed = app.config.key.seal(s.as_bytes());
            kelve_store::settings::put_secret(&mut conn, name, &sealed).await?;
        }
        None => kelve_store::settings::delete_secret(&mut conn, name).await?,
    }
    Ok(())
}

pub(crate) async fn account(app: &App) -> ApiResult<Option<Account>> {
    Ok(open(app, ACCOUNT)
        .await?
        .and_then(|json| serde_json::from_str(&json).ok()))
}

/// Keeps what signing in gave, sealed, or forgets it, and the runs made
/// as that account with it.
pub(crate) async fn keep_account(app: &App, account: Option<&Account>) -> ApiResult<()> {
    let json = account
        .map(serde_json::to_string)
        .transpose()
        .map_err(ApiError::internal)?;
    seal(app, ACCOUNT, json.as_deref()).await?;
    if account.is_none() {
        let mut conn = app.db.acquire().await?;
        kelve_store::hosted::forget(&mut conn).await?;
    }
    Ok(())
}

/// Keeps the page's address with the account's key.
async fn keep_handle(app: &App, handle: Option<String>) -> ApiResult<()> {
    if let Some(mut a) = account(app).await? {
        a.handle = handle;
        keep_account(app, Some(&a)).await?;
    }
    Ok(())
}

/// kelve.ai as the person: None until they sign in.
pub async fn client(app: &App) -> ApiResult<Option<Client>> {
    let Some(a) = account(app).await? else {
        return Ok(None);
    };
    match app.config.endpoints.graph.as_str() {
        "" => Ok(None),
        url => Ok(Client::new(url, Some(a.key)).ok()),
    }
}

/// kelve.ai as the person, or what to do first.
pub async fn signed_in(app: &App) -> ApiResult<Client> {
    client(app).await?.ok_or_else(|| {
        ApiError::bad_request("Sign in to kelve.ai first, in settings.")
            .with(serde_json::json!({ "signIn": true }))
    })
}

/// The person's own credential of a kind for a model's provider, if kelve
/// keeps one.
pub(crate) async fn provider_key(
    app: &App,
    provider: &str,
    kind: CredentialKind,
) -> ApiResult<Option<String>> {
    if !fit_provider(provider) {
        return Ok(None);
    }
    open(app, &format!("{}{provider}", kind.prefix())).await
}

/// Keeps the person's credential of a kind for a provider, sealed, or
/// forgets it.
pub(crate) async fn keep_provider_key(
    app: &App,
    provider: &str,
    kind: CredentialKind,
    key: Option<&str>,
) -> ApiResult<()> {
    if !fit_provider(provider) {
        return Err(ApiError::bad_request(
            "kelve doesn’t know that provider. Choose one kelve.ai offers.",
        ));
    }
    seal(app, &format!("{}{provider}", kind.prefix()), key).await
}

/// What can pay for a run with these agents, as kelve.ai's offer says for
/// each: an API key always, a Claude subscription's token where it allows
/// one. The subscription first, when there is one.
pub fn accepts<'a>(
    agents: impl IntoIterator<Item = &'a kelve_graph_api::v2::AgentOffer>,
) -> Vec<CredentialKind> {
    let mut out: Vec<CredentialKind> = Vec::new();
    for c in agents.into_iter().flat_map(|a| a.credentials.iter()) {
        let kind = CredentialKind::from(*c);
        if !out.contains(&kind) {
            out.push(kind);
        }
    }
    if out.is_empty() {
        out.push(CredentialKind::ApiKey);
    }
    out.sort_by_key(|k| *k != CredentialKind::Subscription);
    out
}

/// A provider's ID as kelve.ai gives it: lower-case letters, digits and hyphens.
fn fit_provider(provider: &str) -> bool {
    !provider.is_empty()
        && provider.len() <= 40
        && provider
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The providers whose keys kelve keeps.
pub async fn kept_keys(app: &App) -> ApiResult<Vec<ProviderKey>> {
    let mut conn = app.db.acquire().await?;
    let mut kept = Vec::new();
    for kind in [CredentialKind::Subscription, CredentialKind::ApiKey] {
        let names = kelve_store::hosted::secrets_named(&mut conn, kind.prefix()).await?;
        kept.extend(
            names
                .into_iter()
                .filter_map(|n| n.strip_prefix(kind.prefix()).map(str::to_string))
                .map(|provider| ProviderKey {
                    name: provider_name(&provider),
                    provider,
                    kind,
                }),
        );
    }
    Ok(kept)
}

/// What a call to kelve.ai that failed says to the person.
pub fn said(e: Error) -> ApiError {
    use axum::http::StatusCode;
    match e {
        Error::SignIn => ApiError::bad_request(
            "kelve.ai doesn’t know this kelve any more. Sign in again, in settings.",
        )
        .with(serde_json::json!({ "signIn": true })),
        // Never a 401: that's this browser signed out of kelve.
        Error::Refused { status, said } => {
            let status = match status {
                404 => StatusCode::NOT_FOUND,
                409 => StatusCode::CONFLICT,
                _ => StatusCode::BAD_REQUEST,
            };
            ApiError::new(status, said)
        }
        e @ Error::Limited { .. } => ApiError::new(StatusCode::TOO_MANY_REQUESTS, e.to_string()),
        e => {
            tracing::info!(error = %e, "kelve.ai couldn't be asked");
            ApiError::new(StatusCode::BAD_GATEWAY, e.to_string())
        }
    }
}

/// The account as settings and Your agent show it. Asks kelve.ai how the
/// page stands, briefly; if it can't, says so and shows what's known here.
pub async fn view(app: &Shared) -> ApiResult<HostedView> {
    let available = service(app).is_some();
    let held = account(app).await?;
    let mut view = HostedView {
        available,
        signed_in: available && held.is_some(),
        waiting: app.hosted.waiting(),
        handle: held.as_ref().and_then(|a| a.handle.clone()),
        page: None,
        public: false,
        runs: None,
        going: None,
        keys: kept_keys(app).await?,
        problem: None,
    };
    if !view.signed_in {
        return Ok(view);
    }
    let Some(c) = client(app).await? else {
        return Ok(view);
    };
    let (asked, _) = tokio::join!(
        tokio::time::timeout(ASK, c.account()),
        runs_up_to_date(app, &c)
    );
    {
        let mut conn = app.db.acquire().await?;
        view.going = kelve_store::hosted::going(&mut conn).await?.map(|r| r.id);
    }
    match asked {
        Ok(Ok(a)) => {
            if a.handle != view.handle {
                keep_handle(app, a.handle.clone()).await?;
            }
            view.handle = a.handle;
            view.page = a.page;
            view.public = a.public;
            view.runs = Some(runs_left(&a.runs));
        }
        // Its key no longer opens anything: the account went, or this
        // kelve was signed out from elsewhere.
        Ok(Err(Error::SignIn)) => {
            keep_account(app, None).await?;
            view.signed_in = false;
            view.handle = None;
            view.going = None;
        }
        Ok(Err(e)) => view.problem = Some(e.to_string()),
        Err(_) => view.problem = Some(Error::Unreachable.to_string()),
    }
    Ok(view)
}

// ---------------------------------------------------------------- runs

/// A run's state as the app says it, and as the store keeps it.
pub(crate) fn run_state(state: v2::RunState) -> (RunState, &'static str) {
    match state {
        v2::RunState::Estimated => (RunState::Estimated, "estimated"),
        v2::RunState::Queued => (RunState::Queued, "queued"),
        v2::RunState::Running => (RunState::Running, "running"),
        v2::RunState::Finished => (RunState::Finished, "finished"),
        v2::RunState::Cancelled => (RunState::Cancelled, "cancelled"),
        v2::RunState::Failed => (RunState::Failed, "failed"),
    }
}

/// Asks kelve.ai how each run still going stands, and keeps it, so a run
/// that finished while no one was on its page doesn't look as if it's going,
/// and its score comes to home. Those it can't ask about now stay as they
/// were, to ask again. Anything that changed is published.
async fn runs_up_to_date(app: &Shared, c: &Client) {
    let going = match app.db.acquire().await {
        Ok(mut conn) => kelve_store::hosted::all_going(&mut conn).await,
        Err(e) => Err(e.into()),
    };
    let going = match going {
        Ok(going) => going,
        Err(e) => {
            tracing::warn!(error = %e, "couldn't read which runs are going");
            return;
        }
    };
    let mut changed = false;
    for run in going {
        let (state, score) = match tokio::time::timeout(ASK, c.run(&run.id)).await {
            Ok(Ok(Some(s))) => (run_state(s.state).1, s.score),
            // kelve.ai no longer has it, so it can't still be going.
            Ok(Ok(None)) => ("failed", None),
            Ok(Err(e)) => {
                tracing::info!(run = %run.id, error = %e, "couldn't ask kelve.ai how a run stands");
                continue;
            }
            Err(_) => continue,
        };
        if state == run.state {
            continue;
        }
        let kept = match app.db.acquire().await {
            Ok(mut conn) => {
                kelve_store::hosted::stands(&mut conn, &run.id, state, score.as_deref()).await
            }
            Err(e) => Err(e.into()),
        };
        match kept {
            Ok(()) => changed = true,
            Err(e) => tracing::warn!(run = %run.id, error = %e, "couldn't keep how a run stands"),
        }
    }
    if changed {
        app.publish(ServerEvent::Settings);
    }
}

/// Keeps what kelve knows of runs right with no one looking: a person who
/// started one and went away finds it as it is.
pub fn start(app: &Shared) {
    let app = app.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                () = tokio::time::sleep(WATCH) => {},
                () = app.stopped() => return,
            }
            match client(&app).await {
                Ok(Some(c)) => runs_up_to_date(&app, &c).await,
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(error = %e.message, "couldn't read the account to ask about its runs")
                }
            }
        }
    });
}

pub(crate) fn runs_left(a: &v2::Allowance) -> RunsLeft {
    RunsLeft {
        left: a.left,
        a_day: a.a_day,
        more: a.more,
    }
}

// ---------------------------------------------------------------- the page

/// How the person's agent looks, and the name they gave it, if they did,
/// as kept in kelve's settings, under `look` and `agentName`: the robot,
/// unnamed, until they choose.
pub async fn look_and_name(app: &App) -> (Look, Option<String>) {
    let raw = match app.db.acquire().await {
        Ok(mut conn) => kelve_store::settings::get(&mut conn)
            .await
            .unwrap_or_default(),
        Err(_) => serde_json::Value::Null,
    };
    let look = raw
        .get("look")
        .and_then(|l| serde_json::from_value::<Look>(l.clone()).ok())
        .unwrap_or_default();
    let name = raw
        .get("agentName")
        .and_then(|n| n.as_str())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty() && kelve_core::character::name_problem(n).is_none());
    (look, name)
}

/// The person's agent as its page draws it, wearing what it drew for them.
pub fn character(app: &App, look: &Look) -> v2::Character {
    let (parts, characters) = crate::your_agent::Kept::of(&app.settings()).made();
    let letters = look.picture(&parts, &characters, Move::Idle).letters();
    let c = &look.robot.colours;
    use kelve_core::character::Role;
    v2::Character {
        format: v2::character::FORMAT.into(),
        name: None,
        colours: letters.colours,
        palette: Some(v2::Palette {
            body: c.of(Role::Body),
            light: c.of(Role::Light),
            screen: c.of(Role::Screen),
            eyes: c.of(Role::Eyes),
        }),
        layers: letters.layers,
    }
}

/// Sends the agent's character and name to the page, and publishes it or
/// takes it down. Publishing sends them first, so what shows is as the
/// person sees their agent now.
pub async fn set_page(app: &Shared, c: &Client, public: bool) -> ApiResult<v2::Account> {
    let (look, name) = look_and_name(app).await;
    if public {
        c.set_character(&character(app, &look))
            .await
            .map_err(said)?;
    }
    let a = c
        .set_page(&v2::PageSettings { name, public })
        .await
        .map_err(said)?;
    tracing::info!(public, "set the person's page on kelve.ai");
    Ok(a)
}

/// The agent's character or name changed: sends them to the page, if
/// it's published. Asked by whatever changes them; a page that can't be
/// told now is told next time it's published.
pub async fn character_changed(app: &Shared) {
    let Ok(Some(c)) = client(app).await else {
        return;
    };
    match c.account().await {
        Ok(a) if a.public => {
            if let Err(e) = set_page(app, &c, true).await {
                tracing::info!(error = %e.message, "couldn't send the agent's character to its page");
            }
        }
        _ => {}
    }
}

/// A score as the app shows it.
pub fn score_view(s: &v2::Score) -> ScoreView {
    let r = &s.result;
    ScoreView {
        id: s.id.clone(),
        run: s.run.clone(),
        link: s.link.clone(),
        shown: s.published,
        kind_name: r.kind_name.clone(),
        agent_name: r.agent_name.clone(),
        model_name: r.model_name.clone(),
        bank: r.bank.clone(),
        squares: r
            .tasks
            .iter()
            .map(|v| match v {
                v2::Verdict::Better => ScoreSquare::Better,
                v2::Verdict::Same => ScoreSquare::Same,
                v2::Verdict::Worse => ScoreSquare::Worse,
            })
            .collect(),
        better: u32::try_from(r.better()).unwrap_or(u32::MAX),
        of: u32::try_from(r.tasks.len()).unwrap_or(u32::MAX),
        gain: r.gain,
        gain_low: r.gain_low,
        gain_high: r.gain_high,
        judge_counted: r.judge_counted,
        verified: r.verified,
        at: r.finished,
    }
}

// ---------------------------------------------------------------- the skills that go

/// Every file of the person's own skills, as it would go to a verified
/// run: each skill folder whole, text files only, every secret and path
/// on their computer taken out. A project's skills stay with the project.
pub fn skills(dir: &Path) -> std::io::Result<Vec<tar::File>> {
    let mut files = Vec::new();
    for skill in crate::skills_home::list(dir)? {
        walk(&skill.dir, &skill.name, &mut files)?;
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn walk(dir: &Path, at: &str, files: &mut Vec<tar::File>) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let path = format!("{at}/{name}");
        // Never a link: it could lead anywhere on the computer.
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if meta.is_dir() {
            walk(&entry.path(), &path, files)?;
            continue;
        }
        if !meta.is_file() || meta.len() > kelve_core::hosted::MOST_FILE as u64 || !tar::fit(&path)
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        files.push(tar::File {
            path,
            bytes: for_sending(&text).into_bytes(),
        });
    }
    Ok(())
}

/// What goes, file by file, as the app lists it.
pub fn listed(files: &[tar::File]) -> Vec<SkillFileView> {
    files
        .iter()
        .map(|f| SkillFileView {
            path: f.path.clone(),
            bytes: u32::try_from(f.bytes.len()).unwrap_or(u32::MAX),
        })
        .collect()
}

// ---------------------------------------------------------------- the sign-in page

/// The page the browser lands on once kelve.ai sends it back: the person's
/// agent, pleased, and a line on what happened.
pub fn signed_in_page(app: &App, look: &Look, ok: bool, said: &str) -> String {
    let moving = if ok { Move::Happy } else { Move::Idle };
    let (parts, characters) = crate::your_agent::Kept::of(&app.settings()).made();
    let svg = look.picture(&parts, &characters, moving).svg(
        &look.robot.colours,
        &Draw {
            moving,
            label: "Your agent".into(),
            style: true,
        },
    );
    let escape = |t: &str| {
        t.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let title = if ok {
        "You’re signed in"
    } else {
        "That didn’t work"
    };
    let back = escape(&format!("{}/#/settings/account", app.config.url));
    format!(
        "<!doctype html><html lang=\"en-GB\"><meta charset=utf-8><meta name=viewport content=\"width=device-width\"><title>kelve</title>\
<style>body{{margin:0;min-height:100vh;display:flex;flex-direction:column;background:#fbfaf7;color:#1d1c1a;font-family:system-ui,sans-serif}}\
header{{display:flex;align-items:center;gap:8px;padding:18px 40px;font-size:15px;font-weight:650}}header i{{width:11px;height:11px;background:#cf4a2f;border-radius:3px}}\
main{{flex:1;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:28px;padding:24px 24px 120px;text-align:center}}\
.px{{width:200px;height:225px}}h1{{margin:0;font-size:46px;line-height:52px;letter-spacing:-.03em}}p{{margin:0;font-size:19px;line-height:29px;color:#3d3a35;max-width:36rem}}\
a{{display:inline-block;margin-top:12px;font-size:16px;font-weight:600;color:#a8361f;background:#fdf0ec;border:1px solid #f0c4b8;border-radius:11px;padding:12px 22px;text-decoration:none}}\
@media (prefers-color-scheme:dark){{body{{background:#1b1a18;color:#ece8e0}}p{{color:#c2bbb0}}a{{color:#ef8f7a;background:#35221d;border-color:#6b3a2e}}}}</style>\
<header><i aria-hidden=true></i>kelve</header><main>{svg}<div><h1>{title}</h1><p>{}</p>{}</div></main></html>",
        escape(said),
        if ok {
            String::new()
        } else {
            format!("<a href=\"{back}\">Back to kelve</a>")
        }
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn every_file_of_every_skill_goes_with_secrets_and_paths_out() {
        let dir = std::env::temp_dir().join(format!("kelve-hosted-{}", crate::crypto::token()));
        let skill = dir.join("rust");
        std::fs::create_dir_all(skill.join("references")).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: rust\n---\nSee /Users/sam/notes.md\n",
        )
        .unwrap();
        std::fs::write(
            skill.join("references/errors.md"),
            "key: ghp_abcdefghijklmnopqrstuvwxyz0123456789\n",
        )
        .unwrap();
        std::fs::write(skill.join(".hidden"), "no").unwrap();
        std::fs::write(skill.join("logo.png"), [0xff, 0xfe, 0x00]).unwrap();
        // A folder with no SKILL.md isn't a skill.
        std::fs::create_dir_all(dir.join("loose")).unwrap();
        std::fs::write(dir.join("loose/x.md"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", skill.join("hosts")).unwrap();

        let files = skills(&dir).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["rust/SKILL.md", "rust/references/errors.md"]);
        let all: String = files
            .iter()
            .map(|f| String::from_utf8_lossy(&f.bytes).to_string())
            .collect();
        assert!(!all.contains("/Users/sam"), "{all}");
        assert!(!all.contains("ghp_abc"), "{all}");
        assert!(tar::read(&tar::write(&files).unwrap()).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
