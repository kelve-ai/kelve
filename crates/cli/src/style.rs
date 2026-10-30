//! How the command line looks, for the commands people run: kelve's red
//! square for its own lines and for the pen, names in ink, paths and counts
//! quiet.
//!
//! It takes its colours from the person's own terminal theme: the theme's
//! red, and the terminal's dim for anything quiet, never fixed greys, so it
//! reads on light, dark and anything between. Every line is wrapped to the
//! terminal's width, carrying on under where it started. Everything goes
//! through `anstream`, which leaves the colour out when the output isn't a
//! terminal or `NO_COLOR` is set. The hooks and `kelve mcp` talk to agents,
//! not people, and never use this.

use std::io::IsTerminal;
use std::sync::OnceLock;

use anstyle::{AnsiColor, Color, Effects, Style};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;

/// kelve's red, as the terminal's theme has it: its mark, and the pen.
const RED: Style = Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red)));
/// Paths, counts, what comes next: the terminal's own dim.
const QUIET: Style = Style::new().effects(Effects::DIMMED);
const BOLD: Style = Style::new().bold();
const LINK: Style = Style::new().underline();

/// The widest a line is written, however wide the terminal: longer lines
/// are harder to read.
const WIDEST: usize = 100;
/// Without a terminal to measure, lines are wrapped at this.
const USUAL: usize = 80;

/// Styles for clap's help, so `kelve --help` looks like the rest.
pub const HELP: clap::builder::Styles = clap::builder::Styles::styled()
    .header(BOLD)
    .usage(BOLD)
    .literal(BOLD)
    .placeholder(QUIET)
    .valid(BOLD)
    .invalid(RED)
    .error(RED);

fn paint(style: Style, text: &str) -> String {
    format!("{style}{text}{style:#}")
}

pub fn red(text: &str) -> String {
    paint(RED, text)
}

pub fn soft(text: &str) -> String {
    paint(QUIET, text)
}

pub fn bold(text: &str) -> String {
    paint(BOLD, text)
}

/// kelve's mark.
pub fn mark() -> String {
    red("■")
}

/// How wide to write: the terminal's width, from `COLUMNS` or the terminal
/// itself, at most `WIDEST`; `USUAL` when there's no terminal to measure.
pub fn width() -> usize {
    static WIDTH: OnceLock<usize> = OnceLock::new();
    *WIDTH.get_or_init(|| {
        let from_env = std::env::var("COLUMNS")
            .ok()
            .and_then(|c| c.trim().parse().ok());
        from_env
            .or_else(measured)
            .unwrap_or(USUAL)
            .clamp(40, WIDEST)
    })
}

/// The terminal's width, as `stty` reads it from the terminal itself.
fn measured() -> Option<usize> {
    if !std::io::stderr().is_terminal() {
        return None;
    }
    let tty = std::fs::File::open("/dev/tty").ok()?;
    let out = std::process::Command::new("stty")
        .arg("size")
        .stdin(tty)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let said = String::from_utf8_lossy(&out.stdout);
    said.split_whitespace().nth(1)?.parse().ok()
}

/// Text as lines no wider than `room`, broken between words. A word too
/// long for a line, like a path, has a line to itself.
pub fn wrap(text: &str, room: usize) -> Vec<String> {
    let room = room.max(10);
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let fits = line.chars().count() + 1 + word.chars().count() <= room;
        if !line.is_empty() && !fits {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// Text after `first` on its first line, wrapped, its other lines indented
/// to where it started, each painted with `style`.
fn hang(first: &str, start: usize, text: &str, style: Option<Style>) {
    let paint_it = |t: &str| style.map_or_else(|| t.to_string(), |s| paint(s, t));
    for (i, piece) in wrap(text, width().saturating_sub(start)).iter().enumerate() {
        if i == 0 {
            anstream::eprintln!("{first}{}", paint_it(piece));
        } else {
            anstream::eprintln!("{}{}", " ".repeat(start), paint_it(piece));
        }
    }
}

/// The first line of a command: `■ kelve stop`.
pub fn head(command: &str) {
    anstream::eprintln!("{} {} {}", mark(), bold("kelve"), soft(command));
    anstream::eprintln!();
}

/// A line kelve says about what happens next, with its mark, and what
/// follows from it, quietly, under it.
pub fn note(lead: &str, rest: &str) {
    hang(&format!("  {} ", mark()), 4, lead, None);
    if !rest.is_empty() {
        hang("    ", 4, rest, Some(QUIET));
    }
}

/// A plain line, indented under the head.
pub fn line(text: &str) {
    hang("  ", 2, text, None);
}

/// A quiet line, indented under the head.
pub fn quiet(text: &str) {
    hang("  ", 2, text, Some(QUIET));
}

/// An address on its own line, never broken, so it can be clicked.
pub fn link(address: &str) {
    anstream::eprintln!("  {}", paint(LINK, address));
}

/// Something that went wrong, in a sentence that says what to do.
pub fn problem(text: &str) {
    anstream::eprintln!();
    hang(&format!("  {} ", red("!")), 4, text, None);
}

/// Whether what's written to the person can be styled: a terminal, without
/// `NO_COLOR`.
pub fn styled() -> bool {
    std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

/// kelve's log, as a person running `kelve serve` sees it: a quiet time,
/// and what happened. Warnings and errors say so, an error in red.
#[derive(Debug)]
pub struct Log;

impl<S, N> FormatEvent<S, N> for Log
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let time = chrono::Local::now().format("%H:%M").to_string();
        let mut said = String::new();
        ctx.field_format()
            .format_fields(Writer::new(&mut said), event)?;
        let level = match *event.metadata().level() {
            tracing::Level::ERROR => format!("{} ", red("error")),
            tracing::Level::WARN => format!("{} ", bold("warning")),
            _ => String::new(),
        };
        // Wrapped like everything else, under where it started.
        let start = 2 + time.len() + 2;
        for (i, piece) in wrap(&said, width().saturating_sub(start))
            .iter()
            .enumerate()
        {
            if i == 0 {
                writeln!(writer, "  {}  {level}{piece}", soft(&time))?;
            } else {
                writeln!(writer, "{}{piece}", " ".repeat(start))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_wrap_between_words_and_never_break_a_word() {
        let text = "Open a folder with kelve open, and kelve marks the ideas in ~/Library/Application_Support/kelve/notes/a-very-long-file-name.md.";
        let lines = wrap(text, 30);
        assert!(
            lines
                .iter()
                .all(|l| l.chars().count() <= 30 || !l.contains(' '))
        );
        assert_eq!(lines.join(" "), text);
        assert!(lines.iter().any(|l| l.starts_with("~/Library")));
        assert_eq!(wrap("", 30), vec![String::new()]);
    }

    #[test]
    fn colours_come_from_the_theme() {
        // The theme's red, and its dim: never a fixed shade that a light or
        // grey background could swallow.
        assert_eq!(RED.get_fg_color(), Some(Color::Ansi(AnsiColor::Red)));
        assert_eq!(QUIET.get_fg_color(), None);
        assert!(QUIET.get_effects().contains(Effects::DIMMED));
    }
}
