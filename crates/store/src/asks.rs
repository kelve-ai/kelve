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
/// The same, for reading plainly: its words and lines are counted closely,
/// and a miss shouldn't throw away a page nearly done.
pub const MOST_PLAIN_TRIES: i64 = 3;

/// The jobs that read plainly.
const PLAIN: &str = "kind in ('plainly', 'plainlyFolder', 'plainlyChange')";

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
    /// Where it goes on the list: the id of the job it finishes, as a
    /// file's check and plainly go where its reading was.
    pub place: Option<i64>,
}

const COLUMNS: &str = "id, key, kind, about, idea, opened, material, schema, resume_kind, \
    resume_key, resume_payload, priority, state, answer, model, agent, tries, last_error, created_at, \
    taken_off_at, place";

/// The order jobs are taken in: what the person said to do next, since
/// they last opened something, first; then everything for what was opened
/// last; then the most wanted, by where each goes. Within what was opened
/// last: a note for the agent; then reading, each file's ideas checked and
/// the file read plainly before the next file's (the plainly job follows
/// the read job); then its folders' lines, and its folder's or change's
/// own page read plainly, written from those files; then the rest it
/// started, such as what it's built on. Its overview, the change's own page
/// or the top folder's, the first on the way in, comes before its files.
const ORDER: &str = "priority >= 0,
    coalesce(opened = (select id from opened order by opened_at desc, id desc limit 1), 0) desc,
    case when coalesce(json_extract(resume_payload, '$.first'), 0) then 0
         when kind in ('read', 'choose', 'plainly') then 1
         when kind in ('folders', 'plainlyFolder', 'plainlyChange') then 2
         when kind = 'builtOn' then 3 else 0 end,
    priority, coalesce(place, id), id";

/// Reading something opened before what was opened last: its way in and
/// the rest of its path, its files, their ideas, what it's built on, its
/// folders, and reading them plainly. It's paused, and not handed out,
/// until that thing is opened again, or the person's agent is asked to do
/// what's waiting for it by its number (`reader::wake`), for an hour after.
/// Only reading: what the person asked for themselves, wherever they
/// asked, still comes.
const PAUSED: &str = "kind in ('way', 'path', 'read', 'choose', 'builtOn', 'folders', 'plainly', 'plainlyFolder', 'plainlyChange')
    and opened is not null
    and opened != coalesce((select id from opened order by opened_at desc, id desc limit 1), opened)
    and opened not in (select id from opened where woken_at > cast(strftime('%s', 'now') as integer) * 1000 - 3600000)";

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
/// that finishes another where that one was, and never reading that's
/// paused. A job held too long without an answer is offered again.
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
/// the same file comes first, so one agent takes a file straight on, from
/// marking it to checking its ideas to writing it plainly, and every part
/// of a big one, before it goes on to another; and so for an idea, from
/// tracing it to writing its entry to fixing it (Nathan, 29 October). An
/// agent asked to do what's waiting for one thing opened, by its number,
/// takes only its jobs, and the notes of its project.
pub async fn take_for(
    conn: &mut Conn,
    agent: &str,
    notes: Option<&str>,
    only: Option<i64>,
    after: After<'_>,
) -> Result<Option<Ask>> {
    let now = now();
    let_go_of_lapsed(conn).await?;
    // A job's file, as its resume key ends: `:12:src/retry.rs`, whether
    // it's reading the file (`read.file:…`) or writing it plainly; or its idea.
    let same = "coalesce(coalesce(substr(resume_key, instr(resume_key, ':')), '') = ?4, 0) or coalesce(idea = ?5, 0)";
    let order = ORDER.replacen(
        "priority >= 0,",
        &format!("priority >= 0, not ({same}),"),
        1,
    );
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "update asks set state = 'taken', taken_at = ?1, agent = ?2
          where id = (select id from asks where state = 'waiting' and not ({PAUSED})
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
/// its fixed answer still finds it open; after `MOST_TRIES`, or
/// `MOST_PLAIN_TRIES` for reading plainly, it's put aside. Whether it was
/// put aside.
pub async fn wrong(conn: &mut Conn, id: i64, why: &str) -> Result<bool> {
    let (tries, most): (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "update asks set tries = tries + 1, last_error = ?2,
                state = case when tries + 1 >= (case when {PLAIN} then ?5 else ?3 end)
                             then 'dropped' else 'taken' end,
                taken_at = case when tries + 1 >= (case when {PLAIN} then ?5 else ?3 end)
                                then null else ?4 end
          where id = ?1
          returning tries, case when {PLAIN} then ?5 else ?3 end"
    )))
    .bind(id)
    .bind(why)
    .bind(MOST_TRIES)
    .bind(now())
    .bind(MOST_PLAIN_TRIES)
    .fetch_one(&mut *conn)
    .await?;
    Ok(tries >= most)
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

/// Takes off the list the reading still to do of something opened: the
/// person doesn't want it read further. How many jobs it was.
pub async fn drop_reading(conn: &mut Conn, opened: i64) -> Result<u64> {
    let done = sqlx::query(
        "update asks set state = 'dropped', taken_off_at = ?2, taken_at = null
          where opened = ?1 and state in ('waiting', 'taken')
            and kind in ('way', 'path', 'read', 'choose', 'builtOn', 'folders', 'plainly', 'plainlyFolder', 'plainlyChange')",
    )
    .bind(opened)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected())
}

