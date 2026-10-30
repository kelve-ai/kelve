//! `kelve open` starts kelve in the background when it isn't running, uses
//! it when it is, and `kelve stop` stops it: the `kelve` command as built,
//! on a folder and a port of its own, with no browser.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Place {
    dir: PathBuf,
    port: u16,
}

impl Place {
    fn new() -> Place {
        let dir = std::env::temp_dir().join(format!("kelve-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        Place { dir, port }
    }

    /// Runs `kelve`, its output going to files, not pipes: on Windows the
    /// kelve it starts in the background would inherit the pipes and hold
    /// them open, and waiting for them to close would never end.
    fn kelve(&self, args: &[&str]) -> Output {
        let out = self.dir.join("out.txt");
        let err = self.dir.join("err.txt");
        let status = Command::new(env!("CARGO_BIN_EXE_kelve"))
            .args(args)
            .env("KELVE_DATA", self.dir.join("data"))
            .env("KELVE_SECRET_FILE", self.dir.join("secret"))
            .env("KELVE_BIND", format!("127.0.0.1:{}", self.port))
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

/// Stops the kelve a test started, however the test ends.
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

fn folder(at: &Path) -> PathBuf {
    let folder = at.join("project");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("README.md"), "# A project\n").unwrap();
    folder
}

#[test]
fn kelve_open_starts_kelve_by_itself_and_kelve_stop_stops_it() {
    let place = Place::new();
    let project = folder(&place.dir);
    let project = project.to_str().unwrap();

    let status = place.kelve(&["status"]);
    assert!(said(&status).contains("isn't running"), "{}", said(&status));

    let opened = place.kelve(&["open", project]);
    assert!(opened.status.success(), "{}", said(&opened));
    let status = place.kelve(&["status"]);
    assert!(said(&status).contains("is running"), "{}", said(&status));
    assert!(
        place.dir.join("data/kelve.log").exists(),
        "its log is beside its data"
    );

    // Running already, it's used, not started again.
    let again = place.kelve(&["open", project]);
    assert!(again.status.success(), "{}", said(&again));

    let stopped = place.kelve(&["stop"]);
    assert!(
        said(&stopped).contains("kelve stopped"),
        "{}",
        said(&stopped)
    );
    let status = place.kelve(&["status"]);
    assert!(said(&status).contains("isn't running"), "{}", said(&status));
    let stop_again = place.kelve(&["stop"]);
    assert!(said(&stop_again).contains("isn't running"));
}
