//! `kelve moment`: what kelve's hook runs, on each agent's own events
//! (`core::hook`). The installed command names the agent and the event: `kelve
//! moment --agent codex --on PostToolUse`. As the person sends a message: one
//! line when notes wait for the agent that it hasn't been told of as they are
//! now, and one, once for each version, when a newer kelve is out. As the agent
//! reads or edits a file, runs a command, or something fails: the lesson from
//! the shared graph that helps now, if there is one. As the turn ends, after a
//! hard stretch of failures the agent got past, one question asking what it
//! learned; an agent with no such event is asked at the person's next message.
//! It reads the event on standard input, answers in a fraction of a second (a
//! few seconds as a turn ends) or says nothing, and starts nothing when kelve
//! isn't running. Never an error.

use std::io::Read;
use std::time::Duration;

use kelve_core::hook::{Agent, moment, reply};
use serde_json::Value;

use crate::client::Kelve;

/// How long the hook waits for kelve.
const WAIT: Duration = Duration::from_millis(200);
/// How long it waits as a turn ends: the agent is done, so a question about
/// what the turn taught, worked out from every file's ideas, isn't lost.
const END_WAIT: Duration = Duration::from_secs(3);

/// The agent and event, from the command's flags, else from the event, for
/// a hook installed before they were named: Claude Code's.
fn which(agent: Option<&str>, on: Option<&str>, event: &Value) -> (Agent, String) {
    let agent = agent.and_then(Agent::parse).unwrap_or(Agent::Claude);
    let on = on
        .map(str::to_string)
        .or_else(|| event["hook_event_name"].as_str().map(str::to_string))
        .unwrap_or_else(|| "UserPromptSubmit".into());
    (agent, on)
}

/// `kelve moment`.
pub async fn run(agent: Option<&str>, on: Option<&str>) -> anyhow::Result<()> {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return Ok(());
    }
    let event: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
    let (agent, on) = which(agent, on, &event);
    let Some(m) = moment(agent, &on, &event) else {
        return Ok(());
    };
    let wait = if m.what == "end" { END_WAIT } else { WAIT };
    let Ok(kelve) = Kelve::new(wait) else {
        return Ok(());
    };
    // As the person sends a message: only notes new, or changed, since the
    // agent was last told of them, so the line isn't said every time.
    let text = if m.what == "prompt" {
        // All at once, so the hook still answers within its wait: the notes,
        // a newer kelve, and work the agent replaced, whose second way
        // stands now the person has written again.
        let (notes, told, asked) = tokio::join!(
            crate::notes::new_here(&kelve, &m.agent),
            kelve.send(kelve.post("/api/agent/update/tell")),
            kelve.send(kelve.post("/api/agent/moment").json(&m))
        );
        let notes = notes.ok().and_then(crate::notes::waiting_line);
        let update = told
            .ok()
            .and_then(|t| t["line"].as_str().map(str::to_string));
        let asked = asked
            .ok()
            .and_then(|t| t["text"].as_str().map(str::to_string))
            .filter(|t| !t.is_empty());
        [notes, update, asked]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        match kelve.send(kelve.post("/api/agent/moment").json(&m)).await {
            Ok(v) => v["text"].as_str().unwrap_or_default().to_string(),
            Err(_) => String::new(),
        }
    };
    if let Some(out) = reply(agent, &on, &text) {
        anstream::println!("{out}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_hook_installed_before_agents_were_named_is_claude_codes() {
        let (agent, on) = which(
            None,
            None,
            &json!({ "hook_event_name": "PostToolUseFailure" }),
        );
        assert_eq!((agent, on.as_str()), (Agent::Claude, "PostToolUseFailure"));
        let (agent, on) = which(Some("cursor"), Some("postToolUse"), &json!({}));
        assert_eq!((agent, on.as_str()), (Agent::Cursor, "postToolUse"));
    }
}
