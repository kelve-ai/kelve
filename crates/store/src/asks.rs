//! The work kelve asks the person's agent to do: added once by key, taken
//! by the agent one at a time, the most wanted first, and answered. A job
//! the agent took and never answered goes back after a while; one answered
//! wrongly twice is put aside. One the person takes off the list is put
//! aside too, and stays so until they put it back themselves.

use crate::{Conn, Result, now};

/// How long the agent may hold a job before it's offered again: as long
/// as a big file takes, and no longer, so a job a stopped agent held comes
/// back soon. An answer handed in after still counts.
const LEASE_MS: i64 = 10 * 60 * 1000;

/// Answers that weren't in the shape asked for, before a job is put aside.
pub const MOST_TRIES: i64 = 2;

/// How long an answered job is kept once it's taken up, in case the work
/// that asked for it runs again.
const KEEP_MS: i64 = 24 * 60 * 60 * 1000;

/// How long a job the person took off the list is kept, so asking for it
/// again doesn't put it back: a month.
const KEEP_OFF_MS: i64 = 30 * KEEP_MS;

/// A job for the agent, as it's added.
#[derive(Debug, Clone, PartialEq)]
pub struct New {
    pub key: String,
    pub kind: String,
    pub about: String,
    pub idea: Option<String>,
    pub opened: Option<i64>,
    pub material: String,
    pub schema: String,
    pub resume_kind: String,
    pub resume_key: Option<String>,
    pub resume_payload: String,
    pub priority: i64,
    /// The key of the job this one finishes: it goes where that one was.
    pub follows: Option<String>,
}

/// A job for the agent, as it's kept.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Ask {
    pub id: i64,
    pub key: String,
    pub kind: String,
    pub about: String,
    pub idea: Option<String>,
    pub opened: Option<i64>,
    pub material: String,
    pub schema: String,
    pub resume_kind: String,
    pub resume_key: Option<String>,
    pub resume_payload: String,
    pub priority: i64,
    pub state: String,
    pub answer: Option<String>,
    pub model: Option<String>,
    pub agent: Option<String>,
    pub tries: i64,
    pub last_error: Option<String>,
    pub created_at: i64,
    /// When the person took it off the list: put aside until they put it back.
    pub taken_off_at: Option<i64>,
    /// Where it goes on the list: the id of the job it finishes.
    pub place: Option<i64>,
}

const COLUMNS: &str = "id, key, kind, about, idea, opened, material, schema, resume_kind, \
    resume_key, resume_payload, priority, state, answer, model, agent, tries, last_error, created_at, \
    taken_off_at, place";

/// The order jobs are taken in: what the person said to do next, since
/// they last opened something, first; then everything for what was opened
/// last; then the most wanted, by where each goes.
const ORDER: &str = "priority >= 0,
    coalesce(opened = (select id from opened order by opened_at desc, id desc limit 1), 0) desc,
    priority, coalesce(place, id), id";

/// Adds a job, unless one is kept under its key: a job put aside is asked
/// for afresh, but for one the person took off the list, which stays put
/// aside; and one waiting moves up when it's now wanted sooner. A job
/// two things need, as a file both read, is for the one that asked last,
/// so it goes with what was opened last, and its answer carries on for it:
/// checking the file's ideas next is for it too. A job that finishes
/// another goes where that one was, as soon as it was wanted.
/// Whether it's new.
pub async fn add(conn: &mut Conn, new: &New) -> Result<bool> {
    let done = sqlx::query(
        "insert into asks (key, kind, about, idea, opened, material, schema, resume_kind,
                           resume_key, resume_payload, priority, place)
         values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                 min(?11, coalesce((select priority from asks where key = ?12), ?11)),
                 (select coalesce(place, id) from asks where key = ?12))
         on conflict (key) do update set
           state = case when asks.state = 'dropped' and asks.taken_off_at is null then 'waiting' else asks.state end,
           tries = case when asks.state = 'dropped' and asks.taken_off_at is null then 0 else asks.tries end,
           material = case when asks.state in ('waiting', 'dropped') then excluded.material else asks.material end,
           priority = min(asks.priority, excluded.priority),
           opened = coalesce(excluded.opened, asks.opened),
           resume_kind = case when excluded.opened is not null then excluded.resume_kind else asks.resume_kind end,
           resume_key = case when excluded.opened is not null then excluded.resume_key else asks.resume_key end,
           resume_payload = case when excluded.opened is not null then excluded.resume_payload else asks.resume_payload end",
    )
    .bind(&new.key)
    .bind(&new.kind)
    .bind(&new.about)
    .bind(&new.idea)
    .bind(new.opened)
    .bind(&new.material)
    .bind(&new.schema)
    .bind(&new.resume_kind)
    .bind(&new.resume_key)
    .bind(&new.resume_payload)
    .bind(new.priority)
    .bind(&new.follows)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0 && done.last_insert_rowid() > 0)
}

