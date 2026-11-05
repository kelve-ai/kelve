//! What's open on the server: its files and changes, read from git, and the
//! notes beside them. The routes are `api::reader`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use chrono::Utc;
use kelve_core::notes::{Front, Passage};
use kelve_core::views::{
    LinesIn, Note, NoteIn, NoteOn, NoteRef, NoteState, OpenKind, Opened, Place, ServerEvent,
};
use kelve_engine::git::{Change, Stamp, Which};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

/// Something opened, and the folder it's in.
#[derive(Debug, Clone)]
pub struct Open {
    pub row: kelve_store::reader::Opened,
    pub root: PathBuf,
}

impl Open {
    pub fn kind(&self) -> OpenKind {
        match self.row.kind.as_str() {
            "file" => OpenKind::File,
            "change" => OpenKind::Change,
            _ => OpenKind::Folder,
        }
    }

    pub fn view(&self) -> Opened {
        Opened {
            id: self.row.id,
            kind: self.kind(),
            root: shown(&self.root),
            path: (!self.row.path.is_empty()).then(|| self.row.path.clone()),
            change: (!self.row.change.is_empty()).then(|| self.row.change.clone()),
            title: self.row.title.clone(),
            opened_at: kelve_core::Timestamp(kelve_store::at(self.row.opened_at)),
            said: self.row.said.clone(),
            agent: (!self.row.agent.is_empty()).then(|| self.row.agent.clone()),
            only: self.only(),
            debrief: None,
            handover: None,
        }
    }

    /// When only some files of the folder were opened: which.
    pub fn only(&self) -> Option<Vec<String>> {
        self.row
            .only
            .as_deref()
            .and_then(|o| serde_json::from_str(o).ok())
    }

    /// Which change it is, if it's one.
    pub fn which(&self) -> Option<Which> {
        which(&self.row.change)
    }
}

/// A folder as the person would write it: `~/projects/kelve`.
pub fn shown(path: &Path) -> String {
    let text = path.display().to_string();
    match std::env::home_dir().map(|h| h.display().to_string()) {
        Some(home) if !home.is_empty() && text.starts_with(&home) => {
            format!("~{}", &text[home.len()..])
        }
        _ => text,
    }
}

/// A path as the person wrote it, `~/projects/kelve` too, as a path.
pub fn unshown(text: &str) -> PathBuf {
    match (
        text.strip_prefix('~'),
        std::env::home_dir().map(|h| h.display().to_string()),
    ) {
        (Some(rest), Some(home))
            if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) =>
        {
            PathBuf::from(format!("{home}{rest}"))
        }
        _ => PathBuf::from(text),
    }
}

/// A change by its words: `uncommitted`, `against main`, `since main`,
/// `commit 3f2a9c1`.
pub fn which(words: &str) -> Option<Which> {
    let words = words.trim();
    if words == "uncommitted" {
        return Some(Which::Uncommitted);
    }
    if let Some(base) = words.strip_prefix("since ") {
        return Some(Which::Since(base.trim().to_string()));
    }
    if let Some(base) = words.strip_prefix("against ") {
        return Some(Which::Against(base.trim().to_string()));
    }
    words
        .strip_prefix("commit ")
        .map(|rev| Which::Commit(rev.trim().to_string()))
}

/// Something opened, by its id.
pub async fn opened(app: &Shared, id: i64) -> ApiResult<Open> {
    let mut conn = app.db.acquire().await?;
    let row = kelve_store::reader::get(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("That isn't open in kelve any more. Open it again."))?;
    let root = PathBuf::from(&row.root);
    if !root.is_dir() {
        return Err(ApiError::not_found(format!(
            "{} isn't there any more.",
            shown(&root)
        )));
    }
    Ok(Open { row, root })
}

/// Whether a path is kelve's own, in a project: its notes, shown in the
/// notes window rather than among the files.
pub fn is_kelves(rel: &str) -> bool {
    rel == ".kelve" || rel.starts_with(".kelve/")
}

/// A change as last read, and how to tell it still holds.
type Kept = Arc<Mutex<Option<(Stamp, Arc<Change>)>>>;

