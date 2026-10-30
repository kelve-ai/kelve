//! What kelve's local API sends and takes: the reader's windows, settings
//! and keys. Plain data, exported to TypeScript for the web app
//! (`web/src/api/gen`), so the two never disagree.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    /// Whether kelve adds to the shared knowledge graph as you work: new
    /// ideas, and entries for ideas it doesn't have yet. Never anything
    /// you open.
    pub contribute: bool,
    /// Whether kelve sends the shared graph a count of how it was used,
    /// once a week: numbers only. Off until the person turns it on.
    pub share_counts: bool,
    /// What turned counts off where kelve runs, whatever was chosen:
    /// `DO_NOT_TRACK` or `KELVE_TELEMETRY`. None when nothing did.
    pub counts_off: Option<String>,
    /// Where notes kept only for you go, a folder for each project, as the
    /// person would write it: `~/Notes`.
    pub notes_dir: String,
    /// Whether that's a folder the person chose, rather than kelve's own.
    pub notes_chosen: bool,
    /// The coding agent the person uses: named in what kelve says, and in
    /// the line that connects it.
    pub agent: AgentKind,
    /// How large code is drawn, and prose: entries, notes, text files.
    pub code_size: Size,
    pub text_size: Size,
    /// Which parts of code have colours of their own.
    pub highlighting: Highlighting,
    /// Whether what's been read is kept in the project's `.kelve/read/`,
    /// committed with it, or in kelve's own store.
    pub keep_read_in_repo: bool,
    /// Who a new note is for, until the person says otherwise.
    pub new_note: Place,
    /// The hints the person has seen, each shown once: `squares`, `notes`.
    pub hints: Vec<String>,
    /// Whether Claude Code's settings have kelve's hook, which tells it of
    /// notes left for it as the person sends it a message.
    pub notes_hook: bool,
    /// The other agents whose settings have kelve's hook, as people name
    /// them: "Codex".
    pub hooks_elsewhere: Vec<String>,
    /// Whether to ask, under an entry their agent wrote, to add it to the
    /// shared graph: at most once a day, and never after a second no.
    pub ask_to_add: bool,
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
    pub contribute: Option<bool>,
    #[ts(optional)]
    pub share_counts: Option<bool>,
    /// A folder for notes kept only for you; "" for kelve's own.
    #[ts(optional)]
    pub notes_dir: Option<String>,
    #[ts(optional)]
    pub agent: Option<AgentKind>,
    #[ts(optional)]
    pub code_size: Option<Size>,
    #[ts(optional)]
    pub text_size: Option<Size>,
    #[ts(optional)]
    pub highlighting: Option<Highlighting>,
    #[ts(optional)]
    pub keep_read_in_repo: Option<bool>,
    #[ts(optional)]
    pub new_note: Option<Place>,
    /// A hint the person has now seen.
    #[ts(optional)]
    pub hint_seen: Option<String>,
    /// Add kelve's hook to Claude Code's settings, or take it out.
    #[ts(optional)]
    pub notes_hook: Option<bool>,
    /// The person said no to adding what their agent wrote, this time.
    #[ts(optional)]
    pub keep_to_myself: Option<bool>,
}

// ---------------------------------------------------------------- keys

/// Where a key comes from: the install's environment, which wins, or the
/// settings page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum KeyFrom {
    Env,
    App,
}

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

/// Being signed in to the shared graph, which adding to it needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GraphAccount {
    /// Whether this kelve has a shared graph at all.
    pub graph: bool,
    /// Where its key came from, once signed in.
    pub from: Option<KeyFrom>,
    /// The name what this kelve adds is shown with, which the graph made
    /// up, when signed in from the app.
    pub handle: Option<String>,
    /// A sign-in is under way: kelve waits for the browser to come back.
    pub waiting: bool,
    /// Whether the account has agreed to the terms for adding to the graph
    /// as they are now. None when the graph can't be asked.
    pub agreed: Option<bool>,
}

/// What a person agrees to before anything they add goes into the graph,
/// as the graph serves it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Terms {
    pub version: String,
    /// What the screen says, a point at a time.
    pub points: Vec<String>,
    /// The terms in full, as markdown.
    pub text: String,
}

