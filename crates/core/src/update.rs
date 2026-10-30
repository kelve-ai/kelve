//! Updating kelve, the parts with no IO: which of two versions is newer,
//! the line a release's notes give it, the checksum `SHA256SUMS` gives a
//! download, which download is this computer's, and the one line the
//! person's agent is told when a newer kelve is out.

use std::cmp::Ordering;

/// A version, as releases are tagged: `0.3.0`, `v0.3.0`, `0.3.0-beta.1`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    numbers: [u64; 3],
    /// After a `-`: a version before the release of the same numbers.
    pre: Option<String>,
}

impl Version {
    /// A version, or None for text that isn't one.
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let text = text.strip_prefix('v').unwrap_or(text);
        // What's after a `+` is the build's, and never orders.
        let text = text.split('+').next().unwrap_or_default();
        let (numbers, pre) = match text.split_once('-') {
            Some((n, p)) if !p.is_empty() => (n, Some(p.to_string())),
            Some(_) => return None,
            None => (text, None),
        };
        let mut parts = numbers.split('.');
        let mut each = [0u64; 3];
        for n in &mut each {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *n = part.parse().ok()?;
        }
        if parts.next().is_some() {
            return None;
        }
        Some(Version { numbers: each, pre })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let [a, b, c] = self.numbers;
        write!(f, "{a}.{b}.{c}")?;
        if let Some(pre) = &self.pre {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numbers
            .cmp(&other.numbers)
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => pre_order(a, b),
            })
    }
}

/// Two pre-releases' order, part by part: numbers as numbers, below words.
fn pre_order(a: &str, b: &str) -> Ordering {
    let mut a = a.split('.');
    let mut b = b.split('.');
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                let by = match (x.parse::<u64>(), y.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => x.cmp(y),
                };
                if by != Ordering::Equal {
                    return by;
                }
            }
        }
    }
}

/// Whether `theirs` is newer than `ours`. Text that isn't a version is
/// never newer.
pub fn is_newer(theirs: &str, ours: &str) -> bool {
    match (Version::parse(theirs), Version::parse(ours)) {
        (Some(t), Some(o)) => t > o,
        _ => false,
    }
}

/// The longest line a release's notes give it.
const LINE: usize = 200;

/// What a release says of itself, in a line: the first line of its notes
/// that isn't a heading, without markdown's marks, in at most 200
/// characters. Empty when its notes say nothing.
pub fn notes_line(notes: &str) -> String {
    let Some(line) = notes
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("<!--"))
    else {
        return String::new();
    };
    let line = line.trim_start_matches(['*', '-', '+', '>', ' ']);
    let plain: String = line
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '*' | '`' | '_'))
        .collect();
    let plain = plain.trim();
    if plain.chars().count() <= LINE {
        return plain.to_string();
    }
    let cut: String = plain.chars().take(LINE).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([',', ';', ':', '.', ' ']))
}

/// The checksum `SHA256SUMS` gives a file, in lower case: each line is a
/// checksum, then the file's name, as `sha256sum` writes it.
pub fn sum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (sum, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim_start();
        let name = name.strip_prefix('*').unwrap_or(name);
        (name == file && sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| sum.to_ascii_lowercase())
    })
}

/// Whether a download is the one `SHA256SUMS` names: its own checksum, in
/// hex, against the one listed for it. A file it doesn't list never is.
pub fn matches(sums: &str, file: &str, sum: &str) -> bool {
    sum_for(sums, file).is_some_and(|listed| listed == sum.to_ascii_lowercase())
}

/// The release's download for a system and machine, as Rust names them
/// (`std::env::consts`), as `.github/workflows/release.yml` names it. None
/// where there's no build.
pub fn download(os: &str, arch: &str) -> Option<&'static str> {
    Some(match (os, arch) {
        ("macos", "aarch64") => "kelve-macos-arm64.tar.gz",
        ("macos", "x86_64") => "kelve-macos-x86_64.tar.gz",
        ("linux", "x86_64") => "kelve-linux-x86_64.tar.gz",
        ("linux", "aarch64") => "kelve-linux-arm64.tar.gz",
        ("windows", "x86_64") => "kelve-windows-x86_64.zip",
        _ => return None,
    })
}

