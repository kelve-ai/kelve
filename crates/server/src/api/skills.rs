//! The skills pages (docs/skills.md, "Your skills"): the list in its four
//! groups, one skill with what each part's checks found, editing it, what
//! can be done with a part, suggested skills, checks, and Fits you. Every
//! route is the person's, signed in on this browser. Each reads the skills
//! on disk again first, so a skill edited by hand shows as it is.

use axum::Json;
use axum::extract::{Path, State};
use kelve_core::skills::compare::{split_part, wording_problem};
use kelve_core::skills::{self as core_skills, PartKey, SectionKind, Skill, SkillKind, WorkKind};
use kelve_core::views::{
    CheckProgress, CheckRun, FitsCloseness, FitsPick, FitsPicked, FitsView, FitsYou, PartCheck,
    PartEvidence, PartFailed, PartSource, ServerEvent, SkillChanged, SkillItem, SkillPartView,
    SkillSaved, SkillSectionView, SkillView, SkillsView, SuggestedItem, SuggestedSkill,
};
use kelve_store::{candidates, checking, checks, skills as kept};
use serde::Deserialize;

use crate::auth::Me;
use crate::error::{ApiError, ApiResult};
use crate::skill_checks;
use crate::skills_home;
use crate::skills_sync::{self, dir_of};
use crate::state::Shared;

/// How the app addresses a skill: its name for the person's own, its number
/// for a project's, whose names may repeat across projects.
fn id_of(skill: &kept::Skill) -> String {
    if skill.root.is_empty() {
        skill.name.clone()
    } else {
        skill.id.to_string()
    }
}

/// The skill the app addresses, read again from disk first.
async fn find(app: &Shared, id: &str) -> ApiResult<kept::Skill> {
    // One of the person's own, made by hand since kelve last looked.
    if core_skills::name_problem(id).is_none() {
        skills_sync::sync_skill(app, "", id).await?;
    }
    let mut conn = app.db.acquire().await?;
    let skill = match kept::by_name(&mut conn, "", id).await? {
        Some(s) if s.state == "made" => Some(s),
        _ => match id.parse::<i64>() {
            Ok(n) => kept::get(&mut conn, n).await?.filter(|s| s.state == "made"),
            Err(_) => None,
        },
    };
    drop(conn);
    let missing =
        || ApiError::not_found("There’s no skill here. It may have been taken out of its folder.");
    let skill = skill.ok_or_else(missing)?;
    skills_sync::sync_skill(app, &skill.root, &skill.name).await?;
    let mut conn = app.db.acquire().await?;
    kept::get(&mut conn, skill.id).await?.ok_or_else(missing)
}

/// A skill's text on disk.
fn text_of(app: &Shared, skill: &kept::Skill) -> ApiResult<String> {
    skills_home::read(&dir_of(app, &skill.root), &skill.name)
        .map_err(ApiError::internal)?
        .ok_or_else(|| {
            ApiError::not_found("There’s no skill here. It may have been taken out of its folder.")
        })
}

/// Writes a skill's text, and reads it back into the store.
async fn write(app: &Shared, skill: &kept::Skill, text: &str) -> ApiResult<()> {
    skills_home::write(&dir_of(app, &skill.root), &skill.name, text).map_err(ApiError::internal)?;
    skills_sync::sync_skill(app, &skill.root, &skill.name).await?;
    app.publish(ServerEvent::Skills);
    Ok(())
}

fn kind_of(kind: &str) -> SkillKind {
    match kind {
        "like" => SkillKind::Like,
        "project" => SkillKind::Project,
        _ => SkillKind::Do,
    }
}

/// What people read a skill as: its title, else its kind of work's name,
/// else its name.
fn title_of(skill: &kept::Skill, text: Option<&str>) -> String {
    text.and_then(|t| Skill::parse(t).title())
        .filter(|t| !t.trim().is_empty())
        .or_else(|| WorkKind::of(&skill.work).map(|w| w.label().to_string()))
        .unwrap_or_else(|| skill.name.clone())
}

/// A project's name: its folder's.
fn project_of(root: &str) -> Option<String> {
    (!root.is_empty()).then(|| {
        std::path::Path::new(root)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.to_string())
    })
}