/// The changes lately read, by folder and which change, the latest last:
/// each is read again only once its repository says it's moved on.
static CHANGES: LazyLock<Mutex<Vec<(String, Kept)>>> = LazyLock::new(Default::default);

/// How many changes are kept read.
const KEEP_CHANGES: usize = 8;

/// A change, read from git's own files: as read before, while nothing it
/// was read from has moved on.
pub fn change(open: &Open) -> ApiResult<Arc<Change>> {
    let which = open
        .which()
        .ok_or_else(|| ApiError::bad_request("That isn't a change kelve can read."))?;
    let key = format!("{}\n{}", open.root.display(), open.row.change);
    let slot = {
        let mut all = CHANGES.lock().unwrap_or_else(|p| p.into_inner());
        let slot = match all.iter().position(|(k, _)| *k == key) {
            Some(at) => all.remove(at).1,
            None => Arc::default(),
        };
        all.push((key, slot.clone()));
        let over = all.len().saturating_sub(KEEP_CHANGES);
        all.drain(..over);
        slot
    };
    // One reading at a time for each change: asked for at once, it's read once.
    let mut kept = slot.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((stamp, change)) = kept.as_ref()
        && stamp.holds()
    {
        return Ok(change.clone());
    }
    // kelve's own folder, its notes and skills, is kept beside the work,
    // not part of it.
    let (change, stamp) = kelve_engine::git::change_stamped(&open.root, &which, &is_kelves)
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let change = Arc::new(change);
    *kept = Some((stamp, change.clone()));
    Ok(change)
}

// ---------------------------------------------------------------- notes

/// Where the notes for a place are kept, for a project: with the work in
/// the project; the person's own, and those for their agent, in their own
/// notes folder.
fn notes_dir(app: &Shared, open: &Open, place: Place) -> PathBuf {
    match place {
        Place::Work => open.root.join(".kelve").join("notes"),
        Place::You | Place::Agent => own_folder(&own_notes(app), &open.row.root),
    }
}

/// Where notes kept only for the person go: the folder they chose, or
/// kelve's own.
pub fn own_notes(app: &Shared) -> PathBuf {
    own_notes_in(app, &app.settings())
}

/// The same, with these settings.
pub fn own_notes_in(app: &Shared, settings: &crate::settings::Settings) -> PathBuf {
    settings
        .notes_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| app.config.data.join("notes"))
}

/// The file in a project's folder of your own notes that says which
/// project it's for, so folders can move, and two projects of one name
/// stay apart.
const PROJECT_MARK: &str = ".kelve-project";

/// The folder in `base` for a project's own notes: named as the project
/// is, then, when another project has that name, with the folder above it,
/// then a short code. One already the project's, or free, is it.
fn own_folder(base: &Path, root: &str) -> PathBuf {
    let path = Path::new(root);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "notes".into());
    let above = path
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let code: String = Sha256::digest(root.as_bytes())
        .iter()
        .take(3)
        .map(|b| format!("{b:02x}"))
        .collect();
    let names = [
        name.clone(),
        format!("{name} ({above})"),
        format!("{name} ({code})"),
    ];
    for n in &names {
        let dir = base.join(n);
        match std::fs::read_to_string(dir.join(PROJECT_MARK)) {
            Ok(said) if said.trim() == root => return dir,
            Ok(_) => continue,
            Err(_) if dir.exists() => continue,
            Err(_) => return dir,
        }
    }
    base.join(&names[2])
}

