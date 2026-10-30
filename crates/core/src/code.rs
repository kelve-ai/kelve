//! Which language a quoted piece of the real thing is in, so the web app can
//! colour it and the export can mark it. kelve says, from what it quotes:
//! a file's name, a shell command. A model never does.

/// What a person or an agent wrote in words: quoted as prose.
pub const PROSE: &str = "prose";

/// The languages kelve colours, by the names the web app's highlighter knows.
pub const LANGUAGES: [&str; 22] = [
    "bash",
    "c",
    "cpp",
    "csharp",
    "css",
    "diff",
    "dockerfile",
    "go",
    "ini",
    "java",
    "javascript",
    "json",
    "kotlin",
    "markdown",
    "python",
    "ruby",
    "rust",
    "sql",
    "swift",
    "typescript",
    "xml",
    "yaml",
];

/// The language of a file, from its name or path: `src/main.rs` is Rust,
/// `Dockerfile` is a Dockerfile. None when kelve can't tell.
pub fn language(path: &str) -> Option<&'static str> {
    let name = path.trim().rsplit(['/', '\\']).next().unwrap_or_default();
    let lower = name.to_ascii_lowercase();
    if lower == "dockerfile" || lower.starts_with("dockerfile.") || lower.ends_with(".dockerfile") {
        return Some("dockerfile");
    }
    if lower == "makefile" || lower == ".bashrc" || lower == ".zshrc" || lower == ".profile" {
        return Some("bash");
    }
    if lower == "cargo.lock" || lower == ".env" || lower.starts_with(".env.") {
        return Some("ini");
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e)?;
    Some(match ext {
        "rs" => "rust",
        "ts" | "tsx" | "mts" | "cts" => "typescript",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "py" | "pyi" => "python",
        "go" => "go",
        "rb" => "ruby",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" => "cpp",
        "cs" => "csharp",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "json" | "jsonc" => "json",
        "yaml" | "yml" => "yaml",
        "toml" | "ini" | "cfg" | "conf" => "ini",
        "css" | "scss" => "css",
        "html" | "htm" | "xml" | "svg" | "vue" => "xml",
        "md" | "mdx" => "markdown",
        "diff" | "patch" => "diff",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_files_name_says_its_language() {
        assert_eq!(language("crates/core/src/code.rs"), Some("rust"));
        assert_eq!(language("Dockerfile"), Some("dockerfile"));
        assert_eq!(language("deploy/api.Dockerfile"), Some("dockerfile"));
        assert_eq!(language("charts/values/api.yaml"), Some("yaml"));
        assert_eq!(language("web/src/App.tsx"), Some("typescript"));
        assert_eq!(language("Cargo.toml"), Some("ini"));
        assert_eq!(language("notes.txt"), None);
        assert_eq!(language("README"), None);
        for found in ["rust", "dockerfile", "yaml", "typescript", "ini", "bash"] {
            assert!(LANGUAGES.contains(&found));
        }
    }
}