async fn item(app: &Shared, skill: &kept::Skill) -> ApiResult<SkillItem> {
    let text = skills_home::read(&dir_of(app, &skill.root), &skill.name)
        .ok()
        .flatten();
    let mut conn = app.db.acquire().await?;
    let parts = kept::parts(&mut conn, skill.id, false).await?;
    Ok(SkillItem {
        id: id_of(skill),
        name: skill.name.clone(),
        title: title_of(skill, text.as_deref()),
        kind: kind_of(&skill.kind),
        work: WorkKind::of(&skill.work),
        project: project_of(&skill.root),
        parts: parts.len() as u32,
        failed: parts.iter().filter(|p| p.state == "failed").count() as u32,
        checking: parts.iter().any(|p| p.state == "running"),
    })
}

/// The skills page's list, in its four groups: suggested, how you like it
/// (everywhere first), how you do something, and one project.
pub async fn list(State(app): State<Shared>, _me: Me) -> ApiResult<Json<SkillsView>> {
    skills_sync::sync(&app).await?;
    let mut conn = app.db.acquire().await?;
    let made = kept::all(&mut conn, "made").await?;
    let suggested = kept::all(&mut conn, "suggested").await?;
    drop(conn);
    let mut view = SkillsView {
        suggested: suggested
            .iter()
            .map(|s| SuggestedItem {
                id: s.id.to_string(),
                title: title_of(s, s.draft.as_deref()),
            })
            .collect(),
        ..Default::default()
    };
    for skill in &made {
        let item = item(&app, skill).await?;
        match item.kind {
            SkillKind::Like => view.like.push(item),
            SkillKind::Do => view.doing.push(item),
            SkillKind::Project => view.project.push(item),
        }
    }
    view.like
        .sort_by_key(|i| (i.work != Some(WorkKind::Everywhere), i.title.to_lowercase()));
    view.doing.sort_by_key(|i| i.title.to_lowercase());
    view.project
        .sort_by_key(|i| (i.project.clone(), i.title.to_lowercase()));
    Ok(Json(view))
}

/// A part's state as the page shows it.
fn check_of(state: &str) -> PartCheck {
    match state {
        "running" => PartCheck::Checking,
        "works" => PartCheck::Works,
        "no_difference" => PartCheck::NoDifference,
        "failed" => PartCheck::Failed,
        _ => PartCheck::NotChecked,
    }
}

fn source_of(kind: &str) -> PartSource {
    match kind {
        "message" => PartSource::Said,
        "edit" => PartSource::Edit,
        _ => PartSource::Debrief,
    }
}

/// A skill's sections, each part with what its checks found, when the
/// store knows it.
async fn sections_of(
    app: &Shared,
    skill: Option<&kept::Skill>,
    text: &str,
) -> ApiResult<Vec<SkillSectionView>> {
    let parsed = Skill::parse(text);
    let mut conn = app.db.acquire().await?;
    let stored = match skill {
        Some(s) => kept::parts(&mut conn, s.id, false).await?,
        None => Vec::new(),
    };
    let name = skill.map_or_else(|| parsed.name().unwrap_or_default(), |s| s.name.clone());
    let mut out = Vec::new();
    for section in parsed.sections() {
        let mut parts = Vec::new();
        for p in &section.parts {
            let row = stored.iter().find(|s| s.key == p.key);
            let check = row.map_or(PartCheck::NotChecked, |r| check_of(&r.state));
            let progress = match row {
                Some(r) if check == PartCheck::Checking => {
                    let (done, of) = checking::progress(&mut conn, r.id).await?;
                    Some(CheckProgress {
                        done: done as u32,
                        of: of as u32,
                    })
                }
                _ => None,
            };
            let failed = match row {
                Some(r) if check == PartCheck::Failed => {
                    let what = checking::failures(&mut conn, r.id)
                        .await?
                        .into_iter()
                        .map(|(notes, _)| notes)
                        .find(|n| !n.is_empty())
                        .unwrap_or_else(|| "Your agent still didn’t do what it says.".into());
                    r.wording.clone().map(|wording| PartFailed {
                        what: format!("{what} Clearer wording:"),
                        wording,
                    })
                }
                _ => None,
            };
            let evidence = match row {
                Some(r) => candidates::evidence_for_part(&mut conn, r.id)
                    .await?
                    .into_iter()
                    .rev()
                    .map(|e| PartEvidence {
                        source: source_of(&e.kind),
                        text: e.text,
                        at: kelve_core::Timestamp(kelve_store::at(e.at)),
                        debrief: None,
                    })
                    .collect(),
                None => Vec::new(),
            };
            parts.push(SkillPartView {
                key: PartKey::new(&name, &p.heading).to_string(),
                heading: p.heading.clone(),
                markdown: p.markdown.clone(),
                line: p.line as u32,
                check,
                kept: row.is_some_and(|r| r.kept),
                hint: row.and_then(|r| r.note.clone()).unwrap_or_default(),
                progress,
                failed,
                evidence,
            });
        }
        out.push(SkillSectionView {
            title: section.title.clone(),
            kind: section.kind,
            parts,
        });
    }
    Ok(out)
}

