//! Lessons: those the shared graph served about the ideas kelve marks, those
//! given to the person's agent and what came of them, what failed as it worked,
//! and what it learned, held a day before it leaves.

use crate::{Conn, Result, now};

/// Keeps the lessons the graph served about some ideas, in place of those
/// kept before: each lesson's ID, the ideas it's about by key, and its page
/// as JSON.
pub async fn keep_fetched(
    conn: &mut Conn,
    ideas: &[String],
    lessons: &[(String, Vec<String>, String)],
) -> Result<()> {
    let at = now();
    for idea in ideas {
        sqlx::query("delete from lesson_ideas where idea = ?")
            .bind(idea)
            .execute(&mut *conn)
            .await?;
        sqlx::query(
            "insert into lessons_fetched (idea, fetched_at) values (?, ?)
             on conflict (idea) do update set fetched_at = excluded.fetched_at",
        )
        .bind(idea)
        .bind(at)
        .execute(&mut *conn)
        .await?;
    }
    for (id, about, page) in lessons {
        sqlx::query(
            "insert into lessons (id, page, fetched_at) values (?, ?, ?)
             on conflict (id) do update set page = excluded.page, fetched_at = excluded.fetched_at",
        )
        .bind(id)
        .bind(page)
        .bind(at)
        .execute(&mut *conn)
        .await?;
        for idea in about {
            sqlx::query("insert or ignore into lesson_ideas (lesson, idea) values (?, ?)")
                .bind(id)
                .bind(idea)
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(())
}

/// Of these ideas, those whose lessons weren't fetched since `since`.
pub async fn stale(conn: &mut Conn, ideas: &[String], since: i64) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for idea in ideas {
        let at: Option<i64> =
            sqlx::query_scalar("select fetched_at from lessons_fetched where idea = ?")
                .bind(idea)
                .fetch_optional(&mut *conn)
                .await?;
        if at.is_none_or(|at| at < since) {
            out.push(idea.clone());
        }
    }
    Ok(out)
}

/// The lessons kept about any of these ideas: each lesson's ID, the idea
/// it was found by, and its page as JSON.
pub async fn about(conn: &mut Conn, ideas: &[String]) -> Result<Vec<(String, String, String)>> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    for idea in ideas {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "select l.id, l.page from lesson_ideas i join lessons l on l.id = i.lesson
             where i.idea = ? order by l.id",
        )
        .bind(idea)
        .fetch_all(&mut *conn)
        .await?;
        for (id, page) in rows {
            if !out.iter().any(|(o, _, _)| *o == id) {
                out.push((id, idea.clone(), page));
            }
        }
    }
    Ok(out)
}

/// One lesson kept, as its page.
pub async fn page(conn: &mut Conn, id: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("select page from lessons where id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// Keeps what the agent wrote in a file, in a session.
pub async fn keep_written(
    conn: &mut Conn,
    session: &str,
    root: &str,
    path: &str,
    text: &str,
) -> Result<()> {
    sqlx::query("insert into written (session, root, path, text, at) values (?, ?, ?, ?, ?)")
        .bind(session)
        .bind(root)
        .bind(path)
        .bind(text)
        .bind(now())
        .execute(&mut *conn)
        .await?;
    sqlx::query("delete from written where at < ?")
        .bind(now() - 24 * 60 * 60 * 1000)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A moment in a session, now: when the one before it was, if there was one
/// in the last day.
pub async fn moment_now(conn: &mut Conn, session: &str) -> Result<Option<i64>> {
    let day_ago = now() - 24 * 60 * 60 * 1000;
    let before: Option<i64> = sqlx::query_scalar("select at from moments where session = ?")
        .bind(session)
        .fetch_optional(&mut *conn)
        .await?;
    sqlx::query(
        "insert into moments (session, at) values (?, ?)
         on conflict (session) do update set at = excluded.at",
    )
    .bind(session)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    sqlx::query("delete from moments where at < ?")
        .bind(day_ago)
        .execute(&mut *conn)
        .await?;
    Ok(before.filter(|at| *at >= day_ago))
}

/// A file as kelve last saw it in a session, masked.
pub async fn seen_text(
    conn: &mut Conn,
    session: &str,
    root: &str,
    path: &str,
) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "select text from files_seen where session = ? and root = ? and path = ?",
    )
    .bind(session)
    .bind(root)
    .bind(path)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Keeps a file as kelve saw it in a session, masked, for a day.
pub async fn keep_seen(
    conn: &mut Conn,
    session: &str,
    root: &str,
    path: &str,
    text: &str,
) -> Result<()> {
    sqlx::query(
        "insert into files_seen (session, root, path, text, at) values (?, ?, ?, ?, ?)
         on conflict (session, root, path) do update set text = excluded.text, at = excluded.at",
    )
    .bind(session)
    .bind(root)
    .bind(path)
    .bind(text)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    sqlx::query("delete from files_seen where at < ?")
        .bind(now() - 24 * 60 * 60 * 1000)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// What the agent wrote in a file earlier in a session, oldest first.
pub async fn written_in(
    conn: &mut Conn,
    session: &str,
    root: &str,
    path: &str,
) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select text from written where session = ? and root = ? and path = ? order by id",
    )
    .bind(session)
    .bind(root)
    .bind(path)
    .fetch_all(&mut *conn)
    .await?)
}

