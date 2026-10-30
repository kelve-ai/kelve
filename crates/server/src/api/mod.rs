//! The reader's API, under `/api` (docs/architecture.md). Every route
//! checks who's asking first: the person, signed in on this browser (`Me`),
//! or kelve's own command line on this computer (`Agent`), which `kelve
//! mcp` speaks for the person's coding agent. It can ask kelve to open
//! something and learn whether the person sees it, take kelve's jobs and
//! hand in their answers, see what's kept for it, and say an agent kelve
//! handed the note for agents to is done. It never reads the person's
//! notes, or what kelve keeps beyond a job's material.

pub mod agent;
pub mod graph;
pub mod me;
pub mod outline;
pub mod own;
pub mod problems;
pub mod reader;
pub mod settings;
pub mod stream;

use axum::Router;
use axum::routing::{delete, get, post};

use crate::state::Shared;

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/me", get(me::me))
        .route("/settings", get(settings::get).post(settings::post))
        .route("/counts", get(settings::counts))
        .route("/marks", get(settings::marks).post(settings::set_marks))
        .route("/follows", post(crate::follows::follow))
        .route("/graph/added", get(graph::added))
        .route("/graph/today", get(graph::today))
        .route(
            "/graph/account",
            get(graph::account).delete(graph::sign_out),
        )
        .route("/graph/signin", post(graph::sign_in))
        .route("/graph/terms", get(graph::terms))
        .route("/graph/agree", post(graph::agree))
        .route("/graph/account/delete", post(graph::delete_account))
        .route("/everything", delete(me::everything))
        .route("/stop", post(me::stop))
        .route("/update", get(crate::updates::for_me))
        .route("/agent/update", get(crate::updates::for_agent))
        .route("/agent/update/tell", post(crate::updates::tell_agent))
        .route("/stream", get(stream::stream))
        .route("/signout", post(crate::auth::sign_out))
        .route("/signin-link", post(crate::auth::link))
        .route("/agent/hello", post(agent::hello))
        .route("/agent/open", post(agent::open))
        .route("/agent/next", post(agent::next))
        .route("/agent/answer", post(agent::answer))
        .route("/agent/kept", get(agent::kept))
        .route("/agent/notes", get(agent::notes))
        .route("/agent/notes/new", post(agent::notes_new))
        .route("/agent/note-done", post(agent::note_done))
        .route("/agent/marks", get(agent::marks).post(agent::set_marks))
        .route("/agent/hook", post(agent::hook_changed))
        .route("/agent/moment", post(crate::lessons::moment))
        .route("/agent/lessons", post(crate::lessons::know))
        .route("/agent/held", post(crate::lessons::held))
        .route("/lessons", get(crate::lessons::settings))
        .route("/lessons/all", get(crate::lessons::of_idea))
        .route("/lessons/{id}", get(crate::lessons::one))
        .route("/lessons/{id}/didnt", post(crate::lessons::didnt))
        .route("/learned/{id}", get(crate::lessons::learned_one))
        .route("/learned/{id}/stop", post(crate::lessons::stop))
        .route("/learned/{id}/send", post(crate::lessons::send_now))
        .route("/kept", get(agent::kept_for_me))
        .route("/list", get(reader::list))
        .route("/list/first", post(reader::list_first))
        .route("/list/off", post(reader::list_off))
        .route("/kept/{id}", delete(agent::put_aside))
        .route("/open", post(reader::open))
        .route("/pick", post(reader::pick))
        .route("/notes/show", post(reader::show_notes))
        .route("/opened", get(reader::lately))
        .route("/opened/{id}", get(reader::one))
        .route("/opened/{id}/again", post(reader::open_again))
        .route("/opened/{id}/folder", get(reader::folder))
        .route("/opened/{id}/file", get(reader::file))
        .route("/opened/{id}/image", get(reader::image))
        .route("/opened/{id}/change", get(reader::change))
        .route("/opened/{id}/diff", get(reader::diff))
        .route("/opened/{id}/origins", get(reader::origins))
        .route("/opened/{id}/part", get(reader::part))
        .route("/opened/{id}/way/part", post(reader::read_part))
        .route("/opened/{id}/way/step", post(reader::read_step))
        .route("/opened/{id}/ready", get(reader::ready))
        .route("/opened/{id}/written", get(reader::written))
        .route("/opened/{id}/asked", post(reader::asked))
        .route("/opened/{id}/plainly", get(reader::plainly))
        .route("/opened/{id}/plainly/folder", get(reader::plainly_folder))
        .route("/opened/{id}/plainly/change", get(reader::plainly_change))
        .route("/opened/{id}/lessons", get(reader::lessons))
        .route(
            "/opened/{id}/notes",
            get(reader::notes)
                .post(reader::keep_note)
                .delete(reader::forget_note),
        )
        .route("/opened/{id}/note", get(reader::note))
        .route("/ideas/{key}", get(reader::idea))
        .route("/read", get(reader::read))
        .route("/ideas/{key}/outline", get(outline::outline))
        .route("/ideas/{key}/notes", get(reader::notes_on_idea))
        .route("/ideas/{key}/write", post(reader::write_again))
        .route("/ideas/{key}/flag", post(reader::flag))
        .route("/ideas/{key}/ask", post(reader::ask))
        .route("/search", get(reader::search))
        .route("/problems", get(problems::list).post(problems::from_page))
        .route("/problems/{id}", delete(problems::forget))
}
