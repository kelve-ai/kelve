//! Walking a folder as git sees it: what `.gitignore` (and `.kelveignore`)
//! leave out stays out, symbolic links aren't followed, and `.git` is never
//! read. Each file is classified by its name before anything reads it:
//! files that may hold secrets, lockfiles, vendored and minified code are
//! listed but never read.

use std::path::Path;

use ignore::WalkBuilder;
use serde::Serialize;

use crate::Error;

/// The largest file kelve reads.
pub const MAX_BYTES: u64 = 1_000_000;

/// The most files one walk lists: past this, the folder is too big to read
/// whole, and the walk says so.
pub const MAX_FILES: usize = 100_000;

/// Why a file isn't read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Skip {
    /// It may hold secrets: `.env`, a key, credentials.
    Secret,
    /// A lockfile: what a package manager wrote down, not what anyone did.
    Lockfile,
    /// Someone else's code, kept here: `node_modules`, `vendor`.
    Vendored,
    /// Squeezed onto a few long lines for machines.
    Minified,
    /// Larger than kelve reads.
    TooLarge,
    /// Not text.
    Binary,
    /// Text, but not UTF-8.
    NotText,
    /// Made by a tool: it says not to edit it, or it sits where a tool's
    /// output goes.
    Generated,
    /// A licence or notices: legal text, not ideas.
    Licence,
}

impl Skip {
    /// Why, in a sentence, for the person.
    pub fn says(self) -> &'static str {
        match self {
            Skip::Secret => "kelve doesn’t read this file: it may hold secrets.",
            Skip::Lockfile => "kelve doesn’t read lockfiles: they list versions, not ideas.",
            Skip::Vendored => "kelve doesn’t read vendored code kept in this folder.",
            Skip::Minified => "kelve doesn’t read minified files.",
            Skip::TooLarge => "kelve doesn’t read files this large.",
            Skip::Binary => "This isn’t a text file.",
            Skip::NotText => "kelve reads only UTF-8 text.",
            Skip::Generated => "kelve doesn’t read files a tool made.",
            Skip::Licence => "kelve doesn’t read licences: they’re legal text, not ideas.",
        }
    }
}

/// A file a walk found, by its path from the folder walked, with `/`
/// between its parts on every system.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Found {
    pub rel: String,
    pub bytes: u64,
    /// Why it won't be read, if it won't.
    pub skip: Option<Skip>,
}

/// What a walk found, in path order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Walked {
    pub files: Vec<Found>,
    /// Whether it stopped at `MAX_FILES`.
    pub more: bool,
}

/// Names of files that may hold secrets, matched case-blind on the name.
const SECRET_NAMES: [&str; 14] = [
    ".env",
    ".envrc",
    ".netrc",
    ".npmrc",
    ".pypirc",
    ".pgpass",
    ".git-credentials",
    "credentials",
    "credentials.json",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "secrets.yml",
];

/// Endings of files that may hold secrets.
const SECRET_ENDINGS: [&str; 10] = [
    ".pem",
    ".key",
    ".p12",
    ".pfx",
    ".jks",
    ".keystore",
    ".kdbx",
    ".ovpn",
    ".tfstate",
    ".tfvars",
];

const LOCKFILES: [&str; 12] = [
    "Cargo.lock",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "bun.lockb",
    "poetry.lock",
    "Pipfile.lock",
    "Gemfile.lock",
    "composer.lock",
    "go.sum",
];

/// Folders of someone else's code, kept in a repo.
const VENDORED: [&str; 6] = [
    "node_modules",
    "vendor",
    "third_party",
    ".venv",
    "venv",
    "bower_components",
];

/// Folders a tool's output goes in: generated code and what a build made.
const GENERATED: [&str; 5] = ["gen", "generated", "__generated__", "dist", "out"];

/// Names of licences and notices, before any `.md` or `.txt`: `LICENSE`,
/// `LICENSE-MIT`, `THIRD_PARTY_NOTICES.md`. Not code that handles them.
fn licence(lower: &str) -> bool {
    let (stem, ending) = lower.split_once('.').unwrap_or((lower, ""));
    let named = matches!(
        stem,
        "license"
            | "licence"
            | "copying"
            | "notice"
            | "notices"
            | "third_party_notices"
            | "third-party-notices"
    ) || stem.starts_with("license-")
        || stem.starts_with("licence-");
    named && matches!(ending, "" | "md" | "txt" | "rst" | "lesser")
}

/// Why a file won't be read, from its path alone, before anything opens it.
pub fn by_name(rel: &str) -> Option<Skip> {
    let parts: Vec<&str> = rel.split('/').collect();
    let name = parts.last().copied().unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    if SECRET_NAMES.contains(&lower.as_str())
        || lower.starts_with(".env.")
        || lower.starts_with("id_rsa")
        || lower.starts_with("id_ed25519")
        || SECRET_ENDINGS.iter().any(|e| lower.ends_with(e))
    {
        // An example of what goes in one, with no secrets in it, is fine.
        let example = [".example", ".sample", ".template", ".dist"]
            .iter()
            .any(|e| lower.ends_with(e));
        if !example {
            return Some(Skip::Secret);
        }
    }
    if LOCKFILES.contains(&name) {
        return Some(Skip::Lockfile);
    }
    if parts[..parts.len().saturating_sub(1)]
        .iter()
        .any(|p| VENDORED.contains(p))
    {
        return Some(Skip::Vendored);
    }
    if lower.ends_with(".min.js") || lower.ends_with(".min.css") || lower.ends_with(".map") {
        return Some(Skip::Minified);
    }
    if parts[..parts.len().saturating_sub(1)]
        .iter()
        .any(|p| GENERATED.contains(p))
    {
        return Some(Skip::Generated);
    }
    if licence(&lower) {
        return Some(Skip::Licence);
    }
    None
}

