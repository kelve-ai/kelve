//! How ideas relate, on this side of the shared graph: what the graph serves
//! about an idea, kept to draw its outline, and what this kelve proposes when
//! the graph has nothing yet, before it writes the idea's entry. Everything
//! here is on demand: an idea's relations are proposed only when someone opens
//! it.

use kelve_core::fence::fence;
use kelve_core::ideas::{IdeaKind, is_id, norm, tidy};
use kelve_core::relations::{RelationKind, RelationState, is_foundation, why_problem};
use kelve_core::text::clip;
use kelve_graph_api::client::Error as GraphError;
use kelve_graph_api::{Change, Changeset, IdeaRelations, MatchRequest, Node, Query as Lookup, Rel};
use kelve_store::entries::Idea;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Result, Then};
use crate::agent::{self, Priority};
use crate::state::Shared;

/// Proposing an idea's relations, before its entry is written.
pub const RELATE: &str = "entry.relate";

/// How long what the graph served about an idea is used before asking
/// again: an hour, as for entries.
const FRESH_MS: i64 = 60 * 60 * 1000;

/// The idea's own ID in what's proposed, until it's sent: the ID the graph
/// gave it, or `ref:idea` when the entry that adds it goes with them.
const SELF: &str = "self";

/// An idea's relations, as kelve knows them.
#[derive(Debug, Clone, Default)]
pub struct Known {
    pub relations: IdeaRelations,
    /// Proposed here, and not yet served by the graph.
    pub proposed: bool,
    /// The question the idea answers, when it's been asked.
    pub question: String,
}

/// Whether the graph says anything about how an idea relates.
pub fn has_any(r: &IdeaRelations) -> bool {
    !(r.down.is_empty()
        && r.kind_of.is_empty()
        && r.answers.is_empty()
        && r.alternatives.is_empty()
        && r.used_with.is_empty()
        && r.up.count == 0)
}

/// Whether how an idea itself relates is known: what it rests on, is a kind of
/// or answers. What rests on it says nothing of that, and nor does what it's
/// used with or instead of: those go both ways, so another idea's say nothing
/// of this one's.
pub fn has_own(r: &IdeaRelations) -> bool {
    !(r.down.is_empty() && r.kind_of.is_empty() && r.answers.is_empty())
}

/// The graph's ID for an idea kelve keeps: its key, or the ID it became.
pub(crate) async fn graph_id(app: &Shared, key: &str) -> Result<Option<String>> {
    if is_id(key) {
        return Ok(Some(key.to_string()));
    }
    let mut conn = app.db.acquire().await?;
    Ok(kelve_store::entries::idea(&mut conn, key)
        .await?
        .and_then(|i| i.became))
}

/// An idea's relations: what the graph serves, asked again once an hour,
/// or what this kelve proposed while the graph has none.
pub async fn of(app: &Shared, key: &str) -> Result<Option<Known>> {
    let kept = {
        let mut conn = app.db.acquire().await?;
        kept_for(&mut conn, key).await?
    };
    let parse = |k: &kelve_store::relations::Kept| Known {
        relations: serde_json::from_str(&k.relations).unwrap_or_default(),
        proposed: k.source == "proposed",
        question: k.question.clone(),
    };
    let fresh = kept
        .as_ref()
        .is_some_and(|k| kelve_store::now() - k.fetched_at < FRESH_MS);
    let id = graph_id(app, key).await?;
    let (Some(graph), Some(id), false) = (app.graph_reader(), id, fresh) else {
        return Ok(kept.as_ref().map(parse));
    };
    match graph.relations(std::slice::from_ref(&id)).await {
        Ok(got) if got.split.contains_key(&id) => {
            super::splits::found(app, key, &id, &got.split[&id]).await?;
            Ok(None)
        }
        Ok(mut got) => {
            let served = got.relations.remove(&id).or_else(|| {
                got.redirects
                    .get(&id)
                    .and_then(|to| got.relations.remove(to))
            });
            let mut conn = app.db.acquire().await?;
            served_or_kept(&mut conn, key, kept.as_ref(), served).await
        }
        Err(e) => {
            tracing::info!(error = %e, "couldn't ask the shared graph how an idea relates; kelve shows what it kept");
            Ok(kept.as_ref().map(parse))
        }
    }
}

