//! A stand-in for the shared graph: the same API, held in memory, for
//! tests and for working on kelve without the real one. It matches names
//! only by their letters, takes every entry at once, and checks nothing a
//! model would; the real graph does all three. A lesson is the same as one
//! there only when its words are. It does keep the rules for
//! changing an entry that code can keep: what each kind of revision may
//! do, merging on block ids, checked claims kept or dropped with why, and
//! claims carried when their pages haven't changed.
//!
//! Run on its own (`kelve-graph-stand-in`), it can start from a folder in
//! the shape of the real graph's dumps, and keep what it's given there.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::net::SocketAddr;
use std::path::{Path as FilePath, PathBuf};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query as Params, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use kelve_core::Entry;
use kelve_core::entry::draft::claim_key;
use kelve_core::entry::revise::{self, AskKind, Merged, RevisionKind};
use kelve_core::ideas::{IdeaKind, key, tidy};
use serde::Deserialize;

use crate::client::Client;
use kelve_core::relations::{RelationKind, RelationState, is_foundation, why_problem};

use crate::{
    Account, Agree, Applied, Ask, Bucket, Candidate, Candidates, Change, Changeset, CheckStatus,
    Checking, ClaimChecked, ClaimState, Counts, EntryPage, Flag, Foundations, Idea, IdeaAsk,
    IdeaRelations, Ideas, Instructions, Manifest, MatchRequest, Node, Problem, Rel, RelationRef,
    RelationTaken, Relations, Revise, SignInCode, SignedIn, Terms, Up, UpPage, WrittenBy, is_ref,
    name_hash, pkce_challenge, too_plain,
};
use crate::{LessonOutcome, LessonPage, LessonState, Lessons, Outcomes};
use kelve_core::lesson::{Lesson, Outcome, Record};
use kelve_core::matching::Card;

/// A relation the stand-in holds, as its dumps keep it.
#[derive(Clone, Debug, serde::Serialize, Deserialize)]
struct HeldRelation {
    from: String,
    kind: RelationKind,
    to: String,
    why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rank: Option<u8>,
    state: RelationState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    /// Why it was taken back, when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    because: Option<String>,
}

impl HeldRelation {
    /// Whether it's there at all: neither taken back nor waiting on a person.
    fn standing(&self) -> bool {
        !matches!(self.state, RelationState::Retracted | RelationState::Held)
    }
}

/// What the stand-in holds.
#[derive(Debug, Default)]
struct Held {
    /// The reading tokens read with: each only once.
    spent: std::collections::BTreeSet<[u8; 32]>,
    ideas: Vec<Idea>,
    /// Every revision of every entry, by idea ID, oldest first.
    entries: BTreeMap<String, Vec<EntryPage>>,
    redirects: BTreeMap<String, String>,
    /// Ideas found to be two, by their old IDs: the ideas they are now.
    splits: BTreeMap<String, Vec<String>>,
    /// Each idempotency key's first answer.
    answered: BTreeMap<String, Applied>,
    /// Sign-ins under way: each one-time code, its PKCE challenge and the
    /// address it went to.
    codes: BTreeMap<String, (String, String)>,
    /// How many times a kelve signed out.
    signed_out: u32,
    /// How many accounts were deleted.
    deleted: u32,
    /// The counts kelves shared.
    counts: Vec<Counts>,
    /// The flags readers sent.
    flags: Vec<Flag>,
    /// What readers asked entries to add, open until a revision answers:
    /// each idea's ID and what it was asked.
    asks: Vec<(String, AskKind)>,
    /// What each idea's current entry's problems are in, by block id.
    faulted: BTreeMap<String, Vec<String>>,
    /// Each claim checked and held, by key, with its pages' text as it was.
    checked: BTreeMap<String, Vec<String>>,
    /// What each pending extend was asked to add, by revision.
    answering: BTreeMap<String, AskKind>,
    /// Whether it grades entries, asking each for what it lacks, as the
    /// real graph does. Off unless a test or a person turns it on, so
    /// opening an entry asks the agent for nothing a reader didn't.
    grading: bool,
    /// How many requests asked which ideas names may be.
    match_requests: u32,
    /// How ideas relate, oldest first. The stand-in has no reviewers: what
    /// passes code's checks is live at once.
    relations: Vec<HeldRelation>,
    minted: u64,
    changesets: u64,
    revisions: u64,
    /// The folder it keeps what it's given in, when it has one.
    keep: Option<PathBuf>,
    /// The terms the account agreed to, if it has.
    agreed: Option<String>,
    /// The version of the terms it serves, when a test changed them: else
    /// the ones kelve was built with.
    terms: Option<String>,
    /// How each entry's check stands, by revision.
    checks: BTreeMap<String, Checking>,
    /// Entries held back while checks are held, by revision: the idea's ID
    /// and the entry.
    waiting: BTreeMap<String, (String, EntryPage)>,
    /// Whether new entries wait to be checked until a test lets them go.
    hold_checks: bool,
    /// The pages tests gave it, by address: a claim's quote is looked for
    /// on its page when the stand-in has it.
    pages: BTreeMap<String, String>,
    /// Who wrote each changeset that said.
    written_by: Vec<WrittenBy>,
    /// Each pending entry's claims, by revision, until it's checked.
    claims: BTreeMap<String, Vec<crate::Claim>>,
    /// How many times each relation was followed, by `from kind to`: one
    /// number each, and nothing about who.
    follows: BTreeMap<String, u32>,
    /// The entries and relations the account added, and how often each
    /// entry was served.
    added_entries: std::collections::BTreeSet<String>,
    added_relations: u32,
    served: BTreeMap<String, u32>,
    /// While the account has added as much as it may today: which limit
    /// ran out, `ideas`, `entries` or `relations`, as the graph's problem
    /// names it. Every changeset gets the graph's "Enough for today" then.
    enough: Option<String>,
    /// How many changesets were turned away for the day's limit.
    over_limit: u32,
    /// Lessons, oldest first. The stand-in checks them in code only, and finds
    /// the same only by their words.
    lessons: Vec<HeldLesson>,
    /// Each lesson's idempotency key's first answer.
    lesson_answers: BTreeMap<String, LessonPage>,
    /// The lessons the account has said what came of: once each.
    outcomes: std::collections::BTreeSet<String>,
}

/// A lesson the stand-in holds, as its dumps keep it.
#[derive(Clone, Debug, serde::Serialize, Deserialize)]
struct HeldLesson {
    #[serde(flatten)]
    page: LessonPage,
    /// Whether the stand-in's own account wrote it.
    #[serde(default)]
    mine: bool,
}

/// A stand-in graph, serving on a port of its own.
#[derive(Clone, Debug)]
pub struct StandIn {
    pub url: String,
    held: Arc<Mutex<Held>>,
}

/// The account a stand-in takes changesets from.
pub const TOKEN: &str = "stand-in";

impl StandIn {
    /// Starts one on a free local port.
    pub async fn start() -> std::io::Result<StandIn> {
        // Tests about anything but the terms start agreed to them.
        let held = Arc::new(Mutex::new(Held {
            agreed: Some(kelve_core::terms::VERSION.to_string()),
            ..Held::default()
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let app = router(held.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.ok() });
        Ok(StandIn { url, held })
    }

    /// Starts one at `addr`, holding what's in `folder` when one is given,
    /// and keeping what it's given there.
    pub async fn serve(addr: SocketAddr, folder: Option<PathBuf>) -> std::io::Result<StandIn> {
        let mut held = Held::default();
        if let Some(folder) = &folder {
            load(&mut held, folder)?;
            held.keep = Some(folder.clone());
        }
        let held = Arc::new(Mutex::new(held));
        let listener = tokio::net::TcpListener::bind(addr).await?;
        let url = format!("http://{}", listener.local_addr()?);
        let app = router(held.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.ok() });
        Ok(StandIn { url, held })
    }

    /// How many ideas, and entries, it holds.
    pub fn held(&self) -> (usize, usize) {
        let held = lock(&self.held);
        (held.ideas.len(), held.entries.len())
    }

    /// A client for it, signed in, or only reading, as a member's kelve
    /// reads before it adds anything.
    pub fn client(&self, signed_in: bool) -> Client {
        #[allow(clippy::expect_used)]
        let c = Client::new(&self.url, signed_in.then(|| TOKEN.to_string()))
            .expect("a client for a local address");
        if signed_in { c } else { c.reading_as(TOKEN) }
    }

    /// A client for it signed in to no account, which it doesn't answer.
    pub fn stranger(&self) -> Client {
        #[allow(clippy::expect_used)]
        Client::new(&self.url, None).expect("a client for a local address")
    }

    /// Adds an idea, as seeding would: its ID.
    pub fn add(&self, name: &str, definition: &str, kind: IdeaKind) -> String {
        let mut held = lock(&self.held);
        add(&mut held, name, definition, kind, None)
    }

    /// Relates two ideas, live at once, as a test needs.
    pub fn relate(&self, from: &str, kind: RelationKind, to: &str, why: &str) {
        lock(&self.held).relations.push(HeldRelation {
            from: from.into(),
            kind,
            to: to.into(),
            why: why.into(),
            rank: None,
            state: RelationState::Live,
            model: None,
            because: None,
        });
    }

    /// How many times a kelve signed out.
    pub fn signed_out(&self) -> u32 {
        lock(&self.held).signed_out
    }

    /// How many accounts were deleted.
    pub fn deleted(&self) -> u32 {
        lock(&self.held).deleted
    }

    /// The flags readers sent, in the order they came.
    pub fn flags(&self) -> Vec<Flag> {
        lock(&self.held).flags.clone()
    }

    /// The counts kelves shared, in the order they came.
    pub fn counts(&self) -> Vec<Counts> {
        lock(&self.held).counts.clone()
    }

    /// How many requests asked which ideas names may be.
    pub fn match_requests(&self) -> u32 {
        lock(&self.held).match_requests
    }

    /// Says what the checks found wrong with an idea's current entry, as
    /// the graph's re-check does.
    pub fn problems(&self, id: &str, problems: &[&str]) {
        let mut held = lock(&self.held);
        if let Some(i) = held.ideas.iter_mut().find(|i| i.id == id) {
            i.problems = problems.iter().map(|p| p.to_string()).collect();
        }
    }

    /// Says which blocks of an idea's current entry its problems are in.
    pub fn faulted(&self, id: &str, blocks: &[&str]) {
        lock(&self.held).faulted.insert(
            id.to_string(),
            blocks.iter().map(|b| b.to_string()).collect(),
        );
    }

    /// Grades entries, as the real graph does, or stops.
    pub fn grade(&self, on: bool) {
        lock(&self.held).grading = on;
    }

    /// What readers asked entries to add, still open.
    pub fn asks(&self) -> Vec<(String, AskKind)> {
        lock(&self.held).asks.clone()
    }

    /// Every revision of an idea's entry, oldest first.
    pub fn revisions(&self, id: &str) -> Vec<EntryPage> {
        lock(&self.held)
            .entries
            .get(id)
            .cloned()
            .unwrap_or_default()
    }

    /// Forgets that the account agreed to the terms, for tests about them.
    pub fn unagree(&self) {
        lock(&self.held).agreed = None;
    }

    /// The terms the account agreed to, if it has.
    pub fn agreed(&self) -> Option<String> {
        lock(&self.held).agreed.clone()
    }

    /// Serves the terms as another version, as the graph does once they
    /// change, for tests about a kelve built with other terms.
    pub fn serve_terms(&self, version: &str) {
        lock(&self.held).terms = Some(version.to_string());
    }

    /// Gives it a page's text, so a claim's quote from that page is looked
    /// for on it, as the graph does when it fetches the page.
    pub fn page(&self, url: &str, text: &str) {
        lock(&self.held).pages.insert(url.into(), text.into());
    }

    /// Holds every new entry's check, pending, until `release_checks`.
    pub fn hold_checks(&self, hold: bool) {
        lock(&self.held).hold_checks = hold;
    }

    /// Finishes every check held back.
    pub fn release_checks(&self) {
        let mut held = lock(&self.held);
        let waiting = std::mem::take(&mut held.waiting);
        for (revision, (id, page)) in waiting {
            finish_check(&mut held, &revision, &id, page);
        }
        for l in held.lessons.iter_mut() {
            if l.page.state == LessonState::Checking {
                l.page.state = LessonState::Live;
            }
        }
    }

    /// Says the account has added as many of `what` as a new account may
    /// today, `ideas`, `entries` or `relations`, so every changeset is
    /// turned away as the graph does until midnight UTC; or, with None,
    /// takes them again.
    pub fn enough(&self, what: Option<&str>) {
        lock(&self.held).enough = what.map(str::to_string);
    }

    /// How many changesets were turned away for the day's limit.
    pub fn over_limit(&self) -> u32 {
        lock(&self.held).over_limit
    }

    /// Who wrote each changeset that said, in the order they came.
    pub fn written_by(&self) -> Vec<WrittenBy> {
        lock(&self.held).written_by.clone()
    }

    /// Adds a lesson another account wrote, live at once, with what came of
    /// it: its ID.
    /// Says a lesson has been given `n` times across every account, as the
    /// real graph counts it.
    pub fn given(&self, id: &str, n: u32) {
        let mut held = lock(&self.held);
        if let Some(l) = held.lessons.iter_mut().find(|l| l.page.id == id) {
            l.page.given = n;
        }
    }

    pub fn learned(&self, lesson: Lesson, record: Record) -> String {
        self.learned_with(lesson, record, None, Vec::new())
    }

    /// The same, with the card the real graph would make for it; without
    /// one, the stand-in makes its own from the lesson's words.
    pub fn learned_with(
        &self,
        lesson: Lesson,
        record: Record,
        card: Option<Card>,
        contradicts: Vec<String>,
    ) -> String {
        let mut held = lock(&self.held);
        let id = format!("kelve:l{:012x}", held.lessons.len() + 1);
        let card = card.unwrap_or_else(|| kelve_core::matching::card(&lesson));
        held.lessons.push(HeldLesson {
            page: LessonPage {
                id: id.clone(),
                lesson,
                state: LessonState::Live,
                record,
                standing: record.standing(),
                same_as: None,
                why: None,
                card,
                contradicts: contradicts.clone(),
                given: 0,
            },
            mine: false,
        });
        // Another way goes both ways.
        for other in &contradicts {
            if let Some(l) = held.lessons.iter_mut().find(|l| &l.page.id == other) {
                l.page.contradicts.push(id.clone());
            }
        }
        id
    }

    /// Every lesson it holds, in whatever state.
    pub fn lessons(&self) -> Vec<LessonPage> {
        lock(&self.held)
            .lessons
            .iter()
            .map(|l| l.page.clone())
            .collect()
    }

    /// Merges `from` into `into`, as the graph does with duplicates.
    pub fn merge(&self, from: &str, into: &str) {
        let mut held = lock(&self.held);
        held.ideas.retain(|i| i.id != from);
        held.redirects.insert(from.to_string(), into.to_string());
    }

    /// Splits an idea found to be two, as the graph does when its models
    /// agree: the IDs of the two it is now, each with its own name and
    /// definition. Its relations are held, as they are in the graph until
    /// its judges say which idea each belongs to; the stand-in has no
    /// judges.
    pub fn split(&self, id: &str, into: [(&str, &str); 2]) -> Vec<String> {
        let mut held = lock(&self.held);
        let Some(at) = held.ideas.iter().position(|i| i.id == id) else {
            return Vec::new();
        };
        let old = held.ideas.remove(at);
        let ids: Vec<String> = into
            .iter()
            .map(|(name, definition)| add(&mut held, name, definition, old.kind, None))
            .collect();
        for r in held.relations.iter_mut() {
            if (r.from == id || r.to == id) && r.standing() {
                r.state = RelationState::Held;
            }
        }
        held.splits.insert(id.to_string(), ids.clone());
        ids
    }
}

/// What a test asks of a stand-in started on its own, as `kelve-graph-stand-in`:
/// the same as the methods above, for tests in another process.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Control {
    /// Holds new entries' checks, pending, or stops holding them.
    hold: Option<bool>,
    /// Finishes every check held back.
    release: bool,
    /// A page's address and its text, for claims' quotes.
    page: Option<(String, String)>,
    /// Forgets that the account agreed to the terms.
    unagree: bool,
    /// Grades entries, or stops.
    grade: Option<bool>,
    /// Turns every changeset away for the day's limit on these, `ideas`,
    /// `entries` or `relations`, or, with "", takes them again.
    enough: Option<String>,
    /// A lesson another account wrote, live, with what came of it.
    lesson: Option<Seed>,
}

