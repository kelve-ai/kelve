//! What the person knows already, kept as `marks.md` in kelve's own folder:
//! which ideas aren't new to them, from "I know this" in a debrief, for their
//! agent as it explains one or looks into a decision. Theirs alone: it goes
//! only to their own agent, never into the project or anywhere else.

use std::path::{Path, PathBuf};

use kelve_core::spec::{self, Spec};

use crate::error::{ApiError, ApiResult};
use crate::state::Shared;

/// The file's name, in kelve's own folder.
const FILE: &str = "marks.md";

/// Where it's kept.
pub fn path(app: &Shared) -> PathBuf {
    app.config.data.join(FILE)
}

/// Where a kelve with notes kept it: moved to where it's kept now, once.
pub fn settle(data: &Path) {
    let (was, now) = (data.join("notes").join(FILE), data.join(FILE));
    if was.is_file()
        && !now.exists()
        && let Err(e) = std::fs::rename(&was, &now)
    {
        tracing::warn!(error = %e, "couldn't move what the person knows to kelve's own folder");
    }
}

/// What the person knows: as it's kept, or, with nothing yet, that they know
/// their work (`Level::Working`) and no idea in particular.
pub fn read(app: &Shared) -> Spec {
    std::fs::read_to_string(path(app))
        .map(|text| spec::parse(&text))
        .unwrap_or_default()
}

/// Keeps what the person knows, today.
pub fn save(app: &Shared, mut spec: Spec) -> ApiResult<Spec> {
    spec.by = Some("you".to_string());
    spec.at = Some(chrono::Utc::now().format("%Y-%m-%d").to_string());
    let file = path(app);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(ApiError::internal)?;
    }
    std::fs::write(&file, spec::write(&spec)).map_err(ApiError::internal)?;
    Ok(spec)
}

/// The ideas' names as the file can hold them: single spaces, and no commas or
/// line breaks.
pub fn tidied(mut spec: Spec) -> Spec {
    let tidy = |names: Vec<String>| -> Vec<String> {
        names
            .into_iter()
            .map(|n| n.split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|n| !n.is_empty() && !n.contains(','))
            .collect()
    };
    spec.always = tidy(spec.always);
    spec.never = tidy(spec.never);
    spec
}
