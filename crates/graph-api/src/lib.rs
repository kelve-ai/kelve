//! The shared knowledge graph's API, version 1 (spec section 10.2): what a
//! kelve sends the graph and gets back. The graph holds ideas and their
//! entries, never anyone's work: a kelve sends the names of ideas to look them
//! up, and new ideas with their entries to add them. It never sends what the
//! person opened, its names or paths, or which ideas their files use.
//!
//! The shapes follow what open knowledge graphs settled on: IDs minted by
//! the graph, never reused, and redirected when ideas merge (Wikidata,
//! OpenAlex); name matching shaped after the W3C Reconciliation API; every
//! change grouped in a changeset that says what made it (OpenStreetMap),
//! with an idempotency key so a retry never applies it twice; RFC 9457
//! problem details for errors; and the version in every URL.

pub mod client;
#[cfg(feature = "openapi")]
pub mod openapi;
#[cfg(feature = "stand-in")]
pub mod stand_in;
pub mod tokens;

use std::collections::BTreeMap;

use kelve_core::Entry;
use kelve_core::entry::Part;
use kelve_core::entry::revise::{AskKind, Op, RevisionKind};
use kelve_core::ideas::IdeaKind;
use kelve_core::relations::{RelationKind, RelationState};
use serde::{Deserialize, Serialize};

/// The version of the API this speaks, in every URL.
pub const API: &str = "v1";

/// An ID in a changeset that stands for an idea the same changeset adds,
/// until the graph gives it its own: `ref:advisory-locks`.
pub const REF: &str = "ref:";

/// What a graph is: its prefix for idea IDs, and what it calls itself.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// The prefix of every idea ID it mints: `kelve`.
    pub space: String,
    pub name: String,
    pub version: String,
    /// The oldest kelve that may use it: an older one asks the person to
    /// update.
    #[serde(rename = "minClient", default, skip_serializing_if = "Option::is_none")]
    pub min_client: Option<String>,
}

/// A name to find ideas for, with a word of where it came up.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    pub query: String,
    /// A word or two of the field it's from: "databases", "React". Never
    /// anything from what the person opened beyond that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// Names to look up, keyed by the asker's own keys.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRequest {
    pub queries: BTreeMap<String, Query>,
}

/// An idea a name may be. The asker's own model chooses among candidates
/// for what it reads; only the graph decides what it writes.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub id: String,
    pub name: String,
    /// What it is, in a line: how a model tells two ideas with one name
    /// apart.
    pub definition: String,
    /// From 0 to 1: how alike the names are.
    pub score: f64,
    /// Whether this is surely the idea: its name or one of its other names
    /// is exactly the one asked for, and no other idea's is.
    #[serde(rename = "match")]
    pub matched: bool,
    #[serde(rename = "sameAs", default, skip_serializing_if = "Option::is_none")]
    pub same_as: Option<String>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Candidates {
    pub result: Vec<Candidate>,
}

/// An idea: what it is, and whether its entry is written yet.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Idea {
    pub id: String,
    pub name: String,
    pub definition: String,
    pub kind: IdeaKind,
    #[serde(rename = "sameAs", default, skip_serializing_if = "Option::is_none")]
    pub same_as: Option<String>,
    /// The current revision of its entry, when it has one that's been
    /// accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// What the graph's checks found wrong with that revision since it was
    /// accepted, in sentences: a reader's report the judge upheld, a claim
    /// its pages no longer support, a page gone. While there are any, a
    /// person's kelve may write the entry again, to replace it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
    /// The blocks of that revision those problems are in, by id, as far as
    /// the graph can tell: what a fix may change.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faulted: Vec<String>,
    /// What its readers, or the graph's own grade, ask its entry to add,
    /// most asked first: only what the graph would take now. A person's
    /// kelve that adds to the graph has its agent extend the entry when
    /// it's opened.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asks: Vec<IdeaAsk>,
    /// Whether it's a first principle, on the ground floor: the open list's
    /// word, until three model families change it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub foundation: bool,
    /// How many steps of served "rests on" it is from a foundation: none
    /// until a chain reaches one. The graph works it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
    /// How many times its entry has been read, with nothing about who.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reads: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Ideas asked for by ID: the ones found, IDs that now redirect to another,
