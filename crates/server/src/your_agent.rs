//! Your agent (docs/skills.md, "Your agent, as a character"): its name, how
//! it looks, the parts and characters it drew for the person, what it knows,
//! and the settings for it and for skill checks. All of it is kept with the
//! person's settings: there's no table for it.
//!
//! The person asks their agent to draw something with the `draw` job: kelve
//! keeps what they asked, queues `v2jobs::DRAW`, and that job asks the agent,
//! with what they asked fenced and the colours they chose. kelve checks the
//! drawing (`core::character`) as it's handed in; one that doesn't fit goes
//! back to the agent once, with what to change, then is dropped. One that
//! fits is kept, for the person to try on.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{Datelike, Duration, TimeZone, Utc};
use kelve_core::character::{
    self as core, Character, Drawing, Look, MOST_NAME, Move, Part, default_name, name_problem,
};
use kelve_core::hook::Agent;
use kelve_core::instructions::DRAW;
use kelve_core::skills::{SkillKind, WorkKind};
use kelve_core::time::Timestamp;
use kelve_core::views::{
    AgentSettings, AgentSettingsIn, CharacterIn, CharacterView, ConnectedAgent, DrawIn,
    DrawingState, DrawingView, FitsYou, KnowsRow, MadeView, PartCheck, ServerEvent, YourAgentView,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agent::{self, Ask, Priority, Resume};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{self, Then};
use crate::state::Shared;
use crate::v2jobs;

/// The most parts and characters kept from what the agent drew, and the most
/// drawings asked for that are remembered: the oldest go first.
const MOST_MADE: usize = 40;
const MOST_ASKED: usize = 20;
/// The longest thing to ask the agent to draw, in characters.
const MOST_ASKED_CHARS: usize = 200;

/// What the person's agent drew for them, and the drawings they asked for:
/// kept in their settings under `drawn`, beside `look` and `agentName`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Drawn {
    pub parts: BTreeMap<String, MadePart>,
    pub characters: BTreeMap<String, MadeCharacter>,
    pub asked: Vec<Asked>,
}

/// What kelve keeps of the person's agent, read from their settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kept {
    /// The name the person gave it; none until they do.
    pub name: Option<String>,
    pub look: Look,
    /// What the agent drew, by key.
    pub parts: BTreeMap<String, MadePart>,
    pub characters: BTreeMap<String, MadeCharacter>,
    /// The drawings the person asked for, the oldest first.
    pub asked: Vec<Asked>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MadePart {
    pub part: Part,
    /// When it was kept, in ms.
    pub at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MadeCharacter {
    pub character: Character,
    pub at: i64,
}

/// A drawing the person asked for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Asked {
    pub id: String,
    /// What they asked, secrets stripped.
    pub asked: String,
    pub at: i64,
    /// What it made, by key, once it's kept.
    pub made: Option<String>,
}

impl Kept {
    /// What's kept, from the person's settings.
    pub fn of(s: &crate::settings::Settings) -> Kept {
        Kept {
            name: s.agent_name.clone(),
            look: s.look.clone(),
            parts: s.drawn.parts.clone(),
            characters: s.drawn.characters.clone(),
            asked: s.drawn.asked.clone(),
        }
    }

    /// Puts what's kept back in the person's settings.
    fn put(self, s: &mut crate::settings::Settings) {
        s.agent_name = self.name;
        s.look = self.look;
        s.drawn = Drawn {
            parts: self.parts,
            characters: self.characters,
            asked: self.asked,
        };
    }

    /// What the agent drew, as `core::character` reads it.
    pub fn made(&self) -> (BTreeMap<String, Part>, BTreeMap<String, Character>) {
        (
            self.parts
                .iter()
                .map(|(k, m)| (k.clone(), m.part.clone()))
                .collect(),
            self.characters
                .iter()
                .map(|(k, m)| (k.clone(), m.character.clone()))
                .collect(),
        )
    }
}