/// A lesson another account wrote, for a test or the sandbox to give the
/// stand-in: with what came of it, and the card the real graph would make.
#[derive(Debug, Deserialize)]
struct Seed {
    lesson: Lesson,
    #[serde(default)]
    record: Record,
    #[serde(default)]
    card: Option<Card>,
    /// The lessons it contradicts, by ID.
    #[serde(default)]
    contradicts: Vec<String>,
}

async fn control(State(held): Shared, Json(c): Json<Control>) -> Response {
    let stand_in = StandIn {
        url: String::new(),
        held,
    };
    if let Some((url, text)) = &c.page {
        stand_in.page(url, text);
    }
    if let Some(hold) = c.hold {
        stand_in.hold_checks(hold);
    }
    if c.release {
        stand_in.release_checks();
    }
    if c.unagree {
        stand_in.unagree();
    }
    if let Some(on) = c.grade {
        stand_in.grade(on);
    }
    if let Some(what) = &c.enough {
        stand_in.enough(Some(what.as_str()).filter(|w| !w.is_empty()));
    }
    if let Some(Seed {
        lesson,
        record,
        card,
        contradicts,
    }) = c.lesson
    {
        stand_in.learned_with(lesson, record, card, contradicts);
    }
    StatusCode::NO_CONTENT.into_response()
}

fn lock(held: &Mutex<Held>) -> std::sync::MutexGuard<'_, Held> {
    held.lock().unwrap_or_else(|p| p.into_inner())
}

fn router(held: Arc<Mutex<Held>>) -> Router {
    Router::new()
        .route("/v1", get(manifest))
        .route("/v1/match", post(matches))
        .route("/v1/names/{prefix}", get(names))
        .route("/v1/ideas", get(ideas))
        .route("/v1/relations", get(relations))
        .route("/v1/relations/{id}/up", get(up))
        .route("/v1/foundations", get(foundations))
        .route("/v1/entries/{id}", get(entry))
        .route("/v1/entries/{id}/revisions/{revision}", get(revision))
        .route("/v1/changesets", post(changeset))
        .route("/v1/signin", get(sign_in).delete(sign_out))
        .route("/v1/signin/token", post(sign_in_token))
        .route("/v1/counts", post(counts))
        .route("/v1/follows", post(follows))
        .route("/v1/account/added", get(added))
        .route("/v1/flags", post(flags))
        .route("/v1/asks", post(asks))
        .route("/v1/account", get(account).delete(delete_account))
        .route("/v1/tokens/key", get(token_key))
        .route("/v1/account/tokens", post(tokens))
        .route("/v1/instructions/{name}", get(instructions))
        .route("/v1/terms", get(terms).post(agree))
        .route("/v1/revisions/{revision}/check", get(checking))
        .route("/v1/lessons", get(lessons).post(add_lesson))
        .route("/v1/lessons/outcomes", post(outcomes))
        .route("/v1/lessons/{id}", get(lesson))
        .route("/stand-in", post(control))
        .route_layer(axum::middleware::from_fn_with_state(
            held.clone(),
            reads_signed_in,
        ))
        .with_state(held)
}

/// The routes that read the graph, which, as the graph, answer only
/// someone signed in: reading it is part of adding to it.
const READS: [&str; 11] = [
    "/v1/match",
    "/v1/names/{prefix}",
    "/v1/ideas",
    "/v1/relations",
    "/v1/relations/{id}/up",
    "/v1/foundations",
    "/v1/entries/{id}",
    "/v1/entries/{id}/revisions/{revision}",
    "/v1/lessons",
    "/v1/lessons/{id}",
    "/v1/changes",
];

async fn reads_signed_in(
    State(held): Shared,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let reads = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .is_some_and(|p| READS.contains(&p.as_str()))
        && req.method() == axum::http::Method::GET
        || req.uri().path() == "/v1/match";
    if reads && !may_read(&held, req.headers()) {
        return problem(
            StatusCode::UNAUTHORIZED,
            "Sign in to kelve’s shared graph to read it and add to it.",
        );
    }
    next.run(req).await
}

type Shared = State<Arc<Mutex<Held>>>;

fn problem(status: StatusCode, detail: &str) -> Response {
    let p = Problem {
        kind: "about:blank".into(),
        title: status.canonical_reason().unwrap_or_default().into(),
        status: status.as_u16(),
        detail: detail.into(),
    };
    (status, Json(p)).into_response()
}

async fn manifest() -> Json<Manifest> {
    Json(Manifest {
        space: kelve_core::ideas::SHARED.into(),
        name: "kelve’s stand-in graph".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        min_client: None,
    })
}

async fn matches(State(held): Shared, Json(req): Json<MatchRequest>) -> Response {
    let mut held = lock(&held);
    held.match_requests += 1;
    let mut answers = BTreeMap::new();
    for (k, q) in req.queries {
        let want = key(&q.query);
        let limit = q.limit.unwrap_or(5).clamp(1, 20) as usize;
        let exact: Vec<&Idea> = held.ideas.iter().filter(|i| key(&i.name) == want).collect();
        let mut result: Vec<Candidate> = held
            .ideas
            .iter()
            .filter_map(|i| {
                let name = key(&i.name);
                let score = if name == want {
                    1.0
                } else if !want.is_empty() && (name.contains(&want) || want.contains(&name)) {
                    0.6
                } else {
                    return None;
                };
                Some(Candidate {
                    id: i.id.clone(),
                    name: i.name.clone(),
                    definition: i.definition.clone(),
                    score,
                    matched: score == 1.0 && exact.len() == 1,
                    same_as: i.same_as.clone(),
                })
            })
            .collect();
        result.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.name.cmp(&b.name)));
        result.truncate(limit);
        answers.insert(k, Candidates { result });
    }
    Json(answers).into_response()
}

/// A bucket of the name index: every idea's name whose hash starts with
/// the prefix.
async fn names(State(held): Shared, Path(prefix): Path<String>) -> Json<Bucket> {
    let held = lock(&held);
    let mut bucket = Bucket::default();
    for i in &held.ideas {
        let hash = name_hash(&i.name);
        if !hash.starts_with(&prefix) {
            continue;
        }
        bucket.names.entry(hash).or_default().push(Candidate {
            id: i.id.clone(),
            name: i.name.clone(),
            definition: i.definition.clone(),
            score: 1.0,
            matched: false,
            same_as: i.same_as.clone(),
        });
    }
    for (hash, list) in &mut bucket.names {
        let plain = held
            .ideas
            .iter()
            .find(|i| name_hash(&i.name) == *hash)
            .is_some_and(|i| too_plain(&kelve_core::ideas::norm(&i.name)));
        if list.len() == 1 && !plain {
            list[0].matched = true;
        }
    }
    Json(bucket)
}

#[derive(Deserialize)]
struct IdsParam {
    ids: String,
}

async fn ideas(State(held): Shared, Params(p): Params<IdsParam>) -> Response {
    let held = lock(&held);
    let mut out = Ideas::default();
    for id in p.ids.split(',').filter(|s| !s.is_empty()) {
        if let Some(now) = held.splits.get(id) {
            out.split.insert(id.to_string(), now.clone());
            continue;
        }
        let to = held.redirects.get(id).cloned();
        let look = to.as_deref().unwrap_or(id);
        match held.ideas.iter().find(|i| i.id == look) {
            Some(i) => {
                if let Some(to) = to {
                    out.redirects.insert(id.to_string(), to);
                }
                if !out.ideas.iter().any(|o| o.id == i.id) {
                    let mut i = i.clone();
                    i.foundation = is_foundation(&i.name);
                    i.depth = depth(&held, &i.id);
                    i.faulted = held.faulted.get(&i.id).cloned().unwrap_or_default();
                    i.asks = asks_of(&held, &i);
                    i.reads = held.served.get(&i.id).copied().unwrap_or_default().into();
                    out.ideas.push(i);
                }
            }
            None => out.missing.push(id.to_string()),
        }
    }
    Json(out).into_response()
}

