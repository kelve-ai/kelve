//! Marking a file: the ideas it rests on, a passage at a time, read by the
//! person's own agent.
//!
//! 1. The engine reads the file, cuts it into passages, and finds in each
//!    what a mark can point at (`kelve_engine`).
//! 2. The agent reads every passage not read before, in one job, and
//!    proposes a few marks for each, by pointer, each with the idea's name,
//!    a line on what it is, and why it matters there.
//! 3. Code keeps them to the rules (`core::marks`).
//! 4. The agent checks each mark and, from the ideas the shared graph has
//!    by that name, chooses the one it means, or none, in a second job.
//! 5. Marks are kept against each passage's hash, so an unchanged passage
//!    is never read twice (`reader::kept`, which keeps them in the project
//!    too).
//!
//! Only names leave for the graph, after `core::redact::query_ok`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use kelve_core::IdeaRef;
use kelve_core::fence::{fence, fence_with};
use kelve_core::lens::Lens;
use kelve_core::marks::{self, Candidate, Mark, Said};
use kelve_engine::passages::{Kind, Passage};
use kelve_graph_api::{MatchRequest, Query};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{Failed, Result};
use crate::agent::{self, Ask, Priority, Resume};
use crate::state::Shared;

/// Whose marks they are, in the store: the agent's, whichever model it
/// ran on.
pub const BY: &str = "agent";

/// What marking a file came to.
#[derive(Debug, Default, PartialEq)]
pub struct Marked {
    /// Its marks, in order, each idea once.
    pub marks: Vec<Mark>,
    /// Passages not marked yet, and why.
    pub waiting: Option<Waiting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waiting {
    /// They're with the person's agent, until it hands in its answer.
    Agent,
}

/// What a passage is marked with: the file, its lines and what's around.
pub(crate) struct File<'a> {
    rel: &'a str,
    lines: Vec<&'a str>,
    imports: Vec<String>,
    outline: String,
    /// Parsed once, for every passage's candidates.
    parsed: kelve_engine::candidates::Parsed<'a>,
}

/// Lines `from` to `to` of a file, counting from 1, both included: in a
/// change, the lines it added.
pub type Lines = (u32, u32);

/// What marking is for: what's open, how soon it's wanted, and the work to
/// run again once the agent has answered.
#[derive(Debug, Clone)]
pub struct For {
    pub opened: Option<i64>,
    pub priority: Priority,
    pub resume: Resume,
}

/// The passages of a text that are marked: not tests, and, in a change,
/// only those its added lines touch.
fn to_mark(rel: &str, text: &str, only: Option<&[Lines]>) -> Vec<Passage> {
    // Every passage, tests too: what they check, and how, rests on ideas.
    passages_in(rel, text)
        .into_iter()
        .filter(|p| only.is_none_or(|o| o.iter().any(|(a, b)| *a <= p.end && *b >= p.start)))
        .collect()
}

/// Files' passages, lately found, by path and text: finding them parses
/// the file, and the same text in the same place has the same passages.
static PASSAGES: LazyLock<Mutex<HashMap<Found, Vec<Passage>>>> = LazyLock::new(Default::default);

/// A file's path, and its text's hash.
type Found = (String, String);

/// How many files' passages are kept found.
const KEEP_PASSAGES: usize = 2_000;

/// A file's passages, found once for each text.
fn passages_in(rel: &str, text: &str) -> Vec<Passage> {
    let key = (rel.to_string(), crate::reader::text_hash(text));
    if let Some(found) = PASSAGES.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
        return found.clone();
    }
    let found = kelve_engine::passages::passages(rel, text);
    let mut kept = PASSAGES.lock().unwrap_or_else(|p| p.into_inner());
    if kept.len() >= KEEP_PASSAGES {
        kept.clear();
    }
    kept.insert(key, found.clone());
    found
}

/// The marks that stand: each idea once, and in a change, only those on its
/// added lines.
fn standing(all: Vec<Mark>, only: Option<&[Lines]>) -> Vec<Mark> {
    marks::first_only(
        all.into_iter()
            .filter(|m| only.is_none_or(|o| o.iter().any(|(a, b)| *a <= m.end && *b >= m.start)))
            .collect(),
    )
}