/// Where to send the browser to sign in to the shared graph.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GraphSignIn {
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
    /// A file's marks came in, or more of them did.
    Marks {
        #[ts(type = "number")]
        opened: i64,
        path: String,
    },
    /// A file of what's open was read, or what it's built on chosen: once
    /// a file, not after each passage, as a folder's tree needs.
    Read {
        #[ts(type = "number")]
        opened: i64,
    },
    /// An idea's entry moved on: written, checked, waiting.
    Idea {
        key: String,
    },
    Notes,
    /// What's waiting for the person's agent changed.
    Asks,
    /// An agent called kelve.
    Agent,
    /// Lessons were given to the agent, came of something, or what it
    /// learned moved on.
    Lessons,
    /// A file of what's open was read plainly.
    Plainly {
        #[ts(type = "number")]
        opened: i64,
    },
    /// The page kelve serves, by its build: on every stream, so a tab that
    /// reconnects to a kelve on another build reloads.
    Page {
        build: String,
    },
    /// What kelve marks for the person changed: marks and what's read again.
    Spec,
}

/// A job kept for the person's agent, as the reader lists them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Kept {
    #[ts(type = "number")]
    pub id: i64,
    /// `read`, `choose`, `builtOn`, `relate`, `write` or `fix`.
    pub kind: String,
    /// What it's for: "Writing the entry for Jitter".
    pub about: String,
    /// The idea it's for, by key, when it's for one.
    pub idea: Option<String>,
    /// Whether the agent has it now.
    pub taken: bool,
}

/// Your agent's list (boards Q-1 to Q-9, J-1 to J-3): what it's doing now,
/// what's next for what you have open, what you asked for in other things,
/// what's ready for you, and what's paused.
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
    /// Things opened before what's open, with files still to read: not
    /// read further until they're opened again. The latest first.
    pub paused: Vec<Paused>,
}

/// Something opened before what's open, its reading paused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Paused {
    #[ts(type = "number")]
    pub opened: i64,
    pub kind: OpenKind,
    pub title: String,
    /// How many files are still to read.
    pub left: u32,
    pub opened_at: crate::Timestamp,
    /// When something else was opened after it.
    pub paused_at: crate::Timestamp,
}

/// One thing on your agent's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListItem {
    /// Steady while it's on the list: `read:3`, `plainly:3:src/retry.rs`,
    /// `idea:…`, `note:…`.
    pub key: String,
    /// `reading`, `pages`, `entry` or `note`.
    pub kind: String,
    /// What it's about: a file, an idea's name, what a note says. For a
    /// file read plainly, its path.
    pub title: String,
    /// A line under it: where a note is, what the agent did.
    pub sub: Option<String>,
    /// What was opened it's for.
    #[ts(type = "number | null")]
    pub opened: Option<i64>,
    pub idea: Option<String>,
    /// A note, by its id.
    pub note: Option<String>,
    /// For reading: how many files are still to read, each its reading,
    /// the check of its ideas and its plainly; and each folder's or the
    /// change's own page still to write.
    pub left: Option<u32>,
    /// The title of what it was asked for in, when that isn't what's open.
    pub asked_in: Option<String>,
    /// The jobs it is, to move it up the list.
    #[ts(type = "number[]")]
    pub asks: Vec<i64>,
    /// For the rest's folder pages: how many there are in all, done and
    /// still to do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub of: Option<u32>,
    /// What the agent has in hand of it now, each with how far it is: the
    /// files it's reading, or the folders whose pages it's writing.
    #[serde(default)]
    pub hand: Vec<StepAt>,
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

// ---------------------------------------------------------------- the reader

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
    /// A way into it, when the agent gave one, or the project keeps one.
    #[serde(default)]
    pub way: Option<Way>,
    /// What's ready of what was asked for in it, where a list of what's
    /// opened says so (the start page).
    #[serde(default)]
    pub ready: Option<Vec<ReadyItem>>,
}

/// Something the person asked for by name, ready to read: an idea's entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export)]
pub enum ReadyItem {
    Entry { key: String, name: String },
}