/// IDs of ideas found to be two, and IDs the graph doesn't know.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ideas {
    pub ideas: Vec<Idea>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub redirects: BTreeMap<String, String>,
    /// An idea models found to be two ("normalization" for text, and
    /// for probabilities), by its old ID: the IDs of the ideas it is now.
    /// Which one a use of it means is for whoever used it to say.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub split: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

/// What an idea's entry is asked to add.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdeaAsk {
    pub kind: AskKind,
    /// The part it goes under, when it's known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<Part>,
    /// How many readers asked: none when it's the graph's own grade.
    #[serde(default)]
    pub readers: u32,
}

pub use kelve_core::lesson::{Backing, Lesson, Outcome, Record, Shown, Standing};

/// Where a lesson is in the graph.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LessonState {
    /// The graph's checks haven't finished.
    Checking,
    /// Served, ranked by what came of it.
    Live,
    /// About security, health, law or money: waits, unserved, until another
    /// project finds the same or a page backs it.
    Waiting,
    /// The checks turned it away, with why.
    TurnedAway,
    /// It says what a lesson already there says, and counted as confirming
    /// it: `sameAs` is that one.
    Same,
}

/// A lesson as the graph serves it: `about` holds the ideas' IDs.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonPage {
    /// The graph's ID for it, never reused: `kelve:l…`.
    pub id: String,
    pub lesson: Lesson,
    pub state: LessonState,
    /// What came of it for projects besides its author's.
    pub record: Record,
    pub standing: Standing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_as: Option<String>,
    /// Why it was turned away, when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    /// What it's matched by, made by the graph from its own words only:
    /// cues, codes, the problem as tools print it, and "holds when" as
    /// conditions (`core::matching`). Empty from a graph that makes none;
    /// kelve then makes its own.
    #[serde(default)]
    pub card: kelve_core::matching::Card,
    /// The lessons it contradicts, by ID: another good way, each with when it
    /// holds. Both stay; what held ranks them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contradicts: Vec<String>,
    /// How many times it was given, across every account, as kelves said what
    /// came of it: a kelve holds a lesson back as a control only once it's
    /// widely used.
    #[serde(default)]
    pub given: u32,
}

/// The live lessons about some ideas, best first, and where merged ideas
/// went.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lessons {
    pub lessons: Vec<LessonPage>,
}

/// What came of lessons an agent was given, sent in a batch, as the
/// person's account so that one project counts once. Nothing else: never
/// the problem, the command or the file.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcomes {
    pub outcomes: Vec<LessonOutcome>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LessonOutcome {
    pub lesson: String,
    pub outcome: Outcome,
    pub shown: Shown,
    /// Held back from the agent, to measure the lessons: what came of the
    /// moment without it. Never counted in the lesson's record.
    #[serde(default)]
    pub withheld: bool,
    /// How likely it was given, in thousandths, at the moment it fitted:
    /// what weighs it when lessons are measured together.
    #[serde(default)]
    pub chance: u16,
}

/// The most outcomes one batch may carry.
pub const MOST_OUTCOMES: usize = 200;

/// The most live lessons one account has about one idea.
pub const MOST_LESSONS_AN_IDEA: usize = 3;

/// A reader's word that an entry didn't help them understand, and what it
/// needs. Like a flag, it carries nothing of who sent it, and nothing of their
/// work: the revision they read, and one of three words.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    /// The entry's revision, as the reader saw it.
    pub revision: String,
    /// `example`, `tooHard` or `missing`.
    pub kind: AskKind,
}

/// An idea's entry, as one revision of it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntryPage {
    pub entry: Entry,
    /// This revision, never reused: an entry at a revision never changes.
    pub revision: String,
    /// Whether a person has reviewed it, beyond kelve's own checks.
    pub reviewed: bool,
    /// Its claims, each with its key and where it's made: what a writer
    /// changing the entry must keep.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub claims: Vec<Claim>,
}

/// What an entry written against another revision does to it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Revise {
    pub kind: RevisionKind,
    /// The revision it was written against: its idea's current one, as the
    /// writer read it.
    pub base: String,
    /// For a fix or an extend, what it does, block by block. The entry it
    /// carries is the base with these applied; written against a revision
    /// since replaced, they're applied to the current one when they touch
    /// no block that changed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<Op>,
    /// For an extend: what it was asked to add.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<IdeaAsk>,
    /// The base's checked claims it leaves out, by key, each with why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<Dropped>,
}

