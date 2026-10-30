//! What's open, by its own ideas: each idea marked in its files, the most used
//! first, with how many files it's in, for search to find ideas here first.

use std::collections::BTreeMap;

use kelve_core::views::OpenKind;

use crate::error::{ApiError, ApiResult};
use crate::reader;
use crate::state::Shared;

/// One of the work's own ideas: how many of its files it's in, and whether
/// it's new to the project.
#[derive(Debug, Clone)]
pub struct OwnIdea {
    pub key: String,
    pub name: String,
    pub files: u32,
    pub new: bool,
}

/// The work's own ideas, the most used first, and how reading it stands.
#[derive(Debug)]
pub struct Own {
    pub top: Vec<OwnIdea>,
    pub files: u32,
    pub read: u32,
}

/// What's open's own ideas: a change's, from its changed lines; a folder's
/// or some files', from every file read.
pub async fn own(app: &Shared, open: &reader::Open) -> ApiResult<Own> {
    // The work's own ideas: each with how many of its files it's in, and
    // whether it's new to the project.
    let mut top: Vec<OwnIdea> = Vec::new();
    let (files, read);
    match open.kind() {
        OpenKind::Change => {
            let change = reader::change(open)?;
            let (built_on, each) = reader::change_built_on(app, open, &change).await?;
            for b in built_on {
                let count = each
                    .values()
                    .filter(|tags| tags.iter().any(|t| t.key == b.key))
                    .count() as u32;
                top.push(OwnIdea {
                    key: b.key,
                    name: b.name,
                    files: count.max(1),
                    new: b.new,
                });
            }
            files = change.files.len() as u32;
            read = each.len() as u32;
        }
        _ => {
            let only = open.only();
            let single = (!open.row.path.is_empty()).then(|| open.row.path.clone());
            let wanted = |path: &str| {
                only.as_ref().is_none_or(|o| o.iter().any(|p| p == path))
                    && single.as_deref().is_none_or(|p| p == path)
            };
            let all = reader::files_read(app, open).await?;
            let mut counts: BTreeMap<(String, String), u32> = BTreeMap::new();
            let mut done = 0;
            for (_, f) in all.iter().filter(|(p, _)| wanted(p)) {
                if f.skip.is_none() {
                    done += 1;
                }
                for i in &f.ideas {
                    *counts.entry((i.key.clone(), i.name.clone())).or_default() += 1;
                }
            }
            let mut listed: Vec<OwnIdea> = counts
                .into_iter()
                .map(|((key, name), files)| OwnIdea {
                    key,
                    name,
                    files,
                    new: false,
                })
                .collect();
            listed.sort_by(|a, b| b.files.cmp(&a.files).then(a.name.cmp(&b.name)));
            top = listed;
            let walked = kelve_engine::walk::walk(&open.root)
                .map_err(|e| ApiError::bad_request(e.to_string()))?;
            files = walked
                .files
                .iter()
                .filter(|f| f.skip.is_none() && !reader::is_kelves(&f.rel) && wanted(&f.rel))
                .count() as u32;
            read = done;
        }
    }
    Ok(Own { top, files, read })
}
