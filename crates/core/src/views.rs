//! What kelve's local API sends and takes: home, debriefs, skills, your
//! agent, what's open and settings. Plain data, exported to TypeScript for the web app
//! (`web/src/api/gen`), so the two never disagree.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::character::{Character, Look, Move, Part, Slot};
use crate::debrief::{Followed, Origin};
use crate::due::DebriefsWhen;
use crate::skills::{SectionKind, SkillKind, WorkKind};
use crate::time::Timestamp;

// ---------------------------------------------------------------- you

/// Whether kelve can do anything yet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Me {
    /// The agent that last did kelve's work, if one has: kelve shows
    /// getting started until one connects.
    pub agent: Option<AgentSeen>,
    /// Every agent that asked kelve for something in the last 30 days, the
    /// latest first.
    pub agents: Vec<AgentSeen>,
    /// The coding agents installed here, found by their commands.
    pub found: Vec<AgentKind>,
}

/// Which kelve this is, and the newest out, when that's newer: what
/// settings shows, and what `kelve update` and `kelve status` ask.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UpdateView {
    /// This kelve's version: `0.2.1`.
    pub version: String,
    /// The newest release, once kelve has heard of one newer than this.
    pub newer: Option<NewerKelve>,
}

/// A newer kelve, as its release says it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NewerKelve {
    pub version: String,
    /// The first line of its notes, plain: empty when they say nothing.
    pub line: String,
}

// ---------------------------------------------------------------- settings

/// Your settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Settings {
    /// Whether kelve sends its hosted service a count of how it was used,
    /// once a week: numbers only. Off until the person turns it on.
    pub share_counts: bool,
    /// What turned counts off where kelve runs, whatever was chosen:
    /// `DO_NOT_TRACK` or `KELVE_TELEMETRY`. None when nothing did.
    pub counts_off: Option<String>,
    /// The coding agent the person uses: named in what kelve says, and in
    /// the line that connects it.
    pub agent: AgentKind,
    /// How large code is drawn, and prose: debriefs and skills.
    pub code_size: Size,
    pub text_size: Size,
    /// Which parts of code have colours of their own.
    pub highlighting: Highlighting,
    /// The hints the person has seen, each shown once: `hook`, `met`.
    pub hints: Vec<String>,
    /// Whether the agent's settings have kelve's hook, which gives it the
    /// person's skills as it works.
    pub hook: bool,
    /// The other agents whose settings have kelve's hook, as people name
    /// them: "Codex".
    pub hooks_elsewhere: Vec<String>,
}

/// How large text is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

/// Which parts of code have colours of their own: strings, constants,
/// comments and definitions, or nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Highlighting {
    /// Everything an IDE colours: keywords, types, properties and calls too.
    #[default]
    Full,
    /// Only what you scan for: strings, constants, comments and what's defined.
    Quiet,
    /// Code in one colour.
    None,
}

/// A coding agent the person uses with kelve.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AgentKind {
    #[default]
    ClaudeCode,
    Codex,
    Cursor,
    Gemini,
    /// Any other agent that speaks MCP.
    Other,
}

/// A change to your settings: only what's given changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct SettingsIn {
    #[ts(optional)]
    pub share_counts: Option<bool>,
    #[ts(optional)]
    pub agent: Option<AgentKind>,
    #[ts(optional)]
    pub code_size: Option<Size>,
    #[ts(optional)]
    pub text_size: Option<Size>,
    #[ts(optional)]
    pub highlighting: Option<Highlighting>,
    /// A hint the person has now seen.
    #[ts(optional)]
    pub hint_seen: Option<String>,
    /// Add kelve's hook to Claude Code's settings, or take it out.
    #[ts(optional)]
    pub hook: Option<bool>,
}

// ---------------------------------------------------------------- counts

/// The weekly count of how kelve was used: this week's so far, exactly as
/// it would go, and the ones sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CountsView {
    /// This week's count so far, as the JSON that would be sent.
    pub this_week: String,
    /// The counts sent, newest first.
    pub sent: Vec<CountSent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CountSent {
    /// The Monday the week began.
    pub week: String,
    /// Exactly what was sent.
    pub report: String,
}

