//! `kelve mcp`: kelve's tools for the person's own coding agent, over MCP on
//! standard input and output. The agent starts it, as it starts any MCP server;
//! it starts kelve in the background if it isn't running, and speaks to it with
//! this computer's secret.
//!
//! The tools: `open`, to show the person something; `next_job` and
//! `hand_in`, to do kelve's work; `kept`, to see what's waiting; `skills`, for
//! the person's skills when kelve's hook didn't give them; and `update`, to
//! update kelve once the person says yes. When a newer kelve is out, the
//! end of one tool's answer says so, once for each version.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::client::Kelve;

/// The versions of MCP this speaks, newest first.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// What the agent is told of kelve when it connects.
const INSTRUCTIONS: &str = "kelve turns what the person you're working with knows into skills their agents use, and shows them what you did and why. Use kelve's tools to hand over your work when you've finished it, and to do the jobs kelve keeps for you. When you've finished what they asked, and it changed files they'd want to see, yours or a subagent's, hand it over in kelve, once, before you hand back to them, without being asked: open, with what: worktree and the branch it'll merge into as base, in a worktree or not; or, in a folder that isn't in a git repository, what: files, with every file you've changed. Give it a short title, and a line on what you changed and why that names any call you made. Hand over once, at the end of your turn: not after each edit, and not when you stop to ask them something. open starts kelve if it isn't running, and opens it in a new browser tab: never start or open kelve another way as well, such as `kelve serve`, `kelve open` or `kelve diff`. It's the same work in kelve each time, brought up to date, so hand it over again only when it has changed since: committing it, pushing it or opening a pull request changes nothing in it. When they ask to see something else, such as a folder, a file or the whole repository, open that. When they say \"Open kelve\", or ask to see kelve itself, use open with what: home and nothing else: it opens kelve at home, and the first time they'll meet it there. When open says a debrief is due, write it straight away, before you hand back to them, while you remember why you did each thing: take the job with next_job, giving for the number open gave, write it as the job says, and hand it in with hand_in, giving for too. When they ask for a debrief, as \"debrief this\", hand the work over with debrief: true. kelve's other jobs wait until you've finished what you're doing: looking into a decision, explaining an idea, and adding what the person showed they want to their skills. When kelve's hook says jobs are waiting, or they say \"Do kelve's jobs\", or ask you to do what's waiting in kelve, take them with next_job, do each as it says, and hand each in with hand_in, until there are none: for one thing, with its number, give that number as for; with none, leave for out. When kelve says it's still preparing the next part, call next_job again with the same for. The person's skills come to you through kelve's hook as you work: follow them as theirs. When the hook didn't give them, or names one there was no room for, read them with skills. Before you start anything they ask, check kept: a line starting \"Redo in\" the project you're working in is a decision in your work they want another way, and it comes first, because it changes the work. Kelve's hook says so too. Take it with next_job, do it, hand it in with hand_in, then open what you changed. What kelve keeps in the project's `.kelve/`, the project's skills, goes with the work: commit it with what you changed, and never add it to `.gitignore`. When they ask about kelve itself, such as what it does, how to set it up or what it keeps, read https://kelve.ai/llms.txt, which lists every page of kelve's docs as Markdown, then the pages that answer it, and answer from them. Then tell them, in a sentence, what you did.";

/// How long a call to kelve may take: handing in an answer runs the work
/// it was for.
const WAIT: Duration = Duration::from_secs(120);

/// Runs kelve's tools on standard input and output until the agent closes
/// them.
pub async fn serve() -> anyhow::Result<()> {
    crate::open::started().await?;
    let kelve = Kelve::new(WAIT)?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    let mut agent = "agent".to_string();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            write(&mut out, error(Value::Null, -32700, "That isn't JSON.")).await?;
            continue;
        };
        let id = message.get("id").cloned();
        let answer = match message["method"].as_str().unwrap_or_default() {
            "initialize" => {
                let (result, name) = initialize(&message["params"]);
                agent = name.unwrap_or(agent);
                hello(&kelve, &agent).await;
                Ok(result)
            }
            "tools/call" => {
                // kelve may have stopped since the agent started these tools:
                // started again, it's told who's calling, and the call goes on.
                if !crate::client::running(&kelve.url).await {
                    match crate::open::started().await {
                        Ok(()) => hello(&kelve, &agent).await,
                        Err(e) => {
                            let answer = said(format!("{e:#}"), true);
                            if let Some(id) = id {
                                write(
                                    &mut out,
                                    json!({ "jsonrpc": "2.0", "id": id, "result": answer }),
                                )
                                .await?;
                            }
                            continue;
                        }
                    }
                }
                let answer = call(&kelve, &agent, &message["params"]).await;
                Ok(with_news(&kelve, &message["params"], answer).await)
            }
            method => match plain(method) {
                Some(answer) => answer,
                None if id.is_none() => continue,
                None => Err((-32601, "kelve doesn't know that method.")),
            },
        };
        // A notification gets no answer.
        let Some(id) = id else {
            continue;
        };
        let reply = match answer {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, said)) => error(id, code, said),
        };
        write(&mut out, reply).await?;
    }
    Ok(())
}