/// Lets go of the jobs of a kind for what's open still waiting for the
/// agent, asked for with the rest of it, whose answers would run again a
/// job keyed `prefix` and a path not among `keep`: they're no longer
/// wanted. Never one the agent has, or one asked for sooner, as for a file
/// the person opened, or later, as a page of the rest after the way in.
/// Asked for again, each is a new job. How many there were.
pub async fn let_go_waiting(
    conn: &mut Conn,
    opened: i64,
    kind: &str,
    prefix: &str,
    keep: &std::collections::HashSet<String>,
) -> Result<u64> {
    let waiting: Vec<(i64, String)> = sqlx::query_as(
        "select id, resume_key from asks
          where opened = ?1 and kind = ?2 and state = 'waiting'
            and substr(resume_key, 1, length(?3)) = ?3
            and coalesce(json_extract(resume_payload, '$.priority'), 2) = 2",
    )
    .bind(opened)
    .bind(kind)
    .bind(prefix)
    .fetch_all(&mut *conn)
    .await?;
    let mut gone = 0;
    for (id, key) in waiting {
        if key.strip_prefix(prefix).is_some_and(|p| keep.contains(p)) {
            continue;
        }
        gone += sqlx::query("delete from asks where id = ?1 and state = 'waiting'")
            .bind(id)
            .execute(&mut *conn)
            .await?
            .rows_affected();
    }
    Ok(gone)
}

/// Puts back on the list what the person took off for an idea: they asked
/// for its entry again. Whether there was any.
pub async fn put_back(conn: &mut Conn, idea: &str) -> Result<bool> {
    let done = sqlx::query(
        "update asks set state = 'waiting', taken_off_at = null, tries = 0, priority = min(priority, 0)
          where idea = ?1 and state = 'dropped' and taken_off_at is not null",
    )
    .bind(idea)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Whether the person took off the list a job for an idea, and nothing
/// for it is on it now.
pub async fn taken_off(conn: &mut Conn, idea: &str) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "select exists(select 1 from asks where idea = ?1 and state = 'dropped' and taken_off_at is not null)
            and not exists(select 1 from asks where idea = ?1 and state in ('waiting', 'taken'))",
    )
    .bind(idea)
    .fetch_one(&mut *conn)
    .await?)
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

/// Lets go of the reading an agent holds of anything opened before what
/// was opened last: it's paused now, so it's no longer being done, and no
/// agent is handed it until that thing is opened again. An answer the
/// agent hands in for it is still taken.
pub async fn let_go_of_paused(conn: &mut Conn) -> Result<u64> {
    Ok(sqlx::query(sqlx::AssertSqlSafe(format!(
        "update asks set state = 'waiting', taken_at = null where state = 'taken' and {PAUSED}"
    )))
    .execute(&mut *conn)
    .await?
    .rows_affected())
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

/// How many jobs aren't yet answered, for what's open: and how many of those
/// read files.
pub async fn counts(conn: &mut Conn, opened: Option<i64>) -> Result<(i64, i64)> {
    Ok(sqlx::query_as(
        "select count(*), coalesce(sum(kind = 'read'), 0) from asks
          where state in ('waiting', 'taken') and (?1 is null or opened = ?1)",
    )
    .bind(opened)
    .fetch_one(&mut *conn)
    .await?)
}

/// How many files in what's open are read, with their ideas still to
/// check or their plainly still to write: a check is asked for only once
/// its file is read, and its plainly once its ideas are checked.
pub async fn checking(conn: &mut Conn, opened: i64) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "select count(*) from asks
          where state in ('waiting', 'taken') and kind in ('choose', 'plainly') and opened = ?1",
    )
    .bind(opened)
    .fetch_one(&mut *conn)
    .await?)
}

/// How many of the jobs reading what's open aren't yet answered: its
/// files, their ideas, and its files, folders and change read plainly.
pub async fn reading(conn: &mut Conn, opened: i64) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "select count(*) from asks
          where state in ('waiting', 'taken') and opened = ?1
            and kind in ('read', 'choose', 'plainly', 'plainlyFolder', 'plainlyChange')",
    )
    .bind(opened)
    .fetch_one(&mut *conn)
    .await?)
}

/// How many of a kind's jobs for what's open, asked for later, as the
/// rest's pages after the way in are, the agent has done: each by what it
/// was for.
pub async fn answered_later(conn: &mut Conn, opened: i64, kind: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "select count(distinct resume_key) from asks
          where opened = ?1 and kind = ?2 and state = 'answered'
            and json_extract(resume_payload, '$.priority') = 3",
    )
    .bind(opened)
    .bind(kind)
    .fetch_one(&mut *conn)
    .await?)
}

