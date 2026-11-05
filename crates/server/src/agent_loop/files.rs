//! The person's skill files as the agent loop changes them: finding them,
//! keeping the store in step with one (its parts, by heading), writing an
//! agreed part into its place, and undoing that. The files are the source of
//! truth: an edit changes only the part it's about, and the person's own
//! edits are never changed back.

use std::path::{Path, PathBuf};

use kelve_core::skills::{self, Place, SectionKind, Skill, SkillKind, WorkKind};
use kelve_store::agent_loop::{self as store, NewEdit};
use kelve_store::{candidates, skills as kept};

use crate::error::{ApiError, ApiResult};
use crate::skills_home;
use crate::state::Shared;

/// A skill on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnDisk {
    /// The project it's in, absolute, or '' for the person's own.
    pub root: String,
    pub name: String,
    /// Its `SKILL.md`.
    pub path: PathBuf,
    pub text: String,
}

impl OnDisk {
    /// How a session is said to have had it.
    pub fn key(&self) -> String {
        format!("{}|{}", self.root, self.name)
    }

    pub fn kind(&self) -> SkillKind {
        kind_of(&self.root, &self.name)
    }

    /// Its description, as its front matter says.
    pub fn description(&self) -> String {
        Skill::parse(&self.text).description().unwrap_or_default()
    }
}

/// The folder a skill's folder is in: the person's own, or a project's.
pub fn dir_for(app: &Shared, root: &str) -> PathBuf {
    if root.is_empty() {
        app.config.skills_dir.clone()
    } else {
        skills_home::project_dir(Path::new(root))
    }
}

/// A skill's kind, from where it is and its name: a project's, the "how you
/// like it" skill for a kind of work, or how to do something.
pub fn kind_of(root: &str, name: &str) -> SkillKind {
    if !root.is_empty() {
        SkillKind::Project
    } else if WorkKind::of(name).is_some_and(|k| k.name() == name) {
        SkillKind::Like
    } else {
        SkillKind::Do
    }
}

fn kind_word(kind: SkillKind) -> &'static str {
    match kind {
        SkillKind::Like => "like",
        SkillKind::Do => "do",
        SkillKind::Project => "project",
    }
}

/// One skill, when it's on disk.
pub fn one(app: &Shared, root: &str, name: &str) -> Option<OnDisk> {
    let dir = dir_for(app, root);
    let text = skills_home::read(&dir, name).ok().flatten()?;
    Some(OnDisk {
        root: root.to_string(),
        name: name.to_string(),
        path: dir.join(name).join(skills_home::FILE),
        text,
    })
}

/// Every skill in a folder, read.
fn all_in(app: &Shared, root: &str) -> Vec<OnDisk> {
    let dir = dir_for(app, root);
    let found = match skills_home::list(&dir) {
        Ok(found) => found,
        Err(e) => {
            tracing::debug!(error = %e, "couldn't list skills");
            return Vec::new();
        }
    };
    found
        .into_iter()
        .filter_map(|f| one(app, root, &f.name))
        .collect()
}

/// The person's own skills, and the project's when there is one.
pub fn all(app: &Shared, project: Option<&str>) -> Vec<OnDisk> {
    let mut out = all_in(app, "");
    if let Some(root) = project.filter(|r| !r.is_empty()) {
        out.extend(all_in(app, root));
    }
    out
}

/// The section word the store keeps a part under.
fn section_word(title: Option<&str>) -> &'static str {
    match title.and_then(SectionKind::of) {
        Some(SectionKind::HowYouLikeIt) => "like",
        Some(SectionKind::Steps) => "steps",
        Some(SectionKind::WatchOutFor) => "watch",
        None => "",
    }
}

/// The section a candidate's word names.
pub fn section_of(word: &str) -> SectionKind {
    match word {
        "steps" => SectionKind::Steps,
        "watch" => SectionKind::WatchOutFor,
        _ => SectionKind::HowYouLikeIt,
    }
}

/// The store's word for a section.
pub fn section_kept(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::HowYouLikeIt => "like",
        SectionKind::Steps => "steps",
        SectionKind::WatchOutFor => "watch",
    }
}

