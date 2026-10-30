//! What a model fills in to write an entry, and how kelve turns it into one.
//!
//! The model fills one wide, flat object per block, with every field there
//! and the ones a block doesn't use left empty. That's the shape every
//! provider's structured outputs accept at once: no optional fields, no
//! unions, no recursion, no bounds. kelve then builds typed blocks from it
//! (`build`), copying in what the model only points at: the lines of an
//! excerpt, by its id and line numbers, and the sources, by number. A model
//! never supplies quoted text or an address. The terms it links, `[[term]]`,
//! it only names: kelve matches each to a node. `check` comes after.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::check::linked_terms;
use super::check::{Checker, Problem, Rule, Span, TITLE, spans};
use super::revise::{Op, RevisionKind};
use super::{
    Bar, Block, Body, CodeNote, Edge, Fold, Formula, Frame, FunnelStage, Input, Layout, Line, Mark,
    Moment, Node, Part, QuadrantAxis, QuadrantPoint, Said, SequenceMessage, Side, Source,
    SpectrumPoint, Square, StepCard, TreeBranch, TrendSeries, Version,
};
use crate::formula;
use crate::ideas::{IdeaKind, LinkKind};
use crate::text::clip;

/// Every block type, as `Body::name` writes it.
const TYPES: [&str; 23] = [
    "paragraph",
    "heading",
    "list",
    "excerpt",
    "diagram",
    "explorable",
    "bars",
    "table",
    "compare",
    "squares",
    "timeline",
    "conversation",
    "answer",
    "sequence",
    "steps",
    "quadrant",
    "spectrum",
    "trend",
    "tree",
    "beforeAfter",
    "number",
    "annotated",
    "funnel",
];

/// Every part, as `Part::as_str` writes it.
const PARTS: [&str; 6] = ["lead", "why", "how", "whyThisWay", "limits", "alternatives"];

/// Every fold, as `Fold::as_str` writes it, and none.
const FOLDS: [&str; 6] = ["", "example", "mistake", "lookAlike", "rule", "depth"];

const FORMULAS: [Formula; 6] = [
    Formula::LoanPayment,
    Formula::CompoundGrowth,
    Formula::ChanceAtLeast,
    Formula::AgreementBeyondChance,
    Formula::PercentChange,
    Formula::RateTimesTime,
];

/// The longest line of an excerpt kelve shows; longer ones are clipped.
const EXCERPT_LINE: usize = 240;
/// The longest claim.
const CLAIM: usize = 400;
/// The longest line about an idea a link leads to.
const ABOUT: usize = 200;

/// What prose may carry.
const RICH: &str = "Prose: plain words, with `code`, **the key sentence**, ^1 for source 1 and [[term]] for a term that leads to another idea's entry, and nothing else.";

/// The JSON Schema for a model's draft. Every property is required and no
/// object takes more, with no unions, recursion, bounds or patterns, so
/// every provider's structured outputs accept it. It's kept small: a block
/// names its type and carries its prose, and every other field its type has
/// comes as one JSON object in `data`, as [`guide`] lists them. A schema
/// with every field of every block compiles to a grammar too large for
/// Anthropic's structured outputs. Properties are written in the order a
/// model should fill them in.
pub fn schema() -> Value {
    let rich = RICH;
    let block = object(&[
        ("id", string("Unique in the entry: b1, b2, …")),
        (
            "part",
            choices(
                &PARTS,
                "The part it's in: lead, the opening that stands alone; why it exists; how it works; why it works this way; where it stops (limits); what else does the job (alternatives).",
            ),
        ),
        (
            "fold",
            choices(
                &FOLDS,
                "\"\" for a block always shown. Else the fold it's in, under its part, closed until opened: a worked example, a common mistake, a look-alike that isn't it, the precise rule behind a \"usually\", or more depth.",
            ),
        ),
        ("type", choices(&TYPES, "Which of kelve's blocks this is.")),
        (
            "text",
            string(&format!(
                "paragraph, heading, answer and number: the text; else \"\". {rich}"
            )),
        ),
        (
            "data",
            string(
                "Every other field this block's type has, as one JSON object, as the guide to blocks lists them; \"{}\" when it has none.",
            ),
        ),
    ]);
    object(&[
        (
            "title",
            string("What it's about, in plain words, under 90 characters."),
        ),
        ("minutes", integer("About how long it takes to read.")),
        ("blocks", array(block)),
        (
            "links",
            array(object(&[
                (
                    "term",
                    string("The words linked in the prose, exactly as written inside [[ ]]."),
                ),
                (
                    "idea",
                    string(
                        "The idea it leads to, by the name its own entry would have: \"Control planes\", \"kubectl\", \"Advisory locks\". Use an idea from the list given when it's the same thing.",
                    ),
                ),
                ("about", string("What it is, in one line.")),
                (
                    "kind",
                    choices(
                        &["concept", "tool", "technology", "practice"],
                        "What the idea is.",
                    ),
                ),
                (
                    "link",
                    choices(
                        &["restsOn", "kindOf", "alternative", "related"],
                        "restsOn: this entry's idea rests on it, so a reader may need it first; kindOf: this entry's idea is a kind of it; alternative: another answer to the same question; related: used alongside it.",
                    ),
                ),
            ])),
        ),
        (
            "claims",
            array(object(&[
                ("text", string("One factual statement the entry makes.")),
                (
                    "sources",
                    integers("The numbers of the sources that support it."),
                ),
            ])),
        ),
    ])
}

/// The schema for an entry an agent writes from pages it found itself: a
/// draft, with its sources listed, and, for each claim, the words on each
/// page it cites that say it.
pub fn agent_schema() -> Value {
    let mut s = schema();
    let quote = object(&[
        ("source", integer("The number of the source, in sources.")),
        (
            "text",
            string("The sentence or two on that page that says it, copied exactly."),
        ),
    ]);
    if let Some(claim) = s["properties"]["claims"]["items"].as_object_mut() {
        claim["properties"]["quotes"] = array(quote);
        claim["properties"]["block"] =
            string("The id of the block that makes it, or \"\" for a claim only a figure makes.");
        claim["properties"]["words"] = string(
            "The words of that block's text that make it, copied exactly: a clause or a sentence.",
        );
        if let Some(required) = claim["required"].as_array_mut() {
            required.push(json!("quotes"));
            required.push(json!("block"));
            required.push(json!("words"));
        }
    }
    let source = object(&[
        (
            "n",
            integer("Its number, from 1, as the entry cites it: ^1."),
        ),
        ("url", string("Its https address, as you read it.")),
        ("title", string("Its title.")),
        ("publisher", string("Who published it, or wrote it.")),
        (
            "published",
            string("When it was published, as the page says, or \"\"."),
        ),
    ]);
    if let Some(top) = s.as_object_mut() {
        top["properties"]["sources"] = array(source);
        if let Some(required) = top["required"].as_array_mut() {
            required.push(json!("sources"));
        }
    }
    s
}

/// The schema for what a writer leaves out of an entry it replaces: the
/// claims checked before that it drops, by number, each with why.
fn drop_schema() -> Value {
    array(object(&[
        (
            "claim",
            integer("The number of a checked claim, as the job lists them."),
        ),
        ("why", string("Why it goes, in a sentence.")),
    ]))
}

/// The schema for an entry that replaces one the graph has: an agent's
/// entry, and the checked claims it drops.
pub fn replacing_schema() -> Value {
    let mut s = agent_schema();
    if let Some(top) = s.as_object_mut() {
        top["properties"]["drop"] = drop_schema();
        if let Some(required) = top["required"].as_array_mut() {
            required.push(json!("drop"));
        }
    }
    s
}

/// The schema for a fix's or an extend's answer: only the blocks it
/// gives, their claims, links and new pages, with no title or minutes. A
/// fix also says the blocks it removes; an extend says, for each block,
/// the block it follows.
pub fn revision_schema(kind: RevisionKind) -> Value {
    let mut s = replacing_schema();
    let Some(top) = s.as_object_mut() else {
        return s;
    };
    if let Some(props) = top["properties"].as_object_mut() {
        props.remove("title");
        props.remove("minutes");
    }
    if let Some(required) = top["required"].as_array_mut() {
        required.retain(|r| r != "title" && r != "minutes");
    }
    match kind {
        RevisionKind::Fix => {
            top["properties"]["remove"] = array(object(&[
                ("id", string("The id of a block found wrong that goes.")),
                ("why", string("Why it goes, in a sentence.")),
            ]));
            if let Some(required) = top["required"].as_array_mut() {
                required.push(json!("remove"));
            }
        }
        RevisionKind::Extend => {
            if let Some(block) = top["properties"]["blocks"]["items"].as_object_mut() {
                block["properties"]["follows"] = string(
                    "The id of the block it goes after: the last of its part, or the block it explains.",
                );
                if let Some(required) = block["required"].as_array_mut() {
                    required.push(json!("follows"));
                }
            }
        }
        RevisionKind::Rewrite => {}
    }
    s
}