/// A text's marks as kept, without asking the agent: the marks, and
/// whether every passage has been marked.
pub async fn kept(
    app: &Shared,
    rel: &str,
    text: &str,
    only: Option<&[Lines]>,
) -> Result<(Vec<Mark>, bool)> {
    let passages = to_mark(rel, text, only);
    let version = read_version(app).await;
    let (wanted, _) = lens_for(app);
    let mut all = Vec::new();
    let mut whole = true;
    for kept in kept_passages(app, &version, &passages).await? {
        match kept {
            Some((kept, lens)) => {
                whole &= lens.covers(&wanted);
                all.extend(kept);
            }
            None => whole = false,
        }
    }
    Ok((standing(all, only), whole))
}

/// The version marks are kept under: the instructions that make them,
/// `read`'s and `choose`'s, which decides what's kept. What a reading looked
/// for isn't in it: that's kept with the marks (`Kept`).
pub async fn read_version(app: &Shared) -> String {
    let read = agent::version(app, agent::READ).await;
    let choose = agent::version(app, agent::CHOOSE).await;
    format!("{read}+{choose}")
}

/// What the person asked kelve to look for, and their own words on it.
pub fn lens_for(app: &Shared) -> (Lens, Option<String>) {
    crate::spec::lens(app)
}

/// The marks shown to the person: those what they asked kelve to mark
/// wants (`spec::Spec::shows`). What's kept is for anyone; this is theirs.
pub fn shown(app: &Shared, marks: Vec<Mark>) -> Vec<Mark> {
    let spec = crate::spec::read(app);
    marks
        .into_iter()
        .filter(|m| spec.shows(&m.area, m.depth, &m.name))
        .collect()
}

/// A passage's marks as kept: what was found, and what was looked for.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kept {
    pub lens: Lens,
    pub marks: Vec<Mark>,
}

impl Kept {
    /// Two readings of one passage as one: every mark either found, each
    /// idea once, and all both looked for.
    pub fn merge(self, other: Kept) -> Kept {
        let mut marks = self.marks;
        for m in other.marks {
            if !marks
                .iter()
                .any(|k| kelve_core::ideas::norm(&k.name) == kelve_core::ideas::norm(&m.name))
            {
                marks.push(m);
            }
        }
        Kept {
            lens: self.lens.widen(&other.lens),
            marks,
        }
    }
}

/// Marks a file under the folder the person opened.
pub async fn file(app: &Shared, root: &Path, rel: &str, asked: &For) -> Result<Marked> {
    let read = kelve_engine::read::read(root, rel).map_err(|e| Failed::Missing(e.to_string()))?;
    text(app, rel, &read.text, None, asked).await
}

/// Marks a text, by the name of its file: every passage not read before,
/// in one job for the agent, then its marks checked and matched to the
/// graph's ideas in a second. In a change, `only` its added lines are
/// marked. Until the agent answers, what's kept already is what there is.
pub async fn text(
    app: &Shared,
    rel: &str,
    text: &str,
    only: Option<&[Lines]>,
    asked: &For,
) -> Result<Marked> {
    let passages = to_mark(rel, text, only);
    let version = read_version(app).await;
    let (wanted, words) = lens_for(app);
    let mut all: Vec<Mark> = Vec::new();
    let mut todo: Vec<Passage> = Vec::new();
    // What each passage to read had found already, under what it looked
    // for: read again only for what this reader asks for beyond it.
    let mut before: Vec<Option<Kept>> = Vec::new();
    for p in passages {
        match kept_passage(app, &version, &p).await? {
            Some((marks, lens)) if lens.covers(&wanted) => all.extend(marks),
            Some((marks, lens)) => {
                all.extend(marks.iter().cloned());
                before.push(Some(Kept { lens, marks }));
                todo.push(p);
            }
            None => {
                before.push(None);
                todo.push(p);
            }
        }
    }
    if todo.is_empty() {
        return Ok(Marked {
            marks: standing(all, only),
            waiting: None,
        });
    }
    let file = file_of(rel, text);
    let above: Vec<String> = standing(all.clone(), None)
        .into_iter()
        .map(|m| m.name)
        .collect();
    let name = Path::new(rel)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| rel.to_string());
    // A big file is read in parts, each a job of its own, so no job is
    // too long for the agent to be given whole.
    let mut waiting = None;
    for part in parts(&file, todo.into_iter().zip(before).collect()) {
        let (todo, before): (Vec<Passage>, Vec<Option<Kept>>) = part.into_iter().unzip();
        let reading = Reading {
            rel,
            name: &name,
            file: &file,
            above: &above,
            wanted: &wanted,
            words: words.as_deref(),
            version: &version,
        };
        let Some(checked) = read_part(app, &reading, &todo, asked).await? else {
            waiting = Some(Waiting::Agent);
            continue;
        };
        for ((p, marks), had) in todo.iter().zip(&checked).zip(before) {
            let found = Kept {
                lens: wanted.clone(),
                marks: marks.clone(),
            };
            // Kept with what was found before: the passage now serves anyone
            // whose lens either reading covers.
            let kept = match had {
                Some(had) => had.merge(found),
                None => found,
            };
            keep_passage(app, &version, p, &kept).await?;
            all.extend(marks.iter().cloned());
        }
    }
    Ok(Marked {
        marks: standing(all, only),
        waiting,
    })
}

