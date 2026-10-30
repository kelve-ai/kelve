//! `kelve update`, and kelve's `update` tool for the person's agent: finds
//! the newest release where the installer finds it, downloads this
//! computer's build, checks it against the release's `SHA256SUMS`, and puts
//! it in place of this binary, whole or not at all. Then, when a kelve is
//! running and is older, it asks it to stop once the work it's doing is
//! done, and starts the new one in the background, as `kelve open` does.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use kelve_core::update::{download, is_newer, matches};
use kelve_server::updates::{Release, VERSION, newest, source_to_update};
use sha2::{Digest, Sha256};

use crate::client::Kelve;
use crate::style;

/// How long a download may take.
const DOWNLOADING: Duration = Duration::from_secs(300);

/// The most a download may be.
const MOST: u64 = 256 * 1024 * 1024;

/// Where Windows' running `kelve.exe` is moved, to be replaced: it can be
/// renamed while it runs, but not removed. A later start removes it.
const OLD: &str = "kelve.old.exe";

/// How long the kelve running has to stop: the jobs it's running finish
/// first.
const STOPPING: Duration = Duration::from_secs(40);

/// `kelve update`.
pub async fn update() -> anyhow::Result<()> {
    style::head("update");
    let said = run().await?;
    style::line(&said);
    Ok(())
}

/// Updates kelve, and restarts the kelve running when it's older: what was
/// done, in a sentence or two.
pub async fn run() -> anyhow::Result<String> {
    let exe = installed()?;
    let http = web()?;
    let source = source_to_update().map_err(|why| anyhow!("KELVE_UPDATES isn't valid: {why}."))?;
    let release = newest(&http, &source).await.map_err(|e| {
        tracing::info!(error = %e, "couldn't find kelve's newest release");
        anyhow!("kelve couldn't find its newest release. Check you're online, then try again.")
    })?;
    let now_on = if is_newer(&release.version, VERSION) {
        install(&http, &release, &exe).await?;
        release.version.clone()
    } else {
        VERSION.to_string()
    };
    let restarted = restart_if_older(&exe, &now_on).await?;
    Ok(sentence(VERSION, &now_on, &restarted))
}

/// What the kelve running came to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Running {
    /// None was.
    No,
    /// It was as new already.
    Current,
    /// It was older, as it said, or too old to say, and started again.
    Restarted(Option<String>),
}

/// What `kelve update` says it did.
fn sentence(from: &str, to: &str, running: &Running) -> String {
    match (from == to, running) {
        (false, Running::Restarted(_)) => {
            format!("kelve is updated from {from} to {to}, and running on it now.")
        }
        (false, Running::No) => format!(
            "kelve is updated from {from} to {to}. It isn't running; `kelve open` starts it."
        ),
        (false, Running::Current) => format!("kelve is updated from {from} to {to}."),
        (true, Running::Restarted(was)) => format!(
            "kelve {to} is the newest. The kelve running was {}, so it's restarted on {to}.",
            was.as_deref().unwrap_or("older")
        ),
        (true, _) => format!("kelve {to} is the newest, so there's nothing to update."),
    }
}

/// Where this binary is, with links followed: what's replaced.
fn installed() -> anyhow::Result<PathBuf> {
    let me = std::env::current_exe().context("kelve couldn't tell where it's installed.")?;
    Ok(std::fs::canonicalize(&me).unwrap_or(me))
}

/// A client for downloads from the web: through a proxy, if one is set.
fn web() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(DOWNLOADING)
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!(
            "kelve/",
            env!("CARGO_PKG_VERSION"),
            " (+https://kelve.ai)"
        ))
        .build()?)
}

/// Downloads this computer's build of `release`, checks it, and puts it in
/// place of `exe`.
async fn install(http: &reqwest::Client, release: &Release, exe: &Path) -> anyhow::Result<()> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let name =
        download(os, arch).ok_or_else(|| anyhow!("kelve has no build for {os} on {arch} yet."))?;
    let (Some(file), Some(sums)) = (release.file(name), release.file("SHA256SUMS")) else {
        bail!(
            "kelve {}'s release has no build for this computer yet. Try again later.",
            release.version
        );
    };
    let file = fetch(http, file).await?;
    let sums = String::from_utf8_lossy(&fetch(http, sums).await?).into_owned();
    if !checked(&sums, name, &file) {
        bail!(
            "The download doesn't match the release's checksum, so it wasn't installed. Try again."
        );
    }
    let binary = unpack(name, &file)?;
    replace(exe, &binary, cfg!(windows)).map_err(|e| {
        anyhow!(
            "kelve couldn't put the new version in {}: {e}. Run `kelve update` as someone who can write there, or install kelve again.",
            exe.parent().unwrap_or(exe).display()
        )
    })
}

