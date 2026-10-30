//! Marks: where a passage rests on an idea, and the rules every mark keeps
//! before anyone sees it.
//!
//! A model never says where a mark goes by position: those come back wrong.
//! Code finds what in a passage a mark can point at, and the model chooses
//! one by its id, or names a line and quotes the exact words on it, or names
//! a run of lines for a pattern. Code finds the words; a quote that isn't
//! there is dropped, never matched loosely. A wrong mark costs more than a
//! missing one (research: "Marking the passages of a file").

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ideas::{IdeaRef, norm, same_idea, tidy};
use crate::lens::Depth;

/// The most marks one passage keeps.
pub const MOST: usize = 8;

/// One mark for about this many lines, at most: enough room for every idea
/// a passage rests on, and no more marks than lines.
pub const LINES_PER_MARK: u32 = 8;

/// The longest run of lines one mark covers: a pattern, not a section.
pub const LONGEST_RUN: u32 = 30;

/// The longest definition, in characters.
pub const DEFINITION: usize = 200;

/// Ideas anyone who reads code knows: always basic, whatever the agent
/// says, so they're marked only for someone new to the area.
const BASIC: &[&str] = &[
    "argument",
    "array",
    "boolean",
    "class",
    "comment",
    "constant",
    "data structure",
    "dictionary",
    "enum",
    "enumeration",
    "error",
    "error handling",
    "file",
    "function",
    "function call",
    "hash map",
    "if statement",
    "import",
    "integer",
    "interface",
    "list",
    "logging",
    "loop",
    "map",
    "method",
    "module",
    "null",
    "object",
    "parameter",
    "return value",
    "string",
    "struct",
    "type",
    "variable",
    "vector",
];

/// Whether an idea is one anyone who reads code knows.
pub fn basic(name: &str) -> bool {
    let n = norm(name);
    BASIC.iter().any(|b| norm(b) == n)
}

/// Something in a passage a mark can point at, found by code: a name, a
/// call, a type, a string. Lines count from 1 in the file; columns are
/// characters on the line, `to` not included.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub line: u32,
    pub from: u32,
    pub to: u32,
    pub text: String,
}

/// What the model said about one mark: where, by one of three pointers,
/// what idea, and why it matters there.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Said {
    /// A candidate's id, or empty.
    pub candidate: String,
    /// A line in the file, or 0.
    pub line: u32,
    /// The exact words on `line`, or empty for a run of lines.
    pub quote: String,
    /// The last line of a run, or 0.
    pub last_line: u32,
    /// The idea's name, as someone would look it up.
    pub name: String,
    /// What the idea is, in general, in a line: how one name's two ideas
    /// are told apart.
    pub definition: String,
    /// How this passage uses the idea, in a line, in the work's own terms:
    /// shown in the margin beside it, and read by the check.
    #[serde(rename = "use")]
    pub use_: String,
    /// The area the idea belongs to: one of the reader's, when one fits.
    pub area: String,
    /// How deep the idea is in its area: `basic`, `common`, `advanced` or
    /// `specialist`.
    pub depth: String,
}

/// The exact words a mark underlines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Span {
    pub line: u32,
    /// Characters into the line, from 0.
    pub from: u32,
    /// Where it ends, not included.
    pub to: u32,
}

/// A mark: lines `start` to `end` of a file rest on an idea. `span` is the
/// words it underlines; a mark on a run of lines has none. `IdeaMark` to
/// the web app, beside an entry figure's `Mark`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "IdeaMark")]
pub struct Mark {
    pub start: u32,
    pub end: u32,
    pub span: Option<Span>,
    pub name: String,
    pub definition: String,
    /// The idea in the shared graph, once it's been found there. None for
    /// an idea the graph doesn't have yet.
    pub idea: Option<IdeaRef>,
    /// How this passage uses the idea, in a line, as the agent said it:
    /// the margin's line beside it. Plain text, with `code` allowed. It's
    /// the person's work, so it never goes to the shared graph.
    #[serde(default, rename = "use")]
    pub use_: String,
    /// The area the idea belongs to, as `lens::area` names it.
    #[serde(default)]
    pub area: String,
    /// How deep the idea is in its area, as the agent said: what decides
    /// whom it's looked for and marked for (`lens::Lens`).
    #[serde(default)]
    pub depth: Depth,
}

