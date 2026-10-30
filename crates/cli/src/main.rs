//! The `kelve` command: open a folder, a file or a change in kelve, and run
//! kelve itself.

mod client;
mod config;
mod jobs;
mod mcp;
mod moment;
mod notes;
mod open;
mod setup;
mod status;
mod style;
mod update;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "kelve",
    version,
    about = "See what anything is built on, and how and why it works.",
    styles = style::HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

// Each command's help starts with a line short enough for any terminal;
// what follows shows in its own `--help`, kept as written, so it fits too.
#[derive(Debug, Subcommand)]
enum Command {
    /// Run kelve in this terminal, and open it in your browser
    ///
    /// `kelve open` starts kelve by itself, in the background; this runs it
    /// here instead, showing what it does. kelve reads its settings from
    /// the environment: see docs/self-hosting.md.
    #[command(verbatim_doc_comment)]
    Serve {
        /// Started by `kelve open`, in the background.
        #[arg(long, hide = true)]
        background: bool,
    },
    /// Open a folder or a file in kelve
    ///
    /// kelve opens in your browser on it, marking the ideas it's built on,
    /// starting first if it isn't running. With no path, it opens kelve
    /// itself. Either way, with a new link that signs you in, once.
    #[command(verbatim_doc_comment)]
    Open {
        /// A folder or a file: `.` for this folder.
        path: Option<std::path::PathBuf>,
    },
    /// Open a change in kelve: what isn't committed, a branch, or a commit
    ///
    ///   kelve diff          what you haven't committed yet
    ///   kelve diff main     this branch, against where it left main
    ///   kelve diff 3f2a9c1  one commit
    #[command(verbatim_doc_comment)]
    Diff {
        /// A branch to compare against, or a commit. Nothing for what you
        /// haven't committed.
        what: Option<String>,
    },
    /// kelve's tools for your coding agent, over MCP
    ///
    /// Your agent starts this itself. Add it once, for Claude Code:
    ///
    ///   claude mcp add --scope user kelve -- kelve mcp
    ///
    /// Then ask your agent to show you something in kelve.
    #[command(verbatim_doc_comment)]
    Mcp,
    /// Take kelve's next job, for an agent without MCP
    ///
    ///   kelve job                        the next job, as JSON
    ///   kelve job --for 12               the next job for what's open as 12
    ///   kelve job done 7 < answer.json   hand in job 7's answer
    #[command(verbatim_doc_comment)]
    Job {
        #[command(subcommand)]
        done: Option<JobCommand>,
        /// The agent, as it names itself.
        #[arg(long, default_value = "agent", global = true)]
        agent: String,
        /// The model the agent runs on.
        #[arg(long, global = true)]
        model: Option<String>,
        /// Only the jobs for one thing opened, by its number.
        #[arg(long = "for", global = true)]
        only: Option<i64>,
    },
    /// What's kept for your agent to do
    Kept,
    /// The notes you left for your agent in kelve, waiting here
    ///
    ///   kelve notes                     every note waiting, for an agent without MCP
    ///   kelve notes --waiting           one line if any wait: what kelve's hook runs
    ///   kelve notes done <id> <version> "<did>"
    ///                                   say one is done, and what you did
    #[command(verbatim_doc_comment)]
    Notes {
        #[command(subcommand)]
        done: Option<NotesCommand>,
        /// Print one line if notes are waiting, and nothing otherwise.
        #[arg(long)]
        waiting: bool,
    },
    /// What kelve has for your agent now: what its hook runs
    ///
    /// Reads the event your agent gives it, and prints the notes waiting,
    /// or the lesson from the shared graph that helps now, or nothing.
    Moment {
        /// The agent whose hook this is: claude, codex, cursor, copilot,
        /// gemini, antigravity, kiro, droid, qwen, qoder or augment.
        #[arg(long)]
        agent: Option<String>,
        /// The event, as the agent names it.
        #[arg(long)]
        on: Option<String>,
    },
    /// Set kelve up: connect your agent, add its hook, and say what kelve marks
    ///
    /// The installer runs this. It connects the agent running it, or the one
    /// installed, adds kelve's hook so it picks up your notes and lessons,
    /// and sets how much kelve marks: new, learning, working or deep. In a
    /// terminal it asks; run by your agent, it leaves your agent to choose.
    ///
    ///   kelve setup
    ///   kelve setup --agent codex --level working
    #[command(verbatim_doc_comment)]
    Setup {
        /// The agent to connect: claude, codex, cursor or gemini.
        #[arg(long)]
        agent: Option<String>,
        /// How much kelve marks: new, learning, working or deep.
        #[arg(long)]
        level: Option<String>,
    },
    /// What kelve marks for you
    ///
    ///   kelve marks set working     how much: new, learning, working or deep
    #[command(verbatim_doc_comment)]
    Marks {
        #[command(subcommand)]
        what: MarksCommand,
    },
    /// Tell your agent of notes you leave it, and lessons as it works
    ///
    /// Adds kelve's hook to your agent's settings, or takes it out, and
    /// leaves the rest of the file as it was. For claude, codex, cursor,
    /// copilot, gemini, antigravity, kiro, droid, qwen, qoder and augment:
    ///
    ///   kelve hook install claude
    ///   kelve hook remove codex
    #[command(verbatim_doc_comment)]
    Hook {
        #[command(subcommand)]
        what: HookCommand,
    },
    /// Show what an agent has done in its worktree so far
    ///
    /// For an agent working in a worktree: run it there.
    ///
    ///   kelve worktree --base main --said "Retries back off with jitter."
    #[command(verbatim_doc_comment)]
    Worktree {
        /// The branch the worktree left.
        #[arg(long, default_value = "main")]
        base: String,
        /// What you've done, in a line.
        #[arg(long, default_value = "")]
        said: String,
        /// The agent, as it names itself.
        #[arg(long, default_value = "agent")]
        agent: String,
    },
    /// Update kelve to its newest version
    ///
    /// Downloads the newest release for this computer, checks it against
    /// the release's checksums, and puts it in place of this one. A kelve
    /// running on an older version restarts on the new one, once the work
    /// it's doing is done.
    #[command(verbatim_doc_comment)]
    Update,
    /// Whether kelve is running
    Status,
    /// Stop kelve
    Stop,
}