/// What's known of an idea once the graph has been asked, kept for next
/// time. The graph's relations, when it serves some that say how the idea
/// itself relates. When it serves only what rests on the idea, or what it's
/// used with or instead of, and this kelve proposed how the idea relates,
/// the proposal stands, with those added: the graph hasn't reviewed the proposal yet, and
/// dropping it would leave the idea untraced, with nothing to ask the agent.
/// Otherwise what was kept, asked about again in an hour.
async fn served_or_kept(
    conn: &mut kelve_store::Conn,
    key: &str,
    kept: Option<&kelve_store::relations::Kept>,
    served: Option<IdeaRelations>,
) -> Result<Option<Known>> {
    let parse = |k: &kelve_store::relations::Kept| Known {
        relations: serde_json::from_str(&k.relations).unwrap_or_default(),
        proposed: k.source == "proposed",
        question: k.question.clone(),
    };
    let question = kept.map(|k| k.question.clone()).unwrap_or_default();
    let proposal = kept
        .map(parse)
        .filter(|k| k.proposed && has_own(&k.relations));
    match served.filter(has_any) {
        Some(r) if has_own(&r) || proposal.is_none() => {
            kelve_store::relations::served(
                conn,
                key,
                &serde_json::to_string(&r).unwrap_or_default(),
            )
            .await?;
            Ok(Some(Known {
                relations: r,
                proposed: false,
                question,
            }))
        }
        Some(r) => {
            let mut known = proposal.unwrap_or_default();
            known.relations.up = r.up;
            if known.relations.grounds.is_empty() {
                known.relations.grounds = r.grounds;
            }
            // What the graph says it's used with or instead of stays,
            // where the proposal says nothing of its own.
            if known.relations.used_with.is_empty() {
                known.relations.used_with = r.used_with;
            }
            if known.relations.alternatives.is_empty() {
                known.relations.alternatives = r.alternatives;
            }
            // Kept where the proposal is: under the new idea's key, when
            // the graph's ID was asked about.
            let at = kept.map_or(key, |k| k.key.as_str());
            kelve_store::relations::refreshed(
                conn,
                at,
                &serde_json::to_string(&known.relations).unwrap_or_default(),
            )
            .await?;
            Ok(Some(known))
        }
        None => {
            kelve_store::relations::asked(conn, kept.map_or(key, |k| k.key.as_str())).await?;
            Ok(kept.map(parse))
        }
    }
}

/// What's kept of an idea's relations: under its key, or, for the graph's
/// ID of an idea kelve proposed as new, under the key it had then. The
/// graph refers to an idea by its ID once it has it, while what kelve
/// proposed for it is kept under the new key until the graph serves its own.
async fn kept_for(
    conn: &mut kelve_store::Conn,
    key: &str,
) -> Result<Option<kelve_store::relations::Kept>> {
    if let Some(kept) = kelve_store::relations::get(conn, key).await? {
        return Ok(Some(kept));
    }
    if !is_id(key) {
        return Ok(None);
    }
    match kelve_store::entries::became_from(conn, key).await? {
        Some(local) => Ok(kelve_store::relations::get(conn, &local).await?),
        None => Ok(None),
    }
}

