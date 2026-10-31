//! An idea's entry: taken from the shared graph when it has one, or written by
//! the person's own agent, then sent to the graph, which checks it claim by
//! claim before anyone else reads it.
//!
//! Writing goes: the agent finds its own pages and writes the entry, with
//! the words on each page that support each claim → kelve builds it from
//! the agent's answer and shows it to the person at once, marked as not
//! yet checked → sent to the graph, if the person adds to it → the graph
//! checks it, and kelve follows the check, claim by claim → accepted, it's
//! everyone's; turned away for claims its pages don't support, the agent
//! is asked to fix them, once; turned away again, it's dropped.
//!
//! An entry the graph has grows and is put right a block at a time
//! (`kelve_core::entry::revise`). When a person who adds to the graph opens
//! one its checks found wrong, their agent fixes only the blocks found
//! wrong, or, when the entry is poor or mostly wrong, writes it again;
//! when they open a sound one that readers, or the graph's grade, ask more
//! of, their agent extends it, touching nothing there. Each is sent against
//! the revision it was written from, with what it changes.
//!
//! kelve checks nothing about an entry itself: the graph does (Nathan, 19
//! October). An entry is written from the idea and the web alone, never from
//! anything the person opened, so it can't carry their work into the graph.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::Utc;
use kelve_core::entry::draft::{self, Claim, DraftLink, Quote};
use kelve_core::entry::revise::{self, RevisionKind};
use kelve_core::fence::fence;
use kelve_core::ideas::{IdeaKind, IdeaRef, is_id, norm, tidy};
use kelve_core::text::clip;
use kelve_core::{Entry, Timestamp};
use kelve_graph_api::client::Error as GraphError;
use kelve_graph_api::{
    Change, Changeset, CheckStatus, ClaimState, Dropped, IdeaAsk, MatchRequest, Query as Lookup,
    Revise, WrittenBy,
};
use kelve_store::entries::Idea;
use kelve_store::sends::Sent;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{Failed, Result, Then};
use crate::agent::{self, Priority};
use crate::state::Shared;

pub const WRITE: &str = "entry.write";
pub const SEND: &str = "entry.send";
pub const FOLLOW: &str = "entry.follow";
pub const FIX: &str = "entry.fix";
pub const REFRESH: &str = "entry.refresh";
/// Fixing or extending an entry the graph has, a block at a time.
pub const REVISE: &str = "entry.revise";

/// Fixes after the graph's checks turn an entry away, before it's dropped.
const FIXES: u32 = 1;

/// How long kelve follows the graph's check of an entry before it stops
/// asking: it asks again when the idea is next opened.
const FOLLOW_FOR_MS: i64 = 2 * 24 * 60 * 60 * 1000;

/// What kelve keeps with an entry while it's written and checked.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Plan {
    /// The one question it answers, from its relations.
    pub question: String,
    /// How it relates, each with why, from its relations: the entry is
    /// written from them.
    pub relations: Vec<String>,
    /// Ideas the graph has that the entry is likely to link: `Name: what
    /// it is`.
    pub known: Vec<String>,
    /// The pages the agent wrote it from, by address.
    pub pages: Vec<Page>,
    /// What the graph's checks found, for the fix.
    pub problems: Vec<String>,
    /// Fixes asked for after the graph's checks.
    pub revisions: u32,
    /// Kept so the reader's notion of a revision still reads.
    pub claim_revisions: u32,
    /// The agent's last answer, as it gave it: what a fix corrects.
    pub draft: Value,
    /// When it replaces an entry the graph's checks found wrong: that
    /// entry, as markdown, and what they found.
    pub replacing: String,
    pub wrong: Vec<String>,
    /// The agent that wrote it, and the model it said it ran on.
    pub agent: Option<String>,
    pub model: Option<String>,
    /// When kelve started following the graph's check of it.
    pub following_since: Option<i64>,
    /// When it changes an entry the graph has: how, and from what.
    pub base: Option<Base>,
    /// The checked claims it leaves out, by key, each with why.
    pub dropped: Vec<Dropped>,
    /// Why each block it removes goes, by id.
    pub whys: BTreeMap<String, String>,
}

/// An entry the graph has, as a revision is written against it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Base {
    pub kind: RevisionKind,
    /// The revision it's written against.
    pub revision: String,
    pub entry: Entry,
    /// Its claims, as the graph serves them: each with its key, where it's
    /// made, and whether the checks since found it wrong.
    #[serde(default)]
    pub claims: Vec<kelve_graph_api::Claim>,
    /// The blocks its problems are in.
    #[serde(default)]
    pub faulted: Vec<String>,
    /// For an extend: what it was asked to add.
    #[serde(default)]
    pub ask: Option<IdeaAsk>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Page {
    pub url: String,
    pub passages: Vec<String>,
}

/// The key a new idea is kept under until the graph has it.
pub fn new_key(name: &str) -> String {
    format!("new:{}", norm(name))
}

async fn idea(app: &Shared, key: &str) -> Result<Option<Idea>> {
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::entries::idea(&mut conn, key).await?)
}

async fn entry(app: &Shared, key: &str) -> Result<Option<kelve_store::entries::Entry>> {
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::entries::get(&mut conn, key).await?)
}

async fn set(app: &Shared, key: &str, state: &str, step: &str) -> Result<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::entries::set(&mut conn, key, state, step).await?;
    drop(conn);
    told(app, key);
    Ok(())
}

/// Tells the reader an idea's entry moved on.
fn told(app: &Shared, key: &str) {
    app.publish(kelve_core::views::ServerEvent::Idea {
        key: key.to_string(),
    });
}

fn plan_of(row: &kelve_store::entries::Entry) -> Plan {
    row.plan
        .as_deref()
        .and_then(|p| serde_json::from_str(p).ok())
        .unwrap_or_default()
}

async fn keep_plan(app: &Shared, key: &str, plan: &Plan) -> Result<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::entries::set_plan(
        &mut conn,
        key,
        &serde_json::to_string(plan).unwrap_or_default(),
    )
    .await?;
    Ok(())
}

async fn next(app: &Shared, kind: &str, key: &str) -> Result<()> {
    next_at(app, kind, key, Utc::now()).await
}

async fn next_at(app: &Shared, kind: &str, key: &str, at: chrono::DateTime<Utc>) -> Result<()> {
    super::add(
        app,
        kind,
        Some(&format!("{kind}:{key}")),
        json!({ "idea": key }),
        at,
    )
    .await
}

/// Asks for an idea's entry, when someone opens it: taken from the graph
/// when it has one, else kept for the person's agent to write. Nothing
/// happens when it's under way or done.
pub async fn want(app: &Shared, wanted: &Idea) -> Result<()> {
    {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::keep_idea(&mut conn, wanted).await?;
        if let Some(e) = kelve_store::entries::get(&mut conn, &wanted.key).await?
            && !matches!(e.state.as_str(), "dropped" | "waiting")
        {
            return Ok(());
        }
    }
    // One the graph found to be two isn't written: the reader offers the two.
    if !super::splits::of(app, &wanted.key).await?.is_empty()
        || super::splits::ask(app, wanted).await?
    {
        return Ok(());
    }
    if from_graph(app, wanted).await? {
        // An entry the graph has, for an idea whose relations it hasn't:
        // they're traced on their own.
        super::relations::ensure(app, wanted).await?;
        // Opened now: what it's asked to add is seen at once, for a person
        // who adds to the graph.
        if app.settings().contribute && app.graph_writer().is_some() {
            next(app, REFRESH, &wanted.key).await?;
        }
        return Ok(());
    }
    let started = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::start(&mut conn, &wanted.key).await?
    };
    if started {
        told(app, &wanted.key);
        // How it relates comes first, and the entry is written from it.
        if super::relations::needed(app, wanted).await? {
            super::relations::ensure(app, wanted).await?;
        } else {
            next(app, WRITE, &wanted.key).await?;
        }
    }
    Ok(())
}

