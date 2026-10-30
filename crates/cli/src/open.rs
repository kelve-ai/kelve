//! `kelve open` and `kelve diff` ask kelve to open something, starting it
//! in the background first when it isn't running, and kelve opens the
//! browser on it, signed in, with a one-time link. `kelve serve` runs kelve
//! in this terminal instead; `kelve stop` stops it, wherever it runs.
//! There's no account: the person at this computer signs in. The secret
//! lets the command line ask for a link, never have one.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::Context;

use crate::client::Kelve;
use crate::style;

/// How long kelve has to start before `kelve open` gives up on it.
const STARTING: Duration = Duration::from_secs(30);

/// Opens a link in the browser. Whether it could. `KELVE_NO_BROWSER`
/// stops it, for a machine with no browser to open, and for tests.
pub fn open_browser(url: &str) -> bool {
    if std::env::var_os("KELVE_NO_BROWSER").is_some() {
        return false;
    }
    let opened = if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).status()
    } else if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .status()
    } else {
        std::process::Command::new("xdg-open").arg(url).status()
    };
    opened.is_ok_and(|s| s.success())
}

/// Runs kelve. From a terminal, it opens itself in the browser. Started in
/// the background by `kelve open`, it opens each link it's asked for, but
/// not its first: the `kelve open` that started it asks for its own. Every
/// link goes to the person at this computer, never to whoever asked.
pub async fn serve(background: bool) -> anyhow::Result<()> {
    style::head("serve");
    let config = kelve_server::Config::from_env()?;
    let url = config.url.clone();
    let terminal = std::io::stdin().is_terminal();
    let first = AtomicBool::new(true);
    kelve_server::serve(config, move |link: &str| {
        let is_first = first.swap(false, Ordering::SeqCst);
        let opened = (terminal || (background && !is_first)) && open_browser(link);
        anstream::eprintln!();
        if is_first {
            style::note(&format!("kelve is running at {url}."), "");
        } else {
            style::note("`kelve open` asked for a new link.", "");
        }
        if opened {
            style::quiet(
                "Opened it in your browser. If it didn't open, this link works once, within a minute:",
            );
        } else {
            style::quiet("This link opens it, once, within a minute:");
        }
        style::link(link);
        style::quiet("For a new link, run `kelve open`. Stop kelve with Ctrl+C, or `kelve stop`.");
        anstream::eprintln!();
        opened
    })
    .await?;
    Ok(())
}

/// Starts kelve in the background when it isn't running, and waits until it
/// answers. Its log goes to `kelve.log` in its data folder.
pub(crate) async fn started() -> anyhow::Result<()> {
    let me = std::env::current_exe().context("kelve couldn't tell where it's installed.")?;
    started_from(&me).await
}