/// Lets go of the reading of something opened that still waits for the
/// agent, as it's opened again: its files may have changed since, and
/// reading for what they were is wasted. What's read now is asked for
/// afresh as it's opened; what the agent has is left with it, and what the
/// person took off stays off. How many there were.
pub async fn let_go_of_reading(conn: &mut Conn, opened: i64) -> Result<u64> {
    Ok(sqlx::query(
        "delete from asks
          where opened = ?1 and state = 'waiting'
            and kind in ('path', 'read', 'choose', 'plainly', 'plainlyFolder', 'plainlyChange', 'builtOn')",
    )
    .bind(opened)
    .execute(&mut *conn)
    .await?
    .rows_affected())
}

/// Where the `rank`th file on a path goes among the jobs of what's open:
/// ahead of every job its path doesn't name, whose places are their ids,
/// and of every later file on it. What finishes a file's reading takes the
/// same place, so it still comes straight after it.
pub fn on_path(rank: usize) -> i64 {
    i64::try_from(rank).unwrap_or(i64::MAX >> 1) - (1 << 40)
}

/// Puts the reading of what's open that's still to do in the order of its
/// path: each file named, by the key its reading resumes, at its place on
/// it (`on_path`); one an agent holds too, so it keeps its place if it
/// comes back.
pub async fn in_order(conn: &mut Conn, opened: i64, resume_keys: &[String]) -> Result<()> {
    // Each file's jobs by its file, whichever it is: its reading, the check
    // of its ideas, or, read before, only reading it plainly.
    let waiting: Vec<(i64, Option<String>)> = sqlx::query_as(
        "select id, resume_key from asks
          where opened = ?1 and kind in ('read', 'choose', 'plainly') and state in ('waiting', 'taken')",
    )
    .bind(opened)
    .fetch_all(&mut *conn)
    .await?;
    let rank: std::collections::HashMap<&str, usize> = resume_keys
        .iter()
        .enumerate()
        .filter_map(|(i, k)| file_of(k).map(|f| (f, i)))
        .collect();
    for (id, key) in waiting {
        if let Some(r) = key.as_deref().and_then(file_of).and_then(|f| rank.get(f)) {
            sqlx::query("update asks set place = ?1 where id = ?2")
                .bind(on_path(*r))
                .bind(id)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// Puts a file's jobs, by the file a key it resumes is for, at its place on
/// the path, while they wait: as they're asked for, which may be after the
/// path is given; its reading, the check of its ideas, or only reading it
/// plainly, when it was read before.
pub async fn place_reading(conn: &mut Conn, resume_key: &str, rank: usize) -> Result<()> {
    let Some(file) = file_of(resume_key) else {
        return Ok(());
    };
    sqlx::query(
        "update asks set place = ?1
          where kind in ('read', 'choose', 'plainly') and state = 'waiting'
            and substr(resume_key, instr(resume_key, ':')) = ?2",
    )
    .bind(on_path(rank))
    .bind(file)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The keys of the jobs whose answers run again the jobs of a kind the
/// agent has in hand, starting with `prefix`: the files it's reading.
pub async fn taken_resuming(conn: &mut Conn, kind: &str, prefix: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select resume_key from asks
          where kind = ?1 and state = 'taken' and substr(resume_key, 1, length(?2)) = ?2
          order by coalesce(place, id), id",
    )
    .bind(kind)
    .bind(prefix)
    .fetch_all(&mut *conn)
    .await?)
}

/// The reading of what's open the agent has started and not finished: its
/// jobs to read a file, check its ideas and read it plainly, and its pages,
/// each by kind, state and the key its answer runs again; reading handed in
/// too, as a file's next job may not be asked yet.
pub async fn reading_of(conn: &mut Conn, opened: i64) -> Result<Vec<(String, String, String)>> {
    Ok(sqlx::query_as(
        "select kind, state, resume_key from asks
          where opened = ?1 and kind in ('read', 'choose', 'plainly', 'plainlyFolder', 'plainlyChange')
            and resume_key is not null
            and (state in ('waiting', 'taken') or (kind = 'read' and state = 'answered'))",
    )
    .bind(opened)
    .fetch_all(&mut *conn)
    .await?)
}

/// Whether a job of a kind for what's open was dropped: put aside after
/// its tries, or taken off the list by the person.
pub async fn dropped_of(conn: &mut Conn, kind: &str, opened: i64) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "select exists(select 1 from asks where kind = ?1 and opened = ?2 and state = 'dropped')",
    )
    .bind(kind)
    .bind(opened)
    .fetch_one(&mut *conn)
    .await?)
}

/// Whether a job of a kind for what's open is with the agent, or waits
/// for it.
pub async fn asking(conn: &mut Conn, kind: &str, opened: i64) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "select exists(select 1 from asks where kind = ?1 and opened = ?2 and state in ('waiting', 'taken'))",
    )
    .bind(kind)
    .bind(opened)
    .fetch_one(&mut *conn)
    .await?)
}