/// A tool's answer, with the line saying a newer kelve is out at its end,
/// the first time kelve gives it for that version, and the one saying these
/// tools are an older kelve's. Not on `update`'s own.
async fn with_news(kelve: &Kelve, params: &Value, mut answer: Value) -> Value {
    if params["name"] == "update" {
        return answer;
    }
    let told = kelve
        .send(kelve.post("/api/agent/update/tell"))
        .await
        .ok()
        .and_then(|t| t["line"].as_str().map(str::to_string));
    let lines: Vec<String> = [told, older(kelve).await].into_iter().flatten().collect();
    if let (false, Some(text)) = (lines.is_empty(), answer["content"][0]["text"].as_str()) {
        answer["content"][0]["text"] = json!(format!("{text}\n\n{}", lines.join("\n\n")));
    }
    answer
}

/// Whether these tools have said they're an older kelve's.
static SAID_OLDER: AtomicBool = AtomicBool::new(false);

/// The line saying kelve was updated since these tools started, so they're
/// still the older version's, its instructions too: once, since only the
/// agent restarting them, or the person reconnecting them, starts the new.
async fn older(kelve: &Kelve) -> Option<String> {
    if SAID_OLDER.load(Ordering::Relaxed) {
        return None;
    }
    let running = kelve.send(kelve.get("/api/agent/update")).await.ok()?;
    let line = older_line(running["version"].as_str()?, env!("CARGO_PKG_VERSION"))?;
    SAID_OLDER.store(true, Ordering::Relaxed);
    Some(line)
}

fn older_line(running: &str, ours: &str) -> Option<String> {
    kelve_core::update::is_newer(running, ours).then(|| {
        format!(
            "kelve was updated to {running} since its tools here started, so they're still {ours}'s. Tell the person, in a sentence, to reconnect kelve's MCP server, or restart you, for the new ones."
        )
    })
}

/// Tells kelve which agent is calling.
async fn hello(kelve: &Kelve, agent: &str) {
    let hello = kelve
        .send(
            kelve
                .post("/api/agent/hello")
                .json(&json!({ "agent": agent })),
        )
        .await;
    if let Err(e) = hello {
        tracing::info!(error = %e, "kelve didn't hear the agent say hello");
    }
}

/// The answer to `initialize`: what kelve is and speaks, in the client's
/// version of MCP when kelve speaks it; and the agent's name, as it gave it.
fn initialize(params: &Value) -> (Value, Option<String>) {
    let asked = params["protocolVersion"].as_str().unwrap_or_default();
    let version = VERSIONS
        .iter()
        .find(|v| **v == asked)
        .copied()
        .unwrap_or(VERSIONS[1]);
    (
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "kelve", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        }),
        params["clientInfo"]["name"].as_str().map(str::to_string),
    )
}

/// The answer to a method that needs nothing of kelve, or None for one that
/// isn't that.
fn plain(method: &str) -> Option<Result<Value, (i64, &'static str)>> {
    match method {
        "ping" => Some(Ok(json!({}))),
        "tools/list" => Some(Ok(json!({ "tools": tools() }))),
        _ => None,
    }
}

fn error(id: Value, code: i64, said: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": said } })
}

async fn write(out: &mut tokio::io::Stdout, message: Value) -> anyhow::Result<()> {
    let mut line = serde_json::to_vec(&message).context("an answer that won't write")?;
    line.push(b'\n');
    out.write_all(&line).await?;
    out.flush().await?;
    Ok(())
}

/// The `for` every job tool takes: one thing opened, by its number.
fn for_property() -> Value {
    json!({ "type": "integer", "description": "What's open in kelve to take jobs for, by its number: only its jobs come. open says the number of what you opened; the person gives one when they ask you to do what's waiting for one thing, as \"(number 12)\". Leave it out to take any job." })
}