/// One change to what's kept at a time, so two never undo each other.
static WRITING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Changes what's kept of the person's agent, and saves it. A new look or
/// name goes to their page on kelve.ai too, if it's published.
async fn change<T>(app: &Shared, f: impl FnOnce(&mut Kept) -> ApiResult<T>) -> ApiResult<T> {
    let _one = WRITING.lock().await;
    let mut settings = (*app.settings()).clone();
    let mut kept = Kept::of(&settings);
    let before = (kept.look.clone(), kept.name.clone());
    let out = f(&mut kept)?;
    let changed = before != (kept.look.clone(), kept.name.clone());
    kept.put(&mut settings);
    app.save_settings(settings).await?;
    app.publish(ServerEvent::Settings);
    if changed {
        let app = app.clone();
        tokio::spawn(async move { crate::hosted::character_changed(&app).await });
    }
    Ok(out)
}

/// What the person's agent drew, as `core::character` reads it: for its page.
pub fn made(app: &Shared) -> (BTreeMap<String, Part>, BTreeMap<String, Character>) {
    Kept::of(&app.settings()).made()
}

// ---------------------------------------------------------------- the name

/// The person's first name, as their git config gives it, or their account
/// on the computer, if either does: for their agent's name until they give
/// it one.
pub fn first_name(home: &Path) -> Option<String> {
    let from_git = [home.join(".gitconfig"), home.join(".config/git/config")]
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .find_map(|text| git_user_name(&text));
    from_git.or_else(|| account_name(home)).and_then(|full| {
        let first = full.split_whitespace().next()?.to_string();
        // A name, not a login or an address.
        (first.chars().next()?.is_alphabetic() && !first.contains(['@', '/', '\\']))
            .then_some(first)
    })
}

