//! The person's skills, read from disk into the store (docs/skills.md): their
//! own, in `config.skills_dir`, and each known project's, in its
//! `.kelve/skills/`. The files are the source of truth. kelve rereads them as
//! it starts and whenever the skills pages are looked at, and keeps only what
//! isn't in them: what kind each skill is, and what each part's checks found.
//! A part whose words changed, by hand or not, isn't checked any more
//! (`store::skills::seen`); a part taken out is kept as removed, with what
//! its checks found, so putting it back brings them back. Nothing here ever
//! writes a skill: a skill edited by hand is reread, never changed back.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use kelve_core::skills::{SectionKind, Skill, WorkKind};
use kelve_store::skills::{self as kept, NewSkill, SeenPart};
use sha2::{Digest, Sha256};

use crate::pipeline;
use crate::skills_home;
use crate::state::Shared;

/// Where the skills of `root` are: the person's own for '', a project's
/// otherwise.
pub fn dir_of(app: &Shared, root: &str) -> PathBuf {
    if root.is_empty() {
        app.config.skills_dir.clone()
    } else {
        skills_home::project_dir(Path::new(root))
    }
}

/// A part's text, as a short hash: one that changes is checked again.
pub fn hash(markdown: &str) -> String {
    Sha256::digest(markdown.trim().as_bytes())[..12]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The section a part is under, as the store has it.
pub fn section_name(kind: Option<SectionKind>) -> &'static str {
    match kind {
        Some(SectionKind::HowYouLikeIt) => "like",
        Some(SectionKind::Steps) => "steps",
        Some(SectionKind::WatchOutFor) => "watch",
        None => "",
    }
}

/// What kind a skill kelve hasn't seen before is: a project's, or one of the
/// person's own named for a kind of work ("how you like it"), or else a
/// procedure ("how you do something").
fn kind_of(root: &str, name: &str) -> (&'static str, &'static str) {
    if !root.is_empty() {
        return ("project", "");
    }
    match WorkKind::of(name) {
        Some(work) => ("like", work.name()),
        None => ("do", ""),
    }
}

/// Reads every skill on disk into the store: the person's own, then each
/// known project's. Skills whose folders are gone are forgotten, but only
/// where the folder they were in can still be read.
pub async fn sync(app: &Shared) -> pipeline::Result<()> {
    let roots = {
        let mut conn = app.db.acquire().await?;
        kelve_store::checking::known_roots(&mut conn).await?
    };
    sync_root(app, "").await?;
    for root in roots {
        if Path::new(&root).is_dir() {
            sync_root(app, &root).await?;
        }
    }
    Ok(())
}

/// Reads the skills of one place, the person's own ('') or a project's.
pub async fn sync_root(app: &Shared, root: &str) -> pipeline::Result<()> {
    let dir = dir_of(app, root);
    let found = match skills_home::list(&dir) {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!(error = %e, own = root.is_empty(), "couldn't list the skills folder");
            return Ok(());
        }
    };
    let mut names = HashSet::new();
    for f in &found {
        match std::fs::read_to_string(f.dir.join(skills_home::FILE)) {
            Ok(text) => {
                keep(app, root, &f.name, &text).await?;
                names.insert(f.name.clone());
            }
            Err(e) => tracing::warn!(error = %e, skill = %f.name, "couldn't read a skill"),
        }
    }
    let mut conn = app.db.acquire().await?;
    for skill in kept::all(&mut conn, "made").await? {
        if skill.root == root && !names.contains(&skill.name) {
            kept::forget(&mut conn, skill.id).await?;
        }
    }
    Ok(())
}

/// Rereads one skill from disk: its id, or none when it isn't there.
pub async fn sync_skill(app: &Shared, root: &str, name: &str) -> pipeline::Result<Option<i64>> {
    let text = skills_home::read(&dir_of(app, root), name)
        .map_err(|e| pipeline::Failed::Other(format!("couldn't read a skill: {e}")))?;
    match text {
        Some(text) => Ok(Some(keep(app, root, name, &text).await?)),
        None => {
            let mut conn = app.db.acquire().await?;
            if let Some(s) = kept::by_name(&mut conn, root, name).await?
                && s.state == "made"
            {
                kept::forget(&mut conn, s.id).await?;
            }
            Ok(None)
        }
    }
}

/// Keeps a skill as its file reads: its kind as kelve knows it, and each
/// part by its heading. Its id.
async fn keep(app: &Shared, root: &str, name: &str, text: &str) -> pipeline::Result<i64> {
    let mut conn = app.db.acquire().await?;
    let known = kept::by_name(&mut conn, root, name).await?;
    let (kind, work) = match &known {
        Some(k) if k.state == "made" => (k.kind.clone(), k.work.clone()),
        _ => {
            let (k, w) = kind_of(root, name);
            (k.to_string(), w.to_string())
        }
    };
    let id = match &known {
        // Unchanged: the store has it as it is.
        Some(k) if k.state == "made" && k.kind == kind && k.work == work => k.id,
        _ => {
            kept::add(
                &mut conn,
                &NewSkill {
                    name,
                    kind: &kind,
                    work: &work,
                    root,
                    suggested: None,
                },
            )
            .await?
        }
    };
    let skill = Skill::parse(text);
    let sections = skill.sections();
    let parts: Vec<(String, String, &'static str, String)> = sections
        .iter()
        .flat_map(|s| {
            s.parts.iter().map(move |p| {
                (
                    p.key.clone(),
                    p.heading.clone(),
                    section_name(s.kind),
                    hash(&p.markdown),
                )
            })
        })
        .filter(|(key, ..)| !key.is_empty())
        .collect();
    // A heading written twice is one part, as the store knows it: the first.
    let mut seen_keys = HashSet::new();
    let seen: Vec<SeenPart> = parts
        .iter()
        .filter(|(key, ..)| seen_keys.insert(key.clone()))
        .map(|(key, heading, section, body_hash)| SeenPart {
            key,
            heading,
            section,
            body_hash,
        })
        .collect();
    kept::seen(&mut conn, id, &seen).await?;
    Ok(id)
}

/// Reads every skill once kelve has started, so its pages and its hook find
/// them in the store.
pub fn start(app: &Shared) {
    let app = app.clone();
    tokio::spawn(async move {
        if let Err(e) = sync(&app).await {
            tracing::warn!(error = %e, "couldn't read the skills on disk");
        }
    });
}