/// A way into what's open: its files in an order to read them, and a line on
/// what each folder is for. The person's agent gives it, when it thinks the
/// person needs one, or kelve asks it for one when there's a lot to read; and
/// the rest of the path, in parts, read when the person asks, when there's a
/// lot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Way {
    pub order: Vec<WayStep>,
    /// The rest of the path after the way in, each part in its order.
    #[serde(default)]
    pub parts: Vec<WayPart>,
    pub folders: Vec<FolderLine>,
    /// The person's agent is finding a way in: none yet.
    #[serde(default)]
    pub finding: bool,
    /// The files on the path the person's agent has started and not
    /// finished, each with how far it is: until each reads plainly.
    #[serde(default)]
    pub reading: Vec<StepAt>,
    /// How many files no part of the path names: no one needs them to
    /// understand what's open. They're in Files.
    #[serde(default)]
    pub left_off: u32,
    /// The steps the person has read, which In order fills in.
    #[serde(default)]
    pub read_steps: Vec<String>,
}

/// A part of the path after the way in: what it is, why it comes now, and
/// its files in the order to read them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WayPart {
    pub name: String,
    pub why: String,
    /// Its files in order; each step's line may be empty.
    pub steps: Vec<WayStep>,
    /// The person asked for it to be read. Only the way in is read without
    /// asking, when there's a lot.
    #[serde(default)]
    pub queued: bool,
}

/// A file the person's agent is working through, and how far it is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StepAt {
    pub path: String,
    pub stage: WorkStage,
}

/// How far the person's agent is with a file: reading it, checking the
/// ideas it marked, then writing it plainly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WorkStage {
    Reading,
    Checking,
    Writing,
}

/// A step on the way in: a file, and why to read it now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WayStep {
    pub path: String,
    pub why: String,
}

/// What a folder is for, in a line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderLine {
    pub path: String,
    pub line: String,
}

/// A folder of what's open, plainly, while its plainly isn't written: its line,
/// and its parts, each with its own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PartView {
    /// From the root; "" for the root itself.
    pub path: String,
    pub line: Option<String>,
    pub parts: Vec<FolderPart>,
}

/// A folder or a file in a folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderPart {
    pub path: String,
    pub folder: bool,
    /// A folder's line, or what a file's for, once it's read plainly.
    pub line: Option<String>,
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
    /// The ideas marked in it, when it's been read.
    pub ideas: Option<Vec<IdeaTag>>,
}

/// An idea, as a tag on what it's marked in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IdeaTag {
    pub key: String,
    pub name: String,
}

/// How far reading what's open has got: its files, then a folder's or a
/// change's own page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Reading {
    /// The files read, of those kelve reads: a file is read once its ideas
    /// are checked and it's read plainly, unless it reads as code. A
    /// folder's or a change's own page read plainly is one more.
    pub read: u32,
    pub of: u32,
    /// Of those not read yet, how many are read, with their ideas still
    /// being checked or the file still being read plainly.
    pub checking: u32,
    /// `done` once there's nothing more it can read now.
    pub marking: Marking,
    /// Of `of`, how many are its own pages, its folders' and a change's,
    /// and how many of those are still to write: the agent's list counts
    /// them on a line of their own.
    #[serde(default)]
    pub pages: u32,
    #[serde(default)]
    pub pages_left: u32,
}

/// An idea that what's open is built on, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BuiltOn {
    pub key: String,
    pub name: String,
    pub why: String,
    /// New to the files kelve has read in this project.
    pub new: bool,
}

/// A folder: its files, and what it's built on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderView {
    pub opened: Opened,
    pub files: Vec<FileItem>,
    /// More files than kelve lists.
    pub more: bool,
    pub built_on: Vec<BuiltOn>,
    pub reading: Reading,
}

/// How far marking a file has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Marking {
    Done,
    /// With the person's agent, until it reads it.
    Marking,
}

/// A mark, with the key of the idea it leads to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Marked {
    pub mark: crate::marks::Mark,
    /// The idea's key: the graph's ID, or `new:` and its name.
    pub key: String,
    /// In a change, whether its idea is new in the change: marked on an
    /// added line, and nowhere the change didn't touch.
    #[serde(default)]
    pub new: bool,
}

/// A file, read, with its marks so far.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileView {
    pub path: String,
    pub lang: Option<String>,
    /// Its text, secrets masked; none when it isn't read.
    pub text: Option<String>,
    pub skip: Option<String>,
    pub marks: Vec<Marked>,
    pub marking: Marking,
}

// ---------------------------------------------------------------- plainly

/// What kelve marks a part with: in a file, from the diff, never from the
/// agent; in a change, as the agent said.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PartMark {
    New,
    Changed,
    Moved,
    TakenOut,
    AsBefore,
}

