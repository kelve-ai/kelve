//! What's been read stays read: in a git repository, each file's marks are kept
//! in the project, in `.kelve/read/`, a file for each version of a file read,
//! named by git's own id for its contents. It's committed with the work
//! (Nathan, 19 October), so no one in the repo reads the same file twice. A
//! file that changes has a new id, so only it is read again; its unchanged
//! passages keep their marks, by their own hashes.
//!
//! Outside git, or when the person turns it off in settings, marks are
//! kept only in kelve's own store. None of this ever
//! goes to the shared graph.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use kelve_core::lens::Lens;

use crate::pipeline::mark;
use crate::reader::Open;
use crate::state::Shared;

/// The folder in a project where what's been read is kept.
pub const DIR: &str = ".kelve/read";

/// One version of a file, as read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Read {
    /// The file, from the project's root.
    pub path: String,
    /// Other files with the same text, which share these marks: the file
    /// goes only when none of them has this text any more.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub also: Vec<String>,
    /// The instructions it was read under: `read@5`.
    pub version: String,
    /// Each passage's marks, by the passage's hash, with lines counted from
    /// the passage's first, as kelve keeps them.
    pub passages: std::collections::BTreeMap<String, serde_json::Value>,
}

impl Read {
    /// Every file it's for.
    fn paths(&self) -> BTreeSet<String> {
        std::iter::once(self.path.clone())
            .chain(self.also.iter().cloned())
            .collect()
    }

    /// For these files, the first by name the one it's named for.
    fn for_paths(&mut self, paths: &BTreeSet<String>) {
        let mut all = paths.iter().cloned();
        if let Some(first) = all.next() {
            self.path = first;
            self.also = all.collect();
        }
    }
}

/// What a project keeps as read, as kelve knows it: each file's text, by
/// its blob id, and each blob's kept file.
#[derive(Default)]
struct Kept {
    blob_of: HashMap<String, String>,
    files: HashMap<String, KeptFile>,
}

/// A kept file, as kelve knows it: the instructions it was read under, the
/// files it's for, what its passages were read looking for, and a print of
/// what it holds, to tell when there's more to keep.
#[derive(Clone, Default)]
struct KeptFile {
    version: String,
    paths: BTreeSet<String>,
    lenses: Vec<Lens>,
    print: String,
}

/// The passages' readings in a kept file, and their lenses, each once.
fn lenses_of(read: &Read) -> Vec<Lens> {
    let mut out: Vec<Lens> = Vec::new();
    for v in read.passages.values() {
        let lens = serde_json::from_value::<mark::Kept>(v.clone())
            .map(|k| k.lens)
            .unwrap_or_default();
        if !out.contains(&lens) {
            out.push(lens);
        }
    }
    out
}

/// A print of what a kept file holds.
fn print_of(read: &Read) -> String {
    let text = serde_json::to_string(&read.passages).unwrap_or_default();
    kelve_engine::git::blob_id(text.as_bytes()).unwrap_or_default()
}

/// Each project's kept reads.
static KEPT: LazyLock<Mutex<HashMap<PathBuf, Kept>>> = LazyLock::new(Default::default);

/// Whether a folder is in a git repository: only then is what's read kept
/// in it.
fn in_git(root: &Path) -> bool {
    kelve_engine::git::root_of(root).is_some()
}

fn file_for(root: &Path, blob: &str) -> PathBuf {
    root.join(DIR).join(&blob[..2]).join(format!("{blob}.json"))
}

/// Writes a kept file.
fn write(file: &Path, read: &Read) -> std::io::Result<()> {
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(read).unwrap_or_default();
    std::fs::write(file, format!("{json}\n"))
}

/// Takes what's been read in a project, as kept there, into kelve's own
/// store: each passage's marks, under the instructions they were read
/// with, unless kelve has them already.
pub async fn load(app: &Shared, open: &Open) {
    if !in_git(&open.root) {
        return;
    }
    let root = open.root.clone();
    let found = tokio::task::spawn_blocking(move || {
        let mut found: Vec<(String, Read)> = Vec::new();
        let Ok(dirs) = std::fs::read_dir(root.join(DIR)) else {
            return found;
        };
        for dir in dirs.filter_map(Result::ok) {
            let Ok(files) = std::fs::read_dir(dir.path()) else {
                continue;
            };
            for file in files.filter_map(Result::ok) {
                let name = file.file_name().to_string_lossy().into_owned();
                let Some(blob) = name.strip_suffix(".json") else {
                    continue;
                };
                if let Ok(text) = std::fs::read_to_string(file.path())
                    && let Ok(read) = serde_json::from_str::<Read>(&text)
                {
                    found.push((blob.to_string(), read));
                }
            }
        }
        found
    })
    .await
    .unwrap_or_default();
    let Ok(mut conn) = app.db.acquire().await else {
        return;
    };
    let mut index = Kept::default();
    for (blob, read) in found {
        for (hash, marks) in &read.passages {
            let theirs = serde_json::from_value::<mark::Kept>(marks.clone()).unwrap_or_default();
            let mine = kelve_store::marks::get(&mut conn, hash, &read.version, mark::BY)
                .await
                .ok()
                .flatten()
                .and_then(|k| serde_json::from_str::<mark::Kept>(&k).ok());
            // Read here for one reader and there for another: kept as one.
            let kept = match mine {
                Some(mine) if mine == theirs => continue,
                Some(mine) => mine.merge(theirs),
                None => theirs,
            };
            let _ = kelve_store::marks::put(
                &mut conn,
                hash,
                &read.version,
                mark::BY,
                &serde_json::to_string(&kept).unwrap_or_default(),
            )
            .await;
        }
        let paths = read.paths();
        for path in &paths {
            index.blob_of.insert(path.clone(), blob.clone());
        }
        index.files.insert(
            blob,
            KeptFile {
                lenses: lenses_of(&read),
                print: print_of(&read),
                version: read.version,
                paths,
            },
        );
    }
    if let Ok(mut kept) = KEPT.lock() {
        kept.insert(open.root.clone(), index);
    }
}