/// Where to send the browser to sign in to kelve's hosted service.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HostedSignIn {
    pub address: String,
}

// ---------------------------------------------------------------- events

/// Something changed: fetch it again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum ServerEvent {
    Settings,
    /// Something was opened.
    Opened,
    /// What's waiting for the person's agent changed.
    Asks,
    /// An agent called kelve.
    Agent,
    /// The page kelve serves, by its build: on every stream, so a tab that
    /// reconnects to a kelve on another build reloads.
    Page {
        build: String,
    },
    /// The ideas the person knows (`marks.md`) changed.
    Spec,
    /// The person's skills, their checks, or Fits you changed.
    Skills,
    /// A debrief moved on: written, answered, researched, explained or sent;
    /// or a handover came in. Home and the debrief fetch again.
    Debriefs,
}

/// A job kept for the person's agent, as the reader lists them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Kept {
    #[ts(type = "number")]
    pub id: i64,
    /// `redo`, `debrief`, `learn`, `research`, `explain`, `compare` or `draw`.
    pub kind: String,
    /// What it's for: "Debriefing kelve since main".
    pub about: String,
    /// The idea it's for, by key, when it's for one.
    pub idea: Option<String>,
    /// Whether the agent has it now.
    pub taken: bool,
}

/// Your agent's list: what it's doing now, what's next for what you have
/// open, what you asked for in other things, and what's ready for you.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentList {
    /// Whether the agent is working: it called kelve in the last two
    /// minutes, or holds a job it took in the last ten.
    pub working: bool,
    /// Until when it counts as working, in milliseconds since 1970: the
    /// reader asks again then, to say whether it's stopped.
    #[ts(type = "number | null")]
    pub working_until: Option<i64>,
    /// The agent working, as it names itself, while one is: the list is
    /// every agent's, and the one doing it is named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub by: Option<String>,
    /// What you have open: the thing opened last.
    #[ts(type = "number | null")]
    pub open: Option<i64>,
    pub now: Vec<ListItem>,
    pub next: Vec<ListItem>,
    /// What you asked for in something other than what's open: still done,
    /// after what's open's.
    pub elsewhere: Vec<ListItem>,
    pub ready: Vec<ListItem>,
}

/// One thing on your agent's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListItem {
    /// Steady while it's on the list: `redo:…`.
    pub key: String,
    /// `redo`.
    pub kind: String,
    /// What it's about: the decision to redo.
    pub title: String,
    /// A line under it: where the decision is, what the agent did.
    pub sub: Option<String>,
    /// What was opened it's for.
    #[ts(type = "number | null")]
    pub opened: Option<i64>,
    /// The title of what it was asked for in, when that isn't what's open.
    pub asked_in: Option<String>,
    /// The jobs it is, to move it up the list.
    #[ts(type = "number[]")]
    pub asks: Vec<i64>,
}

/// The agent that last did kelve's work, as settings show it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentSeen {
    /// As it names itself: `claude-code`.
    pub name: String,
    pub model: Option<String>,
    pub plan: Option<String>,
    pub last_seen_at: crate::Timestamp,
}

// ---------------------------------------------------------------- what's open

/// What kind of thing is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum OpenKind {
    Folder,
    File,
    Change,
}

/// Something opened in kelve: a folder, a file, or a change in a repo.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Opened {
    #[ts(type = "number")]
    pub id: i64,
    pub kind: OpenKind,
    /// The folder it's in, as the person would write it: `~/projects/kelve`.
    pub root: String,
    /// For a file, its path in the folder.
    pub path: Option<String>,
    /// For a change, which: `uncommitted`, `against main`, `commit 3f2a9c1`.
    pub change: Option<String>,
    /// What it is, in a few words: "kelve", "ledger.rs", "Uncommitted changes".
    pub title: String,
    pub opened_at: crate::Timestamp,
    /// The agent's own line about the work, when an agent opened it.
    #[serde(default)]
    pub said: String,
    /// The agent that opened it, when one did.
    #[serde(default)]
    pub agent: Option<String>,
    /// When only some files of the folder were opened: which.
    #[serde(default)]
    pub only: Option<Vec<String>>,
    /// For a change, the latest handover kept for it, if there is one.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub handover: Option<i64>,
    /// And its debrief, once there is one: where the app shows it.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub debrief: Option<i64>,
}