/// What kind of change something is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ChangeKind {
    Adds,
    Fixes,
    Reshapes,
    TakesOut,
    Updates,
}

/// A `[[term]]` in the agent's words that leads to an idea marked where it
/// says it: the term as written, and the idea's key and name. A term that
/// leads nowhere is shown as plain words, and isn't one of these.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainIdea {
    pub term: String,
    pub key: String,
    pub name: String,
}

/// One part of a file, read plainly: its lines, its mark, its goal, what it
/// does and what for, and, in a change, what it did before. The agent's words
/// may carry `code`, **the key sentence** and `[[term]]`, the terms in `ideas`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainPart {
    pub from: u32,
    pub to: u32,
    pub mark: PartMark,
    /// Two to six words: what it's for.
    pub goal: String,
    pub says: String,
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
    /// In a change, what it did before, when the change touched it.
    pub before: Option<String>,
    /// Set-up: what it brings in, its settings. Also here, in a line.
    #[serde(default)]
    pub also: bool,
}

/// Something a change took out of a file: what it was for, and what it did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainOut {
    pub goal: String,
    pub says: String,
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
}

/// Where a page's plainly is while it isn't written: with the agent now, or
/// how many files are read before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainWaiting {
    /// The agent has it now.
    pub writing: bool,
    /// The agent is reading the file itself now, before writing it plainly.
    #[serde(default)]
    pub reading: bool,
    /// It's part of reading what's open, one row on your agent's list: how
    /// many files the agent reads before it, 0 being next. None while what
    /// it's written from is still being read.
    pub before: Option<u32>,
    /// Its job, to move it up the list: Do next.
    #[ts(type = "number | null")]
    pub ask: Option<i64>,
}

/// A file in a change, as its plainly says it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainChange {
    /// New in the change: adds. Otherwise the change's own kind, once its
    /// plainly is written.
    pub kind: Option<ChangeKind>,
    /// Before → now, in a line, as the agent said it.
    pub says: Option<String>,
    /// Where it was, when the change moved it.
    pub moved_from: Option<String>,
}

/// A code or text file read plainly, or where it is on your agent's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainlyView {
    pub path: String,
    /// Code, rather than text: markdown or plain text.
    pub code: bool,
    /// What it's for, or what it says, in a sentence.
    pub what: Option<String>,
    /// The terms in `what` and `change` that lead to ideas.
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
    /// The files and folders beside it that use it, as kelve tells from
    /// their code: a folder's path ends in `/`.
    #[serde(default)]
    pub used_by: Vec<String>,
    /// When what's open is a change that has this file in it.
    pub change: Option<PlainChange>,
    /// Its parts, in the order things happen; none until the agent has
    /// written them.
    pub parts: Option<Vec<PlainPart>>,
    /// What the change took out.
    #[serde(default)]
    pub out: Vec<PlainOut>,
    /// While it isn't written: where it is.
    pub waiting: Option<PlainWaiting>,
}

/// A file read plainly, as kelve keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainFile {
    pub what: String,
    pub change: Option<String>,
    pub parts: Vec<PlainPart>,
    #[serde(default)]
    pub out: Vec<PlainOut>,
}

/// How a folder's parts are told: as the steps one thing takes, or, with
/// no one path, by what each does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FolderShape {
    Steps,
    Parts,
}

/// One of a folder's parts, a file or a folder in it, read plainly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderPlainPart {
    /// From the root.
    pub part: String,
    pub folder: bool,
    pub goal: String,
    pub says: String,
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
    pub before: Option<String>,
    /// In a change, from its files' lines added and taken out.
    pub mark: Option<PartMark>,
}

/// What most of a folder's parts use, in a line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainAlso {
    pub parts: Vec<String>,
    pub says: String,
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
}

/// Something a folder keeps true, and the part that keeps it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainKeep {
    pub says: String,
    pub by: String,
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
}

