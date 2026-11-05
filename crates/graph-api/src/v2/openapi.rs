//! Version 2's OpenAPI document, made from the same types the client and
//! the service speak, so it can't drift from either. `cargo test` writes it
//! to `docs/hosted-api.json`. The paths below only describe the routes: the
//! service serves them, and the client calls them.

#![allow(dead_code)]

use utoipa::openapi::security::{Http, HttpAuthScheme, SecurityScheme};
use utoipa::{Modify, OpenApi};

use super::{
    Account, AgentOffer, Allowance, Character, Confirm, Credential, Deleted, Estimate,
    ExampleCheck, ExampleFile, ExampleTask, HandleAsk, HandleCheck, KindOffer, ModelOffer, Offer,
    PageSettings, Palette, Paused, RunRequest, RunResult, RunState, RunStatus, Score, Scores,
    SignedIn, SkillFile, TaskProgress, TaskState, Verdict,
};
use crate::{Problem, SignInCode};

/// The document.
pub fn document() -> utoipa::openapi::OpenApi {
    Doc::openapi()
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "kelve’s hosted service",
        version = "2",
        description = "Accounts, pages, characters, verified runs and scores, for kelve. A person signs in from their kelve with GitHub or Google, and gets an account key, sent as a bearer token with every call. A verified run takes the person's skills, secrets and paths stripped first, runs their agent with them against a standard agent on the same model, on tasks nobody sees, and deletes the skills as it ends. Nothing is public until the person publishes: then their page, at /@{handle}, shows their agent's character, its name and the scores they chose, never their skills or work. Errors are RFC 9457 problem details, with a sentence to show.",
        license(name = "Apache-2.0", identifier = "Apache-2.0"),
    ),
    servers((url = "https://graph.kelve.ai")),
    paths(
        sign_in,
        sign_in_token,
        sign_out,
        account,
        delete_account,
        check_handle,
        choose_handle,
        set_page,
        character,
        set_character,
        scores,
        publish,
        unpublish,
        offer,
        estimate,
        confirm,
        status,
        cancel,
        page,
        score_page,
        preview,
    ),
    components(schemas(
        Account, Allowance, SignInCode, SignedIn, Deleted, HandleAsk, HandleCheck, PageSettings,
        Character, Palette, Offer, KindOffer, ExampleTask, ExampleCheck, ExampleFile, AgentOffer,
        ModelOffer, RunRequest, SkillFile,
        Estimate, Confirm, Credential, RunState, TaskState, Verdict, TaskProgress, RunStatus, Paused, RunResult, Score, Scores,
        Problem
    )),
    modifiers(&Key),
)]
struct Doc;

/// The account key every call but signing in is sent with.
struct Key;

impl Modify for Key {
    fn modify(&self, doc: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = doc.components.as_mut() {
            components.add_security_scheme(
                "key",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
        }
    }
}

/// Starts signing in: on to GitHub, or Google with `provider=google`, then
/// back to the person's kelve with a one-time code.
#[utoipa::path(
    get,
    path = "/v2/signin",
    params(
        ("provider" = Option<String>, Query, description = "`github` (the default) or `google`."),
        ("redirect_uri" = String, Query, description = "The loopback address the person's kelve waits at."),
        ("state" = String, Query),
        ("code_challenge" = String, Query),
        ("code_challenge_method" = String, Query, description = "`S256`."),
    ),
    responses((status = 303, description = "On to GitHub or Google."))
)]
fn sign_in() {}

/// Exchanges the one-time code for an account key, once.
#[utoipa::path(
    post,
    path = "/v2/signin/token",
    request_body = SignInCode,
    responses((status = 200, body = SignedIn), (status = 400, body = Problem))
)]
fn sign_in_token() {}

/// Signs out: the key it's sent with stops working.
#[utoipa::path(delete, path = "/v2/signin", security(("key" = [])), responses((status = 204)))]
fn sign_out() {}

/// The account: its page, and the runs left today.
#[utoipa::path(
    get,
    path = "/v2/account",
    security(("key" = [])),
    responses((status = 200, body = Account), (status = 401, body = Problem))
)]
fn account() {}

/// Deletes the account, with its page, its character and every score.
/// Every key it has stops working, and its runs are stopped and forgotten.
#[utoipa::path(
    delete,
    path = "/v2/account",
    security(("key" = [])),
    responses((status = 200, body = Deleted), (status = 401, body = Problem))
)]
fn delete_account() {}

/// Whether an address can be had, as it's typed: its form, whether
/// someone has it, whether kelve keeps it, and whether it's fit to show.
#[utoipa::path(
    post,
    path = "/v2/handles/check",
    request_body = HandleAsk,
    security(("key" = [])),
    responses((status = 200, body = HandleCheck), (status = 401, body = Problem))
)]
fn check_handle() {}

/// Takes an address for the account's page. An account has one; choosing
/// another lets the old one go.
#[utoipa::path(
    put,
    path = "/v2/account/handle",
    request_body = HandleAsk,
    security(("key" = [])),
    responses(
        (status = 200, body = Account),
        (status = 409, description = "Someone has it.", body = Problem),
        (status = 422, description = "Not an address it can have, and why.", body = Problem),
    )
)]
fn choose_handle() {}

/// Names the agent on the page, and publishes the page or takes it down.
#[utoipa::path(
    put,
    path = "/v2/account/page",
    request_body = PageSettings,
    security(("key" = [])),
    responses(
        (status = 200, body = Account),
        (status = 409, description = "No address chosen yet.", body = Problem),
        (status = 422, description = "A name it can't show, and why.", body = Problem),
    )
)]
fn set_page() {}

