//! Reading one file the person opened: only from inside the folder they
//! opened, never through a link out of it, never one that may hold secrets,
//! and only text. Every secret in it is masked, character for character, so
//! lines and places stay as they are.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::walk::{MAX_BYTES, Skip, by_name};

/// How far into a file kelve looks for a NUL byte, as git does, to tell
/// text from anything else.
const SNIFF: usize = 8000;

/// A line longer than this in a script or style sheet means it was minified.
const MINIFIED_LINE: usize = 1000;

/// JSON larger than this is data a tool wrote, not something read for ideas:
/// a schema, an API's description, a recording.
const JSON_DATA: usize = 64 * 1024;

/// A file, read: its text with secrets masked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Text {
    pub rel: String,
    /// Its language, as kelve tells from its name: see `core::code`.
    pub lang: Option<&'static str>,
    pub text: String,
    /// A short hash of the text as read, masked.
    pub hash: String,
}

/// Why a file couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error("{}", .0.says())]
    Skipped(Skip),
    #[error("That file isn’t inside the folder you opened.")]
    Outside,
    #[error("That file isn’t there any more.")]
    Missing,
    #[error("kelve couldn’t read that file: {0}")]
    Io(String),
}

/// A file's bytes as kelve reads them: UTF-8 text, not binary, not made by a
/// tool, not minified, with its secrets masked. The same rules for a file on
/// disk and a file from git.
pub fn text_of(rel: &str, bytes: &[u8]) -> Result<String, Skip> {
    if let Some(skip) = by_name(rel) {
        return Err(skip);
    }
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Skip::TooLarge);
    }
    if bytes[..bytes.len().min(SNIFF)].contains(&0) {
        return Err(Skip::Binary);
    }
    if rel.to_ascii_lowercase().ends_with(".json") && bytes.len() > JSON_DATA {
        return Err(Skip::Generated);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| Skip::NotText)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if generated(text) {
        return Err(Skip::Generated);
    }
    let lang = kelve_core::code::language(rel);
    if matches!(lang, Some("javascript" | "css")) && text.lines().any(|l| l.len() > MINIFIED_LINE) {
        return Err(Skip::Minified);
    }
    Ok(kelve_core::redact::mask(text))
}

/// Whether a file says, near its top, that a tool wrote it.
fn generated(text: &str) -> bool {
    text.lines().take(5).any(|l| {
        let l = l.to_ascii_lowercase();
        l.contains("@generated")
            || l.contains("do not edit")
            || l.contains("this file was generated")
            || l.contains("this file is generated")
            || (l.contains("code generated") && l.contains("do not edit"))
    })
}

/// Where `rel` is under `root`, if it's really inside it once every link is
/// followed.
pub fn inside(root: &Path, rel: &str) -> Result<PathBuf, ReadError> {
    if rel.split('/').any(|p| p == ".." || p == ".git") || rel.starts_with('/') {
        return Err(ReadError::Outside);
    }
    let root = std::fs::canonicalize(root).map_err(|_| ReadError::Missing)?;
    let path = std::fs::canonicalize(root.join(rel)).map_err(|_| ReadError::Missing)?;
    if !path.starts_with(&root) {
        return Err(ReadError::Outside);
    }
    Ok(path)
}

/// Reads a file under the folder the person opened.
pub fn read(root: &Path, rel: &str) -> Result<Text, ReadError> {
    if let Some(skip) = by_name(rel) {
        return Err(ReadError::Skipped(skip));
    }
    let path = inside(root, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| ReadError::Io(e.kind().to_string()))?;
    if !meta.is_file() {
        return Err(ReadError::Missing);
    }
    if meta.len() > MAX_BYTES {
        return Err(ReadError::Skipped(Skip::TooLarge));
    }
    let bytes = std::fs::read(&path).map_err(|e| ReadError::Io(e.kind().to_string()))?;
    let text = text_of(rel, &bytes).map_err(ReadError::Skipped)?;
    Ok(Text {
        rel: rel.to_string(),
        lang: kelve_core::code::language(rel),
        hash: kelve_core::text::hash(&text),
        text,
    })
}

/// The most of an image kelve shows: larger ones are left as they are.
pub const IMAGE_MAX_BYTES: u64 = 20_000_000;