/// A folder read plainly, or where it is on your agent's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FolderPlainly {
    /// From the root; "" for the root itself.
    pub path: String,
    /// What it's for; none until the agent has written it.
    pub what: Option<String>,
    /// In a change: what the change does here, before → now.
    pub change: Option<String>,
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
    pub shape: Option<FolderShape>,
    /// At most seven, in the order things happen.
    #[serde(default)]
    pub parts: Vec<FolderPlainPart>,
    pub also: Option<PlainAlso>,
    #[serde(default)]
    pub keeps: Vec<PlainKeep>,
    /// Written for the folder as it was: it's being written again.
    #[serde(default)]
    pub stale: bool,
    /// While it isn't written, or is being written again: where it is.
    pub waiting: Option<PlainWaiting>,
}

/// One part of a change, by purpose, not by file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangePlainPart {
    pub mark: PartMark,
    pub goal: String,
    pub says: String,
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
    pub before: Option<String>,
    /// The files it's in, from the root.
    pub files: Vec<String>,
}

/// A line on what else a change touches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlainLine {
    pub says: String,
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
}

/// A change read plainly, or where it is on your agent's list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangePlainly {
    /// What it changes, and why; none until the agent has written it.
    pub what: Option<String>,
    /// Where it fits in the whole.
    pub fits: Option<String>,
    pub kind: Option<ChangeKind>,
    /// Before → now, in a line.
    pub change: Option<String>,
    #[serde(default)]
    pub ideas: Vec<PlainIdea>,
    /// At most seven, by purpose.
    #[serde(default)]
    pub parts: Vec<ChangePlainPart>,
    /// What tools made, from the root: screenshots, generated files,
    /// lockfiles. Shown in a line, not read.
    #[serde(default)]
    pub tools: Vec<String>,
    /// What else it touches, at most three.
    #[serde(default)]
    pub touches: Vec<PlainLine>,
    /// Its files in the order to read them: as its parts name them, then
    /// the rest; not what tools made.
    #[serde(default)]
    pub order: Vec<String>,
    /// Written for the change as it was: it's being written again.
    #[serde(default)]
    pub stale: bool,
    pub waiting: Option<PlainWaiting>,
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
    /// Tests, which kelve doesn't mark.
    pub test: bool,
    /// The ideas its changed lines rest on, once they're all marked.
    pub ideas: Option<Vec<IdeaTag>>,
    /// Where it was, when the change moved it: what's changed is against
    /// the file it was.
    #[serde(default)]
    pub moved_from: Option<String>,
}

/// A change: its files, and what it's built on, new ideas first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChangeView {
    pub opened: Opened,
    pub title: String,
    pub files: Vec<ChangedFile>,
    pub more: bool,
    pub built_on: Vec<BuiltOn>,
    /// How far marking its changed lines has got: the files kelve marks
    /// as it opens, and how many of them are marked.
    pub reading: Reading,
}

/// One file of a change, whole: every line it has after the change, and
/// those it took out, where they were; and its marks.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiffView {
    pub path: String,
    pub lang: Option<String>,
    pub lines: Vec<DiffLine>,
    pub skip: Option<String>,
    /// On the file after the change, by its line numbers.
    pub marks: Vec<Marked>,
    pub marking: Marking,
}

/// Where an idea's entry has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EntryState {
    Ready,
    Writing,
    Waiting,
    Dropped,
}

/// The steps of writing an entry, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Stage {
    /// Kept for the person's agent, until it takes it.
    Kept,
    /// The agent has it.
    Writing,
    /// The shared graph is checking every claim against the page it cites.
    Checking,
    /// The agent is fixing what the graph's checks found.
    Fixing,
    /// The person took it off their agent's list: it waits until they
    /// keep it for the agent again.
    Off,
}