/// The model the agent says it runs on, as a tool's input asks for it.
fn model_property() -> Value {
    json!({
        "type": "string",
        "description": "The model you're running on, as you'd name it: claude-opus-5-5, say. kelve keeps it with what you hand in.",
    })
}

/// What the agent says it has of kelve's instructions, as next_job and
/// hand_in ask for it: those come by version alone.
fn have_property() -> Value {
    json!({
        "type": "array",
        "items": { "type": "string" },
        "description": "The versions of kelve's instructions, and the answer shapes, you've been given in full in this conversation, as jobs name them: debrief@1 and shape:1a2b3c4d, say. A job following one of those comes with its name alone. Leave out any you no longer have in front of you.",
    })
}

/// How long a job may be before Claude Code keeps it in a file for the
/// agent to read back, in characters: for the rare job past its usual
/// limit of 50,000, such as a debrief of a large change, which reading back
/// would cost a turn.
fn job_size() -> Value {
    json!({ "anthropic/maxResultSizeChars": 100_000 })
}

/// kelve's tools, as the agent is told of them.
fn tools() -> Value {
    json!([
        {
            "name": "open",
            "title": "Show the person something in kelve",
            "description": "Hands your work over to the person in kelve, or opens what they asked to see, in a new browser tab, starting kelve if it isn't running. Use it once you've finished what they asked and it changed files they'd want to see, committed or not, without being asked, as what: worktree with the branch it'll merge into as base, or what: files with every file you've changed outside a git repository, and again only when that work has changed since; and when they ask to see something in kelve, once each time. When they say \"Open kelve\", use what: home, and nothing else. Never start or open kelve another way as well. When it says a debrief is due, write it at once: take the job with next_job, giving for the number it gave.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "what": {
                        "type": "string",
                        "enum": ["change", "worktree", "folder", "files", "file", "home"],
                        "description": "change: what isn't committed, a branch against a base, or one commit. worktree: everything on your branch since it left its base, committed or not, in a worktree or not: how to show your own work. folder: a folder, or the whole repository. files: some files. file: one file. home: kelve itself, where the person starts: what \"Open kelve\" means. It takes nothing else.",
                    },
                    "path": { "type": "string", "description": "The folder or file, from the root: the folder you're working in, for a change or a worktree." },
                    "paths": { "type": "array", "items": { "type": "string" }, "description": "For files: each file, from the root." },
                    "base": { "type": "string", "description": "For a change or a worktree: the branch it's read against, main say. Leave it out for what isn't committed." },
                    "commit": { "type": "string", "description": "For a change: one commit." },
                    "title": { "type": "string", "description": "What it is, in a few words, at most 80 characters, as a title: Retries that back off with jitter." },
                    "said": { "type": "string", "description": "One line on what it is, or what you changed and why, at most 200 characters, in plain words: name any call you made, and what else you could have done." },
                    "debrief": { "type": "boolean", "description": "true when the person asked for a debrief of this work, as \"debrief this\": kelve asks for one whatever their settings say." },
                    "model": model_property(),
                },
                "required": ["what"],
            },
        },
        {
            "name": "next_job",
            "title": "Take kelve's next job",
            "description": "Takes the next of kelve's jobs: writing the debrief of work you handed over, looking into a decision in it, explaining an idea it rests on, turning what the person corrected, changed or sent into parts of their skills, or redoing a decision they want another way. Do it as its instructions say, then hand in the answer with hand_in, in exactly the shape it gives. Says so when there's no more, and when kelve is still preparing the next part, to call it again.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "have": have_property(),
                    "model": model_property(),
                    "for": for_property(),
                },
            },
            "_meta": job_size(),
        },
        {
            "name": "hand_in",
            "title": "Hand in a job's answer",
            "description": "Hands in your answer to one of kelve's jobs. kelve checks its shape at once: one not in the shape asked for comes back with why, to hand in again, fixed. When it's taken, the next job comes with it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "job": { "type": "integer", "description": "The job's number, as next_job gave it." },
                    "answer": { "type": "object", "description": "Your answer, in the job's answerShape." },
                    "more": { "type": "boolean", "description": "false when this is the last job you'll take: no next one comes with it, and you're told how many are left. Leave it out to take the next." },
                    "have": have_property(),
                    "model": model_property(),
                    "for": for_property(),
                },
                "required": ["job", "answer"],
            },
            "_meta": job_size(),
        },
        {
            "name": "kept",
            "title": "See what's kept for you",
            "description": "Lists kelve's work waiting for you, the most wanted first, as the person sees it.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "skills",
            "title": "Read the person's skills",
            "description": "The person's skills, their own and the project's you're working in: with no name, each by name, with what it's for and when to use it; with a name, that skill whole. kelve's hook gives them to you as you work: read them here when it didn't, or when it names one there was no room for. Follow them as theirs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "One skill, by its name, to read it whole. Leave it out to list them." },
                },
            },
        },
        {
            "name": "update",
            "title": "Update kelve",
            "description": "Updates kelve to its newest version: downloads it, checks it, puts it in place, and restarts kelve if it's running. Call it only after the person has said yes to updating kelve, never on your own. It says what it did in a sentence: tell them.",
            "inputSchema": { "type": "object", "properties": {} },
        },
    ])
}