/// Moves every project's folder of notes kept only for the person from one
/// folder to another, into the folder each project has there.
pub fn move_own_notes(from: &Path, to: &Path) -> std::io::Result<()> {
    let Ok(projects) = std::fs::read_dir(from) else {
        return Ok(());
    };
    std::fs::create_dir_all(to)?;
    for project in projects.filter_map(|e| e.ok()) {
        let dir = project.path();
        let Ok(root) = std::fs::read_to_string(dir.join(PROJECT_MARK)) else {
            continue;
        };
        let target = own_folder(to, root.trim());
        std::fs::create_dir_all(&target)?;
        for file in std::fs::read_dir(&dir)?.filter_map(|e| e.ok()) {
            let name = file.file_name().to_string_lossy().into_owned();
            let mut at = target.join(&name);
            // A note of the same name there already: kept beside it.
            if at.exists() && name != PROJECT_MARK {
                let stem = name.strip_suffix(".md").unwrap_or(&name).to_string();
                let mut n = 2;
                while at.exists() {
                    at = target.join(format!("{stem}-{n}.md"));
                    n += 1;
                }
            }
            if std::fs::rename(file.path(), &at).is_err() {
                // Across disks: copied, then let go.
                std::fs::copy(file.path(), &at)?;
                std::fs::remove_file(file.path())?;
            }
        }
        std::fs::remove_dir(&dir).ok();
    }
    Ok(())
}

/// A file name for a note: lower-case words and hyphens.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 60 {
            break;
        }
    }
    out.trim_end_matches('-').to_string()
}

/// A note's title: its first line with words, without its markdown marks,
/// or its file's name.
fn title_of(text: &str, file: &str) -> String {
    text.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .map(|t| {
            let t: String = t.chars().take(80).collect();
            t.replace(['*', '`', '_'], "")
        })
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| file.trim_end_matches(".md").replace('-', " "))
}

fn place_word(place: Place) -> &'static str {
    match place {
        Place::You => "you",
        Place::Work => "work",
        Place::Agent => "agent",
    }
}

/// The file a note's id names, if it's a note kelve keeps: `work/ledger.md`.
fn note_path(app: &Shared, open: &Open, id: &str) -> Option<(Place, PathBuf, String)> {
    let (place, file) = id.split_once('/')?;
    let place = match place {
        "you" => Place::You,
        "work" => Place::Work,
        "agent" => Place::Agent,
        _ => return None,
    };
    let stem = file.strip_suffix(".md")?;
    if stem.is_empty() || slug(stem) != stem {
        return None;
    }
    Some((
        place,
        notes_dir(app, open, place).join(file),
        file.to_string(),
    ))
}

/// A note, by where it's kept, from its file's text: its passage found
/// again in the file as it is now, when `root` is the project it's in.
fn note_ref(root: Option<&Path>, place: Place, file: &str, path: &Path, text: &str) -> NoteRef {
    let (front, body) = kelve_core::notes::split(text);
    let on = front.passage.as_ref().and_then(|p| {
        let (start, end) = p.range()?;
        let now = root
            .and_then(|root| kelve_engine::read::read(root, &p.path).ok())
            .and_then(|read| p.find(&read.text.lines().collect::<Vec<_>>()));
        Some(NoteOn {
            path: p.path.clone(),
            start: now.map_or(start, |n| n.0),
            end: now.map_or(end, |n| n.1),
            found: now.is_some(),
        })
    });
    // An empty note, as one cleared for fresh notes, waits for nothing.
    // Done for other words than it has now, it waits again.
    let state = (front.for_agent && (front.done || !body.trim().is_empty())).then_some(
        if front.waiting(body) {
            NoteState::Waiting
        } else {
            NoteState::Done
        },
    );
    NoteRef {
        id: format!("{}/{file}", place_word(place)),
        title: title_of(body, file),
        place,
        file: shown(path),
        ideas: front.ideas,
        on,
        state,
        did: front.did.unwrap_or_default(),
        done_at: front
            .done_at
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(&at).ok())
            .map(|at| at.with_timezone(&Utc).into()),
        opened: front.opened,
    }
}

/// The notes in a folder, each with the place its front matter says: one
/// for the agent in the person's own folder is the agent's.
fn notes_in(root: Option<&Path>, dir: &Path, own: bool) -> Vec<(NoteRef, String)> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(String, PathBuf)> = read
        .filter_map(|e| e.ok())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .filter(|(name, path)| name.ends_with(".md") && path.is_file())
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|(name, path)| {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let place = match (own, kelve_core::notes::split(&text).0.for_agent) {
                (false, _) => Place::Work,
                (true, true) => Place::Agent,
                (true, false) => Place::You,
            };
            (note_ref(root, place, &name, &path, &text), text)
        })
        .collect()
}