/// An idea, and its entry or how far it's got.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IdeaPage {
    pub key: String,
    pub name: String,
    pub definition: String,
    /// The one question it answers, from its entry or its relations, once
    /// either is known: the line under its name.
    pub question: String,
    pub entry: Option<crate::Entry>,
    pub state: EntryState,
    /// What it's doing now, or why it waits or was dropped.
    pub step: String,
    /// Which step of writing it's on, while it's written.
    pub stage: Option<Stage>,
    /// What the shared graph's checks found wrong with the entry since it
    /// was accepted, in sentences.
    pub problems: Vec<String>,
    /// Whether kelve is writing one to replace it, shown until then.
    pub rewriting: bool,
    /// Whether the person can tell the shared graph it's wrong, for its
    /// models to check again, or that it didn't help them understand: it's
    /// an entry the graph has.
    pub can_flag: bool,
    /// While it's written: the sites of the pages it's written from, as
    /// they're found.
    pub pages: Vec<String>,
    /// When the shared graph found it to be two ideas: the two, to open
    /// instead. Nothing more is shown of it then.
    pub split: Vec<crate::ideas::IdeaRef>,
    /// The agent kelve keeps its work for, as it names itself, when one has
    /// connected.
    pub agent: Option<String>,
    /// How the shared graph's check of an entry the person's agent wrote
    /// stands, while it's checked, or when it was turned away.
    pub checking: Option<Checking>,
    /// Written by the person's agent and kept here, not in the shared
    /// graph: they haven't agreed to add to it yet.
    pub kept_here: bool,
    /// What people's agents learned using it, the best first, a few, and how
    /// many there are in all.
    pub lessons: Vec<LessonView>,
    pub lessons_all: u32,
    /// Whether its entry came from the shared graph, rather than being
    /// written by the person's agent.
    pub from_graph: bool,
    /// How many times the shared graph's entry has been read, by anyone,
    /// as the graph last said: 0 when it hasn't said.
    pub reads: u32,
}

/// How the shared graph's check of an entry stands, claim by claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Checking {
    /// `pending`, `accepted` or `rejected`.
    pub status: String,
    pub claims: Vec<ClaimCheck>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClaimCheck {
    pub n: u32,
    /// What the claim says.
    pub text: String,
    /// `waiting`, `held` or `failed`.
    pub state: String,
    /// For one that failed: what its pages don't show.
    pub found: String,
    /// Where the entry makes it, when its writer said: the block's id, and
    /// its words there, without markup. Empty when it isn't placed.
    pub block: String,
    pub words: String,
}

/// What to choose with the system's picker, to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PickWhat {
    Folder,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PickIn {
    pub what: PickWhat,
}

/// What the person chose: none when they closed the picker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Picked {
    pub path: Option<String>,
}

/// The person's word that an entry, or a link in it, is wrong.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FlagIn {
    pub why: String,
    /// A link's term, when it's the link that's wrong.
    #[serde(default)]
    #[ts(optional)]
    pub link: Option<String>,
    /// A relation from the idea, when it's how the idea relates that's
    /// wrong: its kind, and the idea at its other end.
    #[serde(default)]
    #[ts(optional)]
    pub relation: Option<FlagRelation>,
}

/// The person's word that an entry didn't help them understand, and what
/// it needs: an example, to be easier, or something it's missing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AskIn {
    pub kind: crate::entry::revise::AskKind,
}

/// A relation a reader says is wrong.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FlagRelation {
    pub kind: crate::relations::RelationKind,
    /// The key of the idea at its other end.
    pub to: String,
}

/// Who a note is for, which says where it's kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Place {
    /// Just the person: in their own notes folder, outside the project.
    #[default]
    You,
    /// Everyone on the work: in the project, `.kelve/notes/`, committed
    /// with it.
    Work,
    /// The person's agent, to pick up: in their own notes folder, until
    /// the agent has done it.
    Agent,
}

/// A note, by where it's kept.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NoteRef {
    /// Its place and file name: `work/ledger.md`.
    pub id: String,
    pub title: String,
    pub place: Place,
    /// Where it is, as the person would write it.
    pub file: String,
    /// The ideas it's about, by key.
    pub ideas: Vec<String>,
    /// The lines it's on, if it's on a passage.
    pub on: Option<NoteOn>,
    /// For the person's agent: whether it's waiting for it, or done.
    pub state: Option<NoteState>,
    /// What the agent did, in its own line, once it's done.
    pub did: String,
    /// When the agent said it was done.
    pub done_at: Option<crate::Timestamp>,
    /// What the agent opened when it was done.
    pub opened: Option<i64>,
}

/// Where the entries for what's open came from: how many ideas it has,
/// how many entries the shared graph had, and how many the person's agent
/// wrote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Origins {
    pub ideas: u32,
    pub from_graph: u32,
    pub by_agent: u32,
}

/// What the person's account added to the shared graph, and how many
/// times others read it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Added {
    pub entries: u32,
    pub relations: u32,
    pub reads: u32,
}

/// The entries sent to the shared graph today, since its day began at
/// midnight UTC, by how the graph's check of each stands; and, once the
/// account has added as much as the graph takes a day, until when the
/// rest waits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SentToday {
    pub sent: u32,
    pub accepted: u32,
    pub checking: u32,
    pub turned_away: u32,
    pub held: Option<SendsHeld>,
}

