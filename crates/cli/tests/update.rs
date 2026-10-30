//! `kelve update`, as built, against a local stand-in for where releases
//! are, never GitHub: it puts the newest build in place of itself, checked
//! against the release's checksums, restarts the kelve running on it, and
//! turns away a download that doesn't match.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;

use sha2::{Digest, Sha256};

/// Where releases are: a release `99.0.0` whose build for this computer is
/// `build`, with `sums` as its `SHA256SUMS`. Its address for the newest.
fn releases(build: Vec<u8>, sums: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = format!("http://{}", listener.local_addr().unwrap());
    let name = kelve_core::update::download(std::env::consts::OS, std::env::consts::ARCH)
        .unwrap()
        .to_string();
    let latest = serde_json::json!({
        "tag_name": "v99.0.0",
        "body": "## What's Changed\n* Updates through your agent.\n",
        "assets": [
            { "name": name, "browser_download_url": format!("{at}/build") },
            { "name": "SHA256SUMS", "browser_download_url": format!("{at}/sums") },
        ],
    })
    .to_string();
    let files = Arc::new((latest, build, sums));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let files = files.clone();
            std::thread::spawn(move || {
                let mut line = String::new();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                reader.read_line(&mut line).unwrap_or_default();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                        break;
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or_default();
                let body: &[u8] = match path {
                    "/latest" => files.0.as_bytes(),
                    "/build" => &files.1,
                    "/sums" => files.2.as_bytes(),
                    _ => b"",
                };
                let status = if body.is_empty() {
                    "404 Not Found"
                } else {
                    "200 OK"
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(body);
            });
        }
    });
    format!("{at}/latest")
}

/// This computer's build, as a release has it: the `kelve` just built,
/// packed as the release packs it.
fn packed(binary: &[u8]) -> Vec<u8> {
    if cfg!(windows) {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("kelve.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(binary).unwrap();
        zip.finish().unwrap().into_inner()
    } else {
        let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(binary.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "kelve", binary).unwrap();
        tar.into_inner().unwrap().finish().unwrap()
    }
}

fn sums_for(build: &[u8]) -> String {
    let name = kelve_core::update::download(std::env::consts::OS, std::env::consts::ARCH).unwrap();
    let sum: String = Sha256::digest(build)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{sum}  {name}\n")
}

struct Place {
    dir: PathBuf,
    exe: PathBuf,
    port: u16,
    releases: String,
}

impl Place {
    /// kelve installed in a folder of its own, a copy of the one built.
    fn new(name: &str, releases: String) -> Place {
        let dir = std::env::temp_dir().join(format!("kelve-update-{name}-{}", std::process::id()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join(if cfg!(windows) { "kelve.exe" } else { "kelve" });
        std::fs::copy(env!("CARGO_BIN_EXE_kelve"), &exe).unwrap();
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        Place {
            dir,
            exe,
            port,
            releases,
        }
    }

    fn kelve(&self, args: &[&str]) -> Output {
        let out = self.dir.join("out.txt");
        let err = self.dir.join("err.txt");
        let status = Command::new(&self.exe)
            .args(args)
            .env("KELVE_DATA", self.dir.join("data"))
            .env("KELVE_SECRET_FILE", self.dir.join("secret"))
            .env("KELVE_BIND", format!("127.0.0.1:{}", self.port))
            .env("KELVE_UPDATES", &self.releases)
            .env_remove("KELVE_URL")
            .env("KELVE_GRAPH_URL", "off")
            .env("KELVE_NO_BROWSER", "1")
            .env("NO_COLOR", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(&out).unwrap())
            .stderr(std::fs::File::create(&err).unwrap())
            .status()
            .unwrap();
        Output {
            status,
            stdout: std::fs::read(&out).unwrap_or_default(),
            stderr: std::fs::read(&err).unwrap_or_default(),
        }
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        self.kelve(&["stop"]);
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn folder(at: &Path) -> String {
    let folder = at.join("project");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("README.md"), "# A project\n").unwrap();
    folder.display().to_string()
}

#[test]
fn kelve_update_installs_the_newest_and_restarts_kelve_on_it() {
    let built = std::fs::read(env!("CARGO_BIN_EXE_kelve")).unwrap();
    // The new build: the one just built, marked so it shows it's new.
    let mut new = built.clone();
    new.extend_from_slice(b"\n99.0.0");
    let mut build = packed(&new);
    let sums = sums_for(&build);
    let place = Place::new("restart", releases(std::mem::take(&mut build), sums));
    let project = folder(&place.dir);
    let opened = place.kelve(&["open", &project]);
    assert!(opened.status.success(), "{}", said(&opened));

    // The kelve running is the one built; the new one is the same with a
    // mark on the end, so the kelve it restarts still runs.
    let updated = place.kelve(&["update"]);
    let version = env!("CARGO_PKG_VERSION");
    assert!(updated.status.success(), "{}", said(&updated));
    assert!(
        said(&updated).contains(&format!(
            "kelve is updated from {version} to 99.0.0, and running on it now."
        )),
        "{}",
        said(&updated)
    );
    assert_eq!(std::fs::read(&place.exe).unwrap(), new, "it's in place");
    let status = place.kelve(&["status"]);
    assert!(said(&status).contains("is running"), "{}", said(&status));
    let left: Vec<String> = std::fs::read_dir(place.exe.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|n| n.starts_with(".kelve-update"))
        .collect();
    assert!(left.is_empty(), "nothing half-written left: {left:?}");
}

#[test]
fn a_download_that_doesnt_match_its_checksum_is_turned_away() {
    let build = packed(b"not kelve");
    let wrong = sums_for(b"something else");
    let place = Place::new("mismatch", releases(build, wrong));
    let before = std::fs::read(&place.exe).unwrap();
    let updated = place.kelve(&["update"]);
    assert!(!updated.status.success());
    assert!(
        said(&updated).contains("doesn't match the release's checksum"),
        "{}",
        said(&updated)
    );
    assert_eq!(std::fs::read(&place.exe).unwrap(), before, "left as it was");
}