/// Work the agent replaced, waiting until its second way stands.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Replaced {
    pub id: i64,
    pub root: String,
    pub path: String,
    pub first: String,
    pub second: String,
}

/// Keeps work the agent replaced.
pub async fn keep_replaced(
    conn: &mut Conn,
    session: &str,
    root: &str,
    path: &str,
    first: &str,
    second: &str,
) -> Result<()> {
    sqlx::query(
        "insert into replaced (session, root, path, first, second, at) values (?, ?, ?, ?, ?, ?)",
    )
    .bind(session)
    .bind(root)
    .bind(path)
    .bind(first)
    .bind(second)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Work replaced in a session not asked about yet, oldest first; and how
/// many were asked about already.
pub async fn replaced_waiting(conn: &mut Conn, session: &str) -> Result<(Vec<Replaced>, i64)> {
    let waiting = sqlx::query_as(
        "select id, root, path, first, second from replaced where session = ? and asked = 0 order by id",
    )
    .bind(session)
    .fetch_all(&mut *conn)
    .await?;
    let asked: i64 =
        sqlx::query_scalar("select count(*) from replaced where session = ? and asked = 1")
            .bind(session)
            .fetch_one(&mut *conn)
            .await?;
    Ok((waiting, asked))
}

/// Settles work replaced: asked about, or not to be.
pub async fn replaced_settled(conn: &mut Conn, ids: &[i64], asked: bool) -> Result<()> {
    for id in ids {
        sqlx::query("update replaced set asked = ? where id = ?")
            .bind(if asked { 1 } else { 2 })
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// One piece of replaced work, by id.
pub async fn replaced_one(conn: &mut Conn, id: i64) -> Result<Option<Replaced>> {
    Ok(
        sqlx::query_as("select id, root, path, first, second from replaced where id = ?")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// The folder a walk is for, as kelve's hook first meets an agent in it:
/// true when it's new, so it's walked once.
pub async fn start_walk(conn: &mut Conn, root: &str) -> Result<bool> {
    let made = sqlx::query("insert or ignore into walked_roots (root, walked_at) values (?, ?)")
        .bind(root)
        .bind(now())
        .execute(&mut *conn)
        .await?
        .rows_affected();
    Ok(made == 1)
}

/// What a walk found: the files, and whether it stopped at its cap.
pub async fn keep_walk(conn: &mut Conn, root: &str, files: &[String], cut: bool) -> Result<()> {
    for path in files {
        keep_walked(conn, root, path).await?;
    }
    sqlx::query("update walked_roots set files = ?, cut = ?, walked_at = ? where root = ?")
        .bind(i64::try_from(files.len()).unwrap_or(i64::MAX))
        .bind(cut)
        .bind(now())
        .bind(root)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A file in a walked folder, found by the walk or written since.
pub async fn keep_walked(conn: &mut Conn, root: &str, path: &str) -> Result<()> {
    sqlx::query("insert or ignore into walked_files (root, path) values (?, ?)")
        .bind(root)
        .bind(path)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The deepest walked folder that holds `dir`, or is it, with folders told
/// apart as `reader::read_at` tells them.
pub async fn walked_root_of(conn: &mut Conn, dir: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "select root from walked_roots where root = ?1 or replace(?1, ?2, '/') like replace(root, ?2, '/') || '/%' order by length(root) desc limit 1",
    )
    .bind(dir)
    .bind(std::path::MAIN_SEPARATOR_STR)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The files a walk found in a folder, by path.
pub async fn walked_files(conn: &mut Conn, root: &str) -> Result<Vec<String>> {
    Ok(
        sqlx::query_scalar("select path from walked_files where root = ? order by path")
            .bind(root)
            .fetch_all(&mut *conn)
            .await?,
    )
}

/// A walked folder's record: how many files, and whether the walk was cut.
pub async fn walked(conn: &mut Conn, root: &str) -> Result<Option<(i64, bool)>> {
    Ok(
        sqlx::query_as("select files, cut from walked_roots where root = ?")
            .bind(root)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// Keeps the libraries a file read imports, in place of those kept before.
pub async fn keep_uses(conn: &mut Conn, root: &str, path: &str, names: &[String]) -> Result<()> {
    sqlx::query("delete from uses where root = ? and path = ?")
        .bind(root)
        .bind(path)
        .execute(&mut *conn)
        .await?;
    for name in names {
        sqlx::query("insert or ignore into uses (root, path, name) values (?, ?, ?)")
            .bind(root)
            .bind(path)
            .bind(name)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Adds to what a path uses, keeping what it used already: the tools the
/// agent runs in a folder, kept under no file.
pub async fn add_uses(conn: &mut Conn, root: &str, path: &str, names: &[String]) -> Result<()> {
    for name in names {
        sqlx::query("insert or ignore into uses (root, path, name) values (?, ?, ?)")
            .bind(root)
            .bind(path)
            .bind(name)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Of some libraries, those not looked up in the graph since `since`.
pub async fn to_look_up(conn: &mut Conn, names: &[String], since: i64) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for name in names {
        let looked: Option<i64> =
            sqlx::query_scalar("select looked_at from libraries where name = ?")
                .bind(name)
                .fetch_optional(&mut *conn)
                .await?;
        if looked.is_none_or(|at| at < since) {
            out.push(name.clone());
        }
    }
    Ok(out)
}

/// Keeps which of the graph's ideas a library is, or that it has none.
pub async fn looked_up(conn: &mut Conn, name: &str, idea: Option<&str>) -> Result<()> {
    sqlx::query(
        "insert into libraries (name, idea, looked_at) values (?, ?, ?)
         on conflict (name) do update set idea = excluded.idea, looked_at = excluded.looked_at",
    )
    .bind(name)
    .bind(idea)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The graph's ideas for the libraries some files of a folder import, each
/// once, by key, with the library's name.
pub async fn used_in(
    conn: &mut Conn,
    root: &str,
    paths: &[String],
) -> Result<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();
    for path in paths {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "select l.idea, u.name from uses u join libraries l on l.name = u.name
              where u.root = ? and u.path = ? and l.idea is not null order by u.name",
        )
        .bind(root)
        .bind(path)
        .fetch_all(&mut *conn)
        .await?;
        for row in rows {
            if !out.iter().any(|(k, _)| *k == row.0) {
                out.push(row);
            }
        }
    }
    Ok(out)
}

/// Keeps that the agent read or edited a file in a session.
pub async fn touch(
    conn: &mut Conn,
    session: &str,
    root: &str,
    path: &str,
    edited: bool,
) -> Result<()> {
    sqlx::query(
        "insert into touched (session, root, path, edited, at) values (?, ?, ?, ?, ?)
         on conflict (session, root, path) do update set at = excluded.at, edited = max(edited, excluded.edited)",
    )
    .bind(session)
    .bind(root)
    .bind(path)
    .bind(edited)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// The files the agent touched in a session, in the folder it touched last,
/// oldest first; and the one it edited last.
pub async fn touched(
    conn: &mut Conn,
    session: &str,
) -> Result<(String, Vec<String>, Option<String>)> {
    let rows: Vec<(String, String, i64)> =
        sqlx::query_as("select root, path, edited from touched where session = ? order by at")
            .bind(session)
            .fetch_all(&mut *conn)
            .await?;
    let Some(root) = rows.last().map(|r| r.0.clone()) else {
        return Ok((String::new(), Vec::new(), None));
    };
    let here: Vec<&(String, String, i64)> = rows.iter().filter(|r| r.0 == root).collect();
    let edited = here.iter().rev().find(|r| r.2 == 1).map(|r| r.1.clone());
    Ok((root, here.iter().map(|r| r.1.clone()).collect(), edited))
}

/// The files touched lately in a folder, by any session, newest first.
pub async fn touched_in(conn: &mut Conn, root: &str, since: i64) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select path from touched where root = ? and at >= ? group by path order by max(at) desc limit 50",
    )
    .bind(root)
    .bind(since)
    .fetch_all(&mut *conn)
    .await?)
}

/// Every lesson kept that's seen as a problem, as its page.
pub async fn seen_as_any(conn: &mut Conn) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar(
        "select page from lessons where json_extract(page, '$.lesson.seenAs') is not null",
    )
    .fetch_all(&mut *conn)
    .await?)
}

/// How long what's kept about lessons is kept: what was touched a day;
/// what failed a week; lessons given, once what came of them has gone, 90
/// days; lessons fetched for ideas not looked at since, 30 days.
pub async fn sweep(conn: &mut Conn) -> Result<()> {
    const DAY: i64 = 24 * 60 * 60 * 1000;
    let at = now();
    sqlx::query("delete from touched where at < ?")
        .bind(at - DAY)
        .execute(&mut *conn)
        .await?;
    sqlx::query("delete from failed where failed_at < ?")
        .bind(at - 7 * DAY)
        .execute(&mut *conn)
        .await?;
    sqlx::query("delete from given where given_at < ? and (sent = 1 or outcome = 'unknown')")
        .bind(at - 90 * DAY)
        .execute(&mut *conn)
        .await?;
    sqlx::query("delete from lessons_fetched where fetched_at < ?")
        .bind(at - 30 * DAY)
        .execute(&mut *conn)
        .await?;
    sqlx::query("delete from lesson_ideas where idea not in (select idea from lessons_fetched)")
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "delete from lessons where id not in (select lesson from lesson_ideas)
            and id not in (select lesson from given)",
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Keeps what kelve has for an agent until it next calls its model.
pub async fn hold_for(conn: &mut Conn, session: &str, text: &str) -> Result<()> {
    sqlx::query("insert into held_for_agent (session, text, at) values (?, ?, ?)")
        .bind(session)
        .bind(text)
        .bind(now())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// What was kept for an agent, oldest first, now given: kept no longer.
pub async fn take_held(conn: &mut Conn, session: &str) -> Result<Vec<String>> {
    let texts: Vec<String> =
        sqlx::query_scalar("select text from held_for_agent where session = ? order by id")
            .bind(session)
            .fetch_all(&mut *conn)
            .await?;
    sqlx::query("delete from held_for_agent where session = ?")
        .bind(session)
        .execute(&mut *conn)
        .await?;
    Ok(texts)
}

/// A lesson given to the agent.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Given {
    pub id: i64,
    pub lesson: String,
    pub session: String,
    pub moment: String,
    pub root: String,
    pub path: String,
    pub problem: String,
    pub command: String,
    pub given_at: i64,
    pub outcome: String,
    pub shown: String,
    pub said: String,
    /// Held back from the agent, to measure the lessons.
    pub withheld: bool,
    /// How likely it was given, in thousandths.
    pub chance: i64,
    /// The agent it was given to, as its hook names it: empty when it
    /// didn't say.
    pub agent: String,
    /// Given as the agent wrote the usual way it replaces.
    pub instead: bool,
    /// Then done the lesson's way: the usual way left the file.
    pub done: bool,
    /// The line the agent wrote that was the usual way, as it wrote it:
    /// empty unless it was given instead.
    pub usual: String,
}

/// What's needed to keep a lesson given.
#[derive(Debug, Clone, Default)]
pub struct Give<'a> {
    pub lesson: &'a str,
    pub session: &'a str,
    pub moment: &'a str,
    pub root: &'a str,
    pub path: &'a str,
    pub problem: &'a str,
    pub command: &'a str,
    pub withheld: bool,
    /// How likely it was given, in thousandths: 1000 when it couldn't have
    /// been held back.
    pub chance: i64,
    /// The agent it's given to, as its hook names it.
    pub agent: &'a str,
    /// Given as the agent wrote the usual way it replaces.
    pub instead: bool,
    /// That line, as the agent wrote it.
    pub usual: &'a str,
}

/// Keeps a lesson given to the agent.
pub async fn give(conn: &mut Conn, g: &Give<'_>) -> Result<i64> {
    let done = sqlx::query(
        "insert into given (lesson, session, moment, root, path, problem, command, given_at, withheld, chance, agent, instead, usual)
         values (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(g.lesson)
    .bind(g.session)
    .bind(g.moment)
    .bind(g.root)
    .bind(g.path)
    .bind(g.problem)
    .bind(g.command)
    .bind(now())
    .bind(g.withheld)
    .bind(if g.chance == 0 { 1000 } else { g.chance })
    .bind(g.agent)
    .bind(g.instead)
    .bind(g.usual)
    .execute(&mut *conn)
    .await?;
    Ok(done.last_insert_rowid())
}

/// Keeps that a lesson given as the agent wrote the usual way was then
/// done the lesson's way.
pub async fn done_its_way(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query("update given set done = 1 where id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A query for lessons given, with what follows `from given`.
macro_rules! given {
    ($rest:literal) => {
        concat!(
            "select id, lesson, session, moment, root, path, problem, command, given_at, outcome, shown, said, withheld, chance, agent, instead, done, usual from given ",
            $rest
        )
    };
}

/// The lessons given in a session, oldest first.
pub async fn given_in(conn: &mut Conn, session: &str) -> Result<Vec<Given>> {
    Ok(sqlx::query_as(given!("where session = ? order by id"))
        .bind(session)
        .fetch_all(&mut *conn)
        .await?)
}

/// The lessons given as the agent worked in a folder, newest first: on
/// these files, or anywhere in it when there are none.
pub async fn given_at(conn: &mut Conn, root: &str, paths: &[String]) -> Result<Vec<Given>> {
    let rows: Vec<Given> = sqlx::query_as(given!("where root = ? order by id desc limit 500"))
        .bind(root)
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows
        .into_iter()
        .filter(|g| paths.is_empty() || paths.contains(&g.path))
        .collect())
}

/// Every lesson given, newest first, at most `limit`.
pub async fn given_lately(conn: &mut Conn, limit: i64) -> Result<Vec<Given>> {
    Ok(sqlx::query_as(given!("order by id desc limit ?"))
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?)
}

/// What came of a lesson given, and what showed it. What a stronger sign
/// showed isn't replaced by a weaker one: `strength` is the new one's.
pub async fn came_of(
    conn: &mut Conn,
    id: i64,
    outcome: &str,
    shown: &str,
    strength: u8,
    said: &str,
) -> Result<bool> {
    let row: Option<(String,)> = sqlx::query_as("select shown from given where id = ?")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some((was,)) = row else {
        return Ok(false);
    };
    let was_strength = match was.as_str() {
        "passed" | "cameBack" => 3,
        "stayed" | "changedBack" => 2,
        "said" | "notHere" => 1,
        _ => 0,
    };
    if strength < was_strength {
        return Ok(false);
    }
    sqlx::query("update given set outcome = ?, shown = ?, said = ?, sent = 0 where id = ?")
        .bind(outcome)
        .bind(shown)
        .bind(said)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(true)
}

/// The file a lesson given as something failed is shown on: where the fix went.
pub async fn set_path(conn: &mut Conn, id: i64, path: &str) -> Result<()> {
    sqlx::query("update given set path = ? where id = ?")
        .bind(path)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// What came of lessons given, or held back, that hasn't gone to the
/// graph: each given's id, its lesson, its outcome, what showed it, whether
/// it was held back and how likely it was given, at most `limit`.
pub async fn outcomes_unsent(
    conn: &mut Conn,
    limit: i64,
) -> Result<Vec<(i64, String, String, String, bool, i64)>> {
    Ok(sqlx::query_as(
        "select id, lesson, outcome, shown, withheld, chance from given where sent = 0 and outcome != 'unknown' limit ?",
    )
    .bind(limit)
    .fetch_all(&mut *conn)
    .await?)
}

/// Notes that what came of these has gone.
pub async fn outcomes_sent(conn: &mut Conn, ids: &[i64]) -> Result<()> {
    for id in ids {
        sqlx::query("update given set sent = 1 where id = ?")
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// Something that failed as the agent worked.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Failed {
    pub id: i64,
    pub session: String,
    pub root: String,
    pub command: String,
    pub problem: String,
    pub files: String,
    pub failed_at: i64,
    pub passed_at: Option<i64>,
}

/// Keeps what failed: its id.
pub async fn failed(
    conn: &mut Conn,
    session: &str,
    root: &str,
    command: &str,
    problem: &str,
    files: &str,
) -> Result<i64> {
    let done = sqlx::query(
        "insert into failed (session, root, command, problem, files, failed_at) values (?, ?, ?, ?, ?, ?)",
    )
    .bind(session)
    .bind(root)
    .bind(command)
    .bind(problem)
    .bind(files)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(done.last_insert_rowid())
}

/// A query for what failed, with what follows `from failed`.
macro_rules! failed {
    ($rest:literal) => {
        concat!(
            "select id, session, root, command, problem, files, failed_at, passed_at from failed ",
            $rest
        )
    };
}

/// Everything that failed in a session and hasn't passed since, oldest
/// first.
pub async fn unpassed(conn: &mut Conn, session: &str) -> Result<Vec<Failed>> {
    Ok(sqlx::query_as(failed!(
        "where session = ? and passed_at is null order by id"
    ))
    .bind(session)
    .fetch_all(&mut *conn)
    .await?)
}

/// What failed in a session under a command and hasn't passed since.
pub async fn still_failing(conn: &mut Conn, session: &str, command: &str) -> Result<Vec<Failed>> {
    Ok(sqlx::query_as(failed!(
        "where session = ? and command = ? and passed_at is null order by id"
    ))
    .bind(session)
    .bind(command)
    .fetch_all(&mut *conn)
    .await?)
}

/// Says what failed under a command has passed since.
pub async fn passed(conn: &mut Conn, ids: &[i64]) -> Result<()> {
    for id in ids {
        sqlx::query("update failed set passed_at = ? where id = ?")
            .bind(now())
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// What failed in a session since kelve last settled it: the stretch it
/// asks about as a turn ends, oldest first.
pub async fn stretch(conn: &mut Conn, session: &str) -> Result<Vec<Failed>> {
    Ok(sqlx::query_as(failed!(
        "where session = ? and settled_at is null order by id"
    ))
    .bind(session)
    .fetch_all(&mut *conn)
    .await?)
}

/// Settles a stretch: asked about, or let go.
pub async fn settle(conn: &mut Conn, ids: &[i64], asked: bool) -> Result<()> {
    let at = now();
    for id in ids {
        sqlx::query("update failed set settled_at = ?, asked = ? where id = ?")
            .bind(at)
            .bind(asked)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// How many stretches kelve asked about in a session.
pub async fn stretches_asked(conn: &mut Conn, session: &str) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "select count(distinct settled_at) from failed where session = ? and asked = 1",
    )
    .bind(session)
    .fetch_one(&mut *conn)
    .await?)
}

/// A command passed in a session, now: after what failed before it, even
/// in the same millisecond, so the turn's end can tell it got past them.
pub async fn passed_now(conn: &mut Conn, session: &str) -> Result<()> {
    sqlx::query(
        "insert into moments (session, at, passed_at) values (?1, ?2, max(?2, coalesce(
             (select max(failed_at) + 1 from failed where session = ?1), 0)))
         on conflict (session) do update set passed_at = excluded.passed_at",
    )
    .bind(session)
    .bind(now())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// When a command last passed in a session.
pub async fn last_passed(conn: &mut Conn, session: &str) -> Result<Option<i64>> {
    Ok(
        sqlx::query_scalar("select passed_at from moments where session = ?")
            .bind(session)
            .fetch_optional(&mut *conn)
            .await?
            .flatten(),
    )
}

/// Some of what failed, by id.
pub async fn failed_by_id(conn: &mut Conn, ids: &[i64]) -> Result<Vec<Failed>> {
    let mut out = Vec::new();
    for id in ids {
        if let Some(f) = sqlx::query_as(failed!("where id = ?"))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?
        {
            out.push(f);
        }
    }
    Ok(out)
}

/// Something the agent learned, held here before it leaves.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Learned {
    pub id: i64,
    pub lesson: String,
    pub names: String,
    pub state: String,
    pub leaves_at: i64,
    pub root: String,
    pub graph_id: Option<String>,
    pub same_as: Option<String>,
    pub why: Option<String>,
    pub written_at: i64,
    /// From replaced work: its first way and its second, and the file.
    pub first: Option<String>,
    pub second: Option<String>,
    pub path: Option<String>,
}

/// Keeps what the agent learned, waiting until `leaves_at`: its id.
pub async fn learn(
    conn: &mut Conn,
    lesson: &str,
    names: &str,
    root: &str,
    leaves_at: i64,
    from: Option<&Replaced>,
) -> Result<i64> {
    let done = sqlx::query(
        "insert into learned (lesson, names, state, leaves_at, root, written_at, first, second, path)
         values (?, ?, 'waiting', ?, ?, ?, ?, ?, ?)",
    )
    .bind(lesson)
    .bind(names)
    .bind(leaves_at)
    .bind(root)
    .bind(now())
    .bind(from.map(|r| r.first.as_str()))
    .bind(from.map(|r| r.second.as_str()))
    .bind(from.map(|r| r.path.as_str()))
    .execute(&mut *conn)
    .await?;
    Ok(done.last_insert_rowid())
}

/// A query for what was learned, with what follows `from learned`.
macro_rules! learned {
    ($rest:literal) => {
        concat!(
            "select id, lesson, names, state, leaves_at, root, graph_id, same_as, why, written_at, first, second, path from learned ",
            $rest
        )
    };
}

/// Everything the agent learned, newest first, at most `limit`.
pub async fn learned(conn: &mut Conn, limit: i64) -> Result<Vec<Learned>> {
    Ok(sqlx::query_as(learned!("order by id desc limit ?"))
        .bind(limit)
        .fetch_all(&mut *conn)
        .await?)
}

/// One thing the agent learned.
pub async fn learned_one(conn: &mut Conn, id: i64) -> Result<Option<Learned>> {
    Ok(sqlx::query_as(learned!("where id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?)
}

/// What's waiting to leave and is due, oldest first.
pub async fn due(conn: &mut Conn) -> Result<Vec<Learned>> {
    Ok(sqlx::query_as(learned!(
        "where state = 'waiting' and leaves_at <= ? order by id"
    ))
    .bind(now())
    .fetch_all(&mut *conn)
    .await?)
}

/// Stops what's waiting to leave: true when it was waiting.
pub async fn stop(conn: &mut Conn, id: i64) -> Result<bool> {
    let done =
        sqlx::query("update learned set state = 'stopped' where id = ? and state = 'waiting'")
            .bind(id)
            .execute(&mut *conn)
            .await?;
    Ok(done.rows_affected() == 1)
}

/// Lets what's waiting leave now: true when it was waiting.
pub async fn send_now(conn: &mut Conn, id: i64) -> Result<bool> {
    let done = sqlx::query("update learned set leaves_at = ? where id = ? and state = 'waiting'")
        .bind(now())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(done.rows_affected() == 1)
}

/// How what the agent learned stands in the graph, once it's left.
pub async fn left(
    conn: &mut Conn,
    id: i64,
    state: &str,
    graph_id: Option<&str>,
    same_as: Option<&str>,
    why: Option<&str>,
) -> Result<()> {
    sqlx::query("update learned set state = ?, graph_id = ?, same_as = ?, why = ? where id = ?")
        .bind(state)
        .bind(graph_id)
        .bind(same_as)
        .bind(why)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    #[tokio::test]
    async fn lessons_are_kept_by_idea_and_replaced_when_fetched_again() {
        let pool = crate::memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let ideas = vec!["kelve:i1".to_string(), "kelve:i2".to_string()];
        assert_eq!(stale(&mut conn, &ideas, 0).await.unwrap(), ideas);
        keep_fetched(
            &mut conn,
            &ideas,
            &[
                (
                    "kelve:l1".into(),
                    vec!["kelve:i1".into()],
                    "{\"a\":1}".into(),
                ),
                (
                    "kelve:l2".into(),
                    vec!["kelve:i1".into(), "kelve:i2".into()],
                    "{}".into(),
                ),
            ],
        )
        .await
        .unwrap();
        assert!(stale(&mut conn, &ideas, 0).await.unwrap().is_empty());
        let got = about(&mut conn, &ideas).await.unwrap();
        assert_eq!(got.len(), 2, "each once");
        keep_fetched(&mut conn, &ideas[..1], &[]).await.unwrap();
        let got = about(&mut conn, &ideas).await.unwrap();
        assert_eq!(
            got.iter().map(|g| g.0.as_str()).collect::<Vec<_>>(),
            ["kelve:l2"]
        );
        assert_eq!(
            page(&mut conn, "kelve:l1").await.unwrap().as_deref(),
            Some("{\"a\":1}")
        );
    }

    #[tokio::test]
    async fn what_showed_a_lesson_held_isnt_replaced_by_less() {
        let pool = crate::memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let id = give(
            &mut conn,
            &Give {
                lesson: "kelve:l1",
                session: "s",
                moment: "failed",
                root: "/r",
                ..Give::default()
            },
        )
        .await
        .unwrap();
        assert!(
            came_of(&mut conn, id, "held", "passed", 3, "")
                .await
                .unwrap()
        );
        assert!(
            !came_of(&mut conn, id, "didnt", "said", 1, "no")
                .await
                .unwrap()
        );
        let g = &given_in(&mut conn, "s").await.unwrap()[0];
        assert_eq!((g.outcome.as_str(), g.shown.as_str()), ("held", "passed"));
        let unsent = outcomes_unsent(&mut conn, 10).await.unwrap();
        assert_eq!(unsent.len(), 1);
        outcomes_sent(&mut conn, &[id]).await.unwrap();
        assert!(outcomes_unsent(&mut conn, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn what_was_learned_waits_and_can_be_stopped() {
        let pool = crate::memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let later = learn(&mut conn, "{}", "[]", "/r", now() + 60_000, None)
            .await
            .unwrap();
        let soon = learn(&mut conn, "{}", "[]", "/r", now() - 1, None)
            .await
            .unwrap();
        assert_eq!(
            due(&mut conn)
                .await
                .unwrap()
                .iter()
                .map(|l| l.id)
                .collect::<Vec<_>>(),
            [soon]
        );
        assert!(stop(&mut conn, soon).await.unwrap());
        assert!(due(&mut conn).await.unwrap().is_empty());
        assert!(send_now(&mut conn, later).await.unwrap());
        assert_eq!(due(&mut conn).await.unwrap().len(), 1);
        left(
            &mut conn,
            later,
            "same",
            Some("kelve:l9"),
            Some("kelve:l1"),
            None,
        )
        .await
        .unwrap();
        let l = learned_one(&mut conn, later).await.unwrap().unwrap();
        assert_eq!(l.state, "same");
        let id = failed(&mut conn, "s", "/r", "cargo test", "error", "[]")
            .await
            .unwrap();
        assert_eq!(
            still_failing(&mut conn, "s", "cargo test")
                .await
                .unwrap()
                .len(),
            1
        );
        passed(&mut conn, &[id]).await.unwrap();
        assert!(
            still_failing(&mut conn, "s", "cargo test")
                .await
                .unwrap()
                .is_empty()
        );
    }
}
