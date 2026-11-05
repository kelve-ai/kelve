//! What's open on the server: its files and changes, read from git. The
//! routes are `api::reader`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use kelve_core::views::{OpenKind, Opened};
use kelve_engine::git::{Change, Stamp, Which};

use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

/// Something opened, and the folder it's in.
#[derive(Debug, Clone)]
pub struct Open {
    pub row: kelve_store::reader::Opened,
    pub root: PathBuf,
}

impl Open {
    pub fn kind(&self) -> OpenKind {
        match self.row.kind.as_str() {
            "file" => OpenKind::File,
            "change" => OpenKind::Change,
            _ => OpenKind::Folder,
        }
    }

    pub fn view(&self) -> Opened {
        Opened {
            id: self.row.id,
            kind: self.kind(),
            root: shown(&self.root),
            path: (!self.row.path.is_empty()).then(|| self.row.path.clone()),
            change: (!self.row.change.is_empty()).then(|| self.row.change.clone()),
            title: self.row.title.clone(),
            opened_at: kelve_core::Timestamp(kelve_store::at(self.row.opened_at)),
            said: self.row.said.clone(),
            agent: (!self.row.agent.is_empty()).then(|| self.row.agent.clone()),
            only: self.only(),
            debrief: None,
            handover: None,
        }
    }

    /// When only some files of the folder were opened: which.
    pub fn only(&self) -> Option<Vec<String>> {
        self.row
            .only
            .as_deref()
            .and_then(|o| serde_json::from_str(o).ok())
    }

    /// Which change it is, if it's one.
    pub fn which(&self) -> Option<Which> {
        which(&self.row.change)
    }
}

/// A folder as the person would write it: `~/projects/kelve`.
pub fn shown(path: &Path) -> String {
    let text = path.display().to_string();
    match std::env::home_dir().map(|h| h.display().to_string()) {
        Some(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text,
    }
}

/// A path as the person wrote it, `~/projects/kelve` too, as a path.
pub fn unshown(text: &str) -> PathBuf {
    match (
        text.strip_prefix('~'),
        std::env::home_dir().map(|h| h.display().to_string()),
    ) {
        (Some(rest), Some(home))
            if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) =>
        {
            PathBuf::from(format!("{home}{rest}"))
        }
        _ => PathBuf::from(text),
    }
}

/// A change by its words: `uncommitted`, `against main`, `since main`,
/// `commit 3f2a9c1`.
pub fn which(words: &str) -> Option<Which> {
    let words = words.trim();
    if words == "uncommitted" {
        return Some(Which::Uncommitted);
    }
    if let Some(base) = words.strip_prefix("since ") {
        return Some(Which::Since(base.trim().to_string()));
    }
    if let Some(base) = words.strip_prefix("against ") {
        return Some(Which::Against(base.trim().to_string()));
    }
    words
        .strip_prefix("commit ")
        .map(|rev| Which::Commit(rev.trim().to_string()))
}

/// Something opened, by its id.
pub async fn opened(app: &Shared, id: i64) -> ApiResult<Open> {
    let mut conn = app.db.acquire().await?;
    let row = kelve_store::reader::get(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("That isn't open in kelve any more. Open it again."))?;
    let root = PathBuf::from(&row.root);
    if !root.is_dir() {
        return Err(ApiError::not_found(format!(
            "{} isn't there any more.",
            shown(&root)
        )));
    }
    Ok(Open { row, root })
}

/// Whether a path is kelve's own, in a project: the project's skills, kept
/// beside its files rather than among them.
pub fn is_kelves(rel: &str) -> bool {
    rel == ".kelve" || rel.starts_with(".kelve/")
}

/// A change as last read, and how to tell it still holds.
type Kept = Arc<Mutex<Option<(Stamp, Arc<Change>)>>>;

/// The changes lately read, by folder and which change, the latest last:
/// each is read again only once its repository says it's moved on.
static CHANGES: LazyLock<Mutex<Vec<(String, Kept)>>> = LazyLock::new(Default::default);

/// How many changes are kept read.
const KEEP_CHANGES: usize = 8;

/// A change, read from git's own files: as read before, while nothing it
/// was read from has moved on.
pub fn change(open: &Open) -> ApiResult<Arc<Change>> {
    let which = open
        .which()
        .ok_or_else(|| ApiError::bad_request("That isn't a change kelve can read."))?;
    let key = format!("{}\n{}", open.root.display(), open.row.change);
    let slot = {
        let mut all = CHANGES.lock().unwrap_or_else(|p| p.into_inner());
        let slot = match all.iter().position(|(k, _)| *k == key) {
            Some(at) => all.remove(at).1,
            None => Arc::default(),
        };
        all.push((key, slot.clone()));
        let over = all.len().saturating_sub(KEEP_CHANGES);
        all.drain(..over);
        slot
    };
    // One reading at a time for each change: asked for at once, it's read once.
    let mut kept = slot.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((stamp, change)) = kept.as_ref()
        && stamp.holds()
    {
        return Ok(change.clone());
    }
    // kelve's own folder, the project's skills, is kept beside the work, not
    // part of it.
    let (change, stamp) = kelve_engine::git::change_stamped(&open.root, &which, &is_kelves)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let change = Arc::new(change);
    *kept = Some((stamp, change.clone()));
    Ok(change)
}

/// Whether `path` is `folder` or in it, as the system tells folders apart:
/// on Windows, in any case, by either separator, and with or without the
/// `\\?\` a whole path may start with.
pub(crate) fn within(path: &Path, folder: &Path) -> bool {
    if !cfg!(windows) {
        return path.starts_with(folder);
    }
    let plain = |p: &Path| {
        let p = p.to_string_lossy().replace('\\', "/").to_lowercase();
        let p = p.strip_prefix("//?/").unwrap_or(&p);
        p.trim_end_matches('/').to_string()
    };
    let (path, folder) = (plain(path), plain(folder));
    path == folder || path.starts_with(&format!("{folder}/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_read_by_their_words() {
        assert_eq!(which("uncommitted"), Some(Which::Uncommitted));
        assert_eq!(which("against main"), Some(Which::Against("main".into())));
        assert_eq!(
            which("commit 3f2a9c1"),
            Some(Which::Commit("3f2a9c1".into()))
        );
        assert_eq!(which("anything else"), None);
    }

    #[test]
    fn paths_are_read_as_people_write_them() {
        assert_eq!(unshown("/srv/kelve"), PathBuf::from("/srv/kelve"));
        assert_eq!(unshown("~someone/kelve"), PathBuf::from("~someone/kelve"));
        if let Some(home) = std::env::home_dir().map(|h| h.display().to_string())
            && !home.is_empty()
        {
            assert_eq!(
                unshown("~/projects/kelve"),
                PathBuf::from(format!("{home}/projects/kelve"))
            );
            assert_eq!(unshown("~"), PathBuf::from(home));
        }
    }
}
