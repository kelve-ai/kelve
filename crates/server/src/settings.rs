//! The person's settings, as kelve keeps them: what the settings page
//! shows.

use kelve_core::views::{AgentKind, Highlighting, Place, SettingsIn, Size};
use serde::{Deserialize, Serialize};

/// The hints kelve shows, each once.
/// The hints shown once, and the offers made once: the notes hook.
// `started`: the person has been through the first run, so what their agent
// opens goes straight to the reader. `counts`: the person answered whether to
// share counts.
const HINTS: [&str; 7] = [
    "squares", "notes", "hook", "graph", "marks", "started", "counts",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Whether kelve adds to the shared knowledge graph as the person works.
    pub contribute: bool,
    /// Whether kelve sends the shared graph a weekly count of how it was
    /// used.
    pub share_counts: bool,
    /// The install's ID, sent with those counts: 32 random hex characters,
    /// made when the person says yes and gone when they stop.
    pub install: Option<String>,
    /// The folder notes kept only for the person go in, when they chose
    /// one: kelve's own otherwise.
    pub notes_dir: Option<String>,
    /// The coding agent the person uses.
    pub agent: AgentKind,
    pub code_size: Size,
    pub text_size: Size,
    pub highlighting: Highlighting,
    /// Whether what's been read is kept in the project, committed with it.
    pub keep_read_in_repo: bool,
    /// Who a new note is for.
    pub new_note: Place,
    /// The hints seen.
    pub hints: Vec<String>,
    /// How many times the person said no to adding what their agent wrote
    /// to the shared graph, and the last day they did.
    pub add_nos: u8,
    pub add_no_on: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            contribute: true,
            share_counts: false,
            install: None,
            notes_dir: None,
            agent: AgentKind::default(),
            code_size: Size::default(),
            text_size: Size::default(),
            highlighting: Highlighting::default(),
            keep_read_in_repo: true,
            new_note: Place::default(),
            hints: Vec::new(),
            add_nos: 0,
            add_no_on: None,
        }
    }
}

impl Settings {
    /// The settings as the API shows them.
    pub fn view(&self) -> kelve_core::views::Settings {
        kelve_core::views::Settings {
            contribute: self.contribute,
            share_counts: self.share_counts,
            counts_off: None,
            notes_dir: self.notes_dir.clone().unwrap_or_default(),
            notes_chosen: self.notes_dir.is_some(),
            agent: self.agent,
            code_size: self.code_size,
            text_size: self.text_size,
            highlighting: self.highlighting,
            keep_read_in_repo: self.keep_read_in_repo,
            new_note: self.new_note,
            hints: self.hints.clone(),
            notes_hook: false,
            hooks_elsewhere: Vec::new(),
            ask_to_add: self.add_nos < 2
                && self.add_no_on.as_deref()
                    != Some(&chrono::Utc::now().format("%Y-%m-%d").to_string()),
        }
    }

