//! kelve's engine: reading what the person opens, a folder, a file or a
//! change, into passages kelve can mark. It runs nothing from what it reads:
//! no builds, no hooks, no git (it reads git's files itself), nothing a
//! repo configures to run. Every secret is masked as a file is read.

pub mod candidates;
pub mod central;
pub mod context;
pub mod git;
pub mod passages;
pub mod read;
pub mod walk;

/// Why something couldn't be opened.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} isn’t a folder kelve can open.")]
    NotFolder(String),
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(clippy::expect_used)]
    use std::path::PathBuf;

    /// A fresh folder holding these files, for a test.
    pub fn folder(files: &[(&str, &str)]) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("kelve-engine-{}", uuid::Uuid::now_v7().simple()));
        for (rel, text) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().expect("a folder")).expect("made");
            std::fs::write(path, text).expect("written");
        }
        std::fs::create_dir_all(&root).expect("made");
        std::fs::canonicalize(root).expect("a real path")
    }
}

#[cfg(test)]
mod itself {
    #![allow(clippy::expect_used, clippy::print_stdout)]

    /// kelve's own repo, read whole, and its uncommitted work: for looking at
    /// by hand, not for CI.
    #[test]
    #[ignore = "reads the whole repo; run by hand"]
    fn kelve_itself() {
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = crate::git::root_of(here).expect("a repo");
        let started = std::time::Instant::now();
        let walked = crate::walk::walk(&root).expect("walked");
        let (mut read, mut skipped, mut passages, mut lines) = (0, 0, 0, 0);
        for file in &walked.files {
            match crate::read::read(&root, &file.rel) {
                Ok(text) => {
                    read += 1;
                    lines += text.text.lines().count();
                    passages += crate::passages::passages(&file.rel, &text.text).len();
                }
                Err(_) => skipped += 1,
            }
        }
        println!(
            "{} files: {read} read, {skipped} not; {lines} lines in {passages} passages, in {:?}",
            walked.files.len(),
            started.elapsed()
        );
        let which = match std::env::var("KELVE_AGAINST") {
            Ok(base) => crate::git::Which::Against(base),
            Err(_) => crate::git::Which::Uncommitted,
        };
        let started = std::time::Instant::now();
        let (change, stamp) = crate::git::change_stamped(&root, &which, &|_| false).expect("read");
        println!(
            "{}: {} files, in {:?}",
            change.title,
            change.files.len(),
            started.elapsed()
        );
        // Telling it still holds, as it's asked for again.
        let started = std::time::Instant::now();
        let holds = stamp.holds();
        println!("It holds: {holds}, told in {:?}", started.elapsed());
    }
}