/// Nothing more goes to the shared graph until `until`, when its day's
/// limits reset: the account added as much as it may today.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SendsHeld {
    pub until: crate::Timestamp,
    /// What ran out, when the graph said.
    pub what: Option<Quota>,
    /// A new account's limit, which rises once ten of its entries are
    /// accepted, or after a week.
    pub new_account: bool,
}

/// What an account adds to the shared graph a day, each with its own
/// limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Quota {
    Ideas,
    Entries,
    Relations,
}

/// Where a note for the person's agent has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum NoteState {
    /// Waiting for the person's next message to their agent.
    Waiting,
    Done,
}

/// The lines a note is on, in a file of what's open: where its passage is
/// now, or where it was, when its text has changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NoteOn {
    pub path: String,
    pub start: u32,
    pub end: u32,
    /// Whether its passage is still there, unchanged.
    pub found: bool,
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

/// Lines of a file, from 1, to write a note on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LinesIn {
    pub path: String,
    pub start: u32,
    pub end: u32,
}

/// A note, in markdown: its body, without the front matter kelve keeps.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Note {
    #[serde(flatten)]
    #[ts(flatten)]
    pub note: NoteRef,
    pub text: String,
}

/// A note to keep: a new one by its place, or one there is by its id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NoteIn {
    pub id: Option<String>,
    pub place: Place,
    pub text: String,
    /// For a new note: the lines it's on.
    #[serde(default)]
    #[ts(optional)]
    pub on: Option<LinesIn>,
    /// For a new note: the ideas it's about, by key.
    #[serde(default)]
    #[ts(optional)]
    pub ideas: Option<Vec<String>>,
}

/// What a search found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Found {
    pub ideas: Vec<FoundIdea>,
    pub files: Vec<String>,
    /// Lines in what's open that say the words, as kelve reads them.
    pub lines: Vec<FoundLine>,
    pub notes: Vec<NoteRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FoundLine {
    pub path: String,
    /// From 1.
    pub line: u32,
    /// The line, its secrets masked, cut to the part around the words.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FoundIdea {
    pub key: String,
    pub name: String,
    pub definition: String,
    /// Whether its entry is ready to read here.
    pub ready: bool,
    /// For an idea in what's open: how many of its files it's in.
    pub files: Option<u32>,
}

/// An idea as the Knowledge window's outline draws it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutlineIdea {
    pub key: String,
    pub name: String,
    /// A first principle, on the ground floor.
    pub foundation: bool,
    /// Traced down: a first principle, or how it relates is known. One
    /// that isn't is drawn dashed.
    pub traced: bool,
    /// On the open path, drawn in ink.
    pub on: bool,
    /// Being traced down now.
    #[serde(default)]
    pub busy: bool,
    /// Why it relates to the idea it hangs from, shown when pointed at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub why: Option<String>,
    /// In how many of the open folder's files it's marked, for a folder's
    /// ideas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub files: Option<u32>,
}

/// One level of an outline, going down: what the open idea above rests on,
/// most needed first. One is open.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OutlineRow {
    pub items: Vec<OutlineIdea>,
}

/// An idea as the Knowledge window draws it: beside it, what it's a kind
/// of, what it answers and what else does the job; below, what it rests on,
/// a row a level, the open one followed down; above, what rests on it; and
/// every first principle it comes down to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Outline {
    pub key: String,
    pub name: String,
    /// The one question it answers, when it's known.
    pub question: String,
    pub foundation: bool,
    pub kind_of: Vec<OutlineIdea>,
    pub answers: Vec<OutlineIdea>,
    pub alternatives: Vec<OutlineIdea>,
    /// Often used beside it, though it doesn't rest on them.
    pub used_with: Vec<OutlineIdea>,
    pub rows: Vec<OutlineRow>,
    /// What rests on it, most read first, and how many more.
    pub above: Vec<OutlineIdea>,
    pub above_more: u32,
    pub grounds: Vec<OutlineIdea>,
    /// Proposed here, and not yet reviewed by the shared graph.
    pub proposed: bool,
    /// Being proposed now: nothing's known yet.
    pub proposing: bool,
}