/// A checked claim a revision leaves out, and why.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dropped {
    pub key: String,
    pub why: String,
}

/// One change in a changeset. An idea the same changeset adds is referred
/// to as `ref:` and its `ref` until it has an ID.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Change {
    /// Adds an idea, defined, or finds the one the graph already has.
    AddIdea {
        #[serde(rename = "ref")]
        reference: String,
        name: String,
        definition: String,
        #[serde(default)]
        kind: IdeaKind,
        #[serde(rename = "sameAs", default, skip_serializing_if = "Option::is_none")]
        same_as: Option<String>,
    },
    /// Writes an idea's entry. Its idea and the ideas its links lead to are
    /// IDs, or `ref:` an idea this changeset adds. Its claims, each with
    /// the numbers of the sources behind it, are for the graph's checks: an
    /// older kelve sends none.
    SetEntry {
        entry: Box<Entry>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        claims: Vec<Claim>,
        /// For an idea that has an entry: what this does to it. A fix,
        /// only while the graph's checks have found problems; an extend,
        /// to a sound entry; a rewrite, only when the entry is poor or
        /// mostly wrong (`kelve_core::entry::revise`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        revise: Option<Revise>,
    },
    /// Proposes that one idea relates to another, and why. Both are IDs, or
    /// `ref:` an idea this changeset adds. It's reviewed before anyone sees it.
    Relate {
        from: String,
        kind: RelationKind,
        to: String,
        /// One plain sentence: what `from` takes from `to`.
        why: String,
        /// For "rests on": 1 for the most needed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rank: Option<u8>,
        /// The model that proposed it, by its catalogue ID, so it's reviewed
        /// by others. None when a person proposes it by hand.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// Proposes taking a relation back, and why. The judges review it, as
    /// they would a report.
    Unrelate {
        from: String,
        kind: RelationKind,
        to: String,
        why: String,
    },
}

/// One relation, by its ends and kind.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RelationRef {
    pub from: String,
    pub kind: RelationKind,
    pub to: String,
}

/// An idea as a relation shows it: enough to draw it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub foundation: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u8>,
}

/// A relation from one idea, as readers see it: the idea at its other end,
/// why, where it stands, and its rank among rests-on.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rel {
    pub idea: Node,
    pub why: String,
    pub state: RelationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u8>,
}

/// What rests on an idea: how many, and the first of them, most read
/// first. The rest come a page at a time (`Client::up`).
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Up {
    pub count: u32,
    pub first: Vec<Rel>,
}

/// How one idea relates, both ways: everything served about it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeaRelations {
    /// What it rests on, most needed first.
    pub down: Vec<Rel>,
    pub kind_of: Vec<Rel>,
    pub answers: Vec<Rel>,
    pub alternatives: Vec<Rel>,
    pub used_with: Vec<Rel>,
    pub up: Up,
    /// The first principles it comes down to, by any way.
    pub grounds: Vec<Node>,
}

/// Relations asked for by idea ID, as `Ideas` is: by each ID found, IDs
/// that redirect, IDs of ideas found to be two, and IDs the graph doesn't
/// know.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relations {
    pub relations: BTreeMap<String, IdeaRelations>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub redirects: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub split: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

/// A page of what rests on an idea: `next` asks for the page after.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpPage {
    pub up: Vec<Rel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// The ground floor.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Foundations {
    pub ideas: Vec<Node>,
}

/// How many ideas a request for relations asks about at most.
pub const MOST_RELATIONS: usize = 50;

/// A factual statement an entry makes, and the numbers of its sources
/// that support it: none for what any textbook says.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub text: String,
    #[serde(default)]
    pub sources: Vec<u8>,
    /// The words on each cited page that say it, as the writer copied
    /// them: the graph finds each on its page before it checks the claim.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quotes: Vec<Quote>,
    /// The block that makes it, by id, when the writer said.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub block: String,
    /// Its key (`kelve_core::entry::claims::key`), as the graph serves
    /// it: the graph works it out from what's sent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
    /// Whether it's in a block the graph's checks since found wrong. A
    /// revision is served as it was, so the graph never sets it; kelve
    /// marks it from the idea's `faulted`, for its agent.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub faulted: bool,
}

