//! Learning from the person (docs/skills.md, "kelve catches what you know"):
//! a correction they made while their agent worked, their edits to work it
//! handed over, and the decisions they sent from a debrief to be done that
//! way every time, each become a `learn` job. The answer becomes candidates,
//! each placed in a skill by kelve's three questions (`core::learn::target`):
//! a project's skill, a task's, or the skill for the kind of work it was
//! caught in. What the person sent from a debrief is agreed, and is written
//! into the skill at once (`files::apply`); the rest waits in their next
//! debrief, under For your skills. A new skill is only ever suggested, and
//! never when one they have is close: then it adds to that one.

use chrono::Utc;
use kelve_core::fence::{fence, fence_with};
use kelve_core::learn::{self, Given, Target};
use kelve_core::skills::work::kind_of;
use kelve_core::skills::{self, SkillKind, WorkKind};
use kelve_store::candidates::{self, Evidence, NewCandidate, NewEvidence};
use kelve_store::{agent_loop as store, debriefs, handovers, jobs, skills as kept};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::files::{self, part_markdown, section_kept};
use crate::agent::{self, Ask, Priority, Resume};
use crate::error::ApiResult;
use crate::pipeline::{self, Then};
use crate::state::Shared;
use crate::v2jobs;

/// How far back a session's edits count as what the agent had just done.
const RECENT: i64 = 60 * 60 * 1000;
/// The most of a skill given whole, in characters.
const SKILL: usize = 10_000;
/// How close a "how you do something" skill's description must be to the
/// work for the learn job to be given it whole.
const NEAR: f64 = 0.25;

/// A skill the learn job is given, as kelve keeps it with the job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GivenSkill {
    name: String,
    /// The project it's in, or '' for the person's own.
    root: String,
    /// `like`, `do` or `project`.
    kind: String,
    /// For a `like` skill, its kind of work.
    work: String,
    text: String,
    /// Whether it's on disk, or a draft kelve made for it.
    exists: bool,
}

/// What the job learns from.
#[derive(Debug, Clone, PartialEq, Eq)]
enum From {
    /// A session's corrections.
    Messages(String),
    /// The person's edits to a handover.
    Edit(i64),
    /// A debrief sent.
    Debrief(i64),
}

impl From {
    fn of(job: &jobs::Job) -> Option<From> {
        // Keys may carry the kind first, as `v2jobs::queue` writes them.
        let key = job.key.as_deref().unwrap_or_default();
        let key = key
            .strip_prefix(v2jobs::LEARN)
            .and_then(|k| k.strip_prefix(':'))
            .unwrap_or(key);
        let p = &job.payload;
        if let Some(d) = p["debrief"]
            .as_i64()
            .or_else(|| key.strip_prefix("debrief:")?.parse().ok())
        {
            return Some(From::Debrief(d));
        }
        if let Some(h) = p["handover"]
            .as_i64()
            .or_else(|| key.strip_prefix("edit:")?.parse().ok())
        {
            return Some(From::Edit(h));
        }
        p["session"]
            .as_str()
            .map(str::to_string)
            .or_else(|| key.strip_prefix("messages:").map(str::to_string))
            .filter(|s| !s.is_empty())
            .map(From::Messages)
    }

    fn word(&self) -> &'static str {
        match self {
            From::Messages(_) => "message",
            From::Edit(_) => "edit",
            From::Debrief(_) => "decision",
        }
    }
}

/// Queues learning from a session's corrections.
pub async fn from_messages(app: &Shared, session: &str) -> pipeline::Result<()> {
    pipeline::add(
        app,
        v2jobs::LEARN,
        Some(&format!("{}:messages:{session}", v2jobs::LEARN)),
        json!({ "session": session }),
        Utc::now(),
    )
    .await
}

/// Queues learning from what the person changed after a handover.
pub async fn from_edits(app: &Shared, handover: i64) -> pipeline::Result<()> {
    pipeline::add(
        app,
        v2jobs::LEARN,
        Some(&format!("{}:edit:{handover}", v2jobs::LEARN)),
        json!({ "handover": handover }),
        Utc::now(),
    )
    .await
}

/// Queues learning from a debrief the person sent.
pub async fn from_debrief(app: &Shared, debrief: i64) -> pipeline::Result<()> {
    pipeline::add(
        app,
        v2jobs::LEARN,
        Some(&format!("{}:debrief:{debrief}", v2jobs::LEARN)),
        json!({ "debrief": debrief }),
        Utc::now(),
    )
    .await
}