/// The most of a file's lines one read job gives the agent, as characters:
/// with the rest of the job, under the 50,000 Claude Code gives an agent
/// whole, rather than keeping in a file for it to read back.
const PART_MOST: usize = 30_000;

/// A file's passages to read, in parts of at most `PART_MOST` characters,
/// in order: one part for most files, and a passage longer than that on
/// its own.
fn parts<T>(file: &File<'_>, todo: Vec<(Passage, T)>) -> Vec<Vec<(Passage, T)>> {
    let mut out: Vec<Vec<(Passage, T)>> = Vec::new();
    let mut size = 0;
    for (p, had) in todo {
        let long: usize = (p.start..=p.end)
            .filter_map(|n| file.lines.get(n as usize - 1))
            .map(|l| l.len() + 8)
            .sum();
        match out.last_mut() {
            Some(part) if size + long <= PART_MOST => part.push((p, had)),
            _ => {
                out.push(vec![(p, had)]);
                size = 0;
            }
        }
        size += long;
    }
    out
}

/// What reading a file's parts shares.
struct Reading<'a> {
    rel: &'a str,
    name: &'a str,
    file: &'a File<'a>,
    above: &'a [String],
    wanted: &'a Lens,
    words: Option<&'a str>,
    version: &'a str,
}

/// Reads a part of a file's passages: one job for the agent to mark them,
/// then one to check its marks. Each passage's checked marks, or None
/// while either is with the agent.
async fn read_part(
    app: &Shared,
    r: &Reading<'_>,
    todo: &[Passage],
    asked: &For,
) -> Result<Option<Vec<Vec<Mark>>>> {
    let file = r.file;
    let base = key_base(r.rel, r.version, r.wanted, todo);
    let Some(read) = agent::ask(
        app,
        &format!("read:{base}"),
        Ask {
            kind: agent::READ,
            about: format!("Reading {}", r.name),
            idea: None,
            opened: asked.opened,
            material: read_material(file, todo, r.above, r.wanted, r.words),
            schema: read_schema(),
            priority: asked.priority,
            follows: None,
        },
        asked.resume.clone(),
    )
    .await?
    else {
        return Ok(None);
    };
    // Each passage's marks, kept to the rules.
    let mut placed: Vec<(usize, marks::Placed)> = Vec::new();
    for (i, p) in todo.iter().enumerate() {
        let said = said_for(&read.value, &passage_id(i));
        let candidates = candidates_of(file, p);
        for one in placed_of(file, p, &candidates, &said)? {
            placed.push((i, one));
        }
    }
    let mut checked: Vec<Vec<Mark>> = vec![Vec::new(); todo.len()];
    if placed.is_empty() {
        return Ok(Some(checked));
    }
    let flat: Vec<marks::Placed> = placed.iter().map(|(_, p)| p.clone()).collect();
    let key = format!("choose:{base}");
    let (ideas, reached) = offered(app, &flat).await?;
    // Without the graph's ideas, the agent would choose from none, and
    // every mark would be a new idea: the marks wait for the graph.
    if !reached && !agent::answered(app, &key).await? {
        return Err(Failed::Other(
            "the shared graph couldn't be asked for the ideas marks might be".into(),
        ));
    }
    let Some(choice) = agent::ask(
        app,
        &key,
        Ask {
            kind: agent::CHOOSE,
            about: format!("Checking what {} rests on", r.name),
            idea: None,
            opened: asked.opened,
            material: choose_material(file, todo, &placed, &ideas, r.above),
            schema: choose_schema(),
            priority: asked.priority,
            follows: Some(format!("read:{base}")),
        },
        asked.resume.clone(),
    )
    .await?
    else {
        return Ok(None);
    };
    // Read against what the agent was offered, not what's there now.
    let shown = serde_json::from_value::<Vec<Vec<kelve_graph_api::Candidate>>>(
        choice.material["offered"].clone(),
    )
    .unwrap_or(ideas);
    let marks = checked_of(&choice.value, &flat, &shown)?;
    // Each checked mark back to its passage, by where it is.
    for m in marks {
        if let Some(at) = todo
            .iter()
            .position(|p| m.start >= p.start && m.end <= p.end)
        {
            checked[at].push(m);
        }
    }
    Ok(Some(checked))
}

