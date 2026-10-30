//! Ideas, as the shared knowledge graph holds them and everything in kelve
//! points at them: by ID, with the name they go by. The graph decides which
//! idea a name is (`kelve-graph-api`); here are only the shapes and the
//! rules every side agrees on.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The shared graph's prefix for its idea IDs: `kelve:i3k9xq2m7twa8`.
pub const SHARED: &str = "kelve";

/// How long an idea's minted part is, in Crockford base32 characters.
pub const ID_LEN: usize = 12;

/// An idea, as entries, marks and notes point at it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct IdeaRef {
    /// `kelve:` and its minted part: `kelve:i3k9xq2m7twa8`.
    pub id: String,
    /// The name it goes by: "Advisory locks".
    pub name: String,
}

/// Whether `id` is an idea ID: a prefix of lowercase letters and digits, a
/// colon, `i`, and the minted part in Crockford base32 (no i, l, o or u).
pub fn is_id(id: &str) -> bool {
    let Some((prefix, rest)) = id.split_once(':') else {
        return false;
    };
    let minted = rest.strip_prefix('i').unwrap_or_default();
    !prefix.is_empty()
        && prefix.len() <= 32
        && prefix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && minted.len() == ID_LEN
        && minted.chars().all(|c| {
            (c.is_ascii_lowercase() || c.is_ascii_digit()) && !matches!(c, 'i' | 'l' | 'o' | 'u')
        })
}

/// What an idea is.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum IdeaKind {
    /// An idea: ownership, consensus, amortisation.
    #[default]
    Concept,
    /// A tool people use: kubectl, Helm, cargo.
    Tool,
    /// A technology or standard: containerd, gRPC, OCI images.
    Technology,
    /// A way of doing things: blue-green deploys, code review.
    Practice,
}

impl IdeaKind {
    pub const ALL: [IdeaKind; 4] = [
        IdeaKind::Concept,
        IdeaKind::Tool,
        IdeaKind::Technology,
        IdeaKind::Practice,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            IdeaKind::Concept => "concept",
            IdeaKind::Tool => "tool",
            IdeaKind::Technology => "technology",
            IdeaKind::Practice => "practice",
        }
    }

    /// From a model's word for it; anything unknown is a concept.
    pub fn parse(s: &str) -> IdeaKind {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str() == s.trim())
            .unwrap_or_default()
    }
}

/// What a link in an entry says about the idea it leads to. The person
/// only ever sees words; this keeps the graph sound.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LinkKind {
    /// To understand this entry's idea, you need that one: going down.
    /// Never loops.
    RestsOn,
    /// This entry's idea is a kind of that one. Never loops.
    KindOf,
    /// A different answer to the same question.
    Alternative,
    /// Used alongside it.
    #[default]
    Related,
}

impl LinkKind {
    pub const ALL: [LinkKind; 4] = [
        LinkKind::RestsOn,
        LinkKind::KindOf,
        LinkKind::Alternative,
        LinkKind::Related,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LinkKind::RestsOn => "restsOn",
            LinkKind::KindOf => "kindOf",
            LinkKind::Alternative => "alternative",
            LinkKind::Related => "related",
        }
    }

    /// From a model's word for it; anything unknown is related.
    pub fn parse(s: &str) -> LinkKind {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(s.trim()))
            .unwrap_or_default()
    }
}

/// The longest name an idea goes by.
pub const NAME: usize = 80;

/// A name as it's written for people: its words, single-spaced, trimmed.
/// None when nothing usable is left, or it's too long to be a name.
pub fn tidy(name: &str) -> Option<String> {
    let words: Vec<&str> = name.split_whitespace().collect();
    let tidied = words.join(" ");
    let letters = tidied.chars().filter(|c| c.is_alphanumeric()).count();
    (letters >= 2 && tidied.chars().count() <= NAME).then_some(tidied)
}

/// A name as names are compared: lowercased, with only its letters and
/// digits, a space between each run of them. "Write-ahead logging" and
/// "write ahead  logging" are one name; "C" and "C++" are not.
pub fn key(name: &str) -> String {
    let mut key = String::with_capacity(name.len());
    let mut gap = false;
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            if gap && !key.is_empty() {
                key.push(' ');
            }
            gap = false;
            key.push(c);
        } else if c == '+' || c == '#' {
            // What tells C, C++ and C# apart.
            key.push(c);
        } else {
            gap = true;
        }
    }
    key
}