/// The person sent their agent a message: when it corrects what the agent
/// had just done, it's kept as evidence, with what that was, and learned
/// from.
pub async fn on_message(app: &Shared, session: &str, root: &str, said: &str) -> ApiResult<()> {
    if !learn::corrects(said) {
        return Ok(());
    }
    let done = {
        let mut conn = app.db.acquire().await?;
        handovers::edited_since(&mut conn, session, kelve_store::now() - RECENT).await?
    };
    // A first message, before the agent did anything, asks; it can't correct.
    if done.is_empty() {
        return Ok(());
    }
    let mut files: Vec<String> = done.into_iter().map(|(_, p)| p).collect();
    files.sort();
    files.dedup();
    let text = format!(
        "They said: {}\nThe agent had just edited: {}",
        said.trim(),
        files.join(", ")
    );
    {
        let mut conn = app.db.acquire().await?;
        candidates::add_evidence(
            &mut conn,
            &NewEvidence {
                kind: "message",
                session,
                root,
                text: &text,
                ..Default::default()
            },
        )
        .await?;
    }
    from_messages(app, session).await?;
    Ok(())
}

/// The kinds of work some files are.
fn kinds_of<'a>(paths: impl IntoIterator<Item = &'a str>) -> Vec<WorkKind> {
    paths.into_iter().filter_map(kind_of).collect()
}

/// The skills the learn job is given whole: the one kelve's three questions
/// put it in, Everywhere, the project's, those the person chose, and
/// "how you do something" skills close to the work; and every other skill,
/// by name and description.
fn given_for(
    app: &Shared,
    root: &str,
    kinds: &[WorkKind],
    chosen: &[(String, String)],
    about: &str,
) -> (Vec<GivenSkill>, Vec<(String, String)>) {
    let on_disk = files::all(app, Some(root));
    let mut given: Vec<GivenSkill> = Vec::new();
    let add_like = |kind: WorkKind, given: &mut Vec<GivenSkill>| {
        if given
            .iter()
            .any(|g| g.root.is_empty() && g.name == kind.name())
        {
            return;
        }
        let had = on_disk
            .iter()
            .find(|s| s.root.is_empty() && s.name == kind.name());
        given.push(GivenSkill {
            name: kind.name().into(),
            root: String::new(),
            kind: "like".into(),
            work: kind.name().into(),
            text: had
                .map(|s| s.text.clone())
                .unwrap_or_else(|| learn::like_draft(kind)),
            exists: had.is_some(),
        });
    };
    if let Target::Like(kind) = learn::target(false, None, kinds) {
        add_like(kind, &mut given);
    }
    add_like(WorkKind::Everywhere, &mut given);
    for s in &on_disk {
        let close = s.kind() == SkillKind::Do
            && skills::closeness(&s.description(), about, &[]).score >= NEAR;
        let wanted =
            !s.root.is_empty() || chosen.iter().any(|(r, n)| *r == s.root && *n == s.name) || close;
        if wanted && !given.iter().any(|g| g.root == s.root && g.name == s.name) {
            given.push(GivenSkill {
                name: s.name.clone(),
                root: s.root.clone(),
                kind: match s.kind() {
                    SkillKind::Like => "like",
                    SkillKind::Do => "do",
                    SkillKind::Project => "project",
                }
                .into(),
                work: if s.kind() == SkillKind::Like {
                    s.name.clone()
                } else {
                    String::new()
                },
                text: s.text.clone(),
                exists: true,
            });
        }
    }
    let others = on_disk
        .iter()
        .filter(|s| !given.iter().any(|g| g.root == s.root && g.name == s.name))
        .map(|s| (s.name.clone(), s.description()))
        .collect();
    (given, others)
}

/// What a skill given is to the agent, in a few words.
fn about(g: &GivenSkill) -> String {
    match (g.kind.as_str(), g.exists) {
        ("like", true) => format!("The person's defaults for {}.", label(&g.work)),
        ("like", false) => format!(
            "The person's defaults for {}: they have none yet, so this is the skill kelve would make for it.",
            label(&g.work)
        ),
        ("project", _) => "What matters only in this project.".into(),
        _ => "How they do a task.".into(),
    }
}

fn label(work: &str) -> String {
    WorkKind::of(work)
        .map(|k| k.label().to_string())
        .unwrap_or_else(|| work.to_string())
}

fn schema() -> Value {
    learn::schema()
}