/// Takes an idea's entry ahead, before anyone asks, for an idea the person
/// is likely to open next, when the shared graph has one: that costs
/// nothing, and opening it is then at once. One the graph hasn't got is
/// written only when someone opens its idea.
pub async fn take_ahead(app: &Shared, idea: &Idea) -> Result<()> {
    {
        let mut conn = app.db.acquire().await?;
        if kelve_store::entries::get(&mut conn, &idea.key)
            .await?
            .is_some()
        {
            return Ok(());
        }
        kelve_store::entries::keep_idea(&mut conn, idea).await?;
    }
    from_graph(app, idea).await?;
    Ok(())
}

/// Takes an idea's entry from the shared graph, when the graph has one:
/// whether it did.
async fn from_graph(app: &Shared, wanted: &Idea) -> Result<bool> {
    let graph_id = if is_id(&wanted.key) {
        Some(wanted.key.as_str())
    } else {
        wanted.became.as_deref()
    };
    let (Some(graph), Some(id)) = (app.graph_reader(), graph_id) else {
        return Ok(false);
    };
    // How often it's been read is the idea's, not the revision's, which
    // never changes: asked for beside it.
    let ids = [id.to_string()];
    let (page, idea) = tokio::join!(graph.entry(id, None), graph.ideas(&ids));
    match page {
        Ok(Some(page)) => {
            let reads = idea
                .ok()
                .and_then(|found| found.ideas.into_iter().find(|i| i.id == id))
                .map_or(0, |i| i.reads);
            let mut conn = app.db.acquire().await?;
            kelve_store::entries::taken(
                &mut conn,
                &wanted.key,
                &serde_json::to_string(&page.entry).unwrap_or_default(),
                &page.revision,
            )
            .await?;
            kelve_store::entries::set_reads(
                &mut conn,
                &wanted.key,
                i64::try_from(reads).unwrap_or(i64::MAX),
            )
            .await?;
            drop(conn);
            told(app, &wanted.key);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(e) => {
            tracing::info!(error = %e, "couldn't ask the shared graph for an entry; the agent writes it");
            Ok(false)
        }
    }
}

/// How long a ready entry is kept before the graph is asked about it again.
pub const FRESH: Duration = Duration::from_secs(60 * 60);

/// Asks the shared graph about a ready entry, as a job: a newer revision is
/// taken; problems its checks found with the one kept are kept, and, when the
/// person adds to the graph, their agent writes one to replace it. The entry is
/// shown until a replacement is ready.
pub async fn refresh(app: &Shared, key: &str) -> Result<Then> {
    let (Some(row), Some(idea), Some(graph)) = (
        entry(app, key).await?,
        idea(app, key).await?,
        app.graph_reader(),
    ) else {
        return Ok(Then::Done);
    };
    let id = if is_id(key) {
        key.to_string()
    } else if let Some(id) = idea.became {
        id
    } else {
        return Ok(Then::Done);
    };
    if row.state != "ready" || row.revision.is_none() {
        return Ok(Then::Done);
    }
    let found = graph
        .ideas(std::slice::from_ref(&id))
        .await
        .map_err(|e| Failed::Other(e.to_string()))?;
    if let Some(into) = found.split.get(&id) {
        super::splits::found(app, key, &id, into).await?;
        return Ok(Then::Done);
    }
    let id = found.redirects.get(&id).cloned().unwrap_or(id);
    let Some(now) = found.ideas.into_iter().find(|i| i.id == id) else {
        return Ok(Then::Done);
    };
    let mut conn = app.db.acquire().await?;
    kelve_store::entries::set_reads(&mut conn, key, i64::try_from(now.reads).unwrap_or(i64::MAX))
        .await?;
    match &now.revision {
        // A newer revision: taken, in place of the one kept.
        Some(revision) if Some(revision) != row.revision.as_ref() => {
            drop(conn);
            let Some(page) = graph
                .entry(&id, Some(revision))
                .await
                .map_err(|e| Failed::Other(e.to_string()))?
            else {
                return Ok(Then::Done);
            };
            let mut conn = app.db.acquire().await?;
            kelve_store::entries::taken(
                &mut conn,
                key,
                &serde_json::to_string(&page.entry).unwrap_or_default(),
                &page.revision,
            )
            .await?;
        }
        _ => {
            let problems = (!now.problems.is_empty())
                .then(|| serde_json::to_string(&now.problems).unwrap_or_default());
            kelve_store::entries::checked(&mut conn, key, problems.as_deref()).await?;
            drop(conn);
            // Changed only to send: a person who doesn't add to the graph
            // sees what's wrong, and the entry stays as it is.
            let shown: Option<Entry> = row
                .entry
                .as_deref()
                .and_then(|e| serde_json::from_str(e).ok());
            if app.settings().contribute
                && app.graph_writer().is_some()
                && let (Some(shown), Some(revision)) = (shown, row.revision.as_deref())
            {
                if !now.problems.is_empty() {
                    // Fixed a block at a time, unless it's poor or mostly
                    // wrong. Problems the graph can't place are anywhere.
                    let faulted: Vec<String> = if now.faulted.is_empty() {
                        shown.blocks.iter().map(|b| b.id.clone()).collect()
                    } else {
                        now.faulted.clone()
                    };
                    let kind = if revise::rewrite_allowed(&shown, &faulted) {
                        RevisionKind::Rewrite
                    } else {
                        RevisionKind::Fix
                    };
                    let base = Base {
                        kind,
                        revision: revision.to_string(),
                        entry: shown,
                        claims: Vec::new(),
                        faulted: now.faulted.clone(),
                        ask: None,
                    };
                    start_revision(app, key, &id, base).await?;
                } else if let Some(ask) = now.asks.first() {
                    // A sound entry grows by what's asked; a poor one, one
                    // written before entries had parts, is written again.
                    let kind = if revise::poor(&shown) {
                        RevisionKind::Rewrite
                    } else {
                        RevisionKind::Extend
                    };
                    let base = Base {
                        kind,
                        revision: revision.to_string(),
                        entry: shown,
                        claims: Vec::new(),
                        faulted: Vec::new(),
                        ask: Some(ask.clone()),
                    };
                    start_revision(app, key, &id, base).await?;
                }
            }
        }
    }
    told(app, key);
    Ok(Then::Done)
}

/// The kind of job that writes a kind of revision.
fn job_kind(kind: RevisionKind) -> &'static str {
    match kind {
        RevisionKind::Fix => agent::FIX,
        RevisionKind::Extend => agent::EXTEND,
        RevisionKind::Rewrite => agent::WRITE,
    }
}

/// The key the agent's job for a revision against `revision` is kept
/// under: one job for each base, and for an extend, each kind of ask.
fn revision_key(version: &str, key: &str, base: &Base) -> String {
    match base.kind {
        RevisionKind::Rewrite => format!("write:{version}:{key}:{}", base.revision),
        RevisionKind::Fix => format!("fix:{version}:{key}:base:{}", base.revision),
        RevisionKind::Extend => format!(
            "extend:{version}:{key}:{}:{}",
            base.revision,
            base.ask
                .as_ref()
                .map(|a| a.kind.as_str())
                .unwrap_or_default()
        ),
    }
}

