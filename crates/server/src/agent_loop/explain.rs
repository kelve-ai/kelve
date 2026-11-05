//! Explaining an idea the work rests on (docs/skills.md, "Explain"): the
//! person asks, from a debrief, and kelve keeps an `explain` job holding the
//! idea's name, the lines in the change that use it, and what `marks.md`
//! says they know. The explanation is kept, so the same idea isn't explained
//! twice for the same code.

use chrono::Utc;
use kelve_core::fence::fence;
use kelve_store::handovers::{self, HandedFile};
use kelve_store::{agent_loop as store, debriefs};
use serde_json::{Value, json};

use super::debrief::changed_of;
use super::research::{lines_text, marks};
use crate::agent::{self, Ask, Priority, Resume};
use crate::error::{ApiError, ApiResult};
use crate::pipeline::{self, Then};
use crate::state::Shared;
use crate::v2jobs;

/// The most lines found by the idea's words, when no decision names it.
const MOST_FOUND: usize = 40;

/// The lines of a debrief's change that use an idea: those of the decisions
/// that rest on it, or else added lines that name it.
async fn lines_for(app: &Shared, debrief: i64, idea: &str) -> ApiResult<String> {
    let mut conn = app.db.acquire().await?;
    let d = debriefs::get(&mut conn, debrief)
        .await?
        .ok_or_else(|| ApiError::not_found("That debrief isn't kept any more."))?;
    let handed = match d.handover {
        Some(h) => handovers::files(&mut conn, h).await?,
        None => Vec::new(),
    };
    let decisions = debriefs::decisions(&mut conn, debrief).await?;
    drop(conn);
    let key = debriefs::idea_key(idea);
    let read = d
        .markdown
        .as_deref()
        .and_then(|m| kelve_core::debrief::read(m, &changed_of(&handed)).ok());
    let resting: Vec<String> = read
        .map(|r| {
            r.decisions
                .iter()
                .enumerate()
                .filter(|(_, x)| x.rests_on.iter().any(|i| debriefs::idea_key(i) == key))
                .filter_map(|(n, _)| decisions.iter().find(|k| k.number == n as i64 + 1))
                .map(|k| k.lines.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut out = String::new();
    for lines in resting {
        out.push_str(&lines_text(&handed, &lines));
    }
    if out.is_empty() {
        out = named_in(&handed, idea);
    }
    Ok(out)
}

/// Added lines that name an idea, by any of its longer words.
fn named_in(handed: &[HandedFile], idea: &str) -> String {
    let words: Vec<String> = idea
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 4)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut found = 0;
    for f in handed {
        let Some(text) = f.text.as_deref() else {
            continue;
        };
        let mut head = false;
        for (n, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if found < MOST_FOUND && words.iter().any(|w| lower.contains(w.as_str())) {
                if !head {
                    out.push_str(&format!("--- {}\n", f.path));
                    head = true;
                }
                out.push_str(&format!("{:>5}  {line}\n", n + 1));
                found += 1;
            }
        }
    }
    out
}

/// Asks the agent to explain an idea a debrief's work rests on: the
/// explanation's id. The same idea for the same lines is the same
/// explanation, asked once.
pub async fn ask_for(app: &Shared, debrief: i64, idea: &str) -> ApiResult<i64> {
    let idea = idea.trim();
    if idea.is_empty() {
        return Err(ApiError::bad_request("Say which idea to explain."));
    }
    let lines = lines_for(app, debrief, idea).await?;
    let print = kelve_core::text::hash(&lines);
    let (id, ready) = {
        let mut conn = app.db.acquire().await?;
        let id = debriefs::ask_explain(&mut conn, Some(debrief), idea, &print).await?;
        let ready = store::explanation(&mut conn, id)
            .await?
            .is_some_and(|e| e.state == "ready");
        (id, ready)
    };
    if !ready {
        pipeline::add(
            app,
            v2jobs::EXPLAIN,
            Some(&format!("{}:{id}", v2jobs::EXPLAIN)),
            json!({ "explanation": id, "lines": lines }),
            Utc::now(),
        )
        .await?;
    }
    Ok(id)
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "explanation": { "type": "string", "description": "The explanation, in markdown, at most about 200 words." },
        },
        "required": ["explanation"],
    })
}

/// Whether an explanation is in its shape.
pub fn shape(value: &Value) -> Result<(), String> {
    let text = value["explanation"].as_str().unwrap_or_default();
    if kelve_core::redact::redact(text) != text {
        return Err(
            "The explanation seems to hold a secret. Take it out, and hand it in again.".into(),
        );
    }
    kelve_core::debrief::explanation(text)
        .map(|_| ())
        .map_err(|p| {
            format!(
                "Fix the explanation, then hand it in again: {}",
                p.join(" ")
            )
        })
}

/// The pipeline job: asks for the explanation its payload names, or keeps
/// the answer.
pub async fn job(app: &Shared, payload: &Value) -> pipeline::Result<Then> {
    let Some(id) = payload["explanation"].as_i64() else {
        return Ok(Then::Done);
    };
    let e = {
        let mut conn = app.db.acquire().await?;
        store::explanation(&mut conn, id).await?
    };
    let Some(e) = e.filter(|e| e.state != "ready") else {
        return Ok(Then::Done);
    };
    // The lines it was asked with, or, queued without them, found now.
    let found;
    let lines = match (payload["lines"].as_str(), e.debrief) {
        (Some(lines), _) => lines,
        (None, Some(debrief)) => {
            found = lines_for(app, debrief, &e.idea)
                .await
                .map_err(super::fail)?;
            found.as_str()
        }
        (None, None) => "",
    };
    let material = json!({
        "idea": e.idea,
        "lines": (!lines.is_empty()).then(|| fence("lines", lines)),
        "marks": fence("marks", &marks(app)),
    });
    let answer = agent::ask(
        app,
        &format!("explain:{id}"),
        Ask {
            kind: kelve_core::instructions::EXPLAIN,
            about: format!("Explaining {}", kelve_core::text::clip(&e.idea, 80)),
            idea: None,
            opened: None,
            material,
            schema: schema(),
            priority: Priority::Now,
            follows: None,
        },
        Resume::job(
            v2jobs::EXPLAIN,
            format!("{}:{id}", v2jobs::EXPLAIN),
            json!({ "explanation": id, "lines": lines, "debrief": e.debrief }),
        ),
    )
    .await?;
    let Some(answer) = answer else {
        return Ok(Then::Done);
    };
    let text = answer.value["explanation"].as_str().unwrap_or_default();
    let mut conn = app.db.acquire().await?;
    debriefs::answer_explain(&mut conn, id, text).await?;
    drop(conn);
    app.publish(kelve_core::views::ServerEvent::Asks);
    Ok(Then::Done)
}