/// What names a set of passages to read: the file, the instructions'
/// version, what's looked for, and each passage's text.
fn key_base(rel: &str, version: &str, lens: &Lens, todo: &[Passage]) -> String {
    let mut hash = Sha256::new();
    hash.update(rel.as_bytes());
    hash.update(version.as_bytes());
    hash.update(lens.print().as_bytes());
    for p in todo {
        hash.update(p.hash.as_bytes());
    }
    hash.finalize()
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The id a passage has in a job: p1, p2, ….
fn passage_id(i: usize) -> String {
    format!("p{}", i + 1)
}

/// A file, ready for its passages to be marked: its lines, imports and
/// outline.
pub(crate) fn file_of<'a>(rel: &'a str, text: &'a str) -> File<'a> {
    let every = kelve_engine::passages::passages(rel, text);
    File {
        rel,
        lines: text.lines().collect(),
        parsed: kelve_engine::candidates::Parsed::new(rel, text),
        imports: kelve_engine::candidates::imports(rel, text),
        outline: every
            .iter()
            .map(|p| match &p.name {
                Some(name) => format!("lines {}–{}: {name}", p.start, p.end),
                None => format!("lines {}–{}", p.start, p.end),
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// The passages of a whole file that are marked: not tests.
pub(crate) fn passages_of(rel: &str, text: &str) -> Vec<Passage> {
    to_mark(rel, text, None)
}

/// A passage's kept marks, lines counted in the file, and what the reading
/// looked for, if it was read under these instructions.
pub(crate) async fn kept_passage(
    app: &Shared,
    version: &str,
    p: &Passage,
) -> Result<Option<(Vec<Mark>, Lens)>> {
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::marks::get(&mut conn, &p.hash, version, BY)
        .await?
        .and_then(|kept| serde_json::from_str::<Kept>(&kept).ok())
        .map(|kept| {
            let marks = kept
                .marks
                .iter()
                .map(|m| m.shifted(i64::from(p.start) - 1))
                .collect();
            (marks, kept.lens)
        }))
}

/// Passages' kept marks, as `kept_passage` gives each, asked for at once.
async fn kept_passages(
    app: &Shared,
    version: &str,
    passages: &[Passage],
) -> Result<Vec<Option<(Vec<Mark>, Lens)>>> {
    let hashes: Vec<&str> = passages.iter().map(|p| p.hash.as_str()).collect();
    let mut conn = app.db.acquire().await?;
    let found = kelve_store::marks::get_each(&mut conn, &hashes, version, BY).await?;
    Ok(passages
        .iter()
        .map(|p| {
            let kept = serde_json::from_str::<Kept>(found.get(&p.hash)?).ok()?;
            let marks = kept
                .marks
                .iter()
                .map(|m| m.shifted(i64::from(p.start) - 1))
                .collect();
            Some((marks, kept.lens))
        })
        .collect())
}

/// What in a passage a mark can point at.
pub(crate) fn candidates_of(file: &File<'_>, p: &Passage) -> Vec<Candidate> {
    match p.kind {
        Kind::Code => file.parsed.within(p.start, p.end),
        Kind::Prose => Vec::new(),
    }
}

/// The marks the agent proposed for a passage, kept to the rules.
pub(crate) fn placed_of(
    file: &File<'_>,
    p: &Passage,
    candidates: &[Candidate],
    said: &[Said],
) -> Result<Vec<marks::Placed>> {
    Ok(marks::keep(
        marks::place(&file.lines, p.start, p.end, candidates, said),
        p.end + 1 - p.start,
    ))
}

/// Keeps a passage's marks with lines counted from its first, so it keeps
/// them wherever it moves, with what the reading looked for.
pub(crate) async fn keep_passage(
    app: &Shared,
    version: &str,
    p: &Passage,
    kept: &Kept,
) -> Result<()> {
    let relative = Kept {
        lens: kept.lens.clone(),
        marks: kept
            .marks
            .iter()
            .map(|m| m.shifted(1 - i64::from(p.start)))
            .collect(),
    };
    let mut conn = app.db.acquire().await?;
    kelve_store::marks::put(
        &mut conn,
        &p.hash,
        version,
        BY,
        &serde_json::to_string(&relative).unwrap_or_else(|_| "{}".into()),
    )
    .await?;
    Ok(())
}

/// A passage's lines, numbered as the file numbers them, no wider than
/// the file's last number.
fn numbered(file: &File<'_>, p: &Passage) -> String {
    let w = file.lines.len().to_string().len();
    (p.start..=p.end)
        .filter_map(|n| {
            file.lines
                .get(n as usize - 1)
                .map(|l| format!("{n:>w$}  {l}\n"))
        })
        .collect()
}

/// How many lines either side of a mark the check is shown.
const AROUND: u32 = 3;

/// A passage as its marks are checked: its first line, and each mark's
/// lines with a few either side, `…` where lines are left out; the whole
/// passage when that's most of it.
fn windowed(file: &File<'_>, p: &Passage, marks: &[&Mark]) -> String {
    let mut shown = std::collections::BTreeSet::from([p.start]);
    for m in marks {
        let from = m.start.saturating_sub(AROUND).max(p.start);
        let to = (m.end + AROUND).min(p.end);
        shown.extend(from..=to);
    }
    let all = p.end + 1 - p.start;
    if u32::try_from(shown.len())
        .unwrap_or(u32::MAX)
        .saturating_mul(5)
        >= all * 4
    {
        return numbered(file, p);
    }
    let w = file.lines.len().to_string().len();
    let mut out = String::new();
    let mut last: Option<u32> = None;
    for n in shown {
        if last.is_some_and(|l| n > l + 1) {
            out.push_str("…\n");
        }
        if let Some(l) = file.lines.get(n as usize - 1) {
            out.push_str(&format!("{n:>w$}  {l}\n"));
        }
        last = Some(n);
    }
    if last.is_some_and(|l| l < p.end) {
        out.push_str("…\n");
    }
    out
}

/// What the agent is given to read a file's passages.
fn read_material(
    file: &File<'_>,
    todo: &[Passage],
    above: &[String],
    lens: &Lens,
    words: Option<&str>,
) -> Value {
    let passages: Vec<Value> = todo
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let candidates = candidates_of(file, p);
            let listed: String = candidates
                .iter()
                .map(|c| format!("{} line {}: {}\n", c.id, c.line, c.text))
                .collect();
            json!({
                "id": passage_id(i),
                "lines": format!("{} to {}", p.start, p.end),
                "most": marks::most_for(p.end + 1 - p.start),
                "passage": fence("passage", &numbered(file, p)),
                "candidates": if listed.is_empty() { String::new() } else { fence("candidates", &listed) },
            })
        })
        .collect();
    json!({
        "file": file.rel,
        "imports": if file.imports.is_empty() { String::new() } else { fence("imports", &file.imports.join("\n")) },
        "outline": fence("outline", &file.outline),
        "alreadyMarked": above,
        "lookFor": lens.says(),
        "words": words.map(|w| fence("words", w)).unwrap_or_default(),
        "passages": passages,
    })
}

fn said_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "candidate": { "type": "string" },
            "line": { "type": "integer" },
            "quote": { "type": "string" },
            "lastLine": { "type": "integer" },
            "name": { "type": "string" },
            "definition": { "type": "string" },
            "use": { "type": "string" },
            "area": { "type": "string" },
            "depth": { "type": "string", "enum": ["basic", "common", "advanced", "specialist"] },
        },
        // Only the pointer used: the others are left out.
        "required": ["name", "definition", "use", "area", "depth"],
        "additionalProperties": false,
    })
}

