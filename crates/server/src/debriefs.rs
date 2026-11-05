//! The debrief, as its page shows it (docs/skills.md): what the agent did,
//! how it works and what it decided, read from the markdown it wrote and
//! the change it handed over, with the person's call on each decision, the
//! research and explanations they asked for, and every file. And what they
//! do there: agree, change it (a note for their agent, left as they send
//! it), look into it (research), explain an idea, say they know it
//! (`marks.md`), agree with the rest, and send it, which leaves the notes
//! and asks their agent to learn from it. Nothing here leaves.

use std::collections::{BTreeMap, HashMap, HashSet};

use kelve_core::Timestamp;
use kelve_core::debrief::{self, Debrief, Decision, Origin};
use kelve_core::skills::work::{WorkKind, kind_of};
use kelve_core::views::{
    AnswerState, DebriefChangeView, DebriefDiffView, DebriefFile, DebriefFolder, DebriefLine,
    DebriefLineKind, DebriefState, DebriefView, DecisionChoice, DecisionStatus, DecisionView,
    DiffNote, ExplanationView, IdeaView, LinesIn, NoteIn, Place, ResearchView, SendKind, SendLine,
    ServerEvent, SkillAddition, SkillTarget, UsedPart, WritingStep,
};
use kelve_store::debriefs::{self as store, Decision as Row, Explanation, Research};
use kelve_store::handovers::{self, HandedFile, Handover};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::error::{ApiError, ApiResult};
use crate::state::Shared;
use crate::v2jobs;

/// The longest question the person can ask their agent to look into.
pub const MOST_QUESTION: usize = 1_000;
/// The longest note for the agent, asking for a decision another way.
pub const MOST_NOTE: usize = 4_000;
/// Past this many files, a debrief's folders start closed but the first.
const OPEN_FOLDERS: usize = 6;
/// The most files a folder lists before "N more".
const FOLDER_FILES: usize = 12;
/// The most lines of a decision's own code it shows.
const CODE_LINES: usize = 40;
/// What the person asks their agent first, when they look into a decision.
pub const QUESTION: &str = "Is this the best way here, or would another be better?";

/// A line of a hunk as kept with a handover.
#[derive(Debug, Clone, Deserialize)]
struct HunkLine {
    kind: String,
    #[allow(dead_code)]
    old: Option<u32>,
    new: Option<u32>,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Hunk {
    lines: Vec<HunkLine>,
}

fn hunks_of(file: &HandedFile) -> Vec<Hunk> {
    serde_json::from_str(&file.hunks).unwrap_or_default()
}

/// Everything a debrief's page is drawn from.
struct Loaded {
    row: store::Debrief,
    handover: Option<Handover>,
    files: Vec<HandedFile>,
    /// The files the person changed after it was handed over.
    yours: HashSet<String>,
    /// The debrief as the agent wrote it, once it's ready.
    read: Option<Result<Debrief, Vec<String>>>,
    rows: Vec<Row>,
}

pub fn not_found() -> ApiError {
    ApiError::not_found("There's no such debrief. Go back to home to see your agent's work.")
}

async fn load(app: &Shared, id: i64) -> ApiResult<Loaded> {
    let mut conn = app.db.acquire().await?;
    let row = store::get(&mut conn, id).await?.ok_or_else(not_found)?;
    let handover = match row.handover {
        Some(h) => handovers::get(&mut conn, h).await?,
        None => None,
    };
    let files = match &handover {
        Some(h) => handovers::files(&mut conn, h.id).await?,
        None => Vec::new(),
    };
    let yours = match &handover {
        Some(h) => kelve_store::candidates::edits_after(&mut conn, h.id)
            .await?
            .into_iter()
            .map(|e| e.path)
            .collect(),
        None => HashSet::new(),
    };
    let shown = matches!(row.state.as_str(), "ready" | "sent");
    let read = row
        .markdown
        .as_deref()
        .filter(|_| shown)
        .map(debrief::read_kept);
    // The decisions as it reads now are kept, with the person's calls on
    // those it had before.
    let mut rows = store::decisions(&mut conn, id).await?;
    if let Some(Ok(d)) = &read {
        let same = rows.len() == d.decisions.len()
            && rows
                .iter()
                .zip(&d.decisions)
                .all(|(r, d)| r.title == d.title);
        if !same {
            let kept: Vec<(String, String, String)> = d
                .decisions
                .iter()
                .map(|d| {
                    (
                        origin_word(d.from).to_string(),
                        lines_json(d),
                        d.part.clone().unwrap_or_default(),
                    )
                })
                .collect();
            let new: Vec<store::NewDecision<'_>> = d
                .decisions
                .iter()
                .zip(&kept)
                .enumerate()
                .map(|(i, (d, (origin, lines, part)))| store::NewDecision {
                    number: i64::try_from(i + 1).unwrap_or(i64::MAX),
                    title: &d.title,
                    origin,
                    lines,
                    part,
                })
                .collect();
            store::keep_decisions(&mut conn, id, &new).await?;
            rows = store::decisions(&mut conn, id).await?;
        }
    }
    Ok(Loaded {
        row,
        handover,
        files,
        yours,
        read,
        rows,
    })
}

/// How many of a debrief's decisions the person hasn't answered: its
/// decisions kept first, as it reads now.
pub async fn left(app: &Shared, id: i64) -> ApiResult<usize> {
    let l = load(app, id).await?;
    Ok(l.rows.iter().filter(|r| r.choice.is_none()).count())
}

fn origin_word(o: Origin) -> &'static str {
    match o {
        Origin::Own => "own",
        Origin::Said => "said",
        Origin::Edit => "edit",
    }
}

fn lines_json(d: &Decision) -> String {
    let lines: Vec<serde_json::Value> = d
        .lines
        .iter()
        .flat_map(|l| {
            if l.ranges.is_empty() {
                vec![json!({ "path": l.path, "from": 0, "to": 0 })]
            } else {
                l.ranges
                    .iter()
                    .map(|r| json!({ "path": l.path, "from": r.from, "to": r.to }))
                    .collect()
            }
        })
        .collect();
    serde_json::to_string(&lines).unwrap_or_else(|_| "[]".into())
}

