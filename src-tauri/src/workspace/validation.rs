//! Validação estrutural do workspace (manifesto legível, banco presente).

use std::path::Path;

use crate::error::{Result, SicroError};
use crate::workspace::manifest::Manifest;
use crate::workspace::paths::sqlite_path;

pub fn ensure_workspace_structure(workspace_dir: &Path) -> Result<Manifest> {
    if !workspace_dir.is_dir() {
        return Err(SicroError::Workspace(format!(
            "not a directory: {}",
            workspace_dir.display()
        )));
    }

    let manifest = Manifest::read(workspace_dir)?;

    let db = sqlite_path(workspace_dir);
    if !db.is_file() {
        return Err(SicroError::Workspace(format!(
            "database file missing: {}",
            db.display()
        )));
    }

    Ok(manifest)
}