/// Several ideas' relations at once, as `of` gives one: what the graph
/// serves is asked for in one request, for the ideas not asked within the
/// hour.
pub async fn many(
    app: &Shared,
    keys: &[String],
) -> Result<std::collections::HashMap<String, Known>> {
    let mut out = std::collections::HashMap::new();
    let mut stale: Vec<(String, String)> = Vec::new();
    {
        let mut conn = app.db.acquire().await?;
        for key in keys {
            let kept = kept_for(&mut conn, key).await?;
            let fresh = kept
                .as_ref()
                .is_some_and(|k| kelve_store::now() - k.fetched_at < FRESH_MS);
            if let Some(k) = &kept {
                out.insert(
                    key.clone(),
                    Known {
                        relations: serde_json::from_str(&k.relations).unwrap_or_default(),
                        proposed: k.source == "proposed",
                        question: k.question.clone(),
                    },
                );
            }
            if !fresh {
                let id = if is_id(key) {
                    Some(key.clone())
                } else {
                    kelve_store::entries::idea(&mut conn, key)
                        .await?
                        .and_then(|i| i.became)
                };
                if let Some(id) = id {
                    stale.push((key.clone(), id));
                }
            }
        }
    }
    let (Some(graph), false) = (app.graph_reader(), stale.is_empty()) else {
        return Ok(out);
    };
    let ids: Vec<String> = stale.iter().map(|(_, id)| id.clone()).collect();
    match graph.relations(&ids).await {
        Ok(mut got) => {
            let mut conn = app.db.acquire().await?;
            for (key, id) in stale {
                if let Some(into) = got.split.get(&id) {
                    drop(conn);
                    super::splits::found(app, &key, &id, into).await?;
                    out.remove(&key);
                    conn = app.db.acquire().await?;
                    continue;
                }
                let to = got.redirects.get(&id).cloned().unwrap_or(id);
                let kept = kept_for(&mut conn, &key).await?;
                let served = got.relations.remove(&to);
                match served_or_kept(&mut conn, &key, kept.as_ref(), served).await? {
                    Some(known) => out.insert(key, known),
                    None => out.remove(&key),
                };
            }
        }
        Err(e) => {
            tracing::info!(error = %e, "couldn't ask the shared graph how ideas relate; kelve shows what it kept")
        }
    }
    Ok(out)
}

/// Whether an idea's relations should be proposed: nothing's known about
/// how it relates itself (what rests on it says nothing of that), nothing
/// was proposed here yet, and it isn't a first principle, which rests on
/// nothing.
pub async fn needed(app: &Shared, idea: &Idea) -> Result<bool> {
    if is_foundation(&idea.name) {
        return Ok(false);
    }
    Ok(of(app, &idea.key)
        .await?
        .is_none_or(|k| !has_own(&k.relations) && !k.proposed))
}

/// Proposes an idea's relations when someone opens it, if they're needed.
/// Nothing below it is traced until someone asks: kelve never queues work
/// for the agent that no one asked for (Nathan, 22 October).
pub async fn ensure(app: &Shared, idea: &Idea) -> Result<()> {
    if needed(app, idea).await? {
        queue(app, &idea.key, 0, false).await?;
    }
    Ok(())
}

/// Traces one step down from an idea whose own relations are known, as
/// someone looks at what it rests on: each idea there that nothing's known
/// about gets its relations proposed, after what's opened is read.
pub async fn trace(app: &Shared, key: &str) -> Result<()> {
    trace_down(app, key, 0).await
}

/// Asks for an idea's relations to be proposed, `depth` steps below the
/// idea someone opened; `below` when it's reached by tracing down.
async fn queue(app: &Shared, key: &str, depth: u64, below: bool) -> Result<()> {
    super::add(
        app,
        RELATE,
        Some(&format!("{RELATE}:{key}")),
        json!({ "idea": key, "depth": depth, "below": below }),
        chrono::Utc::now(),
    )
    .await
}

/// Traces one step down from an idea: each idea it rests on that nothing's
/// known about gets its relations proposed. It goes no further by itself:
/// each step down is one someone asked for.
async fn trace_down(app: &Shared, key: &str, depth: u64) -> Result<()> {
    let Some(known) = of(app, key).await? else {
        return Ok(());
    };
    let mut next: Vec<Idea> = Vec::new();
    for rel in &known.relations.down {
        if rel.idea.foundation {
            continue;
        }
        let Some(idea) = local(app, &rel.idea).await? else {
            continue;
        };
        if needed(app, &idea).await? {
            next.push(idea);
        }
    }
    for idea in &next {
        queue(app, &idea.key, depth + 1, true).await?;
    }
    Ok(())
}

