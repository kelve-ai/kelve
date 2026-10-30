//! Words and habits readers take as signs that a machine wrote something,
//! found in an entry's own words after it's written. The writer is never
//! shown this list: naming a word to a model makes it likelier to use it.
//! An entry with
//! tells goes back to the writer once, with each named; one that still has
//! them is shown anyway, since a tell is a matter of style, not of fact.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::check::{Problem, Rule, Span, spans};
use super::{Block, Body};
use crate::text::re;

/// The most em dashes an entry uses before they read as a habit.
const DASHES: usize = 2;
/// The most problems named, so a revision has room to fix them.
const MOST: usize = 8;

/// Words and phrases, each with what to do instead.
static TELLS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (r"\bdelv(?:e|es|ed|ing)\b", "say what it looks at"),
        (r"\btapestry\b", "name the parts"),
        (r"\btestament\b", "say what it shows"),
        (r"\bpivotal\b", "say what depends on it"),
        (r"\bcrucial(?:ly)?\b", "say what depends on it"),
        (r"\bintricate\b", "name what's complicated"),
        (r"\bunderscor(?:e|es|ed|ing)\b", "say it shows"),
        (r"\bshowcas(?:e|es|ed|ing)\b", "say it shows"),
        (r"\bfoster(?:s|ed|ing)?\b", "say what it causes"),
        (r"\blandscape\b", "name the field"),
        (r"\brealm\b", "name the field"),
        (r"\bquietly\b", "cut it"),
        (r"\bgenuinely\b", "cut it"),
        (r"\bseamless(?:ly)?\b", "say what it saves"),
        (r"\bgame[- ]chang(?:er|ing)\b", "say what changed"),
        (
            r"\bit[’']?s worth noting\b|\bit is worth noting\b",
            "just say it",
        ),
        (r"(?:^|[.!?]\s+)importantly,", "just say it"),
        (
            r"\bin short\b|\bin summary\b|\bto sum up\b",
            "end on the last fact, not a recap",
        ),
        (r"(?:^|[.!?]\s+)ultimately,", "cut it"),
        (r"\bhere[’']?s the thing\b", "just say it"),
        (
            r"\b(?:key )?takeaway\b",
            "end on the last fact, not a recap",
        ),
        (
            r"\bnot just\b|\bnot only\b",
            "say what it is, not what it isn't only",
        ),
        (
            r"\b(?:is|isn[’']?t|was|wasn[’']?t)(?: really)? (?:just )?about\b",
            "say what it is directly",
        ),
        (r"\bserv(?:es|ed|ing) as\b|\bstands as\b", "use \"is\""),
        (
            r"\bdive (?:into|deeper)\b|\bdeep dive\b",
            "say what it covers",
        ),
        (
            r"\bgreat (?:question|work|job)\b|\bwell done\b|\bnice work\b",
            "no praise",
        ),
    ]
    .into_iter()
    .map(|(pattern, instead)| (re(&format!("(?i){pattern}")), instead))
    .collect()
});

/// The tells in an entry's title and blocks: what it says in its own words,
/// never the lines it quotes.
pub fn tells(title: &str, blocks: &[Block]) -> Vec<Problem> {
    let mut out: Vec<Problem> = Vec::new();
    let mut dashes = 0;
    let mut bold = 0;
    let mut texts: Vec<(Option<&str>, String)> = vec![(None, title.to_string())];
    for block in blocks {
        if matches!(block.body, Body::Excerpt { .. }) {
            continue;
        }
        let mut strings = Vec::new();
        if let Ok(value) = serde_json::to_value(&block.body) {
            prose(&value, &mut strings);
        }
        for s in strings {
            texts.push((Some(block.id.as_str()), s));
        }
    }
    for (block, text) in &texts {
        let words = words_of(text);
        dashes += words.matches('—').count();
        bold += text.matches("**").count() / 2;
        for (pattern, instead) in TELLS.iter() {
            let Some(found) = pattern.find(&words) else {
                continue;
            };
            let said = found
                .as_str()
                .trim_start_matches(|c: char| ".!? ".contains(c));
            let says = format!("\"{said}\" reads as machine-written: {instead}.");
            if !out.iter().any(|p| p.says == says) {
                out.push(Problem {
                    block: block.map(str::to_string),
                    rule: Rule::Style,
                    says,
                });
            }
        }
    }
    if dashes > DASHES {
        out.push(Problem {
            block: None,
            rule: Rule::Style,
            says: format!(
                "It uses {dashes} em dashes. Use at most {DASHES}: a full stop, a comma or brackets does the job."
            ),
        });
    }
    if bold > 1 {
        out.push(Problem {
            block: None,
            rule: Rule::Style,
            says: format!("It puts {bold} things in bold. Bold one sentence at most, or none."),
        });
    }
    out.truncate(MOST);
    out
}

/// Every string in a block that a model wrote as prose: not the lines it
/// quotes or shows as code.
fn prose(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Array(items) => items.iter().for_each(|v| prose(v, out)),
        Value::Object(map) => {
            for (key, v) in map {
                if !matches!(key.as_str(), "lines" | "type" | "code" | "lang") {
                    prose(v, out);
                }
            }
        }
        _ => {}
    }
}

/// Text without its code spans, which may quote anything.
fn words_of(text: &str) -> String {
    match spans(text) {
        Some(spans) => spans
            .into_iter()
            .map(|s| match s {
                Span::Text(t) => t,
                Span::Code(_) => " ",
            })
            .collect(),
        None => text.to_string(),
    }
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

    fn says(problems: &[Problem]) -> Vec<String> {
        problems.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn plain_writing_has_no_tells() {
        let blocks = [paragraph(
            "b1",
            "A bind mount hides whatever the image put in that folder. Docker mounts the host's folder over it at start.",
        )];
        assert!(tells("Why the folder was empty", &blocks).is_empty());
    }

    #[test]
    fn machine_habits_are_named_where_they_are() {
        let blocks = [
            paragraph(
                "b1",
                "Let's delve into it. This isn't about speed — it's about trust — and **this** matters — a **lot**.",
            ),
            paragraph("b2", "In summary, the cache serves as a buffer."),
        ];
        let problems = tells("A pivotal change", &blocks);
        let said = says(&problems);
        assert!(problems.iter().all(|p| p.rule == Rule::Style));
        for want in [
            "\"pivotal\"",
            "In block b1: \"delve\"",
            "\"isn't about\"",
            "In block b2: \"In summary\"",
            "\"serves as\"",
            "3 em dashes",
            "2 things in bold",
        ] {
            assert!(said.iter().any(|s| s.contains(want)), "{want} in {said:?}");
        }
    }

    #[test]
    fn quoted_lines_and_code_are_left_alone() {
        let blocks = [
            Block {
                id: "b1".into(),
                part: None,
                fold: None,
                body: Body::Excerpt {
                    from: "notes.md".into(),
                    lang: None,
                    lines: vec![super::super::Line {
                        n: Some(1),
                        text: "We delve into the landscape.".into(),
                        mark: false,
                        dim: false,
                    }],
                },
            },
            paragraph("b2", "The flag is `--not-just-once`."),
        ];
        assert!(
            tells("Flags", &blocks).is_empty(),
            "{:?}",
            says(&tells("Flags", &blocks))
        );
    }
}