/// `user.name` in a git config file.
fn git_user_name(text: &str) -> Option<String> {
    let mut in_user = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_user = line
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .eq_ignore_ascii_case("user");
            continue;
        }
        if !in_user {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim().eq_ignore_ascii_case("name") {
            let value = value.trim().trim_matches('"').trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// The full name of the account kelve runs as, where the system keeps one
/// in `/etc/passwd`: only for the person's own home, never a test's.
fn account_name(home: &Path) -> Option<String> {
    let real = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    if real != home {
        return None;
    }
    let user = std::env::var("USER").ok()?;
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        (fields.first() == Some(&user.as_str()))
            .then(|| {
                fields
                    .get(4)
                    .and_then(|g| g.split(',').next())
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
            .filter(|n| !n.is_empty())
    })
}

/// The agent's name: the one given, or "Sam’s agent"; and whether it was given.
fn name(app: &Shared, kept: &Kept) -> (String, bool) {
    match &kept.name {
        Some(n) => (n.clone(), true),
        None => (
            default_name(&first_name(&app.config.agents_home).unwrap_or_default()),
            false,
        ),
    }
}

// ---------------------------------------------------------------- views

fn timestamp(ms: i64) -> Timestamp {
    Timestamp(
        Utc.timestamp_millis_opt(ms)
            .single()
            .unwrap_or_else(Utc::now),
    )
}

/// The person's agent as a character.
pub fn view(app: &Shared) -> CharacterView {
    let kept = &Kept::of(&app.settings());
    let (parts, characters) = kept.made();
    let (name, named) = name(app, kept);
    let svg = kept.look.svg(&parts, &characters, Move::Idle, &name, false);
    let mut made: Vec<MadeView> = kept
        .parts
        .iter()
        .map(|(key, m)| {
            // A part on its own: the robot as the person has it, wearing it.
            let mut look = kept.look.clone();
            look.character = "robot".into();
            look.robot.parts.set(m.part.slot, key);
            MadeView {
                key: key.clone(),
                name: m.part.name.clone(),
                slot: Some(m.part.slot),
                svg: look.svg(&parts, &characters, Move::Still, &m.part.name, false),
                at: timestamp(m.at),
                part: Some(m.part.clone()),
                character: None,
            }
        })
        .chain(kept.characters.iter().map(|(key, m)| {
            let look = Look {
                character: key.clone(),
                ..kept.look.clone()
            };
            MadeView {
                key: key.clone(),
                name: m.character.name.clone(),
                slot: None,
                svg: look.svg(&parts, &characters, Move::Still, &m.character.name, false),
                at: timestamp(m.at),
                part: None,
                character: Some(m.character.clone()),
            }
        }))
        .collect();
    made.sort_by(|a, b| b.at.0.cmp(&a.at.0).then_with(|| a.key.cmp(&b.key)));
    CharacterView {
        name,
        named,
        look: kept.look.clone(),
        svg,
        made,
    }
}

/// Changes the agent's name or how it looks.
pub async fn set(app: &Shared, change_in: CharacterIn) -> ApiResult<CharacterView> {
    change(app, |kept| {
        if let Some(name) = change_in.name {
            let name = name.trim();
            kept.name = if name.is_empty() {
                None
            } else {
                if let Some(problem) = name_problem(name) {
                    return Err(ApiError::bad_request(problem));
                }
                Some(name.chars().take(MOST_NAME).collect())
            };
        }
        if let Some(look) = change_in.look {
            let (parts, characters) = kept.made();
            if let Some(problem) = look.problem(&parts, &characters) {
                return Err(ApiError::bad_request(problem));
            }
            kept.look = look;
        }
        Ok(())
    })
    .await?;
    Ok(view(app))
}

// ---------------------------------------------------------------- drawing

fn ask_key(id: &str) -> String {
    format!("draw:{id}")
}

/// Asks the person's agent to draw something: kept, and queued.
pub async fn draw(app: &Shared, asked: DrawIn) -> ApiResult<DrawingView> {
    let text = kelve_core::redact::redact(asked.asked.trim());
    if text.is_empty() {
        return Err(ApiError::bad_request(
            "Say what to draw: a party hat, say, or a cat.",
        ));
    }
    if text.chars().count() > MOST_ASKED_CHARS {
        return Err(ApiError::bad_request(format!(
            "That's longer than your agent needs. Say what to draw in under {MOST_ASKED_CHARS} characters."
        )));
    }
    let id = crate::crypto::install_id()[..12].to_string();
    let asked = Asked {
        id: id.clone(),
        asked: text,
        at: kelve_store::now(),
        made: None,
    };
    let kept = asked.clone();
    change(app, move |k| {
        k.asked.push(kept);
        // The oldest go first, finished or not.
        let over = k.asked.len().saturating_sub(MOST_ASKED);
        k.asked.drain(..over);
        Ok(())
    })
    .await?;
    v2jobs::queue(app, v2jobs::DRAW, &id, json!({ "id": id })).await?;
    Ok(DrawingView {
        id,
        asked: asked.asked,
        state: DrawingState::Waiting,
        made: None,
        why: None,
        at: timestamp(asked.at),
    })
}

/// The drawings the person asked for, the newest first, each where it is.
pub async fn drawings(app: &Shared) -> ApiResult<Vec<DrawingView>> {
    let kept = Kept::of(&app.settings());
    let mut conn = app.db.acquire().await?;
    let mut out = Vec::new();
    for asked in kept.asked.iter().rev() {
        let (state, why) = match &asked.made {
            Some(_) => (DrawingState::Done, None),
            None => match kelve_store::asks::by_key(&mut conn, &ask_key(&asked.id)).await? {
                None => (DrawingState::Waiting, None),
                Some(a) => match a.state.as_str() {
                    "waiting" => (DrawingState::Waiting, None),
                    "dropped" => (DrawingState::Failed, a.last_error.clone()),
                    // Taken, or answered and about to be kept.
                    _ => (DrawingState::Drawing, None),
                },
            },
        };
        out.push(DrawingView {
            id: asked.id.clone(),
            asked: asked.asked.clone(),
            state,
            made: asked.made.clone(),
            why,
            at: timestamp(asked.at),
        });
    }
    Ok(out)
}

/// The shape a drawing's answer takes.
fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["drawing"],
        "properties": {
            "drawing": {
                "type": "object",
                "description": "One JSON object: a part (kelve-part/1) or a whole character (kelve-character/1)."
            }
        }
    })
}