/// The shape of a read job's answer: each passage's marks, by its id.
pub fn read_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "passages": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "passage": { "type": "string", "description": "The passage's id: p1, p2, …" },
                        "marks": { "type": "array", "items": said_schema() },
                    },
                    "required": ["passage", "marks"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["passages"],
        "additionalProperties": false,
    })
}

#[derive(Deserialize)]
struct ReadAnswer {
    passages: Vec<PassageAnswer>,
}

#[derive(Deserialize)]
struct PassageAnswer {
    passage: String,
    #[serde(default)]
    marks: Vec<Said>,
}

/// Whether a read job's answer is in its shape, for these passages.
pub fn read_shape(material: &Value, value: &Value) -> std::result::Result<(), String> {
    let answer: ReadAnswer = serde_json::from_value(value.clone()).map_err(|e| {
        format!("It isn't in the shape asked for: {e}. Give passages, each with its id and marks.")
    })?;
    let ids: Vec<&str> = material["passages"]
        .as_array()
        .map(|ps| ps.iter().filter_map(|p| p["id"].as_str()).collect())
        .unwrap_or_default();
    if let Some(stray) = answer
        .passages
        .iter()
        .find(|p| !ids.contains(&p.passage.as_str()))
    {
        return Err(format!(
            "There's no passage {} in this job. Use the ids it gives: {}.",
            stray.passage,
            ids.join(", ")
        ));
    }
    Ok(())
}