/// Prose without its markup, as the reader reads it: `[[term]]` as the
/// term, **key** and `code` as their words, and `^1` gone.
pub fn plain_prose(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(r) = rest.strip_prefix("[[").or_else(|| rest.strip_prefix("]]")) {
            rest = r;
        } else if let Some(r) = rest.strip_prefix("**") {
            rest = r;
        } else if let Some(r) = rest.strip_prefix('`') {
            rest = r;
        } else if let Some(r) = rest.strip_prefix('^')
            && r.starts_with(|d: char| d.is_ascii_digit())
        {
            rest = r.trim_start_matches(|d: char| d.is_ascii_digit());
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// Where a claim is made: its words, without markup, when they're in the
/// prose of the block it names.
fn place_claim(blocks: &[Block], block: &str, words: &str) -> Option<String> {
    let words = plain_prose(words).trim().to_string();
    if block.is_empty() || words.is_empty() {
        return None;
    }
    let found = blocks.iter().find(|b| b.id == block)?;
    let texts: Vec<&str> = match &found.body {
        Body::Paragraph { text, .. } | Body::Heading { text } => vec![text.as_str()],
        Body::List { items } => items.iter().map(String::as_str).collect(),
        _ => Vec::new(),
    };
    texts
        .iter()
        .any(|t| plain_prose(t).contains(&words))
        .then_some(words)
}

/// The most sources an agent's entry may list.
pub const MOST_SOURCES: usize = 8;

/// The pages an agent says it wrote an entry from, as the material it's
/// built with: each numbered once, at an https address.
pub fn sources_of(value: &Value) -> Result<Material, Problem> {
    let shape = |says: String| Problem {
        block: None,
        rule: Rule::Shape,
        says,
    };
    let listed = value["sources"].as_array().cloned().unwrap_or_default();
    if listed.len() > MOST_SOURCES {
        return Err(shape(format!(
            "It lists {} sources. Use at most {MOST_SOURCES}.",
            listed.len()
        )));
    }
    let mut sources: Vec<SourceMaterial> = Vec::new();
    for s in listed {
        let n = s["n"]
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .filter(|n| *n >= 1)
            .ok_or_else(|| shape("A source has no number from 1.".into()))?;
        if sources.iter().any(|m| m.n == n) {
            return Err(shape(format!("Two sources are numbered {n}.")));
        }
        let url = s["url"].as_str().unwrap_or_default().trim().to_string();
        if !url.starts_with("https://") || url.len() > 2000 || url.chars().any(char::is_whitespace)
        {
            return Err(shape(format!(
                "Source {n}'s address must be one https address, as you read it."
            )));
        }
        let title = clip(s["title"].as_str().unwrap_or_default().trim(), 200);
        if title.is_empty() {
            return Err(shape(format!("Source {n} has no title.")));
        }
        let publisher = clip(s["publisher"].as_str().unwrap_or_default().trim(), 100);
        let published = s["published"]
            .as_str()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| clip(p, 40));
        let preprint = [
            "arxiv.org",
            "biorxiv.org",
            "medrxiv.org",
            "ssrn.com",
            "osf.io",
        ]
        .iter()
        .any(|h| url.contains(h));
        sources.push(SourceMaterial {
            n,
            source: Source {
                n,
                title,
                publisher,
                url: Some(url),
                published,
                preprint,
                archived: None,
            },
        });
    }
    Ok(Material {
        excerpts: Vec::new(),
        sources,
    })
}

/// Every field a block can have, by type, with what it's for: what [`guide`]
/// describes, and what a block's `data` is read into.
fn wide() -> Value {
    let formulas: Vec<String> = FORMULAS
        .iter()
        .map(|f| format!("{} ({})", name_of(f), formula::names(*f).join(", ")))
        .collect();
    let rich = RICH;
    object(&[
        ("id", string("Unique in the entry: b1, b2, …")),
        ("type", choices(&TYPES, "Which of kelve's blocks this is.")),
        (
            "text",
            string(&format!(
                "paragraph, heading, answer and number: the text. {rich}"
            )),
        ),
        ("quiet", boolean("paragraph: an aside, shown quietly.")),
        ("items", strings(&format!("list: the items. {rich}"))),
        (
            "excerpt",
            string(
                "excerpt and annotated: the id of an excerpt kelve handed over. kelve copies its text in.",
            ),
        ),
        (
            "lines",
            integers(
                "excerpt and annotated: the numbers of the lines to show, as kelve numbered them; empty for all.",
            ),
        ),
        (
            "marks",
            integers("excerpt: the numbers of the lines that matter, marked in red."),
        ),
        (
            "layout",
            choices(
                &["", "row", "cycle", "stack", "timeline"],
                "diagram: how it's laid out.",
            ),
        ),
        (
            "nodes",
            array(object(&[
                ("id", string("Unique in the diagram.")),
                ("label", string("A few words.")),
                ("note", string("A short note under it, or \"\".")),
                ("mark", boolean("The point that matters, in red.")),
            ])),
        ),
        (
            "edges",
            array(object(&[
                ("from", string("A node id.")),
                ("to", string("A node id.")),
                ("label", string("A few words, or \"\".")),
            ])),
        ),
        (
            "steps",
            array(object(&[
                ("caption", string("What happens at this step.")),
                ("marks", strings("The ids of the nodes this step marks.")),
            ])),
        ),
        (
            "describe",
            string(
                "diagram, explorable, sequence, quadrant, spectrum, trend, tree and beforeAfter: what it shows, in words, for anyone who can't see it.",
            ),
        ),
        (
            "formula",
            choices(
                &std::iter::once("")
                    .chain(FORMULAS.iter().map(name_of))
                    .collect::<Vec<_>>(),
                &format!(
                    "explorable: the formula kelve computes, and its inputs: {}.",
                    formulas.join("; ")
                ),
            ),
        ),
        (
            "inputs",
            array(object(&[
                ("name", string("The formula's name for it.")),
                ("label", string("What it is, in words.")),
                ("min", number()),
                ("max", number()),
                ("step", number()),
                ("value", number()),
                ("unit", string("Its unit, or \"\".")),
            ])),
        ),
        (
            "output",
            string("explorable: what the result is, in words, with its unit."),
        ),
        (
            "bars",
            array(object(&[
                ("label", string("What the bar is.")),
                ("value", number()),
                ("display", string("The number as shown: \"123 of 184\".")),
                ("mark", boolean("The one that matters.")),
                ("quiet", boolean("For comparison only.")),
            ])),
        ),
        ("caption", string("bars and funnel: a caption, or \"\".")),
        ("head", strings("table: the column headings.")),
        ("rows", array(strings("A row of cells, one per heading."))),
        (
            "cellMarks",
            array(integers("A cell to mark in red: [row, column], from 0.")),
        ),
        ("left", side()),
        ("right", side()),
        (
            "squares",
            array(object(&[
                ("red", boolean("The one that matters.")),
                ("text", string(rich)),
            ])),
        ),
        (
            "moments",
            array(object(&[
                ("when", string("A rough time: \"Friday afternoon\".")),
                ("text", string("What happened.")),
                ("mark", choices(&["none", "ink", "red"], "How it's marked.")),
            ])),
        ),
        (
            "said",
            array(object(&[
                ("who", string("Who's speaking.")),
                ("text", string(rich)),
            ])),
        ),
        (
            "lead",
            string("answer: the lead, \"So if anyone asks why the builds broke:\"."),
        ),
        (
            "actors",
            strings(
                "sequence: 2 to 5 people, programs or parts, left to right, in a few words each.",
            ),
        ),
        (
            "messages",
            array(object(&[
                ("from", string("An actor, exactly as named.")),
                ("to", string("Another actor, exactly as named.")),
                (
                    "text",
                    string("What goes from one to the other, in a few words."),
                ),
                ("mark", boolean("The one that matters.")),
            ])),
        ),
        (
            "cards",
            array(object(&[
                ("title", string("steps: this stage, in a few words.")),
                (
                    "text",
                    string(&format!(
                        "What happens at this stage, in a sentence or two. {rich}"
                    )),
                ),
            ])),
        ),
        (
            "xLow",
            string("quadrant: what the left of the across axis means."),
        ),
        (
            "xHigh",
            string("quadrant: what the right of the across axis means."),
        ),
        (
            "yLow",
            string("quadrant: what the bottom of the up axis means."),
        ),
        (
            "yHigh",
            string("quadrant: what the top of the up axis means."),
        ),
        (
            "points",
            array(object(&[
                ("label", string("quadrant: what it is, in a few words.")),
                ("x", number_about("Across, from 0 to 100.")),
                ("y", number_about("Up, from 0 to 100.")),
                ("mark", boolean("The one that matters.")),
            ])),
        ),
        ("lowEnd", string("spectrum: what the left end means.")),
        ("highEnd", string("spectrum: what the right end means.")),
        (
            "places",
            array(object(&[
                ("label", string("spectrum: what it is, in a few words.")),
                ("at", number_about("Where it sits, from 0 to 100.")),
                ("note", string("A few words under it, or \"\".")),
                ("mark", boolean("The one that matters.")),
            ])),
        ),
        (
            "xLabels",
            strings("trend: what's along the bottom, one for each value: \"2018\", \"2020\"."),
        ),
        (
            "series",
            array(object(&[
                ("label", string("trend: what the line is.")),
                ("values", array(number())),
                ("mark", boolean("The one that matters.")),
            ])),
        ),
        (
            "unit",
            string("trend: the values' unit, like \"%\", or \"\"."),
        ),
        (
            "branches",
            array(object(&[
                ("id", string("tree: unique in the tree.")),
                ("label", string("A question or an outcome, in a few words.")),
                (
                    "parent",
                    string(
                        "The id of the branch it comes from, earlier in the list; \"\" for the question at the top.",
                    ),
                ),
                (
                    "edge",
                    string("The answer that leads here, \"yes\", or \"\"."),
                ),
                ("mark", boolean("The one that matters.")),
            ])),
        ),
        ("before", version("beforeAfter: the first version.")),
        ("after", version("beforeAfter: the second version.")),
        (
            "figure",
            string(
                "number: the number as shown, \"1 in 3\", \"62%\", \"£4,200\"; under 24 characters.",
            ),
        ),
        (
            "share",
            number_about(
                "number: how much of the whole it is, from 0 to 100, drawn as a bar; leave it out when it isn't a share.",
            ),
        ),
        (
            "notes",
            array(object(&[
                (
                    "line",
                    number_about(
                        "annotated: the number of the line it's about, as kelve numbered it.",
                    ),
                ),
                (
                    "text",
                    string(&format!("What matters about that line. {rich}")),
                ),
            ])),
        ),
        (
            "stages",
            array(object(&[
                ("label", string("funnel: the stage, in a few words.")),
                (
                    "value",
                    number_about("How many reach it; never more than the stage before."),
                ),
                ("display", string("The number as shown.")),
                ("mark", boolean("The stage that matters.")),
            ])),
        ),
    ])
}

/// The guide to blocks a writer is given: every field each type of block
/// carries in its `data`, with what it's for.
pub fn guide() -> String {
    let wide = wide();
    let mut out = String::from(
        "A block's `data` is a JSON object with the fields its type uses, from this list. Each field says which types use it.\n",
    );
    for (name, field) in wide["properties"].as_object().into_iter().flatten() {
        if matches!(name.as_str(), "id" | "type" | "text") {
            continue;
        }
        out.push_str(&format!("- {name}: {}\n", sketch(field)));
    }
    out
}

/// A field's shape and purpose, in a line.
fn sketch(field: &Value) -> String {
    let about = field["description"].as_str().unwrap_or_default();
    let shape = match field["type"].as_str() {
        Some("array") => {
            let items = &field["items"];
            match items["type"].as_str() {
                Some("object") => format!("a list of {}", inner(items)),
                Some("array") => format!(
                    "a list of lists of {}",
                    items["items"]["type"].as_str().unwrap_or("string")
                ),
                Some(t) => format!("a list of {t}s"),
                None => "a list".into(),
            }
        }
        Some("object") => inner(field),
        Some(t) => match field["enum"].as_array() {
            Some(values) => format!(
                "one of {}",
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|v| format!("\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => t.to_string(),
        },
        None => String::new(),
    };
    if about.is_empty() {
        shape
    } else {
        format!("{shape}. {about}")
    }
}

/// An object's fields, as `{name: shape (what), …}`.
fn inner(object: &Value) -> String {
    let fields: Vec<String> = object["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, f)| {
            let about = f["description"].as_str().unwrap_or_default();
            let t = f["type"].as_str().unwrap_or_default();
            if about.is_empty() {
                format!("{name}: {t}")
            } else {
                format!("{name}: {t} ({about})")
            }
        })
        .collect();
    format!("{{{}}}", fields.join(", "))
}

fn object(fields: &[(&str, Value)]) -> Value {
    let mut properties = Map::new();
    for (name, schema) in fields {
        properties.insert((*name).to_string(), schema.clone());
    }
    let required: Vec<&str> = fields.iter().map(|(name, _)| *name).collect();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn number_about(description: &str) -> Value {
    json!({ "type": "number", "description": description })
}

fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn strings(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "string" }, "description": description })
}

fn integer(description: &str) -> Value {
    json!({ "type": "integer", "description": description })
}

fn integers(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "integer" }, "description": description })
}

fn number() -> Value {
    json!({ "type": "number" })
}

fn boolean(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

fn array(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}

fn choices(values: &[&str], description: &str) -> Value {
    json!({ "type": "string", "enum": values, "description": description })
}

fn version(about: &str) -> Value {
    let mut v = object(&[
        ("label", string("What this version is, in a few words.")),
        (
            "excerpt",
            string(
                "The id of an excerpt kelve handed over, to show its real lines; or \"\" to write them.",
            ),
        ),
        (
            "lines",
            integers("With an excerpt: the numbers of its lines to show; empty for all."),
        ),
        (
            "written",
            strings("Without an excerpt: up to 16 lines written to show the idea, as plain text."),
        ),
        (
            "lang",
            string("Without an excerpt: what they're written in, like \"yaml\", or \"\"."),
        ),
    ]);
    v["description"] = json!(about);
    v
}

fn side() -> Value {
    object(&[
        ("title", string("compare: this side's title.")),
        ("points", strings("compare: this side's points, in prose.")),
    ])
}

/// A formula's name as the schema and the API write it.
fn name_of(formula: &Formula) -> &'static str {
    match formula {
        Formula::LoanPayment => "loanPayment",
        Formula::CompoundGrowth => "compoundGrowth",
        Formula::ChanceAtLeast => "chanceAtLeast",
        Formula::AgreementBeyondChance => "agreementBeyondChance",
        Formula::PercentChange => "percentChange",
        Formula::RateTimesTime => "rateTimesTime",
    }
}

/// A model's draft of an entry. Fields it leaves out are empty, so a model
/// without strict structured outputs gets problems to fix, not a failure to
/// read.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Draft {
    pub title: String,
    pub minutes: f64,
    pub blocks: Vec<WideBlock>,
    pub links: Vec<DraftLink>,
    pub claims: Vec<DraftClaim>,
    /// For a fix: the blocks it takes out, each with why.
    pub remove: Vec<DraftRemove>,
    /// For a revision: the claims checked before that it leaves out, by
    /// their numbers as the job gave them, each with why.
    pub drop: Vec<DraftDrop>,
}

/// A block a fix takes out, and why.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct DraftRemove {
    pub id: String,
    pub why: String,
}

/// A checked claim a revision leaves out, by its number, and why.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct DraftDrop {
    pub claim: f64,
    pub why: String,
}

/// A term the prose links, and the idea the model says it leads to. The
/// graph decides which idea that is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DraftLink {
    pub term: String,
    pub idea: String,
    pub about: String,
    pub kind: String,
    pub link: String,
}

impl DraftLink {
    pub fn idea_kind(&self) -> IdeaKind {
        IdeaKind::parse(&self.kind)
    }

    pub fn link_kind(&self) -> LinkKind {
        LinkKind::parse(&self.link)
    }
}

