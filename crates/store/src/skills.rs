//! The person's skills, and each one's parts, as kelve keeps them beside the
//! files (docs/skills.md): what kind each skill is, and what each part's
//! checks found. The files are the source of truth for the words; a part is
//! known by its normalised heading, so an edited heading is a new part.

use crate::{Conn, Result, now};

/// A skill, as kelve keeps it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Skill {
    pub id: i64,
    /// Its folder name.
    pub name: String,
    /// `like`, `do` or `project`.
    pub kind: String,
    /// For a `like` skill, the kind of work it's for; '' for the others.
    pub work: String,
    /// The project it's in, for a `project` skill; '' for the person's own.
    pub root: String,
    /// `made`, `suggested` or `not_now`.
    pub state: String,
    /// A suggestion's draft in full, and why it's suggested.
    pub draft: Option<String>,
    pub why: Option<String>,
    pub made_at: i64,
    pub updated_at: i64,
}

/// A skill to keep: one the person has, or one suggested.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewSkill<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub work: &'a str,
    pub root: &'a str,
    /// For a suggestion: its draft and why. None for a skill the person has.
    pub suggested: Option<(&'a str, &'a str)>,
}

const SKILL: &str =
    "select id, name, kind, work, root, state, draft, why, made_at, updated_at from skills";

/// Keeps a skill, by its folder and project: its id. A skill kept already
/// keeps its id, and is brought up to date. One the person has stays theirs
/// though it's suggested again.
pub async fn add(conn: &mut Conn, skill: &NewSkill<'_>) -> Result<i64> {
    let (state, draft, why) = match skill.suggested {
        Some((draft, why)) => ("suggested", Some(draft), Some(why)),
        None => ("made", None, None),
    };
    let at = now();
    Ok(sqlx::query_scalar(
        "insert into skills (name, kind, work, root, state, draft, why, made_at, updated_at)
         values (?, ?, ?, ?, ?, ?, ?, ?, ?)
         on conflict (root, name) do update set
           kind = excluded.kind, work = excluded.work, updated_at = excluded.updated_at,
           state = case when skills.state = 'made' then 'made' else excluded.state end,
           draft = case when skills.state = 'made' and excluded.state != 'made' then skills.draft else excluded.draft end,
           why = case when skills.state = 'made' and excluded.state != 'made' then skills.why else excluded.why end
         returning id",
    )
    .bind(skill.name)
    .bind(skill.kind)
    .bind(skill.work)
    .bind(skill.root)
    .bind(state)
    .bind(draft)
    .bind(why)
    .bind(at)
    .bind(at)
    .fetch_one(&mut *conn)
    .await?)
}

pub async fn get(conn: &mut Conn, id: i64) -> Result<Option<Skill>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{SKILL} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// A skill by its folder name, in a project or ('') the person's own.
pub async fn by_name(conn: &mut Conn, root: &str, name: &str) -> Result<Option<Skill>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SKILL} where root = ? and name = ?"
    )))
    .bind(root)
    .bind(name)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Every skill in a state, by name: `made` for the skills page's own
/// groups, `suggested` for its suggestions.
pub async fn all(conn: &mut Conn, state: &str) -> Result<Vec<Skill>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SKILL} where state = ? order by kind, name"
    )))
    .bind(state)
    .fetch_all(&mut *conn)
    .await?)
}

/// The person's `like` skill for a kind of work, when they have one.
pub async fn like_for(conn: &mut Conn, work: &str) -> Result<Option<Skill>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SKILL} where kind = 'like' and work = ? and root = '' and state = 'made'
          order by id limit 1"
    )))
    .bind(work)
    .fetch_optional(&mut *conn)
    .await?)
}

