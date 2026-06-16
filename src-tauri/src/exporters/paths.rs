//! Path resolution for exported artefacts.
//!
//! Layout (per doc 02 §9):
//!   <workspace>/exports/html/laudo_<id>_<timestamp>.html
//!   <workspace>/exports/pdf/laudo_<id>_<timestamp>.pdf
//!   <workspace>/exports/docx/laudo_<id>_<timestamp>.docx
//!
//! The timestamp is `YYYYMMDDhhmmss` UTC — short enough for the filename to
//! be human-readable yet unique enough to avoid collisions during a busy
//! editing session.

use std::path::{Path, PathBuf};

use chrono::Utc;
use uuid::Uuid;

use crate::error::{Result, SicroError};
use crate::models::ExportKind;

pub struct ExportTarget {
    pub absolute_path: PathBuf,
    pub relative_path: String,
}

pub fn resolve_export_target(
    workspace_path: &Path,
    laudo_id: &Uuid,
    kind: ExportKind,
) -> Result<ExportTarget> {
    let subdir = kind.subdir();
    let dir = workspace_path.join(subdir);
    std::fs::create_dir_all(&dir).map_err(|e| {
        SicroError::Filesystem(format!(
            "could not create export directory {}: {}",
            dir.display(),
            e
        ))
    })?;

    let timestamp = Utc::now().format("%Y%m%d%H%M%S").to_string();
    let short = laudo_id.to_string()[..8].to_string();
    let filename = format!("laudo_{short}_{timestamp}.{ext}", ext = kind.extension());

    let absolute_path = dir.join(&filename);
    let relative_path = format!("{subdir}/{filename}");

    Ok(ExportTarget {
        absolute_path,
        relative_path,
    })
}
