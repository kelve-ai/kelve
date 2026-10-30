//! An entry, as the shared graph keeps it and the reader shows it: the page
//! for one idea, as a list of blocks, each one of kelve's components, with
//! the sources they cite and the terms that lead to other ideas. The
//! blocks are the record. The page is drawn from them by kelve's own
//! components, and the markdown that's exported is written from them by
//! kelve. A model never writes either: it fills in a draft (`draft`), and
//! kelve checks it (`check`) and copies every quote, number and source in
//! from what it holds.

pub mod check;
pub mod claims;
pub mod copied;
pub mod draft;
pub mod known;
pub mod markdown;
pub mod revise;
pub mod tells;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ideas::{IdeaRef, LinkKind};
use crate::time::Timestamp;

/// Text with a little markup, and nothing else: `code`, **the key
/// sentence**, `^1` for a source in the margin, and `[[term]]` for a term
/// that leads to another idea. Never HTML, and never a link to anywhere
/// else: the reader draws it as text, and kelve makes each term a link to
/// the idea the graph matched.
pub type Rich = String;

/// An entry: the page for one idea. Every idea has exactly one.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Entry {
    /// The idea it's about.
    pub idea: IdeaRef,
    /// A title that says what it's about, in plain words.
    pub title: String,
    /// The one question it answers, as a reader would ask it: the line under
    /// the idea's name. Empty in an entry written before there was one.
    #[serde(default)]
    pub question: String,
    /// About how long it takes to read, in minutes.
    pub minutes: u8,
    /// When kelve finished writing it.
    pub made_at: Timestamp,
    pub blocks: Vec<Block>,
    pub sources: Vec<Source>,
    /// The terms in its prose that lead on, each to an idea.
    pub links: Vec<Link>,
}

/// A term in an entry's prose, written `[[term]]`, and the idea it leads
/// to. The graph matched the idea; the model only named it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "EntryLink")]
pub struct Link {
    /// The words as the prose has them, without the brackets.
    pub term: String,
    pub idea: IdeaRef,
    /// What the link says about it, for the graph; readers see words.
    pub kind: LinkKind,
}

/// One block of an entry: one of kelve's components.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Block {
    /// Unique within its entry, and kept from one revision to the next:
    /// `b1`, `b2`. A block added later gets an id no block had.
    pub id: String,
    /// The part of the entry it's in. None in an entry written before
    /// entries had parts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub part: Option<Part>,
    /// The fold it's in, under its part, closed until the reader opens it.
    /// None for a block that's always shown. Blocks in a row with the same
    /// part and fold are one fold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fold: Option<Fold>,
    pub body: Body,
}

impl Block {
    /// A block always shown, in no part: as entries were before parts.
    pub fn new(id: impl Into<String>, body: Body) -> Block {
        Block {
            id: id.into(),
            part: None,
            fold: None,
            body,
        }
    }
}

/// The parts of an entry, in the order they come: an opening that stands
/// alone, then five parts, each optional but the opening.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Part {
    /// What it is, in a few sentences that make sense alone: always shown.
    Lead,
    /// Why it exists: the problem, in a concrete case.
    Why,
    /// How it works: from the concrete case to the mechanism, named.
    How,
    /// Why it works this way: the constraint or principle beneath it.
    WhyThisWay,
    /// Where it stops, and what it costs.
    Limits,
    /// What else does the job.
    Alternatives,
}

impl Part {
    /// Every part, in order.
    pub const ALL: [Part; 6] = [
        Part::Lead,
        Part::Why,
        Part::How,
        Part::WhyThisWay,
        Part::Limits,
        Part::Alternatives,
    ];

    /// As the API writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Part::Lead => "lead",
            Part::Why => "why",
            Part::How => "how",
            Part::WhyThisWay => "whyThisWay",
            Part::Limits => "limits",
            Part::Alternatives => "alternatives",
        }
    }

    /// The part a word names, if it names one.
    pub fn parse(word: &str) -> Option<Part> {
        Part::ALL.into_iter().find(|p| p.as_str() == word.trim())
    }
}