/// Downloads a file whole, refusing one larger than any build.
async fn fetch(http: &reqwest::Client, url: &str) -> anyhow::Result<Vec<u8>> {
    let failed = |e: reqwest::Error| {
        tracing::info!(error = %e, "a download for kelve's update failed");
        anyhow!("kelve couldn't download its newest release. Check you're online, then try again.")
    };
    let response = http
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(failed)?;
    if response.content_length().is_some_and(|n| n > MOST) {
        bail!("kelve's newest release is larger than any build should be, so it wasn't installed.");
    }
    let bytes = response.bytes().await.map_err(failed)?;
    if bytes.len() as u64 > MOST {
        bail!("kelve's newest release is larger than any build should be, so it wasn't installed.");
    }
    Ok(bytes.to_vec())
}

/// Whether a download is the one the release's `SHA256SUMS` lists.
fn checked(sums: &str, name: &str, file: &[u8]) -> bool {
    let sum: String = Sha256::digest(file)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    matches(sums, name, &sum)
}

/// The binary inside a release's download: `kelve` in a `.tar.gz`, or
/// `kelve.exe` in a `.zip`.
fn unpack(name: &str, file: &[u8]) -> anyhow::Result<Vec<u8>> {
    let unreadable = || anyhow!("kelve's newest release didn't unpack, so it wasn't installed.");
    let mut binary = Vec::new();
    if name.ends_with(".zip") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(file)).map_err(|_| unreadable())?;
        let entry = zip.by_name("kelve.exe").map_err(|_| unreadable())?;
        entry
            .take(MOST)
            .read_to_end(&mut binary)
            .map_err(|_| unreadable())?;
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        let mut found = false;
        for entry in tar.entries().map_err(|_| unreadable())? {
            let entry = entry.map_err(|_| unreadable())?;
            let is_kelve = entry.header().entry_type().is_file()
                && entry.path().is_ok_and(|p| p.as_ref() == Path::new("kelve"));
            if is_kelve {
                entry
                    .take(MOST)
                    .read_to_end(&mut binary)
                    .map_err(|_| unreadable())?;
                found = true;
                break;
            }
        }
        if !found {
            return Err(unreadable());
        }
    }
    if binary.is_empty() {
        return Err(unreadable());
    }
    Ok(binary)
}

/// Puts `binary` in place of `exe`, whole or not at all: written to a file
/// beside it, then renamed over it. With `move_aside`, as on Windows, the
/// running one is moved to `kelve.old.exe` first, and back if the new one
/// can't go in.
fn replace(exe: &Path, binary: &[u8], move_aside: bool) -> std::io::Result<()> {
    use std::io::Write;
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let new = dir.join(format!(".kelve-update-{}", std::process::id()));
    let written = (|| {
        let mut file = std::fs::File::create(&new)?;
        file.write_all(binary)?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(exe).map_or(0o755, |m| m.permissions().mode() & 0o777);
            std::fs::set_permissions(&new, std::fs::Permissions::from_mode(mode | 0o100))?;
        }
        Ok(())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&new);
        return Err(e);
    }
    if move_aside {
        let old = dir.join(OLD);
        let _ = std::fs::remove_file(&old);
        if let Err(e) = std::fs::rename(exe, &old) {
            let _ = std::fs::remove_file(&new);
            return Err(e);
        }
        if let Err(e) = std::fs::rename(&new, exe) {
            let _ = std::fs::rename(&old, exe);
            let _ = std::fs::remove_file(&new);
            return Err(e);
        }
        return Ok(());
    }
    std::fs::rename(&new, exe).inspect_err(|_| {
        let _ = std::fs::remove_file(&new);
    })
}

/// Removes the `kelve.old.exe` an update on Windows left beside kelve, once
/// it's no longer running. Quietly: it's tried again next time.
pub fn tidy() {
    if cfg!(windows)
        && let Some(dir) = installed().ok().as_deref().and_then(Path::parent)
    {
        tidy_in(dir);
    }
}

fn tidy_in(dir: &Path) {
    let old = dir.join(OLD);
    if old.exists() {
        let _ = std::fs::remove_file(old);
    }
}

/// The version of the kelve running, as it says: None for one too old to
/// say.
async fn running_version() -> Option<String> {
    let kelve = Kelve::new(Duration::from_secs(5)).ok()?;
    let view = kelve.send(kelve.get("/api/agent/update")).await.ok()?;
    view["version"].as_str().map(str::to_string)
}

