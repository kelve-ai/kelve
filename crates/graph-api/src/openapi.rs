//! The API's OpenAPI document, made from the same types the client and the
//! graph speak, so it can't drift from either. `cargo test` writes it to
//! `docs/graph-api.json`, and the graph serves it at `/v1/openapi.json`. The
//! paths below only describe the routes: the graph serves them, and the client
//! calls them.
//!
//! The operator's routes and page are for the graph's operator, so they
//! aren't here.

#![allow(dead_code)]

use utoipa::openapi::security::{ApiKey, ApiKeyValue, Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};

use kelve_core::lesson::{Backing, Lesson, Outcome, Record, Shown, Standing};
use kelve_core::matching::{Axis, Card, Facet};
use kelve_core::relations::{RelationKind, RelationState};

use crate::{
    Account, Added, Agree, Applied, Ask, Bucket, Candidate, Candidates, Change, Changeset,
    CheckStatus, Checking, Claim, ClaimChecked, ClaimState, Counts, Dropped, EntryPage, Feed,
    FeedChange, Flag, Followed, Follows, Foundations, Idea, IdeaAsk, IdeaRelations, Ideas,
    Instructions, LessonOutcome, LessonPage, LessonState, Lessons, Manifest, MatchRequest, Node,
    Outcomes, Problem, Query, Quote, Rel, RelationRef, RelationTaken, Relations, Revise,
    SignInCode, SignedIn, Terms, Up, UpPage, WrittenBy,
};

/// The document.
pub fn document() -> utoipa::openapi::OpenApi {
    Doc::openapi()
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "kelve’s shared graph",
        version = "1",
        description = "The shared knowledge graph every kelve reads ideas and entries from, and adds them to. It holds general knowledge only: never anyone's work. Everything in it is public domain, under CC0 1.0, the graph as a database included; this API's own code is Apache-2.0. Reading and adding are for someone signed in, with GitHub or Google: a person's kelve gets an account key by signing in, has the graph sign a batch of reading tokens with it, blind, and spends one on each read: the graph knows a token is one it signed, never whose (Privacy Pass's privately verifiable tokens, RFC 9578, on the VOPRF of RFC 9497). Errors are RFC 9457 problem details.",
        license(name = "Apache-2.0", identifier = "Apache-2.0"),
    ),
    servers((url = "https://graph.kelve.ai")),
    paths(
        manifest,
        names,
        matches,
        ideas,
        idea,
        relations,
        up,
        foundations,
        entry,
        revision,
        changeset,
        changes,
        counts,
        follows,
        added,
        flags,
        asks,
        sign_in,
        signed_in_github,
        signed_in_google,
        sign_in_token,
        sign_out,
        delete_account,
        account,
        token_key,
        tokens,
        instructions,
        terms,
        agree,
        checking,
        lessons,
        lesson,
        add_lesson,
        outcomes,
    ),
    components(schemas(
        Manifest, Bucket, MatchRequest, Query, Candidates, Candidate, Ideas, Idea, EntryPage, Changeset,
        Change, Claim, Applied, Feed, FeedChange, Counts, Flag, Problem, SignInCode, SignedIn,
        RelationKind, RelationState, RelationRef, RelationTaken, Node, Rel, Up, IdeaRelations,
        Relations, UpPage, Foundations, Quote, WrittenBy, Instructions, Terms, Agree, Account, crate::tokens::TokenKey, crate::tokens::TokenAsk, crate::tokens::TokenGrant,
        Checking, CheckStatus, ClaimChecked, ClaimState, Followed, Follows, Added, Ask, IdeaAsk,
        Revise, Dropped, Lesson, Backing, Record, Standing, Outcome, Shown, LessonPage,
        LessonState, Lessons, LessonOutcome, Outcomes, Card, Facet, Axis
    )),
    modifiers(&Key),
)]
struct Doc;

/// The key a changeset is sent with.
struct Key;

impl Modify for Key {
    fn modify(&self, doc: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = doc.components.as_mut() {
            components.add_security_scheme(
                "key",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
            components.add_security_scheme(
                "token",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::with_description(
                    "Authorization",
                    "A reading token, spent once: `PrivateToken token=…`.",
                ))),
            );
        }
    }
}