/// The notes for what's open: for the agent, with the work, and the
/// person's own.
pub fn notes(app: &Shared, open: &Open) -> Vec<NoteRef> {
    let mut out: Vec<NoteRef> =
        notes_in(Some(&open.root), &notes_dir(app, open, Place::Work), false)
            .into_iter()
            .chain(notes_in(
                Some(&open.root),
                &notes_dir(app, open, Place::You),
                true,
            ))
            .map(|(n, _)| n)
            .collect();
    let rank = |p: Place| match p {
        Place::Agent => 0,
        Place::Work => 1,
        Place::You => 2,
    };
    out.sort_by_key(|n| rank(n.place));
    out
}

pub fn note(app: &Shared, open: &Open, id: &str) -> ApiResult<Note> {
    let (place, path, file) =
        note_path(app, open, id).ok_or_else(|| ApiError::not_found("There's no such note."))?;
    let text = std::fs::read_to_string(&path)
        .map_err(|_| ApiError::not_found("There's no such note any more."))?;
    Ok(Note {
        note: note_ref(Some(&open.root), place, &file, &path, &text),
        text: kelve_core::notes::split(&text).1.to_string(),
    })
}

/// Deletes a note.
pub fn forget_note(app: &Shared, open: &Open, id: &str) -> ApiResult<()> {
    let (_, path, _) =
        note_path(app, open, id).ok_or_else(|| ApiError::not_found("There's no such note."))?;
    std::fs::remove_file(&path)
        .map_err(|_| ApiError::not_found("There's no such note any more."))?;
    app.publish(ServerEvent::Notes);
    Ok(())
}

/// Whether `path` is `folder` or in it, as the system tells folders apart:
/// on Windows, in any case, by either separator, and with or without the
/// `\\?\` a whole path may start with.
pub(crate) fn within(path: &Path, folder: &Path) -> bool {
    if !cfg!(windows) {
        return path.starts_with(folder);
    }
    let plain = |p: &Path| {
        let p = p.to_string_lossy().replace('\\', "/").to_lowercase();
        let p = p.strip_prefix("//?/").unwrap_or(&p);
        p.trim_end_matches('/').to_string()
    };
    let (path, folder) = (plain(path), plain(folder));
    path == folder || path.starts_with(&format!("{folder}/"))
}

/// The project a folder is in, among those with notes in the person's
/// folder: its root, and its folder of notes. The longest root the folder
/// is in wins.
fn own_project_for(app: &Shared, folder: &Path) -> Option<(PathBuf, PathBuf)> {
    let projects = std::fs::read_dir(own_notes(app)).ok()?;
    projects
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let root = std::fs::read_to_string(e.path().join(PROJECT_MARK)).ok()?;
            let root = PathBuf::from(root.trim());
            // As Windows tells folders apart too: an agent's `C:\…` is
            // the `\\?\C:\…` kelve keeps.
            within(folder, &root).then(|| (root, e.path()))
        })
        .max_by_key(|(root, _)| root.as_os_str().len())
}

/// A note left for the agent, as it reads it: fenced, as material.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForAgent {
    pub id: String,
    /// Its first line, as the list shows it: never sent.
    #[serde(skip)]
    pub title: String,
    /// What the person asked, fenced: do it only as part of their work.
    pub note: String,
    /// The version of its words: given back with `note_done`, so a note
    /// changed since it was read isn't marked done.
    pub version: String,
    /// The file it's on, and its lines as they are now; as they were when
    /// it was left, when they've changed since.
    pub file: Option<String>,
    pub lines: Option<(u32, u32)>,
    pub text_now: Option<String>,
    /// Said when the lines it's on have changed since it was left.
    pub lines_changed: Option<&'static str>,
    /// The other notes waiting for the agent on those lines, by id.
    pub on_the_same_lines: Vec<String>,
    /// Done before, for other words: what it said then, and what the agent
    /// did, so it does only what's new.
    pub done_before: Option<DoneBefore>,
    /// The lines added since it was done.
    pub added: Vec<String>,
    /// Its words, with secrets out, as the job was asked for: never sent.
    #[serde(skip)]
    pub body: String,
    /// When the agent last said it was done, in milliseconds: never sent.
    #[serde(skip)]
    pub done_at: Option<i64>,
    /// When it last changed: never sent.
    #[serde(skip)]
    pub changed: Option<std::time::SystemTime>,
}