/// A job by its key.
pub async fn by_key(conn: &mut Conn, key: &str) -> Result<Option<Ask>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select {COLUMNS} from asks where key = ?1"
    )))
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?)
}

/// A job by its id.
pub async fn get(conn: &mut Conn, id: i64) -> Result<Option<Ask>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select {COLUMNS} from asks where id = ?1"
    )))
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Takes the next job for the agent: the most wanted, oldest first, a job
/// that finishes another where that one was. A job held too long without an
/// answer is offered again.
pub async fn take(conn: &mut Conn, agent: &str) -> Result<Option<Ask>> {
    take_for(conn, agent, None, None, After::default()).await
}

/// The job an agent just answered, by the key its answer resumes and the
/// idea it's about: its next job is the same file's, or the same idea's.
#[derive(Clone, Copy, Debug, Default)]
pub struct After<'a> {
    pub resume_key: Option<&'a str>,
    pub idea: Option<&'a str>,
}

/// Takes the next job, as `take` does, but a note's only when its key
/// starts with `notes`: an agent is given the notes of the project it
/// works in, never another's. After a job it answered, the next job for
/// the same file, or the same idea, comes first, so one agent takes a piece
/// of work straight on (Nathan, 29 October). An agent asked to do what's
/// waiting for one thing opened, by its number, takes only its jobs, and
/// the notes of its project.
pub async fn take_for(
    conn: &mut Conn,
    agent: &str,
    notes: Option<&str>,
    only: Option<i64>,
    after: After<'_>,
) -> Result<Option<Ask>> {
    let now = now();
    let_go_of_lapsed(conn).await?;
    // A job's file, as its resume key ends: `:12:src/retry.rs`; or its idea.
    let same = "coalesce(coalesce(substr(resume_key, instr(resume_key, ':')), '') = ?4, 0) or coalesce(idea = ?5, 0)";
    let order = ORDER.replacen(
        "priority >= 0,",
        &format!("priority >= 0, not ({same}),"),
        1,
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "update asks set state = 'taken', taken_at = ?1, agent = ?2
          where id = (select id from asks where state = 'waiting'
                         and (?3 is null or kind != 'note' or substr(key, 1, length(?3)) = ?3)
                         and (?6 is null or opened = ?6 or kind = 'note')
                       order by {order} limit 1)
          returning {COLUMNS}"
    )))
    .bind(now)
    .bind(agent)
    .bind(notes)
    .bind(after.resume_key.and_then(file_of))
    .bind(after.idea)
    .bind(only)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The file a job is for, as its resume key ends: `:12:src/retry.rs`. None
/// for a job about no file.
pub fn file_of(resume_key: &str) -> Option<&str> {
    let at = resume_key.find(':')?;
    let rest = &resume_key[at..];
    (rest.matches(':').count() >= 2).then_some(rest)
}