/// A name as names are matched: its `key`, with its last word made
/// singular. Applied the same way to every name, so a wrong guess
/// ("kubernete") still matches itself.
pub fn norm(name: &str) -> String {
    let key = key(name);
    match key.rsplit_once(' ') {
        Some((head, last)) => format!("{head} {}", singular(last)),
        None => singular(&key),
    }
}

/// A word without a plain English plural ending.
fn singular(word: &str) -> String {
    let n = word.chars().count();
    if n > 4 && word.ends_with("ies") {
        return format!("{}y", &word[..word.len() - 3]);
    }
    if n > 3 && word.ends_with('s') && !["ss", "us", "is", "os"].iter().any(|e| word.ends_with(e)) {
        return word[..word.len() - 1].to_string();
    }
    word.to_string()
}

/// Words that don't change which idea a name is: joining words, and the
/// generic tails a name for an idea can take ("Reservation pattern").
const FILLER: [&str; 16] = [
    "a", "an", "and", "the", "of", "for", "in", "on", "to", "with", "via", "using", "based",
    "model", "pattern", "approach",
];

/// A name's words as ideas are compared: no filler, no plurals, no "-ing",
/// and one spelling where British and American differ.
fn idea_words(name: &str) -> BTreeSet<String> {
    key(name)
        .split(' ')
        .filter(|w| !w.is_empty() && !FILLER.contains(w))
        .map(|w| {
            let w = one_spelling(&singular(w));
            match w.strip_suffix("ing") {
                Some(stem) if stem.chars().count() >= 4 => stem.to_string(),
                _ => w,
            }
        })
        .collect()
}

/// A word in one spelling where British and American differ: "minimisation"
/// and "minimization", "analyse" and "analyze", "behaviour" and "behavior".
/// Only for comparing: a name is never shown this way.
fn one_spelling(word: &str) -> String {
    const ENDINGS: [(&str, &str); 8] = [
        ("isation", "ization"),
        ("ising", "izing"),
        ("ised", "ized"),
        ("iser", "izer"),
        ("ise", "ize"),
        ("ysing", "yzing"),
        ("ysed", "yzed"),
        ("yse", "yze"),
    ];
    const OUR: [&str; 9] = [
        "behaviour",
        "colour",
        "favour",
        "flavour",
        "harbour",
        "honour",
        "labour",
        "neighbour",
        "rumour",
    ];
    if OUR.contains(&word) {
        return word.replacen("our", "or", 1);
    }
    for (gb, us) in ENDINGS {
        if let Some(stem) = word.strip_suffix(gb)
            && stem.chars().count() >= 3
        {
            return format!("{stem}{us}");
        }
    }
    word.to_string()
}

/// Whether a one-word name is the initials of a longer one's words, filler
/// aside: "PKCE" and "Proof Key for Code Exchange".
fn acronym_of(short: &str, long: &str) -> bool {
    let short = key(short);
    if short.contains(' ') || short.chars().count() < 2 {
        return false;
    }
    let initials: String = key(long)
        .split(' ')
        .filter(|w| !w.is_empty() && !FILLER.contains(w))
        .filter_map(|w| w.chars().next())
        .collect();
    initials.chars().count() >= 2 && initials == short
}

/// How alike two names are as ideas, for offering one when the other is
/// marked: the most for one idea or an acronym of it, else how many words
/// they share, 0 for none. "Data minimisation" and "Data minimization" are
/// one idea; "Transient errors" and "Transient fault" share a word.
pub fn likeness(a: &str, b: &str) -> usize {
    if same_idea(a, b) || acronym_of(a, b) || acronym_of(b, a) {
        return usize::MAX;
    }
    idea_words(a).intersection(&idea_words(b)).count()
}