/// What a note said when the agent last did it, and what it did.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DoneBefore {
    /// Fenced, as the note is.
    pub text: String,
    pub did: String,
}

/// Said when the lines a note is on have changed since it was left.
pub const LINES_CHANGED: &str = "These lines have changed since the note was left.";

/// Whether two notes are on lines of the same file that overlap.
fn same_lines(a: &NoteOn, b: &NoteOn) -> bool {
    a.path == b.path && a.start <= b.end && b.start <= a.end
}

/// The notes waiting for the agent in the project a folder is in.
pub async fn waiting_for_agent(app: &Shared, folder: &Path) -> ApiResult<Vec<ForAgent>> {
    let Some((root, dir)) = own_project_for(app, folder) else {
        return Ok(Vec::new());
    };
    let all = notes_in(Some(&root), &dir, true);
    let mut out = Vec::new();
    for (n, text) in &all {
        if n.state != Some(NoteState::Waiting) {
            continue;
        }
        let (front, body) = kelve_core::notes::split(text);
        let redacted = kelve_core::redact::redact(body);
        let text_now = n.on.as_ref().filter(|on| on.found).and_then(|on| {
            let read = kelve_engine::read::read(&root, &on.path).ok()?;
            let lines: Vec<&str> = read.text.lines().collect();
            let (a, b) = (on.start as usize, on.end as usize);
            (a >= 1 && b <= lines.len()).then(|| lines[a - 1..b].join("\n"))
        });
        // Done before for other words: what it said then, and what's new.
        let before = front
            .done_for
            .as_deref()
            .filter(|_| front.did.is_some())
            .map(kelve_core::redact::redact);
        let added = before
            .as_deref()
            .map(|b| kelve_core::notes::added(b, &redacted))
            .unwrap_or_default();
        let done_before = before.map(|b| DoneBefore {
            text: kelve_core::fence::fence("note", &b),
            did: front.did.clone().unwrap_or_default(),
        });
        let on_the_same_lines =
            n.on.as_ref()
                .map(|on| {
                    all.iter()
                        .filter(|(o, _)| o.id != n.id && o.state == Some(NoteState::Waiting))
                        .filter(|(o, _)| o.on.as_ref().is_some_and(|x| same_lines(on, x)))
                        .map(|(o, _)| o.id.clone())
                        .collect()
                })
                .unwrap_or_default();
        out.push(ForAgent {
            id: n.id.clone(),
            title: n.title.clone(),
            note: kelve_core::fence::fence("note", &redacted),
            version: kelve_core::notes::version(body),
            file: n.on.as_ref().map(|on| on.path.clone()),
            lines: n.on.as_ref().map(|on| (on.start, on.end)),
            text_now: text_now.map(|t| kelve_core::fence::fence("lines", &t)),
            lines_changed: n
                .on
                .as_ref()
                .is_some_and(|on| !on.found)
                .then_some(LINES_CHANGED),
            on_the_same_lines,
            done_before,
            added,
            body: redacted.trim().to_string(),
            done_at: n.done_at.as_ref().map(|at| at.0.timestamp_millis()),
            changed: n
                .id
                .rsplit('/')
                .next()
                .and_then(|file| std::fs::metadata(dir.join(file)).ok())
                .and_then(|m| m.modified().ok()),
        });
    }
    Ok(out)
}

