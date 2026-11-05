//! kelve.ai from the app (docs/skills.md, "Proof: two scores" and "The
//! hosted service and pages"): what the screens for an account, a page's
//! address, a verified run and its score are sent, and making a skill's
//! text fit to leave: the only thing of the person's that ever goes, and
//! only for a run they asked for.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::redact::redact;
use crate::text::re;
use crate::time::Timestamp;
use crate::views::ScoreSquare;

/// Paths on someone's computer, where they begin: `/Users/…`, `/home/…`,
/// `C:\…`, `~/…`, to the next space, quote or bracket. A path inside a
/// project, `src/main.rs`, is a skill's to keep.
static PATH: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r#"(^|[\s"'`(\[=,])((?:[A-Za-z]:\\|~/|/(?:Users|home|private|var|tmp|opt|root|mnt|Volumes)/)[^\s"'`<>|()\[\]{},;]*)"#,
    )
});

/// A skill's file as it leaves for a verified run: every secret it holds
/// replaced by `[secret]`, and every path on someone's computer by
/// `[path]`.
pub fn for_sending(text: &str) -> String {
    let text = redact(text);
    PATH.replace_all(&text, "${1}[path]").into_owned()
}

/// A model provider's name as people read it: `anthropic` is Anthropic.
pub fn provider_name(provider: &str) -> String {
    match provider {
        "anthropic" => "Anthropic".into(),
        "openai" => "OpenAI".into(),
        "google" => "Google".into(),
        other => other.into(),
    }
}

/// What pays for a verified run's agent: an API key, billed by the
/// provider, or, where the service allows it (one the person runs
/// locally), their Claude subscription, through a token from
/// `claude setup-token`, which uses their plan's usage.
///
/// TODO: kelve's own until the hosted service's v2 shapes say which kinds
/// a run's offer accepts and `Confirm` carries one; switch to theirs then.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS,
)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum CredentialKind {
    #[default]
    ApiKey,
    Subscription,
}

impl CredentialKind {
    /// Where it's kept, sealed, for a provider: `keys.provider.anthropic`,
    /// `keys.subscription.anthropic`.
    pub fn prefix(self) -> &'static str {
        match self {
            CredentialKind::ApiKey => "keys.provider.",
            CredentialKind::Subscription => "keys.subscription.",
        }
    }
}

/// What's wrong with a credential pasted as `kind`, in a sentence, when
/// it's plainly the other kind. A test's key, `sk-test…`, is either.
pub fn credential_problem(kind: CredentialKind, key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return Some(match kind {
            CredentialKind::ApiKey => "Paste your API key.".into(),
            CredentialKind::Subscription => "Paste the token claude setup-token gave you.".into(),
        });
    }
    match kind {
        CredentialKind::ApiKey if key.starts_with("sk-ant-oat") => Some(
            "That’s a subscription token, not an API key. Choose your Claude subscription instead, or paste an API key.".into(),
        ),
        CredentialKind::Subscription if key.starts_with("sk-ant-api") => Some(
            "That’s an API key, not a subscription token. Choose An API key instead, or paste the token from claude setup-token.".into(),
        ),
        _ => None,
    }
}

/// The most of one file that goes, in bytes: a skill's file is short.
pub const MOST_FILE: usize = 64 * 1024;

// ---------------------------------------------------------------- the account

/// Your account on kelve.ai, as settings and Your agent show it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostedView {
    /// Whether this kelve has kelve.ai to sign in to at all.
    pub available: bool,
    pub signed_in: bool,
    /// A sign-in is under way: kelve waits for the browser to come back.
    pub waiting: bool,
    /// The page's address, `sam`, once it's chosen.
    pub handle: Option<String>,
    /// The page in full, `https://kelve.ai/@sam`.
    pub page: Option<String>,
    /// Whether the page is published.
    pub public: bool,
    /// The verified runs left today, when kelve.ai could be asked.
    pub runs: Option<RunsLeft>,
    /// A run under way, by its ID, to go back to.
    pub going: Option<String>,
    /// The model providers' keys kept for runs.
    pub keys: Vec<ProviderKey>,
    /// Why kelve.ai couldn't be asked, in a sentence.
    pub problem: Option<String>,
}

/// How many verified runs are left today.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunsLeft {
    pub left: u32,
    pub a_day: u32,
    /// When more come.
    pub more: Timestamp,
}

/// A model provider's key, kept sealed for verified runs. Never the key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderKey {
    /// `anthropic`.
    pub provider: String,
    /// "Anthropic".
    pub name: String,
    pub kind: CredentialKind,
}

/// Whether a page's address can be had, as it's typed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HandleView {
    /// The address as it would be kept.
    pub handle: String,
    pub available: bool,
    /// What it means, in a sentence: "Someone has that one."
    pub why: Option<String>,
    /// Others like it that can be had.
    pub instead: Vec<String>,
}

// ---------------------------------------------------------------- a verified run

/// What a verified run can be, and what of yours would go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OfferView {
    pub kinds: Vec<KindChoice>,
    pub agents: Vec<AgentChoice>,
    /// Every file of your skills, as it would go: secrets and paths taken
    /// out.
    pub skills: Vec<SkillFileView>,
    pub runs: RunsLeft,
    /// What can pay for a run: an API key always; your subscription only
    /// where the service allows it.
    pub accepts: Vec<CredentialKind>,
}

