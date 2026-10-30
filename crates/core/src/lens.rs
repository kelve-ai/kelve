//! What a reading looks for: by area, how deep, and the ideas always marked,
//! from what the person said kelve marks for them (`marks.md`). Each mark says
//! its area and how deep it is, so what was read for one person serves another
//! whose lens it covers, and what they ask for beyond it is read once, and kept
//! for everyone.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ideas::norm;

/// How deep an idea is in its area: who already knows it.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS,
)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Depth {
    /// Anyone who uses the area knows it: a language's functions, loops,
    /// types.
    Basic,
    /// Anyone working in the area knows it.
    #[default]
    Common,
    /// Someone experienced in the area knows it.
    Advanced,
    /// Only specialists know it.
    Specialist,
}

impl Depth {
    pub fn word(self) -> &'static str {
        match self {
            Depth::Basic => "basic",
            Depth::Common => "common",
            Depth::Advanced => "advanced",
            Depth::Specialist => "specialist",
        }
    }

    /// A depth as the agent says it; anything else is common.
    pub fn from_word(word: &str) -> Depth {
        match word.trim().to_lowercase().as_str() {
            "basic" => Depth::Basic,
            "advanced" => Depth::Advanced,
            "specialist" => Depth::Specialist,
            _ => Depth::Common,
        }
    }
}

/// How well the person knows an area: the depths kelve looks for and marks
/// for them there.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS,
)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Level {
    /// Every idea, the basics too.
    New,
    /// All but the basics.
    #[default]
    Learning,
    /// Advanced and specialist ideas.
    Working,
    /// Specialist ideas only.
    Deep,
}

impl Level {
    /// Whether ideas this deep are looked for, and marked, at this level.
    pub fn looks_for(self, depth: Depth) -> bool {
        depth as u8 >= self as u8
    }

    fn says(self) -> &'static str {
        match self {
            Level::New => "ideas of every depth, the basics too",
            Level::Learning => "common, advanced and specialist ideas",
            Level::Working => "advanced and specialist ideas",
            Level::Deep => "specialist ideas only",
        }
    }
}

/// An area's name as kelve compares it: lower case, single spaces.
pub fn area(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// What a reading looks for. A reading made with one lens serves anyone
/// whose lens it covers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Lens {
    /// How deep to look in an area not named.
    pub level: Level,
    /// How deep to look in each named area, by `area`'s name.
    pub areas: BTreeMap<String, Level>,
    /// Ideas looked for whatever their depth, by name, as the person
    /// wrote them; compared as `ideas::norm` names them.
    pub always: BTreeSet<String>,
    /// The person's own words on what to look for, by a print of them: the
    /// words go to their agent, never into what's kept.
    pub words: BTreeSet<String>,
}

impl Lens {
    /// How deep this lens looks in an area.
    pub fn level_of(&self, name: &str) -> Level {
        self.areas.get(&area(name)).copied().unwrap_or(self.level)
    }

    /// Every area either lens names.
    fn named<'a>(&'a self, other: &'a Lens) -> BTreeSet<&'a String> {
        self.areas.keys().chain(other.areas.keys()).collect()
    }

    /// Whether a reading with this lens found all that `wanted` looks for:
    /// as deep in every area, every idea it always wants, and its words.
    pub fn covers(&self, wanted: &Lens) -> bool {
        self.level <= wanted.level
            && self
                .named(wanted)
                .into_iter()
                .all(|a| self.level_of(a) <= wanted.level_of(a))
            && wanted.always.iter().all(|a| self.always_wants(a))
            && wanted.words.is_subset(&self.words)
    }

    /// What two readings looked for together.
    pub fn widen(&self, other: &Lens) -> Lens {
        let level = self.level.min(other.level);
        let areas = self
            .named(other)
            .into_iter()
            .map(|a| (a.clone(), self.level_of(a).min(other.level_of(a))))
            .filter(|(_, l)| *l != level)
            .collect();
        Lens {
            level,
            areas,
            always: self
                .always
                .iter()
                .chain(other.always.iter().filter(|a| !self.always_wants(a)))
                .cloned()
                .collect(),
            words: self.words.union(&other.words).cloned().collect(),
        }
    }

