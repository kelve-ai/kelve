//! `kelve job`, `kelve job done` and `kelve kept`: kelve's work, for an agent
//! that can't speak MCP, as JSON on the command line.

use std::io::Read;
use std::time::Duration;

use anyhow::Context;
use serde_json::{Value, json};

use crate::client::Kelve;

/// `kelve job`: prints the next job as JSON, or nothing when there's none;
/// with `--for`, the next for that one thing opened.
pub async fn next(agent: &str, model: Option<&str>, only: Option<i64>) -> anyhow::Result<()> {
    crate::open::started().await?;
    let kelve = Kelve::new(Duration::from_secs(60))?;
    let job = kelve
        .send(kelve.post("/api/agent/next").json(
            &json!({ "agent": agent, "model": model, "path": crate::notes::here(), "for": only }),
        ))
        .await?;
    if job.is_null() {
        anstream::eprintln!("There's no more of kelve's work.");
        return Ok(());
    }
    anstream::println!("{}", serde_json::to_string_pretty(&job)?);
    Ok(())
}

/// `kelve job done <job>`: hands in the answer on standard input, and
/// prints what became of it, with the next job when it was taken.
pub async fn done(
    job: i64,
    agent: &str,
    model: Option<&str>,
    only: Option<i64>,
) -> anyhow::Result<()> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .context("kelve couldn't read the answer from standard input.")?;
    let answer: Value = serde_json::from_str(&text)
        .context("The answer isn't JSON. Give it on standard input, in the job's answerShape.")?;
    let kelve = Kelve::new(Duration::from_secs(120))?;
    let got = kelve
        .send(kelve.post("/api/agent/answer").json(&json!({
            "job": job, "answer": answer, "agent": agent, "model": model, "path": crate::notes::here(), "for": only,
        })))
        .await?;
    anstream::println!("{}", serde_json::to_string_pretty(&got)?);
    if got["taken"] != "yes" {
        anyhow::bail!(
            "Not taken: {} Hand it in again, fixed.",
            got["why"].as_str().unwrap_or_default()
        );
    }
    Ok(())
}

/// `kelve kept`: what's kept for the agent, the most wanted first.
pub async fn kept() -> anyhow::Result<()> {
    let kelve = Kelve::new(Duration::from_secs(10))?;
    let kept = kelve.send(kelve.get("/api/agent/kept")).await?;
    match kept.as_array() {
        Some(kept) if !kept.is_empty() => {
            for k in kept {
                anstream::println!("{}  {}", k["id"], k["about"].as_str().unwrap_or_default());
            }
        }
        _ => anstream::println!("Nothing's kept for your agent."),
    }
    Ok(())
}