/// Whether the learn job's answer is sound, against the skills it was given.
pub fn shape(material: &Value, value: &Value) -> Result<(), String> {
    let given: Vec<GivenSkill> =
        serde_json::from_value(material["given"].clone()).unwrap_or_default();
    let given: Vec<Given<'_>> = given
        .iter()
        .map(|g| Given {
            name: &g.name,
            text: &g.text,
        })
        .collect();
    learn::read(value, &given)
        .map(|_| ())
        .map_err(|p| format!("Fix the answer, then hand it in again: {}", p.join(" ")))
}

/// What the job learns from, gathered: the evidence's ids, the material
/// that shows it, the repository, the session, the kinds of work, and the
/// skills the person chose for it.
struct Signs {
    evidence: Vec<i64>,
    /// When the latest of it was kept, or changed.
    latest: i64,
    shown: Value,
    root: String,
    session: String,
    kinds: Vec<WorkKind>,
    chosen: Vec<(String, String)>,
    /// What the work was, in words, to find skills close to it.
    about: String,
    /// The decisions sent, for a debrief.
    decisions: Vec<i64>,
}

async fn signs(app: &Shared, from: &From) -> ApiResult<Option<Signs>> {
    let mut conn = app.db.acquire().await?;
    Ok(match from {
        From::Messages(session) => {
            let found = store::unlearned(&mut conn, "message", Some(session), None).await?;
            let Some(first) = found.first() else {
                return Ok(None);
            };
            let done = handovers::edited_since(&mut conn, session, first.at - RECENT * 24).await?;
            let said: Vec<String> = found.iter().map(|e| e.text.clone()).collect();
            Some(Signs {
                evidence: found.iter().map(|e| e.id).collect(),
                latest: found.iter().map(|e| e.at).max().unwrap_or(0),
                shown: json!({
                    "from": "Corrections the person made while their agent worked, each with what the agent had just done.",
                    "said": fence("said", &said.join("\n\n")),
                }),
                root: first.root.clone(),
                session: session.clone(),
                kinds: kinds_of(done.iter().map(|(_, p)| p.as_str())),
                chosen: Vec::new(),
                about: said.join(" "),
                decisions: Vec::new(),
            })
        }
        From::Edit(handover) => {
            let found = store::unlearned(&mut conn, "edit", None, Some(*handover)).await?;
            let Some(h) = handovers::get(&mut conn, *handover).await? else {
                return Ok(None);
            };
            if found.is_empty() {
                return Ok(None);
            }
            let edits: Vec<String> = found
                .iter()
                .map(|e| fence_with("edit", &[("file", &e.path)], &e.text))
                .collect();
            Some(Signs {
                evidence: found.iter().map(|e| e.id).collect(),
                latest: found.iter().map(|e| e.at).max().unwrap_or(0),
                shown: json!({
                    "from": "The person's edits to work their agent handed over: lines starting - are the agent's, + the person's.",
                    "work": h.title,
                    "edit": edits.join("\n"),
                }),
                root: h.root.clone(),
                session: h.session.clone(),
                kinds: kinds_of(found.iter().map(|e| e.path.as_str())),
                chosen: Vec::new(),
                about: format!("{} {}", h.title, h.said),
                decisions: Vec::new(),
            })
        }
        From::Debrief(id) => {
            let Some(d) = debriefs::get(&mut conn, *id).await? else {
                return Ok(None);
            };
            let every: Vec<debriefs::Decision> = debriefs::decisions(&mut conn, *id)
                .await?
                .into_iter()
                .filter(|x| x.every_time)
                .collect();
            if every.is_empty() {
                return Ok(None);
            }
            let handed = match d.handover {
                Some(h) => handovers::files(&mut conn, h).await?,
                None => Vec::new(),
            };
            let read = d.markdown.as_deref().and_then(|m| {
                kelve_core::debrief::read(m, &super::debrief::changed_of(&handed)).ok()
            });
            let mut chosen = Vec::new();
            let mut kept_text = Vec::new();
            // The files the decisions sent are about, not the whole change:
            // a choice in Rust code is a Rust default, even in a change that
            // touched the docs too.
            let mut decided: Vec<String> = Vec::new();
            for x in &every {
                if let Some(s) = x.skill
                    && let Some(s) = kept::get(&mut conn, s).await?
                {
                    chosen.push((s.root.clone(), s.name.clone()));
                }
                let written = read.as_ref().and_then(|r| {
                    r.decisions
                        .get(usize::try_from(x.number - 1).unwrap_or(usize::MAX))
                });
                if let Some(w) = written {
                    decided.extend(w.lines.iter().map(|l| l.path.clone()));
                }
                let why = written.map(|w| w.why.clone()).unwrap_or_default();
                let call = match x.choice.as_deref() {
                    Some("change") => format!(
                        "They want it another way: {}",
                        x.note.as_deref().unwrap_or_default()
                    ),
                    _ => "They agreed, and want it this way every time.".into(),
                };
                kept_text.push(format!("### {}\n{why}\n\n{call}", x.title));
            }
            let paths: Vec<&str> = if decided.is_empty() {
                handed.iter().map(|f| f.path.as_str()).collect()
            } else {
                decided.iter().map(String::as_str).collect()
            };
            Some(Signs {
                evidence: Vec::new(),
                latest: 0,
                shown: json!({
                    "from": "Decisions in a debrief the person sent to their agent, wanting each done this way every time.",
                    "work": d.title,
                    "kept": fence("kept", &kept_text.join("\n\n")),
                }),
                root: d.root.clone(),
                session: d.session.clone(),
                kinds: kinds_of(paths),
                chosen,
                about: format!("{} {}", d.title, kept_text.join(" ")),
                decisions: every.iter().map(|x| x.id).collect(),
            })
        }
    })
}