/// Keeps the agent's answer to a job.
pub async fn answer(
    conn: &mut Conn,
    id: i64,
    answer: &str,
    model: Option<&str>,
    agent: &str,
) -> Result<()> {
    sqlx::query(
        "update asks set state = 'answered', answer = ?2, model = ?3, agent = ?4,
                answered_at = ?5, last_error = null
          where id = ?1",
    )
    .bind(id)
    .bind(answer)
    .bind(model)
    .bind(agent)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Says an answer wasn't in the shape asked for. The job stays with the
/// agent that handed it in, to fix, so no other takes it meanwhile and
/// its fixed answer still finds it open; after `MOST_TRIES` it's put
/// aside. Whether it was put aside.
pub async fn wrong(conn: &mut Conn, id: i64, why: &str) -> Result<bool> {
    let tries: i64 = sqlx::query_scalar(
        "update asks set tries = tries + 1, last_error = ?2,
                state = case when tries + 1 >= ?3 then 'dropped' else 'taken' end,
                taken_at = case when tries + 1 >= ?3 then null else ?4 end
          where id = ?1
          returning tries",
    )
    .bind(id)
    .bind(why)
    .bind(MOST_TRIES)
    .bind(now())
    .fetch_one(&mut *conn)
    .await?;
    Ok(tries >= MOST_TRIES)
}

/// Puts a job aside: the person took it off the list. Asked for again, it
/// stays aside, until they put it back. Whether it was on the list.
pub async fn drop(conn: &mut Conn, id: i64) -> Result<bool> {
    let done = sqlx::query(
        "update asks set state = 'dropped', taken_off_at = ?2, taken_at = null
          where id = ?1 and state in ('waiting', 'taken')",
    )
    .bind(id)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Moves the jobs for an idea, or for what's open, up the list: the person
/// is looking at it now. Only ever up: a Do next said for one of them
/// stays ahead of the rest.
pub async fn hurry(conn: &mut Conn, idea: Option<&str>, opened: Option<i64>) -> Result<()> {
    sqlx::query(
        "update asks set priority = 0
          where priority > 0 and state in ('waiting', 'taken')
            and ((?1 is not null and idea = ?1) or (?2 is not null and opened = ?2))",
    )
    .bind(idea)
    .bind(opened)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Moves some jobs to the front of the list: the person said do these next.
pub async fn first(conn: &mut Conn, ids: &[i64]) -> Result<()> {
    let least: i64 = sqlx::query_scalar(
        "select coalesce(min(priority), 0) from asks where state in ('waiting', 'taken')",
    )
    .fetch_one(&mut *conn)
    .await?;
    for id in ids {
        sqlx::query(
            "update asks set priority = ?2 where id = ?1 and state in ('waiting', 'taken')",
        )
        .bind(id)
        .bind(least - 1)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Lets go of the jobs an agent took and hasn't answered in time: it
/// stopped, so they wait for an agent again, and aren't being done.
pub async fn let_go_of_lapsed(conn: &mut Conn) -> Result<()> {
    sqlx::query("update asks set state = 'waiting', taken_at = null where state = 'taken' and taken_at < ?1")
        .bind(now() - LEASE_MS)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Lets go of every Do next: something new was opened, and comes first.
pub async fn let_go_of_first(conn: &mut Conn) -> Result<()> {
    sqlx::query(
        "update asks set priority = 0 where priority < 0 and state in ('waiting', 'taken')",
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Puts aside the jobs whose keys start with `prefix`, but `keep`: a note
/// done another way, or changed since. Those the agent has taken only with
/// `taken`: else the job stays with it, and its answer still finds it open.
pub async fn drop_like(
    conn: &mut Conn,
    prefix: &str,
    keep: Option<&str>,
    taken: bool,
) -> Result<bool> {
    let done = sqlx::query(
        "update asks set state = 'dropped', taken_at = null
          where (state = 'waiting' or (?3 and state = 'taken'))
            and substr(key, 1, length(?1)) = ?1
            and (?2 is null or key != ?2)",
    )
    .bind(prefix)
    .bind(keep)
    .bind(taken)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Asks again for a job answered before `before`, in milliseconds: a note
/// done for these words, then for others, and now back to these. Whether
/// it was.
pub async fn reopen_answered_before(conn: &mut Conn, key: &str, before: i64) -> Result<bool> {
    let done = sqlx::query(
        "update asks set state = 'waiting', answer = null, tries = 0, last_error = null,
                taken_at = null, answered_at = null
          where key = ?1 and state = 'answered' and answered_at < ?2",
    )
    .bind(key)
    .bind(before)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Keeps that an agent's hook told it of a note, at a version: whether it
/// hadn't been told of it as it is.
pub async fn tell(
    conn: &mut Conn,
    agent: &str,
    root: &str,
    note: &str,
    version: &str,
) -> Result<bool> {
    let done = sqlx::query(
        "insert into notes_told (agent, root, note, version, at) values (?1, ?2, ?3, ?4, ?5)
         on conflict (agent, root, note) do update set version = excluded.version, at = excluded.at
           where notes_told.version != excluded.version",
    )
    .bind(agent)
    .bind(root)
    .bind(note)
    .bind(version)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Every job not yet answered, the most wanted first.
pub async fn open(conn: &mut Conn) -> Result<Vec<Ask>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select {COLUMNS} from asks where state in ('waiting', 'taken') order by {ORDER}"
    )))
    .fetch_all(&mut *conn)
    .await?)
}

/// How many jobs wait for the agent to take them.
pub async fn waiting(conn: &mut Conn) -> Result<i64> {
    Ok(
        sqlx::query_scalar("select count(*) from asks where state = 'waiting'")
            .fetch_one(&mut *conn)
            .await?,
    )
}

/// Forgets answers taken up long enough ago, and jobs put aside: those
/// the person took off the list after a month.
pub async fn sweep(conn: &mut Conn) -> Result<()> {
    sqlx::query(
        "delete from asks where state in ('answered', 'dropped')
            and (case when taken_off_at is null then coalesce(answered_at, created_at) < ?1
                      else taken_off_at < ?2 end)",
    )
    .bind(now() - KEEP_MS)
    .bind(now() - KEEP_OFF_MS)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Says an agent was here, and the model and plan it said it runs on:
/// whether it's the first time it was.
pub async fn seen(
    conn: &mut Conn,
    name: &str,
    model: Option<&str>,
    plan: Option<&str>,
) -> Result<bool> {
    let now = now();
    let before: Option<i64> = sqlx::query_scalar("select 1 from agents where name = ?")
        .bind(name)
        .fetch_optional(&mut *conn)
        .await?;
    sqlx::query(
        "insert into agents (name, model, plan, first_seen_at, last_seen_at) values (?1, ?2, ?3, ?4, ?4)
         on conflict (name) do update set
           model = coalesce(excluded.model, agents.model),
           plan = coalesce(excluded.plan, agents.plan),
           last_seen_at = excluded.last_seen_at",
    )
    .bind(name)
    .bind(model)
    .bind(plan)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(before.is_none())
}

/// An agent that has done kelve's work.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Agent {
    pub name: String,
    pub model: Option<String>,
    pub plan: Option<String>,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
}

/// The agent seen most lately, if any.
pub async fn latest_agent(conn: &mut Conn) -> Result<Option<Agent>> {
    Ok(sqlx::query_as(
        "select name, model, plan, first_seen_at, last_seen_at from agents order by last_seen_at desc limit 1",
    )
    .fetch_optional(&mut *conn)
    .await?)
}

/// When the job taken last, and not yet answered, was taken: an agent
/// working on it calls kelve again only when it hands it in.
pub async fn latest_taken(conn: &mut Conn) -> Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("select max(taken_at) from asks where state = 'taken'")
            .fetch_one(&mut *conn)
            .await?,
    )
}

/// The agents seen since a time, in milliseconds, the latest first.
pub async fn agents_since(conn: &mut Conn, since: i64) -> Result<Vec<Agent>> {
    Ok(sqlx::query_as(
        "select name, model, plan, first_seen_at, last_seen_at from agents
          where last_seen_at >= ?1 order by last_seen_at desc",
    )
    .bind(since)
    .fetch_all(&mut *conn)
    .await?)
}

/// An agent by name.
pub async fn agent(conn: &mut Conn, name: &str) -> Result<Option<Agent>> {
    Ok(sqlx::query_as(
        "select name, model, plan, first_seen_at, last_seen_at from agents where name = ?1",
    )
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(key: &str, priority: i64) -> New {
        New {
            key: key.into(),
            kind: "read".into(),
            about: format!("Reading {key}"),
            idea: None,
            opened: Some(1),
            material: "{}".into(),
            schema: "{}".into(),
            resume_kind: "read.file".into(),
            resume_key: Some(format!("read.file:{key}")),
            resume_payload: "{}".into(),
            priority,
            follows: None,
        }
    }

    #[tokio::test]
    async fn the_agent_that_answered_a_file_takes_its_next_job_first() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        // Two files waiting, a.rs first; then b.rs's check, behind them.
        add(&mut conn, &new("12:a.rs", 2)).await.unwrap();
        let mut check = new("12:b.rs:check", 2);
        check.kind = "choose".into();
        check.resume_key = Some("read.file:12:b.rs".into());
        add(&mut conn, &check).await.unwrap();
        // Any agent takes a.rs first; the one that just answered b.rs is
        // given b.rs's check, straight on.
        let after = After {
            resume_key: Some("read.file:12:b.rs"),
            ..After::default()
        };
        let next = take_for(&mut conn, "claude-code", None, None, after)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next.key, "12:b.rs:check");
        assert_eq!(file_of("job.kind:12"), None);
        let next = take_for(&mut conn, "claude-code", None, None, After::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next.key, "12:a.rs");
    }

    #[tokio::test]
    async fn the_agent_that_answered_for_an_idea_takes_its_next_job_first() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let about = |key: &str, idea: &str| New {
            kind: "write".into(),
            idea: Some(idea.into()),
            resume_key: Some(format!("entry.write:{idea}")),
            ..new(key, 2)
        };
        add(&mut conn, &about("write backoff", "kelve:backoff"))
            .await
            .unwrap();
        add(&mut conn, &about("fix jitter", "kelve:jitter"))
            .await
            .unwrap();
        // The agent that traced Jitter writes it next, ahead of the rest.
        let after = After {
            resume_key: Some("entry.relate:kelve:jitter"),
            idea: Some("kelve:jitter"),
        };
        let next = take_for(&mut conn, "claude-code", None, None, after)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next.key, "fix jitter");
    }

    #[tokio::test]
    async fn the_agent_takes_the_most_wanted_first_and_answers_once() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        add(&mut conn, &new("a.rs", 2)).await.unwrap();
        add(&mut conn, &new("b.rs", 1)).await.unwrap();
        // Asked again, it's one job, and moves up.
        add(&mut conn, &new("a.rs", 0)).await.unwrap();
        assert_eq!(waiting(&mut conn).await.unwrap(), 2);
        let first = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(first.key, "a.rs");
        let second = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(second.key, "b.rs");
        assert!(take(&mut conn, "claude-code").await.unwrap().is_none());
        answer(
            &mut conn,
            first.id,
            "{\"x\":1}",
            Some("claude-opus"),
            "claude-code",
        )
        .await
        .unwrap();
        let got = by_key(&mut conn, "a.rs").await.unwrap().unwrap();
        assert_eq!(got.state, "answered");
        assert_eq!(got.model.as_deref(), Some("claude-opus"));
        // In the wrong shape, it stays with the agent fixing it: no other
        // takes it meanwhile. Twice, it's put aside; asked for again, it's back.
        assert!(!wrong(&mut conn, second.id, "no").await.unwrap());
        assert_eq!(
            by_key(&mut conn, "b.rs").await.unwrap().unwrap().state,
            "taken"
        );
        assert!(take(&mut conn, "claude-code").await.unwrap().is_none());
        assert!(wrong(&mut conn, second.id, "no").await.unwrap());
        assert_eq!(waiting(&mut conn).await.unwrap(), 0);
        add(&mut conn, &new("b.rs", 2)).await.unwrap();
        assert_eq!(waiting(&mut conn).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn what_was_opened_last_comes_first_after_any_do_next_said_since() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let older = crate::reader::open(&mut conn, "folder", "/a", "", "", "A")
            .await
            .unwrap();
        let latest = crate::reader::open(&mut conn, "folder", "/b", "", "", "B")
            .await
            .unwrap();
        let of = |key: &str, opened: i64, priority: i64| New {
            opened: Some(opened),
            ..new(key, priority)
        };
        add(&mut conn, &of("older, now", older, 0)).await.unwrap();
        add(&mut conn, &of("latest, the rest", latest, 2))
            .await
            .unwrap();
        add(&mut conn, &of("older, asked first", older, 2))
            .await
            .unwrap();
        let asked = by_key(&mut conn, "older, asked first")
            .await
            .unwrap()
            .unwrap();
        first(&mut conn, &[asked.id]).await.unwrap();
        let order: Vec<String> = open(&mut conn)
            .await
            .unwrap()
            .into_iter()
            .map(|a| a.key)
            .collect();
        assert_eq!(
            order,
            ["older, asked first", "latest, the rest", "older, now"]
        );
        // Something new opened: the Do next said before it no longer jumps ahead.
        let newest = crate::reader::open(&mut conn, "folder", "/c", "", "", "C")
            .await
            .unwrap();
        let_go_of_first(&mut conn).await.unwrap();
        add(&mut conn, &of("newest", newest, 2)).await.unwrap();
        let keys = |asks: Vec<Ask>| asks.into_iter().map(|a| a.key).collect::<Vec<_>>();
        assert_eq!(keys(open(&mut conn).await.unwrap())[0], "newest");
        // Said after it, Do next still does.
        first(&mut conn, &[asked.id]).await.unwrap();
        assert_eq!(
            keys(open(&mut conn).await.unwrap())[0],
            "older, asked first"
        );
        // Looking at what it's in moves that up, but never undoes the Do next.
        hurry(&mut conn, None, Some(older)).await.unwrap();
        assert_eq!(
            keys(open(&mut conn).await.unwrap())[0],
            "older, asked first"
        );
    }

    #[tokio::test]
    async fn a_job_that_finishes_another_goes_where_that_one_was() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        add(&mut conn, &new("debrief a", 0)).await.unwrap();
        add(&mut conn, &new("learn", 0)).await.unwrap();
        let debrief = take(&mut conn, "claude-code").await.unwrap().unwrap();
        answer(&mut conn, debrief.id, "{}", None, "claude-code")
            .await
            .unwrap();
        // What follows the debrief, asked for after the learning, still comes first.
        let research = New {
            kind: "research".into(),
            follows: Some("debrief a".into()),
            ..new("research a", 0)
        };
        add(&mut conn, &research).await.unwrap();
        let next = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(next.key, "research a");
    }

    #[tokio::test]
    async fn a_notes_jobs_go_to_its_project_and_stay_with_the_agent_that_took_them() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let note = |key: &str| New {
            kind: "note".into(),
            opened: None,
            ..new(key, 0)
        };
        add(&mut conn, &note("note:aaa:agent/x.md:v1"))
            .await
            .unwrap();
        add(&mut conn, &note("note:bbb:agent/y.md:v1"))
            .await
            .unwrap();
        // An agent in project B is given B's note, never A's.
        let b = take_for(
            &mut conn,
            "codex",
            Some("note:bbb:"),
            None,
            After::default(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(b.key, "note:bbb:agent/y.md:v1");
        assert!(
            take_for(
                &mut conn,
                "codex",
                Some("note:bbb:"),
                None,
                After::default()
            )
            .await
            .unwrap()
            .is_none()
        );
        let a = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(a.key, "note:aaa:agent/x.md:v1");
        // The note changes while A's job is taken: its job stays with the
        // agent, and only waiting ones go.
        add(&mut conn, &note("note:aaa:agent/x.md:v2"))
            .await
            .unwrap();
        drop_like(&mut conn, "note:aaa:agent/x.md:", None, false)
            .await
            .unwrap();
        let kept = by_key(&mut conn, &a.key).await.unwrap().unwrap();
        assert_eq!(kept.state, "taken");
        let v2 = by_key(&mut conn, "note:aaa:agent/x.md:v2")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v2.state, "dropped");
        // Done another way, all of them go.
        drop_like(&mut conn, "note:aaa:agent/x.md:", None, true)
            .await
            .unwrap();
        let gone = by_key(&mut conn, &a.key).await.unwrap().unwrap();
        assert_eq!(gone.state, "dropped");
        // Answered, then done for other words since: asked again.
        answer(&mut conn, b.id, "{}", None, "codex").await.unwrap();
        assert!(
            !reopen_answered_before(&mut conn, &b.key, 0).await.unwrap(),
            "answered after it was last done: kept"
        );
        assert!(
            reopen_answered_before(&mut conn, &b.key, now() + 1_000)
                .await
                .unwrap()
        );
        let again = by_key(&mut conn, &b.key).await.unwrap().unwrap();
        assert_eq!((again.state.as_str(), again.answer), ("waiting", None));
    }

    #[tokio::test]
    async fn an_agent_is_told_of_a_note_once_a_version() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        assert!(
            tell(&mut conn, "claude", "/p", "agent/x.md", "v1")
                .await
                .unwrap()
        );
        assert!(
            !tell(&mut conn, "claude", "/p", "agent/x.md", "v1")
                .await
                .unwrap()
        );
        assert!(
            tell(&mut conn, "codex", "/p", "agent/x.md", "v1")
                .await
                .unwrap()
        );
        assert!(
            tell(&mut conn, "claude", "/p", "agent/x.md", "v2")
                .await
                .unwrap()
        );
        assert!(
            tell(&mut conn, "claude", "/q", "agent/x.md", "v2")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn what_the_person_takes_off_stays_off_until_they_put_it_back() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        add(&mut conn, &new("a.rs", 2)).await.unwrap();
        let a = by_key(&mut conn, "a.rs").await.unwrap().unwrap();
        assert!(drop(&mut conn, a.id).await.unwrap());
        assert!(!drop(&mut conn, a.id).await.unwrap(), "already off");
        // Asked for again by the work, or opened again: it stays off.
        add(&mut conn, &new("a.rs", 0)).await.unwrap();
        assert_eq!(
            by_key(&mut conn, "a.rs").await.unwrap().unwrap().state,
            "dropped"
        );
        assert!(take(&mut conn, "claude-code").await.unwrap().is_none());
        // Kept a day, as what's answered is, it's still off.
        sweep(&mut conn).await.unwrap();
        assert!(by_key(&mut conn, "a.rs").await.unwrap().is_some());
        // One put aside after wrong answers comes back when asked for again.
        add(&mut conn, &new("b.rs", 2)).await.unwrap();
        let b = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert!(!wrong(&mut conn, b.id, "no").await.unwrap());
        assert!(wrong(&mut conn, b.id, "no").await.unwrap());
        add(&mut conn, &new("b.rs", 2)).await.unwrap();
        assert_eq!(
            by_key(&mut conn, "b.rs").await.unwrap().unwrap().state,
            "waiting"
        );
        assert_eq!(
            take(&mut conn, "claude-code").await.unwrap().unwrap().key,
            "b.rs"
        );
    }

    #[tokio::test]
    async fn agents_are_remembered_with_their_models() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        assert!(seen(&mut conn, "codex", Some("gpt-6"), None).await.unwrap());
        assert!(!seen(&mut conn, "codex", None, Some("plus")).await.unwrap());
        let a = agent(&mut conn, "codex").await.unwrap().unwrap();
        assert_eq!(a.model.as_deref(), Some("gpt-6"));
        assert_eq!(a.plan.as_deref(), Some("plus"));
        assert_eq!(
            latest_agent(&mut conn).await.unwrap().unwrap().name,
            "codex"
        );
        // Every agent seen lately, the latest first.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        seen(&mut conn, "claude-code", None, None).await.unwrap();
        let names: Vec<String> = agents_since(&mut conn, 0)
            .await
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, ["claude-code", "codex"]);
        assert!(
            agents_since(&mut conn, now() + 1_000)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