/// What a fold holds: more than the part says, for the reader who wants it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Fold {
    /// One case worked through, with real numbers.
    Example,
    /// A common mistake: what people believe, why it's wrong, and what's
    /// true.
    Mistake,
    /// Something that looks like the idea but isn't it.
    LookAlike,
    /// The precise rule behind something the part says "usually" of.
    Rule,
    /// More depth, for the reader who wants it.
    Depth,
}

impl Fold {
    /// Every kind of fold.
    pub const ALL: [Fold; 5] = [
        Fold::Example,
        Fold::Mistake,
        Fold::LookAlike,
        Fold::Rule,
        Fold::Depth,
    ];

    /// What it says it holds, closed, as the reader shows it, and as an
    /// export titles it.
    pub fn title(self) -> &'static str {
        match self {
            Fold::Example => "A worked example",
            Fold::Mistake => "A common mistake",
            Fold::LookAlike => "Looks like it, but isn’t",
            Fold::Rule => "The precise rule",
            Fold::Depth => "More on this",
        }
    }

    /// As the API writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Fold::Example => "example",
            Fold::Mistake => "mistake",
            Fold::LookAlike => "lookAlike",
            Fold::Rule => "rule",
            Fold::Depth => "depth",
        }
    }

    /// The fold a word names, if it names one.
    pub fn parse(word: &str) -> Option<Fold> {
        Fold::ALL.into_iter().find(|f| f.as_str() == word.trim())
    }
}

/// What a block is.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export)]
pub enum Body {
    /// A paragraph of two or three sentences. `quiet` for an aside.
    Paragraph { text: Rich, quiet: bool },
    /// A heading for a longer entry: a statement, never a label.
    Heading { text: String },
    /// Bullets, only where something is a list.
    List { items: Vec<Rich> },
    /// Lines of the real thing: a change, a file, a message, a clause.
    /// kelve copies the text in from what it heard; the model only says
    /// which lines and which to mark.
    Excerpt {
        from: String,
        /// What it's written in, as kelve tells from what it quoted: a
        /// language in `code::LANGUAGES`, `prose`, or none for plain output.
        #[serde(default)]
        lang: Option<String>,
        lines: Vec<Line>,
    },
    /// A system, a process, a cycle, a timeline: drawn by kelve. With
    /// steps, it plays through them once and rests on the last.
    Diagram {
        layout: Layout,
        nodes: Vec<Node>,
        edges: Vec<Edge>,
        steps: Vec<Frame>,
        /// The diagram in words, for anyone who can't see it, and shown
        /// under it.
        describe: String,
    },
    /// Your own numbers, to move: a formula kelve knows, with inputs.
    Explorable {
        formula: Formula,
        inputs: Vec<Input>,
        /// What the result is, in words: "Monthly payment".
        output: String,
        describe: String,
    },
    /// Bars, for a few numbers side by side.
    Bars {
        rows: Vec<Bar>,
        caption: Option<String>,
    },
    /// A small table. `marks` are `[row, column]` cells kelve's red marks.
    Table {
        head: Vec<String>,
        rows: Vec<Vec<String>>,
        marks: Vec<[u16; 2]>,
    },
    /// Two approaches, or two options, side by side.
    Compare { left: Side, right: Side },
    /// Short lines, each with a square: red for the one that matters.
    Squares { rows: Vec<Square> },
    /// When things happened.
    Timeline { rows: Vec<Moment> },
    /// Whoever you'd have to convince, and what they'd say.
    Conversation { lines: Vec<Said> },
    /// "So if anyone asks…": one sentence you can use.
    Answer { lead: String, text: Rich },
    /// Who says what to whom, in order: people, programs or parts, each
    /// message an arrow from one to another, drawn in turn.
    Sequence {
        actors: Vec<String>,
        messages: Vec<SequenceMessage>,
        describe: String,
    },
    /// A process to walk through a stage at a time, at the reader's pace.
    Steps { cards: Vec<StepCard> },
    /// Things placed on two axes, each from 0 to 100.
    Quadrant {
        x: QuadrantAxis,
        y: QuadrantAxis,
        points: Vec<QuadrantPoint>,
        describe: String,
    },
    /// Where things sit between two ends, from 0 to 100.
    Spectrum {
        low: String,
        high: String,
        points: Vec<SpectrumPoint>,
        describe: String,
    },
    /// A line or two over time, drawn as you reach it.
    Trend {
        /// Along the bottom, one for each value: "2018", "2020".
        labels: Vec<String>,
        series: Vec<TrendSeries>,
        unit: Option<String>,
        describe: String,
    },
    /// A decision that branches, from one question at the top.
    Tree {
        branches: Vec<TreeBranch>,
        describe: String,
    },
    /// Two versions of a thing, to drag between.
    BeforeAfter {
        before: Version,
        after: Version,
        describe: String,
    },
    /// One number, what it means, and where it's from.
    Number {
        /// As shown: "1 in 3", "62%", "£4,200".
        figure: String,
        text: Rich,
        /// How much of the whole it is, from 0 to 100, drawn as a bar.
        share: Option<f64>,
    },
    /// Lines of the real thing, with notes pinned to the ones that matter.
    /// kelve copies the lines in, as for an excerpt.
    Annotated {
        from: String,
        #[serde(default)]
        lang: Option<String>,
        lines: Vec<Line>,
        notes: Vec<CodeNote>,
    },
    /// How many make it through each stage, from the first.
    Funnel {
        stages: Vec<FunnelStage>,
        caption: Option<String>,
    },
}