async fn view(app: &Shared, skill: &kept::Skill) -> ApiResult<SkillView> {
    let text = text_of(app, skill)?;
    let parsed = Skill::parse(&text);
    Ok(SkillView {
        id: id_of(skill),
        name: skill.name.clone(),
        title: title_of(skill, Some(&text)),
        intro: parsed.intro(),
        description: parsed.description().unwrap_or_default(),
        kind: kind_of(&skill.kind),
        work: WorkKind::of(&skill.work),
        project: project_of(&skill.root),
        sections: sections_of(app, Some(skill), &text).await?,
        lines: parsed.lines() as u32,
        most_lines: core_skills::MOST_LINES as u32,
        problems: core_skills::check(&text),
        offer: skill_checks::offer(app, skill).await?,
        running: skill_checks::running(app, skill.id).await?,
        markdown: text,
    })
}

/// One skill, rendered and as its `SKILL.md`, with what each part's checks
/// found.
pub async fn one(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
) -> ApiResult<Json<SkillView>> {
    let skill = find(&app, &id).await?;
    Ok(Json(view(&app, &skill).await?))
}

#[derive(Debug, Deserialize)]
pub struct MarkdownIn {
    pub markdown: String,
}

/// How many of a skill's parts aren't checked yet.
async fn unchecked(app: &Shared, skill: i64) -> ApiResult<u32> {
    let mut conn = app.db.acquire().await?;
    Ok(kept::parts(&mut conn, skill, false)
        .await?
        .iter()
        .filter(|p| p.state == "unchecked")
        .count() as u32)
}

fn not_saved(problems: Vec<String>, id: String) -> Json<SkillSaved> {
    Json(SkillSaved {
        saved: false,
        problems,
        id,
        unchecked: 0,
    })
}

/// Saves a skill as the person wrote it, once kelve's checks find nothing
/// wrong with it: else what's wrong, and nothing's written.
pub async fn save(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
    Json(input): Json<MarkdownIn>,
) -> ApiResult<Json<SkillSaved>> {
    let skill = find(&app, &id).await?;
    let mut problems = core_skills::check(&input.markdown);
    if let Some(name) = Skill::parse(&input.markdown).name()
        && name.trim() != skill.name
        && core_skills::name_problem(&name).is_none()
    {
        problems.push(format!(
            "The name has to stay “{}”, as its folder is called. To rename a skill, make a new one.",
            skill.name
        ));
    }
    if !problems.is_empty() {
        return Ok(not_saved(problems, id_of(&skill)));
    }
    write(&app, &skill, &input.markdown).await?;
    Ok(Json(SkillSaved {
        saved: true,
        problems: Vec::new(),
        id: id_of(&skill),
        unchecked: unchecked(&app, skill.id).await?,
    }))
}

/// Makes a new skill of the person's own, as they wrote it.
pub async fn create(
    State(app): State<Shared>,
    _me: Me,
    Json(input): Json<MarkdownIn>,
) -> ApiResult<Json<SkillSaved>> {
    let mut problems = core_skills::check(&input.markdown);
    let name = Skill::parse(&input.markdown).name().unwrap_or_default();
    let name = name.trim();
    if core_skills::name_problem(name).is_none()
        && skills_home::read(&app.config.skills_dir, name)
            .map_err(ApiError::internal)?
            .is_some()
    {
        problems.push(format!(
            "You have a skill called “{name}” already. Give this one another name, or change that one."
        ));
    }
    if !problems.is_empty() {
        return Ok(not_saved(problems, String::new()));
    }
    skills_home::write(&app.config.skills_dir, name, &input.markdown)
        .map_err(ApiError::internal)?;
    let id = skills_sync::sync_skill(&app, "", name)
        .await?
        .ok_or_else(|| ApiError::internal("a skill just written couldn't be read back"))?;
    app.publish(ServerEvent::Skills);
    Ok(Json(SkillSaved {
        saved: true,
        problems: Vec::new(),
        id: name.to_string(),
        unchecked: unchecked(&app, id).await?,
    }))
}