/// The idea kelve keeps for one a relation leads to: kept now from the
/// graph's when it's one kelve hasn't seen.
async fn local(app: &Shared, node: &Node) -> Result<Option<Idea>> {
    {
        let mut conn = app.db.acquire().await?;
        if let Some(idea) = kelve_store::entries::idea(&mut conn, &node.id).await? {
            return Ok(Some(idea));
        }
    }
    let (Some(graph), true) = (app.graph_reader(), is_id(&node.id)) else {
        return Ok(None);
    };
    let found = match graph.ideas(std::slice::from_ref(&node.id)).await {
        Ok(found) => found,
        Err(e) => {
            tracing::info!(error = %e, "couldn't ask the shared graph about an idea to trace");
            return Ok(None);
        }
    };
    let Some(i) = found.ideas.into_iter().find(|i| i.id == node.id) else {
        return Ok(None);
    };
    let idea = Idea {
        key: i.id,
        name: i.name,
        definition: i.definition,
        kind: i.kind.as_str().to_string(),
        became: None,
    };
    let mut conn = app.db.acquire().await?;
    kelve_store::entries::keep_idea(&mut conn, &idea).await?;
    Ok(Some(idea))
}

/// One idea a proposal relates the idea to, as the agent gave it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Related {
    pub name: String,
    pub definition: String,
    pub why: String,
}

/// How the agent says an idea relates, before code holds its shape.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Proposal {
    pub question: String,
    pub rests_on: Vec<Related>,
    pub kind_of: Vec<Related>,
    pub answers: Vec<Related>,
    pub alternatives: Vec<Related>,
    pub used_with: Vec<Related>,
}

/// What asking the agent how an idea relates came to.
#[derive(Debug)]
pub enum Asked {
    /// What it proposed, and the model it said it ran on.
    Proposed(Proposal, String),
    /// It's with the agent.
    Waiting,
}

/// The shape of a relate job's answer.
pub fn schema() -> Value {
    let related = |most: u32| {
        json!({
            "type": "array",
            "maxItems": most,
            "items": object(json!({
                "name": string("Its name, one to five words."),
                "definition": string("What it is, in one plain line."),
                "why": string("One plain sentence, under 200 characters."),
            })),
        })
    };
    object(json!({
        "question": string("The one question the idea answers."),
        "restsOn": related(3),
        "kindOf": related(2),
        "answers": related(2),
        "alternatives": related(4),
        "usedWith": related(3),
    }))
}