fn choice_of(row: &Row) -> Option<DecisionChoice> {
    match row.choice.as_deref() {
        Some("agree") => Some(DecisionChoice::Agree),
        Some("change") => Some(DecisionChoice::Change),
        Some("look") => Some(DecisionChoice::LookInto),
        _ => None,
    }
}

fn answer_state(state: &str) -> AnswerState {
    match state {
        "ready" => AnswerState::Ready,
        "failed" => AnswerState::Failed,
        _ => AnswerState::Asked,
    }
}

fn status_of(choice: Option<DecisionChoice>, research: Option<&Research>) -> DecisionStatus {
    match choice {
        Some(DecisionChoice::Agree) => DecisionStatus::Agreed,
        Some(DecisionChoice::Change) => DecisionStatus::Redoing,
        Some(DecisionChoice::LookInto) => match research.map(|r| r.state.as_str()) {
            Some("ready") => DecisionStatus::ResearchReady,
            Some("failed") | None => DecisionStatus::YourCall,
            _ => DecisionStatus::Researching,
        },
        None => DecisionStatus::YourCall,
    }
}

/// Where a decision is, as people read it: "src/fetch.rs, lines 59 to 66".
pub fn place(lines: &[debrief::Lines]) -> String {
    let one = |l: &debrief::Lines| {
        let ranges: Vec<String> = l
            .ranges
            .iter()
            .map(|r| {
                if r.from == r.to {
                    r.from.to_string()
                } else {
                    format!("{} to {}", r.from, r.to)
                }
            })
            .collect();
        let single = l.ranges.len() == 1 && l.ranges[0].from == l.ranges[0].to;
        match ranges.len() {
            0 => l.path.clone(),
            _ => format!(
                "{}, {} {}",
                l.path,
                if single { "line" } else { "lines" },
                and(&ranges)
            ),
        }
    };
    lines.iter().map(one).collect::<Vec<_>>().join("; ")
}