/// What the graph is.
#[utoipa::path(get, path = "/v1", responses((status = 200, body = Manifest)))]
fn manifest() {}

/// One bucket of the name index: every live idea's name whose hash starts
/// with `prefix`, keyed by the whole hash. A kelve finds its own name in
/// it, so the graph never learns which name was asked.
#[utoipa::path(
    get,
    path = "/v1/names/{prefix}",
    security(("token" = []), ("key" = [])),
    params(("prefix" = String, Path, description = "The first four hex characters of the SHA-256 of the name, as names are compared.")),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),(status = 200, body = Bucket), (status = 404, body = Problem))
)]
fn names() {}

/// The ideas each name may be, in the shape of the W3C Reconciliation API.
/// Names go only in the body, so they reach no log.
#[utoipa::path(
    post,
    path = "/v1/match",
    security(("token" = []), ("key" = [])),
    request_body = MatchRequest,
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),
        (status = 200, description = "Candidates for each query, by the asker's own keys.", body = std::collections::BTreeMap<String, Candidates>),
        (status = 429, description = "Too many lookups: wait as Retry-After says.", body = Problem),
    )
)]
fn matches() {}

/// Live ideas by ID, following merges.
#[utoipa::path(
    get,
    path = "/v1/ideas",
    security(("token" = []), ("key" = [])),
    params(("ids" = String, Query, description = "Up to 100 IDs, separated by commas.")),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),(status = 200, body = Ideas))
)]
fn ideas() {}

/// One idea.
#[utoipa::path(
    get,
    path = "/v1/ideas/{id}",
    security(("token" = []), ("key" = [])),
    params(("id" = String, Path)),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),
        (status = 200, body = Idea),
        (status = 308, description = "Merged: Location says into which."),
        (status = 404, body = Problem),
        (status = 410, description = "Withdrawn.", body = Problem),
    )
)]
fn idea() {}

/// How ideas relate, both ways: for each, what it rests on (most needed
/// first), what it's a kind of, what it answers, its alternatives, what it's
/// used with, what rests on it, and the first principles it comes down to.
/// Only live and settled relations are served.
#[utoipa::path(
    get,
    path = "/v1/relations",
    security(("token" = []), ("key" = [])),
    params(("ids" = String, Query, description = "Up to 50 idea IDs, separated by commas.")),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),(status = 200, body = Relations))
)]
fn relations() {}

/// What rests on an idea, a page at a time, most read first.
#[utoipa::path(
    get,
    path = "/v1/relations/{id}/up",
    security(("token" = []), ("key" = [])),
    params(
        ("id" = String, Path),
        ("after" = Option<String>, Query, description = "The last page's `next`."),
        ("limit" = Option<u32>, Query, description = "At most 200; 50 if not said."),
    ),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),(status = 200, body = UpPage))
)]
fn up() {}

/// The ground floor: every idea that's a first principle.
#[utoipa::path(get, security(("token" = []), ("key" = [])), path = "/v1/foundations", responses((status = 401, description = "Not signed in.", body = Problem), (status = 200, body = Foundations)))]
fn foundations() {}

/// An idea's current entry: a redirect to its revision.
#[utoipa::path(
    get,
    path = "/v1/entries/{id}",
    security(("token" = []), ("key" = [])),
    params(("id" = String, Path)),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),
        (status = 307, description = "Location is the current revision."),
        (status = 404, description = "No entry yet.", body = Problem),
    )
)]
fn entry() {}

/// One revision of an entry. It never changes, and is cached for a year.
#[utoipa::path(
    get,
    path = "/v1/entries/{id}/revisions/{revision}",
    security(("token" = []), ("key" = [])),
    params(("id" = String, Path), ("revision" = String, Path)),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),(status = 200, body = EntryPage), (status = 404, body = Problem))
)]
fn revision() {}