/// Starts kelve from the binary at `me`, as `started` does: what `kelve
/// update` starts, once it's put a new one there.
pub(crate) async fn started_from(me: &std::path::Path) -> anyhow::Result<()> {
    let url = crate::config::url();
    if crate::client::running(&url).await {
        return Ok(());
    }
    let data = kelve_server::config::data_dir();
    std::fs::create_dir_all(&data)
        .with_context(|| format!("kelve couldn't make its folder, {}.", data.display()))?;
    let log_path = data.join("kelve.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("kelve couldn't write its log, {}.", log_path.display()))?;
    let mut command = std::process::Command::new(me);
    command
        .args(["serve", "--background"])
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    // Its own process group, so closing this terminal or pressing Ctrl+C in
    // it leaves kelve running.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    command.spawn().context("kelve couldn't start.")?;
    let since = std::time::Instant::now();
    while since.elapsed() < STARTING {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if crate::client::running(&url).await {
            return Ok(());
        }
    }
    anyhow::bail!(
        "kelve didn't start. What went wrong is at the end of its log, {}.",
        log_path.display()
    )
}

/// `kelve open`: asks kelve for a new link, starting it first if it isn't
/// running. kelve gives it to the person itself, opening their browser; it
/// never comes back here.
pub async fn open() -> anyhow::Result<()> {
    started().await?;
    let kelve = Kelve::new(Duration::from_secs(10))?;
    let answer = kelve.send(kelve.post("/api/signin-link")).await?;
    let opened = answer["opened"]
        .as_bool()
        .context("kelve didn't say whether it opened. Check it's running with `kelve status`.")?;
    style::head("open");
    if opened {
        style::line("kelve opened in your browser, signed in.");
    } else {
        style::line(
            "kelve couldn't open a browser. Run `kelve serve` in a terminal instead: it shows a link there.",
        );
    }
    Ok(())
}

/// `kelve open <path>`: asks kelve to open a folder or a file. kelve opens
/// the browser on it itself; the command line learns only whether it did.
pub async fn open_path(path: &std::path::Path) -> anyhow::Result<()> {
    let path = std::fs::canonicalize(path)
        .with_context(|| format!("There's nothing at {}.", path.display()))?;
    ask_to_open(&path, None).await
}

/// `kelve diff [what]`: asks kelve to open a change in the repository this
/// folder is in.
pub async fn diff(what: Option<&str>) -> anyhow::Result<()> {
    let here = std::env::current_dir().context("kelve couldn't tell which folder this is.")?;
    let change = match what.map(str::trim) {
        None | Some("") => "uncommitted".to_string(),
        Some(w) if w.starts_with("http://") || w.starts_with("https://") => {
            anyhow::bail!(
                "kelve can't open a pull request by its address yet. Check its branch out, then run `kelve diff main`."
            )
        }
        Some(w) if looks_like_a_commit(w) => format!("commit {w}"),
        Some(w) => format!("against {w}"),
    };
    ask_to_open(&here, Some(&change)).await
}

/// `kelve worktree`: an agent working in a worktree asks kelve to show the
/// person what it's done there so far, since it left `base`.
pub async fn worktree(base: &str, said: &str, agent: &str) -> anyhow::Result<()> {
    let here = std::env::current_dir().context("kelve couldn't tell which folder this is.")?;
    started().await?;
    let kelve = Kelve::new(Duration::from_secs(60))?;
    let body = serde_json::json!({
        "what": "worktree", "path": here.display().to_string(), "base": base,
        "said": said, "agent": agent,
    });
    let o = kelve
        .send(kelve.post("/api/agent/open").json(&body))
        .await?;
    style::head("worktree");
    style::line(&format!(
        "{} Take kelve's work with `kelve job`.",
        opened_line(&o)
    ));
    Ok(())
}

/// `kelve stop`: stops kelve, wherever it was started.
pub async fn stop() -> anyhow::Result<()> {
    style::head("stop");
    let url = crate::config::url();
    if !crate::client::running(&url).await {
        style::line("kelve isn't running.");
        return Ok(());
    }
    let kelve = Kelve::new(Duration::from_secs(10))?;
    kelve.send(kelve.post("/api/stop")).await?;
    // The jobs it's running finish first.
    let since = std::time::Instant::now();
    while since.elapsed() < Duration::from_secs(40) {
        if !crate::client::running(&url).await {
            style::line("kelve stopped.");
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    anyhow::bail!(
        "kelve said it would stop, but it's still running at {}.",
        crate::config::address()
    )
}

/// Whether a name is a commit's: 7 to 40 hex digits. A branch named that
/// way is read as the commit.
fn looks_like_a_commit(w: &str) -> bool {
    (7..=40).contains(&w.len()) && w.chars().all(|c| c.is_ascii_hexdigit())
}

async fn ask_to_open(path: &std::path::Path, change: Option<&str>) -> anyhow::Result<()> {
    started().await?;
    let kelve = Kelve::new(Duration::from_secs(20))?;
    let body = serde_json::json!({ "path": path.display().to_string(), "change": change });
    let answer = kelve.send(kelve.post("/api/open").json(&body)).await?;
    let opened = answer["browser"]
        .as_bool()
        .context("kelve didn't say whether it opened. Check it's running with `kelve status`.")?;
    style::head(if change.is_some() { "diff" } else { "open" });
    if opened {
        style::line("kelve opened it in your browser.");
    } else {
        style::line(
            "kelve couldn't open a browser. Run `kelve serve` in a terminal instead: it shows a link there.",
        );
    }
    Ok(())
}

/// What opening came to, for the agent: "kelve is showing the person
/// ledger: 2 files, both to read." Its folders are read plainly with it,
/// each one more to read: "12 files in 3 folders, 5 to read."
pub(crate) fn opened_line(o: &serde_json::Value) -> String {
    let files = o["files"].as_u64().unwrap_or_default();
    let folders = o["folders"].as_u64().unwrap_or_default();
    let to_read = o["toRead"].as_u64().unwrap_or_default();
    let mut count = if files == 1 {
        "1 file".to_string()
    } else {
        format!("{files} files")
    };
    match folders {
        0 => {}
        1 => count.push_str(" in 1 folder"),
        n => count.push_str(&format!(" in {n} folders")),
    }
    let reading = match (files + folders, to_read) {
        (_, 0) => "all read before".to_string(),
        (1, 1) if folders == 0 => "to read".to_string(),
        (2, 2) if folders == 0 => "both to read".to_string(),
        (all, r) if r >= all => "all to read".to_string(),
        (_, r) => format!("{r} to read"),
    };
    format!(
        "kelve is showing the person {}: {count}, {reading}.",
        o["title"].as_str().unwrap_or("what you opened")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_is_told_what_needs_reading_with_its_folders() {
        let line = |files: u64, folders: u64, to_read: u64| {
            opened_line(&serde_json::json!({
                "title": "ledger", "files": files, "folders": folders, "toRead": to_read,
            }))
        };
        assert_eq!(
            line(2, 0, 2),
            "kelve is showing the person ledger: 2 files, both to read."
        );
        assert_eq!(
            line(12, 3, 16),
            "kelve is showing the person ledger: 12 files in 3 folders, all to read."
        );
        assert_eq!(
            line(12, 3, 5),
            "kelve is showing the person ledger: 12 files in 3 folders, 5 to read."
        );
        assert_eq!(
            line(1, 1, 0),
            "kelve is showing the person ledger: 1 file in 1 folder, all read before."
        );
    }

    #[test]
    fn commits_are_told_from_branches() {
        assert!(looks_like_a_commit("3f2a9c1"));
        assert!(looks_like_a_commit(
            "3f2a9c1e0b4d5a6f7e8d9c0b1a2f3e4d5c6b7a8f"
        ));
        assert!(!looks_like_a_commit("main"));
        assert!(!looks_like_a_commit("abc"));
        assert!(!looks_like_a_commit("first-version"));
    }
}