/// Starts a revision of an entry the graph has, for the person's agent
/// to write: unless one was written against the same revision before, so
/// a revision the graph turned away isn't written over and over.
async fn start_revision(app: &Shared, key: &str, id: &str, mut base: Base) -> Result<()> {
    let (kind, revision) = (base.kind, base.revision.clone());
    let version = agent::version(app, job_kind(kind)).await;
    {
        let mut conn = app.db.acquire().await?;
        if kelve_store::asks::by_key(&mut conn, &revision_key(&version, key, &base))
            .await?
            .is_some()
        {
            return Ok(());
        }
    }
    // Its claims, each with its key: what the revision must keep.
    if let Some(graph) = app.graph_reader() {
        match graph.entry(id, Some(&revision)).await {
            Ok(Some(page)) => {
                // Those in the blocks found wrong may go without a word.
                base.claims = page.claims;
                for c in &mut base.claims {
                    c.faulted |= base.faulted.contains(&c.block);
                }
            }
            Ok(None) => return Ok(()),
            Err(e) => return Err(Failed::Other(e.to_string())),
        }
    }
    let started = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::revise(&mut conn, key).await?
    };
    if !started {
        return Ok(());
    }
    keep_plan(
        app,
        key,
        &Plan {
            base: Some(base),
            ..Plan::default()
        },
    )
    .await?;
    told(app, key);
    next(
        app,
        if kind == RevisionKind::Rewrite {
            WRITE
        } else {
            REVISE
        },
        key,
    )
    .await
}

/// How an idea relates, a line each with why, for the writer.
fn described(r: &kelve_graph_api::IdeaRelations) -> Vec<String> {
    let mut lines = Vec::new();
    let mut say = |what: &str, rels: &[kelve_graph_api::Rel]| {
        for rel in rels {
            lines.push(format!("{what} {}: {}", rel.idea.name, rel.why));
        }
    };
    say("Rests on", &r.down);
    say("A kind of", &r.kind_of);
    say("Answers", &r.answers);
    say("An alternative:", &r.alternatives);
    say("Used with", &r.used_with);
    lines
}

/// The ideas the graph has among those an idea relates to, for the writer
/// to link to by their names.
async fn known(app: &Shared, r: &kelve_graph_api::IdeaRelations) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for rel in r
        .down
        .iter()
        .chain(&r.kind_of)
        .chain(&r.answers)
        .chain(&r.alternatives)
        .chain(&r.used_with)
    {
        // What the idea is, as kelve keeps it, else why it relates.
        let definition = match app.db.acquire().await {
            Ok(mut conn) => kelve_store::entries::idea(&mut conn, &rel.idea.id)
                .await
                .ok()
                .flatten()
                .map(|i| i.definition)
                .filter(|d| !d.trim().is_empty()),
            Err(_) => None,
        };
        let line = format!(
            "{}: {}",
            rel.idea.name,
            definition.as_deref().unwrap_or(&rel.why)
        );
        if !out.contains(&line) {
            out.push(line);
        }
    }
    if out.len() >= 4 {
        return out;
    }
    let Some(graph) = app.graph_reader() else {
        return out;
    };
    let mut req = MatchRequest::default();
    for (i, rel) in r.down.iter().chain(&r.alternatives).take(12).enumerate() {
        req.queries.insert(
            i.to_string(),
            Lookup {
                query: rel.idea.name.clone(),
                context: None,
                limit: Some(2),
            },
        );
    }
    if req.queries.is_empty() {
        return out;
    }
    if let Ok(found) = graph.candidates(&req).await {
        for c in found.into_values().flat_map(|c| c.result) {
            let line = format!("{}: {}", c.name, c.definition);
            if !out.contains(&line) {
                out.push(line);
            }
        }
    }
    out
}

/// What the agent is given to write an idea's entry.
pub fn material(idea: &Idea, plan: &Plan, today: &str) -> Value {
    let said = |lines: &[String]| {
        if lines.is_empty() {
            String::new()
        } else {
            fence("said", &lines.join("\n"))
        }
    };
    json!({
        "today": today,
        "idea": idea_said(idea),
        "question": plan.question,
        "relations": said(&plan.relations),
        "ideasTheGraphHas": said(&plan.known),
        "replacing": if plan.replacing.is_empty() {
            Value::Null
        } else {
            json!({
                "entry": fence("entry", &plan.replacing),
                "found": plan.wrong,
                "claims": plan.base.as_ref().map(|b| fence("claims", &numbered(&b.claims))),
                "ask": plan.base.as_ref().and_then(|b| b.ask.as_ref()).map(|a| a.kind.says(a.part)),
            })
        },
    })
}

/// The idea a job is about, fenced, as the agent is given it.
fn idea_said(idea: &Idea) -> String {
    fence(
        "said",
        &format!(
            "The idea: {}\nWhat it is: {}\nWhat kind of thing it is: {}",
            idea.name, idea.definition, idea.kind
        ),
    )
}