/// The key of the latest job of a kind whose answer runs `resume_key`
/// again: the read job of a file, for what finishes it to follow.
pub async fn key_resuming(conn: &mut Conn, kind: &str, resume_key: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "select key from asks where kind = ?1 and resume_key = ?2 order by id desc limit 1",
    )
    .bind(kind)
    .bind(resume_key)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Whether a job whose answer runs `resume_key` again is with the agent,
/// or waits for it.
pub async fn waiting_resuming(conn: &mut Conn, resume_key: &str) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "select exists(select 1 from asks where resume_key = ?1 and state in ('waiting', 'taken'))",
    )
    .bind(resume_key)
    .fetch_one(&mut *conn)
    .await?)
}

/// The keys of the jobs put aside whose keys start with `prefix`.
pub async fn dropped_like(conn: &mut Conn, prefix: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select key from asks where state = 'dropped' and substr(key, 1, length(?1)) = ?1",
    )
    .bind(prefix)
    .fetch_all(&mut *conn)
    .await?)
}

/// Whether the agent has taken a job of a kind, about something in what's
/// opened, and not yet answered it.
pub async fn taken_about(conn: &mut Conn, kind: &str, opened: i64, about: &str) -> Result<bool> {
    Ok(sqlx::query_scalar(
        "select exists(select 1 from asks where kind = ? and opened = ? and about = ? and state = 'taken')",
    )
    .bind(kind)
    .bind(opened)
    .bind(about)
    .fetch_one(&mut *conn)
    .await?)
}

/// How many jobs wait for the agent to take them, but reading that's paused.
pub async fn waiting(conn: &mut Conn) -> Result<i64> {
    Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "select count(*) from asks where state = 'waiting' and not ({PAUSED})"
    )))
    .fetch_one(&mut *conn)
    .await?)
}

/// Where a page read plainly is in reading what it's part of, as the
/// list says it: the job, whether the agent has it, and how many files
/// the agent reads before it, each file's reading, the check of its
/// ideas and its plainly being one, and each folder's or change's own
/// page one more. None when it isn't on the list, or is paused.
pub async fn files_before(conn: &mut Conn, key: &str) -> Result<Option<(i64, bool, u32)>> {
    let jobs: Vec<(i64, String, String, String, Option<i64>, i64)> =
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "select id, key, state, kind, opened, coalesce(place, id) from asks
              where state in ('waiting', 'taken') and not ({PAUSED})
              order by state = 'taken' desc, {ORDER}"
        )))
        .fetch_all(&mut *conn)
        .await?;
    let Some(at) = jobs.iter().position(|(_, k, ..)| k == key) else {
        return Ok(None);
    };
    let (id, _, state, _, opened, file) = &jobs[at];
    // A file's jobs go where its reading was: that's the file. A folder's
    // or a change's own page is one more, as the list counts what's left.
    // What the agent has in hand is being read, not before it.
    let ahead = jobs[..at]
        .iter()
        .filter(|(_, _, s, _, o, _)| o == opened && s == "waiting");
    let files: std::collections::BTreeSet<i64> = ahead
        .clone()
        .filter(|(_, _, _, kind, _, f)| {
            matches!(kind.as_str(), "read" | "choose" | "plainly") && f != file
        })
        .map(|(.., f)| *f)
        .collect();
    let pages = ahead
        .filter(|(_, _, _, kind, ..)| matches!(kind.as_str(), "plainlyFolder" | "plainlyChange"))
        .count();
    Ok(Some((
        *id,
        state == "taken",
        u32::try_from(files.len() + pages).unwrap_or(u32::MAX),
    )))
}

/// Something opened before what was opened last, its reading paused.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Paused {
    pub opened: i64,
    /// The reading jobs it has left.
    pub left: i64,
    /// When the next thing was opened after it.
    pub paused_at: i64,
}

