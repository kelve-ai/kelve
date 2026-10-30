//! What kelve keeps when something goes wrong: a report the person reads
//! in full, and may choose to post as an issue. Nothing in it can be
//! anyone's work: secrets are stripped, paths on the person's computer
//! are taken out, and a backtrace keeps only the names of functions.

use crate::redact::redact;
use crate::text::re;
use regex::Regex;
use std::sync::LazyLock;

/// The most of a message kept.
const MESSAGE: usize = 600;
/// The most frames of a backtrace kept.
const FRAMES: usize = 30;

/// Absolute paths: `/Users/…`, `/home/…`, `/private/…`, `C:\…`, `~/…`,
/// where a path begins: at the start, or after a space, a quote or a
/// bracket, never partway through a relative one.
static PATH: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(^|[\s"'`(\[=,])((?:[A-Za-z]:\\|\\\\|~/|/)(?:[^\s:"'`<>|()\[\]{},;]+[/\\])+[^\s:"'`<>|()\[\]{},;]*)"#,
    )
});

/// A file of kelve's own source, from wherever it was built: `crates/server/src/reader.rs`.
static KELVES: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?:^|[/\\])(crates[/\\][a-z-]+[/\\](?:src|tests|examples|benches)[/\\][\w/\\.-]+\.rs)$")
});

/// A crate from the registry: `…/registry/src/<index>/tokio-1.48.0/src/runtime/park.rs`.
static CRATE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"[/\\]registry[/\\]src[/\\][^/\\]+[/\\]([\w.-]+[/\\]src[/\\][\w/\\.-]+\.rs)$")
});

/// A path as a report keeps it: kelve's own source files and other crates'
/// by where they are in the code, anything else as `[path]`.
fn path(found: &str) -> String {
    if let Some(c) = KELVES.captures(found) {
        return c[1].replace('\\', "/");
    }
    if let Some(c) = CRATE.captures(found) {
        return c[1].replace('\\', "/");
    }
    "[path]".to_string()
}

/// A message or a place, made fit for a report: secrets and paths out,
/// and no longer than a report needs.
pub fn tidy(text: &str) -> String {
    let text = redact(text);
    let text = PATH.replace_all(&text, |c: &regex::Captures| {
        format!("{}{}", &c[1], path(&c[2]))
    });
    crate::text::clip(text.trim(), MESSAGE)
}

/// The functions a backtrace passed through, in order, by name: nothing
/// else from it, not even where their files are.
pub fn frames(backtrace: &str) -> Vec<String> {
    backtrace
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (n, name) = line.split_once(": ")?;
            n.parse::<u32>().ok()?;
            Some(name.trim().to_string())
        })
        // The panic machinery itself says nothing about what went wrong.
        .filter(|name| {
            !name.starts_with("std::panicking")
                && !name.starts_with("std::panic::")
                && !name.starts_with("core::panicking")
                && !name.starts_with("std::backtrace")
                && !name.starts_with("<alloc::boxed::Box<F,A> as core::ops::function::Fn")
                && !name.starts_with("__rust")
                && !name.starts_with("rust_begin_unwind")
        })
        .map(|name| tidy(&name))
        .take(FRAMES)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_keeps_no_paths_or_secrets() {
        assert_eq!(
            tidy("couldn't read /Users/sam/projects/secret-plan/notes.md: denied"),
            "couldn't read [path]: denied"
        );
        assert_eq!(
            tidy(r"no such file C:\Users\sam\work\ledger.rs"),
            "no such file [path]"
        );
        assert!(
            !tidy("key sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGH was refused")
                .contains("abcdefghij")
        );
    }

    #[test]
    fn kelves_own_places_stay() {
        assert_eq!(
            tidy("/Users/sam/build/kelve/crates/server/src/reader.rs:120:9"),
            "crates/server/src/reader.rs:120:9"
        );
        // As Rust gives kelve's own: relative, and left as it is.
        assert_eq!(
            tidy("crates/server/tests/problems.rs:11:9"),
            "crates/server/tests/problems.rs:11:9"
        );
        assert_eq!(
            tidy(
                "/home/runner/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tokio-1.48.0/src/runtime/park.rs:20"
            ),
            "tokio-1.48.0/src/runtime/park.rs:20"
        );
    }

    #[test]
    fn a_backtrace_keeps_function_names_only() {
        let trace = "   0: std::backtrace_rs::backtrace::libunwind::trace
             at /rustc/abc/library/std/src/../../backtrace/src/backtrace/libunwind.rs:117:9
   1: std::panicking::begin_panic_handler
   2: kelve_server::reader::open::{{closure}}
             at /Users/sam/kelve/crates/server/src/reader.rs:40:5
   3: tokio::runtime::task::harness::poll_future";
        assert_eq!(
            frames(trace),
            vec![
                "kelve_server::reader::open::{{closure}}",
                "tokio::runtime::task::harness::poll_future",
            ]
        );
    }
}
