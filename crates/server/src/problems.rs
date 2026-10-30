//! What went wrong, kept for the person and sent nowhere. When kelve panics, or
//! its page meets an error, a report is written beside kelve's data, tidied as
//! `core::problems` says: no secrets, no paths, function names only. Settings
//! shows each one as it is, and the person can post it as an issue, which they
//! read again before it goes, or forget it.

use std::path::{Path, PathBuf};

use chrono::Utc;
use kelve_core::problems::{frames, tidy};
use kelve_core::time::Timestamp;
use kelve_core::views::Problem;
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};

/// The most reports kept: the oldest go first.
const KEPT: usize = 20;

/// A report, as it's kept and as the person sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// What went wrong: `kelve stopped`, or `the page`.
    pub what: String,
    pub at: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub frames: Vec<String>,
}

impl Report {
    fn new(what: &str, message: &str, place: Option<String>, trace: &str) -> Report {
        Report {
            what: what.to_string(),
            at: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            message: tidy(message),
            place: place.map(|p| tidy(&p)),
            frames: frames(trace),
        }
    }
}

fn folder(data: &Path) -> PathBuf {
    data.join("problems")
}

/// Keeps a report, and only the newest few.
fn keep(data: &Path, report: &Report) -> std::io::Result<()> {
    let dir = folder(data);
    std::fs::create_dir_all(&dir)?;
    let name = format!(
        "{}-{}.json",
        Utc::now().timestamp_millis(),
        if report.what == "the page" {
            "page"
        } else {
            "kelve"
        }
    );
    let text = serde_json::to_string_pretty(report).unwrap_or_default();
    std::fs::write(dir.join(name), text)?;
    let mut all = names(data);
    while all.len() > KEPT {
        let oldest = all.remove(0);
        let _ = std::fs::remove_file(dir.join(oldest));
    }
    Ok(())
}

/// The reports kept, oldest first, by file name.
fn names(data: &Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(folder(data)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| is_id(n.trim_end_matches(".json")) && n.ends_with(".json"))
        .collect();
    names.sort();
    names
}

/// A report's id is its file's name: digits, a hyphen, a word.
fn is_id(id: &str) -> bool {
    id.split_once('-').is_some_and(|(n, w)| {
        !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
            && !w.is_empty()
            && w.chars().all(|c| c.is_ascii_lowercase())
    })
}

/// Keeps a report whenever kelve panics, as well as saying so where it
/// runs, as Rust always does.
pub fn watch(data: &Path) {
    let data = data.to_path_buf();
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        before(info);
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "kelve stopped".to_string());
        let place = info
            .location()
            // Where in kelve, written the same on every platform.
            .map(|l| {
                format!(
                    "{}:{}:{}",
                    l.file().replace('\\', "/"),
                    l.line(),
                    l.column()
                )
            });
        let trace = std::backtrace::Backtrace::force_capture().to_string();
        let report = Report::new("kelve stopped", &message, place, &trace);
        if let Err(e) = keep(&data, &report) {
            tracing::warn!(error = %e, "couldn't keep a report of what went wrong");
        }
    }));
}

/// Keeps a report of an error on kelve's page: its message, and the
/// functions it passed through. The same one again within the hour isn't
/// kept twice.
pub fn from_page(data: &Path, message: &str, stack: &str) -> ApiResult<()> {
    let names: String = stack
        .lines()
        .enumerate()
        // A browser's stack, as `at name (address)` or `name@address`:
        // only the names are kept.
        .map(|(i, line)| {
            let line = line.trim().trim_start_matches("at ");
            let name = line.split(['(', '@']).next().unwrap_or("").trim();
            format!("{i}: {name}")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let report = Report::new("the page", message, None, &names);
    let hour_ago = Utc::now().timestamp_millis() - 3_600_000;
    for name in names_since(data, hour_ago) {
        if let Ok(text) = std::fs::read_to_string(folder(data).join(&name))
            && let Ok(kept) = serde_json::from_str::<Report>(&text)
            && kept.message == report.message
        {
            return Ok(());
        }
    }
    keep(data, &report).map_err(ApiError::internal)
}

fn names_since(data: &Path, ms: i64) -> Vec<String> {
    names(data)
        .into_iter()
        .filter(|n| {
            n.split_once('-')
                .and_then(|(t, _)| t.parse::<i64>().ok())
                .is_some_and(|t| t >= ms)
        })
        .collect()
}

/// Every report kept, newest first, each with the issue it would make.
pub fn list(data: &Path) -> Vec<Problem> {
    let mut out: Vec<Problem> = names(data)
        .into_iter()
        .filter_map(|name| {
            let text = std::fs::read_to_string(folder(data).join(&name)).ok()?;
            let report: Report = serde_json::from_str(&text).ok()?;
            let id = name.trim_end_matches(".json").to_string();
            let at = chrono::DateTime::parse_from_rfc3339(&report.at)
                .map(|t| Timestamp(t.with_timezone(&Utc)))
                .unwrap_or_else(|_| Timestamp::now());
            Some(Problem {
                issue: issue(&report, &text),
                what: report.what,
                id,
                at,
                text,
            })
        })
        .collect();
    out.reverse();
    out
}

/// Forgets a report.
pub fn forget(data: &Path, id: &str) -> ApiResult<()> {
    if !is_id(id) {
        return Err(ApiError::not_found("There's no such report."));
    }
    match std::fs::remove_file(folder(data).join(format!("{id}.json"))) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ApiError::internal(e)),
    }
}

/// The address of a new issue on kelve's repository, filled in with the
/// report, for the person to read, add to, and post or not.
fn issue(report: &Report, text: &str) -> String {
    let title = format!(
        "{}: {}",
        report.what,
        kelve_core::text::clip(&report.message, 80)
    );
    let body = format!(
        "What happened, as kelve kept it:\n\n```json\n{}\n```\n\nWhat you were doing when it happened:\n\n",
        kelve_core::text::clip(text, 5_000)
    );
    let enc = |s: &str| url::form_urlencoded::byte_serialize(s.as_bytes()).collect::<String>();
    format!(
        "{}/issues/new?title={}&body={}",
        env!("CARGO_PKG_REPOSITORY"),
        enc(&title),
        enc(&body)
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn reports_are_kept_tidied_listed_and_forgotten() {
        let dir = std::env::temp_dir().join(format!("kelve-problems-{}", std::process::id()));
        from_page(
            &dir,
            "Cannot read /Users/sam/secret/plan.md",
            "Error\n    at openFile (kelve://localhost/assets/index.js:1:2)\n    at run@kelve://localhost/assets/index.js:3:4",
        )
        .unwrap();
        // The same again within the hour isn't kept twice.
        from_page(&dir, "Cannot read /Users/sam/secret/plan.md", "").unwrap();
        let kept = list(&dir);
        assert_eq!(kept.len(), 1);
        let p = &kept[0];
        assert_eq!(p.what, "the page");
        assert!(p.text.contains("Cannot read [path]"), "{}", p.text);
        assert!(!p.text.contains("sam") && !p.text.contains("plan.md"));
        assert!(p.text.contains("openFile") && p.text.contains("run"));
        assert!(!p.text.contains("assets/index.js"));
        assert!(
            p.issue
                .starts_with("https://github.com/kelve-ai/kelve/issues/new?title=")
        );
        forget(&dir, &p.id).unwrap();
        assert!(list(&dir).is_empty());
        assert!(forget(&dir, "../../etc/passwd").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