#[derive(Debug, Deserialize)]
pub struct PartIn {
    /// The part's key, as the page has it.
    pub key: String,
    /// `keep` (keep it anyway), `unkeep`, `wording` (use the agent's
    /// clearer wording), `own` (the person's own words) or `remove`.
    pub act: String,
    /// For `own`: the part as they wrote it, its `###` heading first.
    pub markdown: Option<String>,
}

/// Does what the person chose for a part: keeps it though it makes no
/// difference, puts other words in its place, or takes it out. What it came
/// to, with the file before and after, to undo it.
pub async fn part(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
    Json(input): Json<PartIn>,
) -> ApiResult<Json<SkillChanged>> {
    let skill = find(&app, &id).await?;
    let title = title_of(&skill, None);
    let text = text_of(&app, &skill)?;
    let title = Skill::parse(&text).title().unwrap_or(title);
    let heading_key = input
        .key
        .split_once('#')
        .map_or(input.key.as_str(), |(_, h)| h);
    let mut conn = app.db.acquire().await?;
    let row = kept::parts(&mut conn, skill.id, false)
        .await?
        .into_iter()
        .find(|p| p.key == core_skills::normalise(heading_key))
        .ok_or_else(|| {
            ApiError::not_found(
                "That part isn’t in the skill any more. Open it again to see it as it is.",
            )
        })?;
    drop(conn);
    let mut parsed = Skill::parse(&text);
    let said = match input.act.as_str() {
        "keep" | "unkeep" => {
            let mut conn = app.db.acquire().await?;
            if input.act == "keep" {
                kept::keep_anyway(&mut conn, row.id).await?;
            } else {
                checking::unkeep(&mut conn, row.id).await?;
            }
            drop(conn);
            app.publish(ServerEvent::Skills);
            let said = if input.act == "keep" {
                format!("Kept in {title}.")
            } else {
                "It’s back as it was.".to_string()
            };
            return Ok(Json(SkillChanged {
                said,
                before: None,
                after: None,
            }));
        }
        "wording" => {
            let wording = row
                .wording
                .clone()
                .filter(|_| row.state == "failed")
                .ok_or_else(|| ApiError::conflict("There’s no clearer wording for that part."))?;
            let (heading, body) = split_part(&wording)
                .ok_or_else(|| ApiError::conflict("There’s no clearer wording for that part."))?;
            parsed
                .replace_part(&row.heading, &heading, body)
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            format!("{title} has the clearer wording.")
        }
        "own" => {
            let markdown = input.markdown.unwrap_or_default();
            wording_problem(&markdown).map_err(ApiError::bad_request)?;
            let (heading, body) = split_part(&markdown).ok_or_else(|| {
                ApiError::bad_request(
                    "Start the part with a `###` heading that is the rule itself.",
                )
            })?;
            parsed
                .replace_part(&row.heading, &heading, body)
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            format!("Saved to {title}.")
        }
        "remove" => {
            parsed
                .remove_part(&row.heading)
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            format!("Removed from {title}.")
        }
        _ => {
            return Err(ApiError::bad_request(
                "kelve can keep a part, use another wording for it, or take it out.",
            ));
        }
    };
    let after = parsed.write();
    write(&app, &skill, &after).await?;
    Ok(Json(SkillChanged {
        said,
        before: Some(text),
        after: Some(after),
    }))
}

#[derive(Debug, Deserialize)]
pub struct UndoIn {
    pub before: String,
    pub after: String,
}

/// Puts a skill back as it was before a change, unless it's changed again
/// since.
pub async fn undo(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
    Json(input): Json<UndoIn>,
) -> ApiResult<Json<SkillChanged>> {
    let skill = find(&app, &id).await?;
    let text = text_of(&app, &skill)?;
    if text != input.after {
        return Err(ApiError::conflict(
            "The skill changed since, so it wasn’t put back. Open it again to see it as it is.",
        ));
    }
    write(&app, &skill, &input.before).await?;
    Ok(Json(SkillChanged {
        said: "It’s back as it was.".into(),
        before: None,
        after: None,
    }))
}

#[derive(Debug, Default, Deserialize)]
pub struct CheckIn {
    /// The parts to check, by key; all those not checked yet when empty.
    #[serde(default)]
    pub keys: Vec<String>,
}