fn object(properties: Value) -> Value {
    let required: Vec<String> = properties
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

/// Whether a relate job's answer is in its shape.
pub fn shape(value: &Value) -> std::result::Result<(), String> {
    if !value["question"].is_string() {
        return Err("Say the question the idea answers, as question.".into());
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    #[allow(dead_code)] // Read only to check the answer's shape.
    struct Strict {
        question: String,
        #[serde(default)]
        rests_on: Vec<StrictRelated>,
        #[serde(default)]
        kind_of: Vec<StrictRelated>,
        #[serde(default)]
        answers: Vec<StrictRelated>,
        #[serde(default)]
        alternatives: Vec<StrictRelated>,
        #[serde(default)]
        used_with: Vec<StrictRelated>,
    }
    #[derive(Deserialize)]
    #[allow(dead_code)] // Read only to check the answer's shape.
    struct StrictRelated {
        name: String,
        definition: String,
        why: String,
    }
    serde_json::from_value::<Strict>(value.clone())
        .map(|_| ())
        .map_err(|e| format!("It isn't in the shape asked for: {e}."))
}

/// Asks the agent how an idea relates, as a job that runs `RELATE` for it
/// again once it's answered.
pub async fn ask(app: &Shared, idea: &Idea, depth: u64, below: bool) -> Result<Asked> {
    let material = json!({
        "idea": fence(
            "said",
            &format!(
                "The idea: {}\nWhat it is: {}\nWhat kind of thing it is: {}",
                idea.name, idea.definition, idea.kind
            ),
        ),
    });
    let version = agent::version(app, agent::RELATE).await;
    let asked = agent::ask(
        app,
        &format!("relate:{version}:{}", idea.key),
        agent::Ask {
            kind: agent::RELATE,
            about: format!("Tracing {} down", idea.name),
            idea: Some(idea.key.clone()),
            opened: None,
            material,
            schema: schema(),
            // The idea opened is traced before its entry is written, so
            // what it rests on is known first. What's below it waits until
            // everything opened is read: no one asked for it yet.
            priority: if below {
                Priority::Later
            } else {
                Priority::Now
            },
            follows: None,
        },
        agent::Resume::job(
            RELATE,
            format!("{RELATE}:{}", idea.key),
            json!({ "idea": idea.key, "depth": depth, "below": below }),
        ),
    )
    .await?;
    Ok(match asked {
        Some(answer) => Asked::Proposed(
            serde_json::from_value(answer.value).unwrap_or_default(),
            answer.model.unwrap_or_else(|| "unknown".into()),
        ),
        None => Asked::Waiting,
    })
}

/// Proposes how an idea relates, with the agent's answer, keeps it to draw
/// the outline at once, and sends it to the graph: on its own when the
/// graph has the idea's entry, or when it was reached by tracing down, so
/// it has none to go with; otherwise with the entry. Then the entry is
/// written, if it's waiting. What it rests on isn't traced until someone
/// asks.
pub async fn propose(app: &Shared, key: &str, depth: u64, below: bool) -> Result<Then> {
    let idea = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::idea(&mut conn, key).await?
    };
    let Some(idea) = idea else {
        return Ok(Then::Done);
    };
    if needed(app, &idea).await? {
        match ask(app, &idea, depth, below).await? {
            Asked::Proposed(proposal, model) => keep(app, &idea, proposal, &model).await?,
            Asked::Waiting => return Ok(Then::Done),
        }
        // An idea the graph has, with its entry there already, is sent on its
        // own, as is one reached by tracing down; otherwise it goes with the
        // entry.
        match graph_id(app, key).await? {
            Some(id) => {
                let has_entry = {
                    let mut conn = app.db.acquire().await?;
                    kelve_store::entries::get(&mut conn, key)
                        .await?
                        .is_some_and(|e| e.from_graph)
                };
                if has_entry || below {
                    send_alone(app, key, &id).await?;
                }
            }
            None if below => send_new(app, &idea).await?,
            None => {}
        }
        app.publish(kelve_core::views::ServerEvent::Idea {
            key: key.to_string(),
        });
    }
    write_next(app, key).await
}

/// Writes the idea's entry next, if it's waiting on its relations.
async fn write_next(app: &Shared, key: &str) -> Result<Then> {
    let waiting = {
        let mut conn = app.db.acquire().await?;
        kelve_store::entries::get(&mut conn, key)
            .await?
            .is_some_and(|e| e.state == "writing")
    };
    if waiting {
        super::add(
            app,
            super::entries::WRITE,
            Some(&format!("{}:{key}", super::entries::WRITE)),
            json!({ "idea": key }),
            chrono::Utc::now(),
        )
        .await?;
    }
    Ok(Then::Done)
}

/// What a proposal comes to once code has held its shape: the relations to
/// show, and the changes that send them.
#[derive(Debug, Default)]
pub struct Held {
    pub relations: IdeaRelations,
    pub changes: Vec<Change>,
    /// What code left out, and why.
    pub dropped: Vec<String>,
}

/// Holds a proposal to the graph's shape, in code: nothing related to itself,
/// every why one plain sentence, no more of a kind than it may have, and a
/// broader category as what it's a kind of, never what it rests on. `found`
/// gives each name's key: the graph's ID, or a new key, with whether it's new.
pub fn hold(
    idea: &Idea,
    proposal: &Proposal,
    model: &str,
    found: &dyn Fn(&str) -> Option<(String, bool)>,
) -> Held {
    let mut held = Held::default();
    let mut added: Vec<String> = Vec::new();
    let broader: Vec<String> = proposal.kind_of.iter().map(|r| norm(&r.name)).collect();
    let groups = [
        (RelationKind::RestsOn, &proposal.rests_on),
        (RelationKind::KindOf, &proposal.kind_of),
        (RelationKind::Answers, &proposal.answers),
        (RelationKind::Alternative, &proposal.alternatives),
        (RelationKind::UsedWith, &proposal.used_with),
    ];
    for (kind, list) in groups {
        let mut taken: Vec<Rel> = Vec::new();
        for r in list {
            let Some(name) = tidy(&r.name) else { continue };
            if norm(&name) == norm(&idea.name) {
                held.dropped.push(format!("“{name}” is the idea itself."));
                continue;
            }
            if kind == RelationKind::RestsOn && broader.contains(&norm(&name)) {
                held.dropped.push(format!(
                    "“{}” is a kind of “{name}”, so it doesn’t rest on it.",
                    idea.name
                ));
                continue;
            }
            if let Some(problem) = why_problem(&r.why, &idea.definition) {
                held.dropped.push(format!("{problem} (“{name}”)"));
                continue;
            }
            if taken.len() >= kind.most() || taken.iter().any(|t| norm(&t.idea.name) == norm(&name))
            {
                continue;
            }
            let Some((key, new)) = found(&name) else {
                continue;
            };
            let to = if new {
                let reference = format!("rel{}", added.len());
                if !added.contains(&key) {
                    added.push(key.clone());
                    held.changes.push(Change::AddIdea {
                        reference: reference.clone(),
                        name: name.clone(),
                        definition: clip(&r.definition, 300),
                        kind: IdeaKind::Concept,
                        same_as: None,
                    });
                }
                let n = added.iter().position(|k| k == &key).unwrap_or(0);
                format!("ref:rel{n}")
            } else {
                key.clone()
            };
            let rank = (kind == RelationKind::RestsOn).then_some(taken.len() as u8 + 1);
            held.changes.push(Change::Relate {
                from: SELF.into(),
                kind,
                to,
                why: r.why.trim().to_string(),
                rank,
                model: Some(model.to_string()),
            });
            taken.push(Rel {
                idea: Node {
                    id: key,
                    foundation: is_foundation(&name),
                    depth: None,
                    name,
                },
                why: r.why.trim().to_string(),
                state: RelationState::Proposed,
                rank,
            });
        }
        match kind {
            RelationKind::RestsOn => held.relations.down = taken,
            RelationKind::KindOf => held.relations.kind_of = taken,
            RelationKind::Answers => held.relations.answers = taken,
            RelationKind::Alternative => held.relations.alternatives = taken,
            RelationKind::UsedWith => held.relations.used_with = taken,
        }
    }
    held
}

/// Finds the graph's idea for each name a proposal gives, adds the ones it
/// hasn't got as new ideas here, holds the proposal's shape, and keeps it.
async fn keep(app: &Shared, idea: &Idea, proposal: Proposal, model: &str) -> Result<()> {
    let all: Vec<&Related> = [
        &proposal.rests_on,
        &proposal.kind_of,
        &proposal.answers,
        &proposal.alternatives,
        &proposal.used_with,
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut matched: std::collections::HashMap<String, String> = Default::default();
    let mut theirs: Vec<Idea> = Vec::new();
    if let Some(graph) = app.graph_reader() {
        let mut req = MatchRequest::default();
        for (i, r) in all.iter().enumerate() {
            req.queries.insert(
                format!("q{i}"),
                Lookup {
                    query: r.name.clone(),
                    context: None,
                    limit: Some(1),
                },
            );
        }
        match graph.candidates(&req).await {
            Ok(found) => {
                for (i, r) in all.iter().enumerate() {
                    if let Some(c) = found
                        .get(&format!("q{i}"))
                        .and_then(|c| c.result.first())
                        .filter(|c| c.matched)
                    {
                        matched.insert(norm(&r.name), c.id.clone());
                        theirs.push(Idea {
                            key: c.id.clone(),
                            name: c.name.clone(),
                            definition: c.definition.clone(),
                            kind: IdeaKind::Concept.as_str().into(),
                            became: None,
                        });
                    }
                }
            }
            Err(e) => {
                tracing::info!(error = %e, "couldn't look up related ideas in the shared graph")
            }
        }
    }
    // Ideas the graph hasn't got are kept here, as new ideas, and the
    // graph's are kept here too, so either can be opened, or traced.
    {
        let mut conn = app.db.acquire().await?;
        for i in &theirs {
            if kelve_store::entries::idea(&mut conn, &i.key)
                .await?
                .is_none()
            {
                kelve_store::entries::keep_idea(&mut conn, i).await?;
            }
        }
        for r in &all {
            let Some(name) = tidy(&r.name) else { continue };
            if matched.contains_key(&norm(&name)) || norm(&name) == norm(&idea.name) {
                continue;
            }
            let key = super::entries::new_key(&name);
            if kelve_store::entries::idea(&mut conn, &key).await?.is_none() {
                kelve_store::entries::keep_idea(
                    &mut conn,
                    &Idea {
                        key,
                        name: name.clone(),
                        definition: clip(&r.definition, 300),
                        kind: IdeaKind::Concept.as_str().into(),
                        became: None,
                    },
                )
                .await?;
            }
        }
    }
    let found = |name: &str| -> Option<(String, bool)> {
        match matched.get(&norm(name)) {
            Some(id) => Some((id.clone(), false)),
            None => Some((super::entries::new_key(name), true)),
        }
    };
    let held = hold(idea, &proposal, model, &found);
    for why in &held.dropped {
        tracing::debug!(why, "a proposed relation was left out");
    }
    let mut conn = app.db.acquire().await?;
    kelve_store::relations::proposed(
        &mut conn,
        &idea.key,
        &serde_json::to_string(&held.relations).unwrap_or_default(),
        proposal.question.trim(),
        &serde_json::to_string(&held.changes).unwrap_or_default(),
    )
    .await?;
    Ok(())
}

/// What's been proposed for an idea and not yet sent, as the graph's
/// changes, from `from`: its ID, or `ref:idea` when it goes with the entry
/// that adds it. Refs are renamed so they can't clash with the entry's own.
pub async fn unsent(app: &Shared, key: &str, from: &str) -> Result<Vec<Change>> {
    let mut conn = app.db.acquire().await?;
    let Some(kept) = kelve_store::relations::get(&mut conn, key).await? else {
        return Ok(Vec::new());
    };
    if kept.sent || kept.source != "proposed" || !app.settings().contribute {
        return Ok(Vec::new());
    }
    let changes: Vec<Change> =
        serde_json::from_str(kept.changes.as_deref().unwrap_or("[]")).unwrap_or_default();
    Ok(changes
        .into_iter()
        .map(|c| match c {
            Change::Relate {
                from: f,
                kind,
                to,
                why,
                rank,
                model,
            } => Change::Relate {
                from: if f == SELF { from.to_string() } else { f },
                kind,
                to,
                why,
                rank,
                model,
            },
            other => other,
        })
        .collect())
}

/// Marks what was proposed for an idea as sent, and keeps the ID the graph
/// gave each new idea it named.
pub async fn settle(
    app: &Shared,
    key: &str,
    changes: &[Change],
    applied: &kelve_graph_api::Applied,
) -> Result<()> {
    let mut conn = app.db.acquire().await?;
    for c in changes {
        if let Change::AddIdea {
            reference, name, ..
        } = c
            && let Some(id) = applied.ideas.get(reference)
        {
            kelve_store::entries::became(&mut conn, &super::entries::new_key(name), id).await?;
        }
    }
    for why in &applied.skipped {
        tracing::info!(why, "the shared graph left something out");
    }
    kelve_store::relations::sent(&mut conn, key).await?;
    Ok(())
}

/// Sends a new idea, reached by tracing down, with what was proposed for
/// it: it has no entry to go with.
async fn send_new(app: &Shared, idea: &Idea) -> Result<()> {
    let Some(graph) = app.graph_writer() else {
        return Ok(());
    };
    // Kept for later while sending waits for the day's limits to reset.
    if app.sends_held().is_some() {
        return Ok(());
    }
    let related = unsent(app, &idea.key, "ref:idea").await?;
    if related.is_empty() {
        return Ok(());
    }
    let mut changes = vec![Change::AddIdea {
        reference: "idea".into(),
        name: idea.name.clone(),
        definition: clip(&idea.definition, 300),
        kind: IdeaKind::parse(&idea.kind),
        same_as: None,
    }];
    changes.extend(related);
    let cs = Changeset {
        agent: kelve_graph_api::client::AGENT.into(),
        comment: "How an idea relates, traced down from one someone opened.".into(),
        changes,
        written_by: None,
    };
    let idem = format!(
        "trace-{}",
        kelve_core::ideas::key(&idea.name).replace(' ', "-")
    );
    match super::apply(app, &graph, &cs, &idem).await {
        Ok(applied) => settle(app, &idea.key, &cs.changes, &applied).await,
        Err(GraphError::SignIn) => Ok(()),
        Err(e) => {
            tracing::info!(error = %e, "couldn't send a traced idea's relations; they go with its entry");
            Ok(())
        }
    }
}

/// Sends what was proposed for an idea the graph has, on its own.
async fn send_alone(app: &Shared, key: &str, id: &str) -> Result<()> {
    let Some(graph) = app.graph_writer() else {
        return Ok(());
    };
    // Kept for later while sending waits for the day's limits to reset.
    if app.sends_held().is_some() {
        return Ok(());
    }
    let changes = unsent(app, key, id).await?;
    if changes.is_empty() {
        return Ok(());
    }
    let cs = Changeset {
        agent: kelve_graph_api::client::AGENT.into(),
        comment: "How an idea relates, proposed when someone opened it.".into(),
        changes,
        written_by: None,
    };
    let body = serde_json::to_string(&cs).unwrap_or_default();
    let idem = {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(body.as_bytes());
        format!(
            "relate-{}",
            digest
                .iter()
                .take(8)
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        )
    };
    match super::apply(app, &graph, &cs, &idem).await {
        Ok(applied) => settle(app, key, &cs.changes, &applied).await,
        Err(GraphError::SignIn) => Ok(()),
        Err(e) => {
            tracing::info!(error = %e, "couldn't send proposed relations; they go with the next try");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn related(name: &str, why: &str) -> Related {
        Related {
            name: name.into(),
            definition: format!("What {name} is."),
            why: why.into(),
        }
    }

    #[test]
    fn code_holds_a_proposals_shape_before_anything_is_kept() {
        let idea = Idea {
            key: "kelve:i1".into(),
            name: "Prompt caching".into(),
            definition: "Reusing the work a model did on a prompt's start.".into(),
            kind: "concept".into(),
            became: None,
        };
        let proposal = Proposal {
            question: "How can a model avoid reading the same start again?".into(),
            rests_on: vec![
                related(
                    "Transformer key-value cache",
                    "What's kept is the keys attention computed.",
                ),
                related("Caching", "It keeps a result to reuse."),
                related("Prompt caching", "Itself."),
                related("Tokenization", "See [[tokens]]."),
                related("Attention", "It weighs tokens."),
                related("Set", "A prompt is a set of tokens."),
                related("Memory hierarchy", "Kept in fast memory."),
            ],
            kind_of: vec![related("Caching", "It's caching of a prompt's start.")],
            ..Proposal::default()
        };
        let found = |name: &str| -> Option<(String, bool)> {
            Some(if name == "Attention" {
                ("kelve:i9".into(), false)
            } else {
                (format!("new:{}", norm(name)), true)
            })
        };
        let held = hold(&idea, &proposal, "anthropic/claude-opus-5.5", &found);
        let down: Vec<&str> = held
            .relations
            .down
            .iter()
            .map(|r| r.idea.name.as_str())
            .collect();
        assert_eq!(
            down,
            ["Transformer key-value cache", "Attention", "Set"],
            "{:?}",
            held.dropped
        );
        assert_eq!(held.relations.down[2].rank, Some(3));
        assert!(held.relations.down[2].idea.foundation);
        assert_eq!(held.relations.kind_of[0].idea.name, "Caching");
        assert_eq!(
            held.dropped.len(),
            3,
            "itself, a broader category, a link in the why: {:?}",
            held.dropped
        );
        // New ideas are added once, by ref; ones the graph has go by ID.
        let adds = held
            .changes
            .iter()
            .filter(|c| matches!(c, Change::AddIdea { .. }))
            .count();
        assert_eq!(adds, 3, "the cache, set and caching");
        assert!(
            held.changes
                .iter()
                .any(|c| matches!(c, Change::Relate { to, .. } if to == "kelve:i9"))
        );
        assert!(held.changes.iter().all(|c| match c {
            Change::Relate { from, model, .. } =>
                from == SELF && model.as_deref() == Some("anthropic/claude-opus-5.5"),
            _ => true,
        }));
    }
}