impl Body {
    /// The block's type, as the API writes it.
    pub fn name(&self) -> &'static str {
        match self {
            Body::Paragraph { .. } => "paragraph",
            Body::Heading { .. } => "heading",
            Body::List { .. } => "list",
            Body::Excerpt { .. } => "excerpt",
            Body::Diagram { .. } => "diagram",
            Body::Explorable { .. } => "explorable",
            Body::Bars { .. } => "bars",
            Body::Table { .. } => "table",
            Body::Compare { .. } => "compare",
            Body::Squares { .. } => "squares",
            Body::Timeline { .. } => "timeline",
            Body::Conversation { .. } => "conversation",
            Body::Answer { .. } => "answer",
            Body::Sequence { .. } => "sequence",
            Body::Steps { .. } => "steps",
            Body::Quadrant { .. } => "quadrant",
            Body::Spectrum { .. } => "spectrum",
            Body::Trend { .. } => "trend",
            Body::Tree { .. } => "tree",
            Body::BeforeAfter { .. } => "beforeAfter",
            Body::Number { .. } => "number",
            Body::Annotated { .. } => "annotated",
            Body::Funnel { .. } => "funnel",
        }
    }
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "ExcerptLine")]
pub struct Line {
    /// Its line number in the real thing, when it has one.
    pub n: Option<u32>,
    pub text: String,
    /// The line that matters, marked in red.
    pub mark: bool,
    /// Context, shown faint.
    pub dim: bool,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Layout {
    /// Left to right: a path, a pipeline.
    Row,
    /// Round and back to the start.
    Cycle,
    /// Top to bottom: layers.
    Stack,
    /// Along a line of time.
    Timeline,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "DiagramNode")]
pub struct Node {
    pub id: String,
    pub label: String,
    pub note: Option<String>,
    /// The point that matters, in red.
    pub mark: bool,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "DiagramEdge")]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
}

/// One step of a step-through: a caption, and the nodes it marks.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, rename = "DiagramStep")]
pub struct Frame {
    pub caption: String,
    pub marks: Vec<String>,
}