/// Walks a folder. Unreadable entries are left out, not a failure.
pub fn walk(root: &Path) -> Result<Walked, Error> {
    walk_at_most(root, MAX_FILES)
}

/// Walks a folder, stopping at `most` files.
pub fn walk_at_most(root: &Path, most: usize) -> Result<Walked, Error> {
    walk_into(root, most, None)
}

/// Walks a folder, and lists the folders it went into, by path from it,
/// "" for itself: where a file added would be.
pub(crate) fn walk_with_folders(root: &Path) -> Result<(Walked, Vec<String>), Error> {
    let mut folders = Vec::new();
    let walked = walk_into(root, MAX_FILES, Some(&mut folders))?;
    Ok((walked, folders))
}

fn walk_into(
    root: &Path,
    most: usize,
    mut folders: Option<&mut Vec<String>>,
) -> Result<Walked, Error> {
    if !root.is_dir() {
        return Err(Error::NotFolder(root.display().to_string()));
    }
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .follow_links(false)
        .require_git(false)
        .git_global(false)
        .add_custom_ignore_filename(".kelveignore")
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(|e| e.file_name() != ".git");
    let mut walked = Walked::default();
    for entry in builder.build() {
        let Ok(entry) = entry else { continue };
        let Some(kind) = entry.file_type() else {
            continue;
        };
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        let rel: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let rel = rel.join("/");
        if kind.is_dir() {
            if let Some(folders) = folders.as_deref_mut() {
                folders.push(rel);
            }
            continue;
        }
        // Symbolic links are never followed: they can lead anywhere.
        if !kind.is_file() {
            continue;
        }
        let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let skip = by_name(&rel).or((bytes > MAX_BYTES).then_some(Skip::TooLarge));
        if walked.files.len() == most {
            walked.more = true;
            break;
        }
        walked.files.push(Found { rel, bytes, skip });
    }
    walked.files.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(walked)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn names_keep_secrets_lockfiles_and_vendored_code_unread() {
        assert_eq!(by_name(".env"), Some(Skip::Secret));
        assert_eq!(by_name("config/.env.production"), Some(Skip::Secret));
        assert_eq!(by_name(".env.example"), None);
        assert_eq!(by_name("certs/server.pem"), Some(Skip::Secret));
        assert_eq!(by_name("home/.ssh/id_ed25519"), Some(Skip::Secret));
        assert_eq!(by_name("Cargo.lock"), Some(Skip::Lockfile));
        assert_eq!(
            by_name("web/node_modules/react/index.js"),
            Some(Skip::Vendored)
        );
        assert_eq!(by_name("dist/app.min.js"), Some(Skip::Minified));
        assert_eq!(by_name("src/vendor.rs"), None, "only a folder of that name");
        assert_eq!(by_name("src/main.rs"), None);
        assert_eq!(by_name("web/src/api/gen/Idea.ts"), Some(Skip::Generated));
        assert_eq!(by_name("evals/out/report.md"), Some(Skip::Generated));
        assert_eq!(by_name("src/gen.rs"), None, "only a folder of that name");
        assert_eq!(by_name("LICENSE"), Some(Skip::Licence));
        assert_eq!(by_name("LICENCE-MIT.md"), Some(Skip::Licence));
        assert_eq!(by_name("THIRD_PARTY_NOTICES.md"), Some(Skip::Licence));
        assert_eq!(by_name("COPYING.LESSER"), Some(Skip::Licence));
        assert_eq!(by_name("web/notices.ts"), None, "code that writes them");
        assert_eq!(by_name("src/license.rs"), None);
    }

    #[test]
    fn a_walk_follows_gitignore_and_never_reads_git() {
        let root = crate::tests::folder(&[
            (".gitignore", "target/\n*.log\n"),
            (".kelveignore", "notes/\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("target/debug/out.rs", "fn ignored() {}\n"),
            ("debug.log", "noise\n"),
            ("notes/private.md", "mine\n"),
            (".env", "KEY=value\n"),
            (".git/HEAD", "ref: refs/heads/main\n"),
        ]);
        let walked = walk(&root).expect("walked");
        let rels: Vec<&str> = walked.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels, [".env", ".gitignore", ".kelveignore", "src/main.rs"]);
        assert_eq!(walked.files[0].skip, Some(Skip::Secret));
        assert!(!walked.more);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_not_followed() {
        let root = crate::tests::folder(&[("a.rs", "fn a() {}\n")]);
        let outside = crate::tests::folder(&[("secret.rs", "fn s() {}\n")]);
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
        let walked = walk(&root).expect("walked");
        let rels: Vec<&str> = walked.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels, ["a.rs"]);
        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }
}