/// The agent's character, as the service has it.
#[utoipa::path(
    get,
    path = "/v2/account/character",
    security(("key" = [])),
    responses((status = 200, body = Character), (status = 404, body = Problem))
)]
fn character() {}

/// Gives the agent its character, checked first: pixels only, on the
/// grid, in a few colours.
#[utoipa::path(
    put,
    path = "/v2/account/character",
    request_body = Character,
    security(("key" = [])),
    responses((status = 200, body = Account), (status = 422, body = Problem))
)]
fn set_character() {}

/// The account's scores, newest first, published or not.
#[utoipa::path(
    get,
    path = "/v2/scores",
    security(("key" = [])),
    responses((status = 200, body = Scores), (status = 401, body = Problem))
)]
fn scores() {}

/// Puts a score on the account's page, publishing the page.
#[utoipa::path(
    put,
    path = "/v2/scores/{id}/published",
    params(("id" = String, Path)),
    security(("key" = [])),
    responses(
        (status = 200, body = Score),
        (status = 404, body = Problem),
        (status = 409, description = "No address chosen yet.", body = Problem),
    )
)]
fn publish() {}

/// Takes a score off the account's page.
#[utoipa::path(
    delete,
    path = "/v2/scores/{id}/published",
    params(("id" = String, Path)),
    security(("key" = [])),
    responses((status = 200, body = Score), (status = 404, body = Problem))
)]
fn unpublish() {}

/// What a verified run can be: kinds of work, agents and models.
#[utoipa::path(
    get,
    path = "/v2/runs/offer",
    security(("key" = [])),
    responses((status = 200, body = Offer))
)]
fn offer() {}

/// Sends the skills for a run, and says what it will take. Nothing runs
/// until it's confirmed.
#[utoipa::path(
    post,
    path = "/v2/runs",
    request_body = RunRequest,
    security(("key" = [])),
    responses(
        (status = 201, body = Estimate),
        (status = 413, body = Problem),
        (status = 422, description = "Skills, or a kind, agent or model, it can't take, and why.", body = Problem),
        (status = 429, description = "No runs left today: `Retry-After` is the seconds to midnight UTC.", body = Problem),
        (status = 503, description = "Not taking runs for a while.", body = Problem),
    )
)]
fn estimate() {}

/// Starts a run the person confirmed, paid for on their own key for the
/// model's provider. The key is used for this run only: never kept, never
/// logged. Confirming twice starts it once.
#[utoipa::path(
    post,
    path = "/v2/runs/{id}/confirm",
    params(("id" = String, Path)),
    request_body = Confirm,
    security(("key" = [])),
    responses(
        (status = 202, body = RunStatus),
        (status = 404, body = Problem),
        (status = 422, description = "The provider turned the key away, and what to do.", body = Problem),
        (status = 429, body = Problem),
    )
)]
fn confirm() {}

/// How a run stands, task by task, and its result once it's finished.
#[utoipa::path(
    get,
    path = "/v2/runs/{id}",
    params(("id" = String, Path)),
    security(("key" = [])),
    responses((status = 200, body = RunStatus), (status = 404, body = Problem))
)]
fn status() {}

/// Stops a run, or drops an estimate. Its skills are deleted.
#[utoipa::path(
    delete,
    path = "/v2/runs/{id}",
    params(("id" = String, Path)),
    security(("key" = [])),
    responses((status = 200, body = RunStatus), (status = 404, body = Problem))
)]
fn cancel() {}

/// A person's page, as HTML: their agent's character and name, and the
/// scores they published. 404, with a page saying so, when there's none.
#[utoipa::path(
    get,
    path = "/@{handle}",
    params(("handle" = String, Path)),
    responses((status = 200, content_type = "text/html"), (status = 404, content_type = "text/html"))
)]
fn page() {}

/// A published score's own page, as HTML.
#[utoipa::path(
    get,
    path = "/s/{id}",
    params(("id" = String, Path)),
    responses((status = 200, content_type = "text/html"), (status = 404, content_type = "text/html"))
)]
fn score_page() {}

/// A published score's link preview, as a 1200 by 630 PNG.
#[utoipa::path(
    get,
    path = "/s/{id}.png",
    params(("id" = String, Path)),
    responses((status = 200, content_type = "image/png"), (status = 404))
)]
fn preview() {}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    /// Writes the document into the docs, as version 1's is: CI fails if
    /// it's out of date.
    #[test]
    fn the_document_is_written_into_the_open_specs() {
        let json = super::document()
            .to_pretty_json()
            .expect("the document as JSON");
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/hosted-api.json");
        let now = format!("{json}\n");
        if std::fs::read_to_string(path).ok().as_deref() != Some(now.as_str()) {
            std::fs::write(path, now).expect("written");
        }
        let doc: serde_json::Value = serde_json::from_str(&json).expect("JSON");
        let schemas = &doc["components"]["schemas"];
        for name in [
            "Account",
            "Character",
            "RunRequest",
            "RunStatus",
            "Paused",
            "RunResult",
            "Score",
        ] {
            assert!(schemas.get(name).is_some(), "{name} is described");
        }
        assert!(doc["paths"]["/v2/runs"]["post"]["security"].is_array());
        assert!(doc["paths"]["/v1/changesets"].is_null(), "only version 2");
    }
}