/// The line the person's agent is told, once for each version, when a
/// newer kelve is out.
pub fn agent_line(newest: &str, have: &str, line: &str) -> String {
    let line = line.trim().trim_end_matches('.');
    let said = if line.is_empty() {
        String::new()
    } else {
        format!(": {line}")
    };
    format!(
        "kelve {newest} is out (you have {have}){said}. Ask the person whether to update kelve; if they say yes, call kelve's update tool."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_their_numbers_then_pre_releases() {
        assert!(is_newer("0.3.0", "0.2.1"));
        assert!(is_newer("v0.10.0", "0.9.9"), "numbers, not text");
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.2.1", "0.2.1"));
        assert!(!is_newer("0.2.0", "0.2.1"));
        assert!(
            is_newer("0.3.0", "0.3.0-beta.2"),
            "a release is after its betas"
        );
        assert!(is_newer("0.3.0-beta.10", "0.3.0-beta.2"));
        assert!(is_newer("0.3.0-rc.1", "0.3.0-beta.1"));
        assert!(!is_newer("0.3.0+build.5", "0.3.0"), "a build never orders");
        assert!(
            !is_newer("latest", "0.2.1"),
            "what isn't a version never is newer"
        );
        assert!(!is_newer("0.3", "0.2.1"));
        assert!(!is_newer("0.3.0.1", "0.2.1"));
        assert!(!is_newer("0.3.0-", "0.2.1"));
        assert_eq!(
            Version::parse("v0.3.0-beta.1").map(|v| v.to_string()),
            Some("0.3.0-beta.1".into())
        );
    }

    #[test]
    fn a_release_says_what_it_is_in_its_first_plain_line() {
        assert_eq!(
            notes_line(
                "## What's Changed\n\n* **Updates** come to you, through your agent.\n* more"
            ),
            "Updates come to you, through your agent."
        );
        assert_eq!(
            notes_line("Faster reading.\r\nAnd more."),
            "Faster reading."
        );
        assert_eq!(notes_line("## Only a heading\n"), "");
        assert_eq!(notes_line(""), "");
        let long = notes_line(&"word ".repeat(80));
        assert!(long.ends_with('…') && long.chars().count() <= 201, "{long}");
        assert!(!notes_line("a\u{1b}[31mred").contains('\u{1b}'));
    }

    #[test]
    fn a_download_is_checked_against_its_listed_sum() {
        let sum = "a".repeat(64);
        let other = "b".repeat(64);
        let sums = format!(
            "{sum}  kelve-macos-arm64.tar.gz\n{other} *kelve-windows-x86_64.zip\nnot a line\n"
        );
        assert_eq!(
            sum_for(&sums, "kelve-macos-arm64.tar.gz").as_deref(),
            Some(sum.as_str())
        );
        assert!(matches(&sums, "kelve-macos-arm64.tar.gz", &sum));
        assert!(matches(
            &sums,
            "kelve-windows-x86_64.zip",
            &other.to_uppercase()
        ));
        assert!(
            !matches(&sums, "kelve-macos-arm64.tar.gz", &other),
            "a mismatch is turned away"
        );
        assert!(
            !matches(&sums, "kelve-linux-x86_64.tar.gz", &sum),
            "and a file it doesn't list"
        );
        assert!(!matches(
            "short  kelve-macos-arm64.tar.gz",
            "kelve-macos-arm64.tar.gz",
            "short"
        ));
    }

    #[test]
    fn each_system_has_the_download_the_release_names() {
        assert_eq!(
            download("macos", "aarch64"),
            Some("kelve-macos-arm64.tar.gz")
        );
        assert_eq!(
            download("linux", "aarch64"),
            Some("kelve-linux-arm64.tar.gz")
        );
        assert_eq!(
            download("windows", "x86_64"),
            Some("kelve-windows-x86_64.zip")
        );
        assert_eq!(download("freebsd", "x86_64"), None);
    }

    #[test]
    fn the_agent_is_told_what_is_out_and_to_ask_first() {
        assert_eq!(
            agent_line("0.3.0", "0.2.1", "Updates come through your agent."),
            "kelve 0.3.0 is out (you have 0.2.1): Updates come through your agent. Ask the person whether to update kelve; if they say yes, call kelve's update tool."
        );
        assert!(
            agent_line("0.3.0", "0.2.1", "")
                .starts_with("kelve 0.3.0 is out (you have 0.2.1). Ask")
        );
    }
}