/// The pipeline job: what's agreed from a debrief goes into the skills;
/// then the learn job is asked, or, once it's answered, its answer is kept.
pub async fn job(app: &Shared, job: &jobs::Job) -> pipeline::Result<Then> {
    let Some(from) = From::of(job) else {
        return Ok(Then::Done);
    };
    if let From::Debrief(id) = from {
        agreed_in(app, id).await;
    }
    let Some(signs) = signs(app, &from).await.map_err(super::fail)? else {
        return Ok(Then::Done);
    };
    let (given, others) = given_for(app, &signs.root, &signs.kinds, &signs.chosen, &signs.about);
    // The ask's key changes as the evidence does, so what's asked is never
    // an answer to less; an older ask for the same, not answered, goes.
    let (group, latest) = match &from {
        From::Messages(session) => (format!("learn:messages:{session}:"), signs.latest),
        From::Edit(h) => (format!("learn:edit:{h}:"), signs.latest),
        From::Debrief(d) => (format!("learn:debrief:{d}:"), 0),
    };
    let key = format!("{group}{latest}");
    {
        let mut conn = app.db.acquire().await?;
        let older: Vec<(i64,)> = sqlx::query_as(
            "select id from asks where substr(key, 1, length(?1)) = ?1 and key != ?2
               and state in ('waiting', 'taken')",
        )
        .bind(&group)
        .bind(&key)
        .fetch_all(&mut *conn)
        .await?;
        for (id,) in older {
            kelve_store::asks::drop(&mut conn, id).await?;
        }
    }
    let mut material = signs.shown.clone();
    if let Some(m) = material.as_object_mut() {
        m.insert(
            "skills".into(),
            Value::Array(
                given
                    .iter()
                    .map(|g| {
                        json!({
                            "name": g.name,
                            "what": about(g),
                            "markdown": fence_with(
                                "skill",
                                &[("name", &g.name)],
                                &kelve_core::text::clip(&kelve_core::redact::redact(&g.text), SKILL),
                            ),
                        })
                    })
                    .collect(),
            ),
        );
        m.insert(
            "otherSkills".into(),
            Value::Array(
                others
                    .iter()
                    .map(|(n, d)| json!({ "name": n, "description": d }))
                    .collect(),
            ),
        );
        // kelve's own, to read the answer: never given to the agent.
        m.insert("given".into(), json!(given));
        m.insert("evidence".into(), json!(signs.evidence));
    }
    let resume_key = job.key.clone().unwrap_or_else(|| key.clone());
    let answer = agent::ask(
        app,
        &key,
        Ask {
            kind: kelve_core::instructions::LEARN,
            about: match &from {
                From::Messages(_) => "Learning from what the person corrected".into(),
                From::Edit(_) => "Learning from what the person changed".into(),
                From::Debrief(_) => "Adding what the person sent to their skills".into(),
            },
            idea: None,
            opened: None,
            material,
            schema: schema(),
            priority: Priority::Rest,
            follows: None,
        },
        Resume::job(v2jobs::LEARN, resume_key, job.payload.clone()),
    )
    .await?;
    let Some(answer) = answer else {
        return Ok(Then::Done);
    };
    keep(app, &from, &signs, &answer.material, &answer.value)
        .await
        .map_err(super::fail)?;
    Ok(Then::Done)
}

