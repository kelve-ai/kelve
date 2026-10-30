//! Changing an entry the shared graph serves, a block at a time, so what was
//! checked stays as it was. There are three kinds of revision, each written
//! against a base revision:
//!
//! - **fix** replaces or removes only the blocks the graph's checks
//!   faulted;
//! - **extend** adds blocks, a fold, an example, a figure or an
//!   alternative, and touches nothing there already: it's how a sound
//!   entry grows;
//! - **rewrite** writes the whole entry again, only when the entry is poor
//!   or its problems cover more than half its blocks.
//!
//! A fix or an extend is a list of operations on block ids. The ids stay
//! from one revision to the next, so two revisions written against the
//! same base can both be taken when they touch different blocks; when they
//! touch the same one, the later is written again against the newer.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::check::{folds, words};
use super::{Block, Body, Entry, Fold, Part};

/// What kind of revision it is.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RevisionKind {
    /// Only the blocks found wrong, replaced or removed.
    Fix,
    /// Blocks added, nothing else touched.
    Extend,
    /// The whole entry, written again.
    Rewrite,
}

impl RevisionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RevisionKind::Fix => "fix",
            RevisionKind::Extend => "extend",
            RevisionKind::Rewrite => "rewrite",
        }
    }
}

/// One change to an entry's blocks, by id.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "camelCase")]
#[ts(export, rename = "BlockOp")]
pub enum Op {
    /// The block with this block's id, replaced by it.
    Replace { block: Block },
    /// A block taken out, and why.
    Remove { id: String, why: String },
    /// A new block, after the block with the id `after`, or first.
    Add {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
        block: Block,
    },
}

impl Op {
    /// The id of the block it changes or adds.
    pub fn id(&self) -> &str {
        match self {
            Op::Replace { block } | Op::Add { block, .. } => &block.id,
            Op::Remove { id, .. } => id,
        }
    }
}

/// What readers, or the graph's own grade, ask an entry to add.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AskKind {
    /// It needs a worked example.
    Example,
    /// It's too hard to follow.
    TooHard,
    /// Something's missing: a part, when the grade names one.
    Missing,
    /// It has no common mistake, set right.
    Mistake,
    /// It has nothing that looks like the idea but isn't.
    LookAlike,
}

impl AskKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AskKind::Example => "example",
            AskKind::TooHard => "tooHard",
            AskKind::Missing => "missing",
            AskKind::Mistake => "mistake",
            AskKind::LookAlike => "lookAlike",
        }
    }

    /// Whether a reader may ask it: the rest come from the graph's grade.
    pub fn from_readers(self) -> bool {
        matches!(self, AskKind::Example | AskKind::TooHard | AskKind::Missing)
    }

    /// What the writer is asked for, in a sentence.
    pub fn says(self, part: Option<Part>) -> String {
        let under = part
            .map(|p| format!(" under {}", p.as_str()))
            .unwrap_or_default();
        match self {
            AskKind::Example => format!(
                "Readers need an example: add one worked example, with real numbers, as an example fold{under}."
            ),
            AskKind::TooHard => format!(
                "Readers found it too hard to follow: add what makes the hard step concrete{under}, a worked example or a figure, one level down."
            ),
            AskKind::Missing => match part {
                Some(p) => format!(
                    "It has nothing under {}: add that part, shown, short.",
                    p.as_str()
                ),
                None => "Readers found something missing: add the one thing a newcomer most needs that it doesn't say.".to_string(),
            },
            AskKind::Mistake => format!(
                "It names no common mistake: add one as a mistake fold{under}, as the belief, why it's wrong, and what's true."
            ),
            AskKind::LookAlike => format!(
                "It shows nothing that looks like the idea but isn't: add one as a lookAlike fold{under}."
            ),
        }
    }
}

