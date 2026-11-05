//! Research on a decision (docs/skills.md, "Research"): the person asks,
//! from a debrief, for their agent to look into a decision. kelve keeps a
//! `research` job holding their question, the decision as the debrief has
//! it, its lines of code and what `marks.md` says they know; the agent's
//! answer, in its fixed shape, is kept under the decision.

use std::collections::BTreeMap;

use kelve_core::fence::fence;
use kelve_store::handovers::{self, HandedFile};
use kelve_store::{agent_loop as store, debriefs};
use serde_json::{Value, json};

use super::{debrief::changed_of, fail};
use crate::agent::{self, Ask, Priority, Resume};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{self, Then};
use crate::state::Shared;
use crate::v2jobs;

/// The most lines of code given with a decision.
const MOST_LINES: usize = 200;

/// Asks the agent to look into a decision, with the person's question:
/// the research's id.
pub async fn ask_for(app: &Shared, decision: i64, question: &str) -> ApiResult<i64> {
    let question = question.trim();
    if question.is_empty() {
        return Err(ApiError::bad_request(
            "Say what you'd like your agent to find out.",
        ));
    }
    let id = {
        let mut conn = app.db.acquire().await?;
        debriefs::decision(&mut conn, decision)
            .await?
            .ok_or_else(|| ApiError::not_found("That decision isn't kept any more."))?;
        debriefs::ask_research(
            &mut conn,
            decision,
            &kelve_core::text::clip(question, 1_000),
        )
        .await?
    };
    v2jobs::queue(
        app,
        v2jobs::RESEARCH,
        &decision.to_string(),
        json!({ "decision": decision, "research": id }),
    )
    .await?;
    app.publish(kelve_core::views::ServerEvent::Debriefs);
    Ok(id)
}

/// The lines of a decision, numbered, from the files as they were handed
/// over: `[{path, from, to}]`, a file whole when it has no `from`.
pub fn lines_text(handed: &[HandedFile], lines: &str) -> String {
    let wanted: Vec<Value> = serde_json::from_str(lines).unwrap_or_default();
    let mut by_file: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
    for w in &wanted {
        let path = w["path"].as_str().unwrap_or_default().to_string();
        let from = w["from"].as_u64().map_or(1, |n| n as u32);
        let to = w["to"].as_u64().map_or(u32::MAX, |n| n as u32);
        by_file.entry(path).or_default().push((from, to));
    }
    let mut out = String::new();
    let mut given = 0;
    for (path, runs) in by_file {
        let Some(text) = handed
            .iter()
            .find(|f| f.path == path)
            .and_then(|f| f.text.as_deref())
        else {
            continue;
        };
        out.push_str(&format!("--- {path}\n"));
        for (n, line) in text.lines().enumerate() {
            let n = n as u32 + 1;
            if runs.iter().any(|(a, b)| (*a..=*b).contains(&n)) && given < MOST_LINES {
                out.push_str(&format!("{n:>5}  {line}\n"));
                given += 1;
            }
        }
    }
    out
}

/// What the person knows, as `marks.md` says.
pub fn marks(app: &Shared) -> String {
    kelve_core::spec::write(&crate::spec::read(app))
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "research": { "type": "string", "description": "What you found, in markdown, in exactly the shape the instructions give." },
        },
        "required": ["research"],
    })
}

/// Whether research is in its shape.
pub fn shape(value: &Value) -> Result<(), String> {
    let text = value["research"].as_str().unwrap_or_default();
    if kelve_core::redact::redact(text) != text {
        return Err(
            "The research seems to hold a secret. Take it out, and hand it in again.".into(),
        );
    }
    kelve_core::debrief::research(text)
        .map(|_| ())
        .map_err(|p| format!("Fix the research, then hand it in again: {}", p.join(" ")))
}

/// The pipeline job: asks for the research its payload names, by its id or
/// the decision's latest, or keeps the answer.
pub async fn job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let id = match (payload["research"].as_i64(), payload["decision"].as_i64()) {
        (Some(id), _) => Some(id),
        (None, Some(decision)) => {
            let mut conn = app.db.acquire().await?;
            store::research_waiting(&mut conn, decision).await?
        }
        _ => None,
    };
    let Some(id) = id else {
        return Ok(Then::Done);
    };
    let mut conn = app.db.acquire().await?;
    let Some(r) = store::research(&mut conn, id).await? else {
        return Ok(Then::Done);
    };
    if r.state == "ready" {
        return Ok(Then::Done);
    }
    let Some(decision) = debriefs::decision(&mut conn, r.decision).await? else {
        return Ok(Then::Done);
    };
    let Some(d) = debriefs::get(&mut conn, decision.debrief).await? else {
        return Ok(Then::Done);
    };
    let handed = match d.handover {
        Some(h) => handovers::files(&mut conn, h).await?,
        None => Vec::new(),
    };
    // What the work was opened as: "do what's waiting for it" takes this too.
    let opened = match d.handover {
        Some(h) => handovers::get(&mut conn, h).await?.and_then(|h| h.opened),
        None => None,
    };
    drop(conn);
    // The decision as the debrief has it: its fields and why.
    let written = d
        .markdown
        .as_deref()
        .and_then(|m| kelve_core::debrief::read(m, &changed_of(&handed)).ok())
        .and_then(|read| {
            read.decisions
                .into_iter()
                .nth(usize::try_from(decision.number - 1).unwrap_or(usize::MAX))
        });
    let about = match &written {
        Some(w) => format!(
            "### {}\n- from: {}\n- could have: {}\n- rests on: {}\n\n{}",
            w.title,
            w.from.words(),
            w.could_have.join("; "),
            w.rests_on.join(", "),
            w.why
        ),
        None => format!("### {}", decision.title),
    };
    let material = json!({
        "question": fence("question", &r.question),
        "decision": fence("decision", &about),
        "lines": Some(lines_text(&handed, &decision.lines)).filter(|l| !l.is_empty()).map(|l| fence("lines", &l)),
        "marks": fence("marks", &marks(app)),
    });
    let answer = agent::ask(
        app,
        &format!("research:{id}"),
        Ask {
            kind: kelve_core::instructions::RESEARCH,
            about: format!(
                "Looking into “{}”",
                kelve_core::text::clip(&decision.title, 80)
            ),
            idea: None,
            opened,
            material,
            schema: schema(),
            priority: Priority::Now,
            follows: None,
        },
        Resume::job(
            v2jobs::RESEARCH,
            // Keyed by the decision, as `v2jobs::queue` keys it.
            format!("{}:{}", v2jobs::RESEARCH, r.decision),
            json!({ "decision": r.decision, "research": id }),
        ),
    )
    .await?;
    let Some(answer) = answer else {
        return Ok(Then::Done);
    };
    let text = answer.value["research"].as_str().unwrap_or_default();
    let mut conn = app.db.acquire().await?;
    debriefs::answer_research(&mut conn, id, text)
        .await
        .map_err(|e| fail(ApiError::from(e)))?;
    drop(conn);
    app.publish(kelve_core::views::ServerEvent::Debriefs);
    app.publish(kelve_core::views::ServerEvent::Asks);
    Ok(Then::Done)
}