/// What to open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OpenIn {
    /// A folder or a file, absolute.
    pub path: String,
    /// To open a change in the repo the path is in: `uncommitted`, a branch
    /// to compare against, or a commit.
    pub change: Option<String>,
}

/// A file in what's open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileItem {
    pub path: String,
    /// Why kelve doesn't read it, in a sentence, when it doesn't.
    pub skip: Option<String>,
}

/// A folder: its files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderView {
    pub opened: Opened,
    pub files: Vec<FileItem>,
    /// More files than kelve lists.
    pub more: bool,
}

/// A file, read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileView {
    pub path: String,
    pub lang: Option<String>,
    /// Its text, secrets masked; none when it isn't read.
    pub text: Option<String>,
    pub skip: Option<String>,
}

/// One line of a change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiffLine {
    /// `added`, `removed` or `context`.
    pub kind: String,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

/// A file a change touched.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangedFile {
    pub path: String,
    /// `added`, `modified` or `deleted`.
    pub status: String,
    pub added: u32,
    pub removed: u32,
    pub skip: Option<String>,
    /// Where it was, when the change moved it: what's changed is against
    /// the file it was.
    #[serde(default)]
    pub moved_from: Option<String>,
}

/// A change: its files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeView {
    pub opened: Opened,
    pub title: String,
    pub files: Vec<ChangedFile>,
    pub more: bool,
}

/// One file of a change, whole: every line it has after the change, and
/// those it took out, where they were.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiffView {
    pub path: String,
    pub lang: Option<String>,
    pub lines: Vec<DiffLine>,
    pub skip: Option<String>,
}

/// A report of something that went wrong, kept for the person and sent
/// nowhere: shown as it is, with an issue it could make.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Problem {
    pub id: String,
    /// What went wrong: `kelve stopped`, or `the page`.
    pub what: String,
    pub at: crate::Timestamp,
    /// The report, exactly as it's kept and as an issue would carry it.
    pub text: String,
    /// A new issue on kelve's repository, filled in with the report.
    pub issue: String,
}

/// An error on kelve's page, to keep a report of.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PageProblem {
    pub message: String,
    pub stack: String,
}

// ---------------------------------------------------------------- skills

/// What a part's checks found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PartCheck {
    /// Not checked yet: it can be.
    #[default]
    NotChecked,
    /// Past tasks are being redone, with it and without.
    Checking,
    /// It changes what the agent does: it stays.
    Works,
    /// The agent does it anyway: it can go.
    NoDifference,
    /// The agent still got it wrong with it: it needs other words.
    Failed,
}

/// Where a part came from, as its margin says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PartSource {
    /// What the person told their agent while it worked.
    Said,
    /// What they changed after it handed over.
    Edit,
    /// A decision they kept, or changed, in a debrief.
    Debrief,
    /// What they wrote in the file themselves.
    ByHand,
}

/// One piece of a part's evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PartEvidence {
    pub source: PartSource,
    /// What showed it, in a line.
    pub text: String,
    pub at: Timestamp,
    /// The debrief it came from, when it came from one.
    pub debrief: Option<String>,
}

/// A skill in the skills page's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillItem {
    /// How the app addresses it: its name for one of the person's own, its
    /// number for a project's.
    pub id: String,
    /// Its name, as its folder and front matter have it: `rust`.
    pub name: String,
    /// Its title, as people read it: "Rust".
    pub title: String,
    pub kind: SkillKind,
    /// The kind of work a "how you like it" skill is for.
    pub work: Option<WorkKind>,
    /// The project a project's skill is kept in, by its folder's name.
    pub project: Option<String>,
    pub parts: u32,
    /// How many of its parts failed their checks.
    pub failed: u32,
    /// Whether its parts are being checked now.
    pub checking: bool,
}

