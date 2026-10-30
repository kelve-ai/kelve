//! Which of a folder's files to read first: the ones the rest lean on. A
//! file whose name other files mention is central (`ledger` in
//! `use crate::ledger;`, `Files` in `from './reader/Files'`); a README and
//! a package's entry points come early, shallow files before deep ones;
//! tests, examples and fixtures come last. Names are only looked for in the
//! text: nothing is run, and no import is resolved.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::walk::Found;

/// How much text is read to rank a folder by. Past it, files are ranked by
/// their paths alone: a folder that large is read over many days anyway.
pub const MOST_TEXT: u64 = 50_000_000;

/// Names that say only "the way into this folder": such a file goes by its
/// folder's name.
const ENTRY: [&str; 7] = ["mod", "index", "lib", "main", "__init__", "init", "app"];

/// Parts of a path that hold tests, examples and the like.
const LATER: [&str; 12] = [
    "test",
    "tests",
    "__tests__",
    "spec",
    "e2e",
    "fixtures",
    "testdata",
    "examples",
    "example",
    "benches",
    "bench",
    "mocks",
];

/// A folder's readable files, most central first.
pub fn order(root: &Path, files: &[Found]) -> Vec<String> {
    let mut budget = MOST_TEXT;
    let texts: Vec<(String, Option<String>)> = files
        .iter()
        .filter(|f| f.skip.is_none())
        .map(|f| {
            let text = (f.bytes <= budget)
                .then(|| crate::read::read(root, &f.rel).ok())
                .flatten()
                .map(|t| t.text);
            if let Some(t) = &text {
                budget = budget.saturating_sub(t.len() as u64);
            }
            (f.rel.clone(), text)
        })
        .collect();
    rank(&texts)
}

/// The name other files would call a file by.
fn name_of(rel: &str) -> String {
    let mut parts = rel.rsplit('/');
    let file = parts.next().unwrap_or(rel);
    let stem = file.split('.').next().unwrap_or(file).to_lowercase();
    if ENTRY.contains(&stem.as_str())
        && let Some(folder) = parts.next()
    {
        return folder.to_lowercase();
    }
    stem
}

fn later(rel: &str) -> bool {
    let lower = rel.to_lowercase();
    let mut parts = lower.split('/').collect::<Vec<_>>();
    let file = parts.pop().unwrap_or_default();
    parts.iter().any(|p| LATER.contains(p))
        || file.contains(".test.")
        || file.contains(".spec.")
        || file.contains("_test.")
        || file.starts_with("test_")
}

/// Files, with their text when it was read, most central first.
pub fn rank(files: &[(String, Option<String>)]) -> Vec<String> {
    let names: Vec<String> = files.iter().map(|(rel, _)| name_of(rel)).collect();
    let mut sharing: HashMap<&str, usize> = HashMap::new();
    for n in &names {
        if n.chars().count() >= 3 {
            *sharing.entry(n.as_str()).or_default() += 1;
        }
    }
    // How many other files mention each name.
    let mut mentions: HashMap<&str, usize> = HashMap::new();
    for ((_, text), own) in files.iter().zip(&names) {
        let Some(text) = text else { continue };
        let words: HashSet<String> = text
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect();
        for w in &words {
            if w != own
                && let Some((name, _)) = sharing.get_key_value(w.as_str())
            {
                *mentions.entry(name).or_default() += 1;
            }
        }
    }
    let mut scored: Vec<(bool, f64, &str)> = files
        .iter()
        .zip(&names)
        .map(|((rel, _), name)| {
            let shared = sharing.get(name.as_str()).copied().unwrap_or(1).max(1);
            let mentioned =
                mentions.get(name.as_str()).copied().unwrap_or(0) as f64 / shared as f64;
            let file = rel.rsplit('/').next().unwrap_or(rel).to_lowercase();
            let stem = file.split('.').next().unwrap_or_default();
            let bonus = if stem == "readme" {
                3.0
            } else if ENTRY.contains(&stem) {
                1.5
            } else {
                0.0
            };
            let depth = rel.matches('/').count() as f64;
            (later(rel), mentioned + bonus - depth * 0.5, rel.as_str())
        })
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)).then(a.2.cmp(b.2)));
    scored
        .into_iter()
        .map(|(_, _, rel)| rel.to_string())
        .collect()
}

