//! The stand-in graph on its own, for working on kelve without the real
//! graph: `kelve-graph-stand-in [--port 4620] [--from <folder>]`. With a
//! folder, it starts from what's there, in the shape of the real graph's
//! dumps, and keeps what it's given there; without one, it starts empty
//! and forgets everything when it stops.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use kelve_graph_api::stand_in::{StandIn, TOKEN};

const USAGE: &str = "Use: kelve-graph-stand-in [--port 4620] [--from <folder>]";

#[tokio::main]
async fn main() -> ExitCode {
    let mut port: u16 = 4620;
    let mut folder: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--port", Some(p)) if p.parse::<u16>().is_ok() => port = p.parse().unwrap_or(port),
            ("--from", Some(f)) => folder = Some(PathBuf::from(f)),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let graph = match StandIn::serve(addr, folder.clone()).await {
        Ok(g) => g,
        Err(e) => {
            eprintln!("The stand-in graph couldn’t start: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (ideas, entries) = graph.held();
    eprintln!("The stand-in graph is at {}.", graph.url);
    match &folder {
        Some(f) => eprintln!(
            "It holds {ideas} ideas and {entries} entries, and keeps what it's given in {}.",
            f.display()
        ),
        None => eprintln!("It starts empty, and forgets everything when it stops."),
    }
    eprintln!("Start kelve with KELVE_GRAPH_URL={}", graph.url);
    eprintln!("and KELVE_GRAPH_KEY={TOKEN} to add to it.");
    match tokio::signal::ctrl_c().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