/// What a tool call came to: text for the agent, and whether it failed.
fn said(text: impl Into<String>, failed: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": failed })
}

/// A job, as the agent is given it: its instructions first, or only their
/// version when it says it has them; then what to do it with, and the
/// shape of the answer, or only its name when it has that too, so neither
/// is sent with every job (Nathan, 29 October: the token savings, on every
/// job).
fn job_text(job: &Value, have: &[String]) -> String {
    let version = job["version"].as_str().unwrap_or_default();
    let instructions = if have.iter().any(|h| h == version) {
        format!("Do it as the instructions for {version} say, as you were given them.")
    } else {
        format!(
            "{}\n\n(Once you have these, list {version} in have as you take jobs, and they come by version alone.)",
            job["instructions"].as_str().unwrap_or_default()
        )
    };
    let mut material = String::new();
    material_text(&job["material"], 0, &mut material);
    let shape = serde_json::to_string(&job["answerShape"]).unwrap_or_default();
    let named = shape_name(&shape);
    let shape = if have.contains(&named) {
        format!("in the shape {named}, as you were given it.")
    } else {
        format!(
            "in this shape, {named}:\n{shape}\n\n(Once you have it, list {named} in have as well, and it comes by name alone.)"
        )
    };
    format!(
        "Job {} ({version}): {}. {} more after this.\n\n{instructions}\n\nThe material:\n{}\nHand in your answer with hand_in, as job {}, {shape}",
        job["job"],
        job["about"].as_str().unwrap_or_default(),
        job["waiting"],
        material,
        job["job"],
    )
}

/// An answer shape's name: `shape:` and eight hex digits of a hash of it
/// (FNV-1a), the same wherever kelve runs, so a shape changed is a new name.
fn shape_name(shape: &str) -> String {
    let hash = shape.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("shape:{:08x}", hash >> 32)
}

/// A job's material as the agent reads it: a line for each field, and text
/// of more than one line, such as a fenced passage, as it is, with nothing
/// escaped. Empty fields are left out.
fn material_text(value: &Value, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let Some(fields) = value.as_object() else {
        out.push_str(&format!(
            "{pad}{}\n",
            serde_json::to_string(value).unwrap_or_default()
        ));
        return;
    };
    let long = |v: &Value| v.as_str().is_some_and(|t| t.contains('\n'));
    for (k, v) in fields {
        match v {
            Value::Null => {}
            Value::String(t) if t.is_empty() => {}
            Value::String(t) if t.contains('\n') => out.push_str(&format!("{pad}{k}:\n{t}\n")),
            Value::String(t) => out.push_str(&format!("{pad}{k}: {t}\n")),
            Value::Array(items) if items.is_empty() => {}
            Value::Object(o) if o.is_empty() => {}
            Value::Array(items) if items.iter().any(Value::is_object) => {
                for (n, item) in items.iter().enumerate() {
                    out.push_str(&format!("{pad}{k} {}:\n", n + 1));
                    material_text(item, depth + 1, out);
                }
            }
            Value::Array(items) if items.iter().any(long) => {
                out.push_str(&format!("{pad}{k}:\n"));
                for item in items {
                    match item.as_str() {
                        Some(t) => out.push_str(&format!("{t}\n")),
                        None => out.push_str(&format!(
                            "{}\n",
                            serde_json::to_string(item).unwrap_or_default()
                        )),
                    }
                }
            }
            Value::Object(_) => {
                out.push_str(&format!("{pad}{k}:\n"));
                material_text(v, depth + 1, out);
            }
            other => out.push_str(&format!(
                "{pad}{k}: {}\n",
                serde_json::to_string(other).unwrap_or_default()
            )),
        }
    }
}