/// The most parts a folder's page shows how they use each other.
const MOST_PARTS: usize = 40;

/// The most files outside a folder its page shows naming its parts.
const MOST_OUTSIDE: usize = 4;

/// A folder's parts, as its page draws them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parts {
    /// Its folders and files, each with whether it's a folder.
    pub parts: Vec<(String, bool)>,
    /// Which use which, as places in `parts`, the user first.
    pub uses: Vec<(usize, usize)>,
    /// Which name which, where one doesn't use the other: a page's link, a
    /// path, a file's name.
    pub names: Vec<(usize, usize)>,
    /// When none of its parts use another: the files outside it that name
    /// its parts, those naming most first, each with the parts it names.
    pub outside: Vec<(String, Vec<usize>)>,
    /// Whether its files are numbered or dated, so they come in order; they
    /// then come in that order.
    pub sequence: bool,
    /// For a sequence: each part's first line, as it says about itself.
    pub first: Vec<Option<String>>,
}

/// A folder's parts, its folders and files, and how they lean on each other. A
/// part uses another when a file in it imports something by the other's name
/// (`use crate::bank::Bank;`, `import { pay } from './bank'`). A part names
/// another when a file in it says the other's file name, or a folder's name
/// with its slash (`retry.rs`, `[specs]`, `./migrations`): any page or file may
/// name another, but code only names what isn't code, since code leans on code
/// by importing it. As with ranking, names are only looked for; nothing is
/// resolved or run. `folder` is "" for the root; `files` is every file, with
/// its text when it was read.
pub fn parts(folder: &str, files: &[(String, Option<String>)]) -> Parts {
    let prefix = if folder.is_empty() {
        String::new()
    } else {
        format!("{folder}/")
    };
    let mut parts: Vec<(String, bool)> = Vec::new();
    // Which part each file is in.
    let mut of: Vec<(usize, &str, Option<&str>)> = Vec::new();
    let mut outside: Vec<(&str, &str)> = Vec::new();
    // Files beside the folder, in the folder that holds it, may name its parts.
    let parent = match folder.rsplit_once('/') {
        Some((up, _)) => format!("{up}/"),
        None => String::new(),
    };
    for (rel, text) in files {
        let Some(rest) = rel.strip_prefix(&prefix) else {
            if !folder.is_empty()
                && rel.starts_with(&parent)
                && let Some(text) = text
            {
                outside.push((rel, text));
            }
            continue;
        };
        let (name, folder) = match rest.split_once('/') {
            Some((dir, _)) => (dir, true),
            None => (rest, false),
        };
        let path = format!("{prefix}{name}");
        let at = match parts.iter().position(|(p, _)| *p == path) {
            Some(at) => at,
            None if parts.len() < MOST_PARTS => {
                parts.push((path, folder));
                parts.len() - 1
            }
            None => continue,
        };
        of.push((at, rel, text.as_deref()));
    }
    // A sequence: its files numbered or dated, nearly all of them.
    let numbered = parts
        .iter()
        .filter(|(p, folder)| !folder && last(p).starts_with(|c: char| c.is_ascii_digit()))
        .count();
    let sequence = numbered >= 2 && numbered * 5 >= parts.len() * 4;
    if sequence {
        let mut order: Vec<usize> = (0..parts.len()).collect();
        order.sort_by(|&a, &b| parts[a].0.cmp(&parts[b].0));
        let at: Vec<usize> = {
            let mut at = vec![0; parts.len()];
            for (new, &old) in order.iter().enumerate() {
                at[old] = new;
            }
            at
        };
        parts = order.iter().map(|&i| parts[i].clone()).collect();
        for f in &mut of {
            f.0 = at[f.0];
        }
    }
    // What other files would call each part.
    let names: Vec<String> = parts
        .iter()
        .map(|(p, folder)| {
            let last = last(p);
            let name = if *folder {
                last
            } else {
                last.split('.').next().unwrap_or(last)
            };
            name.to_lowercase()
        })
        .collect();
    // A folder that's a package also goes by the name its manifest gives
    // it, as code imports it: `kelve-core` is `use kelve_core::`.
    let called: Vec<Vec<String>> = parts
        .iter()
        .zip(&names)
        .map(|((p, folder), name)| {
            let mut all = vec![name.clone()];
            if !*folder {
                return all;
            }
            let here = format!("{p}/");
            for (rel, text) in files {
                let (Some(file), Some(text)) = (rel.strip_prefix(&here), text) else {
                    continue;
                };
                let Some(package) = package_name(file, text) else {
                    continue;
                };
                for n in [
                    package.replace('-', "_"),
                    last(&package).to_string(),
                    package,
                ] {
                    if !all.contains(&n) {
                        all.push(n);
                    }
                }
            }
            all
        })
        .collect();
    let said: Vec<Vec<String>> = parts
        .iter()
        .map(|(p, folder)| said_as(last(p), *folder))
        .collect();
    let mut code = vec![false; parts.len()];
    for &(at, rel, _) in &of {
        if crate::passages::grammar(rel).is_some() {
            code[at] = true;
        }
    }
    let mut uses = std::collections::BTreeSet::new();
    let mut named = std::collections::BTreeSet::new();
    for &(at, rel, text) in &of {
        let Some(text) = text else { continue };
        let is_code = crate::passages::grammar(rel).is_some();
        for line in crate::candidates::imports(rel, text) {
            let words: HashSet<String> = line
                .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                .filter(|w| !w.is_empty())
                .map(str::to_lowercase)
                .collect();
            let called_at = &called[at];
            let lower = line.to_lowercase();
            for (other, called) in called.iter().enumerate() {
                // Its own name, as a word; a package's, only where the
                // import names the package itself.
                let by_name = |n: &String| n.chars().count() >= 3 && !called_at.contains(n);
                let named = by_name(&called[0]) && words.contains(&called[0]);
                let packaged = called[1..]
                    .iter()
                    .any(|n| by_name(n) && imports_package(&lower, n));
                if other != at && (named || packaged) {
                    uses.insert((at, other));
                }
            }
        }
        let lower = text.to_lowercase();
        for (other, said) in said.iter().enumerate() {
            if other != at && !(is_code && code[other]) && said.iter().any(|s| mentions(&lower, s))
            {
                named.insert((at, other));
            }
        }
    }
    let named: Vec<(usize, usize)> = named.into_iter().filter(|e| !uses.contains(e)).collect();
    // From outside, a name counts only when nothing else goes by it: a
    // README, or a `src/`, is anyone's.
    let mut called: HashMap<String, usize> = HashMap::new();
    let mut dirs: HashSet<&str> = HashSet::new();
    for (rel, _) in files {
        *called.entry(last(rel).to_lowercase()).or_default() += 1;
        let mut up = rel.as_str();
        while let Some((dir, _)) = up.rsplit_once('/') {
            if dirs.insert(dir) {
                *called
                    .entry(format!("{}/", last(dir).to_lowercase()))
                    .or_default() += 1;
            }
            up = dir;
        }
    }
    let unique: Vec<bool> = parts
        .iter()
        .map(|(p, folder)| {
            let name = last(p).to_lowercase();
            let name = if *folder { format!("{name}/") } else { name };
            called.get(&name).copied().unwrap_or(0) <= 1
        })
        .collect();
    let mut outside: Vec<(String, Vec<usize>)> = if uses.is_empty() && !pages(&parts) {
        outside
            .into_iter()
            .filter_map(|(rel, text)| {
                let lower = text.to_lowercase();
                let named: Vec<usize> = (0..parts.len())
                    .filter(|&p| unique[p] && said[p].iter().any(|s| mentions(&lower, s)))
                    .collect();
                (!named.is_empty()).then(|| (rel.to_string(), named))
            })
            .collect()
    } else {
        Vec::new()
    };
    outside.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    outside.truncate(MOST_OUTSIDE);
    let first = parts
        .iter()
        .enumerate()
        .map(|(at, (_, folder))| {
            if !sequence || *folder {
                return None;
            }
            of.iter()
                .find(|f| f.0 == at)
                .and_then(|f| f.2)
                .and_then(first_line)
        })
        .collect();
    Parts {
        parts,
        uses: uses.into_iter().collect(),
        names: named,
        outside,
        sequence,
        first,
    }
}

