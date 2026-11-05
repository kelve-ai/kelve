//! Kinds of work: kelve's fixed list, so a person has one Rust skill, never
//! "rust", "Rust CLIs" and "rust-style". Each kind is matched by the files a
//! change touches, by their names alone. `Everywhere` matches no file: it's
//! where a default goes when it was caught in more than one kind.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A kind of work, with a "how you like it" skill of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WorkKind {
    /// Any work: what holds in every kind.
    Everywhere,
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Java,
    Kotlin,
    Swift,
    Ruby,
    Php,
    /// C and C++.
    C,
    CSharp,
    /// HTML, and the templates and components written as HTML.
    Html,
    /// Stylesheets.
    Css,
    Sql,
    /// Shell scripts, and the files that run commands, such as Makefiles.
    Shell,
    /// Settings, manifests, containers and CI.
    Config,
    /// Prose people read: READMEs, guides, changelogs.
    Docs,
    /// The words people see in an app, where they're kept apart from code:
    /// translations and string tables.
    Words,
}

/// How each kind is known by a file's name: its extensions, whole names,
/// and folders on its path. The first kind that matches wins, so `Words`,
/// which is known by its folders, comes before `Config`, which takes JSON.
struct Rule {
    kind: WorkKind,
    extensions: &'static [&'static str],
    names: &'static [&'static str],
    folders: &'static [&'static str],
}

