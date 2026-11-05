//! kelve's hosted service, version 2 (docs/skills.md, "The hosted service
//! and pages"): a person's account, their page's address, their agent's
//! character, verified runs of their agent against a standard one, and the
//! scores those runs give, which they choose to publish or not.
//!
//! Nothing of the person's work goes here. A verified run takes their
//! skills, with secrets and paths stripped by kelve first, for that run
//! only; the service deletes them as the run ends. A page shows the
//! character, its name and the published scores, never skills or work.
//!
//! The shapes keep to version 1's habits: the version in every URL, RFC
//! 9457 problem details for errors, sentences people can read in every
//! refusal, and the account's key as a bearer token.

pub mod character;
pub mod client;
pub mod handle;
#[cfg(feature = "openapi")]
pub mod openapi;
pub mod sample;
#[cfg(feature = "stand-in")]
pub mod stand_in;
pub mod tar;

use kelve_core::time::Timestamp;
use serde::{Deserialize, Serialize};

pub use character::{Character, Palette};

/// The version of the API this speaks, in every URL.
pub const API: &str = "v2";

/// The person's account on the hosted service.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// Their page's address, `@{handle}`, once they've chosen one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    /// Their page's address in full, once they've chosen one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    /// What their agent is called on their page: "Sam’s agent".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether their page is public. Nothing is, until they publish.
    pub public: bool,
    /// Whether they've given their agent a character. Without one, the
    /// page shows kelve's robot.
    pub character: bool,
    /// The verified runs they may start today.
    pub runs: Allowance,
}

/// How many verified runs an account may start in a day, and how many are
/// left.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allowance {
    pub left: u32,
    pub a_day: u32,
    /// When more come: midnight UTC.
    pub more: Timestamp,
}

/// What a person's kelve sends to sign in, once the service has sent their
/// browser back to it, as version 1 does: [`crate::SignInCode`].
pub use crate::SignInCode;

/// What it gets for the code: a key of its own, and the page's address, if
/// the account has chosen one.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedIn {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
}

impl std::fmt::Debug for SignedIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignedIn")
            .field("handle", &self.handle)
            .finish_non_exhaustive()
    }
}

/// What deleting an account took with it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deleted {
    /// Whether it had a page, which is gone.
    pub page: bool,
    pub scores: u32,
}

/// A page's address to check, or to choose, as typed: in a body, never a
/// path, so a secret pasted by mistake reaches no log.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandleAsk {
    pub handle: String,
}

/// Whether an address can be had.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandleCheck {
    /// The address as it would be kept: trimmed, in lower case.
    pub handle: String,
    pub available: bool,
    /// What to change, when it can't be had, in a sentence: "Someone has
    /// that one."
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// Others like it that can be had, when someone has this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instead: Vec<String>,
}

/// What the person changes about their page: its agent's name, and whether
/// it's public.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageSettings {
    /// None for "{Handle}’s agent".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub public: bool,
}

/// The longest an agent's name may be, in characters.
pub const MOST_NAME: usize = 40;

/// A kind of work tasks are set in: `rust`.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindOffer {
    pub kind: String,
    /// As people read it: "Rust".
    pub name: String,
    /// What the tasks are like: "Command-line tools, libraries, and fixing
    /// bugs in code that already exists."
    pub about: String,
    /// How many tasks a run does.
    pub tasks: u32,
    /// How they're scored, in a sentence.
    pub scored: String,
    /// The version of the task bank runs use now.
    pub bank: String,
}

/// An agent that can be run, and the models it can run on.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentOffer {
    /// `claude-code`.
    pub agent: String,
    /// "Claude Code".
    pub name: String,
    pub models: Vec<ModelOffer>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelOffer {
    /// The model's ID at its provider: `claude-sonnet-5-6`.
    pub model: String,
    /// "Sonnet 5.6".
    pub name: String,
    /// Whose key a run on it needs: `anthropic`.
    pub provider: String,
    /// "Anthropic".
    pub provider_name: String,
}

/// What a verified run can be: the kinds of work, and the agents and
/// models both runs can use. Never the tasks themselves.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub kinds: Vec<KindOffer>,
    pub agents: Vec<AgentOffer>,
}

/// The most a run's skills may be, as a tar, before base64.
pub const MOST_SKILLS: usize = 1024 * 1024;

/// A verified run asked for: the kind of work, the agent and model both
/// runs use, and every skill the person's agent has, as a tar of skill
/// folders (`rust/SKILL.md`, `rust/references/errors.md`), secrets and
/// paths stripped by kelve before it's made.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRequest {
    pub kind: String,
    pub agent: String,
    pub model: String,
    /// The tar, in standard base64.
    #[cfg_attr(feature = "openapi", schema(format = Byte))]
    pub skills: String,
}

// Never the skills.
impl std::fmt::Debug for RunRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunRequest")
            .field("kind", &self.kind)
            .field("agent", &self.agent)
            .field("model", &self.model)
            .field("skills", &self.skills.len())
            .finish()
    }
}