/// Words joined as a sentence joins them: "1, 2 and 3".
pub fn and(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Decisions by number, as a sentence names them: "decisions 3 and 4".
fn numbered(ns: &[u32]) -> String {
    let words: Vec<String> = ns.iter().map(u32::to_string).collect();
    format!(
        "{} {}",
        if ns.len() == 1 {
            "decision"
        } else {
            "decisions"
        },
        and(&words)
    )
}

fn capital(text: &str) -> String {
    let mut c = text.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// A decision's own lines, from the hunks handed over: the lines in its
/// ranges, and those taken out among them, with folds for what's between.
/// What the person changed after handover is theirs.
fn code_of(lines: &debrief::Lines, file: Option<&HandedFile>, yours: bool) -> Vec<DebriefLine> {
    let Some(file) = file else {
        return Vec::new();
    };
    let wanted =
        |n: u32| lines.ranges.is_empty() || lines.ranges.iter().any(|r| r.from <= n && n <= r.to);
    let mut out: Vec<DebriefLine> = Vec::new();
    let mut last: Option<u32> = None;
    for hunk in hunks_of(file) {
        let ls = &hunk.lines;
        for (i, l) in ls.iter().enumerate() {
            // A line taken out sits where the next line still there is.
            let anchor = l.new.or_else(|| ls[i..].iter().find_map(|x| x.new));
            let Some(at) = anchor.filter(|n| wanted(*n)) else {
                continue;
            };
            if let Some(before) = last
                && at > before + 1
                && !out.is_empty()
            {
                out.push(DebriefLine {
                    kind: DebriefLineKind::Fold,
                    n: None,
                    text: String::new(),
                    folded: at - before - 1,
                });
            }
            let kind = match l.kind.as_str() {
                "added" if yours => DebriefLineKind::Yours,
                "added" => DebriefLineKind::Added,
                "removed" => DebriefLineKind::Removed,
                _ => DebriefLineKind::Context,
            };
            out.push(DebriefLine {
                kind,
                n: l.new.or(l.old),
                text: l.text.clone(),
                folded: 0,
            });
            if l.new.is_some() {
                last = l.new;
            }
            if out.len() >= CODE_LINES {
                return out;
            }
        }
    }
    out
}

/// The skill a decision goes into: the one the person chose, or their
/// "how you like it" skill for the kind of work its file is, or a new one.
async fn skill_for(
    conn: &mut kelve_store::Conn,
    row: &Row,
    path: Option<&str>,
) -> ApiResult<(SkillTarget, Option<i64>)> {
    if let Some(id) = row.skill
        && let Some(s) = kelve_store::skills::get(conn, id).await?
    {
        return Ok((target(&s.name, &s.work, false), Some(s.id)));
    }
    let work = path.and_then(kind_of).unwrap_or(WorkKind::Everywhere);
    match kelve_store::skills::like_for(conn, work.name()).await? {
        Some(s) => Ok((target(&s.name, &s.work, false), Some(s.id))),
        None => Ok((target(work.name(), work.name(), true), None)),
    }
}

/// A skill as the page names it.
fn target(name: &str, work: &str, new: bool) -> SkillTarget {
    let title = WorkKind::of(work)
        .map(|w| w.label().to_string())
        .unwrap_or_else(|| capital(&name.replace('-', " ")));
    SkillTarget {
        name: name.to_string(),
        title,
        new,
    }
}

/// The research's suggestion, in a sentence, from its answer.
fn suggests(answer: Option<&str>) -> Option<String> {
    let found = debrief::research(answer?).ok()?;
    Some(found.suggests).filter(|s| !s.trim().is_empty())
}

/// The lines an idea is used on in this change, as a print: the same idea
/// for the same code is explained once.
fn print_of(id: i64, idea: &str, decisions: &[DecisionView]) -> String {
    let mut h = Sha256::new();
    let uses: Vec<&DecisionView> = decisions
        .iter()
        .filter(|d| {
            d.rests_on
                .iter()
                .any(|r| store::idea_key(r) == store::idea_key(idea))
        })
        .collect();
    if uses.is_empty() {
        h.update(format!("debrief {id}"));
    }
    for d in uses {
        for l in &d.code {
            h.update(&l.text);
            h.update("\n");
        }
    }
    h.finalize()
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A debrief, as its page shows it.
pub async fn view(app: &Shared, id: i64) -> ApiResult<DebriefView> {
    let l = load(app, id).await?;
    let mut conn = app.db.acquire().await?;
    let state = match l.row.state.as_str() {
        "writing" => DebriefState::Writing,
        "ready" => DebriefState::Ready,
        "failed" => DebriefState::Failed,
        "sent" => DebriefState::Sent,
        _ => DebriefState::Waiting,
    };
    let (files, added, removed) = match &l.handover {
        Some(h) => (h.files, h.added, h.removed),
        None => (0, 0, 0),
    };
    let count = |n: i64| u32::try_from(n).unwrap_or(0);
    let title = [
        l.row.title.as_str(),
        l.handover
            .as_ref()
            .map(|h| h.title.as_str())
            .unwrap_or_default(),
    ]
    .into_iter()
    .find(|t| !t.trim().is_empty())
    .unwrap_or("Your agent's work")
    .to_string();
    let mut view = DebriefView {
        id: id.to_string(),
        title,
        state,
        example: false,
        steps: Vec::new(),
        files: count(files),
        added: count(added),
        removed: count(removed),
        did: String::new(),
        how: String::new(),
        decisions: Vec::new(),
        left: 0,
        for_skills: Vec::new(),
        suggested: None,
        used: Vec::new(),
        ideas: Vec::new(),
        folders: Vec::new(),
        on_send: Vec::new(),
        problems: Vec::new(),
        sent_at: l.row.sent_at.map(|at| Timestamp(kelve_store::at(at))),
    };
    let files_word = match files {
        1 => "the file".to_string(),
        n => format!("the {n} files"),
    };
    match state {
        DebriefState::Waiting => {
            view.steps = vec![
                step("Waiting for your agent", false, true),
                step(&format!("Reading {files_word}"), false, false),
                step("Writing it up", false, false),
            ];
        }
        DebriefState::Writing => {
            view.steps = vec![
                step(&format!("Read {files_word}"), true, false),
                step("Writing it up", false, true),
            ];
        }
        DebriefState::Failed => {
            view.problems = l
                .row
                .last_error
                .as_deref()
                .map(|e| {
                    e.lines()
                        .map(str::to_string)
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();
        }
        DebriefState::Ready | DebriefState::Sent => {}
    }
    let parsed = match &l.read {
        Some(Ok(d)) => d,
        Some(Err(problems)) => {
            tracing::warn!(debrief = id, "a debrief kept doesn't read");
            view.state = DebriefState::Failed;
            view.problems = problems.clone();
            return Ok(view);
        }
        None => {
            view.folders = folders(&l, &[]);
            return Ok(view);
        }
    };
    view.did = parsed.did.clone();
    view.how = parsed.how.clone();

    let research: HashMap<i64, Research> = latest_research(&mut conn, id).await?;
    let mut decisions = Vec::new();
    for (row, d) in l.rows.iter().zip(&parsed.decisions) {
        let first = d.lines.first();
        let path = first.map(|f| f.path.clone());
        let file = path
            .as_deref()
            .and_then(|p| l.files.iter().find(|f| f.path == p));
        let choice = choice_of(row);
        let r = research.get(&row.id);
        let (skill, _) = skill_for(&mut conn, row, path.as_deref()).await?;
        let ready = r.is_some_and(|r| r.state == "ready");
        let can_remember = match choice {
            Some(DecisionChoice::Change) => true,
            Some(DecisionChoice::LookInto) => ready,
            Some(DecisionChoice::Agree) => d.from != Origin::Own,
            None => false,
        };
        decisions.push(DecisionView {
            n: u32::try_from(row.number).unwrap_or(0),
            title: d.title.clone(),
            why: d.why.clone(),
            from: d.from,
            group: d.part.clone(),
            place: place(&d.lines),
            could_have: d.could_have.clone(),
            rests_on: d.rests_on.clone(),
            choice,
            status: status_of(choice, r),
            note: row.note.clone().unwrap_or_default(),
            question: r
                .map(|r| r.question.clone())
                .unwrap_or_else(|| QUESTION.into()),
            research: r.map(|r| ResearchView {
                state: answer_state(&r.state),
                markdown: r.answer.clone().filter(|_| r.state == "ready"),
                suggests: suggests(r.answer.as_deref()),
            }),
            remember: row.every_time,
            skill: Some(skill),
            can_remember,
            code: first
                .map(|f| code_of(f, file, d.from == Origin::Edit || l.yours.contains(&f.path)))
                .unwrap_or_default(),
            lang: path
                .as_deref()
                .and_then(kelve_core::code::language)
                .map(str::to_string),
            path,
        });
    }
    view.left = u32::try_from(decisions.iter().filter(|d| d.choice.is_none()).count()).unwrap_or(0);

    // What it rests on, with what the person knows already, and what was
    // explained.
    let spec = crate::spec::read(app);
    let known: HashSet<String> = spec.never.iter().map(|n| store::idea_key(n)).collect();
    // The same idea for the same code is explained once: whichever
    // debrief asked first.
    view.ideas = Vec::new();
    for i in &parsed.rests_on {
        let key = store::idea_key(&i.name);
        let print = print_of(id, &i.name, &decisions);
        let explained: Option<Explanation> = store::explained(&mut conn, &i.name, &print).await?;
        view.ideas.push(IdeaView {
            name: i.name.clone(),
            what: i.what.clone(),
            new_to_you: !known.contains(&key),
            explanation: explained.map(|e| ExplanationView {
                state: answer_state(&e.state),
                markdown: e.answer.clone().filter(|_| e.state == "ready"),
            }),
        });
    }

    // What it used from the person's skills, and what it adds to them.
    view.used = parsed
        .used
        .iter()
        .map(|u| UsedPart {
            skill: target(&u.skill, &u.skill, false),
            heading: u.part.clone(),
            followed: u.followed,
        })
        .collect();
    for c in kelve_store::candidates::of_debrief(&mut conn, id).await? {
        if c.state == "dismissed" {
            continue;
        }
        let skill = match c.skill {
            Some(s) => kelve_store::skills::get(&mut conn, s).await?,
            None => None,
        };
        view.for_skills.push(SkillAddition {
            skill: match skill {
                Some(s) => target(&s.name, &s.work, s.state != "made"),
                None => target(&c.work, &c.work, true),
            },
            heading: c.heading,
            markdown: c.body,
            decisions: Vec::new(),
            disagrees: None,
        });
    }

    view.folders = folders(&l, &decisions);
    view.decisions = decisions;
    view.on_send = on_send(&view, view.state == DebriefState::Sent);
    Ok(view)
}

fn step(text: &str, done: bool, now: bool) -> WritingStep {
    WritingStep {
        text: text.into(),
        done,
        now,
    }
}

/// The latest research on each decision of a debrief, by the decision's id.
async fn latest_research(
    conn: &mut kelve_store::Conn,
    id: i64,
) -> ApiResult<HashMap<i64, Research>> {
    Ok(kelve_store::home::research_of(conn, id)
        .await?
        .into_iter()
        .map(|r| (r.decision, r))
        .collect())
}

/// Every file of the change, by folder: the folders with decisions first,
/// the top folder last.
fn folders(l: &Loaded, decisions: &[DecisionView]) -> Vec<DebriefFolder> {
    let mut by: BTreeMap<String, Vec<DebriefFile>> = BTreeMap::new();
    for f in &l.files {
        let folder = match f.path.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/"),
            None => String::new(),
        };
        let mut ns: Vec<u32> = decisions
            .iter()
            .filter(|d| d.path.as_deref() == Some(f.path.as_str()))
            .map(|d| d.n)
            .collect();
        ns.sort_unstable();
        by.entry(folder).or_default().push(DebriefFile {
            path: f.path.clone(),
            added: u32::try_from(f.added).unwrap_or(0),
            removed: u32::try_from(f.removed).unwrap_or(0),
            yours: l.yours.contains(&f.path),
            decisions: ns,
            shown: f.skip.is_none(),
        });
    }
    let mut out: Vec<(bool, String, Vec<DebriefFile>)> = by
        .into_iter()
        .map(|(name, mut files)| {
            files.sort_by_key(|f| (f.decisions.is_empty() && !f.yours, f.path.clone()));
            let with = files.iter().any(|f| !f.decisions.is_empty() || f.yours);
            (with, name, files)
        })
        .collect();
    out.sort_by_key(|(with, name, _)| (!with, name.is_empty(), name.clone()));
    let all = l.files.len() <= OPEN_FOLDERS;
    out.into_iter()
        .enumerate()
        .map(|(i, (_, name, mut files))| {
            let count = u32::try_from(files.len()).unwrap_or(0);
            files.truncate(FOLDER_FILES);
            DebriefFolder {
                name: if name.is_empty() {
                    "the top folder".into()
                } else {
                    name
                },
                count,
                open: all || i == 0,
                files,
            }
        })
        .collect()
}

/// What happens when the debrief is sent, a line each; or, once it's sent,
/// what's happening.
fn on_send(view: &DebriefView, sent: bool) -> Vec<SendLine> {
    let line = |kind, text: String, action: Option<&str>, to: Option<String>| SendLine {
        kind,
        text,
        action: action.map(str::to_string),
        to,
    };
    let mut out = Vec::new();
    let nums = |f: &dyn Fn(&DecisionView) -> bool| -> Vec<u32> {
        view.decisions
            .iter()
            .filter(|d| f(d))
            .map(|d| d.n)
            .collect()
    };
    let redo = nums(&|d| d.choice == Some(DecisionChoice::Change));
    if !redo.is_empty() {
        out.push(line(
            SendKind::Redo,
            if sent {
                format!(
                    "Your agent redoes {} when you next message it.",
                    numbered(&redo)
                )
            } else {
                format!("Your agent redoes {}.", numbered(&redo))
            },
            None,
            None,
        ));
    }
    let looking = nums(&|d| d.status == DecisionStatus::Researching);
    if !looking.is_empty() {
        out.push(line(
            SendKind::Research,
            if sent {
                format!("Your agent is looking into {}.", numbered(&looking))
            } else {
                format!("Your agent keeps looking into {}.", numbered(&looking))
            },
            sent.then_some("Open the debrief"),
            sent.then(|| format!("debrief/{}", view.id)),
        ));
    }
    // Into the person's skills: by skill, those they have first.
    let mut skills: Vec<(SkillTarget, Vec<u32>)> = Vec::new();
    for d in view
        .decisions
        .iter()
        .filter(|d| d.can_remember && d.remember)
    {
        let Some(s) = &d.skill else { continue };
        match skills.iter_mut().find(|(t, _)| t.name == s.name) {
            Some((_, ns)) => ns.push(d.n),
            None => skills.push((s.clone(), vec![d.n])),
        }
    }
    skills.sort_by_key(|(t, _)| t.new);
    for (s, ns) in skills {
        let one = ns.len() == 1;
        let text = match (s.new, sent) {
            (false, false) => format!(
                "{} {} into your {} skill.",
                capital(&numbered(&ns)),
                if one { "goes" } else { "go" },
                s.title
            ),
            (true, false) => format!(
                "{} {} a new skill, {}.",
                capital(&numbered(&ns)),
                if one { "starts" } else { "start" },
                s.title
            ),
            (false, true) => format!(
                "Your agent adds {} to your {} skill.",
                numbered(&ns),
                s.title
            ),
            (true, true) => format!(
                "Your agent drafts a new skill, {}, from {}.",
                s.title,
                numbered(&ns)
            ),
        };
        out.push(line(
            if s.new {
                SendKind::NewSkill
            } else {
                SendKind::Skill
            },
            text,
            sent.then_some(if s.new {
                "See your skills"
            } else {
                "See the skill"
            }),
            sent.then(|| {
                if s.new {
                    "skills".to_string()
                } else {
                    format!("skills/{}", s.name)
                }
            }),
        ));
    }
    let open = nums(&|d| d.choice.is_none());
    if !open.is_empty() && !sent {
        let one = open.len() == 1;
        out.push(line(
            SendKind::Open,
            format!(
                "{} {} open.",
                capital(&numbered(&open)),
                if one { "stays" } else { "stay" }
            ),
            None,
            None,
        ));
    }
    if out.is_empty() {
        out.push(line(
            SendKind::Open,
            "Your agent leaves it as it is.".into(),
            None,
            None,
        ));
    }
    out
}

/// A debrief the person can still answer: written, and not sent.
async fn answerable(app: &Shared, id: i64) -> ApiResult<Loaded> {
    let l = load(app, id).await?;
    match l.row.state.as_str() {
        "ready" => Ok(l),
        "sent" => Err(ApiError::conflict(
            "This debrief was sent to your agent, so it can't change now.",
        )),
        _ => Err(ApiError::conflict(
            "Your agent hasn't finished this debrief yet. Wait for it to be on home.",
        )),
    }
}

fn decision_row(l: &Loaded, n: u32) -> ApiResult<&Row> {
    l.rows
        .iter()
        .find(|r| r.number == i64::from(n))
        .ok_or_else(|| {
            ApiError::not_found("That decision isn't in this debrief any more. Reload the page.")
        })
}

fn parsed_decision(l: &Loaded, n: u32) -> Option<&Decision> {
    match &l.read {
        Some(Ok(d)) => d.decisions.get(usize::try_from(n).ok()?.checked_sub(1)?),
        _ => None,
    }
}

/// The person's call on a decision, or none to take it back: with a note
/// for their agent when they want it changed. Agreeing with what was
/// theirs to begin with keeps it for every time.
pub async fn decide(
    app: &Shared,
    id: i64,
    n: u32,
    choice: Option<DecisionChoice>,
    note: Option<&str>,
) -> ApiResult<()> {
    let l = answerable(app, id).await?;
    let row = decision_row(&l, n)?;
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    if note.is_some_and(|n| n.chars().count() > MOST_NOTE) {
        return Err(ApiError::bad_request(format!(
            "That note is too long for your agent. Say it in at most {MOST_NOTE} characters."
        )));
    }
    let word = match choice {
        Some(DecisionChoice::Agree) => Some("agree"),
        Some(DecisionChoice::Change) => Some("change"),
        Some(DecisionChoice::LookInto) => {
            return Err(ApiError::bad_request(
                "To look into it, ask your agent a question.",
            ));
        }
        None => None,
    };
    let note = note
        .filter(|_| choice == Some(DecisionChoice::Change))
        .map(kelve_core::redact::redact);
    let mut conn = app.db.acquire().await?;
    kelve_store::home::decide(&mut conn, row.id, word, note.as_deref()).await?;
    let yours = parsed_decision(&l, n).is_some_and(|d| d.from != Origin::Own);
    if choice == Some(DecisionChoice::Agree) && yours && row.choice.is_none() {
        let path = parsed_decision(&l, n)
            .and_then(|d| d.lines.first())
            .map(|f| f.path.clone());
        let (_, skill) = skill_for(&mut conn, row, path.as_deref()).await?;
        kelve_store::home::every_time(&mut conn, row.id, true, skill).await?;
    }
    drop(conn);
    app.publish(ServerEvent::Debriefs);
    Ok(())
}

/// Whether the person wants a decision this way every time.
pub async fn remember(app: &Shared, id: i64, n: u32, on: bool) -> ApiResult<()> {
    let l = answerable(app, id).await?;
    let row = decision_row(&l, n)?;
    let path = parsed_decision(&l, n)
        .and_then(|d| d.lines.first())
        .map(|f| f.path.clone());
    let mut conn = app.db.acquire().await?;
    let (_, skill) = skill_for(&mut conn, row, path.as_deref()).await?;
    kelve_store::home::every_time(&mut conn, row.id, on, skill).await?;
    drop(conn);
    app.publish(ServerEvent::Debriefs);
    Ok(())
}

/// Agree with the rest: every decision not answered yet.
pub async fn agree_with_rest(app: &Shared, id: i64) -> ApiResult<()> {
    answerable(app, id).await?;
    let mut conn = app.db.acquire().await?;
    store::agree_with_rest(&mut conn, id).await?;
    drop(conn);
    app.publish(ServerEvent::Debriefs);
    Ok(())
}

/// The person asks their agent to look into a decision, with their
/// question: it waits for their agent, who answers under the decision.
pub async fn look_into(app: &Shared, id: i64, n: u32, question: &str) -> ApiResult<()> {
    let l = answerable(app, id).await?;
    let row = decision_row(&l, n)?;
    let question = question.trim();
    if question.is_empty() {
        return Err(ApiError::bad_request(
            "Ask your agent something: what you'd like it to find out.",
        ));
    }
    if question.chars().count() > MOST_QUESTION {
        return Err(ApiError::bad_request(format!(
            "Ask in fewer words: at most {MOST_QUESTION} characters."
        )));
    }
    let question = kelve_core::redact::redact(question);
    let mut conn = app.db.acquire().await?;
    let research = store::ask_research(&mut conn, row.id, &question).await?;
    kelve_store::home::decide(&mut conn, row.id, Some("look"), None).await?;
    drop(conn);
    v2jobs::queue(
        app,
        v2jobs::RESEARCH,
        &row.id.to_string(),
        json!({ "debrief": id, "decision": row.id, "number": n, "research": research }),
    )
    .await
    .map_err(|e| ApiError::internal(format!("couldn't queue research: {e:?}")))?;
    app.publish(ServerEvent::Debriefs);
    app.publish(ServerEvent::Asks);
    Ok(())
}

/// The person asks for an idea the work rests on to be explained: once for
/// the same code, kept.
pub async fn explain(app: &Shared, id: i64, idea: &str) -> ApiResult<()> {
    let v = view(app, id).await?;
    let Some(found) = v
        .ideas
        .iter()
        .find(|i| store::idea_key(&i.name) == store::idea_key(idea))
    else {
        return Err(ApiError::not_found(
            "That idea isn't in this debrief. Reload the page.",
        ));
    };
    if found
        .explanation
        .as_ref()
        .is_some_and(|e| e.state != AnswerState::Failed)
    {
        return Ok(());
    }
    let print = print_of(id, &found.name, &v.decisions);
    let mut conn = app.db.acquire().await?;
    let asked = store::ask_explain(&mut conn, Some(id), &found.name, &print).await?;
    drop(conn);
    v2jobs::queue(
        app,
        v2jobs::EXPLAIN,
        &asked.to_string(),
        json!({ "debrief": id, "explanation": asked, "idea": found.name }),
    )
    .await
    .map_err(|e| ApiError::internal(format!("couldn't queue an explanation: {e:?}")))?;
    app.publish(ServerEvent::Debriefs);
    app.publish(ServerEvent::Asks);
    Ok(())
}

/// The person knows an idea: kept in `marks.md`, so it's new to them
/// nowhere.
pub async fn know(app: &Shared, idea: &str) -> ApiResult<()> {
    let idea = idea.trim();
    if idea.is_empty() {
        return Err(ApiError::bad_request("Say which idea you know."));
    }
    let mut spec = crate::spec::read(app);
    if !spec
        .never
        .iter()
        .any(|n| store::idea_key(n) == store::idea_key(idea))
    {
        spec.never.push(idea.to_string());
        crate::spec::save(app, crate::spec::checked(spec)?, "you")?;
    }
    app.publish(ServerEvent::Spec);
    app.publish(ServerEvent::Debriefs);
    Ok(())
}

/// Sends the debrief to the person's agent: a note for each decision they
/// want another way, and a job to learn from what they kept for every
/// time. Research asked keeps running.
pub async fn send(app: &Shared, id: i64) -> ApiResult<DebriefView> {
    let l = answerable(app, id).await?;
    let v = view(app, id).await?;
    let redo: Vec<&DecisionView> = v
        .decisions
        .iter()
        .filter(|d| d.choice == Some(DecisionChoice::Change))
        .collect();
    if !redo.is_empty() {
        leave_notes(app, &l, &v.title, &redo).await?;
    }
    let mut conn = app.db.acquire().await?;
    if !store::sent(&mut conn, id).await? {
        return Err(ApiError::conflict(
            "This debrief was sent to your agent already.",
        ));
    }
    drop(conn);
    v2jobs::queue(
        app,
        v2jobs::LEARN,
        &format!("debrief:{id}"),
        json!({ "debrief": id }),
    )
    .await
    .map_err(|e| ApiError::internal(format!("couldn't ask the agent to learn: {e:?}")))?;
    app.publish(ServerEvent::Debriefs);
    app.publish(ServerEvent::Notes);
    app.publish(ServerEvent::Asks);
    view(app, id).await
}

/// A note for the person's agent for each decision they want another way,
/// on its lines, waiting for it as their notes do.
async fn leave_notes(
    app: &Shared,
    l: &Loaded,
    title: &str,
    redo: &[&DecisionView],
) -> ApiResult<()> {
    let open = match l.handover.as_ref().and_then(|h| h.opened) {
        Some(opened) => crate::reader::opened(app, opened).await.ok(),
        None => None,
    };
    let open = match open {
        Some(o) => o,
        None => {
            let Some(h) = &l.handover else {
                return Err(ApiError::conflict(
                    "kelve no longer has the work this debrief is about, so it can't leave your agent a note. Tell your agent what to change instead.",
                ));
            };
            let mut conn = app.db.acquire().await?;
            let opened =
                kelve_store::reader::open(&mut conn, "change", &h.root, "", &h.change, &h.title)
                    .await?;
            drop(conn);
            crate::reader::opened(app, opened).await?
        }
    };
    for d in redo {
        let lines = parsed_decision(l, d.n).and_then(|p| p.lines.first());
        let text = format!(
            "# Redo decision {} of “{title}”\n\nYou chose: {}\n\nDo it this way instead: {}\n",
            d.n,
            d.title,
            if d.note.trim().is_empty() {
                "the person wants it another way. Ask them how."
            } else {
                d.note.trim()
            }
        );
        let on = lines.and_then(|f| {
            let r = f.ranges.first()?;
            Some(LinesIn {
                path: f.path.clone(),
                start: r.from,
                end: r.to,
            })
        });
        let note = |on| NoteIn {
            id: None,
            place: Place::Agent,
            text: text.clone(),
            on,
            ideas: None,
        };
        // On its lines while they're there; else on the work as a whole.
        match on {
            Some(on) => crate::reader::keep_note(app, &open, &note(Some(on)))
                .or_else(|_| crate::reader::keep_note(app, &open, &note(None)))?,
            None => crate::reader::keep_note(app, &open, &note(None))?,
        };
    }
    Ok(())
}

/// Asks the agent for a debrief that failed, again.
pub async fn again(app: &Shared, id: i64) -> ApiResult<()> {
    let mut conn = app.db.acquire().await?;
    let row = store::get(&mut conn, id).await?.ok_or_else(not_found)?;
    if !kelve_store::home::ask_again(&mut conn, id).await? {
        return Ok(());
    }
    drop(conn);
    queue_debrief(app, id, row.handover).await
}

async fn queue_debrief(app: &Shared, id: i64, handover: Option<i64>) -> ApiResult<()> {
    let Some(h) = handover else {
        return Err(ApiError::conflict(
            "kelve no longer has the work this debrief is about. Ask your agent to show it in kelve again.",
        ));
    };
    v2jobs::queue(
        app,
        v2jobs::DEBRIEF,
        &h.to_string(),
        json!({ "handover": h, "debrief": id }),
    )
    .await
    .map_err(|e| ApiError::internal(format!("couldn't ask for a debrief: {e:?}")))?;
    app.publish(ServerEvent::Debriefs);
    app.publish(ServerEvent::Asks);
    Ok(())
}

/// Debrief it: the person wants a debrief of work too small for one. Its id.
pub async fn debrief_it(app: &Shared, handover: i64) -> ApiResult<i64> {
    let mut conn = app.db.acquire().await?;
    let h = handovers::get(&mut conn, handover).await?.ok_or_else(|| {
        ApiError::not_found(
            "kelve no longer has that work. Ask your agent to show it in kelve again.",
        )
    })?;
    if let Some(id) = kelve_store::home::debrief_of(&mut conn, handover).await? {
        return Ok(id);
    }
    drop(conn);
    // The agent loop's: asked for now, whatever the person's setting, and
    // kept as its piece of work's debrief.
    if h.opened.is_some() {
        return crate::agent_loop::debrief::anyway(app, h.id).await;
    }
    let mut conn = app.db.acquire().await?;
    let id = store::for_handover(&mut conn, h.id, &h.root, &h.session).await?;
    drop(conn);
    queue_debrief(app, id, Some(h.id)).await?;
    Ok(id)
}

/// A debrief's change, file by file: the change view's list.
pub async fn change(app: &Shared, id: i64) -> ApiResult<DebriefChangeView> {
    let v = view(app, id).await?;
    let l = load(app, id).await?;
    Ok(DebriefChangeView {
        id: v.id.clone(),
        title: v.title.clone(),
        files: v.files,
        added: v.added,
        removed: v.removed,
        folders: all_folders(&l, &v.decisions),
    })
}

/// Every file, by folder, none left out: the change view lists them all.
fn all_folders(l: &Loaded, decisions: &[DecisionView]) -> Vec<DebriefFolder> {
    let mut out = folders(l, decisions);
    // Listed in full, past what the debrief shows.
    for f in &mut out {
        if f.files.len() < f.count as usize {
            let name = if f.name == "the top folder" {
                String::new()
            } else {
                f.name.clone()
            };
            let mut all: Vec<DebriefFile> = l
                .files
                .iter()
                .filter(|h| match h.path.rsplit_once('/') {
                    Some((dir, _)) => format!("{dir}/") == name,
                    None => name.is_empty(),
                })
                .map(|h| DebriefFile {
                    path: h.path.clone(),
                    added: u32::try_from(h.added).unwrap_or(0),
                    removed: u32::try_from(h.removed).unwrap_or(0),
                    yours: l.yours.contains(&h.path),
                    decisions: decisions
                        .iter()
                        .filter(|d| d.path.as_deref() == Some(h.path.as_str()))
                        .map(|d| d.n)
                        .collect(),
                    shown: h.skip.is_none(),
                })
                .collect();
            all.sort_by_key(|f| (f.decisions.is_empty() && !f.yours, f.path.clone()));
            f.files = all;
        }
        f.open = true;
    }
    out
}

/// One file of a debrief's change, whole as it was handed over, the lines
/// it took out where they were, and each decision beside its first line.
pub async fn diff(app: &Shared, id: i64, path: &str) -> ApiResult<DebriefDiffView> {
    let v = view(app, id).await?;
    let l = load(app, id).await?;
    let file = l
        .files
        .iter()
        .find(|f| f.path == path)
        .ok_or_else(|| ApiError::not_found("That file isn't in this change."))?;
    let yours = l.yours.contains(path);
    let kind = if yours {
        "your edit"
    } else {
        match file.status.as_str() {
            "added" => "new",
            "deleted" => "deleted",
            _ => "changed",
        }
    };
    let shown = file.skip.is_none();
    let lines = if shown { whole(file) } else { Vec::new() };
    let mut notes = Vec::new();
    for d in v.decisions.iter() {
        let Some(p) = parsed_decision(&l, d.n) else {
            continue;
        };
        let Some(first) = p
            .lines
            .iter()
            .find(|x| x.path == path)
            .and_then(|x| x.ranges.first().map(|r| r.from).or(Some(1)))
        else {
            continue;
        };
        let at = lines
            .iter()
            .position(|x| x.kind != DebriefLineKind::Removed && x.n.is_some_and(|n| n >= first))
            .unwrap_or(0);
        notes.push(DiffNote {
            at: u32::try_from(at + 1).unwrap_or(1),
            n: d.n,
            title: d.title.clone(),
            status: d.status,
        });
    }
    Ok(DebriefDiffView {
        path: path.to_string(),
        lang: kelve_core::code::language(path).map(str::to_string),
        kind: kind.into(),
        shown,
        lines: lines
            .into_iter()
            .map(|mut x| {
                if yours && x.kind == DebriefLineKind::Added {
                    x.kind = DebriefLineKind::Yours;
                }
                x
            })
            .collect(),
        notes,
    })
}

/// A file as handed over, whole: its text with the lines its hunks took out
/// put back where they were. Without its text, its hunks alone, with a fold
/// for what's between.
fn whole(file: &HandedFile) -> Vec<DebriefLine> {
    let hunks = hunks_of(file);
    let kind = |k: &str| match k {
        "added" => DebriefLineKind::Added,
        "removed" => DebriefLineKind::Removed,
        _ => DebriefLineKind::Context,
    };
    let line = |kind, n, text: &str| DebriefLine {
        kind,
        n,
        text: text.to_string(),
        folded: 0,
    };
    let Some(text) = file.text.as_deref() else {
        let mut out = Vec::new();
        let mut last: Option<u32> = None;
        for h in &hunks {
            let first = h.lines.iter().find_map(|l| l.new);
            match (last, first) {
                (None, Some(f)) if f > 1 => out.push(DebriefLine {
                    kind: DebriefLineKind::Fold,
                    n: None,
                    text: String::new(),
                    folded: f - 1,
                }),
                (Some(a), Some(f)) if f > a + 1 => out.push(DebriefLine {
                    kind: DebriefLineKind::Fold,
                    n: None,
                    text: String::new(),
                    folded: f - a - 1,
                }),
                _ => {}
            }
            for l in &h.lines {
                out.push(line(kind(&l.kind), l.new.or(l.old), &l.text));
                if l.new.is_some() {
                    last = l.new;
                }
            }
        }
        return out;
    };
    // Lines taken out, by the line of the file they came before.
    let mut gone: BTreeMap<u32, Vec<&HunkLine>> = BTreeMap::new();
    let mut added: HashSet<u32> = HashSet::new();
    for h in &hunks {
        let mut next = h.lines.iter().find_map(|l| l.new).unwrap_or(1);
        for (i, l) in h.lines.iter().enumerate() {
            match l.kind.as_str() {
                "removed" => {
                    let before = h.lines[i..].iter().find_map(|x| x.new).unwrap_or(next);
                    gone.entry(before).or_default().push(l);
                }
                "added" => {
                    if let Some(n) = l.new {
                        added.insert(n);
                    }
                }
                _ => {}
            }
            if let Some(n) = l.new {
                next = n + 1;
            }
        }
    }
    let mut out = Vec::new();
    let count = u32::try_from(text.lines().count()).unwrap_or(u32::MAX);
    for (i, t) in text.lines().enumerate() {
        let n = u32::try_from(i + 1).unwrap_or(u32::MAX);
        for g in gone.remove(&n).unwrap_or_default() {
            out.push(line(DebriefLineKind::Removed, g.old, &g.text));
        }
        let k = if added.contains(&n) {
            DebriefLineKind::Added
        } else {
            DebriefLineKind::Context
        };
        out.push(line(k, Some(n), t));
    }
    // Taken out at the end.
    for (_, gs) in gone.range(count + 1..) {
        for g in gs {
            out.push(line(DebriefLineKind::Removed, g.old, &g.text));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kelve_core::debrief::Range;

    fn handed(text: Option<&str>, hunks: &str) -> HandedFile {
        HandedFile {
            path: "src/retry.rs".into(),
            status: "modified".into(),
            moved_from: None,
            skip: None,
            added: 1,
            removed: 1,
            hunks: hunks.into(),
            text: text.map(str::to_string),
        }
    }

    const HUNKS: &str = r#"[{"lines":[
        {"kind":"context","old":1,"new":1,"text":"a"},
        {"kind":"removed","old":2,"new":null,"text":"b"},
        {"kind":"added","old":null,"new":2,"text":"B"},
        {"kind":"context","old":3,"new":3,"text":"c"}]}]"#;

    #[test]
    fn a_file_reads_whole_with_what_it_took_out_where_it_was() {
        let lines = whole(&handed(Some("a\nB\nc\nd\n"), HUNKS));
        let got: Vec<(DebriefLineKind, Option<u32>, &str)> = lines
            .iter()
            .map(|l| (l.kind, l.n, l.text.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (DebriefLineKind::Context, Some(1), "a"),
                (DebriefLineKind::Removed, Some(2), "b"),
                (DebriefLineKind::Added, Some(2), "B"),
                (DebriefLineKind::Context, Some(3), "c"),
                (DebriefLineKind::Context, Some(4), "d"),
            ]
        );
        // Without its text, its hunks, after a fold for what's before.
        let far = HUNKS
            .replace("\"new\":1,", "\"new\":10,")
            .replace("\"new\":2,", "\"new\":11,")
            .replace("\"new\":3,", "\"new\":12,");
        let lines = whole(&handed(None, &far));
        assert_eq!((lines[0].kind, lines[0].folded), (DebriefLineKind::Fold, 9));
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn a_decisions_code_is_its_own_lines_and_whats_taken_out_among_them() {
        let lines = debrief::Lines {
            path: "src/retry.rs".into(),
            ranges: vec![Range::new(2, 2)],
        };
        let code = code_of(&lines, Some(&handed(None, HUNKS)), false);
        let kinds: Vec<DebriefLineKind> = code.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, [DebriefLineKind::Removed, DebriefLineKind::Added]);
        // What the person changed after handover is theirs.
        let code = code_of(&lines, Some(&handed(None, HUNKS)), true);
        assert_eq!(code[1].kind, DebriefLineKind::Yours);
    }

    #[test]
    fn places_read_as_people_say_them() {
        let l = |path: &str, ranges: &[(u32, u32)]| debrief::Lines {
            path: path.into(),
            ranges: ranges.iter().map(|(a, b)| Range::new(*a, *b)).collect(),
        };
        assert_eq!(
            place(&[l("src/fetch.rs", &[(59, 66)])]),
            "src/fetch.rs, lines 59 to 66"
        );
        assert_eq!(
            place(&[l("src/args.rs", &[(15, 15)])]),
            "src/args.rs, line 15"
        );
        assert_eq!(
            place(&[l("a.rs", &[(1, 2), (5, 5)]), l("b.rs", &[])]),
            "a.rs, lines 1 to 2 and 5; b.rs"
        );
        assert_eq!(numbered(&[3]), "decision 3");
        assert_eq!(numbered(&[1, 2, 4]), "decisions 1, 2 and 4");
    }
}
