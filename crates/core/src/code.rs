//! Which language a quoted piece of the real thing is in, so the web app can
//! colour it and the export can mark it. kelve says, from what it quotes:
//! a file's name, a shell command. A model never does.

/// What a person or an agent wrote in words: quoted as prose.
pub const PROSE: &str = "prose";

/// The languages kelve colours, by the names the web app's highlighters know.
pub const LANGUAGES: [&str; 57] = [
    "bash",
    "c",
    "clojure",
    "cmake",
    "coffeescript",
    "cpp",
    "crystal",
    "csharp",
    "css",
    "d",
    "dart",
    "diff",
    "dockerfile",
    "elm",
    "erlang",
    "fortran",
    "fsharp",
    "gherkin",
    "go",
    "groovy",
    "haskell",
    "hcl",
    "ini",
    "java",
    "javascript",
    "json",
    "julia",
    "kotlin",
    "lisp",
    "livescript",
    "lua",
    "markdown",
    "objectivec",
    "ocaml",
    "pascal",
    "perl",
    "php",
    "powershell",
    "properties",
    "protobuf",
    "python",
    "r",
    "ruby",
    "rust",
    "scala",
    "scheme",
    "sql",
    "stylus",
    "swift",
    "tcl",
    "terraform",
    "typescript",
    "vbnet",
    "verilog",
    "vhdl",
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
    if lower == "cmakelists.txt" {
        return Some("cmake");
    }
    if lower == "jenkinsfile" {
        return Some("groovy");
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
        "tf" | "tfvars" => "terraform",
        "hcl" => "hcl",
        "php" | "phtml" => "php",
        "lua" => "lua",
        "pl" | "pm" => "perl",
        "scala" | "sc" => "scala",
        "dart" => "dart",
        "m" | "mm" => "objectivec",
        "hs" | "lhs" => "haskell",
        "erl" | "hrl" => "erlang",
        "clj" | "cljs" | "cljc" | "edn" => "clojure",
        "r" => "r",
        "jl" => "julia",
        "ps1" | "psm1" | "psd1" => "powershell",
        "groovy" | "gradle" => "groovy",
        "proto" => "protobuf",
        "cmake" => "cmake",
        "fs" | "fsi" | "fsx" => "fsharp",
        "ml" | "mli" => "ocaml",
        "elm" => "elm",
        "cr" => "crystal",
        "d" => "d",
        "f" | "f90" | "f95" | "f03" | "for" => "fortran",
        "vb" => "vbnet",
        "tcl" => "tcl",
        "v" | "sv" | "svh" => "verilog",
        "vhd" | "vhdl" => "vhdl",
        "pas" => "pascal",
        "scm" | "ss" | "sld" => "scheme",
        "lisp" | "lsp" | "cl" | "el" => "lisp",
        "coffee" => "coffeescript",
        "ls" => "livescript",
        "styl" => "stylus",
        "feature" => "gherkin",
        "properties" => "properties",
        _ => return None,
    })
}

/// Whether a file reads as text, not code: markdown, or plain text. Any
/// other file kelve reads is code, whether or not it knows its language, so
/// it reads plainly as code does.
pub fn prose(path: &str) -> bool {
    match language(path) {
        Some(lang) => lang == "markdown",
        None => {
            let lower = path.trim().to_ascii_lowercase();
            lower.ends_with(".txt") || lower.ends_with(".text")
        }
    }
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
        assert_eq!(language("infra/main.tf"), Some("terraform"));
        assert_eq!(language("prod.tfvars"), Some("terraform"));
        assert_eq!(language("packer/build.pkr.hcl"), Some("hcl"));
        assert_eq!(language("src/index.php"), Some("php"));
        assert_eq!(language("CMakeLists.txt"), Some("cmake"));
        for found in [
            "rust",
            "dockerfile",
            "yaml",
            "typescript",
            "ini",
            "bash",
            "terraform",
            "hcl",
            "php",
            "cmake",
        ] {
            assert!(LANGUAGES.contains(&found));
        }
    }

    #[test]
    fn only_markdown_and_plain_text_read_as_text() {
        assert!(prose("docs/retries.md"));
        assert!(prose("NOTES.txt"));
        assert!(!prose("src/retry.rs"));
        // A language kelve doesn't colour is still code.
        assert!(!prose("infra/main.tf"));
        assert!(!prose("README"));
        // A language's own file is code, whatever its name ends in.
        assert!(!prose("CMakeLists.txt"));
    }
}