/// Whether the project keeps a file as read, as it is now, under the
/// instructions reading uses now, and looking for all that `wanted` does:
/// what `load` found in `.kelve/read` for its text today.
pub fn kept_as_is(open: &Open, path: &str, version: &str, wanted: &Lens) -> bool {
    // As it was kept: the text as kelve reads it, secrets masked.
    let Some(now) = kelve_engine::read::read(&open.root, path)
        .ok()
        .and_then(|read| kelve_engine::git::blob_id(read.text.as_bytes()))
    else {
        return false;
    };
    KEPT.lock().ok().is_some_and(|k| {
        k.get(&open.root)
            .and_then(|kept| kept.files.get(&now))
            .is_some_and(|f| f.version == version && f.lenses.iter().all(|l| l.covers(wanted)))
    })
}

/// Keeps a file's marks in the project, once every passage of it has been
/// read: under git's id for its contents, and the instructions it was read
/// under, so a file read again under newer ones is kept again. Files with
/// the same text share one; the file kept for an earlier text goes once no
/// file has that text any more.
pub async fn keep_file(app: &Shared, open: &Open, path: &str, text: &str) {
    // The person can keep what's read to kelve alone (settings, Reading).
    if !app.settings().keep_read_in_repo || !in_git(&open.root) || crate::reader::is_kelves(path) {
        return;
    }
    let Some(blob) = kelve_engine::git::blob_id(text.as_bytes()) else {
        return;
    };
    let version = mark::read_version(app).await;
    let root = open.root.clone();
    let Ok(mut conn) = app.db.acquire().await else {
        return;
    };
    let mut read = Read {
        path: path.to_string(),
        also: Vec::new(),
        version: version.clone(),
        passages: Default::default(),
    };
    for p in mark::passages_of(path, text) {
        let Ok(Some(kept)) = kelve_store::marks::get(&mut conn, &p.hash, &version, mark::BY).await
        else {
            // Not wholly read: kept once it is.
            return;
        };
        read.passages.insert(
            p.hash.clone(),
            serde_json::from_str(&kept).unwrap_or(serde_json::Value::Null),
        );
    }
    drop(conn);
    let print = print_of(&read);
    // Kept already, as it is, for this file: nothing to write.
    let (before, had) = {
        let Ok(kept) = KEPT.lock() else {
            return;
        };
        let here = kept.get(&root);
        let before = here.and_then(|k| k.blob_of.get(path).cloned());
        let had = here.and_then(|k| k.files.get(&blob).cloned());
        (before, had)
    };
    if before.as_deref() == Some(blob.as_str())
        && had
            .as_ref()
            .is_some_and(|f| f.version == version && f.print == print && f.paths.contains(path))
    {
        return;
    }
    // For every file with this text: those it was for already, and this one.
    let mut paths = had.map(|f| f.paths).unwrap_or_default();
    paths.insert(path.to_string());
    read.for_paths(&paths);
    let lenses = lenses_of(&read);
    // The text this file had before, if no other file still has it, goes;
    // if another does, it's kept for that one.
    let earlier = before.filter(|b| *b != blob).and_then(|old| {
        let kept = KEPT.lock().ok()?;
        let users = &kept.get(&root)?.files.get(&old)?.paths;
        let rest: BTreeSet<String> = users.iter().filter(|p| *p != path).cloned().collect();
        Some((old, rest))
    });
    let written = tokio::task::spawn_blocking({
        let root = root.clone();
        let earlier = earlier.clone();
        let blob = blob.clone();
        move || -> std::io::Result<()> {
            write(&file_for(&root, &blob), &read)?;
            if let Some((old, rest)) = earlier {
                let old_file = file_for(&root, &old);
                if rest.is_empty() {
                    std::fs::remove_file(&old_file).ok();
                    if let Some(dir) = old_file.parent() {
                        std::fs::remove_dir(dir).ok();
                    }
                } else if let Ok(text) = std::fs::read_to_string(&old_file)
                    && let Ok(mut old_read) = serde_json::from_str::<Read>(&text)
                {
                    old_read.for_paths(&rest);
                    write(&old_file, &old_read)?;
                }
            }
            Ok(())
        }
    })
    .await;
    match written {
        Ok(Ok(())) => {
            if let Ok(mut kept) = KEPT.lock() {
                let here = kept.entry(root).or_default();
                here.blob_of.insert(path.to_string(), blob.clone());
                here.files.insert(
                    blob,
                    KeptFile {
                        version,
                        paths,
                        lenses,
                        print,
                    },
                );
                if let Some((old, rest)) = earlier {
                    if rest.is_empty() {
                        here.files.remove(&old);
                    } else if let Some(entry) = here.files.get_mut(&old) {
                        entry.paths = rest;
                    }
                }
            }
        }
        Ok(Err(e)) => tracing::warn!(error = %e, "couldn't keep what was read in the project"),
        Err(e) => tracing::warn!(error = %e, "couldn't keep what was read in the project"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_is_kept_where_git_would_put_its_blob() {
        let root = Path::new("/work");
        assert_eq!(
            file_for(root, "ce013625030ba8dba906f756967f9e9ca394464a"),
            PathBuf::from("/work/.kelve/read/ce/ce013625030ba8dba906f756967f9e9ca394464a.json")
        );
    }
}