#[derive(Debug, Subcommand)]
enum NotesCommand {
    /// Say a note is done, and what you did, in a line
    Done {
        /// The note's id, as `kelve notes` gave it.
        note: String,
        /// Its version, as `kelve notes` gave it: a note changed since
        /// isn't marked done.
        version: String,
        /// What you did, in one line.
        did: String,
    },
}

#[derive(Debug, Subcommand)]
enum MarksCommand {
    /// How much kelve marks: new (every idea), learning (all but the
    /// basics), working (advanced and specialist ideas) or deep
    /// (specialist ideas only)
    Set {
        /// new, learning, working or deep.
        level: String,
    },
}

#[derive(Debug, Subcommand)]
enum HookCommand {
    /// Add kelve's hook to an agent's settings
    Install {
        /// The agent: claude, codex, cursor, copilot, gemini, antigravity, kiro, droid, qwen, qoder or augment.
        agent: String,
    },
    /// Take kelve's hook out of an agent's settings
    Remove {
        /// The agent, as for install.
        agent: String,
    },
}

#[derive(Debug, Subcommand)]
enum JobCommand {
    /// Hand in a job's answer, as JSON on standard input
    Done {
        /// The job's number.
        job: i64,
    },
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Help, wrapped to the terminal like the rest, under kelve's mark.
    let brand = format!("{} {}", style::mark(), style::bold("kelve"));
    let matches = Cli::command()
        .term_width(style::width())
        .before_help(brand.clone())
        .mut_subcommands(|c| c.before_help(brand.clone()))
        .get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "info,tower_http=warn,sqlx=warn".into());
    let logs = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    // In a terminal, kelve's log takes its look; anywhere else, as it was.
    if style::styled() {
        logs.event_format(style::Log).init();
    } else {
        logs.init();
    }
    // What updating on Windows left behind, once it's no longer running.
    update::tidy();
    let outcome = match cli.command {
        Command::Serve { background } => open::serve(background).await,
        Command::Open { path: None } => open::open().await,
        Command::Open { path: Some(path) } => open::open_path(&path).await,
        Command::Diff { what } => open::diff(what.as_deref()).await,
        Command::Mcp => mcp::serve().await,
        Command::Job {
            done: None,
            agent,
            model,
            only,
        } => jobs::next(&agent, model.as_deref(), only).await,
        Command::Job {
            done: Some(JobCommand::Done { job }),
            agent,
            model,
            only,
        } => jobs::done(job, &agent, model.as_deref(), only).await,
        Command::Kept => jobs::kept().await,
        Command::Notes { waiting: true, .. } => notes::waiting().await,
        Command::Notes {
            done: Some(NotesCommand::Done { note, version, did }),
            ..
        } => notes::done(&note, &version, &did).await,
        Command::Notes { done: None, .. } => notes::list().await,
        Command::Setup { agent, level } => setup::setup(agent.as_deref(), level.as_deref()).await,
        Command::Marks {
            what: MarksCommand::Set { level },
        } => setup::marks_set(&level).await,
        Command::Hook {
            what: HookCommand::Install { agent },
        } => notes::hook(true, &agent).await,
        Command::Hook {
            what: HookCommand::Remove { agent },
        } => notes::hook(false, &agent).await,
        Command::Moment { agent, on } => moment::run(agent.as_deref(), on.as_deref()).await,
        Command::Worktree { base, said, agent } => open::worktree(&base, &said, &agent).await,
        Command::Update => update::update().await,
        Command::Status => status::status().await,
        Command::Stop => open::stop().await,
    };
    match outcome {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            // Each problem is a sentence that says what to do.
            style::problem(&format!("{e:#}"));
            std::process::ExitCode::FAILURE
        }
    }
}