/// What the agent drew, as text to read, from its answer: an object, or the
/// same as a string of JSON.
fn drawn_text(value: &Value) -> Option<String> {
    match value.get("drawing")? {
        Value::String(s) => Some(s.clone()),
        v @ Value::Object(_) => Some(v.to_string()),
        _ => None,
    }
}

/// Whether an answer to a `draw` job is a drawing kelve can keep: why not.
pub fn shape(value: &Value) -> Result<(), String> {
    let text = drawn_text(value).ok_or_else(|| {
        "Answer with drawing: one JSON object, a part or a whole character.".to_string()
    })?;
    core::drawing(&text)
        .map(|_| ())
        .map_err(|problems| problems.join(" "))
}

/// The `v2.draw` job: asks the agent to draw what the person asked for, and
/// keeps what it drew once it's handed in.
pub async fn job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let id = payload["id"].as_str().unwrap_or_default().to_string();
    let kept = Kept::of(&app.settings());
    let Some(asked) = kept.asked.iter().find(|a| a.id == id).cloned() else {
        return Ok(Then::Done);
    };
    if asked.made.is_some() {
        return Ok(Then::Done);
    }
    let c = &kept.look.robot.colours;
    let material = json!({
        "asked": kelve_core::fence::fence("asked", &asked.asked),
        "colours": { "body": c.body, "light": c.light, "screen": c.screen, "eyes": c.eyes },
    });
    let ask = Ask {
        kind: DRAW,
        about: format!("Drawing {}", kelve_core::text::clip(&asked.asked, 80)),
        idea: None,
        opened: None,
        material,
        schema: schema(),
        priority: Priority::Now,
        follows: None,
    };
    let key = ask_key(&id);
    let Some(answer) = agent::ask(
        app,
        &key,
        ask,
        Resume::job(v2jobs::DRAW, id.clone(), payload.clone()),
    )
    .await?
    else {
        return Ok(Then::Done);
    };
    // Checked as it was handed in: read again, to keep.
    let Some(drawing) = drawn_text(&answer.value).and_then(|t| core::drawing(&t).ok()) else {
        tracing::warn!("a drawing kelve took no longer reads; dropped");
        return Ok(Then::Done);
    };
    let made_key = format!("made-{id}");
    let at = kelve_store::now();
    let kept = change(app, |k| {
        match drawing {
            Drawing::Part(part) => {
                k.parts.insert(made_key.clone(), MadePart { part, at });
            }
            Drawing::Character(character) => {
                k.characters
                    .insert(made_key.clone(), MadeCharacter { character, at });
            }
        }
        if let Some(a) = k.asked.iter_mut().find(|a| a.id == id) {
            a.made = Some(made_key.clone());
        }
        forget_oldest(k);
        Ok(())
    })
    .await;
    if let Err(e) = kept {
        tracing::warn!(error = ?e, "couldn't keep a drawing");
    }
    Ok(Then::Done)
}

/// Keeps what the agent made to `MOST_MADE`, the oldest going first, but
/// never one the person wears.
fn forget_oldest(k: &mut Kept) {
    let worn = |key: &str| {
        k.look.character == key
            || core::Slot::ALL
                .iter()
                .any(|s| k.look.robot.parts.get(*s) == key)
    };
    let mut all: Vec<(i64, String)> = k
        .parts
        .iter()
        .map(|(key, m)| (m.at, key.clone()))
        .chain(k.characters.iter().map(|(key, m)| (m.at, key.clone())))
        .filter(|(_, key)| !worn(key))
        .collect();
    let total = k.parts.len() + k.characters.len();
    all.sort();
    for (_, key) in all.into_iter().take(total.saturating_sub(MOST_MADE)) {
        k.parts.remove(&key);
        k.characters.remove(&key);
    }
}

// ---------------------------------------------------------------- your agent