/// What the agent said for one passage, by its id: nothing, if it left it
/// out.
fn said_for(value: &Value, id: &str) -> Vec<Said> {
    serde_json::from_value::<ReadAnswer>(value.clone())
        .ok()
        .and_then(|a| a.passages.into_iter().find(|p| p.passage == id))
        .map(|p| p.marks)
        .unwrap_or_default()
}

/// The ideas the shared graph has for each mark's name, and whether the
/// graph could be asked. Without a graph, there are none.
pub(crate) async fn graph_candidates(
    app: &Shared,
    placed: &[marks::Placed],
) -> (Vec<Vec<kelve_graph_api::Candidate>>, bool) {
    let none = vec![Vec::new(); placed.len()];
    let Some(graph) = app.graph_reader() else {
        return (none, true);
    };
    let mut req = MatchRequest::default();
    for (i, p) in placed.iter().enumerate() {
        if kelve_core::redact::query_ok(&p.mark.name, &[]).is_err() {
            continue;
        }
        req.queries.insert(
            i.to_string(),
            Query {
                query: p.mark.name.clone(),
                context: None,
                limit: Some(5),
            },
        );
    }
    if req.queries.is_empty() {
        return (none, true);
    }
    match graph.candidates(&req).await {
        Ok(found) => (
            (0..placed.len())
                .map(|i| {
                    found
                        .get(&i.to_string())
                        .map(|c| c.result.clone())
                        .unwrap_or_default()
                })
                .collect(),
            true,
        ),
        Err(e) => {
            tracing::info!(error = %e, "couldn't ask the shared graph; marks wait for their ideas");
            (none, false)
        }
    }
}

/// What each mark might be: the shared graph's ideas for its name, and
/// kelve's own new ideas with a name like it, which the graph hasn't taken
/// in yet; and whether the graph could be asked.
async fn offered(
    app: &Shared,
    placed: &[marks::Placed],
) -> Result<(Vec<Vec<kelve_graph_api::Candidate>>, bool)> {
    let (mut ideas, reached) = graph_candidates(app, placed).await;
    let mut conn = app.db.acquire().await?;
    let own = kelve_store::entries::new_ideas(&mut conn).await?;
    drop(conn);
    for (listed, p) in ideas.iter_mut().zip(placed) {
        let itself = crate::pipeline::entries::new_key(&p.mark.name);
        let mut near: Vec<(usize, &kelve_store::entries::Idea)> = own
            .iter()
            .filter(|i| i.key != itself && !listed.iter().any(|l| l.name == i.name))
            .map(|i| (kelve_core::ideas::likeness(&p.mark.name, &i.name), i))
            .filter(|(alike, _)| *alike > 0)
            .collect();
        near.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        listed.extend(
            near.into_iter()
                .take(3)
                .map(|(_, i)| kelve_graph_api::Candidate {
                    id: i.key.clone(),
                    name: i.name.clone(),
                    definition: i.definition.clone(),
                    score: 0.0,
                    matched: false,
                    same_as: None,
                }),
        );
    }
    Ok((ideas, reached))
}