/// An entry's claims, numbered, each with the block that makes it, and
/// whether the checks since found it wrong.
fn numbered(claims: &[kelve_graph_api::Claim]) -> String {
    claims
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let cites = if c.sources.is_empty() {
                "no pages".to_string()
            } else {
                format!(
                    "cites {}",
                    c.sources
                        .iter()
                        .map(|n| format!("^{n}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };
            format!(
                "{}. [{}] {} ({cites}){}",
                i + 1,
                if c.block.is_empty() {
                    "a figure"
                } else {
                    c.block.as_str()
                },
                c.text,
                if c.faulted { " — found wrong" } else { "" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Claims as the graph's API has them, each with the block that makes it.
fn to_graph(claims: Vec<Claim>) -> Vec<kelve_graph_api::Claim> {
    claims
        .into_iter()
        .map(|c| kelve_graph_api::Claim {
            text: c.text,
            sources: c.sources,
            quotes: c
                .quotes
                .into_iter()
                .map(|q| kelve_graph_api::Quote {
                    source: q.source,
                    text: q.text,
                })
                .collect(),
            block: c.block,
            key: String::new(),
            faulted: false,
        })
        .collect()
}

/// Claims as the graph serves them, as kelve keeps its own.
fn from_graph_claims(claims: &[kelve_graph_api::Claim]) -> Vec<Claim> {
    claims
        .iter()
        .map(|c| Claim {
            text: c.text.clone(),
            sources: c.sources.clone(),
            quotes: c
                .quotes
                .iter()
                .map(|q| Quote {
                    source: q.source,
                    text: q.text.clone(),
                })
                .collect(),
            block: c.block.clone(),
            words: String::new(),
        })
        .collect()
}

/// An entry as a writer changing it is given it: its title, each block
/// with its id, part and fold, and its sources, numbered.
fn entry_material(entry: &Entry) -> String {
    serde_json::to_string_pretty(&json!({
        "title": entry.title,
        "blocks": entry.blocks,
        "sources": entry.sources.iter().map(|s| json!({
            "n": s.n, "title": s.title, "publisher": s.publisher, "url": s.url,
        })).collect::<Vec<_>>(),
        "links": entry.links.iter().map(|l| json!({ "term": l.term, "idea": l.idea.name })).collect::<Vec<_>>(),
    }))
    .unwrap_or_default()
}

/// The shape of a write job's answer: an entry, its pages, and the words
/// on each that support each claim; replacing one, the checked claims it
/// drops too.
pub fn schema(replacing: bool) -> Value {
    if replacing {
        draft::replacing_schema()
    } else {
        draft::agent_schema()
    }
}

/// Problems, as one answer to the agent.
fn said(problems: &[kelve_core::entry::check::Problem]) -> String {
    problems
        .iter()
        .take(6)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a whole entry is in its shape: it reads as an entry, its pages
/// are real addresses, and everything it points at is there.
fn whole_shape(value: &Value) -> std::result::Result<draft::Built, String> {
    let material = draft::sources_of(value).map_err(|p| p.to_string())?;
    let d = draft::parse(value).map_err(|p| p.to_string())?;
    draft::build(&d, &material).map_err(|p| said(&p))
}

/// Whether a write, fix or extend job's answer is in its shape: a whole
/// entry for a write, and for one that replaces an entry the graph has,
/// every claim it checked kept or dropped with why; for a fix or an
/// extend, what it changes applies to the entry and leaves it sound.
/// Whether what it says holds is the graph's to check.
pub async fn shape(
    app: &Shared,
    kind: &str,
    idea: Option<&str>,
    value: &Value,
) -> std::result::Result<(), String> {
    let row = match idea {
        Some(key) => entry(app, key).await.map_err(|e| e.to_string())?,
        None => None,
    };
    let plan = row.as_ref().map(plan_of).unwrap_or_default();
    if kind == agent::WRITE {
        let built = whole_shape(value)?;
        let mut held = Vec::new();
        if let Some(base) = &plan.base {
            dropped(base, &built.claims, &built.sources, value)?;
            held = base
                .claims
                .iter()
                .filter(|c| !c.faulted)
                .map(|c| c.text.as_str())
                .collect();
        }
        let d = draft::parse(value).map_err(|p| p.to_string())?;
        return claims_hold(&d, &held);
    }
    let Some(row) = row else {
        return Err("That entry isn't being changed any more. Take the next job.".into());
    };
    let (kind, base, claims, faulted) = revision_base(&row, &plan, kind)
        .ok_or("That entry isn't being changed any more. Take the next job.")?;
    let pages = draft::sources_of(value).map_err(|p| p.to_string())?;
    let d = draft::parse(value).map_err(|p| p.to_string())?;
    draft::build_revision(kind, &d, &pages, &base, &claims, &faulted).map_err(|p| said(&p))?;
    let held: Vec<&str> = claims
        .iter()
        .enumerate()
        .filter(|(i, _)| !faulted.contains(i))
        .map(|(_, c)| c.text.as_str())
        .collect();
    claims_hold(&d, &held)
}

/// kelve's own check of an answer's claims, before the graph's: that each
/// quote is a sentence, and each claim keeps the date its quote gives
/// (`check::claim_quotes`). A claim the entry had, checked by the graph
/// and given again word for word, is the graph's already: `held`.
fn claims_hold(d: &draft::Draft, held: &[&str]) -> std::result::Result<(), String> {
    let mut problems = Vec::new();
    for c in &d.claims {
        if held.iter().any(|h| h.trim() == c.text.trim()) {
            continue;
        }
        let quotes: Vec<(u8, &str)> = c
            .quotes
            .iter()
            .map(|q| (q.source.clamp(0.0, 255.0) as u8, q.text.as_str()))
            .collect();
        problems.extend(kelve_core::entry::check::claim_quotes(
            &c.block, &c.text, &quotes,
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(said(&problems))
    }
}

/// What a revision is written against: its kind, the entry, its claims,
/// and the places among them of those found wrong. A fix after the graph
/// turned an entry away is against that entry, as written; anything else,
/// against the graph's.
fn revision_base(
    row: &kelve_store::entries::Entry,
    plan: &Plan,
    job: &str,
) -> Option<(RevisionKind, Entry, Vec<Claim>, Vec<usize>)> {
    if job == agent::FIX && plan.revisions > 0 {
        let written: Entry = serde_json::from_str(row.entry.as_deref()?).ok()?;
        let claims: Vec<Claim> =
            serde_json::from_str(row.claims.as_deref().unwrap_or("[]")).unwrap_or_default();
        let failed: Vec<usize> = row
            .checking
            .as_deref()
            .and_then(|c| serde_json::from_str::<kelve_graph_api::Checking>(c).ok())
            .map(|c| {
                c.claims
                    .iter()
                    .filter(|cc| cc.state == ClaimState::Failed)
                    .filter_map(|cc| (cc.n as usize).checked_sub(1))
                    .collect()
            })
            .unwrap_or_default();
        return Some((RevisionKind::Fix, written, claims, failed));
    }
    let base = plan.base.as_ref()?;
    if job_kind(base.kind) != job {
        return None;
    }
    let faulted = base
        .claims
        .iter()
        .enumerate()
        .filter(|(_, c)| c.faulted)
        .map(|(i, _)| i)
        .collect();
    Some((
        base.kind,
        base.entry.clone(),
        from_graph_claims(&base.claims),
        faulted,
    ))
}

/// The base's checked claims an entry written to replace it leaves out,
/// each by key with why: every one it doesn't keep must be dropped with
/// why, or it isn't in its shape.
fn dropped(
    base: &Base,
    claims: &[Claim],
    sources: &[kelve_core::entry::Source],
    value: &Value,
) -> std::result::Result<Vec<Dropped>, String> {
    let d = draft::parse(value).map_err(|p| p.to_string())?;
    let kept: Vec<String> = claims
        .iter()
        .map(|c| draft::claim_key(c, sources))
        .collect();
    let mut out = Vec::new();
    for drop in &d.drop {
        let at = (drop.claim.fract() == 0.0 && drop.claim >= 1.0).then(|| drop.claim as usize - 1);
        let Some(claim) = at.and_then(|i| base.claims.get(i)) else {
            return Err(format!(
                "It drops claim {}, which the entry hasn't got.",
                drop.claim
            ));
        };
        if drop.why.trim().is_empty() {
            return Err(format!("It drops claim {} without saying why.", drop.claim));
        }
        out.push(Dropped {
            key: claim.key.clone(),
            why: clip(drop.why.trim(), 300),
        });
    }
    for (i, c) in base.claims.iter().enumerate() {
        if !c.faulted
            && !c.key.is_empty()
            && !kept.contains(&c.key)
            && !out.iter().any(|d| d.key == c.key)
        {
            return Err(format!(
                "Claim {} of the entry it replaces was checked against its page. Keep it, word for word, with its quotes, or say in drop why it goes.",
                i + 1
            ));
        }
    }
    Ok(out)
}

/// Has the person's agent write an entry, and builds it from the answer:
/// the person reads it at once, marked as not yet checked, and it goes to
/// the graph to be checked.
pub async fn write(app: &Shared, key: &str) -> Result<Then> {
    let (Some(row), Some(idea)) = (entry(app, key).await?, idea(app, key).await?) else {
        return Ok(Then::Done);
    };
    if row.state != "writing" {
        return Ok(Then::Done);
    }
    let mut plan = plan_of(&row);
    if plan.relations.is_empty() && plan.question.is_empty() {
        let related = super::relations::of(app, key).await?.unwrap_or_default();
        plan.question = related.question.clone();
        plan.relations = described(&related.relations);
        plan.known = known(app, &related.relations).await;
        // Replacing an entry the graph has: given block by block, so what
        // was checked can be kept word for word.
        plan.replacing = match &plan.base {
            Some(base) => revise::outline(&base.entry),
            None => row
                .replacing
                .as_deref()
                .and_then(|e| serde_json::from_str::<Entry>(e).ok())
                .map(|e| kelve_core::entry::markdown::write(&e))
                .unwrap_or_default(),
        };
        plan.wrong =
            serde_json::from_str(row.problems.as_deref().unwrap_or("[]")).unwrap_or_default();
        keep_plan(app, key, &plan).await?;
    }
    let today = app.config.today().format("%-d %B %Y").to_string();
    let version = agent::version(app, agent::WRITE).await;
    let generation: String = Sha256::digest(plan.replacing.as_bytes())
        .iter()
        .take(4)
        .map(|b| format!("{b:02x}"))
        .collect();
    let job = match &plan.base {
        Some(base) => revision_key(&version, key, base),
        None => format!("write:{version}:{key}:{generation}"),
    };
    let Some(answer) = agent::ask(
        app,
        &job,
        agent::Ask {
            kind: agent::WRITE,
            about: format!("Writing the entry for {}", idea.name),
            idea: Some(key.to_string()),
            opened: None,
            material: material(&idea, &plan, &today),
            schema: schema(plan.base.is_some()),
            priority: Priority::Kept,
            follows: None,
        },
        agent::Resume::job(WRITE, format!("{WRITE}:{key}"), json!({ "idea": key })),
    )
    .await?
    else {
        if row.step != "Kept for your agent" {
            set(app, key, "writing", "Kept for your agent").await?;
        }
        return Ok(Then::Done);
    };
    take_written(app, key, &idea, plan, answer).await
}

/// Builds an entry from the agent's answer, keeps it to show, and sends it
/// to the graph.
async fn take_written(
    app: &Shared,
    key: &str,
    idea: &Idea,
    mut plan: Plan,
    answer: agent::Answer,
) -> Result<Then> {
    plan.draft = answer.value.clone();
    plan.agent = answer.agent.clone();
    plan.model = answer.model.clone();
    let built = draft::sources_of(&answer.value)
        .map_err(|p| vec![p])
        .and_then(|m| {
            draft::parse(&answer.value)
                .map_err(|p| vec![p])
                .and_then(|d| draft::build(&d, &m))
                .map(|b| (b, m))
        });
    let (mut built, material) = match built {
        Ok(b) => b,
        Err(problems) => {
            // Its shape was checked when it was handed in: this is rare.
            let first = problems
                .first()
                .map(ToString::to_string)
                .unwrap_or_default();
            keep_plan(app, key, &plan).await?;
            give_up(
                app,
                key,
                &format!(
                    "Your agent’s entry didn’t hold together: {}",
                    clip(&first, 300)
                ),
            )
            .await?;
            return Ok(Then::Done);
        }
    };
    plan.pages = material
        .sources
        .iter()
        .filter_map(|s| s.source.url.clone())
        .map(|url| Page {
            url,
            passages: Vec::new(),
        })
        .collect();
    if let Some(base) = &plan.base {
        // Checked when it was handed in.
        plan.dropped =
            dropped(base, &built.claims, &built.sources, &answer.value).unwrap_or_default();
    }
    keep_plan(app, key, &plan).await?;

    let (links, nowhere) = resolve_links(app, idea, &built.links).await?;
    kelve_core::entry::known::unlink(&mut built.blocks, &nowhere);
    let written = Entry {
        idea: IdeaRef {
            id: key.to_string(),
            name: idea.name.clone(),
        },
        title: built.title,
        question: plan.question.clone(),
        minutes: built.minutes,
        made_at: Timestamp::now(),
        blocks: built.blocks,
        sources: built.sources,
        links,
    };
    keep_written(app, key, &written, &built.claims).await
}

/// Keeps an entry the agent wrote, to show at once, and sends it to the
/// graph.
async fn keep_written(app: &Shared, key: &str, written: &Entry, claims: &[Claim]) -> Result<Then> {
    let mut conn = app.db.acquire().await?;
    kelve_store::entries::written(
        &mut conn,
        key,
        &serde_json::to_string(written).unwrap_or_default(),
        &serde_json::to_string(claims).unwrap_or_default(),
    )
    .await?;
    kelve_store::entries::set_checking(&mut conn, key, None).await?;
    drop(conn);
    told(app, key);
    next(app, SEND, key).await?;
    Ok(Then::Done)
}

/// The idea each term a writer linked leads to: one the graph has when it
/// surely is that one, else a new idea, kept here until the graph has it.
/// And the terms that lead nowhere, to unlink.
async fn resolve_links(
    app: &Shared,
    idea: &Idea,
    wanted: &[DraftLink],
) -> Result<(Vec<kelve_core::entry::Link>, Vec<String>)> {
    let built_links = wanted;
    let own = norm(&idea.name);
    let mut req = MatchRequest::default();
    for (i, l) in built_links.iter().enumerate() {
        req.queries.insert(
            i.to_string(),
            Lookup {
                query: l.idea.clone(),
                context: None,
                limit: Some(1),
            },
        );
    }
    let found = match app.graph_reader() {
        Some(graph) if !req.queries.is_empty() => graph.candidates(&req).await.unwrap_or_default(),
        _ => Default::default(),
    };
    let mut links = Vec::new();
    let mut nowhere = Vec::new();
    for (i, l) in built_links.iter().enumerate() {
        let Some(name) = tidy(&l.idea).filter(|n| norm(n) != own) else {
            nowhere.push(l.term.clone());
            continue;
        };
        let surely = found
            .get(&i.to_string())
            .and_then(|c| c.result.first())
            .filter(|c| c.matched);
        let to = match surely {
            Some(c) => IdeaRef {
                id: c.id.clone(),
                name: c.name.clone(),
            },
            None => {
                let key = new_key(&name);
                let mut conn = app.db.acquire().await?;
                if kelve_store::entries::idea(&mut conn, &key).await?.is_none() {
                    kelve_store::entries::keep_idea(
                        &mut conn,
                        &Idea {
                            key: key.clone(),
                            name: name.clone(),
                            definition: clip(&l.about, 300),
                            kind: l.idea_kind().as_str().into(),
                            became: None,
                        },
                    )
                    .await?;
                }
                IdeaRef { id: key, name }
            }
        };
        links.push(kelve_core::entry::Link {
            term: l.term.clone(),
            idea: to,
            kind: l.link_kind(),
        });
    }
    Ok((links, nowhere))
}

/// Has the person's agent fix or extend an entry the graph has, a block at
/// a time, and builds the revision from its answer.
pub async fn revise(app: &Shared, key: &str) -> Result<Then> {
    let (Some(row), Some(idea)) = (entry(app, key).await?, idea(app, key).await?) else {
        return Ok(Then::Done);
    };
    if row.state != "writing" {
        return Ok(Then::Done);
    }
    let plan = plan_of(&row);
    let Some(base) = plan
        .base
        .clone()
        .filter(|b| b.kind != RevisionKind::Rewrite)
    else {
        return Ok(Then::Done);
    };
    let job = job_kind(base.kind);
    let version = agent::version(app, job).await;
    let today = app.config.today().format("%-d %B %Y").to_string();
    let mut material = json!({
        "today": today,
        "idea": idea_said(&idea),
        "entry": fence("entry", &entry_material(&base.entry)),
        "claims": fence("claims", &numbered(&base.claims)),
    });
    let about = match base.kind {
        RevisionKind::Fix => {
            let wrong: Vec<String> =
                serde_json::from_str(row.problems.as_deref().unwrap_or("[]")).unwrap_or_default();
            material["found"] = json!(wrong);
            material["blocksFoundWrong"] = json!(base.faulted);
            format!("Fixing the entry for {}", idea.name)
        }
        _ => {
            if let Some(ask) = &base.ask {
                material["asked"] = json!(ask.kind.says(ask.part));
                material["part"] = json!(ask.part.map(|p| p.as_str()));
            }
            format!("Adding to the entry for {}", idea.name)
        }
    };
    let Some(answer) = agent::ask(
        app,
        &revision_key(&version, key, &base),
        agent::Ask {
            kind: job,
            about,
            idea: Some(key.to_string()),
            opened: None,
            material,
            schema: draft::revision_schema(base.kind),
            priority: Priority::Kept,
            follows: None,
        },
        agent::Resume::job(REVISE, format!("{REVISE}:{key}"), json!({ "idea": key })),
    )
    .await?
    else {
        if row.step != "Kept for your agent" {
            set(app, key, "writing", "Kept for your agent").await?;
        }
        return Ok(Then::Done);
    };
    take_revision(app, key, &idea, plan, answer, job).await
}

/// Builds a fix or an extend from the agent's answer, keeps it to show,
/// and sends it to the graph.
async fn take_revision(
    app: &Shared,
    key: &str,
    idea: &Idea,
    mut plan: Plan,
    answer: agent::Answer,
    job: &str,
) -> Result<Then> {
    let Some(row) = entry(app, key).await? else {
        return Ok(Then::Done);
    };
    plan.agent = answer.agent.clone();
    plan.model = answer.model.clone();
    plan.draft = answer.value.clone();
    let Some((kind, base, claims, faulted)) = revision_base(&row, &plan, job) else {
        return Ok(Then::Done);
    };
    let built = draft::sources_of(&answer.value)
        .map_err(|p| vec![p])
        .and_then(|pages| {
            draft::parse(&answer.value)
                .map_err(|p| vec![p])
                .and_then(|d| {
                    draft::build_revision(kind, &d, &pages, &base, &claims, &faulted)
                        .map(|r| (r, d))
                })
        });
    let (revised, d) = match built {
        Ok(b) => b,
        Err(problems) => {
            // Its shape was checked when it was handed in: this is rare.
            let first = problems
                .first()
                .map(ToString::to_string)
                .unwrap_or_default();
            keep_plan(app, key, &plan).await?;
            give_up(
                app,
                key,
                &format!(
                    "Your agent’s change didn’t hold together: {}",
                    clip(&first, 300)
                ),
            )
            .await?;
            return Ok(Then::Done);
        }
    };
    for r in &d.remove {
        plan.whys
            .insert(r.id.trim().to_string(), clip(r.why.trim(), 300));
    }
    if let Some(served) = &plan.base {
        for (i, why) in &revised.dropped {
            if let Some(c) = served.claims.get(*i).filter(|_| plan.revisions == 0) {
                plan.dropped.push(Dropped {
                    key: c.key.clone(),
                    why: clip(why, 300),
                });
            }
        }
    }
    plan.pages = revised
        .sources
        .iter()
        .filter_map(|s| s.url.clone())
        .map(|url| Page {
            url,
            passages: Vec::new(),
        })
        .collect();
    keep_plan(app, key, &plan).await?;
    let (new_links, nowhere) = resolve_links(app, idea, &revised.links).await?;
    let mut blocks = revised.blocks;
    kelve_core::entry::known::unlink(&mut blocks, &nowhere);
    // The entry's own links, while its prose still links their terms, then
    // the revision's.
    let still = kelve_core::entry::check::linked_terms(&blocks);
    let mut links: Vec<kelve_core::entry::Link> = base
        .links
        .iter()
        .filter(|l| still.iter().any(|t| t.eq_ignore_ascii_case(&l.term)))
        .cloned()
        .collect();
    for l in new_links {
        if !links.iter().any(|k| k.term.eq_ignore_ascii_case(&l.term)) {
            links.push(l);
        }
    }
    let written = Entry {
        made_at: Timestamp::now(),
        blocks,
        sources: revised.sources,
        links,
        ..base
    };
    keep_written(app, key, &written, &revised.claims).await
}

/// Gives up on what the agent wrote: an entry the graph has is shown again
/// as the graph has it, and one new is dropped, saying why.
async fn give_up(app: &Shared, key: &str, why: &str) -> Result<()> {
    let plan = entry(app, key)
        .await?
        .map(|r| plan_of(&r))
        .unwrap_or_default();
    match plan.base {
        Some(base) => {
            let mut conn = app.db.acquire().await?;
            kelve_store::entries::taken(
                &mut conn,
                key,
                &serde_json::to_string(&base.entry).unwrap_or_default(),
                &base.revision,
            )
            .await?;
            drop(conn);
            tracing::info!(
                kind = base.kind.as_str(),
                "a revision of an entry was given up"
            );
            set(app, key, "ready", why).await
        }
        None => set(app, key, "dropped", why).await,
    }
}

/// Sends every entry the agent wrote that was kept here, as it would have
/// been sent: once the person agrees to add to the shared graph.
pub async fn send_kept(app: &Shared) -> Result<usize> {
    let keys = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::kept_here(&mut conn).await?
    };
    for key in &keys {
        set(app, key, "checking", "Being sent to the shared graph").await?;
        next(app, SEND, key).await?;
    }
    Ok(keys.len())
}

/// Why an entry the agent wrote stays here until it can go, as `kept_here`
/// says it: what the graph turned away, or what seemed to hold a secret,
/// says otherwise, and stays.
const NOT_ADDING: &str = "Not added to the shared graph, since you don’t add to it.";
const SIGN_IN: &str = "Not added to the shared graph: sign in to it in settings.";
const AGREE: &str = "Not added to the shared graph: agree to its terms in settings.";
const SIGN_IN_AGAIN: &str = "Not added to the shared graph: sign in to it again in settings.";

/// Why an entry the agent wrote stays here: the person doesn't add to the
/// graph, isn't signed in, or hasn't agreed to its terms as the graph
/// serves them now. None when it can go.
async fn kept_here(app: &Shared) -> Option<&'static str> {
    if !app.settings().contribute {
        return Some(NOT_ADDING);
    }
    let Some(graph) = app.graph_writer() else {
        return Some(SIGN_IN);
    };
    let now = crate::api::graph::terms_now(app).await;
    match graph.account().await {
        Ok(account) if account.agreed.as_deref() == Some(now.as_str()) => None,
        Ok(_) => Some(AGREE),
        Err(GraphError::SignIn) => Some(SIGN_IN_AGAIN),
        Err(_) => None,
    }
}

/// Sends what was kept here only until it could go, once it can: say,
/// when the person agreed somewhere kelve didn't see, or kelve learned
/// which terms the graph serves. The hourly sweep runs it.
pub async fn send_held(app: &Shared) -> Result<usize> {
    let mut held = Vec::new();
    for key in {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::kept_here(&mut conn).await?
    } {
        if entry(app, &key)
            .await?
            .is_some_and(|r| [NOT_ADDING, SIGN_IN, AGREE, SIGN_IN_AGAIN].contains(&r.step.as_str()))
        {
            held.push(key);
        }
    }
    if held.is_empty() || kept_here(app).await.is_some() {
        return Ok(0);
    }
    for key in &held {
        set(app, key, "checking", "Being sent to the shared graph").await?;
        next(app, SEND, key).await?;
    }
    Ok(held.len())
}

/// Sends an entry the agent wrote to the shared graph, to be checked: its
/// idea first, if the graph doesn't have it, and the new ideas it links to,
/// so its links lead somewhere.
pub async fn send(app: &Shared, key: &str) -> Result<Then> {
    let (Some(row), Some(idea)) = (entry(app, key).await?, idea(app, key).await?) else {
        return Ok(Then::Done);
    };
    if row.state != "checking" || row.sent || row.from_graph {
        return Ok(Then::Done);
    }
    if let Some(why) = kept_here(app).await {
        set(app, key, "ready", why).await?;
        return Ok(Then::Done);
    }
    let Some(graph) = app.graph_writer() else {
        return Ok(Then::Done);
    };
    // The account added as much as the graph takes today: it goes once
    // the day's limits reset, without asking the graph till then.
    if let Some(held) = app.sends_held() {
        if row.step != super::WAITING_FOR_LIMIT {
            set(app, key, "checking", super::WAITING_FOR_LIMIT).await?;
        }
        return Ok(Then::Again(kelve_store::at(held.until)));
    }
    let mut plan = plan_of(&row);
    let mut written: Entry = serde_json::from_str(row.entry.as_deref().unwrap_or("null"))
        .map_err(|e| Failed::Other(format!("a written entry doesn't read: {e}")))?;
    let mut changes = Vec::new();
    let own = if is_id(key) {
        key.to_string()
    } else if let Some(id) = &idea.became {
        id.clone()
    } else {
        changes.push(Change::AddIdea {
            reference: "idea".into(),
            name: idea.name.clone(),
            definition: idea.definition.clone(),
            kind: IdeaKind::parse(&idea.kind),
            same_as: None,
        });
        "ref:idea".into()
    };
    let mut refs: Vec<(String, String)> = Vec::new();
    for (i, link) in written.links.iter_mut().enumerate() {
        if is_id(&link.idea.id) {
            continue;
        }
        let Some(linked) = self::idea(app, &link.idea.id).await? else {
            continue;
        };
        if let Some(id) = linked.became {
            link.idea.id = id;
            continue;
        }
        let reference = format!("link{i}");
        changes.push(Change::AddIdea {
            reference: reference.clone(),
            name: linked.name.clone(),
            definition: linked.definition.clone(),
            kind: IdeaKind::parse(&linked.kind),
            same_as: None,
        });
        refs.push((reference.clone(), linked.key.clone()));
        link.idea.id = format!("ref:{reference}");
    }
    written.idea.id = own.clone();
    let claims = to_graph(
        serde_json::from_str::<Vec<Claim>>(row.claims.as_deref().unwrap_or("[]"))
            .unwrap_or_default(),
    );
    // Written against an entry the graph has: what it does to it, block by
    // block, from the revision it was written against.
    let revise = plan.base.as_ref().map(|base| {
        let mut ops = match base.kind {
            RevisionKind::Rewrite => Vec::new(),
            _ => revise::diff(&base.entry.blocks, &written.blocks, "Found wrong."),
        };
        for op in &mut ops {
            if let kelve_core::entry::revise::Op::Remove { id, why } = op
                && let Some(said) = plan.whys.get(id)
            {
                *why = said.clone();
            }
        }
        Revise {
            kind: base.kind,
            base: base.revision.clone(),
            ops,
            ask: base.ask.clone(),
            dropped: plan.dropped.clone(),
        }
    });
    changes.push(Change::SetEntry {
        entry: Box::new(written.clone()),
        claims,
        revise,
    });
    // How it relates goes with it, proposed when it was opened.
    let relations = super::relations::unsent(app, key, &own).await?;
    changes.extend(relations.iter().cloned());
    let changeset = Changeset {
        agent: kelve_graph_api::client::AGENT.into(),
        comment: format!("An entry for {}", idea.name),
        changes,
        written_by: written_by(app, plan.agent.as_deref(), plan.model.as_deref()).await,
    };
    // Secrets never go, however they got in (Nathan, 19 October).
    let body = serde_json::to_vec(&changeset).unwrap_or_default();
    let text = String::from_utf8_lossy(&body);
    if kelve_core::redact::redact(&text) != text {
        turned_away(
            app,
            key,
            "Not added to the shared graph: it seemed to hold a secret.",
        )
        .await?;
        tracing::warn!("an entry seemed to hold a secret, so it wasn't sent");
        return Ok(Then::Done);
    }
    let idempotency: String = Sha256::digest(&body)
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect();
    let applied = match super::apply(app, &graph, &changeset, &format!("entry-{idempotency}")).await
    {
        Ok(applied) => applied,
        Err(GraphError::Enough(limit)) => {
            set(app, key, "checking", super::WAITING_FOR_LIMIT).await?;
            return Ok(Then::Again(after(limit.after)));
        }
        Err(GraphError::Limited { after: wait, .. } | GraphError::Busy { after: Some(wait) }) => {
            return Ok(Then::Again(after(wait)));
        }
        Err(GraphError::SignIn) => {
            turned_away(app, key, SIGN_IN_AGAIN).await?;
            return Ok(Then::Done);
        }
        Err(GraphError::Refused { said, .. }) => {
            sent(app, key, None, Sent::TurnedAway).await?;
            turned_away(
                app,
                key,
                &format!("Not added to the shared graph: {}", clip(&said, 300)),
            )
            .await?;
            return Ok(Then::Done);
        }
        Err(e) => return Err(Failed::Other(e.to_string())),
    };
    let mut conn = app.db.acquire().await?;
    if let Some(id) = applied.ideas.get("idea") {
        kelve_store::entries::became(&mut conn, key, id).await?;
        written.idea.id = id.clone();
    }
    for (reference, local) in &refs {
        if let Some(id) = applied.ideas.get(reference) {
            kelve_store::entries::became(&mut conn, local, id).await?;
            for link in &mut written.links {
                if link.idea.id == format!("ref:{reference}") {
                    link.idea.id = id.clone();
                }
            }
        }
    }
    drop(conn);
    if !relations.is_empty() {
        super::relations::settle(app, key, &relations, &applied).await?;
    }
    for skipped in &applied.skipped {
        tracing::info!(skipped, "the shared graph left something out");
    }
    let Some(revision) = applied.entries.get(&written.idea.id).cloned() else {
        // Turned away at once, for what code could see: said in `skipped`.
        let why = applied
            .skipped
            .first()
            .cloned()
            .unwrap_or_else(|| "the shared graph didn’t take it".into());
        sent(app, key, None, Sent::TurnedAway).await?;
        turned_away(
            app,
            key,
            &format!("Not added to the shared graph: {}", clip(&why, 300)),
        )
        .await?;
        return Ok(Then::Done);
    };
    let mut conn = app.db.acquire().await?;
    kelve_store::entries::sent(
        &mut conn,
        key,
        &revision,
        &serde_json::to_string(&written).unwrap_or_default(),
    )
    .await?;
    kelve_store::sends::record(&mut conn, key, Some(&revision), Sent::Checking).await?;
    kelve_store::entries::set(
        &mut conn,
        key,
        "checking",
        "The shared graph is checking each claim against its page",
    )
    .await?;
    drop(conn);
    plan.following_since = Some(kelve_store::now());
    keep_plan(app, key, &plan).await?;
    told(app, key);
    next_at(app, FOLLOW, key, Utc::now() + chrono::Duration::seconds(2)).await?;
    Ok(Then::Done)
}

/// When `wait` from now is.
fn after(wait: Duration) -> chrono::DateTime<Utc> {
    Utc::now() + chrono::Duration::from_std(wait).unwrap_or(chrono::Duration::hours(1))
}

/// Notes a send of an idea's entry, for what settings say was sent today.
async fn sent(app: &Shared, key: &str, revision: Option<&str>, state: Sent) -> Result<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::sends::record(&mut conn, key, revision, state).await?;
    Ok(())
}

/// How the graph's check of a send ended, for what settings say.
async fn settled(app: &Shared, revision: &str, state: Sent) -> Result<()> {
    let mut conn = app.db.acquire().await?;
    kelve_store::sends::settle(&mut conn, revision, state).await?;
    drop(conn);
    app.publish(kelve_core::views::ServerEvent::Settings);
    Ok(())
}

/// When the graph doesn't take what the agent wrote: a new entry is kept
/// here, read as it was written; a change to an entry the graph has is
/// let go, and the graph's shown.
async fn turned_away(app: &Shared, key: &str, why: &str) -> Result<()> {
    let revising = entry(app, key)
        .await?
        .is_some_and(|r| plan_of(&r).base.is_some());
    if revising {
        give_up(app, key, why).await
    } else {
        set(app, key, "ready", why).await
    }
}

/// Who wrote what's sent: the agent, the model it said it ran on, and its
/// plan, when it said.
async fn written_by(app: &Shared, agent: Option<&str>, model: Option<&str>) -> Option<WrittenBy> {
    let agent = agent?;
    let plan = {
        let mut conn = app.db.acquire().await.ok()?;
        kelve_store::asks::agent(&mut conn, agent)
            .await
            .ok()
            .flatten()
            .and_then(|a| a.plan)
    };
    Some(WrittenBy {
        agent: agent.to_string(),
        model: model.unwrap_or("unknown").to_string(),
        plan,
    })
}

/// Follows the graph's check of an entry it was sent: claim by claim, for
/// the reader, until the graph accepts it or turns it away. Turned away for
/// claims its pages don't support, the agent is asked to fix them, once.
pub async fn follow(app: &Shared, key: &str) -> Result<Then> {
    let (Some(row), Some(graph)) = (entry(app, key).await?, app.graph_writer()) else {
        return Ok(Then::Done);
    };
    if row.state != "checking" || !row.sent {
        return Ok(Then::Done);
    }
    let Some(revision) = row.revision.clone() else {
        return Ok(Then::Done);
    };
    let mut plan = plan_of(&row);
    let since = plan.following_since.unwrap_or_else(kelve_store::now);
    let checking = match graph.checking(&revision).await {
        Ok(Some(c)) => c,
        Ok(None) => return Ok(Then::Done),
        Err(e) => {
            tracing::info!(error = %e, "couldn't ask the shared graph how an entry's check stands");
            return Ok(again(since));
        }
    };
    {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::set_checking(
            &mut conn,
            key,
            Some(&serde_json::to_string(&checking).unwrap_or_default()),
        )
        .await?;
    }
    told(app, key);
    match checking.status {
        CheckStatus::Pending => Ok(again(since)),
        CheckStatus::Accepted => {
            settled(app, &revision, Sent::Accepted).await?;
            set(app, key, "ready", "").await?;
            Ok(Then::Done)
        }
        CheckStatus::Rejected => {
            settled(app, &revision, Sent::TurnedAway).await?;
            let claims: Vec<Claim> =
                serde_json::from_str(row.claims.as_deref().unwrap_or("[]")).unwrap_or_default();
            let found = findings(&checking, &claims);
            if found.is_empty() || plan.revisions >= FIXES {
                let why = checking
                    .problem
                    .clone()
                    .unwrap_or_else(|| "its checks didn’t pass".into());
                give_up(
                    app,
                    key,
                    &format!("The shared graph turned it away: {}", clip(&why, 300)),
                )
                .await?;
                return Ok(Then::Done);
            }
            plan.revisions += 1;
            plan.problems = found;
            keep_plan(app, key, &plan).await?;
            let mut conn = app.db.acquire().await?;
            kelve_store::entries::unsent(&mut conn, key).await?;
            drop(conn);
            set(app, key, "writing", "Fixing what the checks found").await?;
            next(app, FIX, key).await?;
            Ok(Then::Done)
        }
    }
}

/// What the graph's check found, claim by claim, for the fix: each claim
/// that failed, with its words, what it quotes from each page, and the
/// graph's finding as the graph gave it. A claim it gave no finding for
/// has the reason it turned the entry away, when that names the claim.
fn findings(checking: &kelve_graph_api::Checking, claims: &[Claim]) -> Vec<String> {
    checking
        .claims
        .iter()
        .filter(|c| c.state == ClaimState::Failed)
        .map(|c| {
            let claim = (c.n as usize).checked_sub(1).and_then(|i| claims.get(i));
            let finding = Some(c.found.trim())
                .filter(|f| !f.is_empty())
                .or_else(|| {
                    checking
                        .problem
                        .as_deref()
                        .map(str::trim)
                        .filter(|p| p.starts_with(&format!("Claim {} ", c.n)))
                })
                .unwrap_or("Its pages don't show it.");
            let mut said = format!(
                "Claim {}: {}",
                c.n,
                claim.map(|c| c.text.as_str()).unwrap_or_default()
            );
            for q in claim.map(|c| c.quotes.as_slice()).unwrap_or_default() {
                said.push_str(&format!("\nIt quotes source {}: “{}”", q.source, q.text));
            }
            said.push_str(&format!("\nWhat the graph found: {finding}"));
            said
        })
        .collect()
}

/// When to ask about a check again: often at first, then less, and not at
/// all after two days.
fn again(since: i64) -> Then {
    let waited = kelve_store::now() - since;
    if waited > FOLLOW_FOR_MS {
        return Then::Done;
    }
    let wait = (waited / 10).clamp(2_000, 120_000);
    Then::Again(Utc::now() + chrono::Duration::milliseconds(wait))
}

/// Has the agent fix what the graph's checks found, then sends it again.
pub async fn fix(app: &Shared, key: &str) -> Result<Then> {
    let (Some(row), Some(idea)) = (entry(app, key).await?, idea(app, key).await?) else {
        return Ok(Then::Done);
    };
    if row.state != "writing" {
        return Ok(Then::Done);
    }
    let plan = plan_of(&row);
    let Some((_, written, claims, failed)) = revision_base(&row, &plan, agent::FIX) else {
        return Ok(Then::Done);
    };
    // Its claims, as the agent is given them: those that failed marked.
    let mut listed = to_graph(claims);
    for i in failed {
        if let Some(c) = listed.get_mut(i) {
            c.faulted = true;
        }
    }
    let faulted: Vec<String> = listed
        .iter()
        .filter(|c| c.faulted && !c.block.is_empty())
        .map(|c| c.block.clone())
        .collect();
    let version = agent::version(app, agent::FIX).await;
    let Some(answer) = agent::ask(
        app,
        &format!("fix:{version}:{key}:{}", plan.revisions),
        agent::Ask {
            kind: agent::FIX,
            about: format!("Fixing the entry for {}", idea.name),
            idea: Some(key.to_string()),
            opened: None,
            material: json!({
                "idea": idea_said(&idea),
                "entry": fence("entry", &entry_material(&written)),
                "claims": fence("claims", &numbered(&listed)),
                // Each finding carries the claim's quotes: words from the web.
                "found": fence("said", &plan.problems.join("\n\n")),
                "blocksFoundWrong": faulted,
            }),
            schema: draft::revision_schema(RevisionKind::Fix),
            priority: Priority::Kept,
            follows: None,
        },
        agent::Resume::job(FIX, format!("{FIX}:{key}"), json!({ "idea": key })),
    )
    .await?
    else {
        return Ok(Then::Done);
    };
    take_revision(app, key, &idea, plan, answer, agent::FIX).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use kelve_graph_api::{Checking, ClaimChecked};

    #[test]
    fn a_fix_is_told_each_failed_claim_its_quotes_and_what_the_graph_found() {
        let claim = |text: &str, quote: &str| Claim {
            text: text.into(),
            sources: vec![1],
            quotes: vec![Quote {
                source: 1,
                text: quote.into(),
            }],
            block: "b2".into(),
            words: String::new(),
        };
        let claims = [
            claim(
                "Volumes outlive containers.",
                "Volumes persist after the container is removed.",
            ),
            claim(
                "A node's role says what kind of thing it is.",
                "What kind of thing is it? For example, is it a button?",
            ),
            claim(
                "An HTTP request names a URL and a method.",
                "A method that defines the required action.",
            ),
        ];
        let checked = |n: u32, state: ClaimState, found: &str| ClaimChecked {
            n,
            state,
            found: found.into(),
            key: String::new(),
            carried: false,
        };
        let checking = Checking {
            status: CheckStatus::Rejected,
            claims: vec![
                checked(1, ClaimState::Held, ""),
                checked(
                    2,
                    ClaimState::Failed,
                    "Claim 2 isn't supported by the pages it cites: they don't show that the page calls this property the 'role'.",
                ),
                checked(3, ClaimState::Failed, ""),
            ],
            problem: Some("Claim 3 isn't supported by the pages it cites.".into()),
        };
        let found = findings(&checking, &claims);
        assert_eq!(
            found,
            [
                "Claim 2: A node's role says what kind of thing it is.\nIt quotes source 1: “What kind of thing is it? For example, is it a button?”\nWhat the graph found: Claim 2 isn't supported by the pages it cites: they don't show that the page calls this property the 'role'.",
                "Claim 3: An HTTP request names a URL and a method.\nIt quotes source 1: “A method that defines the required action.”\nWhat the graph found: Claim 3 isn't supported by the pages it cites.",
            ]
        );
    }
}