/// The words on a page that support a claim, by the entry's number for the
/// page.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub source: u8,
    pub text: String,
}

/// A batch of changes, and what made them.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Changeset {
    /// What made it, and its version: `kelve/0.1.0`.
    pub agent: String,
    #[serde(default)]
    pub comment: String,
    pub changes: Vec<Change>,
    /// The person's agent that wrote what it adds, and the model it ran on,
    /// so one provider's work can be found again if it ever must be.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub written_by: Option<WrittenBy>,
}

/// Who wrote what a changeset adds: the person's agent, its model, and its
/// plan when the agent said.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrittenBy {
    /// The agent, as it names itself: `claude-code`.
    pub agent: String,
    /// The model it ran on, as it named it: `claude-opus-5-5`.
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

/// What a person's agent is told to do for one kind of job. The graph serves
/// them, so they improve for everyone at once.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instructions {
    pub name: String,
    pub version: u32,
    pub text: String,
}

/// What a person agrees to before anything they add goes into the graph.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terms {
    pub version: String,
    /// What the screen says, a point at a time.
    pub points: Vec<String>,
    /// The terms in full, as markdown.
    pub text: String,
}

/// Agreeing to a version of the terms.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agree {
    pub version: String,
}

/// A relation a reader followed: from one idea's entry to the idea it
/// relates to, by how it relates. Nothing else: no account, no install, no
/// project, no file, no time.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Followed {
    pub from: String,
    pub kind: RelationKind,
    pub to: String,
}

/// Relations followed, sent in a batch at most once an hour, in random
/// order, without the account's key.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Follows {
    pub follows: Vec<Followed>,
}

/// The most relations one batch of follows may carry.
pub const MOST_FOLLOWS: usize = 200;

/// What an account has added, and how often others read it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Added {
    /// Entries it added that the graph serves.
    pub entries: u32,
    /// Ways ideas relate it added that the graph serves.
    pub relations: u32,
    /// How many times its entries were served: close to one a reader,
    /// since each kelve keeps an entry once it has it.
    pub reads: u32,
}

/// The person's account, as the graph knows it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    /// The name the graph made up for them.
    pub handle: String,
    /// The version of the terms they agreed to, if any.
    #[serde(default)]
    pub agreed: Option<String>,
}

/// How the graph's check of an entry it hasn't served yet stands, for the
/// account that wrote it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checking {
    pub status: CheckStatus,
    /// Each claim, by its number from 1, in order.
    #[serde(default)]
    pub claims: Vec<ClaimChecked>,
    /// Why it was turned away, when it was, in a sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckStatus {
    /// Still being checked.
    Pending,
    /// Served to everyone.
    Accepted,
    /// Turned away.
    Rejected,
}

/// One claim's check.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimChecked {
    pub n: u32,
    pub state: ClaimState,
    /// For one that failed: what its pages don't show, or that a quote
    /// isn't on its page. kelve gives it, as it is, to the agent fixing
    /// the claim, with the claim's words and quotes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub found: String,
    /// Its key (`kelve_core::entry::claims::key`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
    /// Carried from an earlier check: the same claim, from the same words
    /// on a page that hasn't changed, so it wasn't judged again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub carried: bool,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaimState {
    Waiting,
    Held,
    Failed,
}

/// What a changeset did.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    pub id: String,
    /// The ID each `ref` became.
    pub ideas: BTreeMap<String, String>,
    /// Each entry written, by idea ID: its new revision. An entry stays
    /// pending, seen only by the account that wrote it, until the graph's
    /// own checks pass.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entries: BTreeMap<String, String>,
    /// Each relation taken, and where it stands: proposed, until it's
    /// reviewed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RelationTaken>,
    /// What it left out, and why, in sentences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<String>,
}

/// A relation a changeset proposed or took back, and where it stands.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationTaken {
    #[serde(flatten)]
    pub relation: RelationRef,
    pub state: RelationState,
}