/// What kelve marks for the person (`spec::Spec`), as settings and the
/// reader show it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MarksView {
    pub spec: crate::spec::Spec,
    /// Whether it's been written yet: until it is, kelve marks every idea
    /// but the basics.
    pub kept: bool,
    /// Where it's kept, as the person would write it.
    pub file: String,
}

/// A lesson, as the reader shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LessonView {
    pub id: String,
    pub says: String,
    #[serde(rename = "do")]
    pub act: String,
    pub example: Option<String>,
    pub holds_when: String,
    pub backed_by: Option<crate::lesson::Backing>,
    pub about: Vec<crate::ideas::IdeaRef>,
    pub standing: crate::lesson::Standing,
    pub record: crate::lesson::Record,
    /// How far it's confirmed, as kelve says it.
    pub basis: String,
    /// Each time the person's agent was given it in this work, newest
    /// first, with what came of it.
    pub used: Vec<LessonUse>,
    /// The usual way it replaces, when its `Seen as` is one rather than an
    /// error: "Instead of `x`".
    #[serde(default)]
    pub instead: Option<String>,
    /// The lessons it contradicts: another good way, each with when it
    /// holds and how it's held.
    #[serde(default)]
    pub another: Vec<LessonBrief>,
}

/// Another lesson, in a line: what to do, when it holds and how it's held.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LessonBrief {
    pub id: String,
    #[serde(rename = "do")]
    pub act: String,
    pub holds_when: String,
    pub basis: String,
}

/// One time the person's agent was given a lesson, and what came of it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LessonUse {
    pub outcome: crate::lesson::Outcome,
    pub shown: crate::lesson::Shown,
    /// What showed it, as a line: "held: the error didn't come back, and
    /// the next run passed".
    pub line: String,
    /// The file it was given on, in the folder, or the fix went into.
    pub path: String,
    /// When: `read` as it read or edited the file, `wrote` as it wrote the
    /// usual way the lesson replaces, `failed` as something failed, `asked`
    /// as it asked.
    pub moment: String,
    pub at: crate::Timestamp,
}

/// The lessons about what's open: those the agent used in it first, then
/// the rest about the ideas marked in it, best first, cut short.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LessonsView {
    pub used: Vec<LessonView>,
    /// About the ideas on the file being read, or, for a change, on its files.
    pub here: Vec<LessonView>,
    /// How many more there are about those ideas, beyond those listed.
    pub more: u32,
    /// What the agent learned in this work, waiting to leave or gone.
    pub learned: Vec<LearnedView>,
    /// The agent, as it names itself, when one has connected.
    pub agent: Option<String>,
}

/// Where something the agent learned is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum LearnedState {
    /// Held here until it leaves, so the person can stop it.
    Waiting,
    /// The person stopped it.
    Stopped,
    /// Sent; the shared graph's checks haven't finished.
    Checking,
    /// In the shared graph, not confirmed yet.
    Live,
    /// The graph holds it back until another project finds the same.
    Held,
    /// The graph had it already, and counted this as confirming it.
    Same,
    /// The graph's checks turned it away.
    TurnedAway,
}

/// Something the person's agent learned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LearnedView {
    pub id: i64,
    pub says: String,
    #[serde(rename = "do")]
    pub act: String,
    pub holds_when: String,
    pub about: Vec<crate::ideas::IdeaRef>,
    pub state: LearnedState,
    pub leaves_at: crate::Timestamp,
    /// Why the graph turned it away, or why it waits here.
    pub why: Option<String>,
    /// The lesson in the graph it is, or is the same as.
    pub lesson: Option<String>,
    #[serde(default)]
    pub example: Option<String>,
    /// The usual way it replaces, when its `Seen as` is one.
    #[serde(default)]
    pub instead: Option<String>,
    /// From replaced work: the file, and the first way and the second,
    /// which never leave.
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub first: Option<String>,
    #[serde(default)]
    pub second: Option<String>,
}

/// Lessons, as settings show them: what the agent learned, and what it
/// was given, newest first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LessonsSettings {
    pub learned: Vec<LearnedView>,
    pub given: Vec<LessonView>,
    /// Whether what the agent learns leaves at all: adding to the shared
    /// graph is on.
    pub contribute: bool,
    /// How long what the agent learns waits before it leaves, in seconds.
    #[ts(type = "number")]
    pub hold: i64,
}
