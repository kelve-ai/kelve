//! What the work's context is, for checking whether a lesson holds for it
//! (`core::matching`): read from files kelve already walks, at the top of the
//! folder and one level down, never run. Versions from lockfiles, for code and
//! data; a citation style and a language from Pandoc's front matter, biblatex
//! or a `.csl` file, for writing; the house style from Vale's settings, for
//! prose. Kept on the person's computer.

use std::path::{Path, PathBuf};

use kelve_core::matching::Context;

/// The most of one file read for it.
const MOST: u64 = 2 * 1024 * 1024;

fn text(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MOST {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

/// The files at the top of a folder, and one level down, skipping what's
/// hidden or made by tools.
fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(top) = std::fs::read_dir(root) else {
        return out;
    };
    for e in top.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_file() {
            out.push(p);
        } else if p.is_dir()
            && !name.starts_with('.')
            && !["node_modules", "target", "dist", "build", "vendor", "venv"]
                .contains(&name.as_str())
            && let Ok(inner) = std::fs::read_dir(&p)
        {
            out.extend(inner.flatten().map(|e| e.path()).filter(|p| p.is_file()));
        }
    }
    out
}

/// `name = "x"` then `version = "y"`, as Cargo.lock, uv.lock and
/// poetry.lock have them.
fn toml_packages(text: &str, ctx: &mut Context) {
    let mut name: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("[[") {
            name = None;
        } else if let Some(v) = line.strip_prefix("name = ") {
            name = Some(v.trim_matches('"').to_lowercase());
        } else if let (Some(v), Some(n)) = (line.strip_prefix("version = "), name.take()) {
            ctx.versions
                .entry(n)
                .or_insert_with(|| v.trim_matches('"').to_string());
        }
    }
}

fn package_lock(text: &str, ctx: &mut Context) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };
    if let Some(pkgs) = v["packages"].as_object() {
        for (path, p) in pkgs {
            if let (Some(name), Some(version)) =
                (path.rsplit("node_modules/").next(), p["version"].as_str())
                && !name.is_empty()
            {
                ctx.versions
                    .entry(name.to_lowercase())
                    .or_insert_with(|| version.to_string());
            }
        }
    }
}

fn requirements(text: &str, ctx: &mut Context) {
    for line in text.lines() {
        if let Some((name, version)) = line.split('#').next().unwrap_or_default().split_once("==") {
            let name = name
                .trim()
                .split('[')
                .next()
                .unwrap_or_default()
                .to_lowercase();
            if !name.is_empty() {
                ctx.versions
                    .entry(name)
                    .or_insert_with(|| version.trim().to_string());
            }
        }
    }
}

fn go_mod(text: &str, ctx: &mut Context) {
    for line in text.lines() {
        let words: Vec<&str> = line
            .split_whitespace()
            .filter(|w| *w != "require")
            .collect();
        if let [module, version, ..] = words.as_slice()
            && version.starts_with('v')
            && module.contains('/')
        {
            let name = module.rsplit('/').next().unwrap_or(module).to_lowercase();
            ctx.versions
                .entry(name)
                .or_insert_with(|| version.trim_start_matches('v').to_string());
        }
    }
}

/// A key's value in YAML front matter or a YAML file: `csl: apa.csl`.
fn yaml_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|l| {
        l.trim()
            .strip_prefix(key)
            .and_then(|r| r.strip_prefix(':'))
            .map(|v| v.trim().trim_matches(['"', '\'']))
            .filter(|v| !v.is_empty())
    })
}

/// A citation style, from a `.csl` file's name: `apa-6th-edition.csl` → `apa-6th-edition`.
fn style_of(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.trim_end_matches(".csl").to_lowercase()
}

/// The work's context, from the folder it's in.
pub fn read(root: &Path) -> Context {
    let mut ctx = Context::default();
    for path in files(root) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        match (name.as_str(), ext.as_str()) {
            ("cargo.lock" | "uv.lock" | "poetry.lock", _) => {
                if let Some(t) = text(&path) {
                    toml_packages(&t, &mut ctx);
                }
            }
            ("package-lock.json", _) => {
                if let Some(t) = text(&path) {
                    package_lock(&t, &mut ctx);
                }
            }
            ("requirements.txt", _) => {
                if let Some(t) = text(&path) {
                    requirements(&t, &mut ctx);
                }
            }
            ("go.mod", _) => {
                if let Some(t) = text(&path) {
                    go_mod(&t, &mut ctx);
                }
            }
            (".vale.ini", _) => {
                if let Some(t) = text(&path)
                    && let Some(styles) = t.lines().find_map(|l| {
                        l.trim()
                            .strip_prefix("BasedOnStyles")
                            .map(|r| r.trim_start_matches([' ', '=']).trim().to_lowercase())
                    })
                {
                    ctx.settings.entry("prose-style".into()).or_insert(styles);
                }
            }
            (_, "csl") => {
                ctx.settings
                    .entry("citation-style".into())
                    .or_insert_with(|| style_of(&name));
            }
            (_, "md" | "yaml" | "yml" | "qmd" | "rmd") => {
                if let Some(t) = text(&path) {
                    if let Some(csl) =
                        yaml_value(&t, "csl").or_else(|| yaml_value(&t, "citation-style"))
                    {
                        ctx.settings
                            .entry("citation-style".into())
                            .or_insert_with(|| style_of(csl));
                    }
                    if let Some(lang) = yaml_value(&t, "lang") {
                        ctx.settings
                            .entry("locale".into())
                            .or_insert_with(|| lang.to_string());
                    }
                }
            }
            (_, "tex") => {
                if let Some(t) = text(&path)
                    && let Some(at) = t.find("style=")
                {
                    let style: String = t[at + 6..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '-')
                        .collect();
                    if !style.is_empty() {
                        ctx.settings
                            .entry("citation-style".into())
                            .or_insert(style.to_lowercase());
                    }
                }
            }
            _ => {}
        }
    }
    ctx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::folder;

    #[test]
    fn the_context_is_read_from_lockfiles_and_settings_for_any_work() {
        let root = folder(&[
            (
                "Cargo.lock",
                "[[package]]\nname = \"rand\"\nversion = \"0.9.1\"\n\n[[package]]\nname = \"serde\"\nversion = \"1.0.200\"\n",
            ),
            (
                "web/package-lock.json",
                r#"{"packages":{"":{},"node_modules/react":{"version":"19.1.0"}}}"#,
            ),
            ("requirements.txt", "pandas==2.2.3\nnumpy>=1.0\n"),
            (
                "thesis/thesis.md",
                "---\ntitle: Cool streets\ncsl: styles/apa-6th-edition.csl\nlang: en-GB\n---\n# One\n",
            ),
            (
                ".vale.ini",
                "StylesPath = styles\n[*.md]\nBasedOnStyles = Microsoft\n",
            ),
        ]);
        let ctx = read(&root);
        assert_eq!(ctx.versions.get("rand").map(String::as_str), Some("0.9.1"));
        assert_eq!(
            ctx.versions.get("react").map(String::as_str),
            Some("19.1.0")
        );
        assert_eq!(
            ctx.versions.get("pandas").map(String::as_str),
            Some("2.2.3")
        );
        assert!(
            !ctx.versions.contains_key("numpy"),
            "only a version it says exactly"
        );
        assert_eq!(
            ctx.settings.get("citation-style").map(String::as_str),
            Some("apa-6th-edition")
        );
        assert_eq!(
            ctx.settings.get("locale").map(String::as_str),
            Some("en-GB")
        );
        assert_eq!(
            ctx.settings.get("prose-style").map(String::as_str),
            Some("microsoft")
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