/// The notes waiting for an agent that it hasn't been told of as they are
/// now, by its hook: each is told of now.
pub async fn new_for_agent(app: &Shared, folder: &Path, agent: &str) -> ApiResult<usize> {
    let waiting = waiting_for_agent(app, folder).await?;
    let root = root_of_notes(app, folder).display().to_string();
    let mut conn = app.db.acquire().await?;
    let mut new = 0;
    for n in &waiting {
        if kelve_store::asks::tell(&mut conn, agent, &root, &n.id, &n.version).await? {
            new += 1;
        }
    }
    Ok(new)
}

/// The notes the agent did since a time, in the project a folder is in,
/// each with what it said it did.
pub fn done_for_agent(
    app: &Shared,
    folder: &Path,
    since: chrono::DateTime<Utc>,
) -> Vec<(NoteRef, String)> {
    let Some((root, dir)) = own_project_for(app, folder) else {
        return Vec::new();
    };
    notes_in(Some(&root), &dir, true)
        .into_iter()
        .filter(|(n, _)| {
            n.place == Place::Agent
                && n.state == Some(NoteState::Done)
                && n.done_at.as_ref().is_some_and(|at| at.0 > since)
        })
        .map(|(n, _)| {
            let did = n.did.clone();
            (n, did)
        })
        .collect()
}

/// What the agent opened, for the notes it said were done in the last
/// half hour that don't lead anywhere yet.
pub fn link_done_notes(app: &Shared, folder: &Path, opened: i64) {
    let Some((_, dir)) = own_project_for(app, folder) else {
        return;
    };
    let since = Utc::now() - chrono::Duration::minutes(30);
    for (n, text) in notes_in(None, &dir, true) {
        let recent = n.done_at.as_ref().is_some_and(|at| at.0 > since);
        if n.state != Some(NoteState::Done) || n.opened.is_some() || !recent {
            continue;
        }
        let (mut front, body) = kelve_core::notes::split(&text);
        front.opened = Some(opened);
        let file = n.id.trim_start_matches("agent/");
        if std::fs::write(dir.join(file), kelve_core::notes::join(&front, body)).is_err() {
            tracing::warn!("couldn't keep what the agent opened with a note it did");
        }
    }
    app.publish(ServerEvent::Notes);
}

/// Said when a note changed since the agent read it.
pub const NOTE_CHANGED: &str = "This note changed since you read it: read it again with notes.";

/// For which words the agent says a note is done.
#[derive(Debug, Clone, Copy)]
pub enum DoneFor<'a> {
    /// The version `notes` gave: refused when the note has changed since.
    Read(&'a str),
    /// A job's answer, for the version and words it was asked for: when the
    /// note has changed since, kept against those words, and the note
    /// waits for the agent again with what's new.
    Job { version: &'a str, text: &'a str },
}

/// The agent says a note is done, for the words it read, and what it did.
pub async fn note_done(
    app: &Shared,
    folder: &Path,
    id: &str,
    did: &str,
    opened: Option<i64>,
    done_for: DoneFor<'_>,
) -> ApiResult<()> {
    let (_, dir) = own_project_for(app, folder)
        .ok_or_else(|| ApiError::not_found("There are no notes for you in this project."))?;
    let file = id
        .strip_prefix("agent/")
        .filter(|f| {
            f.ends_with(".md") && slug(f.trim_end_matches(".md")) == f.trim_end_matches(".md")
        })
        .ok_or_else(|| {
            ApiError::not_found("There's no such note for you. Use the id the notes tool gave.")
        })?;
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path)
        .map_err(|_| ApiError::not_found("There's no such note any more."))?;
    let (mut front, body) = kelve_core::notes::split(&text);
    if !front.for_agent {
        return Err(ApiError::not_found(
            "There's no such note for you. Use the id the notes tool gave.",
        ));
    }
    let did = kelve_core::redact::redact(&did.split_whitespace().collect::<Vec<_>>().join(" "));
    if did.is_empty() || did.chars().count() > 300 {
        return Err(ApiError::bad_request(
            "Say what you did in one line, at most 300 characters.",
        ));
    }
    let now = kelve_core::notes::version(body);
    let (version, text) = match done_for {
        DoneFor::Read(v) if v.trim() != now => {
            return Err(ApiError::conflict(NOTE_CHANGED));
        }
        DoneFor::Read(_) => (now.clone(), kelve_core::redact::redact(body.trim())),
        // Done already as it is now: an answer for older words adds nothing.
        DoneFor::Job { version, .. } if version != now && front.done_now(body) => {
            return Ok(());
        }
        DoneFor::Job { version, text } => (version.to_string(), text.to_string()),
    };
    let current = version == now;
    front.done = current;
    front.done_hash = Some(version);
    front.done_for = Some(text);
    front.did = Some(did);
    front.done_at = Some(Utc::now().to_rfc3339());
    front.opened = opened;
    std::fs::write(&path, kelve_core::notes::join(&front, body)).map_err(ApiError::internal)?;
    // Done as it is now: its jobs, if they're still on the list, come off,
    // before it's said to be done: come off later, they could take the job
    // asked when its words change back. Done for older words, the job for
    // these stays.
    if current {
        let prefix = crate::list::note_prefix(&root_of_notes(app, folder), id);
        if let Ok(mut conn) = app.db.acquire().await {
            let _ = kelve_store::asks::drop_like(&mut conn, &prefix, None, true).await;
        }
        app.publish(ServerEvent::Asks);
    }
    app.publish(ServerEvent::Notes);
    Ok(())
}