/// The skills page's list, in its four groups.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillsView {
    /// Skills kelve suggests, not made yet.
    pub suggested: Vec<SuggestedItem>,
    /// How you like it.
    pub like: Vec<SkillItem>,
    /// How you do something.
    pub doing: Vec<SkillItem>,
    /// One project.
    pub project: Vec<SkillItem>,
}

/// A suggested skill, in the list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestedItem {
    pub id: String,
    pub title: String,
}

/// One skill, as its page shows it: rendered with its margin, or as its
/// `SKILL.md`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillView {
    /// How the app addresses it, as in the list.
    pub id: String,
    pub name: String,
    pub title: String,
    /// What it's for: the line under its title.
    pub intro: String,
    pub description: String,
    pub kind: SkillKind,
    pub work: Option<WorkKind>,
    pub project: Option<String>,
    /// The `SKILL.md`, as it is.
    pub markdown: String,
    pub sections: Vec<SkillSectionView>,
    /// How many lines it has, and the most it can.
    pub lines: u32,
    pub most_lines: u32,
    /// What's wrong with the file, as sentences that say what to change.
    pub problems: Vec<String>,
    /// What checking its parts not checked yet would take.
    pub offer: CheckOffer,
    /// Its check, while it runs.
    pub running: Option<CheckRun>,
}

/// Checking a skill's new parts, before it starts: which, on how many past
/// tasks, and roughly how many runs of the agent it takes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckOffer {
    /// The parts a check would take now: those not checked yet, as many as
    /// this week's runs allow.
    pub parts: Vec<OfferedPart>,
    /// The past tasks each part is redone on.
    pub tasks: u32,
    /// Runs of the agent it takes: two a part on each task.
    pub runs: u32,
    /// When checks run, as the person set it.
    pub when: ChecksWhen,
    /// Runs of the agent left this week, of the most the person allows.
    pub left: u32,
    /// Parts not checked yet that wait, because checks are off or this
    /// week's runs are used, and why, in a sentence.
    pub waiting: u32,
    pub held: Option<String>,
}

/// A part a check would redo past work with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OfferedPart {
    pub key: String,
    pub heading: String,
}

/// A skill's check, while it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckRun {
    /// The parts being checked.
    pub parts: u32,
    /// Runs of the agent done, of how many.
    pub done: u32,
    pub runs: u32,
}

/// What saving a skill came to: saved, or what's wrong with it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillSaved {
    pub saved: bool,
    /// What's wrong with it, as sentences that say what to change.
    pub problems: Vec<String>,
    /// How the app addresses it now.
    pub id: String,
    /// How many of its parts aren't checked yet.
    pub unchecked: u32,
}

/// What a change to a skill came to, with the file before and after it, to
/// undo it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillChanged {
    /// What happened, in a line: "Removed from Rust."
    pub said: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// A section of a skill, with its parts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillSectionView {
    pub title: String,
    pub kind: Option<SectionKind>,
    pub parts: Vec<SkillPartView>,
}

/// A part of a skill, with what its checks found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillPartView {
    /// Its key: the skill, and its heading normalised.
    pub key: String,
    pub heading: String,
    /// The part as written, heading and all.
    pub markdown: String,
    /// The line its heading is on in the `SKILL.md`, from 1: where its mark
    /// goes in the markdown view.
    pub line: u32,
    pub check: PartCheck,
    /// The person kept it though it makes no difference.
    pub kept: bool,
    /// What the check found, in a line: "Better on 2 of 2 past tasks".
    pub hint: String,
    /// While it's checked: how many past tasks are redone, of how many.
    pub progress: Option<CheckProgress>,
    /// When it failed: what happened, and clearer words from the agent.
    pub failed: Option<PartFailed>,
    /// Where it came from, the latest first.
    pub evidence: Vec<PartEvidence>,
}

/// How far a check has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckProgress {
    pub done: u32,
    pub of: u32,
}

/// What a failed check found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PartFailed {
    /// What happened, in markdown.
    pub what: String,
    /// The part in clearer words, from the agent, in markdown.
    pub wording: String,
}

/// A skill kelve suggests making, drafted in full.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestedSkill {
    pub id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    pub kind: SkillKind,
    /// Why it's suggested: "A new skill from five of your decisions."
    pub why: String,
    pub markdown: String,
    pub sections: Vec<SkillSectionView>,
    /// A skill the person has whose description is close, to add to
    /// instead.
    pub close_to: Option<SkillItem>,
}

