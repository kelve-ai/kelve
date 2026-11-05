//! `kelve moment`: what kelve's hook runs, on each agent's own events
//! (`core::hook`). The installed command names the agent and the event: `kelve
//! moment --agent codex --on PostToolUse`. As the person sends a message: one
//! line when notes wait for the agent that it hasn't been told of as they are
//! now, and one, once for each version, when a newer kelve is out; and what
//! they sent, short and with secrets stripped, kept to catch what they
//! corrected (`server::moment`); and the person's skills, as their session
//! starts. As the agent reads or edits a file: the skill for that kind of
//! work, the first time. As the turn ends: to show its work in kelve, when
//! it hasn't, and the jobs waiting for it. As it opens something in kelve:
//! nothing, but kelve keeps that its session handed that work over. It
//! reads the event on standard input,
//! answers in a fraction of a second (a few seconds as a turn ends, or as
//! something is opened) or says nothing, and starts nothing when kelve isn't
//! running. Never an error.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use kelve_core::hook::{Agent, Moment, Skill, bench, moment, reply, skill};
use serde_json::Value;

use crate::client::Kelve;

/// How long the hook waits for kelve.
const WAIT: Duration = Duration::from_millis(200);
/// How long it waits as a turn ends: the agent is done, so what kelve has
/// for it then isn't lost. And as the agent opens something in kelve, which
/// is busy then keeping what was handed over: it's kept for its session.
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
pub async fn run(
    agent: Option<&str>,
    on: Option<&str>,
    bench: Option<&Path>,
) -> anyhow::Result<()> {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return Ok(());
    }
    let event: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
    let (agent, on) = which(agent, on, &event);
    let Some(m) = moment(agent, &on, &event) else {
        return Ok(());
    };
    if let Some(folder) = bench {
        let given = given_file(&m.session);
        let text = benched(&m, folder, &given);
        if let Some(out) = reply(agent, &on, &text) {
            anstream::println!("{out}");
        }
        return Ok(());
    }
    let wait = if matches!(m.what.as_str(), "end" | "opened") {
        END_WAIT
    } else {
        WAIT
    };
    let Ok(kelve) = Kelve::new(wait) else {
        return Ok(());
    };
    // As the person sends a message: only notes new, or changed, since the
    // agent was last told of them, so the line isn't said every time.
    let text = if m.what == "prompt" {
        // All at once, so the hook still answers within its wait: the notes,
        // a newer kelve, and the skills the session starts with.
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

/// Where a benchmark run's hook keeps the skills it has handed over in a
/// session: `KELVE_BENCH_GIVEN`, or a file in the temporary folder, named for
/// the session.
fn given_file(session: &str) -> PathBuf {
    if let Some(file) = std::env::var_os("KELVE_BENCH_GIVEN") {
        return PathBuf::from(file);
    }
    let session: String = session
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(64)
        .collect();
    std::env::temp_dir().join(format!("kelve-bench-given-{session}.json"))
}

/// What a benchmark run's hook says, from the skills in `folder`, one
/// folder each with its `SKILL.md`, keeping what it handed over in `given`.
/// Nothing leaves the computer it runs on: no kelve, no network.
fn benched(m: &Moment, folder: &Path, given: &Path) -> String {
    let mut skills: Vec<Skill> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let text = std::fs::read_to_string(entry.path().join("SKILL.md")).ok()?;
            Some(skill(&name, &text))
        })
        .collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    let before: Vec<String> = std::fs::read_to_string(given)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let (text, now) = bench(m, &skills, &before);
    if now != before {
        let _ = std::fs::write(given, serde_json::to_string(&now).unwrap_or_default());
    }
    text
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

    #[test]
    fn a_benchmark_run_reads_its_skills_from_a_folder() {
        let dir = std::env::temp_dir().join(format!("kelve-bench-test-{}", std::process::id()));
        let skills = dir.join("skills");
        std::fs::create_dir_all(skills.join("standin-expert")).unwrap();
        std::fs::create_dir_all(skills.join("rust")).unwrap();
        std::fs::create_dir_all(skills.join("empty")).unwrap();
        std::fs::write(
            skills.join("standin-expert/SKILL.md"),
            "---\nname: standin-expert\ndescription: Expert work\n---\nDo it well.\n",
        )
        .unwrap();
        std::fs::write(
            skills.join("rust/SKILL.md"),
            "---\nname: rust\n---\nOne error enum.\n",
        )
        .unwrap();
        let given = dir.join("given.json");
        let event = json!({ "session_id": "s1", "cwd": "/workspace", "prompt": "Add a flag" });
        let m = moment(Agent::Claude, "UserPromptSubmit", &event).unwrap();
        let said = benched(&m, &skills, &given);
        assert!(said.contains("`standin-expert`: Expert work"), "{said}");
        assert_eq!(benched(&m, &skills, &given), "");
        let read = json!({
            "session_id": "s1", "cwd": "/workspace", "tool_name": "Read",
            "tool_input": { "file_path": "/workspace/src/lib.rs" }
        });
        let m = moment(Agent::Claude, "PostToolUse", &read).unwrap();
        assert!(benched(&m, &skills, &given).contains("One error enum."));
        // A folder that isn't there says nothing.
        assert_eq!(benched(&m, &dir.join("none"), &dir.join("g2.json")), "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