/// The versions of kelve's instructions the agent says it has.
fn have_of(args: &Value) -> Vec<String> {
    args["have"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| h.as_str().map(str::to_string))
        .collect()
}

/// A path the agent gave, whole: one from where it works, such as `.` or
/// `src`, is from `here`.
fn from_here(here: &str, path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() || here.is_empty() {
        return path.to_string();
    }
    let whole = std::path::Path::new(here).join(p);
    let whole = std::path::absolute(&whole).unwrap_or(whole);
    let whole = whole.display().to_string();
    whole.strip_suffix("/.").unwrap_or(&whole).to_string()
}

/// What the agent is told of what it opened: what kelve shows, and to take
/// its jobs, giving for its number, which kelve's hook reads back
/// (`core::hook::opened_in`).
fn opened_text(o: &Value) -> String {
    if o["debrief"]["due"] == true {
        return debrief_text(o);
    }
    let mut text = crate::open::opened_line(o);
    if !o["shown"].as_bool().unwrap_or(false) {
        text.push_str(" kelve wasn't open in their browser, so it opened it there, or showed a link where kelve runs: if they don't see it, they can run `kelve open`.");
    }
    let id = o["id"].as_i64().unwrap_or_default();
    text.push_str(&format!(" Take kelve's work for it with next_job, giving for: {id}, and hand in each answer with hand_in, until there's none."));
    text
}

/// What the agent is told of opening kelve at home: whether a browser opened.
fn home_text(answer: &Value) -> String {
    if answer["opened"].as_bool().unwrap_or(false) {
        "kelve is open in the person's browser, at home. Tell them, in a sentence, that it's there, and that their first debrief comes when you next hand over work.".into()
    } else {
        "kelve couldn't open a browser, so it showed a link where it runs. Tell them, in a sentence, to run `kelve open` in a terminal.".into()
    }
}

/// What the agent is told of work it handed over when a debrief is due: to
/// write it now, by the number of what it opened, which kelve's hook reads
/// back (`core::hook::opened_in`).
fn debrief_text(o: &Value) -> String {
    let id = o["id"].as_i64().unwrap_or_default();
    let mut text = format!(
        "kelve is showing the person {}.",
        o["title"].as_str().unwrap_or("your work")
    );
    if !o["shown"].as_bool().unwrap_or(false) {
        text.push_str(" kelve wasn't open in their browser, so it opened it there, or showed a link where kelve runs: if they don't see it, they can run `kelve open`.");
    }
    let reason = o["debrief"]["reason"].as_str().unwrap_or_default();
    if o["debrief"]["again"] == true {
        text.push_str(" It's more on work whose debrief they haven't sent yet: bring that debrief up to date.");
    } else {
        text.push_str(&format!(" A debrief of this work is due: {reason}"));
    }
    text.push_str(&format!(
        " Write it now, before you hand back to them, while you remember why you did each thing: take the job with next_job, giving for: {id}, write it as the job says, and hand it in with hand_in, giving for: {id}. Then tell them, in a sentence, that the debrief is ready in kelve."
    ));
    text
}

/// What the agent is told when no job came in time, but kelve is still
/// preparing the next part of what it asked for: to ask again, not that
/// there's no more.
fn preparing(only: Option<i64>) -> String {
    match only {
        Some(id) => format!(
            "kelve is still preparing the next part of that: call next_job again, giving for: {id}."
        ),
        None => "kelve is still preparing the next part: call next_job again.".into(),
    }
}

/// The person's skills as the agent reads them: each by name, with what
/// it's for, or one whole.
fn skills_text(v: &Value, one: bool) -> String {
    let none = Vec::new();
    let skills = v.as_array().unwrap_or(&none);
    if one && let Some(s) = skills.first() {
        return format!(
            "The person's {} skill, from {}. Follow it as theirs.\n\n{}",
            s["name"].as_str().unwrap_or_default(),
            s["file"].as_str().unwrap_or_default(),
            s["markdown"].as_str().unwrap_or_default()
        );
    }
    if skills.is_empty() {
        return "The person has no skills yet. kelve makes them from what they correct, change and send from debriefs.".into();
    }
    let mut text = String::from(
        "The person's skills. Read one whole with skills, giving its name, when it applies to what you're doing:\n",
    );
    for s in skills {
        let kind = match s["kind"].as_str().unwrap_or_default() {
            "like" => "how they like it",
            "project" => "this project",
            _ => "how they do something",
        };
        text.push_str(&format!(
            "\n- {} ({kind}): {}",
            s["name"].as_str().unwrap_or_default(),
            s["description"].as_str().unwrap_or_default()
        ));
    }
    text
}