/// Whether two names, marked in one file, are one idea: the same words, as
/// ideas are compared ("Exponential backoff with jitter", "Exponential
/// backoff jitter"); or one of at least two words, and the other the same
/// with one word more ("Git object model", "Git object hashing"). A name of
/// one word is never taken for a longer one: "Locks" and "Advisory locks"
/// are different ideas.
pub fn same_idea(a: &str, b: &str) -> bool {
    let (a, b) = (idea_words(a), idea_words(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (small, large) = if a.len() < b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    small.len() >= 2 && large.len() == small.len() + 1 && small.is_subset(large)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_alike_across_spellings_acronyms_and_shared_words() {
        for (a, b) in [
            ("Data minimisation", "Data minimization"),
            ("Authorisation hold", "Authorization hold"),
            ("Behaviour trees", "Behavior tree"),
            ("PKCE", "Proof Key for Code Exchange"),
            ("STRICT tables", "SQLite STRICT tables"),
            ("Directory traversal", "Directory traversal attack"),
        ] {
            assert_eq!(likeness(a, b), usize::MAX, "{a} / {b}");
            assert_eq!(likeness(b, a), usize::MAX, "{b} / {a}");
        }
        assert_eq!(likeness("Transient errors", "Transient fault"), 1);
        assert_eq!(likeness("Advisory locks", "Write-ahead logging"), 0);
        assert_eq!(
            likeness("C", "Compare and swap"),
            0,
            "one letter is no acronym"
        );
    }

    #[test]
    fn close_names_in_a_file_are_one_idea() {
        for (a, b) in [
            (
                "Exponential backoff with jitter",
                "Exponential backoff jitter",
            ),
            ("Git object model", "Git object hashing"),
            ("Git objects", "Git object model"),
            ("Reservation pattern", "Reservations"),
            ("Idempotency keys", "Idempotency key"),
            ("Lease-based locking", "Lease locking"),
        ] {
            assert!(same_idea(a, b), "{a} / {b}");
            assert!(same_idea(b, a), "{b} / {a}");
        }
        for (a, b) in [
            ("Leases", "Lease-based job claiming"),
            ("Locks", "Advisory locks"),
            ("Leases", "Least privilege"),
            ("Git hooks", "Git objects"),
            ("Token bucket", "Leaky bucket"),
            ("Write-ahead logging", "Write-ahead log checkpoint recovery"),
            ("C", "C++"),
            ("", "Leases"),
            ("with", "the"),
        ] {
            assert!(!same_idea(a, b), "{a} / {b}");
        }
    }

    #[test]
    fn ids_have_a_prefix_and_a_minted_part() {
        assert!(is_id("kelve:i3k9xq2m7twa8"));
        assert!(is_id("acme-corp:iabcdefghjkmn"));
        assert!(!is_id("kelve:3k9xq2m7twa8"), "no i");
        assert!(!is_id("kelve:i3k9xq2m7tw"), "too short");
        assert!(!is_id("kelve:i3k9xq2m7twal"), "l isn't Crockford");
        assert!(!is_id("Kelve:i3k9xq2m7twa8"));
        assert!(!is_id("i3k9xq2m7twa8"));
    }

    #[test]
    fn kinds_read_a_models_words() {
        assert_eq!(LinkKind::parse("restsOn"), LinkKind::RestsOn);
        assert_eq!(LinkKind::parse(" alternative "), LinkKind::Alternative);
        assert_eq!(LinkKind::parse("whatever"), LinkKind::Related);
        assert_eq!(IdeaKind::parse("tool"), IdeaKind::Tool);
        assert_eq!(IdeaKind::parse(""), IdeaKind::Concept);
    }

    #[test]
    fn names_are_tidied_or_refused() {
        assert_eq!(tidy("  advisory   locks "), Some("advisory locks".into()));
        assert_eq!(tidy("-"), None);
        assert_eq!(tidy(&"x".repeat(NAME + 1)), None);
    }

    #[test]
    fn names_compare_by_their_letters() {
        assert_eq!(key("Write-ahead  logging"), key("write ahead logging"));
        assert_eq!(key("  Advisory locks."), "advisory locks");
        assert_ne!(key("C"), key("C++"));
        assert_ne!(key("C++"), key("C#"));
        assert_eq!(key("Café"), "café");
    }

    #[test]
    fn names_match_whatever_their_plural() {
        assert_eq!(norm("Advisory locks"), norm("advisory-lock"));
        assert_eq!(norm("Dependencies"), norm("dependency"));
        assert_eq!(norm("Kubernetes"), norm("kubernetes"));
        assert_eq!(norm("Status"), "status");
        assert_eq!(norm("Analysis"), "analysis");
        assert_ne!(norm("C"), norm("C++"));
    }
}