impl Mark {
    /// The same mark, `by` lines further down (or up, when negative): marks
    /// are kept against their passage, wherever it moves in the file.
    pub fn shifted(&self, by: i64) -> Mark {
        let at = |n: u32| u32::try_from((i64::from(n) + by).max(1)).unwrap_or(n);
        Mark {
            start: at(self.start),
            end: at(self.end),
            span: self.span.as_ref().map(|s| Span {
                line: at(s.line),
                ..s.clone()
            }),
            ..self.clone()
        }
    }

    fn overlaps(&self, other: &Mark) -> bool {
        if self.start > other.end || other.start > self.end {
            return false;
        }
        match (&self.span, &other.span) {
            // Words on one line overlap, or touch and would read as one.
            (Some(a), Some(b)) if a.line == b.line => a.from <= b.to && b.from <= a.to,
            (Some(_), Some(_)) => false,
            // Two runs of lines that share one; a run and words inside it are
            // two ideas, the pattern and what's used in it.
            (None, None) => true,
            _ => false,
        }
    }
}

/// A mark placed in its passage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub mark: Mark,
}

/// The longest margin's line, in characters.
pub const USE: usize = 120;

/// Where `quote` is in `line`, as character columns: exactly, or else
/// ignoring case and runs of spaces. Nothing looser.
fn find(line: &str, quote: &str) -> Option<(u32, u32)> {
    let quote = quote.trim();
    if quote.is_empty() {
        return None;
    }
    let cols = |byte: usize| u32::try_from(line[..byte].chars().count()).unwrap_or(u32::MAX);
    if let Some(at) = line.find(quote) {
        return Some((cols(at), cols(at + quote.len())));
    }
    // Ignoring case and spacing: compare character by character, letting a
    // run of whitespace in one match a run in the other.
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let want: Vec<char> = quote.chars().collect();
    for start in 0..chars.len() {
        let (mut i, mut j) = (start, 0);
        while i < chars.len() && j < want.len() {
            let (c, w) = (chars[i].1, want[j]);
            if c.is_whitespace() && w.is_whitespace() {
                while i < chars.len() && chars[i].1.is_whitespace() {
                    i += 1;
                }
                while j < want.len() && want[j].is_whitespace() {
                    j += 1;
                }
                continue;
            }
            if !c.to_lowercase().eq(w.to_lowercase()) {
                break;
            }
            i += 1;
            j += 1;
        }
        if j == want.len() && start < i {
            let end = chars.get(i).map_or(line.len(), |(b, _)| *b);
            return Some((cols(chars[start].0), cols(end)));
        }
    }
    None
}

/// Places what the model said in a passage of `lines` (the file's, from
/// line 1), lines `start` to `end`. What doesn't point at something real in
/// the passage, or names no plain idea, is dropped.
pub fn place(
    lines: &[&str],
    start: u32,
    end: u32,
    candidates: &[Candidate],
    said: &[Said],
) -> Vec<Placed> {
    let within = |n: u32| n >= start && n <= end && (n as usize) <= lines.len();
    let mut out = Vec::new();
    for s in said {
        let Some(name) = tidy(&s.name) else { continue };
        // A name goes to the shared graph: only plain words.
        if crate::redact::query_ok(&name, &[]).is_err() || name.contains("[[") {
            continue;
        }
        let definition = s
            .definition
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if definition.chars().count() > DEFINITION
            || definition.contains("[[")
            || !crate::entry::check::rich_ok(&definition)
        {
            continue;
        }
        // The margin's line: one plain line, or none.
        let use_ = s.use_.split_whitespace().collect::<Vec<_>>().join(" ");
        let use_ = if use_.chars().count() > USE
            || use_.contains("[[")
            || use_.contains("**")
            || !crate::entry::check::rich_ok(&use_)
        {
            String::new()
        } else {
            use_
        };
        let depth = if basic(&name) {
            Depth::Basic
        } else {
            Depth::from_word(&s.depth)
        };
        let area = crate::lens::area(&s.area);
        let mark = |start: u32, end: u32, span: Option<Span>| Mark {
            start,
            end,
            span,
            name: name.clone(),
            definition: definition.clone(),
            idea: None,
            use_: use_.clone(),
            area: area.clone(),
            depth,
        };
        let placed = if !s.candidate.trim().is_empty() {
            candidates
                .iter()
                .find(|c| c.id == s.candidate.trim() && within(c.line))
                .map(|c| {
                    mark(
                        c.line,
                        c.line,
                        Some(Span {
                            line: c.line,
                            from: c.from,
                            to: c.to,
                        }),
                    )
                })
        } else if !s.quote.trim().is_empty() && within(s.line) {
            find(lines[s.line as usize - 1], &s.quote).map(|(from, to)| {
                mark(
                    s.line,
                    s.line,
                    Some(Span {
                        line: s.line,
                        from,
                        to,
                    }),
                )
            })
        } else if within(s.line)
            && within(s.last_line)
            && s.last_line >= s.line
            && s.last_line - s.line < LONGEST_RUN
        {
            Some(mark(s.line, s.last_line, None))
        } else {
            None
        };
        if let Some(mark) = placed {
            out.push(Placed { mark });
        }
    }
    out
}