// ---------------------------------------------------------------- debriefs

/// Where a debrief is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DebriefState {
    /// Waiting for the agent, busy with something else.
    Waiting,
    /// The agent is writing it.
    Writing,
    /// Ready to read.
    Ready,
    /// It failed its checks twice: it can be asked for again.
    Failed,
    /// Sent to the agent.
    Sent,
}

/// The person's call on a decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DecisionChoice {
    Agree,
    /// Change it: the agent redoes it their way.
    Change,
    /// Look into it: research.
    LookInto,
}

/// What a decision's margin says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DecisionStatus {
    YourCall,
    Agreed,
    Redoing,
    Researching,
    ResearchReady,
}

/// Where a job's answer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AnswerState {
    /// Asked: waiting for the agent, or being written.
    Asked,
    Ready,
    /// It failed its checks twice.
    Failed,
}

/// A debrief, as its page shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DebriefView {
    pub id: String,
    pub title: String,
    pub state: DebriefState,
    /// The example, to try: nothing in it changes the person's skills.
    pub example: bool,
    /// While it's written: what the agent is doing.
    pub steps: Vec<WritingStep>,
    /// How many files the work changed, and lines added and taken out.
    pub files: u32,
    pub added: u32,
    pub removed: u32,
    /// What it did, and how it works, in markdown.
    pub did: String,
    pub how: String,
    pub decisions: Vec<DecisionView>,
    /// How many decisions have no answer yet.
    pub left: u32,
    /// What it adds to the person's skills, and a new skill if one's worth
    /// making.
    pub for_skills: Vec<SkillAddition>,
    pub suggested: Option<SuggestedItem>,
    /// What it used from the person's skills.
    pub used: Vec<UsedPart>,
    /// What the work rests on.
    pub ideas: Vec<IdeaView>,
    /// Every file, by folder.
    pub folders: Vec<DebriefFolder>,
    /// What happens when it's sent, a line each.
    pub on_send: Vec<SendLine>,
    /// When it failed: what was wrong with it.
    pub problems: Vec<String>,
    /// When it was sent.
    pub sent_at: Option<Timestamp>,
}

/// A step the agent takes as it writes a debrief.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WritingStep {
    pub text: String,
    pub done: bool,
    /// Whether it's the step the agent is on.
    pub now: bool,
}

/// A decision in a debrief.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DecisionView {
    /// Its number in the debrief, from 1.
    pub n: u32,
    pub title: String,
    /// Why, in markdown.
    pub why: String,
    pub from: Origin,
    /// On a large change, the part of it the decision belongs to.
    pub group: Option<String>,
    /// Where it is: "src/fetch.rs, lines 59 to 66".
    pub place: String,
    pub could_have: Vec<String>,
    pub rests_on: Vec<String>,
    pub choice: Option<DecisionChoice>,
    pub status: DecisionStatus,
    /// How they want it, when they chose to change it.
    pub note: String,
    /// The question, when they asked their agent to look into it, and its
    /// answer.
    pub question: String,
    pub research: Option<ResearchView>,
    /// Whether it goes into a skill when the debrief is sent, and which.
    pub remember: bool,
    pub skill: Option<SkillTarget>,
    /// Whether the person can say they want it this way every time: they
    /// asked for it another way, their agent's research is in, or it was
    /// theirs to begin with and they agreed.
    pub can_remember: bool,
    /// Its own lines of code, folded away until opened.
    pub code: Vec<DebriefLine>,
    /// The file its code is in, and that file's language, for colouring it.
    pub path: Option<String>,
    pub lang: Option<String>,
}

/// Research on a decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResearchView {
    pub state: AnswerState,
    /// The answer, in markdown, once it's ready.
    pub markdown: Option<String>,
    /// What it suggests, in a sentence: what "Redo it this way" asks for.
    pub suggests: Option<String>,
}

/// The skill something goes into.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillTarget {
    pub name: String,
    pub title: String,
    /// Whether it's a new skill kelve suggests.
    pub new: bool,
}