/// The project a folder is in, as its notes go by.
pub(crate) fn root_of_notes(app: &Shared, folder: &Path) -> PathBuf {
    own_project_for(app, folder).map_or_else(|| folder.to_path_buf(), |(root, _)| root)
}

/// What a new note keeps in its front matter: the passage it's on, read
/// from the file now, and the ideas it's about.
fn front_for(open: &Open, on: Option<&LinesIn>, ideas: &[String]) -> ApiResult<Front> {
    let passage = match on {
        None => None,
        Some(on) => {
            let read = kelve_engine::read::read(&open.root, &on.path)
                .map_err(|_| ApiError::bad_request("kelve can't read the file that note is on."))?;
            let lines: Vec<&str> = read.text.lines().collect();
            Some(
                Passage::of(&on.path, &lines, on.start, on.end).ok_or_else(|| {
                    ApiError::bad_request("Those lines aren't in the file. Choose them again.")
                })?,
            )
        }
    };
    Ok(Front {
        ideas: ideas.iter().take(20).cloned().collect(),
        passage,
        ..Front::default()
    })
}

/// A free file name in `dir` for a note with this text.
fn free_name(dir: &Path, text: &str) -> String {
    let base = match slug(&title_of(text, "")) {
        s if s.is_empty() => "note".to_string(),
        s => s,
    };
    let mut name = format!("{base}.md");
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{base}-{n}.md");
        n += 1;
    }
    name
}

