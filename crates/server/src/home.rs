//! Home (docs/skills.md): the person's agent, what it asks of them in its
//! own words, what waits for it, and what happened lately, a day at a time.
//! Read from what kelve keeps: debriefs and handovers, research and
//! explanations asked for, and what happened in the person's skills.

use std::sync::Mutex;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use kelve_core::Timestamp;
use kelve_core::character::Move;
use kelve_core::skills::work::WorkKind;
use kelve_core::views::{
    ChecksWhen, HomeAsk, HomeAskKind, HomeDay, HomeState, HomeView, ServerEvent, TimelineAct,
    TimelineItem, TimelineKind,
};

use crate::error::ApiResult;
use crate::state::Shared;

/// How lately an agent must have called kelve to be working.
const WORKING_MS: i64 = 2 * 60 * 1000;
/// How long an agent holding a job counts as working on it.
const HOLDING_MS: i64 = 10 * 60 * 1000;
/// How long a working agent's clock runs from the person's last message.
const CLOCK_MS: i64 = 3 * 60 * 60 * 1000;
/// How far back the timeline goes.
const DAYS: i64 = 14;
/// The most lines on the timeline, and asks in the bubble.
const MOST_LINES: usize = 40;
const MOST_ASKS: usize = 3;

/// The day the person said "Not now" to putting kelve's hook back: it's
/// asked again the next day.
static HOOK_LATER: Mutex<Option<NaiveDate>> = Mutex::new(None);

/// The person said "Not now" to putting kelve's hook back.
pub fn hook_later(app: &Shared) {
    if let Ok(mut later) = HOOK_LATER.lock() {
        *later = Some(app.config.today());
    }
    app.publish(ServerEvent::Debriefs);
}

fn hook_put_off(app: &Shared) -> bool {
    HOOK_LATER
        .lock()
        .ok()
        .and_then(|l| *l)
        .is_some_and(|day| day >= app.config.today())
}

/// Whether kelve's hook is gone from the settings of the agent the person
/// uses, after it had worked.
fn hook_gone(app: &Shared, heard: bool) -> Option<&'static str> {
    use kelve_core::hook::Agent;
    use kelve_core::views::AgentKind;
    let agent = match app.settings().agent {
        AgentKind::ClaudeCode => Agent::Claude,
        AgentKind::Codex => Agent::Codex,
        AgentKind::Cursor => Agent::Cursor,
        AgentKind::Gemini => Agent::Gemini,
        AgentKind::Other => return None,
    };
    let there = crate::hooks::installed(agent, &app.config.agents_home, &app.config.claude_dir);
    (heard && !there).then(|| agent.shown())
}

/// A count as a word, for what the agent says: "Two things for you."
fn counted(n: usize) -> String {
    const WORDS: [&str; 10] = [
        "No", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine",
    ];
    WORDS
        .get(n)
        .map(|w| (*w).to_string())
        .unwrap_or_else(|| n.to_string())
}

fn at(ms: i64) -> Timestamp {
    Timestamp(kelve_store::at(ms))
}