async fn entry(State(held): Shared, Path(id): Path<String>) -> Response {
    let mut held = lock(&held);
    let id = held.redirects.get(&id).cloned().unwrap_or(id);
    let page = held
        .entries
        .get(&id)
        .and_then(|pages| pages.last())
        .cloned();
    match page {
        Some(page) => {
            // A serve, counted for the entry, with nothing about who.
            *held.served.entry(id).or_default() += 1;
            Json(page).into_response()
        }
        None => problem(StatusCode::NOT_FOUND, "That idea has no entry yet."),
    }
}

async fn revision(State(held): Shared, Path((id, revision)): Path<(String, String)>) -> Response {
    let held = lock(&held);
    let page = held
        .entries
        .get(&id)
        .and_then(|pages| pages.iter().find(|p| p.revision == revision))
        .cloned();
    match page {
        Some(page) => Json(page).into_response(),
        None => problem(StatusCode::NOT_FOUND, "There’s no such revision."),
    }
}

/// A revision with its claims in blocks its idea's problems are in marked
/// as found wrong, when it's the current one: as kelve marks them.
fn marked(held: &Held, id: &str, mut page: EntryPage) -> EntryPage {
    let current = held
        .ideas
        .iter()
        .any(|i| i.id == id && i.revision.as_deref() == Some(page.revision.as_str()));
    if let Some(faulted) = held.faulted.get(id).filter(|_| current) {
        for c in &mut page.claims {
            c.faulted = faulted.contains(&c.block);
        }
    }
    page
}

#[derive(Deserialize)]
struct SignInParams {
    redirect_uri: String,
    state: String,
    code_challenge: String,
}

/// Signing in, with no GitHub: straight back to the person's kelve, with a
/// code.
async fn sign_in(State(held): Shared, Params(p): Params<SignInParams>) -> Response {
    let Ok(mut back) = reqwest::Url::parse(&p.redirect_uri) else {
        return problem(StatusCode::BAD_REQUEST, "That isn't an address.");
    };
    let mut held = lock(&held);
    held.minted += 1;
    let code = format!("code-{}", held.minted);
    held.codes
        .insert(code.clone(), (p.code_challenge, p.redirect_uri.clone()));
    back.query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &p.state);
    (
        StatusCode::SEE_OTHER,
        [(axum::http::header::LOCATION, back.to_string())],
    )
        .into_response()
}

/// What the stand-in signs reading tokens with: one key, as the graph has.
static ISSUER: std::sync::LazyLock<crate::tokens::Issuer> = std::sync::LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    crate::tokens::Issuer::from_seed(b"the stand-in graph's reading tokens").expect("a seed")
});

async fn token_key() -> Json<crate::tokens::TokenKey> {
    Json(ISSUER.key().clone())
}

/// Signs a batch of reading tokens, blind, for the account signed in.
async fn tokens(headers: HeaderMap, Json(ask): Json<crate::tokens::TokenAsk>) -> Response {
    if !signed_in(&headers) {
        return problem(
            StatusCode::UNAUTHORIZED,
            "Sign in to kelve’s shared graph to read it and add to it.",
        );
    }
    match ISSUER.sign(&ask, 10_000) {
        Ok(grant) => Json(grant).into_response(),
        Err(e) => problem(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

/// Whether a request may read: with a reading token the stand-in signed
/// and nobody has read with, which it spends, or an account's own key.
fn may_read(held: &Arc<Mutex<Held>>, headers: &HeaderMap) -> bool {
    if signed_in(headers) {
        return true;
    }
    let Some(nonce) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(crate::tokens::from_header)
        .and_then(|t| ISSUER.check(t))
    else {
        return false;
    };
    lock(held).spent.insert(nonce)
}

/// Whether a request is from the account signed in.
fn signed_in(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == format!("Bearer {TOKEN}"))
}

async fn instructions(Path(name): Path<String>) -> Response {
    match kelve_core::instructions::get(&name) {
        Some(i) => Json(Instructions {
            name: i.name.into(),
            version: i.version,
            text: i.text,
        })
        .into_response(),
        None => problem(
            StatusCode::NOT_FOUND,
            "There are no instructions by that name.",
        ),
    }
}

impl Held {
    /// The version of the terms it serves now.
    fn terms_now(&self) -> &str {
        self.terms.as_deref().unwrap_or(kelve_core::terms::VERSION)
    }

    /// Whether the account agreed to the terms it serves now.
    fn agreed_now(&self) -> bool {
        self.agreed.as_deref() == Some(self.terms_now())
    }
}

fn the_terms(version: &str) -> Terms {
    Terms {
        version: version.into(),
        points: kelve_core::terms::POINTS
            .iter()
            .map(|p| p.to_string())
            .collect(),
        text: kelve_core::terms::text(),
    }
}

async fn terms(State(held): Shared) -> Json<Terms> {
    Json(the_terms(lock(&held).terms_now()))
}

async fn agree(State(held): Shared, headers: HeaderMap, Json(agree): Json<Agree>) -> Response {
    if !signed_in(&headers) {
        return problem(StatusCode::UNAUTHORIZED, "Sign in to agree to the terms.");
    }
    let mut held = lock(&held);
    if agree.version != held.terms_now() {
        return problem(
            StatusCode::CONFLICT,
            "Those aren’t the terms now. Read them again, and agree to these.",
        );
    }
    held.agreed = Some(agree.version);
    Json(Account {
        handle: "contributor-one".into(),
        agreed: held.agreed.clone(),
    })
    .into_response()
}

async fn account(State(held): Shared, headers: HeaderMap) -> Response {
    if !signed_in(&headers) {
        return problem(StatusCode::UNAUTHORIZED, "Sign in to see your account.");
    }
    Json(Account {
        handle: "contributor-one".into(),
        agreed: lock(&held).agreed.clone(),
    })
    .into_response()
}

async fn checking(
    State(held): Shared,
    headers: HeaderMap,
    Path(revision): Path<String>,
) -> Response {
    if !signed_in(&headers) {
        return problem(
            StatusCode::UNAUTHORIZED,
            "Sign in to see how your entries are checked.",
        );
    }
    match lock(&held).checks.get(&revision) {
        Some(c) => Json(c.clone()).into_response(),
        None => problem(StatusCode::NOT_FOUND, "There’s no such revision of yours."),
    }
}

/// A claim's key, as the graph serves it, from its quotes and the
/// entry's sources.
fn served_key(claim: &crate::Claim, entry: &Entry) -> String {
    claim_key(
        &kelve_core::entry::draft::Claim {
            text: claim.text.clone(),
            sources: claim.sources.clone(),
            quotes: claim
                .quotes
                .iter()
                .map(|q| kelve_core::entry::draft::Quote {
                    source: q.source,
                    text: q.text.clone(),
                })
                .collect(),
            block: claim.block.clone(),
            words: String::new(),
        },
        &entry.sources,
    )
}

/// Whether a revision of an idea's entry may be taken, as the graph's own
/// code decides it, and the entry it makes: the one sent, or, written
/// against a revision since replaced, its operations applied to the
/// current one. Why not, in a sentence, when it may not.
fn revision_allowed(
    held: &Held,
    id: &str,
    entry: Entry,
    claims: &[crate::Claim],
    r: &Revise,
) -> Result<Entry, String> {
    let idea = held
        .ideas
        .iter()
        .find(|i| i.id == id)
        .ok_or("no such idea")?;
    let pages = held.entries.get(id).cloned().unwrap_or_default();
    let current = pages.last().ok_or("The idea has no entry to change.")?;
    let base = pages
        .iter()
        .find(|p| p.revision == r.base)
        .ok_or("It was written against a revision the graph hasn’t got.")?;
    // Problems it can't place are anywhere.
    let faulted: Vec<String> = match held.faulted.get(id) {
        Some(f) if !f.is_empty() => f.clone(),
        _ if !idea.problems.is_empty() => {
            current.entry.blocks.iter().map(|b| b.id.clone()).collect()
        }
        _ => Vec::new(),
    };
    let faulted_blocks = faulted.clone();
    match r.kind {
        RevisionKind::Fix if idea.problems.is_empty() => {
            return Err("A fix is for an entry the checks found wrong, and this one isn’t.".into());
        }
        RevisionKind::Extend if !idea.problems.is_empty() => {
            return Err("It has problems to fix first.".into());
        }
        RevisionKind::Rewrite if !revise::rewrite_allowed(&current.entry, &faulted) => {
            return Err("It isn’t poor or mostly wrong, so it’s fixed or extended, not written again whole.".into());
        }
        _ => {}
    }
    // A fix may change only what's faulted, when the graph knows what is.
    let faulted_or_all: Vec<String> = if faulted_blocks.is_empty() {
        base.entry.blocks.iter().map(|b| b.id.clone()).collect()
    } else {
        faulted_blocks
    };
    revise::fits(r.kind, &r.ops, &faulted_or_all)?;
    let mut entry = entry;
    if r.kind != RevisionKind::Rewrite {
        if revise::apply(&base.entry.blocks, &r.ops).as_ref() != Ok(&entry.blocks) {
            return Err("Its blocks aren’t its base with its operations applied.".into());
        }
        if base.revision != current.revision {
            match revise::merge(&base.entry.blocks, &current.entry.blocks, &r.ops) {
                Merged::Applied(blocks) if base.entry.sources == current.entry.sources => {
                    entry.blocks = blocks;
                }
                _ => {
                    return Err("The entry changed where this changes it, since it was written. It’s written again against the new one.".into());
                }
            }
        }
    } else if base.revision != current.revision {
        return Err(
            "The entry changed since this was written. It’s written again against the new one."
                .into(),
        );
    }
    // Every checked claim of the base is kept, or dropped with why.
    let kept: Vec<String> = claims.iter().map(|c| served_key(c, &entry)).collect();
    let base = marked(held, id, base.clone());
    for c in base.claims.iter().filter(|c| !c.faulted) {
        if !kept.contains(&c.key)
            && !r
                .dropped
                .iter()
                .any(|d| d.key == c.key && !d.why.trim().is_empty())
        {
            return Err(format!(
                "It leaves out a checked claim without saying why: “{}”",
                c.text
            ));
        }
    }
    Ok(entry)
}

/// Words as a page and a quote are compared: lower case, spaces as one.
fn plain_words(text: &str) -> String {
    text.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Checks a pending entry as the stand-in can: each claim's quotes on the
/// pages tests gave it. An entry whose claims all hold becomes its idea's.
fn finish_check(held: &mut Held, revision: &str, id: &str, mut page: EntryPage) {
    let said = held.claims.remove(revision).unwrap_or_default();
    let mut checked = Vec::with_capacity(said.len());
    let mut keys = Vec::with_capacity(said.len());
    for (i, claim) in said.iter().enumerate() {
        let key = served_key(claim, &page.entry);
        // The text of each page it quotes, as the stand-in has it now.
        let pages: Vec<String> = claim
            .quotes
            .iter()
            .filter_map(|q| {
                let url = page
                    .entry
                    .sources
                    .iter()
                    .find(|s| s.n == q.source)?
                    .url
                    .as_ref()?;
                Some(held.pages.get(url).cloned().unwrap_or_default())
            })
            .collect();
        let carried = held.checked.get(&key) == Some(&pages);
        keys.push((key.clone(), pages));
        let mut found = String::new();
        for q in &claim.quotes {
            let on = page
                .entry
                .sources
                .iter()
                .find(|s| s.n == q.source)
                .and_then(|s| s.url.as_ref())
                .and_then(|url| held.pages.get(url));
            if let Some(text) = on
                && !plain_words(text).contains(&plain_words(&q.text))
            {
                found = format!("Its quote from source {} isn’t on that page.", q.source);
            }
        }
        checked.push(ClaimChecked {
            n: u32::try_from(i + 1).unwrap_or(u32::MAX),
            state: if found.is_empty() {
                ClaimState::Held
            } else {
                ClaimState::Failed
            },
            found,
            key,
            carried,
        });
    }
    let failed = checked.iter().position(|c| c.state == ClaimState::Failed);
    let status = if failed.is_some() {
        CheckStatus::Rejected
    } else {
        CheckStatus::Accepted
    };
    held.checks.insert(
        revision.to_string(),
        Checking {
            status,
            claims: checked,
            problem: failed
                .map(|n| format!("Claim {} isn’t supported by the pages it cites.", n + 1)),
        },
    );
    if status == CheckStatus::Accepted {
        for (key, pages) in keys {
            held.checked.insert(key, pages);
        }
        page.claims = said
            .iter()
            .map(|c| crate::Claim {
                key: served_key(c, &page.entry),
                ..c.clone()
            })
            .collect();
        if let Some(i) = held.ideas.iter_mut().find(|i| i.id == id) {
            i.revision = Some(revision.to_string());
            i.problems.clear();
        }
        held.faulted.remove(id);
        // What it was asked for, answered.
        if let Some(kind) = held.answering.remove(revision) {
            held.asks.retain(|(i, k)| !(i == id && *k == kind));
        }
        held.entries.entry(id.to_string()).or_default().push(page);
        // Added through a changeset: the account's.
        held.added_entries.insert(id.to_string());
    }
}

async fn sign_in_token(State(held): Shared, Json(asked): Json<SignInCode>) -> Response {
    let mut held = lock(&held);
    match held.codes.remove(&asked.code) {
        Some((challenge, redirect))
            if redirect == asked.redirect_uri
                && challenge == pkce_challenge(&asked.code_verifier) =>
        {
            Json(SignedIn {
                key: TOKEN.into(),
                handle: "contributor-one".into(),
            })
            .into_response()
        }
        _ => problem(
            StatusCode::BAD_REQUEST,
            "That sign-in has run out, or was used already. Sign in again from kelve.",
        ),
    }
}

/// What an idea's entry is asked to add: readers' asks, most asked
/// first, then the graph's own grade. None while its problems wait for a
/// fix.
fn asks_of(held: &Held, idea: &Idea) -> Vec<IdeaAsk> {
    let Some(page) = held.entries.get(&idea.id).and_then(|p| p.last()) else {
        return Vec::new();
    };
    if !idea.problems.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<IdeaAsk> = Vec::new();
    for (_, kind) in held.asks.iter().filter(|(i, _)| *i == idea.id) {
        match out.iter_mut().find(|a| a.kind == *kind) {
            Some(a) => a.readers += 1,
            None => out.push(IdeaAsk {
                kind: *kind,
                part: (*kind == AskKind::Example).then_some(kelve_core::entry::Part::How),
                readers: 1,
            }),
        }
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.readers));
    let graded = if held.grading {
        revise::grade(&page.entry)
    } else {
        Vec::new()
    };
    for (kind, part) in graded {
        if !out.iter().any(|a| a.kind == kind) {
            out.push(IdeaAsk {
                kind,
                part,
                readers: 0,
            });
        }
    }
    out
}

/// A reader's ask, kept with the idea whose current entry they read.
async fn asks(State(held): Shared, Json(ask): Json<Ask>) -> Response {
    let mut held = lock(&held);
    if !ask.kind.from_readers() {
        return problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Ask for an example, say it's too hard, or say something's missing.",
        );
    }
    let idea = held
        .ideas
        .iter()
        .find(|i| i.revision.as_deref() == Some(ask.revision.as_str()))
        .map(|i| i.id.clone());
    match idea {
        Some(id) => {
            held.asks.push((id, ask.kind));
            (StatusCode::ACCEPTED, Json(serde_json::json!({}))).into_response()
        }
        None => problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "That isn’t an entry the graph shows, so there’s nothing to ask of it.",
        ),
    }
}