/// A file of the skills that goes, as the service read it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFile {
    pub path: String,
    pub bytes: u64,
}

/// What a run will take, before anything runs. Nothing starts until the
/// person confirms it in kelve, with their own key for the model's
/// provider: the run is paid for on that key. An estimate nobody confirms
/// is dropped, with its skills, once it `expires`.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    /// The run's ID, to confirm it with.
    pub run: String,
    pub kind: String,
    pub agent: String,
    pub model: String,
    pub tasks: u32,
    /// How many times each task is expected to run with the person's
    /// skills: from 3, up to 8 where the difference takes longer to tell.
    pub runs_per_task: u32,
    /// About how long it takes, in minutes.
    pub minutes: u32,
    /// Whose key pays for it: `anthropic`.
    pub provider: String,
    /// "Anthropic".
    pub provider_name: String,
    /// About what it costs on the person's own key, in US dollars, at the
    /// provider's list prices: only the runs with their skills. The
    /// standard agent's runs, and judging, are on kelve's key.
    pub dollars: f64,
    /// The runs the account has left today, this one not counted.
    pub runs_left: u32,
    pub expires: Timestamp,
    /// What goes: every file of the skills, as the service read them.
    pub skills: Vec<SkillFile>,
}

/// What a person sends to start a run they've seen the estimate for: their
/// own key for the model's provider. It's used for that run only: held in
/// memory while the run goes, never kept, never logged, and forgotten as
/// the run ends.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirm {
    pub provider_key: String,
}

// Never the key.
impl std::fmt::Debug for Confirm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Confirm").finish_non_exhaustive()
    }
}

/// How a run stands.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    /// Estimated, waiting for the person to confirm it.
    Estimated,
    /// Confirmed, waiting for room to run.
    Queued,
    Running,
    Finished,
    Cancelled,
    /// It couldn't finish: `why` says.
    Failed,
}

/// How one task in a run stands: its square, as the run goes.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Waiting,
    Running,
    Better,
    Same,
    Worse,
}

/// What a task came out as, with the person's skills, against the
/// standard agent: better when the gain is at least 10 points and clear of
/// chance, worse the same way, and the same otherwise.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Better,
    Same,
    Worse,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskProgress {
    pub state: TaskState,
    /// How many times it's run with the person's skills so far.
    pub runs: u32,
}

/// How a run stands, task by task, and its result once it's finished.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunStatus {
    pub run: String,
    pub state: RunState,
    pub kind: String,
    pub agent: String,
    pub model: String,
    /// One for each task, in the order the result gives them.
    pub tasks: Vec<TaskProgress>,
    /// About how long is left, in minutes, while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minutes_left: Option<u32>,
    /// Why it failed, or was cancelled by the service, in a sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<RunResult>,
    /// The score it made, once it's finished: what's published, if the
    /// person chooses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<String>,
}

/// What a finished run found: a verdict for each task, the headline, the
/// mean gain in hidden checks with its 95% interval, and whether the
/// difference is clear enough to call verified.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    pub kind: String,
    /// "Rust".
    pub kind_name: String,
    pub agent: String,
    /// "Claude Code".
    pub agent_name: String,
    pub model: String,
    /// "Sonnet 5.6".
    pub model_name: String,
    /// The version of the task bank it ran on.
    pub bank: String,
    /// Each task's verdict. Never which task it was.
    pub tasks: Vec<Verdict>,
    /// How many times each task ran with the person's skills, most often.
    pub runs_per_task: u32,
    /// Hidden checks passed with the person's skills, of `possible`, and by
    /// the standard agent over as many runs.
    pub passed: u32,
    pub possible: u32,
    pub standard_passed: u32,
    /// The mean gain in hidden checks passed, in points, with its 95%
    /// interval from a two-level bootstrap.
    pub gain: f64,
    pub gain_low: f64,
    pub gain_high: f64,
    /// Whether the bank-level test passed at p < 0.05. Otherwise the score
    /// says "no clear difference".
    pub verified: bool,
    pub finished: Timestamp,
}

impl RunResult {
    /// How many tasks came out better: the headline's "10" of "10 of 12".
    pub fn better(&self) -> usize {
        self.count(Verdict::Better)
    }

    pub fn count(&self, verdict: Verdict) -> usize {
        self.tasks.iter().filter(|v| **v == verdict).count()
    }
}

/// A score: a finished run's result, with its own public link, published
/// or not as the person chooses.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Score {
    /// Its public ID, in its link: `/s/{id}`.
    pub id: String,
    pub link: String,
    pub run: String,
    pub published: bool,
    pub result: RunResult,
}

/// The account's scores, newest first.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scores {
    pub scores: Vec<Score>,
}

/// The title of the problem a run gets, with a 429, once its account has
/// started as many as it may today: `Retry-After` is the seconds to
/// midnight UTC.
pub const NO_RUNS_LEFT: &str = "No runs left today";