/// Candidates proposed in a debrief the person sent, and not turned down:
/// agreed, so written into their skills.
async fn agreed_in(app: &Shared, debrief: i64) {
    let found = match app.db.acquire().await {
        Ok(mut conn) => candidates::of_debrief(&mut conn, debrief)
            .await
            .unwrap_or_default(),
        Err(_) => return,
    };
    for c in found
        .into_iter()
        .filter(|c| c.state == "proposed" && c.relation == "new")
    {
        if let Err(e) = files::apply(app, c.id).await {
            tracing::info!(error = %e.message, "a part agreed in a debrief wasn't added");
        }
    }
}

/// Keeps the learn job's answer: candidates placed in their skills,
/// evidence for parts that say it already, and a new skill suggested.
async fn keep(
    app: &Shared,
    from: &From,
    signs: &Signs,
    material: &Value,
    value: &Value,
) -> ApiResult<()> {
    let given: Vec<GivenSkill> =
        serde_json::from_value(material["given"].clone()).unwrap_or_default();
    let shown_evidence: Vec<i64> =
        serde_json::from_value(material["evidence"].clone()).unwrap_or_default();
    let read = {
        let g: Vec<Given<'_>> = given
            .iter()
            .map(|g| Given {
                name: &g.name,
                text: &g.text,
            })
            .collect();
        learn::read(value, &g)
    };
    let raw = {
        let mut conn = app.db.acquire().await?;
        store::evidence(&mut conn, &shown_evidence).await?
    };
    {
        let mut conn = app.db.acquire().await?;
        store::learned(&mut conn, &shown_evidence).await?;
    }
    let Ok(learned) = read else {
        return Ok(());
    };
    let agreed = matches!(from, From::Debrief(_));
    // Where something waiting for the person shows: the debrief not yet
    // sent in this repository, when there is one.
    let show_in = if agreed {
        None
    } else {
        let mut conn = app.db.acquire().await?;
        sqlx::query_scalar::<_, i64>(
            "select id from debriefs where root = ? and state != 'sent' order by updated_at desc limit 1",
        )
        .bind(&signs.root)
        .fetch_optional(&mut *conn)
        .await?
    };
    let skill_of = |name: &str| given.iter().find(|g| g.name == name);
    let ev = Ev {
        kind: from.word(),
        signs,
        raw: &raw,
    };
    for a in &learned.additions {
        let Some(g) = skill_of(&a.skill) else {
            continue;
        };
        let skill = if g.exists {
            files::sync(app, &g.root, &g.name).await?
        } else {
            None
        };
        let id = add_candidate(
            app,
            &NewCandidate {
                skill,
                work: &g.work,
                section: section_kept(a.section),
                heading: &a.heading,
                body: &part_markdown(&a.heading, &a.body),
                relation: "new",
                debrief: show_in,
                ..Default::default()
            },
            a.after.as_deref(),
            &ev,
            &a.evidence,
        )
        .await?;
        if agreed && let Err(e) = files::apply(app, id).await {
            tracing::info!(error = %e.message, "a part the person sent wasn't added");
        }
    }
    for s in &learned.same {
        let Some(g) = skill_of(&s.skill).filter(|g| g.exists) else {
            continue;
        };
        let Some(skill) = files::sync(app, &g.root, &g.name).await? else {
            continue;
        };
        if let Some(part) = files::part_id(app, skill, &s.part).await? {
            let mut conn = app.db.acquire().await?;
            candidates::add_evidence(&mut conn, &ev.piece(Some(part), None, &s.evidence)).await?;
        }
    }
    for d in &learned.disagrees {
        let Some(g) = skill_of(&d.skill).filter(|g| g.exists) else {
            continue;
        };
        let Some(skill) = files::sync(app, &g.root, &g.name).await? else {
            continue;
        };
        let part = files::part_id(app, skill, &d.part).await?;
        // Two parts that disagree are never both kept: the person chooses.
        add_candidate(
            app,
            &NewCandidate {
                skill: Some(skill),
                work: &g.work,
                section: "",
                heading: &d.heading,
                body: &part_markdown(&d.heading, &d.body),
                part,
                relation: "disagrees",
                debrief: show_in,
            },
            None,
            &ev,
            &d.evidence,
        )
        .await?;
    }
    if let Some(new) = &learned.new_skill {
        suggest(app, signs, new, agreed, show_in, &ev).await?;
    }
    app.publish(kelve_core::views::ServerEvent::Debriefs);
    app.publish(kelve_core::views::ServerEvent::Asks);
    Ok(())
}