/// Stops the kelve running when it's older than `to`, and starts `exe` in
/// its place.
async fn restart_if_older(exe: &Path, to: &str) -> anyhow::Result<Running> {
    let url = crate::config::url();
    if !crate::client::running(&url).await {
        return Ok(Running::No);
    }
    let was = running_version().await;
    if was.as_deref().is_some_and(|v| !is_newer(to, v)) {
        return Ok(Running::Current);
    }
    let kelve = Kelve::new(Duration::from_secs(10))?;
    kelve.send(kelve.post("/api/stop")).await?;
    let since = std::time::Instant::now();
    while crate::client::running(&url).await {
        if since.elapsed() > STOPPING {
            bail!(
                "kelve is updated to {to}, but the kelve running didn't stop. Run `kelve stop`, then `kelve open`."
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    crate::open::started_from(exe).await?;
    Ok(Running::Restarted(was))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use std::io::Write;

    fn folder() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("kelve-update-{}", uuid_like()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn uuid_like() -> String {
        format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        )
    }

    #[test]
    fn the_binary_is_replaced_whole_beside_itself() {
        let dir = folder();
        let exe = dir.join("kelve");
        std::fs::write(&exe, b"old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o750)).unwrap();
        }
        replace(&exe, b"new", false).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&exe).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o750, "as it was");
        }
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().collect();
        assert_eq!(left.len(), 1, "nothing left beside it");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn on_windows_the_running_one_is_moved_aside_then_tidied() {
        let dir = folder();
        let exe = dir.join("kelve.exe");
        std::fs::write(&exe, b"old").unwrap();
        replace(&exe, b"new", true).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.join(OLD)).unwrap(), b"old");
        // Again, over the one left last time.
        replace(&exe, b"newer", true).unwrap();
        assert_eq!(std::fs::read(&exe).unwrap(), b"newer");
        assert_eq!(std::fs::read(dir.join(OLD)).unwrap(), b"new");
        tidy_in(&dir);
        assert!(!dir.join(OLD).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_changes_when_the_new_one_cant_be_written() {
        let dir = folder();
        let exe = dir.join("missing").join("kelve");
        assert!(replace(&exe, b"new", false).is_err());
        assert!(!exe.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn tar_gz(name: &str, body: &[u8]) -> Vec<u8> {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, name, body).unwrap();
        tar.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn a_download_is_checked_then_unpacked() {
        let file = tar_gz("kelve", b"#!kelve");
        let sum: String = Sha256::digest(&file)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let sums = format!("{sum}  kelve-macos-arm64.tar.gz\n");
        assert!(checked(&sums, "kelve-macos-arm64.tar.gz", &file));
        let mut changed = file.clone();
        if let Some(last) = changed.last_mut() {
            *last ^= 1;
        }
        assert!(
            !checked(&sums, "kelve-macos-arm64.tar.gz", &changed),
            "a mismatch is turned away"
        );
        assert!(!checked(&sums, "kelve-linux-x86_64.tar.gz", &file));
        assert_eq!(
            unpack("kelve-macos-arm64.tar.gz", &file).unwrap(),
            b"#!kelve"
        );
        assert!(unpack("kelve-macos-arm64.tar.gz", &tar_gz("other", b"x")).is_err());
        assert!(unpack("kelve-macos-arm64.tar.gz", b"not a tar").is_err());
    }

    #[test]
    fn a_zip_gives_kelve_exe() {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file(
            "kelve.exe",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(b"MZ kelve").unwrap();
        let file = zip.finish().unwrap().into_inner();
        assert_eq!(
            unpack("kelve-windows-x86_64.zip", &file).unwrap(),
            b"MZ kelve"
        );
        assert!(unpack("kelve-windows-x86_64.zip", b"PK not a zip").is_err());
    }

    #[test]
    fn it_says_what_it_did() {
        assert_eq!(
            sentence("0.2.1", "0.3.0", &Running::Restarted(Some("0.2.1".into()))),
            "kelve is updated from 0.2.1 to 0.3.0, and running on it now."
        );
        assert_eq!(
            sentence("0.2.1", "0.3.0", &Running::No),
            "kelve is updated from 0.2.1 to 0.3.0. It isn't running; `kelve open` starts it."
        );
        assert_eq!(
            sentence("0.3.0", "0.3.0", &Running::Restarted(None)),
            "kelve 0.3.0 is the newest. The kelve running was older, so it's restarted on 0.3.0."
        );
        assert_eq!(
            sentence("0.3.0", "0.3.0", &Running::Current),
            "kelve 0.3.0 is the newest, so there's nothing to update."
        );
    }
}