/// Runs one of kelve's tools.
async fn call(kelve: &Kelve, agent: &str, params: &Value) -> Value {
    let args = &params["arguments"];
    let model = args["model"].as_str();
    let here = crate::hook::here();
    let only = args["for"].as_i64();
    let who =
        json!({ "agent": agent, "model": model, "path": here, "for": only, "saysPreparing": true });
    match params["name"].as_str().unwrap_or_default() {
        "open" if args["what"] == "home" => {
            match kelve.send(kelve.post("/api/signin-link")).await {
                Ok(a) => said(home_text(&a), false),
                Err(e) => said(format!("{e:#}"), true),
            }
        }
        "open" => {
            let mut body = args.clone();
            if let Some(o) = body.as_object_mut() {
                o.insert("agent".into(), json!(agent));
                // A path from where the agent works, such as `.`, is from here.
                if let Some(p) = o.get("path").and_then(Value::as_str) {
                    let whole = from_here(&here, p);
                    o.insert("path".into(), json!(whole));
                }
            }
            match kelve.send(kelve.post("/api/agent/open").json(&body)).await {
                Ok(o) => said(opened_text(&o), false),
                Err(e) => said(format!("{e:#}"), true),
            }
        }
        "next_job" => match kelve.send(kelve.post("/api/agent/next").json(&who)).await {
            Ok(v) if v["preparing"] == true => said(preparing(only), false),
            Ok(Value::Null) => said(
                if only.is_some() {
                    "There's no more of kelve's work for that. Tell the person, in a sentence, what you did."
                } else {
                    "There's no more of kelve's work. Tell the person, in a sentence, what you did."
                },
                false,
            ),
            Ok(job) => said(job_text(&job, &have_of(args)), false),
            Err(e) => said(format!("{e:#}"), true),
        },
        "hand_in" => {
            let Some(job) = args["job"].as_i64() else {
                return said("Say which job, by its number.", true);
            };
            // Some agents send the answer as a string of JSON.
            let answer = match &args["answer"] {
                Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
                other => other.clone(),
            };
            let more = args["more"].as_bool().unwrap_or(true);
            let body = json!({ "job": job, "answer": answer, "more": more, "agent": agent, "model": model, "path": here, "for": only });
            match kelve
                .send(kelve.post("/api/agent/answer").json(&body))
                .await
            {
                Ok(a) if a["taken"] == "yes" && !more => said(
                    format!(
                        "Taken. That was your last: stop here, and say that {} of kelve's jobs are left.",
                        a["left"].as_i64().unwrap_or_default()
                    ),
                    false,
                ),
                Ok(a) if a["taken"] == "yes" => match &a["next"] {
                    Value::Null if a["preparing"] == true => {
                        said(format!("Taken. {}", preparing(only)), false)
                    }
                    Value::Null => said(
                        "Taken. That was kelve's last job for now. Tell the person, in a sentence, what you did.",
                        false,
                    ),
                    next => said(
                        format!("Taken. The next one:\n\n{}", job_text(next, &have_of(args))),
                        false,
                    ),
                },
                Ok(a) => {
                    let why = a["why"].as_str().unwrap_or_default();
                    if a["putAside"].as_bool().unwrap_or(false) {
                        said(
                            format!(
                                "Not taken: {why} It's been put aside. Take the next job with next_job."
                            ),
                            true,
                        )
                    } else {
                        said(format!("Not taken: {why} Hand it in again, fixed."), true)
                    }
                }
                Err(e) => said(format!("{e:#}"), true),
            }
        }
        "kept" => match kelve.send(kelve.get("/api/agent/kept")).await {
            Ok(Value::Array(kept)) if kept.is_empty() => said("Nothing's kept for you.", false),
            Ok(Value::Array(kept)) => said(
                kept.iter()
                    .map(|k| format!("- {}", k["about"].as_str().unwrap_or_default()))
                    .collect::<Vec<_>>()
                    .join("\n"),
                false,
            ),
            Ok(_) => said("Nothing's kept for you.", false),
            Err(e) => said(format!("{e:#}"), true),
        },
        "skills" => {
            let name = args["name"]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty());
            let mut query = vec![("path", here.clone())];
            if let Some(n) = name {
                query.push(("name", n.to_string()));
            }
            match kelve
                .send(kelve.get("/api/agent/skills").query(&query))
                .await
            {
                Ok(v) => said(skills_text(&v, name.is_some()), false),
                Err(e) => said(format!("{e:#}"), true),
            }
        }
        "update" => match crate::update::run().await {
            Ok(done) => said(done, false),
            Err(e) => said(format!("{e:#}"), true),
        },
        other => said(format!("kelve has no tool called {other}."), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_older_than_the_kelve_running_say_so() {
        let line = older_line("0.1.4", "0.1.3").unwrap();
        assert!(line.starts_with("kelve was updated to 0.1.4"), "{line}");
        assert!(line.contains("still 0.1.3's") && line.contains("reconnect kelve's MCP server"));
        assert_eq!(older_line("0.1.3", "0.1.3"), None);
        assert_eq!(
            older_line("0.1.2", "0.1.3"),
            None,
            "kelve going back isn't news"
        );
    }

    #[test]
    fn a_path_from_where_the_agent_works_is_made_whole() {
        // Whole paths as each platform writes them: `/w/dice` has no drive
        // on Windows, so isn't one there.
        let dice = std::env::temp_dir().join("dice");
        let here = dice.display().to_string();
        let path = |p: String| std::path::PathBuf::from(p);
        assert_eq!(path(from_here(&here, ".")), dice);
        assert_eq!(
            path(from_here(&here, "src/lib.rs")),
            dice.join("src").join("lib.rs")
        );
        let elsewhere = std::env::temp_dir().join("elsewhere").display().to_string();
        assert_eq!(from_here(&here, &elsewhere), elsewhere);
    }

    #[test]
    fn every_tool_says_what_it_takes() {
        let tools = tools();
        let none = Vec::new();
        let listed = tools.as_array().unwrap_or(&none);
        let names: Vec<&str> = listed.iter().filter_map(|t| t["name"].as_str()).collect();
        assert_eq!(
            names,
            ["open", "next_job", "hand_in", "kept", "skills", "update"]
        );
        for t in listed {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
            assert!(!t["description"].as_str().unwrap_or_default().is_empty());
        }
    }

    #[test]
    fn an_agent_is_answered_in_its_own_version_and_named() {
        let (said, name) = initialize(&json!({
            "protocolVersion": "2025-03-26",
            "clientInfo": { "name": "claude-code", "version": "2.1" },
        }));
        assert_eq!(said["protocolVersion"], "2025-03-26");
        assert_eq!(said["serverInfo"]["name"], "kelve");
        assert!(
            said["instructions"]
                .as_str()
                .unwrap_or_default()
                .contains("use open with what: home")
        );
        // What the person sent back from a debrief reaches an agent with no hook too.
        assert!(
            said["instructions"]
                .as_str()
                .unwrap_or_default()
                .contains("Before you start anything they ask, check kept")
        );
        // Asked about kelve itself, the agent reads kelve's docs.
        assert!(
            said["instructions"]
                .as_str()
                .unwrap_or_default()
                .contains("https://kelve.ai/llms.txt")
        );
        assert_eq!(name.as_deref(), Some("claude-code"));
        // A version kelve doesn't speak gets one it does.
        let (said, name) = initialize(&json!({ "protocolVersion": "1999-01-01" }));
        assert!(VERSIONS.contains(&said["protocolVersion"].as_str().unwrap_or_default()));
        assert_eq!(name, None);
        assert!(matches!(plain("ping"), Some(Ok(_))));
        assert!(plain("tools/list").is_some());
        assert!(plain("resources/list").is_none());
    }

    #[test]
    fn the_update_tool_waits_for_the_persons_yes() {
        let tools = tools();
        let update = tools
            .as_array()
            .and_then(|t| t.iter().find(|t| t["name"] == "update"))
            .cloned()
            .unwrap_or_default();
        assert!(
            update["description"]
                .as_str()
                .unwrap_or_default()
                .contains("only after the person has said yes")
        );
    }

    #[test]
    fn what_was_opened_is_read_back_by_its_number_and_its_work_goes_on_while_its_prepared() {
        let opened = json!({ "id": 12, "title": "Retries", "shown": true });
        let text = opened_text(&opened);
        assert!(text.contains("giving for: 12"), "{text}");
        // kelve's hook knows what the agent opened from what it was told.
        assert_eq!(
            kelve_core::hook::opened_in(&json!([{ "type": "text", "text": text }])),
            Some(12)
        );
        // Still preparing isn't no more: the agent asks again, for the same.
        let again = preparing(Some(12));
        assert!(
            again.contains("still preparing")
                && again.contains("call next_job again, giving for: 12")
        );
        assert!(!again.contains("no more"));
    }

    #[test]
    fn open_kelve_opens_it_at_home_and_says_nothing_of_a_number() {
        let tools = tools();
        let open = tools
            .as_array()
            .and_then(|t| t.iter().find(|t| t["name"] == "open"))
            .cloned()
            .unwrap_or_default();
        assert!(
            open["inputSchema"]["properties"]["what"]["enum"]
                .as_array()
                .is_some_and(|e| e.contains(&json!("home")))
        );
        let text = home_text(&json!({ "opened": true }));
        assert!(text.contains("at home"), "{text}");
        // Nothing for kelve's hook to read back: no work was handed over.
        assert_eq!(
            kelve_core::hook::opened_in(&json!([{ "type": "text", "text": text }])),
            None
        );
        assert!(home_text(&json!({ "opened": false })).contains("`kelve open`"));
    }

    #[test]
    fn work_handed_over_with_a_debrief_due_is_debriefed_now_by_its_number() {
        let opened = json!({ "id": 7, "title": "Retries", "shown": true,
            "debrief": { "debrief": 2, "due": true, "reason": "It adds a dependency, rand.", "again": false } });
        let text = opened_text(&opened);
        assert!(
            text.contains("A debrief of this work is due: It adds a dependency, rand."),
            "{text}"
        );
        assert!(
            text.contains("Write it now") && text.contains("giving for: 7"),
            "{text}"
        );
        assert_eq!(
            kelve_core::hook::opened_in(&json!([{ "type": "text", "text": text }])),
            Some(7)
        );
        let again = json!({ "id": 7, "shown": true, "debrief": { "due": true, "again": true, "reason": "" } });
        assert!(opened_text(&again).contains("bring that debrief up to date"));
        // Not due: what's opened is as it was.
        let quiet = json!({ "id": 7, "title": "Typo", "shown": true,
            "debrief": { "due": false, "reason": "Nothing changed.", "again": false } });
        assert!(!opened_text(&quiet).contains("debrief"));
        let list = skills_text(
            &json!([{ "name": "rust", "kind": "like", "description": "Use when writing Rust." }]),
            false,
        );
        assert!(
            list.contains("- rust (how they like it): Use when writing Rust."),
            "{list}"
        );
    }

    #[test]
    fn a_job_is_given_with_its_instructions_material_and_shape() {
        let job = json!({
            "job": 7, "version": "read@5", "about": "Reading ledger.rs", "waiting": 2,
            "instructions": "Read the passages.",
            "material": {
                "file": "ledger.rs",
                "words": "",
                "alreadyMarked": [],
                "passages": [{ "id": "p1", "most": 3, "passage": "<passage>\n1  fn post() {}\n</passage>" }],
            },
            "answerShape": { "type": "object" },
        });
        let text = job_text(&job, &[]);
        assert!(text.starts_with("Job 7 (read@5): Reading ledger.rs. 2 more after this."));
        assert!(text.contains("Read the passages."));
        assert!(text.contains("list read@5 in have"));
        assert!(text.contains("as job 7"));
        // Fields a line each, passages as they are, nothing escaped, and
        // empty fields left out.
        assert!(text.contains("file: ledger.rs\n"));
        assert!(text.contains(
            "passages 1:\n  id: p1\n  most: 3\n  passage:\n<passage>\n1  fn post() {}\n</passage>\n"
        ));
        assert!(!text.contains("words"));
        assert!(!text.contains("alreadyMarked"));
        assert!(text.contains("{\"type\":\"object\"}"));
        // Instructions it has come by version alone.
        let again = job_text(&job, &["read@5".to_string()]);
        assert!(!again.contains("Read the passages."));
        assert!(again.contains("as the instructions for read@5 say"));
        assert!(
            again.contains("{\"type\":\"object\"}"),
            "the shape, until it's named"
        );
        // And the shape by its name alone, once the agent has it.
        let named = shape_name("{\"type\":\"object\"}");
        assert!(text.contains(&format!("list {named} in have")));
        let both = job_text(&job, &["read@5".to_string(), named.clone()]);
        assert!(!both.contains("{\"type\":\"object\"}"));
        assert!(both.contains(&format!("in the shape {named}, as you were given it.")));
        assert_ne!(named, shape_name("{\"type\":\"array\"}"));
    }
}