/// Keeps the store in step with a skill's file: the skill, and its parts as
/// they read now. Its id, or none when the file isn't there.
pub async fn sync(app: &Shared, root: &str, name: &str) -> ApiResult<Option<i64>> {
    let Some(disk) = one(app, root, name) else {
        return Ok(None);
    };
    let skill = Skill::parse(&disk.text);
    let parts = skill.parts();
    let hashes: Vec<String> = parts
        .iter()
        .map(|p| kelve_core::text::hash(&p.markdown))
        .collect();
    let mut conn = app.db.acquire().await?;
    let had = kept::by_name(&mut conn, root, name).await?;
    let kind = had
        .as_ref()
        .map(|s| s.kind.clone())
        .unwrap_or_else(|| kind_word(disk.kind()).to_string());
    let work = had.as_ref().map(|s| s.work.clone()).unwrap_or_else(|| {
        if kind == "like" {
            name.to_string()
        } else {
            String::new()
        }
    });
    let id = kept::add(
        &mut conn,
        &kept::NewSkill {
            name,
            kind: &kind,
            work: &work,
            root,
            suggested: None,
        },
    )
    .await?;
    let seen: Vec<kept::SeenPart<'_>> = parts
        .iter()
        .zip(&hashes)
        .map(|(p, h)| kept::SeenPart {
            key: &p.key,
            heading: &p.heading,
            section: section_word(p.section.as_deref()),
            body_hash: h,
        })
        .collect();
    kept::seen(&mut conn, id, &seen).await?;
    Ok(Some(id))
}

/// A part's id in a skill kept, by its heading.
pub async fn part_id(app: &Shared, skill: i64, heading: &str) -> ApiResult<Option<i64>> {
    let key = skills::normalise(heading);
    let mut conn = app.db.acquire().await?;
    Ok(kept::parts(&mut conn, skill, false)
        .await?
        .into_iter()
        .find(|p| p.key == key)
        .map(|p| p.id))
}

/// A part's markdown, as a candidate keeps it: its heading and body.
pub fn part_markdown(heading: &str, body: &str) -> String {
    format!("### {}\n{}\n", heading.trim(), body.trim())
}

/// A candidate's body, without its heading line.
fn body_of(markdown: &str) -> String {
    let t = markdown.trim();
    match t.split_once('\n') {
        Some((first, rest)) if first.trim_start().starts_with("###") => rest.trim().to_string(),
        _ if t.starts_with("###") => String::new(),
        _ => t.to_string(),
    }
}

/// Where a candidate goes: the project it's in ('' for the person's own),
/// the skill's name, and the text to start from when the file isn't there.
async fn target(app: &Shared, c: &candidates::Candidate) -> ApiResult<(String, String, String)> {
    if let Some(id) = c.skill {
        let mut conn = app.db.acquire().await?;
        let s = kept::get(&mut conn, id)
            .await?
            .ok_or_else(|| ApiError::not_found("That skill isn't kept any more."))?;
        let start = match (s.kind.as_str(), &s.draft) {
            (_, Some(draft)) => draft.clone(),
            ("like", None) => WorkKind::of(&s.work)
                .or_else(|| WorkKind::of(&s.name))
                .map(kelve_core::learn::like_draft)
                .unwrap_or_default(),
            _ => String::new(),
        };
        return Ok((s.root, s.name, start));
    }
    let kind = WorkKind::of(&c.work).unwrap_or(WorkKind::Everywhere);
    Ok((
        String::new(),
        kind.name().to_string(),
        kelve_core::learn::like_draft(kind),
    ))
}