async fn flags(State(held): Shared, Json(flag): Json<Flag>) -> Json<serde_json::Value> {
    lock(&held).flags.push(flag);
    Json(serde_json::json!({}))
}

/// Relations followed: one more each, with nothing about who sent them.
async fn follows(State(held): Shared, Json(f): Json<crate::Follows>) -> Response {
    if f.follows.len() > crate::MOST_FOLLOWS {
        return problem(StatusCode::UNPROCESSABLE_ENTITY, "Too many in one batch.");
    }
    let mut held = lock(&held);
    for r in f.follows {
        *held
            .follows
            .entry(follow_key(&r.from, r.kind, &r.to))
            .or_default() += 1;
    }
    (StatusCode::ACCEPTED, Json(serde_json::json!({}))).into_response()
}

async fn added(State(held): Shared, headers: HeaderMap) -> Response {
    if !signed_in(&headers) {
        return problem(
            StatusCode::UNAUTHORIZED,
            "Sign in to see what you’ve added.",
        );
    }
    let held = lock(&held);
    Json(crate::Added {
        entries: held.added_entries.len() as u32,
        relations: held.added_relations,
        reads: held
            .added_entries
            .iter()
            .map(|id| held.served.get(id).copied().unwrap_or(0))
            .sum(),
    })
    .into_response()
}

async fn counts(State(held): Shared, headers: HeaderMap, Json(counts): Json<Counts>) -> Response {
    // Counts come with no account and no reading token, as the real graph
    // asks: one that does is turned away, so a test sees it.
    if headers.contains_key("authorization") {
        return problem(
            StatusCode::BAD_REQUEST,
            "Counts never come with a key or a reading token.",
        );
    }
    lock(&held).counts.push(counts);
    Json(serde_json::json!({})).into_response()
}

async fn delete_account(State(held): Shared, headers: HeaderMap) -> Response {
    let signed_in = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == format!("Bearer {TOKEN}"));
    if !signed_in {
        return problem(StatusCode::UNAUTHORIZED, "Sign in to delete your account.");
    }
    lock(&held).deleted += 1;
    Json(serde_json::json!({ "ideas": 0, "entries": 0 })).into_response()
}

async fn sign_out(State(held): Shared) -> StatusCode {
    lock(&held).signed_out += 1;
    StatusCode::NO_CONTENT
}

async fn changeset(State(held): Shared, headers: HeaderMap, Json(cs): Json<Changeset>) -> Response {
    if !signed_in(&headers) {
        return problem(StatusCode::UNAUTHORIZED, "Sign in to add to the graph.");
    }
    if !lock(&held).agreed_now() {
        return problem(
            StatusCode::FORBIDDEN,
            "Agree to the terms for adding to the graph first. kelve shows them in settings.",
        );
    }
    let Some(idem) = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .filter(|k| !k.is_empty() && k.len() <= 255)
        .map(str::to_string)
    else {
        return problem(
            StatusCode::BAD_REQUEST,
            "A changeset needs an Idempotency-Key header.",
        );
    };
    let mut held = lock(&held);
    if let Some(first) = held.answered.get(&idem) {
        return Json(first.clone()).into_response();
    }
    if let Some(what) = held.enough.clone() {
        held.over_limit += 1;
        return enough_for_today(&what);
    }
    if let Some(by) = &cs.written_by {
        held.written_by.push(by.clone());
    }
    let applied = apply(&mut held, cs);
    held.answered.insert(idem, applied.clone());
    if let Some(folder) = held.keep.clone()
        && let Err(e) = save(&held, &folder)
    {
        return problem(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!(
                "The stand-in couldn’t keep that in {}: {e}",
                folder.display()
            ),
        );
    }
    Json(applied).into_response()
}

/// The graph's answer once an account has added as many of `what` as a
/// new account may today: its words, and the seconds to midnight UTC.
fn enough_for_today(what: &str) -> Response {
    let p = Problem {
        kind: "about:blank".into(),
        title: crate::ENOUGH_FOR_TODAY.into(),
        status: 429,
        detail: format!(
            "You’ve added as many {what} as a new account can in a day. kelve will send the rest tomorrow; the limit rises once ten of your entries are accepted, or after a week."
        ),
    };
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut res = (StatusCode::TOO_MANY_REQUESTS, Json(p)).into_response();
    if let Ok(v) = (86_400 - secs % 86_400).to_string().parse() {
        res.headers_mut().insert(axum::http::header::RETRY_AFTER, v);
    }
    res
}

/// The live lessons about some ideas, best first.
async fn lessons(State(held): Shared, Params(p): Params<IdsParam>) -> Response {
    let held = lock(&held);
    let ids: Vec<&str> = p
        .ids
        .split(',')
        .filter(|s| !s.is_empty())
        .take(200)
        .collect();
    let mut out: Vec<LessonPage> = held
        .lessons
        .iter()
        .filter(|l| l.page.state == LessonState::Live)
        .filter(|l| {
            l.page
                .lesson
                .about
                .iter()
                .any(|a| ids.contains(&a.as_str()))
        })
        .map(|l| l.page.clone())
        .collect();
    out.sort_by(|a, b| b.record.rank().total_cmp(&a.record.rank()));
    Json(Lessons { lessons: out }).into_response()
}

/// A lesson: live or the same as one, to anyone; in any other state, only
/// to its author, as the graph does.
async fn lesson(State(held): Shared, headers: HeaderMap, Path(id): Path<String>) -> Response {
    let author = signed_in(&headers);
    match lock(&held).lessons.iter().find(|l| l.page.id == id) {
        Some(l)
            if matches!(l.page.state, LessonState::Live | LessonState::Same)
                || (l.mine && author) =>
        {
            Json(l.page.clone()).into_response()
        }
        _ => problem(StatusCode::NOT_FOUND, "There’s no such lesson."),
    }
}

async fn add_lesson(
    State(held): Shared,
    headers: HeaderMap,
    Json(lesson): Json<Lesson>,
) -> Response {
    if !signed_in(&headers) {
        return problem(StatusCode::UNAUTHORIZED, "Sign in to add to the graph.");
    }
    if !lock(&held).agreed_now() {
        return problem(
            StatusCode::FORBIDDEN,
            "Agree to the terms for adding to the graph first. kelve shows them in settings.",
        );
    }
    let Some(idem) = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .filter(|k| !k.is_empty() && k.len() <= 255)
        .map(str::to_string)
    else {
        return problem(
            StatusCode::BAD_REQUEST,
            "A lesson needs an Idempotency-Key header.",
        );
    };
    let mut held = lock(&held);
    if let Some(first) = held.lesson_answers.get(&idem) {
        return Json(first.clone()).into_response();
    }
    if held.enough.as_deref() == Some("lessons") {
        held.over_limit += 1;
        return enough_for_today("lessons");
    }
    if let Some(p) = kelve_core::lesson::problems(&lesson, &[]).first() {
        return problem(StatusCode::UNPROCESSABLE_ENTITY, p);
    }
    if let Some(gone) = lesson
        .about
        .iter()
        .find(|a| !held.ideas.iter().any(|i| &i.id == *a))
    {
        return problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            &format!("A lesson is about ideas in the graph, and {gone} isn’t one."),
        );
    }
    for a in &lesson.about {
        let mine = held
            .lessons
            .iter()
            .filter(|l| {
                l.mine
                    && matches!(
                        l.page.state,
                        LessonState::Live | LessonState::Checking | LessonState::Waiting
                    )
                    && l.page.lesson.about.contains(a)
            })
            .count();
        if mine >= crate::MOST_LESSONS_AN_IDEA {
            return problem(
                StatusCode::UNPROCESSABLE_ENTITY,
                &format!(
                    "You have {} lessons about this idea already, the most one account may.",
                    crate::MOST_LESSONS_AN_IDEA
                ),
            );
        }
    }
    let norm = kelve_core::lesson::norm(&lesson);
    let same = held.lessons.iter().position(|l| {
        l.page.state == LessonState::Live
            && l.page.lesson.about.iter().any(|a| lesson.about.contains(a))
            && kelve_core::lesson::norm(&l.page.lesson) == norm
    });
    let id = format!("kelve:l{:012x}", held.lessons.len() + 1);
    let page = match same {
        Some(at) => {
            let there = &mut held.lessons[at];
            if !there.mine {
                there.page.record.held += 1;
                there.page.standing = there.page.record.standing();
            }
            LessonPage {
                id,
                card: kelve_core::matching::card(&lesson),
                lesson,
                state: LessonState::Same,
                record: Record::default(),
                standing: Default::default(),
                same_as: Some(there.page.id.clone()),
                why: None,
                contradicts: Vec::new(),
                given: 0,
            }
        }
        None => {
            let page = LessonPage {
                id,
                card: kelve_core::matching::card(&lesson),
                lesson,
                state: if held.hold_checks {
                    LessonState::Checking
                } else {
                    LessonState::Live
                },
                record: Record::default(),
                standing: Default::default(),
                same_as: None,
                why: None,
                contradicts: Vec::new(),
                given: 0,
            };
            held.lessons.push(HeldLesson {
                page: page.clone(),
                mine: true,
            });
            page
        }
    };
    held.lesson_answers.insert(idem, page.clone());
    if let Some(folder) = held.keep.clone() {
        save(&held, &folder).ok();
    }
    (StatusCode::CREATED, Json(page)).into_response()
}