/// Applies operations to blocks, in order. Every id an operation names
/// must be there, except a block it adds, whose id must be new.
pub fn apply(blocks: &[Block], ops: &[Op]) -> Result<Vec<Block>, String> {
    let mut out = blocks.to_vec();
    for op in ops {
        match op {
            Op::Replace { block } => {
                let Some(at) = out.iter().position(|b| b.id == block.id) else {
                    return Err(format!(
                        "It replaces {}, which the entry hasn't got.",
                        block.id
                    ));
                };
                out[at] = block.clone();
            }
            Op::Remove { id, why } => {
                if why.trim().is_empty() {
                    return Err(format!("It removes {id} without saying why."));
                }
                let Some(at) = out.iter().position(|b| &b.id == id) else {
                    return Err(format!("It removes {id}, which the entry hasn't got."));
                };
                out.remove(at);
            }
            Op::Add { after, block } => {
                if out.iter().any(|b| b.id == block.id) {
                    return Err(format!(
                        "It adds {}, an id the entry already has. A new block needs an id no block has.",
                        block.id
                    ));
                }
                let at = match after {
                    None => 0,
                    Some(after) => match out.iter().position(|b| &b.id == after) {
                        Some(at) => at + 1,
                        None => {
                            return Err(format!(
                                "It adds {} after {after}, which the entry hasn't got.",
                                block.id
                            ));
                        }
                    },
                };
                out.insert(at, block.clone());
            }
        }
    }
    Ok(out)
}

/// The operations that turn `from` into `to`: each block gone, removed;
/// each changed, replaced; each new, added after the block before it.
/// Blocks keep their order, so moving one is removing it and adding it.
pub fn diff(from: &[Block], to: &[Block], why: &str) -> Vec<Op> {
    let kept: HashSet<&str> = to.iter().map(|b| b.id.as_str()).collect();
    let mut ops: Vec<Op> = from
        .iter()
        .filter(|b| !kept.contains(b.id.as_str()))
        .map(|b| Op::Remove {
            id: b.id.clone(),
            why: why.to_string(),
        })
        .collect();
    let mut before: Option<String> = None;
    for block in to {
        match from.iter().find(|b| b.id == block.id) {
            Some(was) if was == block => {}
            Some(_) => ops.push(Op::Replace {
                block: block.clone(),
            }),
            None => ops.push(Op::Add {
                after: before.clone(),
                block: block.clone(),
            }),
        }
        before = Some(block.id.clone());
    }
    ops
}

/// The ids of the blocks that differ between two lists: changed, gone or
/// new.
pub fn changed(from: &[Block], to: &[Block]) -> HashSet<String> {
    let mut out = HashSet::new();
    for b in from {
        if to.iter().find(|t| t.id == b.id) != Some(b) {
            out.insert(b.id.clone());
        }
    }
    for b in to {
        if !from.iter().any(|f| f.id == b.id) {
            out.insert(b.id.clone());
        }
    }
    out
}

/// The ids operations touch: the blocks they replace or remove, and the
/// blocks they add after, which must still be there.
pub fn touched(ops: &[Op]) -> HashSet<String> {
    let mut out = HashSet::new();
    for op in ops {
        match op {
            Op::Replace { block } => {
                out.insert(block.id.clone());
            }
            Op::Remove { id, .. } => {
                out.insert(id.clone());
            }
            Op::Add { after, .. } => {
                if let Some(after) = after {
                    out.insert(after.clone());
                }
            }
        }
    }
    out
}

/// What became of a revision written against an older base.
#[derive(Clone, Debug, PartialEq)]
pub enum Merged {
    /// Its operations, applied to the current blocks.
    Applied(Vec<Block>),
    /// It touches a block that changed since its base: written again
    /// against the current one.
    Redo,
}

/// Merges a revision's operations, written against `base`, onto
/// `current`: applied when they touch no block that changed since,
/// written again when they do.
pub fn merge(base: &[Block], current: &[Block], ops: &[Op]) -> Merged {
    if base == current {
        return apply(current, ops).map_or(Merged::Redo, Merged::Applied);
    }
    let moved = changed(base, current);
    if touched(ops).iter().any(|id| moved.contains(id))
        || ops
            .iter()
            .any(|op| matches!(op, Op::Add { .. }) && moved.contains(op.id()))
    {
        return Merged::Redo;
    }
    apply(current, ops).map_or(Merged::Redo, Merged::Applied)
}