/// What the agent is given to check a file's marks and choose each one's
/// idea in the graph.
fn choose_material(
    file: &File<'_>,
    todo: &[Passage],
    placed: &[(usize, marks::Placed)],
    ideas: &[Vec<kelve_graph_api::Candidate>],
    above: &[String],
) -> Value {
    // Each passage with marks, around its marks.
    let mut passages: Vec<String> = Vec::new();
    for (i, p) in todo.iter().enumerate() {
        let marks: Vec<&Mark> = placed
            .iter()
            .filter(|(at, _)| *at == i)
            .map(|(_, placed)| &placed.mark)
            .collect();
        if !marks.is_empty() {
            passages.push(fence_with(
                "passage",
                &[("id", &passage_id(i))],
                &windowed(file, p, &marks),
            ));
        }
    }
    let marks: Vec<String> = placed
        .iter()
        .enumerate()
        .map(|(n, (_, placed))| {
            let m = &placed.mark;
            let at = match &m.span {
                Some(s) => {
                    let words: String = file
                        .lines
                        .get(s.line as usize - 1)
                        .map(|l| {
                            l.chars()
                                .skip(s.from as usize)
                                .take((s.to - s.from) as usize)
                                .collect()
                        })
                        .unwrap_or_default();
                    format!("“{words}”, on line {}", s.line)
                }
                None => format!("lines {} to {}", m.start, m.end),
            };
            let mut body = format!(
                "Marked: {at}\nDefinition: {}\nHow the passage uses it: {}\nIdeas in the shared graph:\n",
                m.definition, m.use_
            );
            let listed = ideas.get(n).map(Vec::as_slice).unwrap_or_default();
            if listed.is_empty() {
                body.push_str("none\n");
            }
            for (k, idea) in listed.iter().enumerate() {
                let own = if idea.id.starts_with("new:") {
                    " (marked here before, not in the shared graph yet)"
                } else {
                    ""
                };
                body.push_str(&format!("{}. {}{own}: {}\n", k + 1, idea.name, idea.definition));
            }
            fence_with(
                "mark",
                &[("number", &(n + 1).to_string()), ("name", &m.name)],
                &body,
            )
        })
        .collect();
    json!({
        "file": file.rel,
        "alreadyMarked": above,
        "passages": passages,
        "marks": marks,
        // What each mark was offered, by number, for reading the answer:
        // kelve's own, never given to the agent, who has them in `marks`.
        "offered": ideas,
    })
}

/// The shape of a choose job's answer: a check of each mark, by number.
pub fn choose_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "checks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "mark": { "type": "integer" },
                        "named": { "type": "boolean" },
                        "instance": { "type": "boolean" },
                        "matters": { "type": "boolean" },
                        "idea": { "type": "integer" },
                    },
                    "required": ["mark", "named", "instance", "matters", "idea"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["checks"],
        "additionalProperties": false,
    })
}

#[derive(Deserialize)]
struct Checked {
    mark: usize,
    named: bool,
    instance: bool,
    matters: bool,
    idea: usize,
}

/// Whether a choose job's answer is in its shape.
pub fn choose_shape(value: &Value) -> std::result::Result<(), String> {
    serde_json::from_value::<Vec<Checked>>(value["checks"].clone())
        .map(|_| ())
        .map_err(|e| {
            format!("It isn't in the shape asked for: {e}. Give checks, one for each mark by its number.")
        })
}