/// What came of lessons, once a lesson for the account, never its own.
async fn outcomes(State(held): Shared, headers: HeaderMap, Json(o): Json<Outcomes>) -> Response {
    if !signed_in(&headers) {
        return problem(
            StatusCode::UNAUTHORIZED,
            "Sign in to say what came of lessons.",
        );
    }
    if o.outcomes.len() > crate::MOST_OUTCOMES {
        return problem(StatusCode::UNPROCESSABLE_ENTITY, "Too many in one batch.");
    }
    let mut held = lock(&held);
    for LessonOutcome {
        lesson,
        outcome,
        shown,
        withheld,
        ..
    } in o.outcomes
    {
        // What came of a moment it was held back from measures the lessons
        // together; it's no evidence for or against this one. Nor is a
        // moment nothing showed anything of.
        if withheld
            || outcome == Outcome::Unknown
            || shown == kelve_core::lesson::Shown::Nothing
            || held.outcomes.contains(&lesson)
        {
            continue;
        }
        let Some(l) = held
            .lessons
            .iter_mut()
            .find(|l| l.page.id == lesson && !l.mine)
        else {
            continue;
        };
        match outcome {
            Outcome::Held => l.page.record.held += 1,
            _ => l.page.record.didnt += 1,
        }
        l.page.given += 1;
        l.page.standing = l.page.record.standing();
        held.outcomes.insert(lesson);
    }
    if let Some(folder) = held.keep.clone() {
        save(&held, &folder).ok();
    }
    (StatusCode::ACCEPTED, Json(serde_json::json!({}))).into_response()
}

/// One idea, as a dump has it.
#[derive(serde::Serialize, Deserialize)]
struct DumpIdea {
    id: String,
    name: String,
    definition: String,
    kind: IdeaKind,
    #[serde(rename = "sameAs", default)]
    same_as: Option<String>,
}

/// One idea's current entry, as a dump has it.
#[derive(serde::Serialize, Deserialize)]
struct DumpEntry {
    idea: String,
    revision: String,
    entry: Entry,
    /// Its claims, with their keys, when the dump has them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    claims: Vec<crate::Claim>,
}

/// Takes in a folder in the shape of the real graph's dumps
/// (`ideas.jsonl`, `entries.jsonl`), when it has them. The links are the
/// entries' own, so `links.jsonl` isn't read.
fn load(held: &mut Held, folder: &FilePath) -> std::io::Result<()> {
    let lines = |name: &str| -> std::io::Result<Vec<String>> {
        match std::fs::File::open(folder.join(name)) {
            Ok(f) => std::io::BufReader::new(f).lines().collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    };
    let bad = |name: &str, n: usize, e: serde_json::Error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("line {} of {name} isn’t what a dump holds: {e}", n + 1),
        )
    };
    for (n, line) in lines("ideas.jsonl")?.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let i: DumpIdea = serde_json::from_str(line).map_err(|e| bad("ideas.jsonl", n, e))?;
        held.ideas.push(Idea {
            id: i.id,
            name: i.name,
            definition: i.definition,
            kind: i.kind,
            same_as: i.same_as,
            revision: None,
            problems: Vec::new(),
            faulted: Vec::new(),
            asks: Vec::new(),
            foundation: false,
            depth: None,
            reads: 0,
        });
    }
    for (n, line) in lines("relations.jsonl")?.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let r: HeldRelation =
            serde_json::from_str(line).map_err(|e| bad("relations.jsonl", n, e))?;
        held.relations.push(r);
    }
    for (n, line) in lines("entries.jsonl")?.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let e: DumpEntry = serde_json::from_str(line).map_err(|e| bad("entries.jsonl", n, e))?;
        if let Some(i) = held.ideas.iter_mut().find(|i| i.id == e.idea) {
            i.revision = Some(e.revision.clone());
        }
        // Revisions it makes are numbered after those it starts with.
        if let Some(n) = e
            .revision
            .strip_prefix('r')
            .and_then(|n| n.parse::<u64>().ok())
        {
            held.revisions = held.revisions.max(n);
        }
        held.entries.entry(e.idea).or_default().push(EntryPage {
            entry: e.entry,
            revision: e.revision,
            reviewed: false,
            claims: e.claims,
        });
    }
    for (n, line) in lines("lessons.jsonl")?.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let l: HeldLesson = serde_json::from_str(line).map_err(|e| bad("lessons.jsonl", n, e))?;
        held.lessons.push(l);
    }
    Ok(())
}

/// Writes what it holds into a folder in the shape of the real graph's
/// dumps: each idea, its current entry, and the links the entries make.
fn save(held: &Held, folder: &FilePath) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let write = |name: &str, lines: Vec<serde_json::Value>| -> std::io::Result<()> {
        let part = folder.join(format!("{name}.part"));
        let mut out = std::io::BufWriter::new(std::fs::File::create(&part)?);
        for line in lines {
            serde_json::to_writer(&mut out, &line)?;
            out.write_all(b"\n")?;
        }
        out.flush()?;
        std::fs::rename(part, folder.join(name))
    };
    let ideas = held
        .ideas
        .iter()
        .map(|i| {
            serde_json::json!(DumpIdea {
                id: i.id.clone(),
                name: i.name.clone(),
                definition: i.definition.clone(),
                kind: i.kind,
                same_as: i.same_as.clone(),
            })
        })
        .collect();
    let mut entries = Vec::new();
    let mut links = Vec::new();
    for (id, pages) in &held.entries {
        let Some(page) = pages.last() else { continue };
        entries.push(serde_json::json!(DumpEntry {
            idea: id.clone(),
            revision: page.revision.clone(),
            entry: page.entry.clone(),
            claims: page.claims.clone(),
        }));
        for link in &page.entry.links {
            links.push(serde_json::json!({
                "from": id, "kind": link.kind, "to": link.idea.id, "term": link.term,
            }));
        }
    }
    let counts = (held.ideas.len(), entries.len(), links.len());
    write("ideas.jsonl", ideas)?;
    write("entries.jsonl", entries)?;
    write("links.jsonl", links)?;
    write(
        "relations.jsonl",
        held.relations
            .iter()
            .map(|r| serde_json::json!(r))
            .collect(),
    )?;
    write(
        "lessons.jsonl",
        held.lessons.iter().map(|l| serde_json::json!(l)).collect(),
    )?;
    write(
        "manifest.json",
        vec![serde_json::json!({
            "feed": 0, "ideas": counts.0, "entries": counts.1, "links": counts.2,
            "licence": "CC0-1.0",
        })],
    )
}

fn add(
    held: &mut Held,
    name: &str,
    definition: &str,
    kind: IdeaKind,
    same_as: Option<String>,
) -> String {
    let want = key(name);
    if let Some(i) = held.ideas.iter_mut().find(|i| key(&i.name) == want) {
        if i.definition.is_empty() {
            i.definition = definition.to_string();
        }
        return i.id.clone();
    }
    let id = loop {
        held.minted += 1;
        let id = format!("{}:i{:012}", kelve_core::ideas::SHARED, held.minted);
        if !held.ideas.iter().any(|i| i.id == id) {
            break id;
        }
    };
    held.ideas.push(Idea {
        id: id.clone(),
        name: name.to_string(),
        definition: definition.to_string(),
        kind,
        same_as,
        revision: None,
        problems: Vec::new(),
        faulted: Vec::new(),
        asks: Vec::new(),
        foundation: false,
        depth: None,
        reads: 0,
    });
    id
}

fn apply(held: &mut Held, cs: Changeset) -> Applied {
    held.changesets += 1;
    let mut applied = Applied {
        id: format!("c{}", held.changesets),
        ..Applied::default()
    };
    for change in cs.changes {
        match change {
            Change::AddIdea {
                reference,
                name,
                definition,
                kind,
                same_as,
            } => {
                let Some(name) = tidy(&name) else {
                    applied
                        .skipped
                        .push(format!("“{name}” isn’t a name an idea can have."));
                    continue;
                };
                let id = add(held, &name, &definition, kind, same_as);
                applied.ideas.insert(reference, id);
            }
            Change::SetEntry {
                entry,
                revise,
                claims,
            } => {
                let mut entry: Entry = *entry;
                let Some(id) = resolve(held, &applied, &entry.idea.id) else {
                    applied.skipped.push(format!(
                        "The entry for “{}” is about an idea the graph doesn’t have.",
                        entry.idea.name
                    ));
                    continue;
                };
                // A revision is taken only as its kind allows.
                let name = entry.idea.name.clone();
                if let Some(r) = &revise {
                    match revision_allowed(held, &id, entry, &claims, r) {
                        Ok(e) => entry = e,
                        Err(why) => {
                            applied
                                .skipped
                                .push(format!("The entry for “{name}” wasn’t taken: {why}"));
                            continue;
                        }
                    }
                }
                entry.idea.id = id.clone();
                let mut kept = Vec::with_capacity(entry.links.len());
                for mut link in entry.links {
                    if let Some(to) = resolve(held, &applied, &link.idea.id) {
                        link.idea.id = to;
                        kept.push(link);
                    }
                }
                entry.links = kept;
                held.revisions += 1;
                let revision = format!("r{}", held.revisions);
                let page = EntryPage {
                    entry,
                    revision: revision.clone(),
                    reviewed: false,
                    claims: Vec::new(),
                };
                if let Some(ask) = revise.as_ref().and_then(|r| r.ask.as_ref()) {
                    held.answering.insert(revision.clone(), ask.kind);
                }
                held.checks.insert(
                    revision.clone(),
                    Checking {
                        status: CheckStatus::Pending,
                        claims: (1..=claims.len())
                            .map(|n| ClaimChecked {
                                n: u32::try_from(n).unwrap_or(u32::MAX),
                                state: ClaimState::Waiting,
                                found: String::new(),
                                key: String::new(),
                                carried: false,
                            })
                            .collect(),
                        problem: None,
                    },
                );
                held.claims.insert(revision.clone(), claims);
                if held.hold_checks {
                    held.waiting.insert(revision.clone(), (id.clone(), page));
                } else {
                    finish_check(held, &revision, &id, page);
                }
                applied.entries.insert(id, revision);
            }
            Change::Relate {
                from,
                kind,
                to,
                why,
                rank,
                model,
            } => relate(held, &mut applied, &from, kind, &to, why, rank, model),
            Change::Unrelate {
                from,
                kind,
                to,
                why,
            } => {
                let (Some(from), Some(to)) =
                    (resolve(held, &applied, &from), resolve(held, &applied, &to))
                else {
                    applied
                        .skipped
                        .push("That relation is between ideas the graph doesn’t have.".into());
                    continue;
                };
                let found = held
                    .relations
                    .iter_mut()
                    .find(|r| r.standing() && r.kind == kind && same_ends(r, &from, &to));
                match found {
                    Some(r) => {
                        r.state = RelationState::Retracted;
                        r.because = Some(why);
                        applied.relations.push(RelationTaken {
                            relation: RelationRef { from, kind, to },
                            state: RelationState::Retracted,
                        });
                    }
                    None => applied
                        .skipped
                        .push("There’s no such relation to take back.".into()),
                }
            }
        }
    }
    applied
}