/// A skill's title, as people read it: a kind of work's name for how you
/// like it, else its file's `#` title, else its name.
fn title(app: &Shared, skill: &kelve_store::skills::Skill) -> String {
    if skill.kind == "like"
        && let Some(work) = WorkKind::of(&skill.work)
    {
        return work.label().to_string();
    }
    let dir = if skill.root.is_empty() {
        app.config.skills_dir.clone()
    } else {
        crate::skills_home::project_dir(Path::new(&skill.root))
    };
    crate::skills_home::read(&dir, &skill.name)
        .ok()
        .flatten()
        .and_then(|text| kelve_core::skills::Skill::parse(&text).title())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| skill.name.clone())
}

fn check_of(state: &str) -> PartCheck {
    match state {
        "running" => PartCheck::Checking,
        "works" => PartCheck::Works,
        "no_difference" => PartCheck::NoDifference,
        "failed" => PartCheck::Failed,
        _ => PartCheck::NotChecked,
    }
}

/// What the agent knows: a row for each of the person's skills, how you
/// like it first, everywhere before any kind of work, a square for each part.
pub async fn knows(app: &Shared) -> ApiResult<Vec<KnowsRow>> {
    let mut conn = app.db.acquire().await?;
    let mut rows = Vec::new();
    for skill in kelve_store::skills::all(&mut conn, "made").await? {
        let kind = match skill.kind.as_str() {
            "like" => SkillKind::Like,
            "do" => SkillKind::Do,
            _ => SkillKind::Project,
        };
        let parts = kelve_store::skills::parts(&mut conn, skill.id, false)
            .await?
            .iter()
            .map(|p| check_of(&p.state))
            .collect();
        let order = match kind {
            SkillKind::Like if skill.work == "everywhere" => 0,
            SkillKind::Like => 1,
            SkillKind::Do => 2,
            SkillKind::Project => 3,
        };
        rows.push((
            order,
            KnowsRow {
                title: title(app, &skill),
                name: skill.name,
                kind,
                parts,
            },
        ));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.title.cmp(&b.1.title)));
    Ok(rows.into_iter().map(|(_, r)| r).collect())
}

/// Fits you so far: each blind pick, oldest first, and whether the person
/// picked their own agent's work. None till they've picked once.
pub async fn fits(app: &Shared) -> ApiResult<Option<FitsYou>> {
    let mut conn = app.db.acquire().await?;
    let mut picks = kelve_store::checks::picks(&mut conn, 1000).await?;
    picks.reverse();
    let tests: Vec<bool> = picks
        .iter()
        .filter_map(|p| p.picked.as_ref().map(|picked| *picked == p.skills_on))
        .collect();
    if tests.is_empty() {
        return Ok(None);
    }
    Ok(Some(FitsYou {
        picked: tests.iter().filter(|t| **t).count() as u32,
        of: tests.len() as u32,
        tests,
    }))
}

/// The Your agent page: its character, what it knows, and Fits you. Verified
/// scores and the page come with signing in to kelve.ai.
pub async fn page(app: &Shared) -> ApiResult<YourAgentView> {
    Ok(YourAgentView {
        character: view(app),
        knows: knows(app).await?,
        fits: fits(app).await?,
        scores: Vec::new(),
        page: None,
    })
}

// ---------------------------------------------------------------- settings

/// The agents whose settings hold kelve's hook.
fn connected(app: &Shared) -> Vec<ConnectedAgent> {
    Agent::ALL
        .into_iter()
        .filter(|a| crate::hooks::installed(*a, &app.config.agents_home, &app.config.claude_dir))
        .map(|a| ConnectedAgent {
            agent: a.name().to_string(),
            name: a.shown().to_string(),
        })
        .collect()
}

/// When this week began, Monday at midnight where the person is, in ms.
fn week_start(app: &Shared) -> i64 {
    let now = Utc::now().with_timezone(&app.config.offset);
    let monday = now.date_naive() - Duration::days(i64::from(now.weekday().num_days_from_monday()));
    monday
        .and_hms_opt(0, 0, 0)
        .and_then(|t| t.and_local_timezone(app.config.offset).single())
        .map_or(0, |t| t.timestamp_millis())
}