/// Keeps a note: a new one under a name from its title, with the passage
/// and ideas it's about, or one there is, keeping what its front matter
/// says. A note given a new place moves there. `text` is the note's body.
pub fn keep_note(app: &Shared, open: &Open, note: &NoteIn) -> ApiResult<Note> {
    let (id, place, text) = (note.id.as_deref(), note.place, note.text.as_str());
    if text.len() > 1_000_000 {
        return Err(ApiError::bad_request(
            "That note is too long to keep. Split it into two.",
        ));
    }
    let was = match id {
        Some(id) => Some(
            note_path(app, open, id).ok_or_else(|| ApiError::not_found("There's no such note."))?,
        ),
        None => None,
    };
    let mut front = match &was {
        Some((_, path, _)) => {
            let kept = std::fs::read_to_string(path).unwrap_or_default();
            let (mut front, body) = kelve_core::notes::split(&kept);
            if front.for_agent {
                // Done before kelve kept for which words: for these.
                if front.done && front.done_hash.is_none() {
                    front.done_hash = Some(kelve_core::notes::version(body));
                    front.done_for = Some(kelve_core::redact::redact(body.trim()));
                }
                if text.trim().is_empty() && !body.trim().is_empty() {
                    // Cleared for fresh notes: what was done goes.
                    front.forget_done();
                } else {
                    // Written in again, it waits for the agent, with what
                    // it did before kept, so it does only what's new;
                    // written back as it was done, it's done again.
                    front.done = front.done_now(text);
                }
            }
            front
        }
        None => front_for(
            open,
            note.on.as_ref(),
            note.ideas.as_deref().unwrap_or_default(),
        )?,
    };
    // For the agent, it waits for it again; not for it, it's the person's.
    if (place == Place::Agent) != front.for_agent {
        front.for_agent = place == Place::Agent;
        front.forget_done();
    }
    let dir = notes_dir(app, open, place);
    let (path, file) = match &was {
        Some((_, path, file)) if path.parent() == Some(dir.as_path()) => {
            (path.clone(), file.clone())
        }
        _ => {
            let file = free_name(&dir, text);
            (dir.join(&file), file)
        }
    };
    let whole = kelve_core::notes::join(&front, text);
    std::fs::create_dir_all(&dir).map_err(ApiError::internal)?;
    // A project's folder of your own notes says which project it's for.
    if place != Place::Work && !dir.join(PROJECT_MARK).exists() {
        std::fs::write(dir.join(PROJECT_MARK), &open.row.root).map_err(ApiError::internal)?;
    }
    std::fs::write(&path, &whole).map_err(ApiError::internal)?;
    // Moved: the old file goes.
    if let Some((_, old, _)) = &was
        && old != &path
    {
        std::fs::remove_file(old).ok();
    }
    app.publish(ServerEvent::Notes);
    // Once it's been left alone, it's on the agent's list: the reader's
    // told then.
    if place == Place::Agent {
        let app = app.clone();
        tokio::spawn(async move {
            tokio::time::sleep(crate::list::SETTLE + std::time::Duration::from_secs(1)).await;
            app.publish(ServerEvent::Notes);
        });
    }
    Ok(Note {
        note: note_ref(Some(&open.root), place, &file, &path, &whole),
        text: text.to_string(),
    })
}

/// A note left for the agent, made the person's own: it was taken off the
/// agent's list, so the agent isn't told of it. Its words, the lines it's
/// on and its ideas stay.
pub async fn note_to_you(app: &Shared, opened_id: i64, id: &str) -> ApiResult<()> {
    let open = opened(app, opened_id).await?;
    let Some((Place::Agent, path, _)) = note_path(app, &open, id) else {
        return Ok(());
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let body = kelve_core::notes::split(&text).1.to_string();
    keep_note(
        app,
        &open,
        &NoteIn {
            id: Some(id.to_string()),
            place: Place::You,
            text: body,
            on: None,
            ideas: None,
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_are_read_by_their_words() {
        assert_eq!(which("uncommitted"), Some(Which::Uncommitted));
        assert_eq!(which("against main"), Some(Which::Against("main".into())));
        assert_eq!(
            which("commit 3f2a9c1"),
            Some(Which::Commit("3f2a9c1".into()))
        );
        assert_eq!(which("anything else"), None);
    }

    #[test]
    fn note_names_are_plain() {
        assert_eq!(
            slug("Spending ledger: what to check!"),
            "spending-ledger-what-to-check"
        );
        assert_eq!(
            title_of("# Spending ledger\n\nText", "x.md"),
            "Spending ledger"
        );
        assert_eq!(
            title_of("Make the **cap** a setting.\nMore", "x.md"),
            "Make the cap a setting."
        );
        assert_eq!(title_of("\n\n", "rate-limits.md"), "rate limits");
    }

    #[test]
    fn paths_are_read_as_people_write_them() {
        assert_eq!(unshown("/srv/kelve"), PathBuf::from("/srv/kelve"));
        assert_eq!(unshown("~someone/kelve"), PathBuf::from("~someone/kelve"));
        if let Some(home) = std::env::home_dir().map(|h| h.display().to_string())
            && !home.is_empty()
        {
            assert_eq!(
                unshown("~/projects/kelve"),
                PathBuf::from(format!("{home}/projects/kelve"))
            );
            assert_eq!(unshown("~"), PathBuf::from(home));
        }
    }
}