/// Whether operations are what their kind may do: a fix replaces or
/// removes only `faulted` blocks; an extend only adds, each new block in
/// a part; a rewrite has none, since it's the whole entry.
pub fn fits(kind: RevisionKind, ops: &[Op], faulted: &[String]) -> Result<(), String> {
    match kind {
        RevisionKind::Rewrite if !ops.is_empty() => {
            Err("A rewrite is the whole entry, with no operations.".into())
        }
        RevisionKind::Rewrite => Ok(()),
        RevisionKind::Fix => {
            if ops.is_empty() {
                return Err("A fix changes at least one block.".into());
            }
            for op in ops {
                match op {
                    Op::Add { .. } => {
                        return Err(format!(
                            "A fix only replaces or removes blocks found wrong; it adds {}.",
                            op.id()
                        ));
                    }
                    _ if !faulted.iter().any(|f| f == op.id()) => {
                        return Err(format!(
                            "A fix changes only the blocks found wrong; {} wasn't.",
                            op.id()
                        ));
                    }
                    _ => {}
                }
            }
            Ok(())
        }
        RevisionKind::Extend => {
            if ops.is_empty() {
                return Err("An extend adds at least one block.".into());
            }
            for op in ops {
                match op {
                    Op::Add { block, .. } if block.part.is_some() => {}
                    Op::Add { block, .. } => {
                        return Err(format!(
                            "{} has no part: say which part it's under.",
                            block.id
                        ));
                    }
                    _ => {
                        return Err(format!(
                            "An extend only adds; it changes {}, which is there already.",
                            op.id()
                        ));
                    }
                }
            }
            Ok(())
        }
    }
}

/// Whether an entry is poor: written before entries had parts, or with no
/// lead. A poor entry may be written again whole.
pub fn poor(entry: &Entry) -> bool {
    entry.blocks.iter().any(|b| b.part.is_none())
        || !entry.blocks.iter().any(|b| b.part == Some(Part::Lead))
}

/// Whether an entry may be rewritten whole, rather than fixed a block at
/// a time: it's poor, or what's wrong covers more than half its blocks.
pub fn rewrite_allowed(entry: &Entry, faulted: &[String]) -> bool {
    poor(entry) || faulted.len() * 2 > entry.blocks.len()
}

/// The graph's own grade of a sound entry: what it lacks that a newcomer
/// needs, each as an ask and the part it goes under. Only an entry with
/// parts is graded; a poor one is written again when it's next asked
/// about.
pub fn grade(entry: &Entry) -> Vec<(AskKind, Option<Part>)> {
    if poor(entry) {
        return Vec::new();
    }
    let has_part = |p: Part| entry.blocks.iter().any(|b| b.part == Some(p));
    let has_fold = |f: Fold| folds(&entry.blocks).iter().any(|(_, k, _)| *k == f);
    let mut out = Vec::new();
    if !has_part(Part::How) {
        out.push((AskKind::Missing, Some(Part::How)));
    }
    if !has_fold(Fold::Example) {
        out.push((AskKind::Example, Some(Part::How)));
    }
    if !has_fold(Fold::Mistake) {
        out.push((AskKind::Mistake, Some(Part::Limits)));
    }
    if !has_fold(Fold::LookAlike) {
        out.push((AskKind::LookAlike, Some(Part::Limits)));
    }
    out
}

/// The blocks whose text cites source `n`, as `^n`.
pub fn citing(blocks: &[Block], n: u8) -> Vec<String> {
    let cite = format!("^{n}");
    blocks
        .iter()
        .filter(|b| {
            serde_json::to_string(&b.body).is_ok_and(|text| {
                text.match_indices(&cite).any(|(at, _)| {
                    !text[at + cite.len()..].starts_with(|c: char| c.is_ascii_digit())
                })
            })
        })
        .map(|b| b.id.clone())
        .collect()
}

/// An entry as a writer or a judge reads it to change it: each block on a
/// line, by its id, part and fold, with its words. Figures say what they
/// are and what they show in words.
pub fn outline(entry: &Entry) -> String {
    let mut out = format!("{}\n", entry.title);
    for b in &entry.blocks {
        let place = match (b.part, b.fold) {
            (Some(p), Some(f)) => format!("{}, {} fold", p.as_str(), f.as_str()),
            (Some(p), None) => p.as_str().to_string(),
            (None, _) => "no part".to_string(),
        };
        let text = match &b.body {
            Body::Paragraph { text, .. } | Body::Heading { text } => text.clone(),
            Body::List { items } => items.join(" / "),
            Body::Answer { lead, text } => format!("{lead} {text}"),
            Body::Number { figure, text, .. } => format!("{figure}: {text}"),
            other => {
                let mut texts = Vec::new();
                if let Ok(v) = serde_json::to_value(other) {
                    gather(&v, &mut texts);
                }
                texts.retain(|t| t != other.name());
                texts.join(" / ")
            }
        };
        out.push_str(&format!(
            "[{}] {} ({}, {} words): {}\n",
            b.id,
            b.body.name(),
            place,
            words(b),
            text
        ));
    }
    out
}