/// Whether a folder's files are all pages to read (`.md`, `.txt` and the
/// like): pages lead to each other, so what names them from outside isn't
/// looked for.
pub fn pages(parts: &[(String, bool)]) -> bool {
    const PAGES: [&str; 6] = ["md", "mdx", "txt", "rst", "adoc", "org"];
    parts.iter().filter(|(_, folder)| !folder).all(|(p, _)| {
        p.rsplit_once('.')
            .is_some_and(|(_, ext)| PAGES.contains(&ext.to_lowercase().as_str()))
    })
}

/// Whether an import line names a package itself: `kelve_core::` in
/// Rust, a quoted path in JavaScript, `from` or `import` in Python.
fn imports_package(line: &str, name: &str) -> bool {
    [
        format!("{name}::"),
        format!("'{name}"),
        format!("\"{name}"),
        format!("from {name}"),
        format!("import {name}"),
        format!("crate {name}"),
    ]
    .iter()
    .any(|p| line.contains(p.as_str()))
}

/// The name a package's manifest gives it, for a manifest at the top of
/// a folder: `Cargo.toml`'s package, `package.json`'s, `pyproject.toml`'s
/// project, `go.mod`'s module. Lowercase.
fn package_name(file: &str, text: &str) -> Option<String> {
    let quoted = |line: &str| {
        let (_, rest) = line.split_once('=').or_else(|| line.split_once(':'))?;
        let name = rest
            .trim()
            .trim_end_matches(',')
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        (!name.is_empty()).then(|| name.to_lowercase())
    };
    match file {
        "Cargo.toml" | "pyproject.toml" => {
            let mut in_package = false;
            for line in text.lines().map(str::trim) {
                if line.starts_with('[') {
                    in_package =
                        line == "[package]" || line == "[project]" || line == "[tool.poetry]";
                } else if in_package && line.starts_with("name") {
                    return quoted(line);
                }
            }
            None
        }
        "package.json" => text
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("\"name\""))
            .and_then(quoted),
        "go.mod" => text
            .lines()
            .find_map(|l| l.trim().strip_prefix("module "))
            .map(|m| m.trim().to_lowercase()),
        _ => None,
    }
}