/// Makes a suggested skill the person's, or turns it down (`not_now`).
pub async fn set_state(conn: &mut Conn, id: i64, state: &str) -> Result<bool> {
    let done = sqlx::query(
        "update skills set state = ?, updated_at = ?,
           draft = case when ? = 'made' then null else draft end
         where id = ?",
    )
    .bind(state)
    .bind(now())
    .bind(state)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Forgets a skill, and with it its parts and what their checks found.
pub async fn forget(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query("delete from skills where id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A part of a skill, as kelve keeps it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Part {
    pub id: i64,
    pub skill: i64,
    /// Its heading, normalised: what it's known by.
    pub key: String,
    pub heading: String,
    /// `like`, `steps`, `watch`, or ''.
    pub section: String,
    pub body_hash: String,
    /// `unchecked`, `running`, `works`, `no_difference` or `failed`.
    pub state: String,
    pub checked_at: Option<i64>,
    pub note: Option<String>,
    pub wording: Option<String>,
    /// The person kept it though it made no difference.
    pub kept: bool,
    pub seen_at: i64,
    pub removed_at: Option<i64>,
    /// The hash of the text its check was last started for.
    pub tried: Option<String>,
}

/// A part as it's read from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenPart<'a> {
    pub key: &'a str,
    pub heading: &'a str,
    pub section: &'a str,
    pub body_hash: &'a str,
}

const PART: &str = "select id, skill, key, heading, section, body_hash, state, checked_at, note,
     wording, kept, seen_at, removed_at, tried from parts";

/// The parts of a skill as its file reads now: each kept, by its key, with
/// its id; one whose text changed is to be checked again, and one no
/// longer in the file is kept as removed, with what its checks found.
pub async fn seen(conn: &mut Conn, skill: i64, parts: &[SeenPart<'_>]) -> Result<Vec<i64>> {
    let at = now();
    let mut ids = Vec::with_capacity(parts.len());
    for p in parts {
        let id: i64 = sqlx::query_scalar(
            "insert into parts (skill, key, heading, section, body_hash, seen_at)
             values (?, ?, ?, ?, ?, ?)
             on conflict (skill, key) do update set
               heading = excluded.heading, section = excluded.section,
               seen_at = excluded.seen_at, removed_at = null,
               state = case when parts.body_hash != excluded.body_hash and parts.state != 'running'
                            then 'unchecked' else parts.state end,
               kept = case when parts.body_hash != excluded.body_hash then 0 else parts.kept end,
               body_hash = excluded.body_hash
             returning id",
        )
        .bind(skill)
        .bind(p.key)
        .bind(p.heading)
        .bind(p.section)
        .bind(p.body_hash)
        .bind(at)
        .fetch_one(&mut *conn)
        .await?;
        ids.push(id);
    }
    let keys: Vec<&str> = parts.iter().map(|p| p.key).collect();
    let gone: Vec<(i64, String)> =
        sqlx::query_as("select id, key from parts where skill = ? and removed_at is null")
            .bind(skill)
            .fetch_all(&mut *conn)
            .await?;
    for (id, key) in gone {
        if !keys.contains(&key.as_str()) {
            remove(conn, id).await?;
        }
    }
    Ok(ids)
}

/// A skill's parts, in the order they were first seen; those removed only
/// when asked.
pub async fn parts(conn: &mut Conn, skill: i64, removed_too: bool) -> Result<Vec<Part>> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{PART} where skill = ? and (? or removed_at is null) order by id"
    )))
    .bind(skill)
    .bind(removed_too)
    .fetch_all(&mut *conn)
    .await?)
}

pub async fn part(conn: &mut Conn, id: i64) -> Result<Option<Part>> {
    Ok(
        sqlx::query_as(sqlx::AssertSqlSafe(format!("{PART} where id = ?")))
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?,
    )
}