/// One bucket of the name index: every live idea's name whose hash starts
/// with the bucket's prefix, keyed by the whole hash (`name_hash`), each
/// with the ideas that go by it. A kelve asks for a bucket by prefix and
/// finds its own name in it, so the graph never learns which name it was
/// after, as Have I Been Pwned does for passwords.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Bucket {
    pub names: BTreeMap<String, Vec<Candidate>>,
}

/// How many hex characters of a name's hash name its bucket: 65,536
/// buckets, so each holds many names.
pub const BUCKET_CHARS: usize = 4;

/// A name's hash, as the name index keys it: the SHA-256 of the name as
/// names are compared (`kelve_core::ideas::norm`), in hex.
pub fn name_hash(name: &str) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(kelve_core::ideas::norm(name).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a name, as names are compared, is too short and plain to be
/// taken as surely one idea without the asker's own judgement: a one-word
/// name of a few letters ("map", "set").
pub fn too_plain(norm: &str) -> bool {
    norm.chars().count() < 6 && !norm.contains(' ')
}

/// A reader's word that an entry, or one link in it, is wrong. The graph's
/// models check it again, with nothing that says who sent it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flag {
    /// The entry's revision, as the reader saw it.
    pub revision: String,
    /// A link's term, when it's the link that's wrong.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// A relation, when it's a relation that's wrong ("this didn't help me
    /// understand"); then `revision` may be empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<RelationRef>,
    /// What's wrong, in the reader's words.
    pub why: String,
}

/// A week of how one kelve was used, as numbers only, which its person chose to
/// share. It says nothing about who sent it, or what they opened, read or
/// wrote. Its one ID is the install's, random, made when its person said yes
/// and gone when they stop: never sent with the graph's key or reading tokens,
/// so the graph can't tie it to an account.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    /// The Monday the week began: `2026-10-12`.
    pub week: String,
    /// The install's ID: 32 hex characters, 128 random bits.
    pub install: String,
    /// What sent it, and its version: `kelve/0.1.0`.
    pub agent: String,
    /// kelve's version: `0.1.0`.
    pub version: String,
    /// `macos`, `windows` or `linux`.
    pub platform: String,
    /// Whole weeks since this kelve was first used: 0 in its first week.
    pub weeks_in: u32,
    /// Days in the week it was used.
    pub days: u8,
    /// Days on which three or more links were followed.
    pub deep_days: u8,
    /// Whether it adds to the shared graph.
    pub contributing: bool,
    /// Each count, by what it counts: `opened.change`, `links.followed`,
    /// `links.written`, `entries.read`, `notes.agent`, and so on.
    pub counts: BTreeMap<String, u32>,
    /// The same counts, a day at a time: each day with any use, as
    /// `2026-10-13`, the person's own date.
    #[serde(default)]
    pub by_day: BTreeMap<String, BTreeMap<String, u32>>,
}

/// What became public, oldest first, for mirrors: `next` is where to ask
/// from next time.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feed {
    pub changes: Vec<FeedChange>,
    pub next: i64,
}

/// One thing that became public.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedChange {
    pub seq: i64,
    /// `live`: an idea went live. `merged`: a live idea merged `into`
    /// another. `entry`: a `revision` became its idea's entry. `pulled`: its
    /// entry was reverted, back to `revision`, if any. `withdrawn`:
    /// a live idea left the graph. `relation`: a `relation` from `idea`
    /// came to `state`. `split`: a live idea was found to be two, one of
    /// which is `into`; it comes once for each.
    pub kind: String,
    pub idea: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<RelationRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<RelationState>,
    pub at: kelve_core::time::Timestamp,
}

/// An error, as RFC 9457 problem details.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub status: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

/// The title of the problem a changeset gets, with a 429, once its account
/// has added as much as it may in a day: `Retry-After` is the seconds to
/// midnight UTC, when the day's limits reset, and the detail says which
/// ran out, "as many ideas", "entries" or "relations", and "as a new
/// account" while the account's limits are a new account's.
pub const ENOUGH_FOR_TODAY: &str = "Enough for today";

/// What a person's kelve sends to sign in, once the graph has sent their
/// browser back to it: the one-time code it came with, the PKCE verifier
/// only this kelve holds, and the address the code was sent to.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignInCode {
    pub code: String,
    pub code_verifier: String,
    pub redirect_uri: String,
}

