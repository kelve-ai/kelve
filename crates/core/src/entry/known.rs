//! Linking what the writer left unlinked: every mention of an idea the
//! graph already has, in an entry's prose, becomes a way on to it. Code
//! does it, by name, so an idea the graph knows is never missed because a
//! model didn't mark it. Only the first mention is linked, and only whole words
//! outside code and outside terms already linked.

use super::check::{LINKED, Span, spans};
use super::{Block, Body, Link};
use crate::ideas::{IdeaRef, LinkKind};

/// Names shorter than this are too likely to be ordinary words ("go",
/// "pod" stays, "ui" doesn't).
const SHORTEST: usize = 3;

/// An idea the graph has, by one of the names it goes by, in words: "bind
/// mounts", "kubectl".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    pub words: String,
    pub idea: IdeaRef,
}

/// Links the first mention of each named idea in paragraphs and lists that
/// the entry doesn't link already, and gives the links it added. The entry's
/// own idea is never linked.
pub fn link_known(blocks: &mut [Block], names: &[Named], own: &str, links: &[Link]) -> Vec<Link> {
    let mut names: Vec<&Named> = names
        .iter()
        .filter(|n| n.idea.id != own && n.words.trim().chars().count() >= SHORTEST)
        .collect();
    // Longest first, so "named volume" wins over "volume".
    names.sort_by_key(|n| std::cmp::Reverse(n.words.chars().count()));
    let mut linked: Vec<&str> = links.iter().map(|l| l.idea.id.as_str()).collect();
    let mut added = Vec::new();
    for block in blocks.iter_mut() {
        let texts: Vec<&mut String> = match &mut block.body {
            Body::Paragraph { text, .. } => vec![text],
            Body::List { items } => items.iter_mut().collect(),
            _ => continue,
        };
        for text in texts {
            for (i, name) in names.iter().enumerate() {
                if linked.contains(&name.idea.id.as_str()) {
                    continue;
                }
                // A longer name holding this one ("named volume" holds
                // "volume") is a mention of that, not of this.
                let within: Vec<String> = names[..i]
                    .iter()
                    .map(|n| n.words.trim().to_lowercase())
                    .filter(|w| w.contains(&name.words.trim().to_lowercase()))
                    .collect();
                if let Some((next, term)) = wrap(text, &name.words, &within) {
                    *text = next;
                    linked.push(name.idea.id.as_str());
                    added.push(Link {
                        term,
                        idea: name.idea.clone(),
                        kind: LinkKind::Related,
                    });
                }
            }
        }
    }
    added
}

/// The text with the first whole-word mention of `words` outside code and
/// linked terms wrapped as `[[…]]`, and the words as written; None when
/// there's no such mention.
fn wrap(text: &str, words: &str, within: &[String]) -> Option<(String, String)> {
    let pieces = spans(text)?;
    let mut out = String::with_capacity(text.len() + 4);
    let mut found = None;
    for piece in pieces {
        match piece {
            Span::Code(code) => out.push_str(code),
            Span::Text(prose) => {
                if found.is_some() {
                    out.push_str(prose);
                    continue;
                }
                // Terms already linked are left as they are.
                let mut last = 0;
                for m in LINKED.find_iter(prose) {
                    let before = &prose[last..m.start()];
                    match (found.is_none(), find(before, words, within)) {
                        (true, Some((a, b))) => {
                            found = Some(before[a..b].to_string());
                            out.push_str(&format!(
                                "{}[[{}]]{}",
                                &before[..a],
                                &before[a..b],
                                &before[b..]
                            ));
                        }
                        _ => out.push_str(before),
                    }
                    out.push_str(m.as_str());
                    last = m.end();
                }
                let rest = &prose[last..];
                match (found.is_none(), find(rest, words, within)) {
                    (true, Some((a, b))) => {
                        found = Some(rest[a..b].to_string());
                        out.push_str(&format!("{}[[{}]]{}", &rest[..a], &rest[a..b], &rest[b..]));
                    }
                    _ => out.push_str(rest),
                }
            }
        }
    }
    found.map(|term| (out, term))
}

