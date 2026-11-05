//! The stand-in for kelve's hosted service on its own, for working on kelve
//! without the real one: `kelve-graph-stand-in [--port 4620]`. It starts
//! empty and forgets everything when it stops.

use std::net::SocketAddr;
use std::process::ExitCode;

use kelve_graph_api::stand_in::StandIn;

const USAGE: &str = "Use: kelve-graph-stand-in [--port 4620]";

#[tokio::main]
async fn main() -> ExitCode {
    let mut port: u16 = 4620;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--port", Some(p)) if p.parse::<u16>().is_ok() => port = p.parse().unwrap_or(port),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let hosted = match StandIn::serve(addr).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("The stand-in hosted service couldn’t start: {e}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!("The stand-in hosted service is at {}.", hosted.url);
    eprintln!("It starts empty, and forgets everything when it stops.");
    eprintln!("Start kelve with KELVE_GRAPH_URL={}", hosted.url);
    match tokio::signal::ctrl_c().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
