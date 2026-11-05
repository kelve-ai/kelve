//! Where skills are kept on disk, and reading and writing them: the person's
//! own in `config.skills_dir`, a project's in its `.kelve/skills/`. One skill
//! is one folder holding `SKILL.md`. The files are the source of truth: kelve
//! rereads them, and never keeps its own facts in them.

use std::io;
use std::path::{Path, PathBuf};

/// The file each skill folder holds.
pub const FILE: &str = "SKILL.md";

/// Where a project keeps its skills, under its root.
pub fn project_dir(root: &Path) -> PathBuf {
    root.join(".kelve").join("skills")
}

/// One skill found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The folder's name, which is the skill's name.
    pub name: String,
    /// The folder.
    pub dir: PathBuf,
}

/// Every skill folder in `dir` that holds a `SKILL.md`, by name. A missing
/// folder has none.
pub fn list(dir: &Path) -> io::Result<Vec<Found>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if name.starts_with('.') || !path.join(FILE).is_file() {
            continue;
        }
        found.push(Found { name, dir: path });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// A skill's text, or `None` if there's no such skill.
pub fn read(dir: &Path, name: &str) -> io::Result<Option<String>> {
    let Some(folder) = folder(dir, name) else {
        return Ok(None);
    };
    match std::fs::read_to_string(folder.join(FILE)) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Writes a skill's text whole, making its folder if it's new. It writes a
/// file beside it first and moves it into place, so an agent reading the
/// skill never sees half of it.
pub fn write(dir: &Path, name: &str, text: &str) -> io::Result<PathBuf> {
    let folder = folder(dir, name).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "A skill's name is lowercase letters, digits and hyphens.",
        )
    })?;
    std::fs::create_dir_all(&folder)?;
    let file = folder.join(FILE);
    let aside = folder.join(format!(".{FILE}.writing"));
    std::fs::write(&aside, text)?;
    std::fs::rename(&aside, &file)?;
    Ok(file)
}

/// The folder a skill named `name` lives in, if the name is one a skill can
/// have: never a path out of `dir`.
fn folder(dir: &Path, name: &str) -> Option<PathBuf> {
    kelve_core::skills::name_problem(name)
        .is_none()
        .then(|| dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder of its own for each test, under the system's temp folder.
    struct Temp(PathBuf);
    impl Temp {
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn tempdir() -> Temp {
        let dir = std::env::temp_dir().join(format!(
            "kelve-skills-{}-{}",
            std::process::id(),
            crate::crypto::token()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir)
    }

    #[test]
    fn a_skill_written_is_read_back_and_listed() {
        let dir = tempdir();
        write(dir.path(), "rust", "---\nname: rust\n---\n").unwrap();
        assert_eq!(
            read(dir.path(), "rust").unwrap().as_deref(),
            Some("---\nname: rust\n---\n")
        );
        let names: Vec<_> = list(dir.path())
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, ["rust"]);
    }

    #[test]
    fn a_name_that_is_a_path_is_never_written_or_read() {
        let dir = tempdir();
        assert!(write(dir.path(), "../out", "x").is_err());
        assert_eq!(read(dir.path(), "../out").unwrap(), None);
    }

    #[test]
    fn a_missing_folder_has_no_skills() {
        let dir = tempdir();
        assert!(list(&dir.path().join("none")).unwrap().is_empty());
    }
}