/// What a candidate's evidence is made from.
struct Ev<'a> {
    kind: &'static str,
    signs: &'a Signs,
    raw: &'a [Evidence],
}

impl Ev<'_> {
    /// A piece of evidence for a part or a candidate: what the learn job
    /// said showed it, and what it came from.
    fn piece<'b>(
        &'b self,
        part: Option<i64>,
        candidate: Option<i64>,
        said: &'b str,
    ) -> NewEvidence<'b> {
        NewEvidence {
            kind: self.kind,
            part,
            candidate,
            session: &self.signs.session,
            root: &self.signs.root,
            // An edit's own evidence is one per file: this one is the
            // candidate's, so it isn't tied to the handover's file.
            handover: None,
            path: self.raw.first().map_or("", |e| e.path.as_str()),
            decision: self.signs.decisions.first().copied(),
            text: said,
        }
    }
}

/// Keeps a candidate, proposed, with what it goes after and its evidence:
/// its id.
async fn add_candidate(
    app: &Shared,
    c: &NewCandidate<'_>,
    after: Option<&str>,
    ev: &Ev<'_>,
    said: &str,
) -> ApiResult<i64> {
    let mut conn = app.db.acquire().await?;
    let id = candidates::add(&mut conn, c).await?;
    store::set_after(&mut conn, id, after).await?;
    sqlx::query("update candidates set state = 'proposed' where id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    let said = if said.trim().is_empty() {
        ev.raw.first().map_or("", |e| e.text.as_str())
    } else {
        said
    };
    candidates::add_evidence(&mut conn, &ev.piece(None, Some(id), said)).await?;
    Ok(id)
}

/// A new skill the learn job drafted: when one the person has is close,
/// its parts are added to that one instead; else it's suggested, for the
/// skills page and the debrief.
async fn suggest(
    app: &Shared,
    signs: &Signs,
    new: &learn::NewSkill,
    agreed: bool,
    show_in: Option<i64>,
    ev: &Ev<'_>,
) -> ApiResult<()> {
    let have = files::all(app, Some(&signs.root));
    let named: Vec<(String, String)> = have
        .iter()
        .map(|s| (s.name.clone(), s.description()))
        .collect();
    let pairs: Vec<(&str, &str)> = named
        .iter()
        .map(|(n, d)| (n.as_str(), d.as_str()))
        .collect();
    let draft = skills::Skill::parse(&new.markdown);
    if let Some((close, _)) = skills::closest(&new.description, &pairs, &[]) {
        let Some(s) = have.iter().find(|s| s.name == close) else {
            return Ok(());
        };
        let skill = files::sync(app, &s.root, &s.name).await?;
        for p in draft.parts() {
            let section = p
                .section
                .as_deref()
                .and_then(skills::SectionKind::of)
                .unwrap_or(skills::SectionKind::HowYouLikeIt);
            let id = add_candidate(
                app,
                &NewCandidate {
                    skill,
                    section: section_kept(section),
                    heading: &p.heading,
                    body: &p.markdown,
                    relation: "new",
                    debrief: show_in,
                    ..Default::default()
                },
                None,
                ev,
                &format!("From a skill suggested for {}, close to {close}.", new.name),
            )
            .await?;
            if agreed && let Err(e) = files::apply(app, id).await {
                tracing::info!(error = %e.message, "a part the person sent wasn't added");
            }
        }
        return Ok(());
    }
    let root = match new.kind {
        SkillKind::Project if !signs.root.is_empty() => signs.root.clone(),
        _ => String::new(),
    };
    let why = match ev.kind {
        "message" => "From what you corrected while your agent worked.",
        "edit" => "From what you changed after your agent handed over.",
        _ => "From decisions you wanted done the same way every time.",
    };
    let mut conn = app.db.acquire().await?;
    kept::add(
        &mut conn,
        &kept::NewSkill {
            name: &new.name,
            kind: match new.kind {
                SkillKind::Project => "project",
                _ => "do",
            },
            work: "",
            root: &root,
            suggested: Some((&new.markdown, why)),
        },
    )
    .await?;
    Ok(())
}