/// The kind of image a file is, by its name, if it's one a browser draws.
pub fn image_kind(rel: &str) -> Option<&'static str> {
    let ext = rel.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

/// An image under the folder the person opened, as it is, to show them:
/// the same folder and the same files kelve keeps away from as for text.
/// Only for the person's own browser; nothing in it goes anywhere else.
pub fn image(root: &Path, rel: &str) -> Result<(Vec<u8>, &'static str), ReadError> {
    if let Some(skip) = by_name(rel) {
        return Err(ReadError::Skipped(skip));
    }
    let kind = image_kind(rel).ok_or(ReadError::Skipped(Skip::Binary))?;
    let path = inside(root, rel)?;
    let meta = std::fs::metadata(&path).map_err(|e| ReadError::Io(e.kind().to_string()))?;
    if !meta.is_file() {
        return Err(ReadError::Missing);
    }
    if meta.len() > IMAGE_MAX_BYTES {
        return Err(ReadError::Skipped(Skip::TooLarge));
    }
    let bytes = std::fs::read(&path).map_err(|e| ReadError::Io(e.kind().to_string()))?;
    Ok((bytes, kind))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    #[test]
    fn images_are_known_by_name_and_read_only_inside_the_folder() {
        assert_eq!(image_kind("logo.PNG"), Some("image/png"));
        assert_eq!(image_kind("icon.svg"), Some("image/svg+xml"));
        assert_eq!(image_kind("main.rs"), None);
        let dir = std::env::temp_dir().join(format!("kelve-image-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.png"), b"\x89PNG").unwrap();
        assert_eq!(
            image(&dir, "a.png").unwrap(),
            (b"\x89PNG".to_vec(), "image/png")
        );
        assert_eq!(image(&dir, "../a.png"), Err(ReadError::Outside));
        assert_eq!(image(&dir, "a.rs"), Err(ReadError::Skipped(Skip::Binary)));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_plain_text_is_read() {
        assert_eq!(text_of("a.bin", b"\x00\x01\x02"), Err(Skip::Binary));
        assert_eq!(text_of("a.txt", &[0xff, 0xfe, 0x41]), Err(Skip::NotText));
        assert_eq!(
            text_of(
                "api.ts",
                b"// This file was generated by ts-rs. Do not edit.\nexport type A = 1\n"
            ),
            Err(Skip::Generated)
        );
        let minified = format!("var a={};", "x".repeat(2000));
        assert_eq!(text_of("app.js", minified.as_bytes()), Err(Skip::Minified));
        let data = format!("{{\"rows\": [{}]}}", "1,\n".repeat(30_000));
        assert_eq!(text_of("api.json", data.as_bytes()), Err(Skip::Generated));
        assert!(text_of("package.json", b"{\"name\": \"app\"}").is_ok());
        assert_eq!(text_of(".env", b"KEY=1"), Err(Skip::Secret));
        assert_eq!(
            text_of("a.rs", "\u{feff}fn a() {}".as_bytes()).unwrap(),
            "fn a() {}"
        );
    }

    #[test]
    fn secrets_are_masked_in_place() {
        let text = text_of(
            "config.rs",
            b"const A: u8 = 1;\nconst KEY: &str = \"sk-ant-api03-AbCdEf0123456789_AbCdEf0123456789-xyz\";\n",
        )
        .unwrap();
        assert!(!text.contains("sk-ant-api03-AbCdEf"), "{text}");
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn nothing_outside_the_folder_is_read() {
        let root = crate::tests::folder(&[("src/a.rs", "fn a() {}\n")]);
        assert_eq!(read(&root, "src/a.rs").unwrap().text, "fn a() {}\n");
        assert_eq!(read(&root, "../a.rs"), Err(ReadError::Outside));
        assert_eq!(read(&root, "/etc/passwd"), Err(ReadError::Outside));
        assert_eq!(read(&root, "src/gone.rs"), Err(ReadError::Missing));
        #[cfg(unix)]
        {
            let outside = crate::tests::folder(&[("s.rs", "fn s() {}\n")]);
            std::os::unix::fs::symlink(outside.join("s.rs"), root.join("link.rs")).unwrap();
            assert_eq!(read(&root, "link.rs"), Err(ReadError::Outside));
            std::fs::remove_dir_all(&outside).unwrap();
        }
        std::fs::remove_dir_all(&root).unwrap();
    }
}
