//! Choosing a folder or a file to open with the system's own picker: a page
//! in the browser can't give kelve a path, but kelve runs on the person's
//! computer, so it shows the picker itself. macOS's through `osascript`,
//! Windows' through PowerShell, and on Linux zenity's or kdialog's.
//! `KELVE_PICKER` names a program to run instead, given `folder` or `file`,
//! which prints the path: for the browser tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// What to choose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum What {
    Folder,
    File,
}

impl What {
    fn word(self) -> &'static str {
        match self {
            What::Folder => "folder",
            What::File => "file",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PickError {
    #[error(
        "kelve couldn't show a picker here. Open it from a terminal instead, with `kelve open` and its path."
    )]
    NoPicker,
    #[error("A picker is open already: choose there, or close it.")]
    Busy,
}

/// One picker at a time.
static PICKING: AtomicBool = AtomicBool::new(false);

/// Shows the picker, and waits: the path chosen, or None when the person
/// closed it.
pub async fn pick(what: What) -> Result<Option<PathBuf>, PickError> {
    if PICKING.swap(true, Ordering::SeqCst) {
        return Err(PickError::Busy);
    }
    let chosen = ask(what).await;
    PICKING.store(false, Ordering::SeqCst);
    chosen
}

async fn ask(what: What) -> Result<Option<PathBuf>, PickError> {
    for mut command in pickers(what) {
        match command.output().await {
            Ok(out) => {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                return Ok((out.status.success() && !path.is_empty()).then(|| PathBuf::from(path)));
            }
            // Not on this computer: the next one.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                tracing::warn!(error = %e, "the picker couldn't start");
                return Err(PickError::NoPicker);
            }
        }
    }
    Err(PickError::NoPicker)
}

/// The pickers to try, in order.
fn pickers(what: What) -> Vec<tokio::process::Command> {
    if let Some(program) = std::env::var_os("KELVE_PICKER") {
        let mut c = tokio::process::Command::new(program);
        c.arg(what.word());
        return vec![c];
    }
    let prompt = format!("Choose a {} to open in kelve", what.word());
    if cfg!(target_os = "macos") {
        // Through System Events, so the picker comes to the front, over the
        // browser.
        let choose = match what {
            What::Folder => "choose folder",
            What::File => "choose file",
        };
        let mut c = tokio::process::Command::new("osascript");
        c.args([
            "-e",
            "tell application \"System Events\" to activate",
            "-e",
            &format!(
                "tell application \"System Events\" to return POSIX path of ({choose} with prompt \"{prompt}\")"
            ),
        ]);
        vec![c]
    } else if cfg!(windows) {
        let script = match what {
            What::Folder => format!(
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = '{prompt}'; if ($d.ShowDialog() -eq 'OK') {{ $d.SelectedPath }}"
            ),
            What::File => format!(
                "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Title = '{prompt}'; if ($d.ShowDialog() -eq 'OK') {{ $d.FileName }}"
            ),
        };
        let mut c = tokio::process::Command::new("powershell");
        c.args(["-NoProfile", "-STA", "-Command", &script]);
        vec![c]
    } else {
        let mut zenity = tokio::process::Command::new("zenity");
        zenity.args(["--file-selection", &format!("--title={prompt}")]);
        let mut kdialog = tokio::process::Command::new("kdialog");
        kdialog.args(["--title", &prompt]);
        match what {
            What::Folder => {
                zenity.arg("--directory");
                kdialog.args(["--getexistingdirectory", "."]);
            }
            What::File => {
                kdialog.args(["--getopenfilename", "."]);
            }
        }
        vec![zenity, kdialog]
    }
}

/// Shows a folder of kelve's own, like the person's notes folder, in the
/// system's file manager. Under the browser tests, which set
/// `KELVE_PICKER`, nothing is shown.
pub async fn reveal(folder: &std::path::Path) -> Result<(), PickError> {
    if std::env::var_os("KELVE_PICKER").is_some() {
        return Ok(());
    }
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    match tokio::process::Command::new(program)
        .arg(folder)
        .status()
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::warn!(error = %e, "the file manager couldn't start");
            Err(PickError::NoPicker)
        }
    }
}