/// A path's last part.
fn last(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// How a part would be named in text, lowercase: a file by its name
/// (`retry.rs`), a folder by its name and slash (`specs/`, `./specs`).
/// Names too short to tell apart are never looked for.
fn said_as(name: &str, folder: bool) -> Vec<String> {
    let name = name.to_lowercase();
    let stem = name.split('.').next().unwrap_or(&name);
    if folder {
        if name.chars().count() < 3 {
            return Vec::new();
        }
        vec![format!("{name}/"), format!("./{name}")]
    } else if stem.chars().count() >= 3 && name.contains('.') {
        vec![name]
    } else {
        Vec::new()
    }
}

/// Whether some lowercase text says a name on its own: not as part of a
/// longer word or file name.
fn mentions(text: &str, name: &str) -> bool {
    let part = |c: char| c.is_alphanumeric() || c == '_' || c == '-';
    let mut from = 0;
    while let Some(i) = text[from..].find(name) {
        let at = from + i;
        let end = at + name.len();
        let before = text[..at].chars().next_back();
        let after = text[end..].chars().next();
        let starts = before.is_none_or(|c| !(part(c) || c == '.'));
        let ends = name.ends_with('/') || after.is_none_or(|c| !part(c));
        if starts && ends {
            return true;
        }
        from = end;
    }
    false
}

/// What a file says about itself first: its first sentence, from the
/// first lines with words in them, without the marks that make them a
/// comment or a heading, cut to a line.
fn first_line(text: &str) -> Option<String> {
    const MOST: usize = 90;
    let bare = |l: &str| {
        l.trim()
            .trim_start_matches(['-', '/', '#', '*', '!', '<', ';', '%', '"', '\''])
            .trim_end_matches("-->")
            .trim_end_matches("*/")
            .trim()
            .to_string()
    };
    let mut lines = text
        .lines()
        .map(bare)
        .skip_while(|l| !l.chars().any(char::is_alphanumeric));
    let mut said = lines.next()?;
    // A sentence that runs on, onto the lines after it.
    for next in lines {
        if said.ends_with(['.', '?', '!', ':']) || next.is_empty() || said.chars().count() > MOST {
            break;
        }
        said = format!("{said} {next}");
    }
    if let Some(end) = said.find(". ") {
        said.truncate(end + 1);
    }
    if said.chars().count() <= MOST {
        return Some(said);
    }
    let cut: String = said.chars().take(MOST).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    Some(format!("{}…", cut.trim_end_matches([',', ';', ':', ' '])))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn files(list: &[(&str, &str)]) -> Vec<(String, Option<String>)> {
        list.iter()
            .map(|(r, t)| (r.to_string(), Some(t.to_string())))
            .collect()
    }

    #[test]
    fn what_others_mention_comes_first() {
        let order = rank(&files(&[
            ("src/a.rs", "use crate::ledger;"),
            ("src/b.rs", "use crate::ledger; use crate::spend;"),
            ("src/c.rs", "ledger::hold()"),
            ("src/ledger.rs", "pub fn hold() {}"),
            ("src/spend.rs", "pub fn spend() {}"),
        ]));
        assert_eq!(order[0], "src/ledger.rs");
        assert_eq!(order[1], "src/spend.rs");
    }

    #[test]
    fn a_way_into_a_folder_goes_by_its_folder() {
        assert_eq!(name_of("crates/store/src/mod.rs"), "src");
        assert_eq!(name_of("web/src/reader/index.ts"), "reader");
        assert_eq!(name_of("Files.tsx"), "files");
        let order = rank(&files(&[
            ("app.ts", "import { x } from './reader'"),
            ("reader/index.ts", "export const x = 1"),
            ("reader/other.ts", "export const y = 2"),
        ]));
        assert_eq!(order[0], "reader/index.ts");
    }

    #[test]
    fn readmes_early_and_tests_last() {
        let order = rank(&files(&[
            ("tests/ledger.rs", "use kelve::ledger;"),
            ("src/ledger.rs", "pub fn hold() {}"),
            ("README.md", "# kelve"),
            ("src/thing.test.ts", "thing"),
        ]));
        assert_eq!(order[0], "README.md");
        assert_eq!(&order[2..], ["src/thing.test.ts", "tests/ledger.rs"]);
    }

    #[test]
    fn unread_and_skipped_files_still_have_a_place() {
        let root = crate::tests::folder(&[
            ("README.md", "# x"),
            (".env", "KEY=1"),
            ("src/a.rs", "fn a() {}"),
        ]);
        let walked = crate::walk::walk(&root).expect("walked");
        let order = order(&root, &walked.files);
        assert_eq!(
            order,
            ["README.md", "src/a.rs"],
            "never a file kelve won't read"
        );
        assert!(rank(&[("x.rs".into(), None)]) == ["x.rs"]);
    }

    #[test]
    fn a_folders_parts_use_each_other_by_what_they_import() {
        let files = files(&[
            ("src/main.rs", "use crate::api::routes;\nfn main() {}\n"),
            (
                "src/api/v1/charges.rs",
                "use crate::bank::Bank;\nuse crate::ledger::write;\n",
            ),
            ("src/bank/mod.rs", "use crate::retry::with_retries;\n"),
            ("src/ledger.rs", "use sqlx::PgPool;\n"),
            ("src/retry.rs", "use std::time::Duration;\n"),
            ("README.md", "# ledger, with a bank\n"),
        ]);
        let Parts {
            parts: found, uses, ..
        } = parts("src", &files);
        let names: Vec<&str> = found.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            names,
            [
                "src/main.rs",
                "src/api",
                "src/bank",
                "src/ledger.rs",
                "src/retry.rs"
            ]
        );
        assert!(found[1].1, "a folder");
        let named: Vec<(&str, &str)> = uses.iter().map(|&(a, b)| (names[a], names[b])).collect();
        assert_eq!(
            named,
            [
                ("src/main.rs", "src/api"),
                ("src/api", "src/bank"),
                ("src/api", "src/ledger.rs"),
                ("src/bank", "src/retry.rs"),
            ]
        );
        // At the top, and only by imports: the README's words aren't.
        let top = parts("", &files);
        assert_eq!(top.parts.len(), 2);
        assert!(top.uses.is_empty());
    }

    #[test]
    fn pages_name_each_other_and_code_names_what_isnt_code() {
        let files = files(&[
            ("README.md", "See [the docs](docs/retries.md) and `src/`.\n"),
            (
                "docs/retries.md",
                "# Retries\nThey live in `src/retry.rs`.\n",
            ),
            (
                "src/main.rs",
                "use crate::retry;\n// Runs ./migrations, then serves. See retry.rs.\n",
            ),
            ("src/retry.rs", "use std::time::Duration;\n"),
            ("migrations/0001_ledger.sql", "-- The ledger.\n"),
            ("Cargo.toml", "[package]\nname = \"ledger\"\n"),
        ]);
        let top = parts("", &files);
        let at = |p: &str| top.parts.iter().position(|(q, _)| q == p).expect(p);
        assert!(top.names.contains(&(at("README.md"), at("docs"))));
        assert!(top.names.contains(&(at("README.md"), at("src"))));
        assert!(top.names.contains(&(at("docs"), at("src"))));
        assert!(
            top.names.contains(&(at("src"), at("migrations"))),
            "a path in code"
        );
        assert!(
            !top.names.iter().any(|&(a, _)| a == at("Cargo.toml")),
            "the word ledger isn't a name"
        );
        // Inside src, code names no code: it imports it.
        let src = parts("src", &files);
        assert_eq!(src.uses, [(0, 1)]);
        assert!(src.names.is_empty(), "{:?}", src.names);
        assert!(!top.sequence);
    }

    #[test]
    fn numbered_files_come_in_order_each_with_its_first_line() {
        let files = files(&[
            (
                "migrations/0002_keys.sql",
                "\n-- Request keys, so a retry\n-- lands once. Kept a day.\ncreate table keys();\n",
            ),
            (
                "migrations/0001_ledger.sql",
                "-- The ledger: every charge, twice.\n",
            ),
            (
                "migrations/0003_long.sql",
                &format!("/* {} */", "word ".repeat(40)),
            ),
        ]);
        let found = parts("migrations", &files);
        assert!(found.sequence);
        let names: Vec<&str> = found.parts.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            names,
            [
                "migrations/0001_ledger.sql",
                "migrations/0002_keys.sql",
                "migrations/0003_long.sql"
            ]
        );
        assert_eq!(
            found.first[0].as_deref(),
            Some("The ledger: every charge, twice.")
        );
        assert_eq!(
            found.first[1].as_deref(),
            Some("Request keys, so a retry lands once.")
        );
        let long = found.first[2].as_deref().expect("a line");
        assert!(
            long.ends_with("word…") && long.chars().count() <= 91,
            "{long}"
        );
    }

    #[test]
    fn what_uses_none_of_the_rest_shows_what_names_it_from_outside() {
        let files = files(&[
            (
                "web/index.html",
                "<!-- See README.md --><link rel=\"icon\" href=\"/favicon.svg\"><link rel=\"manifest\" href=\"/site.webmanifest\">",
            ),
            ("web/public/favicon.svg", "<svg/>"),
            (
                "web/public/site.webmanifest",
                "{\"icons\": [{\"src\": \"/icon-192.png\"}]}",
            ),
            ("web/public/icon-192.png", ""),
            ("web/src/app.ts", "export const x = 1"),
            ("docs/icons.md", "favicon.svg is drawn by hand."),
            ("web/public/README.md", "The icons."),
            ("README.md", "See web/public/README.md."),
        ]);
        let found = parts("web/public", &files);
        assert!(found.uses.is_empty());
        let at = |p: &str| found.parts.iter().position(|(q, _)| q == p).expect(p);
        assert_eq!(
            found.names,
            [(
                at("web/public/site.webmanifest"),
                at("web/public/icon-192.png")
            )]
        );
        assert_eq!(
            found.outside,
            [(
                "web/index.html".to_string(),
                vec![
                    at("web/public/favicon.svg"),
                    at("web/public/site.webmanifest")
                ]
            )],
            "only what's beside it, in web/, and never a name others share"
        );
    }

    #[test]
    fn a_name_counts_only_on_its_own() {
        assert!(mentions("see retry.rs.", "retry.rs"));
        assert!(mentions("(../src/retry.rs)", "retry.rs"));
        assert!(!mentions("old_retry.rs", "retry.rs"));
        assert!(!mentions("retry.rst", "retry.rs"));
        assert!(mentions("in specs/reader.md", "specs/"));
        assert!(!mentions("the subspecs/ folder", "specs/"));
        assert!(said_as("a.rs", false).is_empty());
        assert!(said_as("Makefile", false).is_empty());
    }

    #[test]
    fn a_package_goes_by_the_name_its_manifest_gives_it() {
        let files = files(&[
            (
                "crates/core/Cargo.toml",
                "[package]\nname = \"kelve-core\"\n\n[dependencies]\nname = \"not-this\"\n",
            ),
            ("crates/core/src/lib.rs", "pub mod views;\n"),
            (
                "crates/graph-api/Cargo.toml",
                "[package]\nname = \"kelve-graph-api\"\n",
            ),
            ("crates/graph-api/src/lib.rs", "use kelve_core::views;\n"),
            (
                "crates/server/Cargo.toml",
                "[package]\nname = \"kelve-server\"\n",
            ),
            (
                "crates/server/src/lib.rs",
                "use kelve_core::views;\nuse kelve_graph_api::Client;\n",
            ),
            (
                "web/package.json",
                "{\n  \"name\": \"@kelve/web\",\n  \"version\": \"1.0.0\"\n}\n",
            ),
        ]);
        let found = parts("crates", &files);
        let at = |p: &str| found.parts.iter().position(|(q, _)| q == p).expect(p);
        let (core, graph, server) = (
            at("crates/core"),
            at("crates/graph-api"),
            at("crates/server"),
        );
        assert!(found.uses.contains(&(graph, core)), "{:?}", found.uses);
        assert!(found.uses.contains(&(server, core)));
        assert!(found.uses.contains(&(server, graph)));
        assert_eq!(found.uses.len(), 3);
        // A helper that goes by a package's name isn't the package.
        let mut more = files.clone();
        more.push((
            "crates/cli/Cargo.toml".into(),
            Some("[package]\nname = \"kelve\"\n".into()),
        ));
        more.push((
            "crates/cli/src/main.rs".into(),
            Some("fn main() {}\n".into()),
        ));
        more.push((
            "crates/server/tests/a.rs".into(),
            Some("use common::kelve;\n".into()),
        ));
        let found = parts("crates", &more);
        let cli = found
            .parts
            .iter()
            .position(|(q, _)| q == "crates/cli")
            .expect("cli");
        assert!(
            !found.uses.iter().any(|&(_, b)| b == cli),
            "{:?}",
            found.uses
        );
        assert_eq!(
            package_name("package.json", "{\n  \"name\": \"@kelve/web\"\n}").as_deref(),
            Some("@kelve/web")
        );
        assert_eq!(
            package_name("go.mod", "module github.com/x/kelve\n").as_deref(),
            Some("github.com/x/kelve")
        );
        assert_eq!(package_name("README.md", "name = x"), None);
    }
}