/// Where `words` first appear in `text` as whole words, ignoring case,
/// and not as part of one of the longer names `within`.
fn find(text: &str, words: &str, within: &[String]) -> Option<(usize, usize)> {
    let hay = text.to_lowercase();
    let needle = words.trim().to_lowercase();
    // Lower-casing can change byte lengths outside ASCII: only match where it didn't.
    if needle.is_empty() || hay.len() != text.len() {
        return None;
    }
    let mut from = 0;
    while let Some(at) = hay[from..].find(&needle) {
        let a = from + at;
        let b = a + needle.len();
        let before = hay[..a].chars().next_back();
        let after = hay[b..].chars().next();
        let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '-');
        let inside = within.iter().any(|w| {
            hay.match_indices(w.as_str())
                .any(|(s, m)| s <= a && b <= s + m.len())
        });
        if edge(before) && edge(after) && !inside {
            return Some((a, b));
        }
        from = a + needle.len().max(1);
    }
    None
}

/// Unlinks `terms`: wherever an entry writes `[[term]]` for one of them,
/// it becomes plain `term`. For links that lead to no idea, such as one
/// withdrawn since the entry was written.
pub fn unlink(blocks: &mut [Block], terms: &[String]) {
    if terms.is_empty() {
        return;
    }
    for block in blocks.iter_mut() {
        let Ok(mut value) = serde_json::to_value(&block.body) else {
            continue;
        };
        plain(&mut value, terms);
        if let Ok(body) = serde_json::from_value(value) {
            block.body = body;
        }
    }
}

fn plain(value: &mut serde_json::Value, terms: &[String]) {
    match value {
        serde_json::Value::String(text) => {
            let next = LINKED.replace_all(text, |c: &regex::Captures<'_>| {
                let term = c.get(1).map_or("", |m| m.as_str());
                if terms
                    .iter()
                    .any(|t| t.trim().eq_ignore_ascii_case(term.trim()))
                {
                    term.to_string()
                } else {
                    c[0].to_string()
                }
            });
            if let std::borrow::Cow::Owned(next) = next {
                *text = next;
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(|v| plain(v, terms)),
        serde_json::Value::Object(fields) => fields.values_mut().for_each(|v| plain(v, terms)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(words: &str, n: u128) -> Named {
        Named {
            words: words.into(),
            idea: IdeaRef {
                id: format!("kelve:i{n:0>12}"),
                name: words.into(),
            },
        }
    }

    fn paragraph(text: &str) -> Block {
        Block {
            id: "b1".into(),
            part: None,
            fold: None,
            body: Body::Paragraph {
                text: text.into(),
                quiet: false,
            },
        }
    }

    fn text(b: &Block) -> &str {
        match &b.body {
            Body::Paragraph { text, .. } => text,
            _ => "",
        }
    }

    #[test]
    fn known_nodes_are_linked_once_as_written() {
        let mut blocks = vec![
            paragraph("A Named Volume isn't a bind mount; `volume` in code isn't linked."),
            paragraph("Another named volume, and the [[kubelet]]."),
        ];
        let names = [
            named("named volume", 1),
            named("volume", 2),
            named("kubelet", 3),
            named("go", 4),
        ];
        let added = link_known(&mut blocks, &names, "kelve:i000000000009", &[]);
        assert_eq!(
            text(&blocks[0]),
            "A [[Named Volume]] isn't a bind mount; `volume` in code isn't linked."
        );
        assert_eq!(
            text(&blocks[1]),
            "Another named volume, and the [[kubelet]]."
        );
        let terms: Vec<&str> = added.iter().map(|l| l.term.as_str()).collect();
        assert_eq!(
            terms,
            ["Named Volume"],
            "kubelet is already linked; volume only in code"
        );
    }

    #[test]
    fn only_whole_words_and_never_the_entry_itself() {
        let mut blocks = vec![paragraph("Volumes, involve, and pods-ish words; the pod.")];
        let names = [named("volume", 1), named("pod", 2), named("pods", 3)];
        let added = link_known(&mut blocks, &names, "kelve:i000000000002", &[]);
        assert_eq!(
            text(&blocks[0]),
            "Volumes, involve, and pods-ish words; the pod."
        );
        assert!(added.is_empty());
    }

    #[test]
    fn terms_that_lead_nowhere_become_plain_words() {
        let mut blocks = vec![Block {
            id: "b1".into(),
            part: None,
            fold: None,
            body: Body::List {
                items: vec![
                    "A [[named volume]] outlives its [[container]].".into(),
                    "Unlike a [[Bind mount]].".into(),
                ],
            },
        }];
        unlink(&mut blocks, &["bind mount".into(), "container".into()]);
        let items = match &blocks[0].body {
            Body::List { items } => items.clone(),
            _ => Vec::new(),
        };
        assert_eq!(
            items,
            [
                "A [[named volume]] outlives its container.",
                "Unlike a Bind mount."
            ]
        );
    }
}
