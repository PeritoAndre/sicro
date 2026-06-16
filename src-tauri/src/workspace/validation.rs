//! Workspace integrity checks.
//!
//! Spike A only verifies *structural* validity (manifest readable, DB file
//! present). Deep integrity (hash-checking media against the manifest) is
//! deferred to later spikes.

use std::path::Path;

use crate::error::{Result, SicroError};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

pub fn ensure_workspace_structure(workspace_dir: &Path) -> Result<Manifest> {
    if !workspace_dir.is_dir() {
        return Err(SicroError::Workspace(format!(
            "not a directory: {}",
            workspace_dir.display()
        )));
    }

    let manifest = Manifest::read(workspace_dir)?;

    let db = workspace_dir.join(SQLITE_FILENAME);
    if !db.is_file() {
        return Err(SicroError::Workspace(format!(
            "database file missing: {}",
            db.display()
        )));
    }

    Ok(manifest)
}