const RULES: &[Rule] = &[
    Rule {
        kind: WorkKind::Words,
        extensions: &[
            "po",
            "pot",
            "ftl",
            "strings",
            "stringsdict",
            "xliff",
            "xlf",
            "arb",
            "resx",
        ],
        names: &["strings.xml"],
        folders: &[
            "locales",
            "locale",
            "i18n",
            "l10n",
            "translations",
            "lang",
            "messages",
        ],
    },
    Rule {
        kind: WorkKind::Rust,
        extensions: &["rs"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::TypeScript,
        extensions: &["ts", "tsx", "mts", "cts"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::JavaScript,
        extensions: &["js", "jsx", "mjs", "cjs"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Python,
        extensions: &["py", "pyi", "ipynb"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Go,
        extensions: &["go"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Java,
        extensions: &["java"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Kotlin,
        extensions: &["kt", "kts"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Swift,
        extensions: &["swift"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Ruby,
        extensions: &["rb", "rake", "erb"],
        names: &["gemfile", "rakefile"],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Php,
        extensions: &["php", "phtml"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::C,
        extensions: &["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "hxx"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::CSharp,
        extensions: &["cs", "csx"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Html,
        extensions: &[
            "html", "htm", "vue", "svelte", "astro", "hbs", "njk", "liquid",
        ],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Css,
        extensions: &["css", "scss", "sass", "less", "styl", "pcss"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Sql,
        extensions: &["sql", "psql"],
        names: &[],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Shell,
        extensions: &["sh", "bash", "zsh", "fish", "ps1", "psm1", "bat", "cmd"],
        names: &["makefile", "justfile", ".bashrc", ".zshrc", ".profile"],
        folders: &[],
    },
    Rule {
        kind: WorkKind::Config,
        extensions: &[
            "toml",
            "yaml",
            "yml",
            "json",
            "jsonc",
            "json5",
            "ini",
            "cfg",
            "conf",
            "env",
            "xml",
            "plist",
            "properties",
            "gradle",
            "tf",
            "tfvars",
            "hcl",
            "nix",
            "dockerfile",
        ],
        names: &[
            "dockerfile",
            ".editorconfig",
            ".gitignore",
            ".gitattributes",
            ".npmrc",
            ".nvmrc",
            ".env",
            "procfile",
            "containerfile",
        ],
        folders: &[".github", ".circleci", ".gitlab"],
    },
    Rule {
        kind: WorkKind::Docs,
        extensions: &["md", "mdx", "markdown", "rst", "adoc", "txt", "tex"],
        names: &[
            "readme",
            "changelog",
            "contributing",
            "license",
            "licence",
            "authors",
            "notice",
            "copying",
        ],
        folders: &[],
    },
];

/// Files tools write, which no kind of work is caught in: lockfiles and
/// what's built or generated.
const TOOLS: &[&str] = &[
    "cargo.lock",
    "package-lock.json",
    "npm-shrinkwrap.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "bun.lock",
    "bun.lockb",
    "poetry.lock",
    "pipfile.lock",
    "uv.lock",
    "gemfile.lock",
    "composer.lock",
    "go.sum",
    "flake.lock",
    "podfile.lock",
    "packages.lock.json",
    "mix.lock",
    "pubspec.lock",
];

/// Folders whose files tools write.
const TOOL_FOLDERS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "vendor",
    "__pycache__",
    "__snapshots__",
    ".next",
    "coverage",
];

impl WorkKind {
    /// Every kind, in the order kelve lists them.
    pub const ALL: [WorkKind; 20] = [
        WorkKind::Everywhere,
        WorkKind::Rust,
        WorkKind::TypeScript,
        WorkKind::JavaScript,
        WorkKind::Python,
        WorkKind::Go,
        WorkKind::Java,
        WorkKind::Kotlin,
        WorkKind::Swift,
        WorkKind::Ruby,
        WorkKind::Php,
        WorkKind::C,
        WorkKind::CSharp,
        WorkKind::Html,
        WorkKind::Css,
        WorkKind::Sql,
        WorkKind::Shell,
        WorkKind::Config,
        WorkKind::Docs,
        WorkKind::Words,
    ];

    /// Its name as a skill's name: the folder and the `name:` of the "how
    /// you like it" skill for it.
    pub fn name(self) -> &'static str {
        match self {
            WorkKind::Everywhere => "everywhere",
            WorkKind::Rust => "rust",
            WorkKind::TypeScript => "typescript",
            WorkKind::JavaScript => "javascript",
            WorkKind::Python => "python",
            WorkKind::Go => "go",
            WorkKind::Java => "java",
            WorkKind::Kotlin => "kotlin",
            WorkKind::Swift => "swift",
            WorkKind::Ruby => "ruby",
            WorkKind::Php => "php",
            WorkKind::C => "c",
            WorkKind::CSharp => "csharp",
            WorkKind::Html => "html",
            WorkKind::Css => "css",
            WorkKind::Sql => "sql",
            WorkKind::Shell => "shell",
            WorkKind::Config => "config",
            WorkKind::Docs => "docs",
            WorkKind::Words => "words",
        }
    }

    /// Its name as people read it: the skill's title.
    pub fn label(self) -> &'static str {
        match self {
            WorkKind::Everywhere => "Everywhere",
            WorkKind::Rust => "Rust",
            WorkKind::TypeScript => "TypeScript",
            WorkKind::JavaScript => "JavaScript",
            WorkKind::Python => "Python",
            WorkKind::Go => "Go",
            WorkKind::Java => "Java",
            WorkKind::Kotlin => "Kotlin",
            WorkKind::Swift => "Swift",
            WorkKind::Ruby => "Ruby",
            WorkKind::Php => "PHP",
            WorkKind::C => "C and C++",
            WorkKind::CSharp => "C#",
            WorkKind::Html => "HTML",
            WorkKind::Css => "CSS",
            WorkKind::Sql => "SQL",
            WorkKind::Shell => "Shell",
            WorkKind::Config => "Config",
            WorkKind::Docs => "Docs",
            WorkKind::Words => "Words",
        }
    }

    /// When its skill is used, as the description kelve drafts for it says.
    pub fn when(self) -> String {
        match self {
            WorkKind::Everywhere => "Use always, in any project.".to_string(),
            WorkKind::Docs => {
                "Use when writing or reviewing docs, READMEs or changelogs.".to_string()
            }
            WorkKind::Words => "Use when writing the words people see in an app.".to_string(),
            WorkKind::Config => {
                "Use when changing settings, manifests, containers or CI.".to_string()
            }
            WorkKind::Shell => {
                "Use when reading, writing or reviewing shell scripts and Makefiles.".to_string()
            }
            other => format!("Use when reading, writing or reviewing {}.", other.label()),
        }
    }

    /// The kind with this name, as `name` gives it.
    pub fn of(name: &str) -> Option<WorkKind> {
        let name = name.trim();
        WorkKind::ALL
            .into_iter()
            .find(|k| k.name().eq_ignore_ascii_case(name))
    }

    /// The extensions it's known by, without their dots.
    pub fn extensions(self) -> &'static [&'static str] {
        RULES
            .iter()
            .find(|r| r.kind == self)
            .map_or(&[], |r| r.extensions)
    }
}

/// Whether a tool wrote a file, from its path: a lockfile, or something in
/// a folder of built or generated files.
pub fn made_by_tools(path: &str) -> bool {
    let lower = path.trim().replace('\\', "/").to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or_default();
    TOOLS.contains(&name)
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".map")
        || name.ends_with(".snap")
        || lower
            .split('/')
            .rev()
            .skip(1)
            .any(|folder| TOOL_FOLDERS.contains(&folder))
}

/// The kind of work a file is, from its path. None for a file no kind
/// covers, such as an image, and for files tools write.
pub fn kind_of(path: &str) -> Option<WorkKind> {
    if made_by_tools(path) {
        return None;
    }
    let lower = path.trim().replace('\\', "/").to_ascii_lowercase();
    let mut parts: Vec<&str> = lower.split('/').filter(|p| !p.is_empty()).collect();
    let name = parts.pop()?;
    let folders = parts;
    let ext = name.rsplit_once('.');
    let stem = ext.map_or(name, |(s, _)| s);
    RULES
        .iter()
        .find(|r| {
            let by_ext = ext.is_some_and(|(_, e)| r.extensions.contains(&e));
            let by_name = r.names.contains(&name) || r.names.contains(&stem);
            let by_folder = r.folders.iter().any(|f| folders.contains(f));
            // A folder alone says words only of the files words are kept in.
            let words_file = r.kind != WorkKind::Words
                || by_ext
                || by_name
                || ext.is_some_and(|(_, e)| {
                    matches!(e, "json" | "yaml" | "yml" | "toml" | "properties")
                });
            by_ext || by_name || (by_folder && words_file)
        })
        .map(|r| r.kind)
        .or_else(|| {
            // Dockerfiles have their purpose before or after a dot.
            (name.starts_with("dockerfile") || name.ends_with(".dockerfile"))
                .then_some(WorkKind::Config)
        })
}

/// The one kind a set of kinds comes to: theirs when there's one, and
/// `Everywhere` when there's more than one, or none.
pub fn one_kind(kinds: &[WorkKind]) -> WorkKind {
    let mut found: Vec<WorkKind> = kinds
        .iter()
        .copied()
        .filter(|k| *k != WorkKind::Everywhere)
        .collect();
    found.sort();
    found.dedup();
    match found.as_slice() {
        [one] => *one,
        _ => WorkKind::Everywhere,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_matched_by_their_names() {
        let cases = [
            ("src/retry.rs", Some(WorkKind::Rust)),
            ("web/src/App.tsx", Some(WorkKind::TypeScript)),
            ("web/src/types.d.ts", Some(WorkKind::TypeScript)),
            ("scripts/build.mjs", Some(WorkKind::JavaScript)),
            ("tools/run.py", Some(WorkKind::Python)),
            ("cmd/main.go", Some(WorkKind::Go)),
            ("lib/a.hpp", Some(WorkKind::C)),
            ("Program.cs", Some(WorkKind::CSharp)),
            ("web/index.html", Some(WorkKind::Html)),
            ("web/src/App.vue", Some(WorkKind::Html)),
            ("web/app.scss", Some(WorkKind::Css)),
            (
                "crates/store/migrations/0047_skills.sql",
                Some(WorkKind::Sql),
            ),
            ("install.sh", Some(WorkKind::Shell)),
            ("Makefile", Some(WorkKind::Shell)),
            ("Cargo.toml", Some(WorkKind::Config)),
            ("package.json", Some(WorkKind::Config)),
            (".github/workflows/ci.yml", Some(WorkKind::Config)),
            ("Dockerfile", Some(WorkKind::Config)),
            ("deploy/api.Dockerfile", Some(WorkKind::Config)),
            ("Dockerfile.dev", Some(WorkKind::Config)),
            ("README.md", Some(WorkKind::Docs)),
            ("README", Some(WorkKind::Docs)),
            ("CHANGELOG", Some(WorkKind::Docs)),
            ("docs/skills.md", Some(WorkKind::Docs)),
            ("web/src/locales/en.json", Some(WorkKind::Words)),
            ("app/i18n/fr.yaml", Some(WorkKind::Words)),
            ("po/de.po", Some(WorkKind::Words)),
            ("res/values/strings.xml", Some(WorkKind::Words)),
            ("Resources/Localizable.strings", Some(WorkKind::Words)),
            ("logo.png", None),
            ("Cargo.lock", None),
            ("web/package-lock.json", None),
            ("go.sum", None),
            ("web/dist/app.js", None),
            ("target/debug/build.rs", None),
            ("web/app.min.js", None),
            ("", None),
        ];
        for (path, kind) in cases {
            assert_eq!(kind_of(path), kind, "{path}");
        }
        // A code file in a folder named for words is still code.
        assert_eq!(kind_of("src/messages/send.rs"), Some(WorkKind::Rust));
        assert_eq!(kind_of("src\\retry.rs"), Some(WorkKind::Rust));
    }

    #[test]
    fn every_kind_has_a_name_a_label_and_comes_back() {
        for kind in WorkKind::ALL {
            assert_eq!(WorkKind::of(kind.name()), Some(kind));
            assert!(!kind.label().is_empty());
            // A skill's name keeps to the format's rules.
            assert!(
                crate::skills::name_problem(kind.name()).is_none(),
                "{kind:?}"
            );
            if kind != WorkKind::Everywhere {
                assert!(RULES.iter().any(|r| r.kind == kind), "{kind:?} has no rule");
            }
        }
        assert_eq!(WorkKind::of("Rust"), Some(WorkKind::Rust));
        assert_eq!(WorkKind::of("rust-style"), None);
        assert_eq!(WorkKind::Rust.extensions(), &["rs"]);
        assert!(WorkKind::Everywhere.extensions().is_empty());
        assert_eq!(
            WorkKind::Rust.when(),
            "Use when reading, writing or reviewing Rust."
        );
    }

    #[test]
    fn more_than_one_kind_is_everywhere() {
        assert_eq!(one_kind(&[WorkKind::Rust]), WorkKind::Rust);
        assert_eq!(one_kind(&[WorkKind::Rust, WorkKind::Rust]), WorkKind::Rust);
        assert_eq!(
            one_kind(&[WorkKind::Rust, WorkKind::Everywhere]),
            WorkKind::Rust
        );
        assert_eq!(
            one_kind(&[WorkKind::Rust, WorkKind::Docs]),
            WorkKind::Everywhere
        );
        assert_eq!(one_kind(&[]), WorkKind::Everywhere);
    }

    #[test]
    fn tools_files_are_known() {
        assert!(made_by_tools("Cargo.lock"));
        assert!(made_by_tools("web/node_modules/react/index.js"));
        assert!(made_by_tools("web/src/__snapshots__/a.snap"));
        assert!(!made_by_tools("src/build.rs"));
        assert!(!made_by_tools("docs/target.md"));
    }
}
