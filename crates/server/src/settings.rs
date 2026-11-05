//! The person's settings, as kelve keeps them: what the settings page
//! shows.

use kelve_core::due::DebriefsWhen;
use kelve_core::views::{AgentKind, ChecksWhen, Highlighting, SettingsIn, Size};
use serde::{Deserialize, Serialize};

/// The hints shown once, and the offers made once: the hook.
// `counts`: the person answered whether to share counts. `met`: the person
// has met their agent, so home is home. `example`: the person has opened the
// example from home, so it isn't offered again.
const HINTS: [&str; 4] = ["hook", "counts", "met", "example"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Whether kelve sends its hosted service a weekly count of how it was
    /// used.
    pub share_counts: bool,
    /// The install's ID, sent with those counts: 32 random hex characters,
    /// made when the person says yes and gone when they stop.
    pub install: Option<String>,
    /// The coding agent the person uses.
    pub agent: AgentKind,
    pub code_size: Size,
    pub text_size: Size,
    pub highlighting: Highlighting,
    /// The hints seen.
    pub hints: Vec<String>,
    /// How the person's agent looks, the name they gave it, and what it
    /// drew for them (`your_agent`).
    pub look: kelve_core::character::Look,
    pub agent_name: Option<String>,
    pub drawn: crate::your_agent::Drawn,
    /// How often a debrief comes.
    pub debriefs: DebriefsWhen,
    /// Whether skills go in each agent's own skills folder too: on for a new
    /// install (`Default`). Settings kept from before there was this setting
    /// don't have it, and that stays off until the person says.
    #[serde(default)]
    pub skills_folder: bool,
    /// When skill checks run, and how many a week at most.
    pub checks: ChecksWhen,
    pub checks_a_week: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            share_counts: false,
            install: None,
            agent: AgentKind::default(),
            code_size: Size::default(),
            text_size: Size::default(),
            highlighting: Highlighting::default(),
            hints: Vec::new(),
            look: kelve_core::character::Look::default(),
            agent_name: None,
            drawn: crate::your_agent::Drawn::default(),
            debriefs: DebriefsWhen::default(),
            skills_folder: true,
            checks: ChecksWhen::default(),
            checks_a_week: 10,
        }
    }
}

impl Settings {
    /// The settings as the API shows them.
    pub fn view(&self) -> kelve_core::views::Settings {
        kelve_core::views::Settings {
            share_counts: self.share_counts,
            counts_off: None,
            agent: self.agent,
            code_size: self.code_size,
            text_size: self.text_size,
            highlighting: self.highlighting,
            hints: self.hints.clone(),
            hook: false,
            hooks_elsewhere: Vec::new(),
        }
    }

    /// Applies a change from the settings page, or says, in a sentence,
    /// what's wrong with it.
    pub fn apply(&mut self, change: SettingsIn) -> Result<(), String> {
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
        s.apply(SettingsIn {
            share_counts: Some(true),
            agent: Some(AgentKind::Codex),
            code_size: Some(Size::Large),
            highlighting: Some(Highlighting::None),
            hint_seen: Some("met".into()),
            ..SettingsIn::default()
        })
        .expect("applied");
        s.apply(SettingsIn {
            hint_seen: Some("met".into()),
            ..SettingsIn::default()
        })
        .expect("seen again");
        let v = s.view();
        assert_eq!(v.agent, AgentKind::Codex);
        assert_eq!(v.code_size, Size::Large);
        assert_eq!(v.text_size, Size::Medium);
        assert_eq!(v.highlighting, Highlighting::None);
        assert_eq!(v.hints, vec!["met".to_string()], "once each");
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
        assert!(Settings::default().skills_folder, "on for a new install");
        let kept: Settings =
            serde_json::from_value(serde_json::json!({ "shareCounts": false })).expect("reads");
        assert!(!kept.skills_folder, "off for settings kept from before it");
    }
}