/// The formulas an explorable can use. kelve computes them; the model only
/// picks one and sets its inputs.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Formula {
    /// A loan's monthly payment: `principal`, `rate` (% a year), `years`.
    LoanPayment,
    /// Growth over time: `start`, `rate` (% a year), `years`.
    CompoundGrowth,
    /// The chance that at least `k` of `n` go one way by luck, each with
    /// chance `p` (%).
    ChanceAtLeast,
    /// Agreement beyond chance (Cohen's kappa): `observed` (%) and
    /// `byChance` (%).
    AgreementBeyondChance,
    /// The change from `before` to `after`, as a percentage.
    PercentChange,
    /// An amount at a rate over a time: `rate` and `time`.
    RateTimesTime,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, rename = "ExplorableInput")]
pub struct Input {
    /// The formula's name for it: `principal`, `rate`.
    pub name: String,
    pub label: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub value: f64,
    pub unit: Option<String>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Bar {
    pub label: String,
    /// From 0 to 100, how long the bar is.
    pub value: f64,
    /// The number as shown: "123 of 184".
    pub display: String,
    pub mark: bool,
    pub quiet: bool,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Side {
    pub title: String,
    pub points: Vec<Rich>,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Square {
    pub red: bool,
    pub text: Rich,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Moment {
    /// Rough times only: "Friday afternoon", "9 October".
    pub when: String,
    pub text: String,
    pub mark: Mark,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Mark {
    None,
    Ink,
    Red,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Said {
    pub who: String,
    pub text: Rich,
}

/// One arrow in a sequence: from one actor to another, by name.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SequenceMessage {
    pub from: String,
    pub to: String,
    pub text: String,
    /// The one that matters, in red.
    pub mark: bool,
}

/// One stage of a walk-through.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StepCard {
    pub title: String,
    pub text: Rich,
}

/// An axis of a quadrant: what its low and high ends mean.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QuadrantAxis {
    pub low: String,
    pub high: String,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct QuadrantPoint {
    pub label: String,
    /// From 0, the low end, to 100, the high end.
    pub x: f64,
    pub y: f64,
    pub mark: bool,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpectrumPoint {
    pub label: String,
    /// From 0, the low end, to 100, the high end.
    pub at: f64,
    pub note: Option<String>,
    pub mark: bool,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TrendSeries {
    pub label: String,
    /// One for each of the trend's labels.
    pub values: Vec<f64>,
    pub mark: bool,
}

/// A branch of a tree: the question at the top has no parent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TreeBranch {
    pub id: String,
    pub label: String,
    /// The branch it comes from, earlier in the list.
    pub parent: Option<String>,
    /// The words on the way down to it: "yes", "under 10 people".
    pub edge: Option<String>,
    pub mark: bool,
}

/// One side of a before and after: lines of the real thing kelve copied
/// in (with `from`), or lines written to show the idea (without).
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Version {
    pub label: String,
    pub from: Option<String>,
    /// What it's written in: a language in `code::LANGUAGES`, or none.
    pub lang: Option<String>,
    pub lines: Vec<String>,
}

/// A note pinned to a line of an annotated excerpt.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CodeNote {
    /// Where the line is among the excerpt's lines, from 0.
    pub line: u16,
    pub text: Rich,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FunnelStage {
    pub label: String,
    /// How many reach it, in any unit, the same for every stage.
    pub value: f64,
    /// The number as shown: "4,100", "62%".
    pub display: String,
    pub mark: bool,
}

/// A source an entry cites: a page kelve fetched from its publisher, or a
/// session it heard. Numbered as the margin shows them.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Source {
    pub n: u8,
    pub title: String,
    /// Who published it, or wrote it: "Docker", "Panickssery, Bowman and Feng".
    pub publisher: String,
    /// Its address, from the publisher; none for a session kelve heard.
    pub url: Option<String>,
    /// When it was published, as the page says: "2024".
    pub published: Option<String>,
    /// A preprint, not yet published.
    pub preprint: bool,
    /// A copy at the Internet Archive, if kelve made one.
    pub archived: Option<String>,
}