fn gather(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(s) if !s.trim().is_empty() => out.push(s.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|v| gather(v, out)),
        serde_json::Value::Object(fields) => fields.values().for_each(|v| gather(v, out)),
        _ => {}
    }
}

/// A new id for a block, one no block in `blocks` has: `b` and the next
/// number after the highest.
pub fn next_id(blocks: &[Block]) -> String {
    let most = blocks
        .iter()
        .filter_map(|b| b.id.strip_prefix('b')?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("b{}", most + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ideas::IdeaRef;
    use crate::time::Timestamp;

    fn p(id: &str, part: Part, text: &str) -> Block {
        Block {
            id: id.into(),
            part: Some(part),
            fold: None,
            body: Body::Paragraph {
                text: text.into(),
                quiet: false,
            },
        }
    }

    fn folded(id: &str, part: Part, fold: Fold, text: &str) -> Block {
        Block {
            fold: Some(fold),
            ..p(id, part, text)
        }
    }

    fn entry(blocks: Vec<Block>) -> Entry {
        Entry {
            idea: IdeaRef {
                id: "kelve:i1".into(),
                name: "Exponential backoff".into(),
            },
            title: "Exponential backoff".into(),
            question: String::new(),
            minutes: 2,
            made_at: Timestamp::now(),
            blocks,
            sources: Vec::new(),
            links: Vec::new(),
        }
    }

    fn base() -> Vec<Block> {
        vec![
            p("b1", Part::Lead, "Wait longer after each failure."),
            p(
                "b2",
                Part::Why,
                "Retrying at once makes a busy server busier^1.",
            ),
            p("b3", Part::How, "Each wait doubles."),
        ]
    }

    #[test]
    fn operations_apply_by_id() {
        let ops = vec![
            Op::Replace {
                block: p("b2", Part::Why, "Retrying at once adds load."),
            },
            Op::Add {
                after: Some("b3".into()),
                block: folded("b4", Part::How, Fold::Example, "1 s, 2 s, then 4 s."),
            },
            Op::Remove {
                id: "b1".into(),
                why: "Wrong.".into(),
            },
        ];
        let got = apply(&base(), &ops).unwrap();
        let ids: Vec<&str> = got.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["b2", "b3", "b4"]);
        assert_eq!(got[0], p("b2", Part::Why, "Retrying at once adds load."));

        let bad = |op: Op| apply(&base(), &[op]).unwrap_err();
        assert!(
            bad(Op::Replace {
                block: p("b9", Part::How, "x")
            })
            .contains("b9")
        );
        assert!(
            bad(Op::Add {
                after: None,
                block: p("b1", Part::How, "x")
            })
            .contains("already")
        );
        assert!(
            bad(Op::Add {
                after: Some("b9".into()),
                block: p("b4", Part::How, "x")
            })
            .contains("b9")
        );
        assert!(
            bad(Op::Remove {
                id: "b1".into(),
                why: " ".into()
            })
            .contains("why")
        );
    }

    #[test]
    fn a_diff_applied_gives_back_what_it_was_made_from() {
        let from = base();
        let mut to = base();
        to[1] = p("b2", Part::Why, "Changed.");
        to.insert(2, folded("b4", Part::Why, Fold::Mistake, "A belief."));
        to.remove(0);
        let ops = diff(&from, &to, "Found wrong.");
        assert_eq!(apply(&from, &ops).unwrap(), to);
        assert_eq!(
            changed(&from, &to),
            ["b1", "b2", "b4"].iter().map(|s| s.to_string()).collect()
        );
        assert!(diff(&from, &from, "").is_empty());
    }

    #[test]
    fn revisions_on_different_blocks_merge_and_on_the_same_block_are_written_again() {
        let b = base();
        let mut current = base();
        current[1] = p("b2", Part::Why, "Someone else fixed this.");
        // Adding under b3, which didn't change: taken onto the current.
        let add = vec![Op::Add {
            after: Some("b3".into()),
            block: folded("b4", Part::How, Fold::Example, "1 s, 2 s, 4 s."),
        }];
        match merge(&b, &current, &add) {
            Merged::Applied(blocks) => {
                assert_eq!(blocks[1], current[1], "the other change kept");
                assert_eq!(blocks.len(), 4);
            }
            Merged::Redo => panic!("should merge"),
        }
        // Replacing b2, which changed since: written again.
        let fix = vec![Op::Replace {
            block: p("b2", Part::Why, "Mine."),
        }];
        assert_eq!(merge(&b, &current, &fix), Merged::Redo);
        // Against the base itself, it just applies.
        assert!(matches!(merge(&b, &b, &fix), Merged::Applied(_)));
    }

    #[test]
    fn each_kind_does_only_what_it_may() {
        let replace = Op::Replace {
            block: p("b2", Part::Why, "x"),
        };
        let add = Op::Add {
            after: None,
            block: folded("b9", Part::How, Fold::Example, "x"),
        };
        let faulted = vec!["b2".to_string()];
        assert!(fits(RevisionKind::Fix, std::slice::from_ref(&replace), &faulted).is_ok());
        assert!(fits(RevisionKind::Fix, std::slice::from_ref(&replace), &[]).is_err());
        assert!(fits(RevisionKind::Fix, std::slice::from_ref(&add), &faulted).is_err());
        assert!(fits(RevisionKind::Extend, std::slice::from_ref(&add), &[]).is_ok());
        assert!(fits(RevisionKind::Extend, std::slice::from_ref(&replace), &[]).is_err());
        assert!(fits(RevisionKind::Rewrite, &[], &[]).is_ok());
        assert!(fits(RevisionKind::Rewrite, &[add], &[]).is_err());
    }

    #[test]
    fn a_poor_entry_may_be_rewritten_and_a_sound_one_only_when_mostly_wrong() {
        let sound = entry(base());
        assert!(!poor(&sound));
        assert!(!rewrite_allowed(&sound, &["b2".into()]));
        assert!(rewrite_allowed(&sound, &["b2".into(), "b3".into()]));
        let old = entry(vec![Block::new(
            "b1",
            Body::Paragraph {
                text: "No parts.".into(),
                quiet: false,
            },
        )]);
        assert!(poor(&old) && rewrite_allowed(&old, &[]));
    }

    #[test]
    fn the_grade_names_what_a_sound_entry_lacks() {
        let mut e = entry(base());
        let asked: Vec<AskKind> = grade(&e).into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            asked,
            vec![AskKind::Example, AskKind::Mistake, AskKind::LookAlike]
        );
        e.blocks
            .push(folded("b4", Part::How, Fold::Example, "1 s, 2 s."));
        assert!(!grade(&e).contains(&(AskKind::Example, Some(Part::How))));
        e.blocks.retain(|b| b.part != Some(Part::How));
        assert!(grade(&e).contains(&(AskKind::Missing, Some(Part::How))));
        // A poor entry isn't graded: it's written again.
        let old = entry(vec![Block::new("b1", Body::Heading { text: "x".into() })]);
        assert!(grade(&old).is_empty());
    }

    #[test]
    fn citing_finds_the_blocks_that_cite_a_source() {
        let mut blocks = base();
        blocks.push(p("b4", Part::How, "See^12."));
        assert_eq!(citing(&blocks, 1), vec!["b2".to_string()]);
        assert_eq!(citing(&blocks, 12), vec!["b4".to_string()]);
        assert_eq!(next_id(&blocks), "b5");
    }

    #[test]
    fn an_outline_shows_each_block_by_id_and_part() {
        let mut blocks = base();
        blocks.push(folded("b4", Part::How, Fold::Example, "1 s, 2 s."));
        let o = outline(&entry(blocks));
        assert!(
            o.contains("[b1] paragraph (lead, 5 words): Wait longer"),
            "{o}"
        );
        assert!(o.contains("[b4] paragraph (how, example fold"), "{o}");
    }
}