/// Writes an agreed candidate into its skill, in its place, and keeps the
/// change so it can be undone: its id. A part that says what one already
/// says changes nothing in the file: the part gets the evidence. A skill
/// that would grow past its limit isn't changed, and the person is told.
pub async fn apply(app: &Shared, candidate: i64) -> ApiResult<Option<i64>> {
    let c = {
        let mut conn = app.db.acquire().await?;
        candidates::get(&mut conn, candidate)
            .await?
            .ok_or_else(|| ApiError::not_found("That suggestion isn't kept any more."))?
    };
    if c.state == "accepted" {
        let mut conn = app.db.acquire().await?;
        return Ok(store::edit_of(&mut conn, candidate).await?.map(|e| e.id));
    }
    if c.relation == "same" {
        let mut conn = app.db.acquire().await?;
        candidates::decide(&mut conn, candidate, "accepted", c.part).await?;
        return Ok(None);
    }
    let (root, name, start) = target(app, &c).await?;
    let existed = one(app, &root, &name).map(|d| d.text);
    let made = existed.is_none();
    let before = existed.unwrap_or(start);
    if before.trim().is_empty() {
        return Err(ApiError::conflict(
            "That skill isn't there any more, so kelve can't add to it.",
        ));
    }
    let mut skill = Skill::parse(&before);
    let heading = c.heading.trim().to_string();
    let body = body_of(&c.body);
    let (relation, old) = match (c.relation.as_str(), c.part) {
        ("disagrees" | "rewords", Some(part)) => {
            let mut conn = app.db.acquire().await?;
            let old = kept::part(&mut conn, part).await?;
            drop(conn);
            let old_heading = old.map(|p| p.heading).unwrap_or_default();
            let old_body = skill.part(&old_heading).map(|p| p.body).unwrap_or_default();
            skill
                .replace_part(&old_heading, &heading, &body)
                .map_err(|e| ApiError::conflict(e.to_string()))?;
            ("replaced", Some((old_heading, old_body)))
        }
        _ => {
            let after = {
                let mut conn = app.db.acquire().await?;
                store::after_of(&mut conn, candidate).await?
            };
            let section = section_of(&c.section);
            let placed = match after.filter(|a| skill.part(a).is_some()) {
                Some(a) => skill.add_part(section, &Place::After(a), &heading, &body),
                None => skill.add_part(section, &Place::End, &heading, &body),
            };
            match placed {
                Ok(()) => ("new", None),
                // Already there, as the person or another candidate wrote it.
                Err(skills::EditError::Exists(_)) => {
                    let id = sync(app, &root, &name).await?;
                    let into = match id {
                        Some(id) => part_id(app, id, &heading).await?,
                        None => None,
                    };
                    let mut conn = app.db.acquire().await?;
                    candidates::decide(&mut conn, candidate, "accepted", into).await?;
                    return Ok(None);
                }
                Err(e) => return Err(ApiError::conflict(e.to_string())),
            }
        }
    };
    let after = skill.write();
    if skill.lines() > skills::MOST_LINES {
        return Err(ApiError::conflict(format!(
            "That would take the {name} skill past {} lines, so kelve left it as it is. Take out a part that makes no difference, or move a long example into references/, then add it again.",
            skills::MOST_LINES
        )));
    }
    let dir = dir_for(app, &root);
    skills_home::write(&dir, &name, &after).map_err(|e| {
        tracing::warn!(error = %e, "couldn't write a skill");
        ApiError::internal(format!("couldn't write the {name} skill"))
    })?;
    let skill_id = sync(app, &root, &name).await?;
    let into = match skill_id {
        Some(id) => part_id(app, id, &heading).await?,
        None => None,
    };
    let edit = {
        let mut conn = app.db.acquire().await?;
        let edit = store::add_edit(
            &mut conn,
            &NewEdit {
                skill: skill_id,
                candidate: Some(candidate),
                root: &root,
                name: &name,
                relation,
                heading: &heading,
                old_heading: old.as_ref().map(|(h, _)| h.as_str()),
                old_body: old.as_ref().map(|(_, b)| b.as_str()),
                // A skill this made: nothing before it.
                before: if made { "" } else { &before },
                after: &after,
            },
        )
        .await?;
        if c.skill.is_none()
            && let Some(id) = skill_id
        {
            sqlx::query("update candidates set skill = ? where id = ?")
                .bind(id)
                .bind(candidate)
                .execute(&mut *conn)
                .await?;
        }
        candidates::decide(&mut conn, candidate, "accepted", into).await?;
        edit
    };
    if root.is_empty() {
        super::links::keep_in_step(app);
    }
    Ok(Some(edit))
}

/// Undoes a change kelve made to a skill: the file as it was, when nothing
/// else changed it since; else only that part taken out, or put back as
/// it was, so the person's own edits stay.
pub async fn undo(app: &Shared, edit: i64) -> ApiResult<()> {
    let e = {
        let mut conn = app.db.acquire().await?;
        store::edit(&mut conn, edit)
            .await?
            .ok_or_else(|| ApiError::not_found("kelve has no such change to undo."))?
    };
    if e.undone_at.is_some() {
        return Err(ApiError::conflict("That change is undone already."));
    }
    let now = one(app, &e.root, &e.name).map(|d| d.text);
    let text = match now {
        Some(now) if now == e.after => Some(e.before.clone()),
        Some(now) => {
            let mut skill = Skill::parse(&now);
            let done = match (e.relation.as_str(), &e.old_heading) {
                ("replaced", Some(old)) => {
                    skill.replace_part(&e.heading, old, e.old_body.as_deref().unwrap_or_default())
                }
                _ => skill.remove_part(&e.heading),
            };
            match done {
                Ok(()) => Some(skill.write()),
                // The person took it out, or changed it, themselves: theirs stands.
                Err(_) => None,
            }
        }
        None => None,
    };
    let dir = dir_for(app, &e.root);
    match text {
        // A skill this made, with nothing else in it now, goes with it.
        Some(text) if e.before.is_empty() && (text.is_empty() || is_draft(&text)) => {
            if let Err(err) = std::fs::remove_dir_all(dir.join(&e.name)) {
                tracing::warn!(error = %err, "couldn't take out a skill kelve made");
            }
        }
        Some(text) => skills_home::write(&dir, &e.name, &text)
            .map(|_| ())
            .map_err(|err| {
                tracing::warn!(error = %err, "couldn't write a skill");
                ApiError::internal(format!("couldn't write the {} skill", e.name))
            })?,
        None => {}
    }
    sync(app, &e.root, &e.name).await?;
    let mut conn = app.db.acquire().await?;
    store::undone(&mut conn, edit).await?;
    if let Some(c) = e.candidate {
        candidates::decide(&mut conn, c, "dismissed", None).await?;
    }
    Ok(())
}

/// Whether a skill's text is only a draft kelve made: no parts.
fn is_draft(text: &str) -> bool {
    Skill::parse(text).parts().is_empty()
}