/// Starts checking a skill's parts not checked yet: the person agreed, with
/// what it takes in front of them.
pub async fn check(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
    Json(input): Json<CheckIn>,
) -> ApiResult<Json<CheckRun>> {
    let skill = find(&app, &id).await?;
    Ok(Json(skill_checks::start(&app, &skill, &input.keys).await?))
}

/// Stops a skill's check.
pub async fn stop(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
) -> ApiResult<Json<SkillChanged>> {
    let skill = find(&app, &id).await?;
    skill_checks::stop(&app, &skill).await?;
    Ok(Json(SkillChanged {
        said: "Stopped. Those parts aren’t checked yet.".into(),
        before: None,
        after: None,
    }))
}

/// A suggestion by its number, while it waits for the person.
async fn suggestion(app: &Shared, id: i64) -> ApiResult<kept::Skill> {
    let mut conn = app.db.acquire().await?;
    kept::get(&mut conn, id)
        .await?
        .filter(|s| s.state == "suggested" && s.draft.is_some())
        .ok_or_else(|| ApiError::not_found("That suggestion isn’t waiting any more."))
}

/// A suggested skill, drafted in full, with a skill the person has whose
/// description is close, to add to instead.
pub async fn suggested(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<i64>,
) -> ApiResult<Json<SuggestedSkill>> {
    skills_sync::sync(&app).await?;
    let s = suggestion(&app, id).await?;
    let draft = s.draft.clone().unwrap_or_default();
    let parsed = Skill::parse(&draft);
    let description = parsed.description().unwrap_or_default();
    let mut conn = app.db.acquire().await?;
    let made = kept::all(&mut conn, "made").await?;
    drop(conn);
    let mut descriptions = Vec::new();
    for m in &made {
        if let Ok(Some(text)) = skills_home::read(&dir_of(&app, &m.root), &m.name) {
            descriptions.push((m, Skill::parse(&text).description().unwrap_or_default()));
        }
    }
    let pairs: Vec<(&str, &str)> = descriptions
        .iter()
        .map(|(m, d)| (m.name.as_str(), d.as_str()))
        .collect();
    let close_to = match core_skills::closest(&description, &pairs, &[]) {
        Some((name, _)) => match descriptions.iter().find(|(m, _)| m.name == name) {
            Some((m, _)) => Some(item(&app, m).await?),
            None => None,
        },
        None => None,
    };
    Ok(Json(SuggestedSkill {
        id: s.id.to_string(),
        name: s.name.clone(),
        title: title_of(&s, Some(&draft)),
        description,
        kind: kind_of(&s.kind),
        why: s.why.clone().unwrap_or_default(),
        sections: sections_of(&app, None, &draft).await?,
        markdown: draft,
        close_to,
    }))
}

#[derive(Debug, Deserialize)]
pub struct DecideIn {
    /// `create`, `add` (to the skill `to`) or `not_now`.
    pub act: String,
    pub to: Option<String>,
}

/// What the person chose for a suggested skill: make it, add its parts to a
/// skill they have instead, or not now. Where it went, as the app addresses
/// it.
pub async fn decide(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<i64>,
    Json(input): Json<DecideIn>,
) -> ApiResult<Json<SkillSaved>> {
    let s = suggestion(&app, id).await?;
    let draft = s.draft.clone().unwrap_or_default();
    match input.act.as_str() {
        "create" => {
            let problems = core_skills::check(&draft);
            if !problems.is_empty() {
                return Ok(not_saved(problems, String::new()));
            }
            skills_home::write(&dir_of(&app, &s.root), &s.name, &draft)
                .map_err(ApiError::internal)?;
            let mut conn = app.db.acquire().await?;
            kept::set_state(&mut conn, s.id, "made").await?;
            drop(conn);
            skills_sync::sync_skill(&app, &s.root, &s.name).await?;
            app.publish(ServerEvent::Skills);
            let mut conn = app.db.acquire().await?;
            let made = kept::get(&mut conn, s.id)
                .await?
                .ok_or_else(|| ApiError::internal("a skill just made is gone"))?;
            drop(conn);
            Ok(Json(SkillSaved {
                saved: true,
                problems: Vec::new(),
                id: id_of(&made),
                unchecked: unchecked(&app, made.id).await?,
            }))
        }
        "add" => {
            let to = find(&app, input.to.as_deref().unwrap_or_default()).await?;
            let mut target = Skill::parse(&text_of(&app, &to)?);
            let from = Skill::parse(&draft);
            for section in from.sections() {
                let kind = section.kind.unwrap_or(SectionKind::HowYouLikeIt);
                for p in section.parts {
                    match target.add_part(kind, &core_skills::Place::End, &p.heading, &p.body) {
                        Ok(()) | Err(core_skills::EditError::Exists(_)) => {}
                        Err(e) => return Err(ApiError::bad_request(e.to_string())),
                    }
                }
            }
            write(&app, &to, &target.write()).await?;
            let mut conn = app.db.acquire().await?;
            kept::forget(&mut conn, s.id).await?;
            drop(conn);
            Ok(Json(SkillSaved {
                saved: true,
                problems: Vec::new(),
                id: id_of(&to),
                unchecked: unchecked(&app, to.id).await?,
            }))
        }
        "not_now" => {
            let mut conn = app.db.acquire().await?;
            kept::set_state(&mut conn, s.id, "not_now").await?;
            drop(conn);
            app.publish(ServerEvent::Skills);
            Ok(Json(SkillSaved {
                saved: true,
                problems: Vec::new(),
                id: String::new(),
                unchecked: 0,
            }))
        }
        _ => Err(ApiError::bad_request(
            "A suggestion can be made a skill, added to one you have, or left for now.",
        )),
    }
}