/// The settings for the person's agent, and for skill checks.
pub async fn settings(app: &Shared) -> ApiResult<AgentSettings> {
    let s = app.settings();
    let mut conn = app.db.acquire().await?;
    let this_week = kelve_store::your_agent::checks_since(&mut conn, week_start(app)).await?;
    Ok(AgentSettings {
        connected: connected(app),
        debriefs: s.debriefs,
        skills_folder: s.skills_folder,
        checks: s.checks,
        checks_a_week: s.checks_a_week,
        checks_this_week: u32::try_from(this_week).unwrap_or(u32::MAX),
    })
}

/// Changes them, or takes kelve's hook out of an agent's settings.
pub async fn set_settings(app: &Shared, change_in: AgentSettingsIn) -> ApiResult<AgentSettings> {
    if let Some(name) = &change_in.disconnect {
        let agent = Agent::parse(name).ok_or_else(|| {
            ApiError::bad_request("kelve doesn't know that agent. Reload the page, and try again.")
        })?;
        crate::hooks::set(
            agent,
            false,
            &app.config.agents_home,
            &app.config.claude_dir,
        )
        .map_err(|e| {
            tracing::warn!(
                agent = agent.name(),
                "couldn't take kelve's hook out of an agent's settings"
            );
            ApiError::bad_request(e)
        })?;
    }
    if let Some(n) = change_in.checks_a_week
        && !(1..=100).contains(&n)
    {
        return Err(ApiError::bad_request(
            "Choose between 1 and 100 check runs a week.",
        ));
    }
    {
        let _one = WRITING.lock().await;
        let mut s = (*app.settings()).clone();
        if let Some(d) = change_in.debriefs {
            s.debriefs = d;
        }
        if let Some(on) = change_in.skills_folder {
            s.skills_folder = on;
        }
        if let Some(c) = change_in.checks {
            s.checks = c;
        }
        if let Some(n) = change_in.checks_a_week {
            s.checks_a_week = n;
        }
        app.save_settings(s).await?;
    }
    app.publish(ServerEvent::Settings);
    settings(app).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_name_comes_from_git_config() {
        let config = "[core]\n\tname = not this\n[user]\n\temail = sam@example.com\n\tname = \"Sam Clark\"\n";
        assert_eq!(git_user_name(config).as_deref(), Some("Sam Clark"));
        assert_eq!(git_user_name("[user]\nemail = x@y.z\n"), None);
        let home =
            std::env::temp_dir().join(format!("kelve-test-{}", uuid::Uuid::now_v7().simple()));
        std::fs::create_dir_all(&home).unwrap_or_default();
        assert_eq!(
            first_name(&home),
            None,
            "a test's home is never the person's"
        );
        std::fs::write(home.join(".gitconfig"), config).unwrap_or_default();
        assert_eq!(first_name(&home).as_deref(), Some("Sam"));
        std::fs::write(home.join(".gitconfig"), "[user]\n name = sam@example.com\n")
            .unwrap_or_default();
        assert_eq!(first_name(&home), None, "an address isn't a name");
    }

    #[test]
    fn an_answer_is_a_drawing_as_an_object_or_as_text() {
        let hat = json!({
            "format": "kelve-part/1", "name": "Party hat", "slot": "hat", "at": [9, 0],
            "colours": { "a": "light", "g": "#f4c84a" }, "pixels": ["..gg..", "agggga"],
        });
        assert_eq!(shape(&json!({ "drawing": hat })), Ok(()));
        assert_eq!(shape(&json!({ "drawing": hat.to_string() })), Ok(()));
        assert!(
            shape(&json!({ "hat": hat }))
                .unwrap_err()
                .contains("Answer with drawing")
        );
        let mut wide = hat.clone();
        wide["at"] = json!([20, 0]);
        assert!(
            shape(&json!({ "drawing": wide }))
                .unwrap_err()
                .contains("outside its slot's box")
        );
    }
}