/// What's paused: each thing opened before what was opened last, with
/// reading still to do, the latest first.
pub async fn paused(conn: &mut Conn) -> Result<Vec<Paused>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select opened, sum(kind not in ('way', 'path')) as \"left\",
                coalesce((select min(o.opened_at) from opened o
                           where o.opened_at > (select p.opened_at from opened p where p.id = asks.opened)), 0)
                  as paused_at
           from asks
          where state in ('waiting', 'taken') and {PAUSED}
          group by opened
          having sum(kind not in ('way', 'path')) > 0
          order by (select p.opened_at from opened p where p.id = asks.opened) desc, opened desc"
    )))
    .fetch_all(&mut *conn)
    .await?)
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
    async fn a_file_read_before_is_written_plainly_at_its_place_on_the_path() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        // The second file on the path was read before: only its plainly
        // waits, behind the third's reading as it's asked.
        add(&mut conn, &new("12:c.rs", 0)).await.unwrap();
        let mut plain = new("12:b.rs:plainly", 0);
        plain.kind = "plainly".into();
        plain.resume_key = Some("plainly.file:12:b.rs".into());
        add(&mut conn, &plain).await.unwrap();
        in_order(
            &mut conn,
            1,
            &[
                "read.file:12:a.rs".into(),
                "read.file:12:b.rs".into(),
                "read.file:12:c.rs".into(),
            ],
        )
        .await
        .unwrap();
        let first = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(
            first.key, "12:b.rs:plainly",
            "b, second on the path, before c"
        );
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
        // Writing it plainly is the same file too.
        assert_eq!(
            file_of("plainly.file:12:b.rs"),
            file_of("read.file:12:b.rs")
        );
        assert_eq!(file_of("way.find:12"), None);
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
        assert_eq!(counts(&mut conn, None).await.unwrap(), (2, 2));
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
        assert_eq!(counts(&mut conn, None).await.unwrap(), (0, 0));
        // Reading plainly gets three tries: its words are counted closely.
        add(
            &mut conn,
            &New {
                kind: "plainly".into(),
                ..new("plainly c.rs", 0)
            },
        )
        .await
        .unwrap();
        let plain = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert!(!wrong(&mut conn, plain.id, "too many words").await.unwrap());
        assert!(!wrong(&mut conn, plain.id, "a line left out").await.unwrap());
        assert_eq!(
            by_key(&mut conn, "plainly c.rs")
                .await
                .unwrap()
                .unwrap()
                .state,
            "taken"
        );
        assert!(wrong(&mut conn, plain.id, "no").await.unwrap());
        add(&mut conn, &new("b.rs", 2)).await.unwrap();
        assert_eq!(counts(&mut conn, None).await.unwrap(), (1, 1));
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
    async fn an_agent_asked_for_one_thing_by_its_number_takes_only_its_jobs_and_wakes_it() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let a = crate::reader::open(&mut conn, "change", "/a", "", "", "A")
            .await
            .unwrap();
        let of = |key: &str, opened: i64| New {
            opened: Some(opened),
            ..new(key, 2)
        };
        add(&mut conn, &of("a/one.rs", a)).await.unwrap();
        // Another tab opens something else after: A's reading is paused.
        let b = crate::reader::open(&mut conn, "change", "/b", "", "", "B")
            .await
            .unwrap();
        add(&mut conn, &of("b/one.rs", b)).await.unwrap();
        assert!(
            take_for(&mut conn, "codex", None, Some(a), After::default())
                .await
                .unwrap()
                .is_none(),
            "paused, until its agent is asked for it"
        );
        // Asked for A by its number, its agent takes A's jobs alone, though B
        // was opened after.
        crate::reader::wake(&mut conn, a).await.unwrap();
        let mine = take_for(&mut conn, "codex", None, Some(a), After::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(mine.key, "a/one.rs");
        assert!(
            paused(&mut conn).await.unwrap().is_empty(),
            "woken, it isn't paused"
        );
        assert_eq!(
            let_go_of_paused(&mut conn).await.unwrap(),
            0,
            "what its agent holds stays held"
        );
        // The other agent, asked for B, takes only B's.
        let theirs = take_for(&mut conn, "claude-code", None, Some(b), After::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(theirs.key, "b/one.rs");
        assert!(
            take_for(&mut conn, "claude-code", None, Some(b), After::default())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn reading_what_was_opened_before_is_paused_until_its_opened_again() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let older = crate::reader::open(&mut conn, "folder", "/a", "", "", "A")
            .await
            .unwrap();
        let of = |key: &str, opened: i64| New {
            opened: Some(opened),
            ..new(key, 2)
        };
        add(&mut conn, &of("a/one.rs", older)).await.unwrap();
        add(&mut conn, &of("a/two.rs", older)).await.unwrap();
        // Reading a file plainly is part of reading it.
        add(
            &mut conn,
            &New {
                kind: "plainly".into(),
                ..of("plainly a/zero.rs", older)
            },
        )
        .await
        .unwrap();
        let zero = by_key(&mut conn, "plainly a/zero.rs")
            .await
            .unwrap()
            .unwrap()
            .id;
        assert_eq!(
            files_before(&mut conn, "plainly a/zero.rs").await.unwrap(),
            Some((zero, false, 2)),
            "two files are read before it"
        );
        // Read part way, then something else is opened.
        let one = take(&mut conn, "claude-code").await.unwrap().unwrap();
        answer(&mut conn, one.id, "{}", None, "claude-code")
            .await
            .unwrap();
        // The rest of its path is part of reading it too.
        add(
            &mut conn,
            &New {
                kind: "path".into(),
                ..of("path a", older)
            },
        )
        .await
        .unwrap();
        // An agent holds a file of it as something else is opened.
        let held = take(&mut conn, "claude-code").await.unwrap().unwrap();
        let latest = crate::reader::open(&mut conn, "folder", "/b", "", "", "B")
            .await
            .unwrap();
        assert_eq!(let_go_of_paused(&mut conn).await.unwrap(), 1);
        let held_now = by_key(&mut conn, &held.key).await.unwrap().unwrap();
        assert_eq!(
            held_now.state, "waiting",
            "paused, it's no longer being done"
        );
        add(&mut conn, &of("b/one.rs", latest)).await.unwrap();
        assert_eq!(waiting(&mut conn).await.unwrap(), 1);
        let b_one = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(b_one.key, "b/one.rs");
        answer(&mut conn, b_one.id, "{}", None, "claude-code")
            .await
            .unwrap();
        // Only what's paused is left: there's no more for the agent.
        assert!(take(&mut conn, "claude-code").await.unwrap().is_none());
        assert_eq!(waiting(&mut conn).await.unwrap(), 0);
        let paused_now = paused(&mut conn).await.unwrap();
        assert_eq!(paused_now.len(), 1);
        assert_eq!((paused_now[0].opened, paused_now[0].left), (older, 2));
        assert_eq!(
            files_before(&mut conn, "plainly a/zero.rs").await.unwrap(),
            None,
            "paused, it isn't on the list"
        );
        let b = crate::reader::get(&mut conn, latest)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(paused_now[0].paused_at, b.opened_at);
        // Opened again, it's what's open, and its reading carries on where it was.
        assert!(crate::reader::again(&mut conn, older).await.unwrap());
        assert!(
            !crate::reader::again(&mut conn, older).await.unwrap(),
            "what's open already doesn't move"
        );
        assert!(paused(&mut conn).await.unwrap().is_empty());
        // The rest of its path first, then its files where they were.
        assert_eq!(
            take(&mut conn, "claude-code").await.unwrap().unwrap().key,
            "path a"
        );
        assert_eq!(
            take(&mut conn, "claude-code").await.unwrap().unwrap().key,
            "a/two.rs"
        );
    }

    #[tokio::test]
    async fn what_you_asked_for_in_something_opened_before_still_comes_after_whats_open() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let older = crate::reader::open(&mut conn, "change", "/a", "", "commit 1", "A")
            .await
            .unwrap();
        add(
            &mut conn,
            &New {
                kind: "plainly".into(),
                opened: Some(older),
                ..new("plainly a.rs", 0)
            },
        )
        .await
        .unwrap();
        add(
            &mut conn,
            &New {
                kind: "write".into(),
                idea: Some("kelve:jitter".into()),
                opened: Some(older),
                ..new("write jitter", 1)
            },
        )
        .await
        .unwrap();
        add(
            &mut conn,
            &New {
                opened: Some(older),
                ..new("a.rs", 0)
            },
        )
        .await
        .unwrap();
        let latest = crate::reader::open(&mut conn, "folder", "/b", "", "", "B")
            .await
            .unwrap();
        add(
            &mut conn,
            &New {
                opened: Some(latest),
                ..new("b.rs", 2)
            },
        )
        .await
        .unwrap();
        let mut order = Vec::new();
        while let Some(a) = take(&mut conn, "claude-code").await.unwrap() {
            order.push(a.key);
        }
        // What's open's reading first, then what you asked for in the change;
        // the change's reading waits, its files read plainly too, as part of
        // reading them.
        assert_eq!(order, ["b.rs", "write jitter"]);
        for key in ["a.rs", "plainly a.rs"] {
            assert_eq!(
                by_key(&mut conn, key).await.unwrap().unwrap().state,
                "waiting"
            );
        }
    }

    #[tokio::test]
    async fn a_file_two_things_read_is_read_for_the_one_opened_last() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let older = crate::reader::open(&mut conn, "change", "/a", "", "commit 1", "A")
            .await
            .unwrap();
        let other = New {
            opened: Some(older),
            ..new("other.rs", 0)
        };
        add(&mut conn, &other).await.unwrap();
        let shared = New {
            opened: Some(older),
            ..new("shared.rs", 0)
        };
        add(&mut conn, &shared).await.unwrap();
        // Opened since, and the same lines asked for again.
        let latest = crate::reader::open(&mut conn, "change", "/a", "", "commit 2", "B")
            .await
            .unwrap();
        add(
            &mut conn,
            &New {
                opened: Some(latest),
                resume_key: Some("read.diff:latest".into()),
                ..new("shared.rs", 0)
            },
        )
        .await
        .unwrap();
        let next = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(next.key, "shared.rs");
        assert_eq!(next.opened, Some(latest));
        // What its answer runs next is the latest's too.
        assert_eq!(next.resume_key.as_deref(), Some("read.diff:latest"));
    }

    #[tokio::test]
    async fn a_job_that_finishes_another_goes_where_that_one_was() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        add(&mut conn, &new("read a.rs", 0)).await.unwrap();
        add(&mut conn, &new("trace", 0)).await.unwrap();
        let read = take(&mut conn, "claude-code").await.unwrap().unwrap();
        answer(&mut conn, read.id, "{}", None, "claude-code")
            .await
            .unwrap();
        // Checking a.rs's ideas, asked for after the tracing, still comes first.
        let choose = New {
            kind: "choose".into(),
            follows: Some("read a.rs".into()),
            ..new("choose a.rs", 0)
        };
        add(&mut conn, &choose).await.unwrap();
        let next = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(next.key, "choose a.rs");
        // A file read and its ideas being checked.
        assert_eq!(checking(&mut conn, 1).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn each_file_is_read_and_read_plainly_before_the_next_then_its_page_then_the_rest() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let id = crate::reader::open(&mut conn, "folder", "/a", "", "", "A")
            .await
            .unwrap();
        let job = |key: &str, kind: &str, priority: i64, follows: Option<&str>| New {
            kind: kind.into(),
            opened: Some(id),
            follows: follows.map(str::to_string),
            ..new(key, priority)
        };
        // Opened: what it's built on, asked as soon as there are ideas, and
        // its folders' lines come before its files' reading is asked for.
        add(&mut conn, &job("built on", "builtOn", 0, None))
            .await
            .unwrap();
        add(&mut conn, &job("folders", "folders", 2, None))
            .await
            .unwrap();
        add(&mut conn, &job("read a.rs", "read", 2, None))
            .await
            .unwrap();
        add(&mut conn, &job("read b.rs", "read", 2, None))
            .await
            .unwrap();
        add(&mut conn, &job("note", "note", 0, None)).await.unwrap();
        // The person looks at it: everything for it moves up, in its order.
        hurry(&mut conn, None, Some(id)).await.unwrap();
        let mut order = Vec::new();
        while let Some(a) = take(&mut conn, "claude-code").await.unwrap() {
            answer(&mut conn, a.id, "{}", None, "claude-code")
                .await
                .unwrap();
            // What each answer asks for next, as kelve's work asks it.
            let then: &[(&str, &str, Option<&str>)] = match a.key.as_str() {
                "read a.rs" => &[("choose a.rs", "choose", Some("read a.rs"))],
                "choose a.rs" => &[("plainly a.rs", "plainly", Some("read a.rs"))],
                // Its ideas were checked before: read plainly next.
                "read b.rs" => &[("plainly b.rs", "plainly", Some("read b.rs"))],
                // Every file read plainly: the folder's own page.
                "plainly b.rs" => &[("folder plainly", "plainlyFolder", None)],
                _ => &[],
            };
            for (key, kind, follows) in then {
                add(&mut conn, &job(key, kind, 2, *follows)).await.unwrap();
            }
            order.push(a.key);
        }
        assert_eq!(
            order,
            [
                "note",
                "read a.rs",
                "choose a.rs",
                "plainly a.rs",
                "read b.rs",
                "plainly b.rs",
                "folders",
                "folder plainly",
                "built on",
            ]
        );
        // A file read and its plainly being written counts as checking.
        assert_eq!(reading(&mut conn, id).await.unwrap(), 0);
        add(&mut conn, &job("plainly c.rs", "plainly", 2, None))
            .await
            .unwrap();
        assert_eq!(checking(&mut conn, id).await.unwrap(), 1);
        // And as reading what's open, as a folder's page is; what it's
        // built on isn't.
        add(
            &mut conn,
            &job("folder src plainly", "plainlyFolder", 2, None),
        )
        .await
        .unwrap();
        add(&mut conn, &job("built on again", "builtOn", 2, None))
            .await
            .unwrap();
        assert_eq!(reading(&mut conn, id).await.unwrap(), 2);
        // The read job a file's plainly follows, by the work it resumes.
        assert_eq!(
            key_resuming(&mut conn, "read", "read.file:read a.rs")
                .await
                .unwrap()
                .as_deref(),
            Some("read a.rs")
        );
    }

    #[tokio::test]
    async fn reading_no_longer_wanted_is_let_go_but_never_what_the_agent_has_or_was_asked_sooner() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let job = |path: &str, priority: i64| New {
            resume_key: Some(format!("read.file:1:{path}")),
            resume_payload: format!(r#"{{"opened":1,"path":"{path}","priority":{priority}}}"#),
            ..new(&format!("read {path}"), 0)
        };
        for path in ["a.rs", "b.rs", "c.rs"] {
            add(&mut conn, &job(path, 2)).await.unwrap();
        }
        // The person opened d.rs: asked for sooner.
        add(&mut conn, &job("d.rs", 0)).await.unwrap();
        // The agent has a.rs.
        let taken = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(taken.key, "read a.rs");
        let keep: std::collections::HashSet<String> = ["c.rs".to_string()].into();
        let gone = let_go_waiting(&mut conn, 1, "read", "read.file:1:", &keep)
            .await
            .unwrap();
        assert_eq!(
            gone, 1,
            "only b.rs: a.rs is taken, c.rs kept, d.rs asked sooner"
        );
        assert!(by_key(&mut conn, "read b.rs").await.unwrap().is_none());
        for kept in ["read a.rs", "read c.rs", "read d.rs"] {
            assert!(by_key(&mut conn, kept).await.unwrap().is_some(), "{kept}");
        }
        // Asked for again, it's a new job.
        add(&mut conn, &job("b.rs", 2)).await.unwrap();
        assert_eq!(
            by_key(&mut conn, "read b.rs").await.unwrap().unwrap().state,
            "waiting"
        );
    }

    #[tokio::test]
    async fn a_page_deep_in_a_long_reading_says_how_many_files_are_read_before_it() {
        let pool = crate::memory().await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let id = crate::reader::open(&mut conn, "folder", "/a", "", "", "A")
            .await
            .unwrap();
        let job = |key: &str, kind: &str, follows: Option<&str>| New {
            kind: kind.into(),
            opened: Some(id),
            follows: follows.map(str::to_string),
            ..new(key, 2)
        };
        // Eight files: each read, its ideas checked and read plainly, as
        // many jobs as there are, but a file each.
        for f in 0..8 {
            let read = format!("read {f}.rs");
            add(&mut conn, &job(&read, "read", None)).await.unwrap();
            if f < 6 {
                add(
                    &mut conn,
                    &job(&format!("choose {f}.rs"), "choose", Some(&read)),
                )
                .await
                .unwrap();
            }
        }
        add(
            &mut conn,
            &job("plainly 6.rs", "plainly", Some("read 6.rs")),
        )
        .await
        .unwrap();
        add(&mut conn, &job("folder plainly", "plainlyFolder", None))
            .await
            .unwrap();
        add(&mut conn, &job("top plainly", "plainlyFolder", None))
            .await
            .unwrap();
        add(&mut conn, &job("note", "note", None)).await.unwrap();
        // The seventh file's page: six files before it, though more jobs are.
        let (_, writing, before) = files_before(&mut conn, "plainly 6.rs")
            .await
            .unwrap()
            .unwrap();
        assert!(!writing);
        assert_eq!(before, 6);
        // The folder's page comes after every file.
        let (.., before) = files_before(&mut conn, "folder plainly")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before, 8);
        // The top's after that folder's page: one more.
        let (.., before) = files_before(&mut conn, "top plainly")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before, 9);
        // Being read now, a file is still before it while what's left of
        // it waits; its own reading isn't.
        let note = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(note.key, "note");
        answer(&mut conn, note.id, "{}", None, "claude-code")
            .await
            .unwrap();
        let read = take(&mut conn, "claude-code").await.unwrap().unwrap();
        assert_eq!(read.key, "read 0.rs");
        let (.., before) = files_before(&mut conn, "plainly 6.rs")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before, 6);
        // Do next: nothing before it.
        let (ask, ..) = files_before(&mut conn, "plainly 6.rs")
            .await
            .unwrap()
            .unwrap();
        first(&mut conn, &[ask]).await.unwrap();
        let (.., before) = files_before(&mut conn, "plainly 6.rs")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(before, 0, "what the agent has in hand isn't before it");
        assert!(files_before(&mut conn, "nothing").await.unwrap().is_none());
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
        add(
            &mut conn,
            &New {
                kind: "write".into(),
                idea: Some("kelve:jitter".into()),
                opened: None,
                ..new("write jitter", 1)
            },
        )
        .await
        .unwrap();
        let a = by_key(&mut conn, "a.rs").await.unwrap().unwrap();
        let w = by_key(&mut conn, "write jitter").await.unwrap().unwrap();
        assert!(drop(&mut conn, a.id).await.unwrap());
        assert!(drop(&mut conn, w.id).await.unwrap());
        assert!(!drop(&mut conn, a.id).await.unwrap(), "already off");
        assert!(taken_off(&mut conn, "kelve:jitter").await.unwrap());
        // Asked for again by the work, or opened again: it stays off.
        add(&mut conn, &new("a.rs", 0)).await.unwrap();
        add(
            &mut conn,
            &New {
                kind: "write".into(),
                idea: Some("kelve:jitter".into()),
                opened: None,
                ..new("write jitter", 0)
            },
        )
        .await
        .unwrap();
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
        // The person asks for the entry again: it's back, first.
        assert!(put_back(&mut conn, "kelve:jitter").await.unwrap());
        assert!(!taken_off(&mut conn, "kelve:jitter").await.unwrap());
        assert_eq!(
            take(&mut conn, "claude-code").await.unwrap().unwrap().key,
            "write jitter"
        );
        // What's paused, taken off: the rest of its reading, at once.
        add(
            &mut conn,
            &New {
                opened: Some(7),
                ..new("c.rs", 2)
            },
        )
        .await
        .unwrap();
        add(
            &mut conn,
            &New {
                kind: "plainly".into(),
                opened: Some(7),
                ..new("plainly c.rs", 2)
            },
        )
        .await
        .unwrap();
        assert_eq!(drop_reading(&mut conn, 7).await.unwrap(), 2);
        add(
            &mut conn,
            &New {
                opened: Some(7),
                ..new("c.rs", 0)
            },
        )
        .await
        .unwrap();
        assert!(take(&mut conn, "claude-code").await.unwrap().is_none());
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