/// A skill's name as people read it.
fn titled(name: &str) -> String {
    WorkKind::of(name)
        .map(|w| w.label().to_string())
        .unwrap_or_else(|| {
            let words = name.replace(['-', '_'], " ");
            let mut c = words.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
}

/// Home, as it is now.
pub async fn view(app: &Shared) -> ApiResult<HomeView> {
    let now = Utc::now().timestamp_millis();
    let since = now - DAYS * 24 * 60 * 60 * 1000;
    let mut conn = app.db.acquire().await?;
    kelve_store::asks::let_go_of_lapsed(&mut conn).await?;
    let seen = kelve_store::asks::latest_agent(&mut conn)
        .await?
        .map(|a| a.last_seen_at + WORKING_MS);
    let holding = kelve_store::asks::latest_taken(&mut conn)
        .await?
        .map(|t| t + HOLDING_MS);
    // Working: it holds a job, or it's called kelve lately and hasn't
    // handed over since the person last messaged it.
    let message = kelve_store::home::last_message_at(&mut conn).await?;
    let handover = kelve_store::home::last_handover_at(&mut conn).await?;
    let active = seen.is_some_and(|until| now < until);
    let handed_back = match (message, handover) {
        (_, None) => false,
        (Some(m), Some(h)) => h >= m,
        (None, Some(h)) => now - h < WORKING_MS,
    };
    let working = holding.is_some_and(|until| now < until) || (active && !handed_back);
    let working_since = match message {
        Some(t) if working && now - t < CLOCK_MS => Some(at(t)),
        _ => None,
    };

    let debriefs = kelve_store::home::debriefs_since(&mut conn, since, 60).await?;
    let small = kelve_store::home::small(&mut conn, since, 30).await?;
    let skills = kelve_store::home::skill_events(&mut conn, since, 30).await?;
    let scored = kelve_store::hosted::scored_since(&mut conn, since, 10).await?;
    let research = kelve_store::home::research_waiting(&mut conn).await?;
    let explaining = kelve_store::home::explanations_waiting(&mut conn).await?;
    // Checks are offered only when the person agrees to each, and while
    // this week's runs last.
    let unchecked = if app.settings().checks == ChecksWhen::Ask
        && crate::skill_checks::runs_left(app).await? >= 2
    {
        kelve_store::home::unchecked(&mut conn).await?
    } else {
        Vec::new()
    };
    let fits_ready = kelve_store::checking::pairs_ready(&mut conn).await?;
    let heard = kelve_store::home::hook_heard(&mut conn).await?;
    let ever = !debriefs.is_empty()
        || !small.is_empty()
        || !skills.is_empty()
        || !kelve_store::debriefs::latest(&mut conn, 1)
            .await?
            .is_empty()
        || !kelve_store::home::small(&mut conn, 0, 1).await?.is_empty();

    // What it asks of the person.
    let mut asks = Vec::new();
    let hook = hook_gone(app, heard).filter(|_| !hook_put_off(app));
    if let Some(agent) = hook {
        asks.push(HomeAsk {
            id: "hook".into(),
            kind: HomeAskKind::Hook,
            text: format!(
                "kelve’s hook is gone from {agent}’s settings, so I can’t send you debriefs."
            ),
            action: Some("Put it back".into()),
            to: None,
            main: true,
            alt: Some("Not now".into()),
        });
    }
    let failed: Vec<_> = debriefs.iter().filter(|d| d.state == "failed").collect();
    for d in &failed {
        asks.push(HomeAsk {
            id: format!("again:{}", d.id),
            kind: HomeAskKind::Stalled,
            text: format!(
                "I stopped before finishing the debrief for {}.",
                title_of(&d.title, &mut conn, d.handover).await?
            ),
            action: Some("Ask me again".into()),
            to: None,
            main: false,
            alt: None,
        });
    }
    for d in debriefs.iter().filter(|d| d.state == "ready") {
        let left = crate::debriefs::left(app, d.id).await?;
        let ready = kelve_store::home::research_ready(&mut conn, d.id).await?;
        let (text, action) = if ready {
            (
                format!("I looked into what you asked about {}.", d.title),
                "Read it",
            )
        } else if left == 0 {
            (format!("I finished {}.", d.title), "Read the debrief")
        } else {
            (
                format!(
                    "I finished {}. I made {} you might want a say in.",
                    d.title,
                    match left {
                        1 => "a decision".to_string(),
                        n => format!("{n} decisions"),
                    }
                ),
                "Read the debrief",
            )
        };
        asks.push(HomeAsk {
            id: format!("debrief:{}", d.id),
            kind: HomeAskKind::Debrief,
            text,
            action: Some(action.into()),
            to: Some(format!("debrief/{}", d.id)),
            main: false,
            alt: None,
        });
    }
    if let Some((skill, n)) = unchecked.first() {
        asks.push(HomeAsk {
            id: format!("check:{skill}"),
            kind: HomeAskKind::Check,
            text: format!(
                "I can check {} new {} {} on your past work.",
                n,
                titled(skill),
                if *n == 1 { "part" } else { "parts" }
            ),
            action: Some(if *n == 1 { "Check it" } else { "Check them" }.into()),
            to: Some(format!("skills/{skill}")),
            main: false,
            alt: None,
        });
    }
    if fits_ready > 0 {
        asks.push(HomeAsk {
            id: "fits".into(),
            kind: HomeAskKind::Fits,
            text: if fits_ready == 1 {
                "I redid a past task twice, once with your skills. Which would you rather keep?".into()
            } else {
                format!(
                    "I redid {} past tasks twice, once with your skills. Which would you rather keep?",
                    counted(fits_ready as usize).to_lowercase()
                )
            },
            action: Some("Pick one".into()),
            to: Some("agent/fits".into()),
            main: false,
            alt: None,
        });
    }
    asks.truncate(MOST_ASKS);
    if !asks.iter().any(|a| a.main)
        && let Some(first) = asks.first_mut()
    {
        first.main = true;
    }

    // What waits for it, in its words.
    let mut waiting: Vec<String> = Vec::new();
    match research.len() {
        0 => {}
        1 => waiting.push("look into your question".into()),
        n => waiting.push(format!("look into your {n} questions")),
    }
    for e in explaining.iter().take(2) {
        waiting.push(format!("explain {}", e.title));
    }
    for d in debriefs
        .iter()
        .filter(|d| d.state == "waiting" || d.state == "writing")
        .take(1)
    {
        waiting.push(format!(
            "write up {}",
            title_of(&d.title, &mut conn, d.handover).await?
        ));
    }
    let then = |when: &str| match waiting.as_slice() {
        [] => String::new(),
        [one] => format!(" {when}, I’ll {one}."),
        [first, rest @ ..] => format!(" {when}, I’ll {first}, and {}.", rest.join(", ")),
    };

    let name = kelve_core::character::default_name("");
    let new_here = !ever && !working && asks.is_empty();
    if new_here {
        asks.push(HomeAsk {
            id: "example".into(),
            kind: HomeAskKind::Debrief,
            text: "Want to see what that looks like first?".into(),
            action: Some("Try the example".into()),
            to: Some("debrief/example".into()),
            main: true,
            alt: None,
        });
    }
    let state = if new_here {
        HomeState::Empty
    } else if hook.is_some() || !failed.is_empty() {
        HomeState::Attention
    } else if working {
        HomeState::Working
    } else {
        HomeState::Normal
    };
    let (say, status, moving) = match state {
        HomeState::Empty => (
            format!(
                "Hello. I’m {}. Give me something to do, as you usually would, and I’ll show you what I did here.",
                if name == "Your agent" {
                    "your agent"
                } else {
                    name.as_str()
                }
            ),
            "New here".to_string(),
            Move::Wave,
        ),
        HomeState::Attention => (
            if hook.is_some() {
                "Something needs fixing.".to_string()
            } else if asks.len() == 1 {
                "One thing for you.".to_string()
            } else {
                "A few things for you.".to_string()
            },
            "Needs you".to_string(),
            Move::Idle,
        ),
        HomeState::Working => (
            format!(
                "I’m working on what you asked.{}",
                then("When I hand it over")
            ),
            "Working".to_string(),
            Move::Work,
        ),
        HomeState::Normal => (
            match asks.len() {
                0 => format!("Nothing needs you right now.{}", then("Next time I work")),
                n => format!(
                    "{} {} for you.{}",
                    counted(n),
                    if n == 1 { "thing" } else { "things" },
                    then("Next time I work")
                ),
            },
            if asks.is_empty() {
                "All set"
            } else {
                "Waiting for you"
            }
            .to_string(),
            Move::Idle,
        ),
    };

    // What happened, a day at a time.
    let mut items: Vec<TimelineItem> = Vec::new();
    for d in &debriefs {
        let title = title_of(&d.title, &mut conn, d.handover).await?;
        let (kind, title, when) = match d.state.as_str() {
            "sent" => (TimelineKind::Done, title, d.sent_at.unwrap_or(d.updated_at)),
            "failed" => (
                TimelineKind::Failed,
                format!("Couldn’t write up {title}"),
                d.updated_at,
            ),
            "ready" => (TimelineKind::Debrief, title, d.updated_at),
            _ => (
                TimelineKind::Debrief,
                format!("Writing up {title}"),
                d.updated_at,
            ),
        };
        items.push(TimelineItem {
            id: format!("debrief:{}", d.id),
            kind,
            title,
            at: at(when),
            act: Some(TimelineAct::Open),
            to: Some(format!("debrief/{}", d.id)),
            squares: Vec::new(),
        });
    }
    for h in &small {
        items.push(TimelineItem {
            id: format!("handover:{}", h.id),
            kind: TimelineKind::Small,
            title: if h.title.trim().is_empty() {
                "A small change".into()
            } else {
                h.title.clone()
            },
            at: at(h.at),
            act: Some(TimelineAct::DebriefIt),
            to: None,
            squares: Vec::new(),
        });
    }
    for e in &skills {
        let skill = titled(&e.skill);
        let (kind, title) = match e.kind.as_str() {
            "made" => (TimelineKind::Skill, format!("Started a new skill: {skill}")),
            "added" => (
                TimelineKind::Skill,
                format!("Added to {skill}: {}", e.heading),
            ),
            _ => (
                TimelineKind::Check,
                format!(
                    "Checked {}: {}",
                    e.heading,
                    match e.verdict.as_str() {
                        "works" => "it makes a difference",
                        "no_difference" => "your agent does it anyway",
                        _ => "your agent still gets it wrong",
                    }
                ),
            ),
        };
        items.push(TimelineItem {
            id: format!("{}:{}", e.kind, e.id),
            kind,
            title,
            at: at(e.at),
            act: Some(TimelineAct::SeeIt),
            to: Some(format!("skills/{}", e.skill)),
            squares: Vec::new(),
        });
    }
    for (key, name, whole, when) in crate::your_agent::drawn_since(app, since) {
        items.push(TimelineItem {
            id: format!("part:{key}"),
            kind: TimelineKind::Part,
            title: if whole {
                format!("Drew a new character: {name}")
            } else {
                format!("Drew a new part: {name}")
            },
            at: at(when),
            act: Some(TimelineAct::TryItOn),
            to: Some("agent/look".into()),
            squares: Vec::new(),
        });
    }
    for r in &scored {
        items.push(TimelineItem {
            id: format!("score:{}", r.id),
            kind: TimelineKind::Score,
            title: format!("A verified run on {} tasks, on {}", r.kind_name, r.model_name),
            at: at(r.at),
            act: Some(TimelineAct::SeeIt),
            to: Some(format!("agent/score/{}", r.id)),
            squares: Vec::new(),
        });
    }
    items.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    items.truncate(MOST_LINES);
    let days = by_day(app, items);

    Ok(HomeView {
        state,
        name,
        status,
        working_since,
        say,
        moving,
        asks,
        days,
    })
}

/// A debrief's title, or, before it's written, what was handed over.
async fn title_of(
    title: &str,
    conn: &mut kelve_store::Conn,
    handover: Option<i64>,
) -> ApiResult<String> {
    if !title.trim().is_empty() {
        return Ok(title.to_string());
    }
    let handed = match handover {
        Some(h) => kelve_store::handovers::get(conn, h).await?,
        None => None,
    };
    Ok(handed
        .map(|h| h.title)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "your work".into()))
}

/// Lines by the day they happened, in the person's time: "Today",
/// "Yesterday", then the date.
fn by_day(app: &Shared, items: Vec<TimelineItem>) -> Vec<HomeDay> {
    let today = app.config.today();
    let mut days: Vec<HomeDay> = Vec::new();
    let mut last: Option<NaiveDate> = None;
    for item in items {
        let day = app.config.day_of(DateTime::<Utc>::from(item.at));
        if last != Some(day) {
            last = Some(day);
            days.push(HomeDay {
                label: day_label(day, today),
                items: Vec::new(),
            });
        }
        if let Some(d) = days.last_mut() {
            d.items.push(item);
        }
    }
    days
}

fn day_label(day: NaiveDate, today: NaiveDate) -> String {
    if day == today {
        "Today".into()
    } else if day == today - Duration::days(1) {
        "Yesterday".into()
    } else {
        day.format("%A %-d %B").to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_read_as_people_say_them() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 5).unwrap_or_default();
        assert_eq!(day_label(today, today), "Today");
        assert_eq!(day_label(today - Duration::days(1), today), "Yesterday");
        assert_eq!(
            day_label(today - Duration::days(3), today),
            "Monday 2 November"
        );
        assert_eq!(counted(2), "Two");
        assert_eq!(counted(12), "12");
        assert_eq!(titled("rust"), "Rust");
        assert_eq!(titled("retrying-network-calls"), "Retrying network calls");
    }
}