    /// Whether this lens looks for, and marks, an idea of this area and
    /// depth, by this name.
    pub fn wants(&self, area_name: &str, depth: Depth, name: &str) -> bool {
        self.always_wants(name) || self.level_of(area_name).looks_for(depth)
    }

    /// Whether an idea is looked for whatever its depth.
    fn always_wants(&self, name: &str) -> bool {
        let n = norm(name);
        self.always.iter().any(|a| norm(a) == n)
    }

    /// What to look for, in words for the agent.
    pub fn says(&self) -> String {
        let mut out: Vec<String> = self
            .areas
            .iter()
            .map(|(a, l)| format!("in {a}, {}", l.says()))
            .collect();
        out.push(format!(
            "{} {}",
            if self.areas.is_empty() {
                "everywhere,"
            } else {
                "anywhere else,"
            },
            self.level.says()
        ));
        let mut said = format!("Look for: {}.", out.join("; "));
        if !self.always.is_empty() {
            let always: Vec<&str> = self.always.iter().map(String::as_str).collect();
            said.push_str(&format!(
                " Mark these whatever their depth: {}.",
                always.join(", ")
            ));
        }
        said
    }

    /// The lens as a short, stable string: what names a reading made with it.
    pub fn print(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lens(level: Level, areas: &[(&str, Level)]) -> Lens {
        Lens {
            level,
            areas: areas.iter().map(|(a, l)| (area(a), *l)).collect(),
            ..Lens::default()
        }
    }

    #[test]
    fn a_reading_serves_anyone_who_asks_for_no_more_than_it_looked_for() {
        let everything = lens(Level::New, &[]);
        let rust_deep = lens(Level::Learning, &[("Rust", Level::Deep)]);
        let rust_new = lens(Level::Learning, &[("rust", Level::New)]);
        assert!(everything.covers(&rust_deep));
        assert!(everything.covers(&rust_new));
        assert!(
            rust_new.covers(&rust_deep),
            "it looked deeper in Rust, and the same elsewhere"
        );
        assert!(
            !rust_deep.covers(&rust_new),
            "Rust's basics weren't looked for"
        );
        assert!(!rust_deep.covers(&lens(Level::New, &[])));
        // Ideas always wanted, and the person's own words, are looked for once.
        let mut wants_keys = rust_deep.clone();
        wants_keys.always.insert("Idempotency keys".into());
        assert!(!rust_deep.covers(&wants_keys));
        assert!(rust_deep.widen(&wants_keys).covers(&wants_keys));
    }

    #[test]
    fn widened_a_reading_covers_both_and_no_more() {
        let a = lens(Level::Working, &[("rust", Level::New)]);
        let b = lens(Level::Learning, &[("databases", Level::Deep)]);
        let both = a.widen(&b);
        assert!(both.covers(&a) && both.covers(&b));
        assert_eq!(both.level_of("rust"), Level::New);
        assert_eq!(
            both.level_of("databases"),
            Level::Working,
            "looking for as much as either did"
        );
        assert_eq!(both.level_of("anything"), Level::Learning);
        assert!(!both.covers(&lens(Level::New, &[])));
    }

    #[test]
    fn what_is_marked_follows_the_level_and_ideas_always_wanted() {
        let mut l = lens(Level::Working, &[("rust", Level::New)]);
        assert!(l.wants("Rust", Depth::Basic, "Loop"));
        assert!(!l.wants("databases", Depth::Common, "Index"));
        assert!(l.wants("databases", Depth::Advanced, "Write-ahead logging"));
        l.always.insert("Index".into());
        assert!(l.wants("databases", Depth::Common, "index"));
        assert!(l.says().contains("in rust, ideas of every depth"));
        assert!(
            l.says()
                .contains("anywhere else, advanced and specialist ideas")
        );
        assert_eq!(Depth::from_word("Specialist"), Depth::Specialist);
        assert_eq!(Depth::from_word("whatever"), Depth::Common);
    }
}