/// The most marks a passage of this many lines keeps.
pub fn most_for(lines: u32) -> usize {
    (lines.div_ceil(LINES_PER_MARK) as usize).clamp(1, MOST)
}

/// Keeps a passage's marks to the rules, in the model's order of
/// importance: one mark for each idea, none overlapping another, and no
/// more than the passage's share.
pub fn keep(placed: Vec<Placed>, lines: u32) -> Vec<Placed> {
    let most = most_for(lines);
    let mut seen = HashSet::new();
    let mut kept: Vec<Placed> = Vec::new();
    for p in placed {
        if kept.len() == most {
            break;
        }
        if !seen.insert(norm(&p.mark.name)) || kept.iter().any(|k| k.mark.overlaps(&p.mark)) {
            continue;
        }
        kept.push(p);
    }
    kept
}

/// A file's marks, from its passages' in order: each idea is marked where
/// it first comes up, and only there. Two marks are one idea when the graph
/// says so, or, unless it says they're two, when their names are close
/// (`ideas::same_idea`).
pub fn first_only(marks: Vec<Mark>) -> Vec<Mark> {
    let mut seen = HashSet::new();
    let mut kept: Vec<Mark> = Vec::new();
    for m in marks {
        let k = m
            .idea
            .as_ref()
            .map_or_else(|| norm(&m.name), |i| i.id.clone());
        let close = kept.iter().any(|k| {
            let apart = matches!((&k.idea, &m.idea), (Some(a), Some(b)) if a.id != b.id);
            !apart && same_idea(&k.name, &m.name)
        });
        if seen.insert(k) && !close {
            kept.push(m);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "use sqlx::PgPool;\n\
        \n\
        // Take the lock in the transaction, so it's released on commit.\n\
        async fn reserve(tx: &mut Tx) {\n\
            sqlx::query(\"select pg_advisory_xact_lock($1)\").execute(tx).await;\n\
        }\n";

    fn lines() -> Vec<&'static str> {
        FILE.lines().collect()
    }

    fn said(name: &str) -> Said {
        Said {
            name: name.into(),
            definition: "A lock a program names, and the database keeps.".into(),
            use_: "One charge at a time for each customer.".into(),
            ..Said::default()
        }
    }

    #[test]
    fn a_mark_points_by_candidate_quote_or_run_and_nothing_else() {
        let candidates = vec![Candidate {
            id: "c1".into(),
            line: 5,
            from: 14,
            to: 47,
            text: "\"select pg_advisory_xact_lock($1)\"".into(),
        }];
        let by_candidate = Said {
            candidate: "c1".into(),
            ..said("Advisory locks")
        };
        let by_quote = Said {
            line: 3,
            quote: "released  ON commit".into(),
            ..said("Transaction-scoped locks")
        };
        let by_run = Said {
            line: 4,
            last_line: 6,
            ..said("Async functions")
        };
        let invented = Said {
            line: 3,
            quote: "released at the end".into(),
            ..said("Invented")
        };
        let elsewhere = Said {
            candidate: "c9".into(),
            ..said("Nowhere")
        };
        let outside = Said {
            line: 1,
            quote: "PgPool".into(),
            ..said("Connection pools")
        };
        let placed = place(
            &lines(),
            3,
            6,
            &candidates,
            &[by_candidate, by_quote, by_run, invented, elsewhere, outside],
        );
        let names: Vec<&str> = placed.iter().map(|p| p.mark.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Advisory locks",
                "Transaction-scoped locks",
                "Async functions"
            ]
        );
        let span = placed[1].mark.span.clone().expect("words");
        let words: String = lines()[2]
            .chars()
            .skip(span.from as usize)
            .take((span.to - span.from) as usize)
            .collect();
        assert_eq!((span.line, words.as_str()), (3, "released on commit"));
        assert_eq!(
            (
                placed[2].mark.start,
                placed[2].mark.end,
                &placed[2].mark.span
            ),
            (4, 6, &None)
        );
    }

    #[test]
    fn names_that_arent_plain_never_leave() {
        let bad = [
            said("sk-ant-api03-AbCdEf0123456789_AbCdEf0123456789-xyz"),
            said("https://example.com/secret"),
            said("x"),
        ];
        let placed = place(
            &lines(),
            1,
            6,
            &[],
            &bad.map(|s| Said {
                line: 1,
                quote: "sqlx".into(),
                ..s
            }),
        );
        assert!(placed.is_empty(), "{placed:?}");
    }

    #[test]
    fn a_passage_keeps_distinct_marks_that_dont_crowd() {
        let at = |line: u32, from: u32, to: u32, name: &str| Placed {
            mark: Mark {
                start: line,
                end: line,
                span: Some(Span { line, from, to }),
                name: name.into(),
                definition: String::new(),
                idea: None,
                use_: String::new(),
                area: String::new(),
                depth: Default::default(),
            },
        };
        let kept = keep(
            vec![
                at(1, 0, 4, "Advisory locks"),
                at(1, 4, 9, "Touching"),
                at(2, 0, 4, "Function"),
                at(3, 0, 4, "advisory lock"),
                at(4, 0, 4, "Idempotency"),
                at(5, 0, 4, "One too many"),
            ],
            10,
        );
        let names: Vec<&str> = kept.iter().map(|p| p.mark.name.as_str()).collect();
        assert_eq!(
            names,
            ["Advisory locks", "Function"],
            "two for ten lines; a basic idea is kept, for someone new to it"
        );
        assert_eq!(most_for(5), 1);
        assert_eq!(most_for(30), 4, "room for every idea");
        assert_eq!(most_for(200), MOST);
        // A pattern over lines, and an idea used inside it, are both kept.
        let run = Placed {
            mark: Mark {
                start: 1,
                end: 6,
                span: None,
                name: "Retry loop".into(),
                definition: String::new(),
                idea: None,
                use_: String::new(),
                area: String::new(),
                depth: Default::default(),
            },
        };
        let kept = keep(vec![run, at(3, 0, 4, "Jitter")], 10);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn an_idea_is_marked_where_it_first_comes_up() {
        let m = |line: u32, name: &str| Mark {
            start: line,
            end: line,
            span: None,
            name: name.into(),
            definition: String::new(),
            idea: None,
            use_: String::new(),
            area: String::new(),
            depth: Default::default(),
        };
        let marks = first_only(vec![
            m(3, "Advisory locks"),
            m(40, "advisory lock"),
            m(41, "Idempotency"),
            m(50, "Exponential backoff with jitter"),
            m(60, "Exponential backoff jitter"),
        ]);
        assert_eq!(
            marks.iter().map(|m| m.start).collect::<Vec<_>>(),
            [3, 41, 50]
        );
        // Close names the graph says are two ideas stay two.
        let mut a = m(1, "Git object model");
        a.idea = Some(IdeaRef {
            id: "kelve:iaaaaaaaaaaaa".into(),
            name: a.name.clone(),
        });
        let mut b = m(2, "Git object hashing");
        b.idea = Some(IdeaRef {
            id: "kelve:ibbbbbbbbbbbb".into(),
            name: b.name.clone(),
        });
        assert_eq!(first_only(vec![a.clone(), b]).len(), 2);
        assert_eq!(first_only(vec![a, m(3, "Git object hashing")]).len(), 1);
        assert_eq!(m(10, "x").shifted(-4).start, 6);
    }
}