/// One block, with every field any block has; a block uses only its own.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WideBlock {
    pub id: String,
    pub part: String,
    pub fold: String,
    /// For a block an extend adds: the id of the block it goes after, or
    /// "" for first.
    pub follows: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    pub quiet: bool,
    pub items: Vec<String>,
    pub excerpt: String,
    pub lines: Vec<f64>,
    pub marks: Vec<f64>,
    pub layout: String,
    pub nodes: Vec<WideNode>,
    pub edges: Vec<WideEdge>,
    pub steps: Vec<WideStep>,
    pub describe: String,
    pub formula: String,
    pub inputs: Vec<WideInput>,
    pub output: String,
    pub bars: Vec<WideBar>,
    pub caption: String,
    pub head: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub cell_marks: Vec<Vec<f64>>,
    pub left: WideSide,
    pub right: WideSide,
    pub squares: Vec<WideSquare>,
    pub moments: Vec<WideMoment>,
    pub said: Vec<WideSaid>,
    pub lead: String,
    pub actors: Vec<String>,
    pub messages: Vec<WideMessage>,
    pub cards: Vec<WideCard>,
    pub x_low: String,
    pub x_high: String,
    pub y_low: String,
    pub y_high: String,
    pub points: Vec<WidePoint>,
    pub low_end: String,
    pub high_end: String,
    pub places: Vec<WidePlace>,
    pub x_labels: Vec<String>,
    pub series: Vec<WideSeries>,
    pub unit: String,
    pub branches: Vec<WideBranch>,
    pub before: WideVersion,
    pub after: WideVersion,
    pub figure: String,
    pub share: Option<f64>,
    pub notes: Vec<WideNote>,
    pub stages: Vec<WideStage>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideNode {
    pub id: String,
    pub label: String,
    pub note: String,
    pub mark: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideEdge {
    pub from: String,
    pub to: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideStep {
    pub caption: String,
    pub marks: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideInput {
    pub name: String,
    pub label: String,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub value: f64,
    pub unit: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideBar {
    pub label: String,
    pub value: f64,
    pub display: String,
    pub mark: bool,
    pub quiet: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideSide {
    pub title: String,
    pub points: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideSquare {
    pub red: bool,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideMoment {
    pub when: String,
    pub text: String,
    pub mark: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideSaid {
    pub who: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideMessage {
    pub from: String,
    pub to: String,
    pub text: String,
    pub mark: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideCard {
    pub title: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WidePoint {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub mark: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WidePlace {
    pub label: String,
    pub at: f64,
    pub note: String,
    pub mark: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideSeries {
    pub label: String,
    pub values: Vec<f64>,
    pub mark: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideBranch {
    pub id: String,
    pub label: String,
    pub parent: String,
    pub edge: String,
    pub mark: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideVersion {
    pub label: String,
    pub excerpt: String,
    pub lines: Vec<f64>,
    pub written: Vec<String>,
    pub lang: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideNote {
    pub line: f64,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct WideStage {
    pub label: String,
    pub value: f64,
    pub display: String,
    pub mark: bool,
}

/// A factual statement in a draft, and the sources the model says support
/// it, by the numbers kelve handed them over with.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct DraftClaim {
    pub text: String,
    pub sources: Vec<f64>,
    /// What each source it cites says, copied from the page: an agent's
    /// own pages are checked against these by the shared graph.
    pub quotes: Vec<DraftQuote>,
    /// Where the entry makes it: a block's id, and the words of that
    /// block that make it. Empty for a claim a figure makes.
    pub block: String,
    pub words: String,
}

/// The words on a page that support a claim, by the source's number.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct DraftQuote {
    pub source: f64,
    pub text: String,
}

/// A claim in a built entry, with its sources numbered as the entry
/// numbers them. kelve checks each against its sources.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    pub text: String,
    pub sources: Vec<u8>,
    /// The words on each cited page that say it, as the writer copied them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quotes: Vec<Quote>,
    /// Where the entry makes it, when the writer said and its words are
    /// there: the block's id, and the words, without their markup, as the
    /// reader finds them in that block's text.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub block: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub words: String,
}

/// The words on a page that support a claim, by the entry's number for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub source: u8,
    pub text: String,
}

/// The longest quote kept from a page.
const QUOTE: usize = 600;

/// What kelve hands a model to write from, and copies back in by id.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// Real text kelve holds, to be quoted.
    pub excerpts: Vec<ExcerptMaterial>,
    /// The source pack, numbered.
    pub sources: Vec<SourceMaterial>,
}

/// Real text kelve holds: a file, a change, a message.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExcerptMaterial {
    pub id: String,
    /// What it's written in, as kelve tells: see `code::language`.
    #[serde(default)]
    pub lang: Option<String>,
    /// Where it's from, as the excerpt says it: `Dockerfile`, "Sam's
    /// message on 9 October".
    pub from: String,
    /// Each line, with its number in the real thing when it has one.
    pub lines: Vec<(Option<u32>, String)>,
}

impl ExcerptMaterial {
    /// The number the model uses for each line: its real number when every
    /// line has a different one, else its place, counting from 1.
    pub fn keys(&self) -> Vec<u32> {
        let real: Vec<u32> = self.lines.iter().filter_map(|(n, _)| *n).collect();
        let unique = real.iter().collect::<HashSet<_>>().len() == real.len();
        if real.len() == self.lines.len() && unique {
            real
        } else {
            (1..).take(self.lines.len()).collect()
        }
    }

    /// The lines as the model sees them, each with the number it uses.
    pub fn numbered(&self) -> Vec<(u32, &str)> {
        self.keys()
            .into_iter()
            .zip(self.lines.iter().map(|(_, text)| text.as_str()))
            .collect()
    }
}

/// A source in the pack, with the number the model cites it by.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMaterial {
    pub n: u8,
    pub source: Source,
}

/// An entry built from a draft, ready for `check`.
#[derive(Clone, Debug, PartialEq)]
pub struct Built {
    pub title: String,
    pub minutes: u8,
    pub blocks: Vec<Block>,
    /// The sources it cites, numbered from 1 in the order it first cites
    /// them.
    pub sources: Vec<Source>,
    pub claims: Vec<Claim>,
    /// The terms its prose links, each once, and the idea each leads to.
    pub links: Vec<DraftLink>,
}

/// Reads a model's answer as a draft.
pub fn parse(value: &Value) -> Result<Draft, Problem> {
    let mut value = value.clone();
    if let Some(blocks) = value.get_mut("blocks").and_then(Value::as_array_mut) {
        for block in blocks {
            unpack(block)?;
        }
    }
    shown_words(&mut value);
    Draft::deserialize(&value).map_err(|e| Problem {
        block: None,
        rule: Rule::Shape,
        says: format!("The draft isn't in the shape kelve asked for: {e}."),
    })
}

/// Links written with other words shown, `[[deadlock|deadlocks]]`, as a
/// writer often does: the words shown are linked, `[[deadlocks]]`, to the
/// idea the term listed leads to.
fn shown_words(value: &mut Value) {
    fn walk(v: &mut Value, pairs: &mut Vec<(String, String)>) {
        match v {
            Value::String(text) if text.contains("[[") && text.contains('|') => {
                let mut out = String::with_capacity(text.len());
                let mut rest = text.as_str();
                while let Some(at) = rest.find("[[") {
                    out.push_str(&rest[..at]);
                    let after = &rest[at + 2..];
                    match after.find("]]") {
                        Some(end) if !after[..end].contains("[[") => {
                            let inner = &after[..end];
                            match inner.split_once('|') {
                                Some((term, shown))
                                    if !term.trim().is_empty() && !shown.trim().is_empty() =>
                                {
                                    pairs.push((term.trim().to_string(), shown.trim().to_string()));
                                    out.push_str(&format!("[[{}]]", shown.trim()));
                                }
                                _ => out.push_str(&format!("[[{inner}]]")),
                            }
                            rest = &after[end + 2..];
                        }
                        _ => {
                            out.push_str("[[");
                            rest = after;
                        }
                    }
                }
                out.push_str(rest);
                *text = out;
            }
            Value::Array(items) => items.iter_mut().for_each(|i| walk(i, pairs)),
            Value::Object(fields) => fields.values_mut().for_each(|f| walk(f, pairs)),
            _ => {}
        }
    }
    let mut pairs = Vec::new();
    if let Some(blocks) = value.get_mut("blocks") {
        walk(blocks, &mut pairs);
    }
    let Some(links) = value.get_mut("links").and_then(Value::as_array_mut) else {
        return;
    };
    let term_of = |l: &Value| l["term"].as_str().unwrap_or_default().to_lowercase();
    for (term, shown) in pairs {
        if links.iter().any(|l| term_of(l) == shown.to_lowercase()) {
            continue;
        }
        if let Some(listed) = links.iter().find(|l| term_of(l) == term.to_lowercase()) {
            let mut link = listed.clone();
            link["term"] = Value::String(shown);
            links.push(link);
        }
    }
}

/// A block's data, written as JSON. A model sometimes closes it once too
/// often: brackets left over after the whole value are let go, and
/// anything else after it is a problem.
fn data_of(text: &str) -> Result<Value, serde_json::Error> {
    let mut values = serde_json::Deserializer::from_str(text).into_iter::<Value>();
    let first = values.next().unwrap_or_else(|| serde_json::from_str(text));
    let value = first?;
    let rest = &text[values.byte_offset()..];
    if rest
        .chars()
        .all(|c| c.is_whitespace() || matches!(c, '}' | ']'))
    {
        Ok(value)
    } else {
        serde_json::from_str(text)
    }
}

/// Reads a block's `data` into the block: every field its type has. The
/// fields the block carries itself stay as they are.
fn unpack(block: &mut Value) -> Result<(), Problem> {
    let Some(object) = block.as_object_mut() else {
        return Ok(());
    };
    let id = object.get("id").and_then(Value::as_str).map(str::to_string);
    let data = match object.remove("data") {
        Some(Value::String(text)) if text.trim().is_empty() => return Ok(()),
        Some(Value::String(text)) => data_of(&text).map_err(|e| Problem {
            block: id.clone(),
            rule: Rule::Shape,
            says: format!(
                "Its data isn't a JSON object: {e}. Write it as one JSON object, with nothing after it."
            ),
        })?,
        Some(other) => other,
        None => return Ok(()),
    };
    let Value::Object(mut fields) = data else {
        return Err(Problem {
            block: id,
            rule: Rule::Shape,
            says: "Its data isn't a JSON object.".into(),
        });
    };
    // What a figure shows, in words, is its `describe`; a model often
    // calls it its description.
    if !fields.contains_key("describe")
        && let Some(said) = fields.remove("description")
    {
        fields.insert("describe".into(), said);
    }
    for (name, field) in fields {
        if !matches!(name.as_str(), "id" | "type" | "text") || !object.contains_key(&name) {
            object.insert(name, field);
        }
    }
    Ok(())
}

/// Whether a built entry rests on the pages it was given: one that cites
/// none of them is written from the model's own knowledge alone.
pub fn grounded(built: &Built, material: &Material) -> Option<Problem> {
    (!material.sources.is_empty() && built.sources.is_empty()).then(|| Problem {
        block: None,
        rule: Rule::Grounding,
        says: "It cites none of the pages it was given. Ground it in them: cite, as ^1, each page where it says something the entry says.".into(),
    })
}

/// A claim's key (`claims::key`), from its words and the words it quotes
/// from each page, by the pages' addresses in `sources`.
pub fn claim_key(claim: &Claim, sources: &[Source]) -> String {
    let pages: Vec<(&str, &str)> = claim
        .quotes
        .iter()
        .filter_map(|q| {
            let url = sources.iter().find(|s| s.n == q.source)?.url.as_deref()?;
            Some((url, q.text.as_str()))
        })
        .collect();
    super::claims::key(&claim.text, &pages)
}

/// A revision of an entry, built from an agent's answer to a fix or an
/// extend: what it does to the entry's blocks, and the entry it makes.
#[derive(Clone, Debug, PartialEq)]
pub struct Revised {
    pub ops: Vec<Op>,
    /// The entry's blocks, with the revision applied.
    pub blocks: Vec<Block>,
    /// The entry's sources, then any page the revision adds.
    pub sources: Vec<Source>,
    /// Every claim of the revised entry: the entry's own, but those in
    /// blocks it changed or that it drops, then the revision's.
    pub claims: Vec<Claim>,
    /// The terms the revision's blocks link.
    pub links: Vec<DraftLink>,
    /// The entry's checked claims it drops, by their places in its claims,
    /// each with why.
    pub dropped: Vec<(usize, String)>,
}

/// Builds a fix or an extend of `entry` from an agent's answer: `draft`,
/// and `pages`, the pages it lists. The entry's sources keep their
/// numbers; a page it lists under one of them must be that page. A fix
/// gives blocks the entry has, and removes some; an extend gives new ones.
/// `claims` are the entry's own, each with where it's made; `faulted`,
/// the places among them of those its checks found wrong, which may go
/// without a word. Any other claim a changed block made must be given
/// again, word for word, or dropped with why.
pub fn build_revision(
    kind: RevisionKind,
    draft: &Draft,
    pages: &Material,
    entry: &super::Entry,
    claims: &[Claim],
    faulted: &[usize],
) -> Result<Revised, Vec<Problem>> {
    let mut c = Checker::new(0);
    let kept = u8::try_from(entry.sources.len()).unwrap_or(u8::MAX);
    let mut material = Material {
        excerpts: Vec::new(),
        sources: entry
            .sources
            .iter()
            .map(|s| SourceMaterial {
                n: s.n,
                source: s.clone(),
            })
            .collect(),
    };
    for page in &pages.sources {
        match entry.sources.iter().find(|s| s.n == page.n) {
            Some(own) if own.url == page.source.url => {}
            Some(_) => c.say(
                Rule::Pointer,
                format!(
                    "Source {} is the entry's own, at another address. Number a new page after {kept}.",
                    page.n
                ),
            ),
            None => material.sources.push(page.clone()),
        }
    }
    let built = match build_onto(draft, &material, kept) {
        Ok(b) => b,
        Err(mut problems) => {
            c.problems.append(&mut problems);
            return Err(c.problems);
        }
    };
    // A block given again as it was changes only if a claim it makes does:
    // a fix may quote a page that says what its words already say.
    let had_claims: Vec<String> = claims
        .iter()
        .map(|cl| claim_key(cl, &entry.sources))
        .collect();
    let claims_changed = |id: &str| {
        built
            .claims
            .iter()
            .filter(|cl| cl.block == id)
            .any(|cl| !had_claims.contains(&claim_key(cl, &built.sources)))
    };
    let mut ops = Vec::new();
    for block in &built.blocks {
        let was = entry.blocks.iter().find(|b| b.id == block.id);
        let had = was.is_some();
        match kind {
            // Given again as it was: nothing changed.
            RevisionKind::Fix if was == Some(block) && !claims_changed(&block.id) => {}
            RevisionKind::Fix if had => ops.push(Op::Replace {
                block: block.clone(),
            }),
            RevisionKind::Fix => c.say(
                Rule::Pointer,
                format!(
                    "{} isn't one of the entry's blocks. A fix gives again only blocks found wrong, by their ids.",
                    block.id
                ),
            ),
            RevisionKind::Extend if had => c.say(
                Rule::Pointer,
                format!(
                    "The entry already has {}. Give each new block an id no block has.",
                    block.id
                ),
            ),
            _ => {
                let follows = draft
                    .blocks
                    .iter()
                    .find(|w| w.id == block.id)
                    .map(|w| w.follows.trim().to_string())
                    .filter(|f| !f.is_empty());
                ops.push(Op::Add {
                    after: follows,
                    block: block.clone(),
                });
            }
        }
    }
    if kind == RevisionKind::Fix {
        for r in &draft.remove {
            ops.push(Op::Remove {
                id: r.id.trim().to_string(),
                why: r.why.trim().to_string(),
            });
        }
    }
    if ops.is_empty() {
        c.say(
            Rule::Shape,
            match kind {
                RevisionKind::Fix => {
                    "It changes nothing. Give again each block found wrong, fixed."
                }
                _ => "It adds nothing. Add what was asked, as new blocks.",
            },
        );
    }
    let blocks = match super::revise::apply(&entry.blocks, &ops) {
        Ok(b) => b,
        Err(says) => {
            c.say(Rule::Pointer, says);
            return Err(c.problems);
        }
    };
    super::check::parts(&mut c, &blocks);

    // The claims: the entry's own, but those its changed blocks made, then
    // the revision's.
    let changed: Vec<&str> = ops
        .iter()
        .filter(|op| !matches!(op, Op::Add { .. }))
        .map(Op::id)
        .collect();
    let mut dropped = Vec::new();
    for d in &draft.drop {
        let at = whole(d.claim)
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| n.checked_sub(1))
            .filter(|i| *i < claims.len());
        match at {
            Some(i) if !d.why.trim().is_empty() => dropped.push((i, d.why.trim().to_string())),
            Some(i) => c.say(
                Rule::Shape,
                format!("It drops claim {} without saying why.", i + 1),
            ),
            None => c.say(
                Rule::Pointer,
                format!("It drops claim {}, which the entry hasn't got.", d.claim),
            ),
        }
    }
    let gone = |i: usize| dropped.iter().any(|(d, _)| *d == i) || faulted.contains(&i);
    let new_keys: Vec<String> = built
        .claims
        .iter()
        .map(|cl| claim_key(cl, &built.sources))
        .collect();
    let mut out: Vec<Claim> = Vec::new();
    for (i, claim) in claims.iter().enumerate() {
        let in_changed = !claim.block.is_empty() && changed.contains(&claim.block.as_str());
        if gone(i) {
            continue;
        }
        if in_changed {
            if !new_keys.contains(&claim_key(claim, &entry.sources)) {
                c.say(
                    Rule::Shape,
                    format!(
                        "Claim {} was checked against its page, and its block changed. Give it again, word for word, or say in drop why it goes.",
                        i + 1
                    ),
                );
            }
            continue;
        }
        out.push(claim.clone());
    }
    for (claim, key) in built.claims.iter().zip(&new_keys) {
        if !out.iter().any(|o| &claim_key(o, &built.sources) == key) {
            out.push(claim.clone());
        }
    }
    if !c.problems.is_empty() {
        return Err(c.problems);
    }
    Ok(Revised {
        ops,
        blocks,
        sources: built.sources,
        claims: out,
        links: built.links,
        dropped,
    })
}

/// Builds an entry from a draft and the material it was written from.
/// Every quoted line is copied from `material`, and every source is kept
/// from it and numbered afresh. Anything the draft points at that isn't
/// there, and anything that can't become a block, is a problem.
pub fn build(draft: &Draft, material: &Material) -> Result<Built, Vec<Problem>> {
    build_with(draft, material, 0, true)
}

/// Builds the blocks a revision adds or replaces, for an entry with
/// `kept` sources already: those keep their numbers, 1 to `kept`, so the
/// blocks left as they were still cite them rightly, and any page the
/// revision adds comes after them. Its title and minutes are the entry's
/// own, so a draft of a revision leaves them out; the whole entry's parts
/// are checked once the revision is applied (`check`).
pub fn build_onto(draft: &Draft, material: &Material, kept: u8) -> Result<Built, Vec<Problem>> {
    build_with(draft, material, kept, false)
}

fn build_with(
    draft: &Draft,
    material: &Material,
    kept: u8,
    entire: bool,
) -> Result<Built, Vec<Problem>> {
    let mut b = Builder {
        c: Checker::new(0),
        material,
        order: (1..=kept).collect(),
    };
    // An excerpt that points at nothing has nothing to quote: it's left
    // out, not worth a revision. One that points at an excerpt kelve didn't
    // hand over is a problem.
    let blocks: Vec<Block> = draft
        .blocks
        .iter()
        .filter(|w| {
            !(matches!(w.kind.as_str(), "excerpt" | "annotated") && w.excerpt.trim().is_empty())
        })
        .filter_map(|w| b.block(w))
        .collect();
    b.c.at(None);
    // The parts are checked once every block is built: one left out moves
    // the rest.
    if entire && b.c.problems.is_empty() {
        if blocks.iter().any(|b| b.part.is_none()) {
            b.c.say(
                Rule::Shape,
                format!(
                    "Give every block its part, the lead first: {}.",
                    PARTS.join(", ")
                ),
            );
        } else {
            super::check::parts(&mut b.c, &blocks);
        }
    }

    if entire && draft.claims.is_empty() {
        b.c.say(
            Rule::Shape,
            "The entry lists no claims. List each factual statement it makes, with the sources behind it.",
        );
    }
    let mut claims = Vec::new();
    for (i, claim) in draft.claims.iter().enumerate() {
        let mut built = b.claim(i + 1, claim);
        // Where it's made: the block, when the entry has it, which is what
        // a later revision keeps or changes it with; and its words, only
        // when they're there, since they're shown, not checked.
        match place_claim(&blocks, &built.block, &built.words) {
            Some(words) => built.words = words,
            None => {
                if !blocks.iter().any(|b| b.id == built.block) {
                    built.block.clear();
                }
                built.words.clear();
            }
        }
        claims.push(built);
    }
    let title = draft.title.trim().to_string();

    let sources: Vec<Source> = b
        .order
        .iter()
        .zip(1u8..)
        .filter_map(|(old, new)| {
            let found = material.sources.iter().find(|s| s.n == *old)?;
            Some(Source {
                n: new,
                ..found.source.clone()
            })
        })
        .collect();

    let mut c = Checker::new(sources.len());
    if entire {
        c.plain("The title", &title, TITLE);
    }
    let minutes = if entire {
        whole(draft.minutes)
            .filter(|m| (1..=60).contains(m))
            .and_then(|m| u8::try_from(m).ok())
    } else {
        Some(0)
    };
    if minutes.is_none() {
        c.say(
            Rule::Length,
            format!(
                "The entry takes {} minutes to read. It should be a whole number from 1 to 60.",
                draft.minutes
            ),
        );
    }

    let links = links(&mut c, draft, &blocks);

    let mut problems = b.c.problems;
    problems.append(&mut c.problems);
    match minutes {
        Some(minutes) if problems.is_empty() => Ok(Built {
            title,
            minutes,
            blocks,
            sources,
            claims,
            links,
        }),
        _ => Err(problems),
    }
}

/// The links a draft lists, each sound and once, in the order its prose
/// links them. A link its prose never uses is left out, not worth a
/// revision; `check` finds any term the prose links that isn't listed.
fn links(c: &mut Checker, draft: &Draft, blocks: &[Block]) -> Vec<DraftLink> {
    let written = linked_terms(blocks);
    let mut out: Vec<DraftLink> = Vec::new();
    for term in &written {
        let Some(link) = draft
            .links
            .iter()
            .find(|l| l.term.trim().eq_ignore_ascii_case(term))
        else {
            continue;
        };
        let Some(idea) = crate::ideas::tidy(&link.idea) else {
            c.say(
                Rule::Shape,
                format!("The link [[{term}]] doesn't name the idea it leads to, in a few words."),
            );
            continue;
        };
        c.plain(&format!("What [[{term}]] is"), &link.about, ABOUT);
        out.push(DraftLink {
            term: term.clone(),
            idea,
            about: link.about.trim().to_string(),
            kind: link.idea_kind().as_str().to_string(),
            link: link.link_kind().as_str().to_string(),
        });
    }
    out
}

struct Builder<'a> {
    c: Checker,
    material: &'a Material,
    /// The material's numbers of the sources cited so far, in order.
    order: Vec<u8>,
}

impl<'a> Builder<'a> {
    /// A source cited by its number in the material: its number in the
    /// entry, if the material has it.
    fn cite(&mut self, old: u64) -> Option<u8> {
        let old = u8::try_from(old).ok()?;
        if !self.material.sources.iter().any(|s| s.n == old) {
            return None;
        }
        let at = match self.order.iter().position(|n| *n == old) {
            Some(at) => at,
            None => {
                self.order.push(old);
                self.order.len() - 1
            }
        };
        u8::try_from(at + 1).ok()
    }

    /// Prose with each `^n` outside code renumbered as the entry numbers its
    /// sources. A source the material doesn't have is a problem, and left
    /// as it was.
    fn renumber(&mut self, text: &str) -> String {
        let pieces = spans(text).unwrap_or_else(|| vec![Span::Text(text)]);
        let mut out = String::with_capacity(text.len());
        for piece in pieces {
            match piece {
                Span::Code(code) => out.push_str(code),
                Span::Text(prose) => {
                    let mut last = 0;
                    for cite in super::check::CITE.captures_iter(prose) {
                        let (Some(all), Some(number)) = (cite.get(0), cite.get(1)) else {
                            continue;
                        };
                        out.push_str(&prose[last..all.start()]);
                        let new = number
                            .as_str()
                            .parse::<u64>()
                            .ok()
                            .and_then(|n| self.cite(n));
                        match new {
                            Some(new) => out.push_str(&format!("^{new}")),
                            None => {
                                self.c.say(
                                    Rule::Pointer,
                                    format!(
                                        "It cites {}, but kelve handed over no source {}.",
                                        all.as_str(),
                                        number.as_str()
                                    ),
                                );
                                out.push_str(all.as_str());
                            }
                        }
                        last = all.end();
                    }
                    out.push_str(&prose[last..]);
                }
            }
        }
        out
    }

    fn renumber_all(&mut self, texts: &[String]) -> Vec<String> {
        texts.iter().map(|t| self.renumber(t)).collect()
    }

    fn claim(&mut self, i: usize, claim: &DraftClaim) -> Claim {
        let what = format!("Claim {i}");
        if claim.text.trim().is_empty() {
            self.c.say(Rule::Shape, format!("{what} is empty."));
        }
        let length = claim.text.chars().count();
        if length > CLAIM {
            self.c.say(
                Rule::Length,
                format!("{what} is {length} characters long. Keep it under {CLAIM}."),
            );
        }
        let mut sources = Vec::new();
        for n in &claim.sources {
            match whole(*n).and_then(|n| self.cite(u64::from(n))) {
                Some(new) => sources.push(new),
                None => self.c.say(
                    Rule::Pointer,
                    format!("{what} points at source {n}, which kelve didn't hand over."),
                ),
            }
        }
        let mut quotes = Vec::new();
        for q in &claim.quotes {
            let text = q.text.trim();
            if text.is_empty() {
                continue;
            }
            match whole(q.source).and_then(|n| self.cite(u64::from(n))) {
                Some(source) => quotes.push(Quote {
                    source,
                    text: clip(text, QUOTE),
                }),
                None => self.c.say(
                    Rule::Pointer,
                    format!("{what} quotes source {}, which it doesn't list.", q.source),
                ),
            }
        }
        Claim {
            text: claim.text.trim().to_string(),
            sources,
            quotes,
            block: claim.block.trim().to_string(),
            words: claim.words.trim().to_string(),
        }
    }

    fn block(&mut self, w: &WideBlock) -> Option<Block> {
        self.c.at(Some(&w.id));
        let body = match w.kind.as_str() {
            "paragraph" => Body::Paragraph {
                text: self.renumber(&w.text),
                quiet: w.quiet,
            },
            "heading" => Body::Heading {
                text: w.text.clone(),
            },
            "list" => Body::List {
                items: self.renumber_all(&w.items),
            },
            "excerpt" => self.excerpt(w)?,
            "diagram" => self.diagram(w)?,
            "explorable" => self.explorable(w)?,
            "bars" => Body::Bars {
                rows: w
                    .bars
                    .iter()
                    .map(|b| Bar {
                        label: b.label.clone(),
                        value: b.value,
                        display: b.display.clone(),
                        mark: b.mark,
                        quiet: b.quiet,
                    })
                    .collect(),
                caption: some(&w.caption),
            },
            "table" => self.table(w)?,
            "compare" => Body::Compare {
                left: Side {
                    title: w.left.title.clone(),
                    points: self.renumber_all(&w.left.points),
                },
                right: Side {
                    title: w.right.title.clone(),
                    points: self.renumber_all(&w.right.points),
                },
            },
            "squares" => Body::Squares {
                rows: w
                    .squares
                    .iter()
                    .map(|s| Square {
                        red: s.red,
                        text: self.renumber(&s.text),
                    })
                    .collect(),
            },
            "timeline" => self.timeline(w)?,
            "conversation" => Body::Conversation {
                lines: w
                    .said
                    .iter()
                    .map(|s| Said {
                        who: s.who.clone(),
                        text: self.renumber(&s.text),
                    })
                    .collect(),
            },
            "answer" => Body::Answer {
                lead: w.lead.clone(),
                text: self.renumber(&w.text),
            },
            "sequence" => Body::Sequence {
                actors: w.actors.clone(),
                messages: w
                    .messages
                    .iter()
                    .map(|m| SequenceMessage {
                        from: m.from.clone(),
                        to: m.to.clone(),
                        text: m.text.clone(),
                        mark: m.mark,
                    })
                    .collect(),
                describe: w.describe.clone(),
            },
            "steps" => Body::Steps {
                cards: w
                    .cards
                    .iter()
                    .map(|c| StepCard {
                        title: c.title.clone(),
                        text: self.renumber(&c.text),
                    })
                    .collect(),
            },
            "quadrant" => Body::Quadrant {
                x: QuadrantAxis {
                    low: w.x_low.clone(),
                    high: w.x_high.clone(),
                },
                y: QuadrantAxis {
                    low: w.y_low.clone(),
                    high: w.y_high.clone(),
                },
                points: w
                    .points
                    .iter()
                    .map(|p| QuadrantPoint {
                        label: p.label.clone(),
                        x: p.x,
                        y: p.y,
                        mark: p.mark,
                    })
                    .collect(),
                describe: w.describe.clone(),
            },
            "spectrum" => Body::Spectrum {
                low: w.low_end.clone(),
                high: w.high_end.clone(),
                points: w
                    .places
                    .iter()
                    .map(|p| SpectrumPoint {
                        label: p.label.clone(),
                        at: p.at,
                        note: some(&p.note),
                        mark: p.mark,
                    })
                    .collect(),
                describe: w.describe.clone(),
            },
            "trend" => Body::Trend {
                labels: w.x_labels.clone(),
                series: w
                    .series
                    .iter()
                    .map(|s| TrendSeries {
                        label: s.label.clone(),
                        values: s.values.clone(),
                        mark: s.mark,
                    })
                    .collect(),
                unit: some(&w.unit),
                describe: w.describe.clone(),
            },
            "tree" => Body::Tree {
                branches: w
                    .branches
                    .iter()
                    .map(|b| TreeBranch {
                        id: b.id.clone(),
                        label: b.label.clone(),
                        parent: some(&b.parent),
                        edge: some(&b.edge),
                        mark: b.mark,
                    })
                    .collect(),
                describe: w.describe.clone(),
            },
            "beforeAfter" => Body::BeforeAfter {
                before: self.version(&w.before)?,
                after: self.version(&w.after)?,
                describe: w.describe.clone(),
            },
            "number" => Body::Number {
                figure: w.figure.clone(),
                text: self.renumber(&w.text),
                share: w.share,
            },
            "annotated" => self.annotated(w)?,
            "funnel" => Body::Funnel {
                stages: w
                    .stages
                    .iter()
                    .map(|s| FunnelStage {
                        label: s.label.clone(),
                        value: s.value,
                        display: s.display.clone(),
                        mark: s.mark,
                    })
                    .collect(),
                caption: some(&w.caption),
            },
            other => {
                self.c.say(
                    Rule::Shape,
                    format!(
                        "It's a {other:?} block, which isn't one of kelve's: {}.",
                        TYPES.join(", ")
                    ),
                );
                return None;
            }
        };
        let part = match w.part.trim() {
            "" => None,
            word => {
                let part = Part::parse(word);
                if part.is_none() {
                    self.c.say(
                        Rule::Shape,
                        format!("Its part, {word:?}, isn't one of: {}.", PARTS.join(", ")),
                    );
                }
                part
            }
        };
        let fold = match w.fold.trim() {
            "" => None,
            word => {
                let fold = Fold::parse(word);
                if fold.is_none() {
                    self.c.say(
                        Rule::Shape,
                        format!(
                            "Its fold, {word:?}, isn't one of: {}, or \"\" for none.",
                            FOLDS[1..].join(", ")
                        ),
                    );
                }
                fold
            }
        };
        Some(Block {
            id: w.id.clone(),
            part,
            fold,
            body,
        })
    }

    /// The excerpt the draft points at, if kelve handed it over.
    fn found(&mut self, id: &str, what: &str) -> Option<&'a ExcerptMaterial> {
        let material = self.material;
        let found = material.excerpts.iter().find(|e| e.id == id);
        if found.is_none() {
            self.c.say(
                Rule::Pointer,
                format!("{what} points at {id:?}, which kelve didn't hand over."),
            );
        }
        found
    }

    /// Where the lines the draft shows are among an excerpt's lines, in
    /// order: every line when it names none.
    fn shown(&mut self, found: &ExcerptMaterial, numbers: &[f64], what: &str) -> Vec<usize> {
        let keys = found.keys();
        let mut shown: Vec<usize> = if numbers.is_empty() {
            (0..found.lines.len()).collect()
        } else {
            let mut shown = Vec::new();
            for n in numbers {
                match place(&keys, *n) {
                    Some(at) => shown.push(at),
                    None => self.c.say(
                        Rule::Pointer,
                        format!("{what} shows line {n}, which {} doesn't have.", found.id),
                    ),
                }
            }
            shown
        };
        shown.sort_unstable();
        shown.dedup();
        shown
    }

    /// The lines the draft points at, copied from the material, with a
    /// faint `…` where lines are skipped.
    fn excerpt(&mut self, w: &WideBlock) -> Option<Body> {
        let found = self.found(&w.excerpt, "The excerpt")?;
        let shown = self.shown(found, &w.lines, "The excerpt");
        let keys = found.keys();
        let mut marked = HashSet::new();
        for n in &w.marks {
            match place(&keys, *n).filter(|at| shown.contains(at)) {
                Some(at) => {
                    marked.insert(at);
                }
                None => self.c.say(
                    Rule::Pointer,
                    format!("The excerpt marks line {n}, which isn't among the lines it shows."),
                ),
            }
        }
        Some(Body::Excerpt {
            from: found.from.clone(),
            lang: found.lang.clone(),
            lines: copied(found, &shown, &marked).0,
        })
    }

    /// An excerpt with notes pinned to its lines: each noted line is
    /// marked, and each note says where its line is among those shown.
    fn annotated(&mut self, w: &WideBlock) -> Option<Body> {
        let found = self.found(&w.excerpt, "The annotated excerpt")?;
        let shown = self.shown(found, &w.lines, "The annotated excerpt");
        let keys = found.keys();
        let mut pinned = Vec::new();
        for note in &w.notes {
            match place(&keys, note.line).filter(|at| shown.contains(at)) {
                Some(at) => pinned.push((at, note)),
                None => self.c.say(
                    Rule::Pointer,
                    format!(
                        "A note is about line {}, which isn't among the lines it shows.",
                        note.line
                    ),
                ),
            }
        }
        let marked: HashSet<usize> = pinned.iter().map(|(at, _)| *at).collect();
        let (lines, places) = copied(found, &shown, &marked);
        let mut notes = Vec::new();
        for (at, note) in pinned {
            let line = places
                .iter()
                .position(|p| *p == Some(at))
                .and_then(|i| u16::try_from(i).ok())?;
            notes.push(CodeNote {
                line,
                text: self.renumber(&note.text),
            });
        }
        notes.sort_by_key(|n| n.line);
        Some(Body::Annotated {
            from: found.from.clone(),
            lang: found.lang.clone(),
            lines,
            notes,
        })
    }

    /// One side of a before and after: the real lines of an excerpt, or
    /// lines written to show the idea.
    fn version(&mut self, v: &WideVersion) -> Option<Version> {
        if v.excerpt.trim().is_empty() {
            let lang = Some(v.lang.trim())
                .filter(|l| crate::code::LANGUAGES.contains(l))
                .map(str::to_string);
            return Some(Version {
                label: v.label.clone(),
                from: None,
                lang,
                lines: v.written.iter().map(|l| clip(l, EXCERPT_LINE)).collect(),
            });
        }
        let found = self.found(&v.excerpt, "The before and after")?;
        let shown = self.shown(found, &v.lines, "The before and after");
        let lines = copied(found, &shown, &HashSet::new())
            .0
            .into_iter()
            .map(|l| l.text)
            .collect();
        Some(Version {
            label: v.label.clone(),
            from: Some(found.from.clone()),
            lang: found.lang.clone(),
            lines,
        })
    }

    fn diagram(&mut self, w: &WideBlock) -> Option<Body> {
        let layout = match w.layout.as_str() {
            // No layout given: a row, the plainest. Not worth a revision.
            "row" | "" => Layout::Row,
            "cycle" => Layout::Cycle,
            "stack" => Layout::Stack,
            "timeline" => Layout::Timeline,
            other => {
                self.c.say(
                    Rule::Shape,
                    format!("The diagram's layout is {other:?}. It should be row, cycle, stack or timeline."),
                );
                return None;
            }
        };
        Some(Body::Diagram {
            layout,
            nodes: w
                .nodes
                .iter()
                .map(|n| Node {
                    id: n.id.clone(),
                    label: n.label.clone(),
                    note: some(&n.note),
                    mark: n.mark,
                })
                .collect(),
            edges: w
                .edges
                .iter()
                .map(|e| Edge {
                    from: e.from.clone(),
                    to: e.to.clone(),
                    label: some(&e.label),
                })
                .collect(),
            steps: w
                .steps
                .iter()
                .map(|s| Frame {
                    caption: s.caption.clone(),
                    marks: s.marks.clone(),
                })
                .collect(),
            describe: w.describe.clone(),
        })
    }

    fn explorable(&mut self, w: &WideBlock) -> Option<Body> {
        let Some(formula) = FORMULAS.iter().find(|f| name_of(f) == w.formula) else {
            let names: Vec<&str> = FORMULAS.iter().map(name_of).collect();
            self.c.say(
                Rule::Shape,
                format!(
                    "The explorable's formula is {:?}. It should be one of {}.",
                    w.formula,
                    names.join(", ")
                ),
            );
            return None;
        };
        Some(Body::Explorable {
            formula: *formula,
            inputs: w
                .inputs
                .iter()
                .map(|i| Input {
                    name: i.name.clone(),
                    label: i.label.clone(),
                    min: i.min,
                    max: i.max,
                    step: i.step,
                    value: i.value,
                    unit: some(&i.unit),
                })
                .collect(),
            output: w.output.clone(),
            describe: w.describe.clone(),
        })
    }

    fn table(&mut self, w: &WideBlock) -> Option<Body> {
        let mut marks = Vec::new();
        for mark in &w.cell_marks {
            let cell = match mark.as_slice() {
                [r, c] => whole(*r)
                    .zip(whole(*c))
                    .and_then(|(r, c)| Some([u16::try_from(r).ok()?, u16::try_from(c).ok()?])),
                _ => None,
            };
            // A mark only colours a cell: one that isn't [row, column] is
            // left out, not worth a revision.
            if let Some(cell) = cell {
                marks.push(cell);
            }
        }
        Some(Body::Table {
            head: w.head.clone(),
            rows: w.rows.clone(),
            marks,
        })
    }

    fn timeline(&mut self, w: &WideBlock) -> Option<Body> {
        let mut rows = Vec::new();
        for m in &w.moments {
            let mark = match m.mark.as_str() {
                "" | "none" => Mark::None,
                "red" => Mark::Red,
                // A label where the mark goes ("key", "hardest to undo")
                // means it's marked: ink, the quieter mark. Not worth a
                // revision.
                _ => Mark::Ink,
            };
            rows.push(Moment {
                when: m.when.clone(),
                text: m.text.clone(),
                mark,
            });
        }
        Some(Body::Timeline { rows })
    }
}

/// Where a line the model numbered is among an excerpt's lines.
fn place(keys: &[u32], n: f64) -> Option<usize> {
    whole(n).and_then(|n| keys.iter().position(|k| *k == n))
}

/// The lines shown, copied from the excerpt, with a faint `…` where lines
/// are skipped; and for each line, where it is in the excerpt, if it's
/// one of its lines.
fn copied(
    found: &ExcerptMaterial,
    shown: &[usize],
    marked: &HashSet<usize>,
) -> (Vec<Line>, Vec<Option<usize>>) {
    let mut lines = Vec::with_capacity(shown.len());
    let mut places = Vec::with_capacity(shown.len());
    let mut last: Option<usize> = None;
    for &at in shown {
        if last.is_some_and(|last| at > last + 1) {
            lines.push(Line {
                n: None,
                text: "…".to_string(),
                mark: false,
                dim: true,
            });
            places.push(None);
        }
        let (n, text) = &found.lines[at];
        lines.push(Line {
            n: *n,
            text: clip(text, EXCERPT_LINE),
            mark: marked.contains(&at),
            dim: false,
        });
        places.push(Some(at));
        last = Some(at);
    }
    (lines, places)
}

/// `""` as nothing.
fn some(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.to_string())
}

/// A number the model gave as a whole number, if it is one.
fn whole(n: f64) -> Option<u32> {
    (n.is_finite() && n.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&n))
        .then_some(n as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::check::check;

    fn material() -> Material {
        let source = |n: u8, title: &str| SourceMaterial {
            n,
            source: Source {
                n,
                title: title.into(),
                publisher: "Docker".into(),
                url: Some(format!("https://docs.docker.com/{n}")),
                published: None,
                preprint: false,
                archived: None,
            },
        };
        Material {
            excerpts: vec![ExcerptMaterial {
                lang: None,
                id: "e1".into(),
                from: "Dockerfile".into(),
                lines: vec![
                    (
                        Some(35),
                        "    && useradd --system --home-dir /data kelve \\".into(),
                    ),
                    (Some(36), "    && mkdir /data && chown kelve /data".into()),
                    (Some(37), "".into()),
                    (Some(39), "USER kelve".into()),
                    (Some(44), "VOLUME /data".into()),
                ],
            }],
            sources: vec![
                source(1, "Volumes"),
                source(2, "Bind mounts"),
                source(3, "tmpfs mounts"),
            ],
        }
    }

    /// A paragraph that builds and checks, for tests about something else.
    fn plain(id: &str) -> WideBlock {
        WideBlock {
            text: "Plain words.".into(),
            ..wide(id, "paragraph")
        }
    }

    fn wide(id: &str, kind: &str) -> WideBlock {
        WideBlock {
            id: id.into(),
            kind: kind.into(),
            ..WideBlock::default()
        }
    }

    /// A draft of these blocks, each in a part: the first the lead, the
    /// rest how it works, unless a test says otherwise.
    fn draft(mut blocks: Vec<WideBlock>) -> Draft {
        for (i, b) in blocks.iter_mut().enumerate() {
            if b.part.is_empty() {
                b.part = if i == 0 { "lead" } else { "how" }.into();
            }
        }
        Draft {
            title: "Your own Dockerfile already knew this".into(),
            minutes: 1.0,
            blocks,
            claims: vec![DraftClaim {
                text: "A named volume starts with what the image has at its path.".into(),
                sources: vec![],
                ..Default::default()
            }],
            ..Draft::default()
        }
    }

    /// An entry the graph serves, with two claims, the second in b2.
    fn served() -> (super::super::Entry, Vec<Claim>) {
        let source = material().sources[0].source.clone();
        let p = |id: &str, part: Part, text: &str| Block {
            id: id.into(),
            part: Some(part),
            fold: None,
            body: Body::Paragraph {
                text: text.into(),
                quiet: false,
            },
        };
        let entry = super::super::Entry {
            idea: crate::ideas::IdeaRef {
                id: "kelve:i1".into(),
                name: "Volumes".into(),
            },
            title: "Volumes".into(),
            question: String::new(),
            minutes: 2,
            made_at: crate::time::Timestamp::now(),
            blocks: vec![
                p(
                    "b1",
                    Part::Lead,
                    "A volume keeps data after its container goes.",
                ),
                p(
                    "b2",
                    Part::How,
                    "Docker keeps volumes in its own folder^1. Each has a name.",
                ),
            ],
            sources: vec![source],
            links: Vec::new(),
        };
        let claim = |text: &str, block: &str, quote: &str| Claim {
            text: text.into(),
            sources: vec![1],
            quotes: vec![Quote {
                source: 1,
                text: quote.into(),
            }],
            block: block.into(),
            words: String::new(),
        };
        let claims = vec![
            claim("A volume outlives its container.", "b1", "Volumes persist."),
            claim(
                "Docker keeps volumes in its own folder.",
                "b2",
                "Docker manages volumes.",
            ),
        ];
        (entry, claims)
    }

    #[test]
    fn a_fix_replaces_only_the_blocks_it_gives_and_keeps_the_other_claims() {
        let (entry, claims) = served();
        let answer = json!({
            "blocks": [{ "id": "b2", "part": "how", "fold": "", "type": "paragraph", "data": "{}",
                         "text": "Docker manages volumes^1." }],
            "remove": [],
            "claims": [{ "text": "Docker manages volumes.", "sources": [1],
                         "quotes": [{ "source": 1, "text": "Docker manages volumes." }],
                         "block": "b2", "words": "Docker manages volumes" }],
            "links": [], "sources": [], "drop": [],
        });
        let d = parse(&answer).unwrap();
        let pages = sources_of(&answer).unwrap();
        // Claim 2, in b2, was found wrong: it may go without a word.
        let r = build_revision(RevisionKind::Fix, &d, &pages, &entry, &claims, &[1])
            .unwrap_or_else(|p| panic!("{p:?}"));
        assert_eq!(r.ops.len(), 1);
        assert_eq!(r.blocks[0], entry.blocks[0], "b1 untouched");
        let texts: Vec<&str> = r.claims.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            texts,
            vec![
                "A volume outlives its container.",
                "Docker manages volumes."
            ]
        );
        assert_eq!(
            r.sources, entry.sources,
            "the entry's own source, its number kept"
        );

        // A checked claim in a changed block must be given again or dropped.
        let err = build_revision(RevisionKind::Fix, &d, &pages, &entry, &claims, &[]).unwrap_err();
        assert!(err[0].says.contains("Claim 2 was checked"), "{err:?}");
        let mut dropping = answer.clone();
        dropping["drop"] = json!([{ "claim": 2, "why": "Its page says less." }]);
        let r = build_revision(
            RevisionKind::Fix,
            &parse(&dropping).unwrap(),
            &pages,
            &entry,
            &claims,
            &[],
        )
        .unwrap_or_else(|p| panic!("{p:?}"));
        assert_eq!(r.dropped, vec![(1, "Its page says less.".to_string())]);

        // A fix gives only blocks the entry has.
        let mut new = answer.clone();
        new["blocks"][0]["id"] = json!("b9");
        let err = build_revision(
            RevisionKind::Fix,
            &parse(&new).unwrap(),
            &pages,
            &entry,
            &claims,
            &[1],
        )
        .unwrap_err();
        assert!(
            err[0].says.contains("b9 isn't one of the entry's blocks"),
            "{err:?}"
        );

        // Its block given again as it was, with its claim quoting a page
        // that says it, is a fix; as it was, with nothing new, isn't.
        let requoted = json!({
            "blocks": [{ "id": "b2", "part": "how", "fold": "", "type": "paragraph", "data": "{}",
                         "text": "Docker keeps volumes in its own folder^1. Each has a name." }],
            "remove": [],
            "claims": [{ "text": "Docker keeps volumes in its own folder.", "sources": [1],
                         "quotes": [{ "source": 1, "text": "Volumes are stored in a part of the host filesystem which is managed by Docker." }],
                         "block": "b2", "words": "Docker keeps volumes in its own folder" }],
            "links": [], "sources": [], "drop": [],
        });
        let r = build_revision(
            RevisionKind::Fix,
            &parse(&requoted).unwrap(),
            &pages,
            &entry,
            &claims,
            &[1],
        )
        .unwrap_or_else(|p| panic!("{p:?}"));
        assert_eq!(r.ops.len(), 1);
        assert_eq!(r.blocks, entry.blocks, "its words as they were");
        assert!(r.claims[1].quotes[0].text.starts_with("Volumes are stored"));
        let mut same = requoted.clone();
        same["claims"][0]["quotes"][0]["text"] = json!("Docker manages volumes.");
        let err = build_revision(
            RevisionKind::Fix,
            &parse(&same).unwrap(),
            &pages,
            &entry,
            &claims,
            &[1],
        )
        .unwrap_err();
        assert!(err[0].says.contains("It changes nothing"), "{err:?}");
    }

    #[test]
    fn an_extend_adds_a_fold_after_a_block_with_a_new_page_numbered_after_the_entrys() {
        let (entry, claims) = served();
        let answer = json!({
            "blocks": [{ "id": "b3", "part": "how", "fold": "example", "follows": "b2",
                         "type": "paragraph", "data": "{}",
                         "text": "Say a database writes 2 GB to `/var/lib/data`^2. The volume keeps all 2 GB." }],
            "claims": [{ "text": "Data in a volume stays when the container is removed.", "sources": [2],
                         "quotes": [{ "source": 2, "text": "Data stays." }],
                         "block": "b3", "words": "The volume keeps all 2 GB" }],
            "links": [], "drop": [],
            "sources": [{ "n": 2, "url": "https://docs.docker.com/storage", "title": "Storage",
                          "publisher": "Docker", "published": "" }],
        });
        let r = build_revision(
            RevisionKind::Extend,
            &parse(&answer).unwrap(),
            &sources_of(&answer).unwrap(),
            &entry,
            &claims,
            &[],
        )
        .unwrap_or_else(|p| panic!("{p:?}"));
        let ids: Vec<&str> = r.blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["b1", "b2", "b3"]);
        assert_eq!(r.blocks[2].fold, Some(Fold::Example));
        assert_eq!(r.sources.len(), 2);
        assert_eq!(r.sources[1].n, 2);
        assert_eq!(
            r.claims.len(),
            3,
            "the entry's claims kept, the new one after"
        );
        assert!(matches!(&r.ops[0], Op::Add { after: Some(a), .. } if a == "b2"));

        // An extend never gives a block the entry has, and keeps the parts sound.
        let mut taken = answer.clone();
        taken["blocks"][0]["id"] = json!("b2");
        let err = build_revision(
            RevisionKind::Extend,
            &parse(&taken).unwrap(),
            &sources_of(&taken).unwrap(),
            &entry,
            &claims,
            &[],
        )
        .unwrap_err();
        assert!(
            err.iter().any(|p| p.says.contains("already has b2")),
            "{err:?}"
        );
        let mut first = answer.clone();
        first["blocks"][0]["follows"] = json!("");
        let err = build_revision(
            RevisionKind::Extend,
            &parse(&first).unwrap(),
            &sources_of(&first).unwrap(),
            &entry,
            &claims,
            &[],
        )
        .unwrap_err();
        assert!(
            err.iter().any(|p| p.says.contains("open with its lead")),
            "{err:?}"
        );
    }

    #[test]
    fn excerpts_are_copied_from_material() {
        let mut excerpt = wide("b1", "excerpt");
        excerpt.excerpt = "e1".into();
        excerpt.lines = vec![44.0, 35.0, 36.0];
        excerpt.marks = vec![36.0];
        let built = build(&draft(vec![excerpt]), &material()).unwrap_or_else(|p| panic!("{p:?}"));
        let Body::Excerpt { from, lines, .. } = &built.blocks[0].body else {
            panic!("not an excerpt");
        };
        assert_eq!(from, "Dockerfile");
        let shown: Vec<(Option<u32>, &str, bool, bool)> = lines
            .iter()
            .map(|l| (l.n, l.text.as_str(), l.mark, l.dim))
            .collect();
        assert_eq!(
            shown,
            vec![
                (
                    Some(35),
                    "    && useradd --system --home-dir /data kelve \\",
                    false,
                    false
                ),
                (
                    Some(36),
                    "    && mkdir /data && chown kelve /data",
                    true,
                    false
                ),
                (None, "…", false, true),
                (Some(44), "VOLUME /data", false, false),
            ]
        );
    }

    #[test]
    fn excerpts_point_at_real_lines() {
        let mut excerpt = wide("b1", "excerpt");
        excerpt.excerpt = "e1".into();
        excerpt.lines = vec![36.0, 38.0];
        excerpt.marks = vec![44.0];
        let problems = build(&draft(vec![excerpt]), &material())
            .err()
            .unwrap_or_default();
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems.iter().all(|p| p.rule == Rule::Pointer));

        let mut missing = wide("b1", "excerpt");
        missing.excerpt = "e9".into();
        let problems = build(&draft(vec![missing]), &material())
            .err()
            .unwrap_or_default();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].says.contains("e9"));
    }

    #[test]
    fn lines_without_numbers_are_counted_from_one() {
        let mut m = material();
        m.excerpts[0].lines = vec![
            (None, "Hi Nathan,".into()),
            (None, "The build broke again.".into()),
            (None, "Sam".into()),
        ];
        assert_eq!(m.excerpts[0].keys(), vec![1, 2, 3]);
        let mut excerpt = wide("b1", "excerpt");
        excerpt.excerpt = "e1".into();
        excerpt.lines = vec![2.0];
        excerpt.marks = vec![2.0];
        let built = build(&draft(vec![excerpt]), &m).unwrap_or_else(|p| panic!("{p:?}"));
        let Body::Excerpt { lines, .. } = &built.blocks[0].body else {
            panic!("not an excerpt");
        };
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "The build broke again.");
        assert!(lines[0].mark);
        // Repeated numbers can't name a line, so places are used.
        m.excerpts[0].lines = vec![(Some(4), "a".into()), (Some(4), "b".into())];
        assert_eq!(m.excerpts[0].numbered(), vec![(1, "a"), (2, "b")]);
    }

    #[test]
    fn sources_are_renumbered_in_order_of_use() {
        let mut first = wide("b1", "paragraph");
        first.text = "A bind mount hides the folder^2, unlike a volume^1. In code: `x^3`.".into();
        let mut second = wide("b2", "list");
        second.items = vec!["Scratch files^3".into(), "Again^2".into()];
        let mut d = draft(vec![first, second]);
        d.claims.push(DraftClaim {
            text: "Volumes are filled from the image.".into(),
            sources: vec![1.0],
            ..Default::default()
        });
        let built = build(&d, &material()).unwrap_or_else(|p| panic!("{p:?}"));
        let Body::Paragraph { text, .. } = &built.blocks[0].body else {
            panic!("not a paragraph");
        };
        assert_eq!(
            text,
            "A bind mount hides the folder^1, unlike a volume^2. In code: `x^3`."
        );
        let Body::List { items } = &built.blocks[1].body else {
            panic!("not a list");
        };
        assert_eq!(
            items,
            &vec!["Scratch files^3".to_string(), "Again^1".to_string()]
        );
        let titles: Vec<(u8, &str)> = built
            .sources
            .iter()
            .map(|s| (s.n, s.title.as_str()))
            .collect();
        assert_eq!(
            titles,
            vec![(1, "Bind mounts"), (2, "Volumes"), (3, "tmpfs mounts")]
        );
        assert_eq!(built.claims[1].sources, vec![2]);
        assert_eq!(grounded(&built, &material()), None);
        // Unused sources aren't kept, and an entry that cites none of its
        // pages isn't grounded in them.
        let built =
            build(&draft(vec![plain("b1")]), &material()).unwrap_or_else(|p| panic!("{p:?}"));
        assert!(built.sources.is_empty());
        assert_eq!(
            grounded(&built, &material()).map(|p| p.rule),
            Some(Rule::Grounding)
        );
        let nothing_found = Material {
            sources: vec![],
            ..material()
        };
        assert_eq!(grounded(&built, &nothing_found), None);
    }

    #[test]
    fn an_agents_pages_and_quotes_are_kept_with_its_claims() {
        let value = json!({
            "title": "Volumes",
            "minutes": 2,
            "blocks": [{ "id": "b1", "part": "lead", "type": "paragraph", "text": "Volumes outlive containers^2.", "data": "{}" }],
            "links": [],
            "claims": [{
                "text": "A volume outlives the container that made it.",
                "sources": [2],
                "quotes": [{ "source": 2, "text": "  Volumes persist after the container is removed. " }]
            }],
            "sources": [
                { "n": 1, "url": "https://example.com/unused", "title": "Unused", "publisher": "Ex", "published": "" },
                { "n": 2, "url": "https://docs.docker.com/volumes", "title": "Volumes", "publisher": "Docker", "published": "2025" }
            ]
        });
        let material = sources_of(&value).unwrap_or_else(|p| panic!("{p}"));
        assert_eq!(material.sources.len(), 2);
        assert_eq!(
            material.sources[1].source.published.as_deref(),
            Some("2025")
        );
        let d = parse(&value).unwrap_or_else(|p| panic!("{p}"));
        let built = build(&d, &material).unwrap_or_else(|p| panic!("{p:?}"));
        // Numbered as the entry first cites them: source 2 is now 1.
        assert_eq!(built.sources.len(), 1);
        assert_eq!(built.sources[0].title, "Volumes");
        assert_eq!(
            built.claims[0].quotes,
            vec![Quote {
                source: 1,
                text: "Volumes persist after the container is removed.".into()
            }]
        );
        // A quote of a source it doesn't list is a problem.
        let mut bad = value.clone();
        bad["claims"][0]["quotes"][0]["source"] = json!(5);
        let d = parse(&bad).unwrap_or_else(|p| panic!("{p}"));
        assert!(build(&d, &material).is_err());
    }

    #[test]
    fn a_claim_is_placed_where_its_words_are_and_only_there() {
        let value = json!({
            "title": "Volumes",
            "minutes": 2,
            "blocks": [
                { "id": "b1", "part": "lead", "type": "paragraph", "text": "A [[volume]] outlives its container^1, so **data stays** after it goes.", "data": "{}" },
                { "id": "b2", "part": "how", "type": "list", "data": "{\"items\": [\"Named volumes^1\", \"Bind mounts\"]}" }
            ],
            "links": [{ "term": "volume", "idea": "Volume", "about": "Storage a container mounts.", "kind": "concept", "link": "restsOn" }],
            "claims": [
                // Copied with its markup, or without: found either way.
                { "text": "A volume outlives its container.", "sources": [1],
                  "quotes": [{ "source": 1, "text": "Volumes persist." }],
                  "block": "b1", "words": "A [[volume]] outlives its container^1" },
                { "text": "Data stays after the container goes.", "sources": [1],
                  "quotes": [{ "source": 1, "text": "Volumes persist." }],
                  "block": "b1", "words": "so data stays after it goes" },
                { "text": "There are named volumes.", "sources": [1],
                  "quotes": [{ "source": 1, "text": "Volumes persist." }],
                  "block": "b2", "words": "Named volumes" },
                // Words that aren't there: in its block, but not shown
                // where. A block that isn't: unplaced.
                { "text": "Volumes are fast.", "sources": [1],
                  "quotes": [{ "source": 1, "text": "Volumes persist." }],
                  "block": "b1", "words": "volumes are fast" },
                { "text": "Volumes are named.", "sources": [1],
                  "quotes": [{ "source": 1, "text": "Volumes persist." }],
                  "block": "b9", "words": "Named volumes" }
            ],
            "sources": [{ "n": 1, "url": "https://docs.docker.com/volumes", "title": "Volumes", "publisher": "Docker", "published": "" }]
        });
        let material = sources_of(&value).unwrap_or_else(|p| panic!("{p}"));
        let d = parse(&value).unwrap_or_else(|p| panic!("{p}"));
        let built = build(&d, &material).unwrap_or_else(|p| panic!("{p:?}"));
        let placed: Vec<(&str, &str)> = built
            .claims
            .iter()
            .map(|c| (c.block.as_str(), c.words.as_str()))
            .collect();
        assert_eq!(
            placed,
            vec![
                ("b1", "A volume outlives its container"),
                ("b1", "so data stays after it goes"),
                ("b2", "Named volumes"),
                ("b1", ""),
                ("", ""),
            ]
        );
        assert_eq!(
            plain_prose("A [[volume]] outlives `docker run`^12, **so**."),
            "A volume outlives docker run, so."
        );
    }

    #[test]
    fn an_agents_pages_must_be_real_addresses() {
        let with = |sources: Value| sources_of(&json!({ "sources": sources }));
        assert!(
            with(json!([{ "n": 1, "url": "http://example.com", "title": "T", "publisher": "" }]))
                .is_err()
        );
        assert!(
            with(
                json!([{ "n": 1, "url": "https://example.com/a b", "title": "T", "publisher": "" }])
            )
            .is_err()
        );
        assert!(
            with(json!([{ "n": 0, "url": "https://example.com", "title": "T", "publisher": "" }]))
                .is_err()
        );
        assert!(
            with(json!([{ "n": 1, "url": "https://example.com", "title": "", "publisher": "" }]))
                .is_err()
        );
        assert!(
            with(json!([
                { "n": 1, "url": "https://a.example", "title": "A", "publisher": "" },
                { "n": 1, "url": "https://b.example", "title": "B", "publisher": "" }
            ]))
            .is_err()
        );
        let many: Vec<Value> = (1..=9)
            .map(|n| json!({ "n": n, "url": format!("https://e.example/{n}"), "title": "T", "publisher": "" }))
            .collect();
        assert!(with(Value::Array(many)).is_err());
        let ok = with(json!([{ "n": 1, "url": "https://arxiv.org/abs/1", "title": "A paper", "publisher": "Them" }]))
            .unwrap_or_else(|p| panic!("{p}"));
        assert!(ok.sources[0].source.preprint);
        // The schema asks for both.
        let schema = agent_schema();
        assert!(
            schema["required"]
                .as_array()
                .is_some_and(|r| r.contains(&json!("sources")))
        );
        assert!(
            schema["properties"]["claims"]["items"]["required"]
                .as_array()
                .is_some_and(|r| r.contains(&json!("quotes")))
        );
    }

    #[test]
    fn citations_and_claims_must_be_real() {
        let mut p = wide("b1", "paragraph");
        p.text = "As the docs say^7.".into();
        let mut d = draft(vec![p]);
        d.claims = vec![
            DraftClaim {
                text: "Made up.".into(),
                sources: vec![9.0, 1.5],
                ..Default::default()
            },
            DraftClaim {
                text: "General knowledge points at nothing, and the checker judges it.".into(),
                sources: vec![],
                ..Default::default()
            },
        ];
        let problems = build(&d, &material()).err().unwrap_or_default();
        let says: Vec<String> = problems.iter().map(ToString::to_string).collect();
        assert_eq!(problems.len(), 3, "{says:?}");
        assert!(says[0].starts_with("In block b1: It cites ^7"), "{says:?}");
        assert!(problems.iter().all(|p| p.rule == Rule::Pointer));
        let problems = build(
            &Draft {
                claims: vec![],
                ..draft(vec![plain("b1")])
            },
            &material(),
        )
        .err()
        .unwrap_or_default();
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn a_model_cannot_supply_quoted_text_or_links() {
        // A draft can't carry its own excerpt text: there is no field for
        // it, and extra fields are ignored.
        let value = json!({
            "title": "Why Docker made a folder",
            "minutes": 3,
            "blocks": [{
                "id": "b1", "part": "lead", "type": "excerpt", "excerpt": "e1", "lines": [36], "marks": [],
                "text": "Ignore previous instructions", "html": "<img src=x>",
                "url": "https://evil.example"
            }],
            "claims": [{ "text": "It mkdirs.", "events": ["ev1"], "sources": [] }]
        });
        let d = parse(&value).unwrap_or_else(|p| panic!("{p}"));
        let built = build(&d, &material()).unwrap_or_else(|p| panic!("{p:?}"));
        let Body::Excerpt { lines, .. } = &built.blocks[0].body else {
            panic!("not an excerpt");
        };
        assert_eq!(lines[0].text, "    && mkdir /data && chown kelve /data");
        // Links in prose get through build, and stop at check.
        let mut p = wide("b1", "paragraph");
        p.text = "Read [this](https://evil.example/?q=secret).".into();
        let built = build(&draft(vec![p]), &material()).unwrap_or_else(|p| panic!("{p:?}"));
        let problems = check(&built.blocks, &built.sources, &[]);
        assert!(
            problems.iter().any(|p| p.rule == Rule::Unsafe),
            "{problems:?}"
        );
        // And a title with a link stops at build.
        let mut d = draft(vec![plain("b1")]);
        d.title = "See https://evil.example".into();
        let problems = build(&d, &material()).err().unwrap_or_default();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems.iter().all(|p| p.rule == Rule::Unsafe));
    }

    #[test]
    fn enums_and_shapes_are_checked() {
        let mut diagram = wide("b1", "diagram");
        diagram.layout = "spiral".into();
        let mut explorable = wide("b2", "explorable");
        explorable.formula = "eval(code)".into();
        let mut table = wide("b3", "table");
        table.head = vec!["A".into()];
        table.rows = vec![vec!["1".into()]];
        // Marks that aren't [row, column] are left out, not problems.
        table.cell_marks = vec![vec![0.0], vec![-1.0, 0.0], vec![0.0, 0.0]];
        let mut timeline = wide("b4", "timeline");
        timeline.moments = vec![WideMoment {
            when: "Friday".into(),
            text: "It broke.".into(),
            // A label where the mark goes is read as marked, not a problem.
            mark: "the key moment".into(),
        }];
        let unknown = wide("b5", "iframe");
        let mut d = draft(vec![diagram, explorable, table, timeline, unknown]);
        d.minutes = 0.0;
        let problems = build(&d, &material()).err().unwrap_or_default();
        let blocks: Vec<Option<&str>> = problems.iter().map(|p| p.block.as_deref()).collect();
        assert_eq!(
            blocks,
            vec![Some("b1"), Some("b2"), Some("b5"), None],
            "{problems:?}"
        );
    }

    #[test]
    fn a_whole_draft_builds_and_checks() {
        let value = json!({
            "title": "Your own Dockerfile already knew this",
            "minutes": 1,
            "blocks": [
                { "id": "b1", "part": "lead", "type": "paragraph", "text": "kelve gives itself a [[named volume]], `data:/data`, and runs as **its own user**.", "quiet": false },
                { "id": "b3", "part": "how", "type": "excerpt", "excerpt": "e1", "lines": [35, 36, 39, 44], "marks": [36] },
                { "id": "b4", "part": "how", "type": "paragraph", "text": "A new volume copies the image's folder in, owner and all^3, unlike a [[bind mount]]." }
            ],
            "links": [
                { "term": "bind mount", "idea": "Bind mounts", "about": "A host folder mounted into a container.", "kind": "concept", "link": "alternative" },
                { "term": "named volume", "idea": "Docker volumes", "about": "Storage Docker manages.", "kind": "technology", "link": "restsOn" },
                { "term": "unused", "idea": "Nothing", "about": "Never linked.", "kind": "concept", "link": "related" }
            ],
            "claims": [
                { "text": "The image makes /data owned by kelve.", "events": ["ev2"], "sources": [] },
                { "text": "A new volume is filled from the image.", "events": [], "sources": [3] }
            ]
        });
        let d = parse(&value).unwrap_or_else(|p| panic!("{p}"));
        let built = build(&d, &material()).unwrap_or_else(|p| panic!("{p:?}"));
        assert_eq!(built.minutes, 1);
        assert_eq!(built.sources.len(), 1);
        assert_eq!(built.sources[0].title, "tmpfs mounts");
        let terms: Vec<String> = built.links.iter().map(|l| l.term.clone()).collect();
        assert_eq!(terms, ["named volume", "bind mount"]);
        assert_eq!(built.links[0].link_kind(), LinkKind::RestsOn);
        assert_eq!(built.links[1].link_kind(), LinkKind::Alternative);
        assert_eq!(built.links[0].idea, "Docker volumes");
        assert_eq!(check(&built.blocks, &built.sources, &terms), vec![]);
    }

    #[test]
    fn links_name_the_idea_they_lead_to() {
        let mut p = wide("b1", "paragraph");
        p.text = "A [[pod]] runs.".into();
        let mut d = draft(vec![p]);
        d.links = vec![DraftLink {
            term: "pod".into(),
            idea: "!!".into(),
            ..DraftLink::default()
        }];
        let problems = build(&d, &material()).expect_err("broken");
        let says: Vec<&str> = problems.iter().map(|p| p.says.as_str()).collect();
        assert!(says.iter().any(|s| s.contains("doesn't name")), "{says:?}");
    }

    #[test]
    fn a_bad_answer_is_a_problem_not_a_panic() {
        assert!(parse(&json!("not a draft")).is_err());
        assert!(parse(&json!({ "blocks": "nope" })).is_err());
        // Missing fields are empty, and build says what's missing.
        let d = parse(&json!({})).unwrap_or_else(|p| panic!("{p}"));
        let problems = build(&d, &Material::default()).err().unwrap_or_default();
        assert!(!problems.is_empty());
        // Numbers that aren't whole, or are huge, don't panic.
        let mut excerpt = wide("b1", "excerpt");
        excerpt.excerpt = "e1".into();
        excerpt.lines = vec![f64::NAN, 1e300, -3.0, 36.5];
        let problems = build(&draft(vec![excerpt]), &material())
            .err()
            .unwrap_or_default();
        assert_eq!(problems.len(), 4);
    }

    /// Walks a schema, checking what both providers' structured outputs
    /// require.
    fn strict(schema: &Value, at: &str) {
        let Some(object) = schema.as_object() else {
            panic!("{at} isn't an object");
        };
        for banned in [
            "oneOf",
            "anyOf",
            "allOf",
            "not",
            "$ref",
            "$defs",
            "minLength",
            "maxLength",
            "minimum",
            "maximum",
            "exclusiveMinimum",
            "exclusiveMaximum",
            "multipleOf",
            "pattern",
            "format",
            "minItems",
            "maxItems",
            "uniqueItems",
            "default",
            "const",
        ] {
            assert!(!object.contains_key(banned), "{at} has {banned}");
        }
        let kind = object.get("type").and_then(Value::as_str);
        assert!(kind.is_some(), "{at} has no single type");
        if let Some(values) = object.get("enum").and_then(Value::as_array) {
            assert!(values.iter().all(Value::is_string), "{at}");
        }
        match kind {
            Some("object") => {
                assert_eq!(
                    object.get("additionalProperties"),
                    Some(&Value::Bool(false)),
                    "{at}"
                );
                let properties = object
                    .get("properties")
                    .and_then(Value::as_object)
                    .unwrap_or_else(|| panic!("{at} has no properties"));
                let mut required: Vec<&str> = object
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|r| r.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                let mut names: Vec<&str> = properties.keys().map(String::as_str).collect();
                required.sort_unstable();
                names.sort_unstable();
                assert_eq!(required, names, "{at}: every property is required");
                for (name, property) in properties {
                    strict(property, &format!("{at}.{name}"));
                }
            }
            Some("array") => strict(&object["items"], &format!("{at}[]")),
            _ => {}
        }
    }

    #[test]
    fn the_schema_suits_every_provider() {
        let schema = schema();
        strict(&schema, "draft");
        let block = &schema["properties"]["blocks"]["items"];
        let types: Vec<&str> = block["properties"]["type"]["enum"]
            .as_array()
            .map(|v| v.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        assert_eq!(types, TYPES.to_vec());
        let fields: Vec<&String> = block["properties"]
            .as_object()
            .map(|p| p.keys().collect())
            .unwrap_or_default();
        assert_eq!(fields, ["id", "part", "fold", "type", "text", "data"]);
        // Every other field a draft reads is in the guide, and strict too.
        strict(&super::wide(), "wide");
        let guide = guide();
        for name in super::wide()["properties"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(n, _)| n)
        {
            if !matches!(name.as_str(), "id" | "type" | "text") {
                assert!(
                    guide.contains(&format!("- {name}: ")),
                    "{name} isn't in the guide"
                );
            }
        }
        for formula in FORMULAS {
            let name = serde_json::to_value(formula).unwrap_or_default();
            assert_eq!(name.as_str(), Some(name_of(&formula)));
        }
    }

    #[test]
    fn an_excerpt_of_nothing_is_left_out_and_one_of_something_unknown_is_a_problem() {
        let empty = wide("b2", "excerpt");
        let built = build(&draft(vec![plain("b1"), empty]), &material())
            .unwrap_or_else(|p| panic!("{p:?}"));
        assert_eq!(built.blocks.len(), 1);
        let mut unknown = wide("b2", "excerpt");
        unknown.excerpt = "x99".into();
        assert!(build(&draft(vec![plain("b1"), unknown]), &material()).is_err());
    }

    #[test]
    fn a_link_with_other_words_shown_links_those_words_to_its_idea() {
        let value = json!({
            "title": "Advisory locks", "minutes": 3,
            "blocks": [
                { "id": "b1", "part": "lead", "type": "paragraph", "text": "Two locks taken in turn can cause [[deadlock|deadlocks]], like any [[lock]].", "data": "{}" },
                { "id": "b2", "part": "how", "type": "list", "text": "", "data": "{\"items\": [\"Avoid [[deadlock|a deadlock]] by order.\", \"[[|nothing]] and [[lock|]] stay.\"]}" },
            ],
            "links": [
                { "term": "deadlock", "idea": "Deadlock", "about": "Each waits on the other.", "kind": "concept", "link": "related" },
                { "term": "lock", "idea": "Locks", "about": "One at a time.", "kind": "concept", "link": "kindOf" },
            ],
        });
        let d = parse(&value).unwrap_or_else(|p| panic!("{p}"));
        assert_eq!(
            d.blocks[0].text,
            "Two locks taken in turn can cause [[deadlocks]], like any [[lock]]."
        );
        assert_eq!(d.blocks[1].items[0], "Avoid [[a deadlock]] by order.");
        assert_eq!(d.blocks[1].items[1], "[[|nothing]] and [[lock|]] stay.");
        let shown: Vec<(&str, &str)> = d
            .links
            .iter()
            .map(|l| (l.term.as_str(), l.idea.as_str()))
            .collect();
        assert!(shown.contains(&("deadlocks", "Deadlock")), "{shown:?}");
        assert!(shown.contains(&("a deadlock", "Deadlock")), "{shown:?}");
    }

    #[test]
    fn a_blocks_data_is_read_into_it() {
        let value = json!({
            "title": "Why your volume hid the files", "minutes": 3, "from": "From your session.",
            "blocks": [
                { "id": "b1", "part": "lead", "type": "paragraph", "text": "**A mount covers the folder.**", "data": "{}" },
                { "id": "b2", "part": "how", "type": "list", "text": "",
                  "data": "{\"items\": [\"Hidden\", \"Merged\"], \"id\": \"b9\"}" },
            ],
            "cards": [{ "question": "What does a mount do to the folder?", "answer": "It covers it." }],
            "claims": [{ "text": "It mounts over /app.", "events": ["ev1"], "sources": [] }],
        });
        let draft = parse(&value).unwrap_or_else(|p| panic!("{p}"));
        assert_eq!(
            draft.blocks[1].id, "b2",
            "data can't change what the block says itself"
        );
        assert_eq!(draft.blocks[1].items.len(), 2);
        let built = build(&draft, &material()).unwrap_or_else(|p| panic!("{p:?}"));
        assert!(matches!(built.blocks[1].body, Body::List { .. }));

        let mut described = value.clone();
        described["blocks"][1]["data"] = json!("{\"description\": \"Two layers.\"}");
        let draft = parse(&described).unwrap_or_else(|p| panic!("{p}"));
        assert_eq!(
            draft.blocks[1].describe, "Two layers.",
            "a description is its describe"
        );
        described["blocks"][1]["data"] =
            json!("{\"describe\": \"Mine.\", \"description\": \"Not.\"}");
        let draft = parse(&described).unwrap_or_else(|p| panic!("{p}"));
        assert_eq!(draft.blocks[1].describe, "Mine.");

        let mut broken = value.clone();
        broken["blocks"][1]["data"] = json!("{\"items\": ");
        let problem = parse(&broken).expect_err("not JSON");
        assert_eq!(problem.block.as_deref(), Some("b2"));
        assert_eq!(problem.rule, Rule::Shape);
    }

    #[test]
    fn every_block_type_is_kelves() {
        let bodies = [
            Body::Paragraph {
                text: String::new(),
                quiet: false,
            },
            Body::Heading {
                text: String::new(),
            },
            Body::List { items: vec![] },
            Body::Excerpt {
                lang: None,
                from: String::new(),
                lines: vec![],
            },
            Body::Diagram {
                layout: Layout::Row,
                nodes: vec![],
                edges: vec![],
                steps: vec![],
                describe: String::new(),
            },
            Body::Explorable {
                formula: Formula::RateTimesTime,
                inputs: vec![],
                output: String::new(),
                describe: String::new(),
            },
            Body::Bars {
                rows: vec![],
                caption: None,
            },
            Body::Table {
                head: vec![],
                rows: vec![],
                marks: vec![],
            },
            Body::Compare {
                left: Side {
                    title: String::new(),
                    points: vec![],
                },
                right: Side {
                    title: String::new(),
                    points: vec![],
                },
            },
            Body::Squares { rows: vec![] },
            Body::Timeline { rows: vec![] },
            Body::Conversation { lines: vec![] },
            Body::Answer {
                lead: String::new(),
                text: String::new(),
            },
            Body::Sequence {
                actors: vec![],
                messages: vec![],
                describe: String::new(),
            },
            Body::Steps { cards: vec![] },
            Body::Quadrant {
                x: QuadrantAxis {
                    low: String::new(),
                    high: String::new(),
                },
                y: QuadrantAxis {
                    low: String::new(),
                    high: String::new(),
                },
                points: vec![],
                describe: String::new(),
            },
            Body::Spectrum {
                low: String::new(),
                high: String::new(),
                points: vec![],
                describe: String::new(),
            },
            Body::Trend {
                labels: vec![],
                series: vec![],
                unit: None,
                describe: String::new(),
            },
            Body::Tree {
                branches: vec![],
                describe: String::new(),
            },
            Body::BeforeAfter {
                before: Version {
                    label: String::new(),
                    from: None,
                    lang: None,
                    lines: vec![],
                },
                after: Version {
                    label: String::new(),
                    from: None,
                    lang: None,
                    lines: vec![],
                },
                describe: String::new(),
            },
            Body::Number {
                figure: String::new(),
                text: String::new(),
                share: None,
            },
            Body::Annotated {
                from: String::new(),
                lang: None,
                lines: vec![],
                notes: vec![],
            },
            Body::Funnel {
                stages: vec![],
                caption: None,
            },
        ];
        let names: Vec<&str> = bodies.iter().map(Body::name).collect();
        assert_eq!(names, TYPES.to_vec());
    }

    /// A draft as a model writes it, with each block's fields in `data`.
    fn written(blocks: Value) -> Draft {
        let blocks: Vec<Value> = blocks
            .as_array()
            .unwrap_or_else(|| panic!("blocks"))
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let mut b = b.clone();
                let data = b["data"].to_string();
                b["data"] = json!(data);
                b["part"] = json!(if i == 0 { "lead" } else { "how" });
                b
            })
            .collect();
        parse(&json!({
            "title": "How a bind mount hides what the image built",
            "minutes": 3,
            "from": "From `Dockerfile` and Docker's documentation^1.",
            "blocks": blocks,
            "cards": [{ "question": "What does a bind mount do to the path?", "answer": "It covers it." }],
            "claims": [{ "text": "A bind mount covers the path it's mounted on.", "events": [], "sources": [1] }],
        }))
        .unwrap_or_else(|p| panic!("{p}"))
    }

    fn block(id: &str, kind: &str, data: Value) -> Value {
        json!({ "id": id, "type": kind, "text": "", "data": data })
    }

    /// Every one of the newer visuals, as a model would write it.
    fn visuals() -> Value {
        json!([
            block(
                "b1",
                "sequence",
                json!({
                    "actors": ["You", "Compose", "Postgres"],
                    "messages": [
                        { "from": "You", "to": "Compose", "text": "up -d", "mark": false },
                        { "from": "Compose", "to": "Postgres", "text": "start, with the volume", "mark": true },
                        { "from": "Postgres", "to": "You", "text": "ready on 5442", "mark": false },
                    ],
                    "describe": "You ask Compose, Compose starts Postgres, Postgres answers.",
                })
            ),
            block(
                "b2",
                "steps",
                json!({
                    "cards": [
                        { "title": "Read the file", "text": "Compose reads `docker-compose.yml`." },
                        { "title": "Start", "text": "It starts each service, in order^1." },
                    ],
                })
            ),
            block(
                "b3",
                "quadrant",
                json!({
                    "xLow": "harder to start", "xHigh": "easier to start",
                    "yLow": "less control", "yHigh": "more control",
                    "points": [
                        { "label": "Postgres installed", "x": 20, "y": 80, "mark": false },
                        { "label": "Compose", "x": 75, "y": 70, "mark": true },
                    ],
                    "describe": "Compose is easier to start, with nearly as much control.",
                })
            ),
            block(
                "b4",
                "spectrum",
                json!({
                    "lowEnd": "a feeling", "highEnd": "a full suite",
                    "places": [
                        { "label": "Eyeballing", "at": 5, "note": "", "mark": false },
                        { "label": "Ten cases", "at": 45, "note": "where you are", "mark": true },
                    ],
                    "describe": "From a feeling to a full suite, ten cases sit in the middle.",
                })
            ),
            block(
                "b5",
                "trend",
                json!({
                    "xLabels": ["2020", "2022", "2024"],
                    "series": [{ "label": "Use Compose", "values": [40, 55, 68], "mark": true }],
                    "unit": "%",
                    "describe": "More people use Compose each year.",
                })
            ),
            block(
                "b6",
                "tree",
                json!({
                    "branches": [
                        { "id": "t1", "label": "Docker installed?", "parent": "", "edge": "", "mark": false },
                        { "id": "t2", "label": "compose up", "parent": "t1", "edge": "yes", "mark": true },
                        { "id": "t3", "label": "Install Postgres", "parent": "t1", "edge": "no", "mark": false },
                    ],
                    "describe": "With Docker, Compose; without it, install Postgres.",
                })
            ),
            block(
                "b7",
                "beforeAfter",
                json!({
                    "before": { "label": "Your Dockerfile", "excerpt": "e1", "lines": [39, 44], "written": [], "lang": "" },
                    "after": { "label": "With a volume", "excerpt": "", "lines": [], "written": ["volumes:", "  - data:/data"], "lang": "yaml" },
                    "describe": "The volume keeps the data outside the image.",
                })
            ),
            json!({ "id": "b8", "part": "how", "type": "number", "reveal": "", "text": "of setups stop at the database^1.", "data": { "figure": "1 in 3", "share": 33 } }),
            block(
                "b9",
                "annotated",
                json!({
                    "excerpt": "e1",
                    "lines": [36, 39, 44],
                    "notes": [
                        { "line": 44, "text": "The volume is made here." },
                        { "line": 36, "text": "kelve owns `/data`." },
                    ],
                })
            ),
            block(
                "b10",
                "funnel",
                json!({
                    "stages": [
                        { "label": "Read the docs", "value": 100, "display": "100", "mark": false },
                        { "label": "Database running", "value": 41, "display": "41", "mark": true },
                    ],
                    "caption": "",
                })
            ),
        ])
    }

    #[test]
    fn the_newer_visuals_build_and_pass_their_checks() {
        let built = build(&written(visuals()), &material()).unwrap_or_else(|p| panic!("{p:?}"));
        let names: Vec<&str> = built.blocks.iter().map(|b| b.body.name()).collect();
        assert_eq!(
            names,
            [
                "sequence",
                "steps",
                "quadrant",
                "spectrum",
                "trend",
                "tree",
                "beforeAfter",
                "number",
                "annotated",
                "funnel"
            ]
        );
        assert_eq!(check(&built.blocks, &built.sources, &[]), vec![]);

        // The before is copied from the excerpt; the after is written, in
        // a language kelve knows.
        let Body::BeforeAfter { before, after, .. } = &built.blocks[6].body else {
            panic!("not a before and after");
        };
        assert_eq!(before.from.as_deref(), Some("Dockerfile"));
        assert_eq!(before.lines, ["USER kelve", "VOLUME /data"]);
        assert_eq!(
            (after.from.as_deref(), after.lang.as_deref()),
            (None, Some("yaml"))
        );

        // Notes are pinned to where their lines are among those shown,
        // after the gap, in order.
        let Body::Annotated { lines, notes, .. } = &built.blocks[8].body else {
            panic!("not annotated");
        };
        let shown: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            shown,
            [
                "    && mkdir /data && chown kelve /data",
                "…",
                "USER kelve",
                "VOLUME /data"
            ]
        );
        let pinned: Vec<u16> = notes.iter().map(|n| n.line).collect();
        assert_eq!(pinned, [0, 3]);
        assert!(lines[0].mark && lines[3].mark && !lines[2].mark);

        // Sources cited in the new blocks are numbered with the rest.
        assert_eq!(built.sources.len(), 1);
    }

    #[test]
    fn the_newer_visuals_say_what_is_wrong() {
        let cases = [
            (
                block(
                    "b1",
                    "sequence",
                    json!({
                        "actors": ["You", "Compose"],
                        "messages": [{ "from": "You", "to": "Docker", "text": "up", "mark": false }],
                        "describe": "You ask.",
                    }),
                ),
                "Docker, which isn't one of the sequence's actors",
            ),
            (
                block(
                    "b1",
                    "tree",
                    json!({
                        "branches": [
                            { "id": "t1", "label": "Question", "parent": "", "edge": "", "mark": false },
                            { "id": "t2", "label": "Yes", "parent": "t3", "edge": "", "mark": false },
                            { "id": "t3", "label": "No", "parent": "t1", "edge": "", "mark": false },
                        ],
                        "describe": "A question.",
                    }),
                ),
                "t3, which isn't a branch before it",
            ),
            (
                block(
                    "b1",
                    "funnel",
                    json!({
                        "stages": [
                            { "label": "Visit", "value": 10, "display": "10", "mark": false },
                            { "label": "Install", "value": 12, "display": "12", "mark": false },
                        ],
                    }),
                ),
                "no more than the one before",
            ),
            (
                block(
                    "b1",
                    "trend",
                    json!({
                        "xLabels": ["2020", "2022"],
                        "series": [{ "label": "Use", "values": [1, 2, 3], "mark": false }],
                        "describe": "Up.",
                    }),
                ),
                "has 3 values, and the trend has 2 labels",
            ),
            (
                block(
                    "b1",
                    "beforeAfter",
                    json!({
                        "before": { "label": "Before", "written": ["image: postgres"], "lang": "yaml" },
                        "after": { "label": "After", "written": ["see https://example.com"], "lang": "" },
                        "describe": "A change.",
                    }),
                ),
                "has a link or a web address in it",
            ),
            (
                block(
                    "b1",
                    "quadrant",
                    json!({
                        "xLow": "a", "xHigh": "b", "yLow": "c", "yHigh": "d",
                        "points": [
                            { "label": "One", "x": 120, "y": 10, "mark": false },
                            { "label": "Two", "x": 10, "y": 10, "mark": false },
                        ],
                        "describe": "Two.",
                    }),
                ),
                "Both go from 0 to 100",
            ),
        ];
        for (one, says) in cases {
            let built =
                build(&written(json!([one])), &material()).unwrap_or_else(|p| panic!("{p:?}"));
            let found = check(&built.blocks, &built.sources, &[]);
            assert!(
                found.iter().any(|p| p.says.contains(says)),
                "wanted {says:?} in {found:?}"
            );
        }

        // A note on a line the excerpt doesn't show is a pointer problem.
        let stray = block(
            "b1",
            "annotated",
            json!({
                "excerpt": "e1",
                "lines": [44],
                "notes": [{ "line": 36, "text": "Not shown." }],
            }),
        );
        let problems = build(&written(json!([stray])), &material()).expect_err("a stray note");
        assert!(
            problems.iter().any(|p| p.rule == Rule::Pointer),
            "{problems:?}"
        );
    }

    #[test]
    fn data_closed_once_too_often_is_read_and_anything_else_after_it_isnt() {
        let ok = data_of(r#"{"steps": [{"a": 1}]}}"#).expect("read");
        assert_eq!(ok["steps"][0]["a"], 1);
        assert!(data_of(r#"{"a": 1} ]"#).is_ok());
        assert!(data_of(r#"{"a": 1} and more"#).is_err());
        assert!(data_of(r#"{"a": 1}{"b": 2}"#).is_err());
        assert!(data_of("").is_err());
    }
}