/// Whether a relation joins these two, either way round for a kind that
/// goes both ways.
fn same_ends(r: &HeldRelation, from: &str, to: &str) -> bool {
    (r.from == from && r.to == to) || (r.kind.both_ways() && r.from == to && r.to == from)
}

fn name_of(held: &Held, id: &str) -> String {
    held.ideas
        .iter()
        .find(|i| i.id == id)
        .map(|i| i.name.clone())
        .unwrap_or_default()
}

/// Whether `from` already comes down to `to` along standing relations of
/// this kind: then `to` relating to `from` would make a loop.
fn reaches(held: &Held, kind: RelationKind, from: &str, to: &str) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    let mut queue = vec![from.to_string()];
    while let Some(at) = queue.pop() {
        if at == to {
            return true;
        }
        if !seen.insert(at.clone()) {
            continue;
        }
        for r in held
            .relations
            .iter()
            .filter(|r| r.standing() && r.kind == kind && r.from == at)
        {
            queue.push(r.to.clone());
        }
    }
    false
}

/// Takes one proposed relation, or says why not, as the graph does in code.
/// There's no review here, so what passes is live at once, unless the idea has
/// as many of its kind as it may.
#[allow(clippy::too_many_arguments)]
fn relate(
    held: &mut Held,
    applied: &mut Applied,
    from: &str,
    kind: RelationKind,
    to: &str,
    why: String,
    rank: Option<u8>,
    model: Option<String>,
) {
    let (Some(from), Some(to)) = (resolve(held, applied, from), resolve(held, applied, to)) else {
        applied
            .skipped
            .push("That relation is between ideas the graph doesn’t have.".into());
        return;
    };
    let (a, b) = (name_of(held, &from), name_of(held, &to));
    let mut refuse = |why: String| applied.skipped.push(why);
    if from == to {
        return refuse(format!("“{a}” can’t relate to itself."));
    }
    let definition = held
        .ideas
        .iter()
        .find(|i| i.id == from)
        .map(|i| i.definition.clone())
        .unwrap_or_default();
    if let Some(problem) = why_problem(&why, &definition) {
        return refuse(format!("{problem} (“{a}” and “{b}”)"));
    }
    if kind.never_loops() && reaches(held, kind, &to, &from) {
        return refuse(format!(
            "“{a}” to “{b}” would make a loop: “{b}” already comes down to “{a}”."
        ));
    }
    let standing = |f: &str, k: RelationKind, t: &str| {
        held.relations
            .iter()
            .any(|r| r.standing() && r.kind == k && r.from == f && r.to == t)
    };
    if kind == RelationKind::RestsOn && standing(&to, RelationKind::RestsOn, &from) {
        return refuse(format!(
            "The other way round is already there: “{b}” rests on “{a}”."
        ));
    }
    if kind == RelationKind::RestsOn && standing(&from, RelationKind::KindOf, &to) {
        return refuse(format!(
            "“{a}” is a kind of “{b}”, so it doesn’t rest on it."
        ));
    }
    if let Some(r) = held
        .relations
        .iter_mut()
        .find(|r| r.kind == kind && same_ends(r, &from, &to))
    {
        let state = match r.state {
            RelationState::Retracted => {
                let because = r.because.clone().unwrap_or_default();
                return applied.skipped.push(format!(
                    "“{a}” to “{b}” was taken back, and isn’t proposed again: {because}"
                ));
            }
            // Proposed again, from another model or by a person: agreed.
            RelationState::Live if r.model != model => RelationState::Settled,
            other => other,
        };
        r.state = state;
        applied.relations.push(RelationTaken {
            relation: RelationRef { from, kind, to },
            state,
        });
        return;
    }
    if kind == RelationKind::KindOf {
        // The broader category isn't a prerequisite: it's what it's a kind of.
        for r in held.relations.iter_mut().filter(|r| {
            r.standing() && r.kind == RelationKind::RestsOn && r.from == from && r.to == to
        }) {
            r.state = RelationState::Retracted;
            r.because = Some(format!("“{a}” is a kind of “{b}”."));
        }
    }
    let full = held
        .relations
        .iter()
        .filter(|r| {
            r.state.served()
                && r.kind == kind
                && (r.from == from || (kind.both_ways() && r.to == from))
        })
        .count()
        >= kind.most();
    let state = if full {
        RelationState::Proposed
    } else {
        RelationState::Live
    };
    held.relations.push(HeldRelation {
        from: from.clone(),
        kind,
        to: to.clone(),
        why,
        rank,
        state,
        model,
        because: None,
    });
    if state == RelationState::Live {
        held.added_relations += 1;
    }
    applied.relations.push(RelationTaken {
        relation: RelationRef { from, kind, to },
        state,
    });
}

/// An idea as a relation shows it.
fn node(held: &Held, id: &str) -> Node {
    let name = name_of(held, id);
    Node {
        id: id.to_string(),
        foundation: is_foundation(&name),
        depth: depth(held, id),
        name,
    }
}

/// How many steps of served rests-on an idea is from a foundation.
fn depth(held: &Held, id: &str) -> Option<u8> {
    let mut seen = std::collections::BTreeSet::new();
    let mut level = vec![id.to_string()];
    for d in 0..=u8::MAX {
        if level.iter().any(|i| is_foundation(&name_of(held, i))) {
            return Some(d);
        }
        let mut next = Vec::new();
        for at in &level {
            if !seen.insert(at.clone()) {
                continue;
            }
            for r in held
                .relations
                .iter()
                .filter(|r| r.state.served() && r.kind == RelationKind::RestsOn && &r.from == at)
            {
                next.push(r.to.clone());
            }
        }
        if next.is_empty() {
            return None;
        }
        level = next;
    }
    None
}

/// How many of what rests on an idea come with its relations.
const FIRST_UP: usize = 20;
/// The most first principles listed as what an idea comes down to.
const MOST_GROUNDS: usize = 24;

fn rel(held: &Held, r: &HeldRelation, other: &str) -> Rel {
    Rel {
        idea: node(held, other),
        why: r.why.clone(),
        state: r.state,
        rank: r.rank,
    }
}

/// A relation's key for its count: its two ends, in order for those that
/// go one way, and either way for those that go both.
fn follow_key(from: &str, kind: RelationKind, to: &str) -> String {
    let both = matches!(kind, RelationKind::Alternative | RelationKind::UsedWith);
    let (a, b) = if both && to < from {
        (to, from)
    } else {
        (from, to)
    };
    format!("{a} {} {b}", kind.as_str())
}

/// The most followed first; those nobody has followed keep their order,
/// after them.
fn by_follows(held: &Held, from: &str, kind: RelationKind, rels: &mut [Rel]) {
    rels.sort_by_key(|r| {
        std::cmp::Reverse(
            held.follows
                .get(&follow_key(from, kind, &r.idea.id))
                .copied()
                .unwrap_or(0),
        )
    });
}

fn relations_of(held: &Held, id: &str) -> IdeaRelations {
    let served = || held.relations.iter().filter(|r| r.state.served());
    let from = |k: RelationKind| -> Vec<Rel> {
        served()
            .filter(|r| r.kind == k && r.from == id)
            .map(|r| rel(held, r, &r.to))
            .collect()
    };
    let both = |k: RelationKind| -> Vec<Rel> {
        served()
            .filter(|r| r.kind == k && (r.from == id || r.to == id))
            .map(|r| rel(held, r, if r.from == id { &r.to } else { &r.from }))
            .collect()
    };
    let mut down = from(RelationKind::RestsOn);
    down.sort_by_key(|r| r.rank.unwrap_or(u8::MAX));
    by_follows(held, id, RelationKind::RestsOn, &mut down);
    let mut kind_of = from(RelationKind::KindOf);
    by_follows(held, id, RelationKind::KindOf, &mut kind_of);
    let mut answers = from(RelationKind::Answers);
    by_follows(held, id, RelationKind::Answers, &mut answers);
    let mut alternatives = both(RelationKind::Alternative);
    by_follows(held, id, RelationKind::Alternative, &mut alternatives);
    let mut used_with = both(RelationKind::UsedWith);
    by_follows(held, id, RelationKind::UsedWith, &mut used_with);
    let above: Vec<&HeldRelation> = served()
        .filter(|r| r.kind == RelationKind::RestsOn && r.to == id)
        .collect();
    // The first principles it comes down to, by any way.
    let mut grounds = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut queue = vec![id.to_string()];
    while let Some(at) = queue.pop() {
        if !seen.insert(at.clone()) {
            continue;
        }
        for r in served().filter(|r| r.kind == RelationKind::RestsOn && r.from == at) {
            let n = node(held, &r.to);
            if n.foundation {
                if !grounds.iter().any(|g: &Node| g.id == n.id) {
                    grounds.push(n);
                }
            } else {
                queue.push(r.to.clone());
            }
        }
    }
    grounds.truncate(MOST_GROUNDS);
    IdeaRelations {
        down,
        kind_of,
        answers,
        alternatives,
        used_with,
        up: Up {
            count: above.len() as u32,
            first: above
                .iter()
                .take(FIRST_UP)
                .map(|r| rel(held, r, &r.from))
                .collect(),
        },
        grounds,
    }
}

async fn relations(State(held): Shared, Params(p): Params<IdsParam>) -> Response {
    let held = lock(&held);
    let mut out = Relations::default();
    for id in p
        .ids
        .split(',')
        .filter(|s| !s.is_empty())
        .take(crate::MOST_RELATIONS)
    {
        if let Some(now) = held.splits.get(id) {
            out.split.insert(id.to_string(), now.clone());
            continue;
        }
        let to = held.redirects.get(id).cloned();
        let look = to.as_deref().unwrap_or(id);
        if !held.ideas.iter().any(|i| i.id == look) {
            out.missing.push(id.to_string());
            continue;
        }
        if let Some(to) = &to {
            out.redirects.insert(id.to_string(), to.clone());
        }
        out.relations
            .insert(look.to_string(), relations_of(&held, look));
    }
    Json(out).into_response()
}

