//! What kelve marks for the person, kept as `marks.md` in their own notes
//! folder: read for every file the agent reads, and for every mark shown.
//! Theirs alone: it goes only to their own agent, never into the project or to
//! the shared graph.

use std::path::PathBuf;

use kelve_core::lens::Lens;
use kelve_core::spec::{self, Spec};

use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

/// The note's name, in the person's own notes folder.
pub const FILE: &str = "marks.md";

/// Where it's kept.
pub fn path(app: &Shared) -> PathBuf {
    crate::reader::own_notes(app).join(FILE)
}

/// What kelve marks for the person: as they wrote it, or, with no note yet,
/// every idea but the basics.
pub fn read(app: &Shared) -> Spec {
    std::fs::read_to_string(path(app))
        .map(|text| spec::parse(&text))
        .unwrap_or_default()
}

/// Whether the person has one yet.
pub fn exists(app: &Shared) -> bool {
    path(app).is_file()
}

/// Keeps what kelve marks for the person, by whoever wrote it, today.
pub fn save(app: &Shared, mut spec: Spec, by: &str) -> ApiResult<Spec> {
    spec.by = Some(by.to_string());
    spec.at = Some(chrono::Utc::now().format("%Y-%m-%d").to_string());
    let file = path(app);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(ApiError::internal)?;
    }
    std::fs::write(&file, spec::write(&spec)).map_err(ApiError::internal)?;
    Ok(spec)
}

/// The most areas, and ideas always or never marked, a spec keeps; and the
/// longest words.
const MOST: usize = 50;
const WORDS: usize = 2000;

/// A spec as given, or why it can't be kept, in a sentence.
pub fn checked(mut spec: Spec) -> ApiResult<Spec> {
    if spec.words.chars().count() > WORDS {
        return Err(ApiError::bad_request(format!(
            "Say it in fewer words: at most {WORDS} characters."
        )));
    }
    if spec.areas.len() > MOST || spec.always.len() > MOST || spec.never.len() > MOST {
        return Err(ApiError::bad_request(format!(
            "Name at most {MOST} areas, and {MOST} ideas to always or never mark."
        )));
    }
    spec.areas
        .retain(|a| !kelve_core::lens::area(&a.area).is_empty());
    let tidy = |names: Vec<String>| -> Vec<String> {
        names
            .into_iter()
            .map(|n| n.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|n| !n.is_empty() && !n.contains(',') && !n.contains('\n'))
            .collect()
    };
    spec.always = tidy(spec.always);
    spec.never = tidy(spec.never);
    Ok(spec)
}

/// What a reading for the person looks for, and their own words on it.
pub fn lens(app: &Shared) -> (Lens, Option<String>) {
    let spec = read(app);
    let words = Some(spec.words.clone()).filter(|w| !w.trim().is_empty());
    (spec.lens(), words)
}