impl std::fmt::Debug for SignInCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignInCode")
            .field("redirect_uri", &self.redirect_uri)
            .finish_non_exhaustive()
    }
}

/// What it gets for the code: a key of its own, and the name what it adds
/// is shown with, which the graph makes up.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedIn {
    pub key: String,
    pub handle: String,
}

impl std::fmt::Debug for SignedIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignedIn")
            .field("handle", &self.handle)
            .finish_non_exhaustive()
    }
}

/// The S256 PKCE challenge for a verifier (RFC 7636): the verifier's
/// SHA-256, in base64url without padding.
pub fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine;
    use sha2::Digest;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(sha2::Sha256::digest(verifier.as_bytes()))
}

/// Whether an ID stands for an idea added in the same changeset.
pub fn is_ref(id: &str) -> bool {
    id.starts_with(REF)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;

    #[test]
    fn claims_go_with_an_entry_and_an_older_kelve_sends_none() {
        let entry = serde_json::json!({
            "idea": { "id": "ref:a", "name": "Advisory locks" },
            "title": "Advisory locks", "minutes": 2, "madeAt": "2026-10-15T00:00:00.000Z",
            "blocks": [], "sources": [], "links": []
        });
        let older: Change =
            serde_json::from_value(serde_json::json!({ "op": "setEntry", "entry": entry }))
                .expect("an older kelve's entry reads");
        let Change::SetEntry { claims, .. } = &older else {
            panic!("an entry")
        };
        assert!(claims.is_empty());
        assert!(
            serde_json::to_value(&older).expect("written")["claims"].is_null(),
            "none, then nothing sent"
        );
        let newer: Change = serde_json::from_value(serde_json::json!({
            "op": "setEntry", "entry": entry,
            "claims": [{ "text": "The database never enforces them.", "sources": [1] }]
        }))
        .expect("reads");
        let Change::SetEntry { claims, .. } = newer else {
            panic!("an entry")
        };
        assert_eq!(claims[0].sources, vec![1]);
    }

    #[test]
    fn a_relation_says_what_it_is_on_the_wire() {
        let c = Change::Relate {
            from: "kelve:i1".into(),
            kind: RelationKind::RestsOn,
            to: "ref:kv".into(),
            why: "What's kept is the keys attention computed.".into(),
            rank: Some(1),
            model: None,
        };
        let json = serde_json::to_value(&c).expect("json");
        assert_eq!(json["op"], "relate");
        assert_eq!(json["kind"], "restsOn");
        assert!(json.get("model").is_none(), "a person's sends none");
        assert_eq!(serde_json::from_value::<Change>(json).expect("back"), c);
        let taken = RelationTaken {
            relation: RelationRef {
                from: "a".into(),
                kind: RelationKind::Answers,
                to: "b".into(),
            },
            state: RelationState::Proposed,
        };
        let json = serde_json::to_value(&taken).expect("json");
        assert_eq!(json["kind"], "answers");
        assert_eq!(json["state"], "proposed");
    }

    #[test]
    fn names_hash_as_they_compare() {
        assert_eq!(name_hash("Write-ahead logs"), name_hash("write ahead  log"));
        assert_ne!(name_hash("C"), name_hash("C++"));
        assert_eq!(name_hash("x").len(), 64);
        assert!(too_plain("map") && !too_plain("advisory lock") && !too_plain("postgres"));
    }

    #[test]
    fn pkce_challenges_are_as_the_rfc_writes_them() {
        // RFC 7636, appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn changes_say_what_they_are_on_the_wire() {
        let c = Change::AddIdea {
            reference: "a".into(),
            name: "Advisory locks".into(),
            definition: "A lock a program names and the database keeps.".into(),
            kind: IdeaKind::Concept,
            same_as: None,
        };
        let json = serde_json::to_value(&c).expect("json");
        assert_eq!(json["op"], "addIdea");
        assert_eq!(json["ref"], "a");
        let back: Change = serde_json::from_value(json).expect("read back");
        assert_eq!(back, c);
        assert!(is_ref("ref:a"));
        assert!(!is_ref("kelve:i3k9xq2m7twa8"));
    }
}