/// Adds ideas and entries, and proposes relations and taking them back.
/// Each change is taken or refused on its own; what's refused comes back in
/// `skipped`, with why. A relation must have a why: one plain sentence,
/// at most 200 characters, saying what one idea takes from the other. "Rests
/// on" is for what someone must understand first, at most three an idea,
/// most needed first; the problem an idea exists for is "answers"; a broader
/// category is "kind of". Relations are reviewed before they're served. The
/// same Idempotency-Key always gets the first answer, so a retry is applied
/// once.
#[utoipa::path(
    post,
    path = "/v1/changesets",
    request_body = Changeset,
    params(("Idempotency-Key" = String, Header)),
    security(("key" = [])),
    responses(
        (status = 201, body = Applied),
        (status = 400, body = Problem),
        (status = 401, description = "No key, or one the graph doesn't know.", body = Problem),
        (status = 413, body = Problem),
        (status = 422, body = Problem),
        (status = 429, description = "Over a limit: Retry-After says when to try again, in seconds. Titled \"Too many requests\", it's a few seconds. Titled \"Enough for today\", the account has added as many ideas, entries or relations as it may in a day, as the detail says, and \"as a new account\" while it's a new account's limit: Retry-After is the seconds to midnight UTC, when it resets.", body = Problem),
        (status = 503, description = "Not taking changes for now: Retry-After says when to try again.", body = Problem),
    )
)]
fn changeset() {}

/// The live lessons about some ideas, best first: what people's agents
/// learned using them. Each with what came of it for projects besides its
/// author's, and how far it's confirmed.
#[utoipa::path(
    get,
    path = "/v1/lessons",
    security(("token" = []), ("key" = [])),
    params(("ids" = String, Query, description = "Up to 100 idea IDs, separated by commas.")),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),(status = 200, body = Lessons))
)]
fn lessons() {}

/// One lesson: live or the same as one, to anyone; in any other state,
/// only to its author's key. Anyone else is told there's no such lesson.
#[utoipa::path(
    get,
    path = "/v1/lessons/{id}",
    security(("token" = []), ("key" = [])),
    params(("id" = String, Path)),
    security((), ("key" = [])),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),(status = 200, body = LessonPage), (status = 404, body = Problem))
)]
fn lesson() {}

/// Adds a lesson. It's checked in code at once, then by the graph's
/// models; it's live, not confirmed yet, once they pass it. One that says
/// what a lesson already there says isn't kept again: it's answered as
/// `same`, and counted as confirming that one. The same Idempotency-Key
/// always gets the first answer.
#[utoipa::path(
    post,
    path = "/v1/lessons",
    request_body = Lesson,
    params(("Idempotency-Key" = String, Header)),
    security(("key" = [])),
    responses(
        (status = 201, body = LessonPage),
        (status = 400, body = Problem),
        (status = 401, description = "No key, or one the graph doesn't know.", body = Problem),
        (status = 422, description = "The lesson isn't one the graph takes, as the detail says.", body = Problem),
        (status = 429, description = "Over a limit, as for changesets: \"as many lessons\".", body = Problem),
    )
)]
fn add_lesson() {}

/// What came of lessons an agent was given: held, didn't, or not known,
/// and what showed it. Counted once a lesson for each account, and never
/// for its author's. Nothing else goes with it: never the problem, the
/// command or the file.
#[utoipa::path(
    post,
    path = "/v1/lessons/outcomes",
    request_body = Outcomes,
    security(("key" = [])),
    responses(
        (status = 202, description = "Counted."),
        (status = 401, body = Problem),
        (status = 422, description = "Too many in one batch.", body = Problem),
    )
)]
fn outcomes() {}

/// What became public, in order, for mirrors.
#[utoipa::path(
    get,
    path = "/v1/changes",
    security(("token" = []), ("key" = [])),
    params(("after" = Option<i64>, Query, description = "The `next` of the last page read.")),
    responses(
        (status = 401, description = "Not signed in: read with a reading token, or an account’s key.", body = Problem),(status = 200, body = Feed))
)]
fn changes() {}