/// One line of a decision's code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DebriefLine {
    pub kind: DebriefLineKind,
    /// Its number in the file as it is now, or as it was for a line taken
    /// out; none for a fold.
    pub n: Option<u32>,
    pub text: String,
    /// For a fold: how many lines it hides.
    pub folded: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DebriefLineKind {
    Added,
    Removed,
    /// What the person changed after the agent handed over.
    Yours,
    Context,
    /// Lines left out, as before.
    Fold,
}

/// A part the debrief adds to a skill.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SkillAddition {
    pub skill: SkillTarget,
    pub heading: String,
    pub markdown: String,
    /// The decisions it comes from, by number.
    pub decisions: Vec<u32>,
    /// The heading of a part it disagrees with, when it does: the person
    /// chooses.
    pub disagrees: Option<String>,
}

/// A part of the person's skills the work used.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct UsedPart {
    pub skill: SkillTarget,
    pub heading: String,
    pub followed: Followed,
}

/// An idea the work rests on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IdeaView {
    pub name: String,
    /// What it is, in a line.
    pub what: String,
    /// Whether it's new to the person, by `marks.md`.
    pub new_to_you: bool,
    /// Its explanation, once asked for.
    pub explanation: Option<ExplanationView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ExplanationView {
    pub state: AnswerState,
    pub markdown: Option<String>,
}

/// A folder of the work's files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DebriefFolder {
    /// Its path, or "the top folder".
    pub name: String,
    /// How many files it has in the change.
    pub count: u32,
    /// Whether it starts open: past about 20 files, folders start closed.
    pub open: bool,
    pub files: Vec<DebriefFile>,
}

/// A file of the work.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DebriefFile {
    pub path: String,
    pub added: u32,
    pub removed: u32,
    /// Whether the person changed it after the agent handed over.
    pub yours: bool,
    /// The decisions in it, by number.
    pub decisions: Vec<u32>,
    /// Whether kelve reads it as text, and shows it.
    pub shown: bool,
}

/// A line of what happens when a debrief is sent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SendLine {
    pub kind: SendKind,
    pub text: String,
    /// What it leads to, when it leads somewhere.
    pub action: Option<String>,
    pub to: Option<String>,
}

/// What a line of what happens when a debrief is sent is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum SendKind {
    /// The agent redoes decisions.
    Redo,
    /// Research still running.
    Research,
    /// Into a skill the person has.
    Skill,
    /// A new skill.
    NewSkill,
    /// Decisions left open, or nothing to do.
    Open,
}

/// A debrief's change, file by file: the change view's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DebriefChangeView {
    pub id: String,
    pub title: String,
    pub files: u32,
    pub added: u32,
    pub removed: u32,
    pub folders: Vec<DebriefFolder>,
}

/// One file of a debrief's change, whole, with each decision beside its
/// lines.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DebriefDiffView {
    pub path: String,
    /// Its language, for colouring it.
    pub lang: Option<String>,
    /// `new`, `changed`, `deleted` or `your edit`.
    pub kind: String,
    /// Whether kelve shows it: not when it's a secret, a lockfile or not text.
    pub shown: bool,
    /// Its lines, the ones taken out where they were. A fold stands for
    /// lines kelve didn't keep.
    pub lines: Vec<DebriefLine>,
    /// The decisions in it, each beside its first line.
    pub notes: Vec<DiffNote>,
}

/// A decision, in the change view's margin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiffNote {
    /// The line it's beside, as a place in the file's lines, from 1.
    pub at: u32,
    pub n: u32,
    pub title: String,
    pub status: DecisionStatus,
}

// ---------------------------------------------------------------- home

/// Which of home's states it's in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HomeState {
    /// Things for the person, or nothing.
    Normal,
    /// The agent is at work.
    Working,
    /// Nothing yet: a new person.
    Empty,
    /// Something needs fixing.
    Attention,
}