/// The marks that pass their check, each with the idea chosen for it.
pub(crate) fn checked_of(
    value: &Value,
    placed: &[marks::Placed],
    ideas: &[Vec<kelve_graph_api::Candidate>],
) -> Result<Vec<Mark>> {
    let checks: Vec<Checked> = serde_json::from_value(value["checks"].clone())
        .map_err(|e| Failed::Other(format!("a choose answer doesn't read: {e}")))?;
    let mut out = Vec::new();
    for (i, placed) in placed.iter().enumerate() {
        // A mark the check doesn't answer for isn't shown.
        let Some(c) = checks.iter().find(|c| c.mark == i + 1) else {
            continue;
        };
        if !(c.named && c.instance && c.matters) {
            continue;
        }
        let mut mark = placed.mark.clone();
        let listed = ideas.get(i).map(Vec::as_slice).unwrap_or_default();
        // A number it wasn't offered is no choice. One of kelve's own new
        // ideas is chosen by taking its name, so the mark leads to it.
        let chosen = c.idea.checked_sub(1).and_then(|n| listed.get(n));
        match chosen {
            Some(idea) if idea.id.starts_with("new:") => {
                mark.name = idea.name.clone();
                mark.idea = None;
            }
            _ => {
                mark.idea = chosen.map(|idea| IdeaRef {
                    id: idea.id.clone(),
                    name: idea.name.clone(),
                });
            }
        }
        out.push(mark);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark_at(start: u32, end: u32) -> Mark {
        Mark {
            start,
            end,
            span: None,
            name: "Retries".into(),
            definition: String::new(),
            idea: None,
            use_: String::new(),
            area: String::new(),
            depth: Default::default(),
        }
    }

    #[test]
    fn a_mark_is_checked_with_the_lines_around_it_not_its_whole_passage() {
        let mut text = String::from("pub fn long() {\n");
        for n in 2..=59 {
            text.push_str(&format!("    let x{n} = {n};\n"));
        }
        text.push_str("}\n");
        let file = file_of("src/long.rs", &text);
        let p = passages_of("src/long.rs", &text).remove(0);
        assert_eq!((p.start, p.end), (1, 60));
        let m = mark_at(30, 31);
        let shown = windowed(&file, &p, &[&m]);
        let lines: Vec<&str> = shown.lines().collect();
        // Its first line, then the mark's with three either side, `…`
        // where lines are left out; numbers no wider than the last.
        assert_eq!(lines[0], " 1  pub fn long() {");
        assert_eq!(lines[1], "…");
        assert_eq!(lines[2], "27      let x27 = 27;");
        assert_eq!(lines[9], "34      let x34 = 34;");
        assert_eq!(lines[10], "…");
        assert_eq!(lines.len(), 11);
        // A mark over most of the passage is shown it whole.
        let whole = windowed(&file, &p, &[&mark_at(2, 58)]);
        assert_eq!(whole, numbered(&file, &p));
    }

    #[test]
    fn a_big_file_is_read_in_parts_each_short_enough_to_be_given_whole() {
        let mut text = String::new();
        for n in 0..400 {
            text.push_str(&format!("pub fn f{n}() {{\n"));
            for l in 0..8 {
                text.push_str(&format!(
                    "    let value_{l} = compute_something_long({n}, {l});\n"
                ));
            }
            text.push_str("}\n\n");
        }
        let file = file_of("src/big.rs", &text);
        let todo: Vec<(Passage, ())> = passages_of("src/big.rs", &text)
            .into_iter()
            .map(|p| (p, ()))
            .collect();
        let all = todo.len();
        let split = parts(&file, todo);
        assert!(split.len() > 1, "one part for {all} passages");
        assert_eq!(split.iter().map(Vec::len).sum::<usize>(), all);
        for part in &split {
            let long: usize = part.iter().map(|(p, _)| numbered(&file, p).len()).sum();
            assert!(long <= PART_MOST, "a part of {long} characters");
        }
        // A small file is one part.
        let small = "pub fn a() {}\n";
        let file = file_of("src/a.rs", small);
        let todo: Vec<(Passage, ())> = passages_of("src/a.rs", small)
            .into_iter()
            .map(|p| (p, ()))
            .collect();
        assert_eq!(parts(&file, todo).len(), 1);
    }

    #[test]
    fn a_read_answer_names_only_the_jobs_passages() {
        let material = json!({ "passages": [{ "id": "p1" }, { "id": "p2" }] });
        assert!(read_shape(&material, &json!({ "passages": [] })).is_ok());
        assert!(
            read_shape(
                &material,
                &json!({ "passages": [{ "passage": "p2", "marks": [] }] })
            )
            .is_ok()
        );
        let stray = read_shape(
            &material,
            &json!({ "passages": [{ "passage": "p9", "marks": [] }] }),
        )
        .unwrap_err();
        assert!(stray.contains("p1, p2"), "{stray}");
        assert!(read_shape(&material, &json!({ "marks": [] })).is_err());
        assert!(choose_shape(&json!({ "checks": [] })).is_ok());
        assert!(choose_shape(&json!({ "checks": [{ "mark": 1 }] })).is_err());
    }
}