/// What a part's check found: its state, a line on what happened, and the
/// clearer wording the agent suggested, if it did.
pub async fn checked(
    conn: &mut Conn,
    id: i64,
    state: &str,
    note: Option<&str>,
    wording: Option<&str>,
) -> Result<()> {
    let at = (state != "running" && state != "unchecked").then(now);
    sqlx::query(
        "update parts set state = ?, checked_at = coalesce(?, checked_at),
           note = coalesce(?, note), wording = ?
         where id = ?",
    )
    .bind(state)
    .bind(at)
    .bind(note)
    .bind(wording)
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// A part's check started, for its text as it is: when checks run by
/// themselves, it isn't started again till the text changes.
pub async fn tried(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query("update parts set tried = body_hash where id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// The person keeps a part that made no difference.
pub async fn keep_anyway(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query("update parts set kept = 1 where id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A part gone from its skill.
pub async fn remove(conn: &mut Conn, id: i64) -> Result<()> {
    sqlx::query("update parts set removed_at = ? where id = ? and removed_at is null")
        .bind(now())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::memory;

    fn seen_part<'a>(key: &'a str, hash: &'a str) -> SeenPart<'a> {
        SeenPart {
            key,
            heading: key,
            section: "like",
            body_hash: hash,
        }
    }

    #[tokio::test]
    async fn a_skill_is_kept_once_and_a_suggestion_never_takes_one_over() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let rust = NewSkill {
            name: "rust",
            kind: "like",
            work: "rust",
            ..NewSkill::default()
        };
        let id = add(&mut conn, &rust).await.expect("kept");
        assert_eq!(add(&mut conn, &rust).await.expect("again"), id);
        let suggested = NewSkill {
            suggested: Some(("---\nname: rust\n---", "It came up twice.")),
            ..rust.clone()
        };
        assert_eq!(add(&mut conn, &suggested).await.expect("suggested"), id);
        let s = get(&mut conn, id).await.expect("read").expect("there");
        assert_eq!((s.state.as_str(), s.draft), ("made", None));
        assert_eq!(
            like_for(&mut conn, "rust")
                .await
                .expect("read")
                .map(|s| s.id),
            Some(id)
        );

        let retry = NewSkill {
            name: "retry-network-calls",
            kind: "do",
            suggested: Some(("---\nname: retry-network-calls\n---", "Twice now.")),
            ..NewSkill::default()
        };
        let other = add(&mut conn, &retry).await.expect("suggested");
        let listed: Vec<i64> = all(&mut conn, "suggested")
            .await
            .expect("listed")
            .iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(listed, vec![other]);
        assert!(set_state(&mut conn, other, "made").await.expect("made"));
        let made = get(&mut conn, other).await.expect("read").expect("there");
        assert_eq!((made.state.as_str(), made.draft), ("made", None));
        assert_eq!(all(&mut conn, "made").await.expect("listed").len(), 2);
        assert!(
            by_name(&mut conn, "", "rust")
                .await
                .expect("read")
                .is_some()
        );
        assert!(
            by_name(&mut conn, "/w", "rust")
                .await
                .expect("read")
                .is_none(),
            "a project's skills are its own"
        );
        forget(&mut conn, other).await.expect("forgotten");
        assert!(get(&mut conn, other).await.expect("read").is_none());
    }

    #[tokio::test]
    async fn parts_are_known_by_heading_and_checked_again_when_changed() {
        let pool = memory().await.expect("a store");
        let mut conn = pool.acquire().await.expect("a connection");
        let skill = add(
            &mut conn,
            &NewSkill {
                name: "rust",
                kind: "like",
                work: "rust",
                ..NewSkill::default()
            },
        )
        .await
        .expect("kept");
        let ids = seen(
            &mut conn,
            skill,
            &[
                seen_part("errors are one enum", "a"),
                seen_part("exit 1", "b"),
            ],
        )
        .await
        .expect("seen");
        checked(&mut conn, ids[0], "works", Some("Changed 3 of 4."), None)
            .await
            .expect("checked");
        checked(
            &mut conn,
            ids[1],
            "no_difference",
            Some("It did it anyway."),
            None,
        )
        .await
        .expect("checked");
        keep_anyway(&mut conn, ids[1]).await.expect("kept");

        // The first unchanged, the second reworded, a third added.
        let again = seen(
            &mut conn,
            skill,
            &[
                seen_part("errors are one enum", "a"),
                seen_part("exit 1", "c"),
                seen_part("fix clippy warnings", "d"),
            ],
        )
        .await
        .expect("seen");
        assert_eq!(&again[..2], &ids[..]);
        let first = part(&mut conn, ids[0]).await.expect("read").expect("there");
        assert_eq!(first.state, "works");
        assert!(first.checked_at.is_some());
        let second = part(&mut conn, ids[1]).await.expect("read").expect("there");
        assert_eq!((second.state.as_str(), second.kept), ("unchecked", false));
        assert_eq!(second.note.as_deref(), Some("It did it anyway."));

        // One taken out of the file.
        seen(
            &mut conn,
            skill,
            &[
                seen_part("errors are one enum", "a"),
                seen_part("exit 1", "c"),
            ],
        )
        .await
        .expect("seen");
        assert_eq!(parts(&mut conn, skill, false).await.expect("read").len(), 2);
        let all = parts(&mut conn, skill, true).await.expect("read");
        assert_eq!(all.len(), 3);
        assert!(all[2].removed_at.is_some());
        // Back in the file: a part again, not a new one.
        let back = seen(&mut conn, skill, &[seen_part("fix clippy warnings", "d")])
            .await
            .expect("seen");
        assert_eq!(back, vec![all[2].id]);
        assert_eq!(parts(&mut conn, skill, false).await.expect("read").len(), 1);

        forget(&mut conn, skill).await.expect("forgotten");
        assert!(part(&mut conn, ids[0]).await.expect("read").is_none());
    }
}