/// Home: the agent's bubble, what it asks, and what's happened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HomeView {
    pub state: HomeState,
    /// The agent's name: the one given, or "Sam's agent".
    pub name: String,
    /// The agent's status, in a few words: "Waiting for you".
    pub status: String,
    /// When it started working, for a clock.
    pub working_since: Option<Timestamp>,
    /// What the agent says, in its own words.
    pub say: String,
    /// How it moves.
    pub moving: Move,
    pub asks: Vec<HomeAsk>,
    pub days: Vec<HomeDay>,
}

/// What a request in the agent's bubble is about, for its icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HomeAskKind {
    Debrief,
    Check,
    /// Fits you has two results to pick between.
    Fits,
    /// kelve's hook is gone from an agent's settings.
    Hook,
    /// The agent stopped before it finished.
    Stalled,
}

/// Something the agent asks of the person, in its bubble.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HomeAsk {
    pub id: String,
    pub kind: HomeAskKind,
    /// In the agent's words: "I finished Retry failed downloads. I made 4
    /// decisions you might want a say in."
    pub text: String,
    /// Its button, and where it leads, when it has one.
    pub action: Option<String>,
    pub to: Option<String>,
    /// Whether it's the main thing to do.
    pub main: bool,
    /// A second, quieter choice: "Not now".
    pub alt: Option<String>,
}

/// A day on home's timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HomeDay {
    /// "Today", "Yesterday", or the date.
    pub label: String,
    pub items: Vec<TimelineItem>,
}

/// What a line on the timeline is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TimelineKind {
    Debrief,
    Check,
    /// A skill was added to, or started.
    Skill,
    /// A verified score.
    Score,
    /// The agent drew a part or a character.
    Part,
    /// A piece of work done, debriefed.
    Done,
    /// A small change, with no debrief.
    Small,
    /// A debrief the agent couldn't write.
    Failed,
}

/// What a line on the timeline can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum TimelineAct {
    Undo,
    DebriefIt,
    Open,
    TryItOn,
    SeeIt,
}

/// A line on home's timeline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TimelineItem {
    pub id: String,
    pub kind: TimelineKind,
    pub title: String,
    pub at: Timestamp,
    pub act: Option<TimelineAct>,
    /// Where it leads.
    pub to: Option<String>,
    /// For a score: a square for each task.
    pub squares: Vec<ScoreSquare>,
}

/// How the person's agent did on one task, against a standard one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ScoreSquare {
    Better,
    Same,
    Worse,
}

// ---------------------------------------------------------------- your agent

/// The person's agent as a character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CharacterView {
    /// Its name: the one given, or "Sam's agent".
    pub name: String,
    /// Whether the person gave it that name.
    pub named: bool,
    pub look: Look,
    /// It drawn, as an SVG whose layers the app's CSS moves.
    pub svg: String,
    /// The parts and characters the agent drew for the person.
    pub made: Vec<MadeView>,
}

/// A part or a character the agent drew.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MadeView {
    /// What it's known by, in a look.
    pub key: String,
    pub name: String,
    /// The slot a part goes in; none for a whole character.
    pub slot: Option<Slot>,
    /// It drawn on its own, as an SVG.
    pub svg: String,
    pub at: Timestamp,
    /// What it is, so the app can try it on: a part, or a whole character.
    pub part: Option<Part>,
    pub character: Option<Character>,
}

/// A change to the person's agent: only what's given changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct CharacterIn {
    /// Its name; empty for the one it has until it's given one.
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub look: Option<Look>,
}

/// Asking the person's agent to draw a part or a character.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DrawIn {
    /// What to draw, in the person's words: "a party hat".
    pub asked: String,
}

/// Where a drawing the person asked for is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum DrawingState {
    /// Waiting for the agent to take it.
    Waiting,
    /// The agent is drawing it.
    Drawing,
    /// Drawn, checked and kept: it's in what the agent made.
    Done,
    /// What the agent drew didn't fit, twice.
    Failed,
}

/// A drawing the person asked their agent for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DrawingView {
    pub id: String,
    pub asked: String,
    pub state: DrawingState,
    /// What it made, by its key, once it's done.
    pub made: Option<String>,
    /// Why it failed, in the agent's last try.
    pub why: Option<String>,
    pub at: Timestamp,
}