/// A share of what was kept, in percent.
fn percent(close: f64) -> u32 {
    (close * 100.0).round().clamp(0.0, 100.0) as u32
}

/// Fits you's page: the next pair to pick between, blind, and the tally.
pub async fn fits(State(app): State<Shared>, _me: Me) -> ApiResult<Json<FitsView>> {
    let mut conn = app.db.acquire().await?;
    let pick = match checking::next_pair(&mut conn).await? {
        Some(pair) => {
            let request = checks::task(&mut conn, pair.task)
                .await?
                .map(|t| t.request)
                .unwrap_or_default();
            let mine = pair.skills_diff.unwrap_or_default();
            let theirs = pair.standard_diff.unwrap_or_default();
            let (left, right) = if pair.skills_on == "left" {
                (mine, theirs)
            } else {
                (theirs, mine)
            };
            Some(FitsPick {
                id: pair.id,
                request,
                left,
                right,
            })
        }
        None => None,
    };
    let tests = checking::picked_own(&mut conn).await?;
    let view = FitsView {
        pick,
        waiting: checking::pairs_waiting(&mut conn).await? as u32,
        tasks: checking::tasks_known(&mut conn).await? as u32,
        tally: FitsYou {
            picked: tests.iter().filter(|t| **t).count() as u32,
            of: tests.len() as u32,
            tests,
        },
        closeness: checking::closeness_so_far(&mut conn)
            .await?
            .map(|(skills, standard)| FitsCloseness {
                skills: percent(skills),
                standard: percent(standard),
            }),
    };
    Ok(Json(view))
}

/// Starts Fits you on a few past tasks.
pub async fn fits_start(State(app): State<Shared>, me: Me) -> ApiResult<Json<FitsView>> {
    skill_checks::fits_start(&app).await?;
    fits(State(app), me).await
}

#[derive(Debug, Deserialize)]
pub struct PickIn {
    /// `left`, `right` or `same`.
    pub picked: String,
}

/// The side the person would rather keep, or neither: which was theirs,
/// and how close each came to what they kept.
pub async fn fits_pick(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<i64>,
    Json(input): Json<PickIn>,
) -> ApiResult<Json<FitsPicked>> {
    if !matches!(input.picked.as_str(), "left" | "right" | "same") {
        return Err(ApiError::bad_request(
            "Pick left, right, or about the same.",
        ));
    }
    let mut conn = app.db.acquire().await?;
    let pair = checking::pair(&mut conn, id)
        .await?
        .filter(|p| p.skills_diff.is_some())
        .ok_or_else(|| ApiError::not_found("That pair isn’t here any more."))?;
    if !checks::pick(&mut conn, id, &input.picked).await? {
        return Err(ApiError::conflict("You picked one of this pair already."));
    }
    drop(conn);
    app.publish(ServerEvent::Skills);
    Ok(Json(FitsPicked {
        mine: pair.skills_on,
        picked: input.picked,
        skills_close: pair.skills_close.map(percent),
        standard_close: pair.standard_close.map(percent),
    }))
}