/// A kind of work a run's tasks are set in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KindChoice {
    pub kind: String,
    /// "Rust".
    pub name: String,
    /// What the tasks are like.
    pub about: String,
    pub tasks: u32,
}

/// An agent both runs can be, and the models it runs on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentChoice {
    pub agent: String,
    pub name: String,
    pub models: Vec<ModelChoice>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ModelChoice {
    pub model: String,
    /// "Sonnet 5.6".
    pub name: String,
    /// Whose key pays for it: `anthropic`.
    pub provider: String,
    pub provider_name: String,
}

/// A file of your skills, as it goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillFileView {
    /// `rust/SKILL.md`.
    pub path: String,
    pub bytes: u32,
}

/// What a run will take, before anything runs: kelve.ai has your skills,
/// and waits for you to confirm.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EstimateView {
    pub run: String,
    pub kind_name: String,
    pub agent_name: String,
    pub model_name: String,
    pub tasks: u32,
    pub runs_per_task: u32,
    /// About how long it takes.
    pub minutes: u32,
    pub provider: String,
    pub provider_name: String,
    /// About what it costs on your own key, in US dollars.
    pub dollars: f64,
    /// How many times the agent runs with your skills: what a run uses of
    /// a subscription's plan.
    pub agent_runs: u32,
    /// The runs left today after this one.
    pub runs_left: u32,
    /// What can pay for it.
    pub accepts: Vec<CredentialKind>,
    /// What kelve keeps already for this provider.
    pub kept: Vec<CredentialKind>,
    pub skills: Vec<SkillFileView>,
}

/// How a run stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RunState {
    Estimated,
    Queued,
    Running,
    Finished,
    Cancelled,
    Failed,
}

/// One task's square, as a run goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TaskSquare {
    Waiting,
    Running,
    Better,
    Same,
    Worse,
}

/// A run, as its page shows it: how it stands, task by task, and its score
/// once it's finished.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RunView {
    pub run: String,
    pub state: RunState,
    pub kind_name: String,
    pub agent_name: String,
    pub model_name: String,
    pub squares: Vec<TaskSquare>,
    /// About how long is left, while it runs.
    pub minutes_left: Option<u32>,
    /// Paused until then: the subscription's plan reached its limit. It
    /// goes on by itself.
    pub paused_until: Option<Timestamp>,
    /// Why it stopped, in a sentence, when kelve.ai stopped it.
    pub why: Option<String>,
    pub score: Option<ScoreView>,
}

/// A verified score.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScoreView {
    pub id: String,
    /// The run it came from: its page.
    pub run: String,
    /// Its own public link.
    pub link: String,
    /// Whether it shows on your page.
    pub shown: bool,
    pub kind_name: String,
    pub agent_name: String,
    pub model_name: String,
    /// The version of the task bank it ran on.
    pub bank: String,
    /// A square for each task. Never which task it was.
    pub squares: Vec<ScoreSquare>,
    /// How many came out better: the "10" of "10 of 12".
    pub better: u32,
    pub of: u32,
    /// The mean gain in hidden checks passed, in points, and its 95% interval.
    pub gain: f64,
    pub gain_low: f64,
    pub gain_high: f64,
    /// Whether the difference is clear enough to call verified; otherwise
    /// there's no clear difference.
    pub verified: bool,
    pub at: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_and_paths_never_leave_but_a_projects_paths_do() {
        let text = "Read /Users/sam/acme/notes.md and ~/keys.txt, then src/main.rs.\nUse token ghp_abcdefghijklmnopqrstuvwxyz0123456789 there.\n(C:\\Users\\sam\\x)";
        let sent = for_sending(text);
        assert!(!sent.contains("/Users/sam"), "{sent}");
        assert!(!sent.contains("~/keys"), "{sent}");
        assert!(!sent.contains("C:\\Users"), "{sent}");
        assert!(!sent.contains("ghp_abcdefghij"), "{sent}");
        assert!(
            sent.contains("Read [path] and [path], then src/main.rs."),
            "{sent}"
        );
        assert!(sent.contains("([path])"), "{sent}");
    }

    #[test]
    fn a_credential_of_the_other_kind_is_caught_before_it_goes() {
        use CredentialKind::*;
        assert!(credential_problem(ApiKey, "sk-ant-api03-x").is_none());
        assert!(credential_problem(Subscription, "sk-ant-oat01-x").is_none());
        assert!(credential_problem(ApiKey, "sk-ant-oat01-x").is_some());
        assert!(credential_problem(Subscription, "sk-ant-api03-x").is_some());
        assert!(credential_problem(Subscription, "sk-test-1").is_none());
        assert!(credential_problem(ApiKey, " ").is_some());
    }

    #[test]
    fn plain_text_goes_as_it_is() {
        let text = "# Rust\n\n## How you like it\n\n### Errors are one enum\n\nSee https://doc.rust-lang.org/book/.\n";
        assert_eq!(for_sending(text), text);
    }
}