/// When kelve runs skill checks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ChecksWhen {
    /// kelve asks on home before each one: checks run only when the person
    /// agrees (docs/skills.md).
    #[default]
    Ask,
    /// When the person's agent has nothing else to do: new and changed parts
    /// are checked without asking, within the week's runs.
    WhenFree,
    /// Never: parts stay not checked.
    Never,
}

/// An agent whose settings hold kelve's hook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ConnectedAgent {
    /// As kelve names it: `claude`.
    pub agent: String,
    /// As people call it: "Claude Code".
    pub name: String,
}

/// Settings for the person's agent, and for skill checks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AgentSettings {
    pub connected: Vec<ConnectedAgent>,
    /// How often a debrief comes.
    pub debriefs: DebriefsWhen,
    /// Whether the person's skills go in each agent's own skills folder too.
    pub skills_folder: bool,
    pub checks: ChecksWhen,
    /// How many check runs a week, at most.
    pub checks_a_week: u32,
    /// How many ran this week so far.
    pub checks_this_week: u32,
}

/// A change to those settings: only what's given changes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AgentSettingsIn {
    #[ts(optional)]
    pub debriefs: Option<DebriefsWhen>,
    #[ts(optional)]
    pub skills_folder: Option<bool>,
    #[ts(optional)]
    pub checks: Option<ChecksWhen>,
    #[ts(optional)]
    pub checks_a_week: Option<u32>,
    /// Take kelve's hook out of this agent's settings, by kelve's name for it.
    #[ts(optional)]
    pub disconnect: Option<String>,
}

/// Your agent: its character, what it knows, and how it scores.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct YourAgentView {
    pub character: CharacterView,
    /// A row for each skill: a square for each part, by what its check found.
    pub knows: Vec<KnowsRow>,
    /// How often the person picked their agent, blind, on their past work.
    pub fits: Option<FitsYou>,
    /// Fits you's results ready to pick between.
    pub fits_ready: u32,
    /// Whether there's past work Fits you can redo, or is redoing.
    pub fits_open: bool,
    /// Verified scores, and whether each is on their page.
    pub scores: Vec<ScoreRow>,
    /// Their page, once it's published.
    pub page: Option<String>,
}

/// A skill, in what the agent knows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KnowsRow {
    pub name: String,
    pub title: String,
    pub kind: SkillKind,
    pub parts: Vec<PartCheck>,
}

/// Fits you: blind picks between the person's agent and a standard one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FitsYou {
    /// How many times they picked their own agent, of how many.
    pub picked: u32,
    pub of: u32,
    /// Each test, oldest first: whether they picked their own.
    pub tests: Vec<bool>,
}

/// Fits you's page: the next pair of results to pick between, blind, and
/// the tally so far.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FitsView {
    /// The next pair ready to pick between.
    pub pick: Option<FitsPick>,
    /// Pairs the agent is still redoing.
    pub waiting: u32,
    /// Past tasks there are to redo.
    pub tasks: u32,
    pub tally: FitsYou,
    /// How close each agent's work came to what the person kept, on
    /// average over the pairs picked, in percent.
    pub closeness: Option<FitsCloseness>,
}

/// Two results of one past task, side by side, which is which hidden.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FitsPick {
    #[ts(type = "number")]
    pub id: i64,
    /// What the person asked, as they asked it.
    pub request: String,
    /// Each side's change, as a diff.
    pub left: String,
    pub right: String,
}

/// A pick, once made: which side was the person's agent, and how close each
/// came to what they kept, in percent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FitsPicked {
    /// `left` or `right`.
    pub mine: String,
    /// `left`, `right` or `same`.
    pub picked: String,
    pub skills_close: Option<u32>,
    pub standard_close: Option<u32>,
}

/// How close each agent came to what the person kept, in percent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FitsCloseness {
    pub skills: u32,
    pub standard: u32,
}

/// A verified score.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScoreRow {
    pub id: String,
    pub work: WorkKind,
    /// The model it ran on, as people name it: "Sonnet 5.6".
    pub model: String,
    pub squares: Vec<ScoreSquare>,
    pub at: Timestamp,
    /// Whether it shows on the person's page.
    pub shown: bool,
    /// Whether a newer model has come out since.
    pub stale: bool,
}