/// A reader's word that an entry, a link in it, or a relation is wrong.
/// The graph's models check it again; no account is needed.
#[utoipa::path(
    post,
    path = "/v1/flags",
    request_body = Flag,
    responses(
        (status = 202, description = "Taken, for the reviewers."),
        (status = 422, description = "Not a flag the graph can take.", body = Problem),
    )
)]
fn flags() {}

/// A reader's word that an entry didn't help them understand: it needs an
/// example, it's too hard, or something's missing. Kept with the idea, with
/// no account and no address, until a revision answers it.
#[utoipa::path(
    post,
    path = "/v1/asks",
    request_body = Ask,
    responses(
        (status = 202, description = "Kept, with the idea."),
        (status = 422, description = "Not an ask the graph can take.", body = Problem),
    )
)]
fn asks() {}

/// A week's counts, which a person chose to share: with the install's random
/// ID, and never an account, a key or a reading token.
#[utoipa::path(
    post,
    path = "/v1/counts",
    request_body = Counts,
    responses(
        (status = 202, description = "Taken."),
        (status = 422, description = "Not counts the graph keeps.", body = Problem),
    )
)]
fn counts() {}

/// A person's kelve starts signing in: the browser goes on to GitHub, or
/// Google, and comes back to `redirect_uri` with a one-time code and
/// `state`. Either only tells the graph who someone is: it keeps a number
/// for the account (GitHub's id, Google's `sub`), never a name or email.
#[utoipa::path(
    get,
    path = "/v1/signin",
    params(
        ("redirect_uri" = String, Query, description = "http, to 127.0.0.1, [::1] or localhost, on a port (RFC 8252)."),
        ("state" = String, Query),
        ("code_challenge" = String, Query, description = "The S256 PKCE challenge."),
        ("code_challenge_method" = String, Query, description = "S256."),
        ("provider" = Option<String>, Query, description = "github, the default, or google."),
    ),
    responses(
        (status = 303, description = "On to GitHub, or Google."),
        (status = 400, description = "A page saying what's wrong."),
        (status = 503, description = "That way of signing in isn't set up on this graph: a page saying so."),
    )
)]
fn sign_in() {}

/// Where Google sends the browser back to, after someone signs in there.
#[utoipa::path(
    get,
    path = "/v1/signin/google",
    params(("code" = Option<String>, Query), ("state" = String, Query), ("error" = Option<String>, Query)),
    responses((status = 303, description = "Back to kelve's redirect_uri, with a one-time code and state."), (status = 400, description = "A page saying what's wrong."))
)]
fn signed_in_google() {}

/// Where GitHub sends the person back.
#[utoipa::path(
    get,
    path = "/v1/signin/github",
    params(("code" = Option<String>, Query), ("state" = String, Query)),
    responses(
        (status = 303, description = "Back to the person's kelve, with `code` and `state`, or `error=access_denied`."),
        (status = 400, description = "A page saying what's wrong."),
    )
)]
fn signed_in_github() {}

/// Exchanges a sign-in's one-time code, with its PKCE verifier, for a key
/// of this kelve's own. A code is taken once, within two minutes.
#[utoipa::path(
    post,
    path = "/v1/signin/token",
    request_body = SignInCode,
    responses((status = 200, body = SignedIn), (status = 400, body = Problem))
)]
fn sign_in_token() {}

/// Deletes the account the key is for: the graph forgets who they are,
/// stops all their keys, withdraws ideas only they added and rejects their
/// waiting entries. What others can already read stays, under the handle
/// the graph made up.
#[utoipa::path(
    delete,
    path = "/v1/account",
    security(("key" = [])),
    responses(
        (status = 200, description = "Deleted."),
        (status = 401, body = Problem),
        (status = 403, description = "Not a person's account.", body = Problem),
    )
)]
fn delete_account() {}

/// The account the key is for: the name the graph made up for them, and
/// the version of the terms they agreed to.
#[utoipa::path(
    get,
    path = "/v1/account",
    security(("key" = [])),
    responses((status = 200, body = Account), (status = 401, body = Problem))
)]
fn account() {}

