//! Whether an entry copies its sources' words. An entry is written from
//! the pages kelve fetched, and says what they say in its own words,
//! citing them: a long run of a page's words in the entry's prose is
//! copying, whatever the page's licence (research: "n-gram overlap with
//! fetched sources"). Code excerpts, which kelve copies in itself, aren't
//! prose, and aren't looked at.

use std::collections::HashSet;

use serde_json::Value;

use super::check::{Problem, Rule};
use super::{Block, Body};

/// How many words in a row, the same as a page's, count as copied.
pub const RUN: usize = 12;

/// Lower-case words, letters and digits only: what's compared.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|w| w.trim_matches('\'').to_lowercase())
        .filter(|w| !w.is_empty())
        .collect()
}

/// An entry's prose, block by block: not excerpts, not code.
fn prose(blocks: &[Block]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for block in blocks {
        if matches!(block.body, Body::Excerpt { .. }) {
            continue;
        }
        let mut strings = Vec::new();
        if let Ok(value) = serde_json::to_value(&block.body) {
            collect(&value, &mut strings);
        }
        for s in strings {
            // Code spans may quote anything.
            let without: String = s.split('`').step_by(2).collect::<Vec<_>>().join(" ");
            out.push((block.id.clone(), without));
        }
    }
    out
}

fn collect(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|v| collect(v, out)),
        Value::Object(map) => {
            for (key, v) in map {
                if !matches!(
                    key.as_str(),
                    "lines" | "type" | "code" | "lang" | "written" | "from"
                ) {
                    collect(v, out);
                }
            }
        }
        _ => {}
    }
}

/// Each place an entry's prose repeats `RUN` words in a row from one of
/// the pages it was written from, at most one a block.
pub fn copied(blocks: &[Block], pages: &[&str]) -> Vec<Problem> {
    let mut runs: HashSet<Vec<String>> = HashSet::new();
    for page in pages {
        let w = words(page);
        for run in w.windows(RUN) {
            runs.insert(run.to_vec());
        }
    }
    if runs.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Problem> = Vec::new();
    for (block, text) in prose(blocks) {
        if out
            .iter()
            .any(|p| p.block.as_deref() == Some(block.as_str()))
        {
            continue;
        }
        let w = words(&text);
        if let Some(run) = w.windows(RUN).find(|run| runs.contains(*run)) {
            out.push(Problem {
                block: Some(block),
                rule: Rule::Copied,
                says: format!(
                    "It copies a page's words: “{} …”. Say it in the entry's own words, and cite the page.",
                    run[..6].join(" ")
                ),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph(id: &str, text: &str) -> Block {
        Block {
            id: id.into(),
            part: None,
            fold: None,
            body: Body::Paragraph {
                text: text.into(),
                quiet: false,
            },
        }
    }

    const PAGE: &str = "Advisory locks are application-defined locks. PostgreSQL provides a means for creating locks that have application-defined meanings; the system does not enforce their use, it is up to the application to use them correctly.";

    #[test]
    fn prose_that_copies_a_page_is_found() {
        let blocks = vec![
            paragraph(
                "b1",
                "An advisory lock is one your program names. The system does not enforce their use, it is up to the application to use them correctly^1.",
            ),
            paragraph(
                "b2",
                "The database never checks them for you: your code takes and releases them^1.",
            ),
        ];
        let found = copied(&blocks, &[PAGE]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].block.as_deref(), Some("b1"));
        assert_eq!(found[0].rule, Rule::Copied);
        assert!(
            found[0]
                .says
                .contains("“the system does not enforce their …”"),
            "{}",
            found[0].says
        );
    }

    #[test]
    fn its_own_words_and_short_shared_phrases_pass() {
        let blocks = vec![paragraph(
            "b1",
            "PostgreSQL provides a means for creating locks, but the database itself never enforces them^1.",
        )];
        assert!(copied(&blocks, &[PAGE]).is_empty());
        assert!(copied(&blocks, &[]).is_empty());
    }

    #[test]
    fn code_in_prose_is_left_alone() {
        let blocks = vec![paragraph(
            "b1",
            "Take one with `the system does not enforce their use it is up to the application` and go on^1.",
        )];
        assert!(copied(&blocks, &[PAGE]).is_empty());
    }
}