    /// Applies a change from the settings page, or says, in a sentence,
    /// what's wrong with it.
    pub fn apply(&mut self, change: SettingsIn) -> Result<(), String> {
        // Adding what your agent writes and learns is part of using the
        // shared graph free (Nathan, 26 October): it can't be turned off;
        // kelve runs without the graph instead.
        if change.contribute == Some(false) {
            return Err("Adding what your agent writes and learns is part of using the shared graph. To use kelve without it, set KELVE_GRAPH_URL=off where kelve runs.".into());
        }
        if let Some(share) = change.share_counts {
            self.share_counts = share;
            // Its ID is made on yes and goes on stop: sharing again is a
            // new install, as far as anyone can tell.
            self.install = match (share, self.install.take()) {
                (true, Some(id)) => Some(id),
                (true, None) => Some(crate::crypto::install_id()),
                (false, _) => None,
            };
        }
        if let Some(dir) = change.notes_dir {
            let dir = dir.trim();
            self.notes_dir = if dir.is_empty() {
                None
            } else {
                let path = crate::reader::unshown(dir);
                if !path.is_absolute() || !path.is_dir() {
                    return Err("Choose a folder that's there, for your notes.".into());
                }
                Some(path.display().to_string())
            };
        }
        if let Some(agent) = change.agent {
            self.agent = agent;
        }
        if let Some(size) = change.code_size {
            self.code_size = size;
        }
        if let Some(size) = change.text_size {
            self.text_size = size;
        }
        if let Some(h) = change.highlighting {
            self.highlighting = h;
        }
        if let Some(keep) = change.keep_read_in_repo {
            self.keep_read_in_repo = keep;
        }
        if let Some(place) = change.new_note {
            self.new_note = place;
        }
        if change.keep_to_myself == Some(true) {
            self.add_nos = self.add_nos.saturating_add(1);
            self.add_no_on = Some(chrono::Utc::now().format("%Y-%m-%d").to_string());
        }
        if let Some(hint) = change.hint_seen {
            if !HINTS.contains(&hint.as_str()) {
                return Err("That isn't a hint kelve shows.".into());
            }
            if !self.hints.contains(&hint) {
                self.hints.push(hint);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn bad_changes_say_why_and_good_ones_stick() {
        let mut s = Settings::default();
        assert!(s.contribute, "growing the graph is part of using it");
        assert!(
            s.clone()
                .apply(SettingsIn {
                    contribute: Some(false),
                    ..SettingsIn::default()
                })
                .is_err(),
            "and can't be turned off"
        );
        assert!(
            s.keep_read_in_repo,
            "what's read is shared with the project"
        );
        let err = s
            .apply(SettingsIn {
                notes_dir: Some("/no/such/folder/for/notes".into()),
                ..SettingsIn::default()
            })
            .expect_err("refused");
        assert!(err.contains("folder"), "{err}");
        s.apply(SettingsIn {
            share_counts: Some(true),
            agent: Some(AgentKind::Codex),
            code_size: Some(Size::Large),
            highlighting: Some(Highlighting::None),
            new_note: Some(Place::Work),
            hint_seen: Some("squares".into()),
            ..SettingsIn::default()
        })
        .expect("applied");
        s.apply(SettingsIn {
            hint_seen: Some("squares".into()),
            ..SettingsIn::default()
        })
        .expect("seen again");
        let v = s.view();
        assert_eq!(v.agent, AgentKind::Codex);
        assert_eq!(v.code_size, Size::Large);
        assert_eq!(v.text_size, Size::Medium);
        assert_eq!(v.highlighting, Highlighting::None);
        assert_eq!(v.new_note, Place::Work);
        assert_eq!(v.hints, vec!["squares".to_string()], "once each");
        assert!(v.contribute);
        assert!(v.share_counts);
        let id = s.install.clone().expect("an install ID, made on yes");
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        s.apply(SettingsIn {
            share_counts: Some(true),
            ..SettingsIn::default()
        })
        .expect("yes again");
        assert_eq!(
            s.install.as_deref(),
            Some(id.as_str()),
            "kept while sharing"
        );
        s.apply(SettingsIn {
            share_counts: Some(false),
            ..SettingsIn::default()
        })
        .expect("stopped");
        assert_eq!(s.install, None, "deleted on stop");
        s.apply(SettingsIn {
            share_counts: Some(true),
            ..SettingsIn::default()
        })
        .expect("yes once more");
        assert_ne!(s.install.as_deref(), Some(id.as_str()), "a new one");
        assert!(
            s.apply(SettingsIn {
                hint_seen: Some("<script>".into()),
                ..SettingsIn::default()
            })
            .is_err()
        );
        assert!(
            !Settings::default().share_counts,
            "off until the person says"
        );
    }

    #[test]
    fn kelve_asks_to_add_at_most_once_a_day_and_never_after_a_second_no() {
        let mut s = Settings::default();
        assert!(s.view().ask_to_add);
        let no = SettingsIn {
            keep_to_myself: Some(true),
            ..SettingsIn::default()
        };
        s.apply(no.clone()).expect("said no");
        assert!(!s.view().ask_to_add, "not again today");
        s.add_no_on = Some("2026-01-01".into());
        assert!(s.view().ask_to_add, "another day");
        s.apply(no).expect("said no again");
        s.add_no_on = Some("2026-01-01".into());
        assert!(!s.view().ask_to_add, "never after a second no");
    }
}