/// The key the graph signs reading tokens with, the same for everyone:
/// asked for with no account, so it can't be one account's alone.
#[utoipa::path(get, path = "/v1/tokens/key", responses((status = 200, body = crate::tokens::TokenKey)))]
fn token_key() {}

/// Signs a batch of blinded reading tokens for the account, at most 100, as
/// many as it may have today, with a proof they were signed with the key
/// shown everyone. The graph never sees the tokens, so it can't tell later
/// whose one is.
#[utoipa::path(
    post,
    path = "/v1/account/tokens",
    security(("key" = [])),
    request_body = crate::tokens::TokenAsk,
    responses(
        (status = 200, body = crate::tokens::TokenGrant),
        (status = 401, body = Problem),
        (status = 429, description = "As many as the account may have today: Retry-After says when there are more.", body = Problem),
    )
)]
fn tokens() {}

/// Relations readers followed, from one idea's entry to an idea it relates
/// to: one more for each. No account, and nothing about who sent them,
/// which project or file, or when. The graph orders each idea's relations,
/// within each kind, by how often they're followed.
#[utoipa::path(
    post,
    path = "/v1/follows",
    request_body = Follows,
    responses(
        (status = 202, description = "Counted."),
        (status = 422, description = "Too many in one batch.", body = Problem),
    )
)]
fn follows() {}

/// What the account has added, and how many times its entries were served.
#[utoipa::path(
    get,
    path = "/v1/account/added",
    security(("key" = [])),
    responses((status = 200, body = Added), (status = 401, body = Problem))
)]
fn added() {}

/// What a person's agent is told to do for one kind of job: `read`,
/// `choose`, `builtOn`, `relate`, `write`, `fix`, `extend`,
/// `folders`, `note`, `plainly`, `plainlyFolder` or `plainlyChange`. The
/// graph serves them, so they improve for everyone at once.
#[utoipa::path(
    get,
    path = "/v1/instructions/{name}",
    params(("name" = String, Path)),
    responses((status = 200, body = Instructions), (status = 404, body = Problem))
)]
fn instructions() {}

/// What a person agrees to before anything they add goes into the graph.
#[utoipa::path(get, path = "/v1/terms", responses((status = 200, body = Terms)))]
fn terms() {}

/// Agrees to the terms, by their version. The graph takes nothing from an
/// account that hasn't agreed to the terms as they are now.
#[utoipa::path(
    post,
    path = "/v1/terms",
    request_body = Agree,
    security(("key" = [])),
    responses(
        (status = 200, body = Account),
        (status = 401, body = Problem),
        (status = 409, description = "Not the terms as they are now.", body = Problem),
    )
)]
fn agree() {}

/// How the graph's check of an entry stands, for the account that wrote
/// it: claim by claim, as each is checked against its pages.
#[utoipa::path(
    get,
    path = "/v1/revisions/{revision}/check",
    params(("revision" = String, Path)),
    security(("key" = [])),
    responses((status = 200, body = Checking), (status = 401, body = Problem), (status = 404, body = Problem))
)]
fn checking() {}

/// Signs out: the key it's sent with stops working.
#[utoipa::path(
    delete,
    path = "/v1/signin",
    security(("key" = [])),
    responses((status = 204))
)]
fn sign_out() {}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    /// Writes the document into the docs, as `cargo test` writes the web app's
    /// types: CI fails if it's out of date.
    #[test]
    fn the_document_is_written_into_the_open_specs() {
        let json = super::document()
            .to_pretty_json()
            .expect("the document as JSON");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/graph-api.json");
        let now = format!("{json}\n");
        if std::fs::read_to_string(path).ok().as_deref() != Some(now.as_str()) {
            std::fs::write(path, now).expect("written");
        }
        let doc: serde_json::Value = serde_json::from_str(&json).expect("JSON");
        let schemas = &doc["components"]["schemas"];
        for name in ["Changeset", "Entry", "Body", "SignInCode", "Feed"] {
            assert!(schemas.get(name).is_some(), "{name} is described");
        }
        assert!(doc["paths"]["/v1/changesets"]["post"]["security"].is_array());
    }
}
