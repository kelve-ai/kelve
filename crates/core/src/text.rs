//! Small helpers for text: patterns, clipping to a length, and short hashes.

use regex::Regex;
use sha2::{Digest, Sha256};

/// Compiles a pattern written in this crate. They're all fixed, so a bad one is
/// a bug that every test run catches.
pub(crate) fn re(pattern: &str) -> Regex {
    #[allow(clippy::expect_used)]
    Regex::new(pattern).expect("a fixed pattern compiles")
}

/// Cuts text to at most `max` characters, ending with an ellipsis when cut.
pub fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// A short hash of a text: the same text, the same hash.
pub fn hash(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_marks_the_cut() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("a sentence that runs on", 10), "a sentenc…");
        assert_eq!(clip("naïve café", 6), "naïve…");
    }

    #[test]
    fn hash_is_short_and_stable() {
        assert_eq!(
            hash("Cache entries expire after an hour."),
            hash("Cache entries expire after an hour.")
        );
        assert_ne!(hash("an hour"), hash("five minutes"));
        assert_eq!(hash("x").len(), 16);
    }
}