#[derive(Deserialize)]
struct UpParams {
    #[serde(default)]
    after: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

async fn up(State(held): Shared, Path(id): Path<String>, Params(p): Params<UpParams>) -> Response {
    let held = lock(&held);
    let from = p.after.and_then(|a| a.parse::<usize>().ok()).unwrap_or(0);
    let limit = p.limit.unwrap_or(50).clamp(1, 200);
    let above: Vec<Rel> = held
        .relations
        .iter()
        .filter(|r| r.state.served() && r.kind == RelationKind::RestsOn && r.to == id)
        .skip(from)
        .take(limit + 1)
        .map(|r| rel(&held, r, &r.from))
        .collect();
    let next = (above.len() > limit).then(|| (from + limit).to_string());
    Json(UpPage {
        up: above.into_iter().take(limit).collect(),
        next,
    })
    .into_response()
}

async fn foundations(State(held): Shared) -> Json<Foundations> {
    let held = lock(&held);
    Json(Foundations {
        ideas: held
            .ideas
            .iter()
            .filter(|i| is_foundation(&i.name))
            .map(|i| node(&held, &i.id))
            .collect(),
    })
}

/// The ID an idea in a changeset stands for: a `ref:` this changeset added,
/// or an ID the graph has, following a merge.
fn resolve(held: &Held, applied: &Applied, id: &str) -> Option<String> {
    if is_ref(id) {
        return applied.ideas.get(&id[crate::REF.len()..]).cloned();
    }
    let id = held.redirects.get(id).map_or(id, String::as_str);
    held.ideas
        .iter()
        .any(|i| i.id == id)
        .then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use kelve_core::entry::Link;
    use kelve_core::ideas::{IdeaRef, LinkKind};

    use super::*;
    use crate::Query;
    use crate::client::{AGENT, Error};

    fn entry(id: &str, name: &str, links: &[(&str, &str)]) -> Entry {
        Entry {
            idea: IdeaRef {
                id: id.into(),
                name: name.into(),
            },
            title: name.into(),
            question: String::new(),
            minutes: 2,
            made_at: kelve_core::time::Timestamp::now(),
            blocks: vec![],
            sources: vec![],
            links: links
                .iter()
                .map(|(term, id)| Link {
                    term: (*term).into(),
                    idea: IdeaRef {
                        id: (*id).into(),
                        name: (*term).into(),
                    },
                    kind: LinkKind::RestsOn,
                })
                .collect(),
        }
    }

    fn ask(name: &str) -> MatchRequest {
        let mut req = MatchRequest::default();
        req.queries.insert(
            "q".into(),
            Query {
                query: name.into(),
                context: None,
                limit: None,
            },
        );
        req
    }

    #[tokio::test]
    async fn a_changeset_adds_ideas_and_entries_once_however_often_its_sent() {
        let g = StandIn::start().await.unwrap();
        let locks = g.add(
            "Locks",
            "A way to let one thing in at a time.",
            IdeaKind::Concept,
        );
        let client = g.client(true);
        let cs = Changeset {
            agent: AGENT.into(),
            comment: String::new(),
            changes: vec![
                Change::AddIdea {
                    reference: "a".into(),
                    name: " Advisory   locks ".into(),
                    definition: "A lock a program names and the database keeps.".into(),
                    kind: IdeaKind::Concept,
                    same_as: None,
                },
                Change::SetEntry {
                    entry: Box::new(entry(
                        "ref:a",
                        "Advisory locks",
                        &[("locks", &locks), ("nothing", "kelve:i999999999999")],
                    )),
                    claims: Vec::new(),
                    revise: None,
                },
            ],
            written_by: None,
        };
        let applied = client.apply(&cs, "one").await.unwrap();
        let again = client.apply(&cs, "one").await.unwrap();
        assert_eq!(applied, again, "a retry gets the first answer");
        let id = &applied.ideas["a"];

        let found = client.candidates(&ask("advisory-locks")).await.unwrap();
        let best = &found["q"].result[0];
        assert!(best.matched && &best.id == id, "{found:?}");
        assert_eq!(best.name, "Advisory locks");

        let page = client.entry(id, None).await.unwrap().expect("an entry");
        assert_eq!(page.entry.idea.id, *id);
        assert_eq!(page.entry.links.len(), 1, "a link to nothing is left out");
        assert_eq!(page.entry.links[0].idea.id, locks);
        let ideas = client.ideas(std::slice::from_ref(id)).await.unwrap();
        assert_eq!(
            ideas.ideas[0].revision.as_deref(),
            Some(page.revision.as_str())
        );
        assert_eq!(
            g.client(false).apply(&cs, "two").await.unwrap_err(),
            Error::SignIn
        );
    }

    #[tokio::test]
    async fn one_on_its_own_keeps_what_its_given_in_a_dumps_shape() {
        let folder = std::env::temp_dir().join(format!("kelve-stand-in-{}", std::process::id()));
        std::fs::remove_dir_all(&folder).ok();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let g = StandIn::serve(addr, Some(folder.clone())).await.unwrap();
        let cs = Changeset {
            agent: AGENT.into(),
            comment: String::new(),
            changes: vec![
                Change::AddIdea {
                    reference: "a".into(),
                    name: "Advisory locks".into(),
                    definition: "A lock a program names and the database keeps.".into(),
                    kind: IdeaKind::Concept,
                    same_as: None,
                },
                Change::SetEntry {
                    entry: Box::new(entry("ref:a", "Advisory locks", &[])),
                    claims: Vec::new(),
                    revise: None,
                },
            ],
            written_by: None,
        };
        // On its own, it takes nothing until the terms are agreed to.
        assert!(g.client(true).apply(&cs, "early").await.is_err());
        g.client(true)
            .agree(kelve_core::terms::VERSION)
            .await
            .unwrap();
        let applied = g.client(true).apply(&cs, "one").await.unwrap();
        let id = applied.ideas["a"].clone();
        let set = g.add("Set", "A collection of distinct things.", IdeaKind::Concept);
        send(
            &g,
            vec![relate(
                &id,
                RelationKind::RestsOn,
                &set,
                "A lock is held over a set of rows.",
                None,
            )],
            "two",
        )
        .await;
        assert!(folder.join("manifest.json").exists());

        let again = StandIn::serve(addr, Some(folder.clone())).await.unwrap();
        assert_eq!(again.held(), (2, 1));
        let page = again.client(false).entry(&id, None).await.unwrap();
        assert_eq!(page.expect("its entry").entry.idea.id, id);
        let kept = again
            .client(false)
            .relations(std::slice::from_ref(&id))
            .await
            .unwrap();
        assert_eq!(
            kept.relations[&id].down[0].idea.name, "Set",
            "relations are kept too"
        );
        let next = again.add(
            "Locks",
            "A way to let one thing in at a time.",
            IdeaKind::Concept,
        );
        assert_ne!(next, id, "a new idea never takes a kept one's ID");
        std::fs::remove_dir_all(&folder).ok();
    }

    fn relate(from: &str, kind: RelationKind, to: &str, why: &str, model: Option<&str>) -> Change {
        Change::Relate {
            from: from.into(),
            kind,
            to: to.into(),
            why: why.into(),
            rank: None,
            model: model.map(str::to_string),
        }
    }

    async fn send(g: &StandIn, changes: Vec<Change>, key: &str) -> Applied {
        let cs = Changeset {
            agent: AGENT.into(),
            comment: String::new(),
            changes,
            written_by: None,
        };
        g.client(true).apply(&cs, key).await.unwrap()
    }

    #[tokio::test]
    async fn relations_go_down_to_the_ground_and_code_refuses_what_cant_be() {
        use RelationKind::*;
        let g = StandIn::start().await.unwrap();
        let caching = g.add(
            "Prompt caching",
            "Reusing a prompt's start.",
            IdeaKind::Concept,
        );
        let kv = g.add(
            "Transformer key-value cache",
            "Keys and values kept.",
            IdeaKind::Concept,
        );
        let attention = g.add(
            "Attention",
            "Weighing a sequence by itself.",
            IdeaKind::Concept,
        );
        let set = g.add("Set", "A collection of distinct things.", IdeaKind::Concept);
        let cache = g.add("Caching", "Keeping a result to reuse.", IdeaKind::Concept);
        let rag = g.add(
            "Retrieval-augmented generation",
            "Sending what's needed.",
            IdeaKind::Concept,
        );
        let opus = Some("anthropic/claude-opus-5.5");
        let applied = send(
            &g,
            vec![
                relate(
                    &caching,
                    RestsOn,
                    &kv,
                    "What's kept is the keys attention computed.",
                    opus,
                ),
                relate(
                    &kv,
                    RestsOn,
                    &attention,
                    "The cache holds what attention compares against.",
                    opus,
                ),
                relate(
                    &attention,
                    RestsOn,
                    &set,
                    "It weighs a set of tokens against each other.",
                    opus,
                ),
                relate(
                    &caching,
                    RestsOn,
                    &cache,
                    "It keeps a result to reuse.",
                    opus,
                ),
                relate(
                    &caching,
                    Alternative,
                    &rag,
                    "Both keep long context cheap.",
                    opus,
                ),
                // Refused in code, each with a sentence.
                relate(&set, RestsOn, &caching, "Sets are made of caches.", opus),
                relate(&kv, RestsOn, &caching, "Backwards.", opus),
                relate(&caching, RestsOn, &caching, "Itself.", opus),
                relate(&caching, UsedWith, &attention, "See [[attention]].", opus),
            ],
            "one",
        )
        .await;
        assert_eq!(applied.relations.len(), 5, "{:?}", applied.skipped);
        assert_eq!(applied.skipped.len(), 4, "{:?}", applied.skipped);
        assert!(
            applied
                .skipped
                .iter()
                .any(|s| s.contains("would make a loop"))
        );
        assert!(
            applied
                .relations
                .iter()
                .all(|r| r.state == RelationState::Live)
        );

        // The broader category is what it's a kind of, not what it rests on.
        let applied = send(
            &g,
            vec![relate(
                &caching,
                KindOf,
                &cache,
                "It's caching of a prompt's start.",
                opus,
            )],
            "two",
        )
        .await;
        assert_eq!(applied.relations[0].state, RelationState::Live);
        let applied = send(
            &g,
            vec![relate(&caching, RestsOn, &cache, "Again.", opus)],
            "three",
        )
        .await;
        assert!(
            applied.skipped[0].contains("is a kind of"),
            "{:?}",
            applied.skipped
        );

        let client = g.client(false);
        let got = client
            .relations(&[caching.clone(), set.clone()])
            .await
            .unwrap();
        let c = &got.relations[&caching];
        assert_eq!(c.down.len(), 1, "caching rests on the cache only, now");
        assert_eq!(c.down[0].idea.name, "Transformer key-value cache");
        assert_eq!(c.down[0].idea.depth, Some(2));
        assert_eq!(c.kind_of[0].idea.id, cache);
        assert_eq!(c.alternatives[0].idea.id, rag);
        assert_eq!(
            c.grounds
                .iter()
                .map(|n| n.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Set"]
        );
        let s = &got.relations[&set];
        assert_eq!(s.up.count, 1);
        assert!(
            s.grounds.is_empty(),
            "a first principle comes down to nothing"
        );
        let r = &client
            .relations(std::slice::from_ref(&rag))
            .await
            .unwrap()
            .relations[&rag];
        assert_eq!(r.alternatives[0].idea.id, caching, "both ways");
        let ideas = client.ideas(&[caching.clone(), set.clone()]).await.unwrap();
        let depth = |id: &str| ideas.ideas.iter().find(|i| i.id == id).unwrap().depth;
        assert_eq!((depth(&caching), depth(&set)), (Some(3), Some(0)));
        assert!(ideas.ideas.iter().find(|i| i.id == set).unwrap().foundation);
        let page = client.up(&attention, None).await.unwrap();
        assert_eq!(page.up[0].idea.id, kv);
        assert!(
            client
                .foundations()
                .await
                .unwrap()
                .ideas
                .iter()
                .any(|n| n.id == set)
        );
    }

    #[tokio::test]
    async fn a_relation_is_agreed_by_another_model_and_once_taken_back_stays_back() {
        use RelationKind::*;
        let g = StandIn::start().await.unwrap();
        let keys = g.add("Idempotency keys", "A key per request.", IdeaKind::Concept);
        let failure = g.add("Partial failure", "Some parts fail.", IdeaKind::Concept);
        let store = g.add("Key-value store", "Values by key.", IdeaKind::Concept);
        let why = "A client can't tell a lost request from a lost reply.";
        send(
            &g,
            vec![relate(
                &keys,
                Answers,
                &failure,
                why,
                Some("anthropic/claude-opus-5.5"),
            )],
            "a",
        )
        .await;
        let again = send(
            &g,
            vec![relate(
                &keys,
                Answers,
                &failure,
                why,
                Some("openai/gpt-6.1-sol"),
            )],
            "b",
        )
        .await;
        assert_eq!(again.relations[0].state, RelationState::Settled);

        send(
            &g,
            vec![relate(
                &keys,
                UsedWith,
                &store,
                "Keys are kept in one.",
                None,
            )],
            "c",
        )
        .await;
        let back = send(
            &g,
            vec![Change::Unrelate {
                from: store.clone(),
                kind: UsedWith,
                to: keys.clone(),
                why: "Any store does.".into(),
            }],
            "d",
        )
        .await;
        assert_eq!(
            back.relations[0].state,
            RelationState::Retracted,
            "found either way round"
        );
        let again = send(
            &g,
            vec![relate(
                &keys,
                UsedWith,
                &store,
                "Keys are kept in one.",
                None,
            )],
            "e",
        )
        .await;
        assert!(
            again.skipped[0].contains("taken back"),
            "{:?}",
            again.skipped
        );
        let got = g
            .client(false)
            .relations(std::slice::from_ref(&keys))
            .await
            .unwrap();
        assert!(got.relations[&keys].used_with.is_empty());
        assert_eq!(
            got.relations[&keys].answers[0].state,
            RelationState::Settled
        );
    }

    #[tokio::test]
    async fn an_idea_rests_on_three_at_most_and_a_fourth_waits() {
        let g = StandIn::start().await.unwrap();
        let a = g.add("Write-ahead logging", "Log first.", IdeaKind::Concept);
        let under: Vec<String> = [
            "Durability",
            "Append-only log",
            "Database transaction",
            "Memory hierarchy",
        ]
        .iter()
        .map(|n| g.add(n, "", IdeaKind::Concept))
        .collect();
        let changes = under
            .iter()
            .map(|u| relate(&a, RelationKind::RestsOn, u, "It needs it to work.", None))
            .collect();
        let applied = send(&g, changes, "x").await;
        let states: Vec<_> = applied.relations.iter().map(|r| r.state).collect();
        assert_eq!(states[..3], [RelationState::Live; 3]);
        assert_eq!(states[3], RelationState::Proposed);
        assert_eq!(
            g.client(false)
                .relations(std::slice::from_ref(&a))
                .await
                .unwrap()
                .relations[&a]
                .down
                .len(),
            3
        );
    }

    #[tokio::test]
    async fn merged_ideas_redirect_and_unknown_ones_are_missing() {
        let g = StandIn::start().await.unwrap();
        let a = g.add("Write-ahead logging", "", IdeaKind::Concept);
        let b = g.add(
            "WAL",
            "Writing down a change before making it.",
            IdeaKind::Concept,
        );
        g.merge(&a, &b);
        let client = g.client(false);
        let unknown = "kelve:i0000000000zz".to_string();
        let got = client.ideas(&[a.clone(), unknown.clone()]).await.unwrap();
        assert_eq!(got.redirects[&a], b);
        assert_eq!(got.ideas[0].id, b);
        assert_eq!(got.missing, vec![unknown]);
        assert_eq!(client.entry(&b, None).await.unwrap(), None);
        assert_eq!(client.entry("not an id", None).await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_idea_found_to_be_two_answers_with_both() {
        let g = StandIn::start().await.unwrap();
        let n = g.add(
            "Normalization",
            "Making things comparable by a standard form.",
            IdeaKind::Concept,
        );
        let now = g.split(
            &n,
            [
                (
                    "Unicode normalization",
                    "Making text that looks the same compare as the same.",
                ),
                (
                    "Normalization (statistics)",
                    "Rescaling values to a common range.",
                ),
            ],
        );
        assert_eq!(now.len(), 2);
        let client = g.client(false);
        let got = client.ideas(std::slice::from_ref(&n)).await.unwrap();
        assert_eq!(got.split[&n], now);
        assert!(got.ideas.is_empty() && got.missing.is_empty());
        let got = client.relations(std::slice::from_ref(&n)).await.unwrap();
        assert_eq!(got.split[&n], now);
        let got = client.ideas(&now).await.unwrap();
        assert_eq!(got.ideas.len(), 2);
    }

    #[tokio::test]
    async fn a_kelve_signs_in_with_a_code_only_its_verifier_exchanges() {
        let g = StandIn::start().await.unwrap();
        let client = g.client(false);
        let redirect = "http://127.0.0.1:9/signed-in";
        let verifier = "a-verifier-only-this-kelve-holds-a-verifier-only";
        let address = client
            .sign_in_address(redirect, "s1", &pkce_challenge(verifier), None)
            .unwrap();
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let back = http.get(&address).send().await.unwrap();
        let back = reqwest::Url::parse(back.headers()["location"].to_str().unwrap()).unwrap();
        let q: BTreeMap<_, _> = back.query_pairs().into_owned().collect();
        assert_eq!(q["state"], "s1");
        let asked = |verifier: &str| SignInCode {
            code: q["code"].clone(),
            code_verifier: verifier.into(),
            redirect_uri: redirect.into(),
        };

        assert!(client.exchange(&asked("someone-else's")).await.is_err());
        // The wrong verifier spent the code: a code is taken once.
        let address = client
            .sign_in_address(redirect, "s2", &pkce_challenge(verifier), None)
            .unwrap();
        let back = http.get(&address).send().await.unwrap();
        let back = reqwest::Url::parse(back.headers()["location"].to_str().unwrap()).unwrap();
        let q: BTreeMap<_, _> = back.query_pairs().into_owned().collect();
        let signed = client
            .exchange(&SignInCode {
                code: q["code"].clone(),
                code_verifier: verifier.into(),
                redirect_uri: redirect.into(),
            })
            .await
            .unwrap();
        assert_eq!(signed.key, TOKEN);
        assert!(!format!("{signed:?}").contains(TOKEN), "a key never shows");

        let writer = client.with_token(Some(signed.key));
        assert!(writer.can_write());
        writer.sign_out().await.unwrap();
        assert_eq!(g.signed_out(), 1);
    }

    #[tokio::test]
    async fn relations_followed_most_come_first_and_the_account_sees_what_it_added() {
        use RelationKind::*;
        let g = StandIn::start().await.unwrap();
        let jitter = g.add("Jitter", "A random share of a wait.", IdeaKind::Concept);
        let random = g.add("Randomness", "Choosing unpredictably.", IdeaKind::Concept);
        let time = g.add("Time", "What clocks measure.", IdeaKind::Concept);
        g.relate(&jitter, RestsOn, &random, "It picks at random.");
        g.relate(&jitter, RestsOn, &time, "It's a wait.");
        let client = reqwest::Client::new();
        let down = |r: &serde_json::Value| -> Vec<String> {
            r["relations"][&jitter]["down"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x["idea"]["name"].as_str().unwrap().to_string())
                .collect()
        };
        let get = || async {
            client
                .get(format!("{}/v1/relations?ids={jitter}", g.url))
                .bearer_auth(TOKEN)
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap()
        };
        assert_eq!(down(&get().await), ["Randomness", "Time"], "as they were");
        let sent = client
            .post(format!("{}/v1/follows", g.url))
            .json(&crate::Follows {
                follows: vec![crate::Followed {
                    from: jitter.clone(),
                    kind: RestsOn,
                    to: time.clone(),
                }],
            })
            .send()
            .await
            .unwrap();
        assert_eq!(sent.status(), 202, "no account needed");
        assert_eq!(
            down(&get().await),
            ["Time", "Randomness"],
            "the followed first"
        );
        let added = client
            .get(format!("{}/v1/account/added", g.url))
            .send()
            .await
            .unwrap();
        assert_eq!(added.status(), 401, "only for the account");
    }

    #[tokio::test]
    async fn the_graph_is_read_only_by_someone_signed_in_and_never_as_them() {
        let g = StandIn::start().await.unwrap();
        let id = g.add("Jitter", "A random share of a wait.", IdeaKind::Concept);
        assert_eq!(
            g.stranger()
                .ideas(std::slice::from_ref(&id))
                .await
                .unwrap_err(),
            Error::SignIn
        );
        assert_eq!(
            g.stranger().terms().await.map(|_| ()),
            Ok(()),
            "the terms are for anyone"
        );
        // Signed in, each read spends a token signed blind, which says
        // nothing of whose; one read with once doesn't read again.
        let (pending, ask) = crate::tokens::blind(ISSUER.key(), 1).unwrap();
        let signed: crate::tokens::TokenGrant = reqwest::Client::new()
            .post(format!("{}/v1/account/tokens", g.url))
            .bearer_auth(TOKEN)
            .json(&ask)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let token = crate::tokens::finish(pending, &signed).unwrap().remove(0);
        let read = || async {
            reqwest::Client::new()
                .get(format!("{}/v1/ideas?ids={id}", g.url))
                .header("authorization", format!("PrivateToken token={token}"))
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        };
        assert_eq!(read().await, 200);
        assert_eq!(read().await, 401, "spent");
        let member = g.client(true);
        assert_eq!(member.ideas(&[id]).await.unwrap().ideas.len(), 1);
    }

    #[tokio::test]
    async fn a_lesson_is_live_at_once_and_the_same_words_confirm_one_there() {
        let g = StandIn::start().await.unwrap();
        let rand = g.add("rand", "A Rust library for random numbers.", IdeaKind::Tool);
        let theirs = g.learned(
            Lesson {
                about: vec![rand.clone()],
                says: "rand 0.9 renamed `thread_rng()` to `rng()`; the old name warns, then fails."
                    .into(),
                act: "Call `rand::rng()` instead.".into(),
                example: None,
                holds_when: "rand 0.9 and later.".into(),
                backed_by: None,
                seen_as: None,
            },
            Record::default(),
        );
        let client = g.client(true);
        let mut mine = Lesson {
            about: vec![rand.clone()],
            says: "`SeedableRng::from_entropy` became `from_os_rng` in rand 0.9.".into(),
            act: "Call `from_os_rng()` to seed from the system.".into(),
            example: None,
            holds_when: "rand 0.9 and later.".into(),
            backed_by: None,
            seen_as: None,
        };
        let page = client.add_lesson(&mine, "one").await.unwrap();
        assert_eq!(page.state, LessonState::Live);
        assert_eq!(
            client.add_lesson(&mine, "one").await.unwrap().id,
            page.id,
            "once"
        );
        let same = Lesson {
            says: "Rand 0.9 renamed thread_rng() to rng(): the old name warns, then fails!".into(),
            act: "Call `rand::rng()` instead.".into(),
            ..mine.clone()
        };
        let page = client.add_lesson(&same, "two").await.unwrap();
        assert_eq!(page.state, LessonState::Same);
        assert_eq!(page.same_as.as_deref(), Some(theirs.as_str()));
        let about = client.lessons(std::slice::from_ref(&rand)).await.unwrap();
        assert_eq!(about.lessons.len(), 2, "the same isn't kept twice");
        assert_eq!(about.lessons[0].id, theirs, "confirmed, so first");
        assert_eq!(about.lessons[0].record.held, 1);

        mine.says = "You MUST call `rng()`.".into();
        assert!(matches!(
            client.add_lesson(&mine, "three").await,
            Err(crate::client::Error::Refused { status: 422, .. })
        ));
        mine.says = "Something else about it.".into();
        mine.about = vec!["kelve:i0000000000ff".into()];
        assert!(
            client.add_lesson(&mine, "four").await.is_err(),
            "not an idea there"
        );

        let outcomes = Outcomes {
            outcomes: vec![LessonOutcome {
                lesson: theirs.clone(),
                outcome: Outcome::Held,
                shown: kelve_core::lesson::Shown::Passed,
                withheld: false,
                chance: 950,
            }],
        };
        client.send_outcomes(&outcomes).await.unwrap();
        client.send_outcomes(&outcomes).await.unwrap();
        let page = client.lesson(&theirs).await.unwrap().unwrap();
        assert_eq!(page.record.held, 2, "once for the account");
        assert_eq!(page.standing, kelve_core::lesson::Standing::Confirmed);
        assert!(
            g.client(false).send_outcomes(&outcomes).await.is_err(),
            "as an account"
        );
        // A lesson still being checked shows only to its author.
        g.hold_checks(true);
        let checking = client
            .add_lesson(
                &Lesson {
                    about: vec![rand.clone()],
                    says: "`StdRng::seed_from_u64` makes a test's random waits repeat.".into(),
                    act: "Seed tests' random sources from a fixed number.".into(),
                    ..mine.clone()
                },
                "five",
            )
            .await
            .unwrap();
        assert_eq!(checking.state, LessonState::Checking);
        assert!(
            client.lesson(&checking.id).await.unwrap().is_some(),
            "to its author"
        );
        assert!(
            g.client(false)
                .lesson(&checking.id)
                .await
                .unwrap()
                .is_none(),
            "to no one else"
        );
    }
}
