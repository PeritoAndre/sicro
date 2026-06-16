//! Workspace open / reload.

use std::path::Path;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::occurrence_repo;
use crate::error::{Result, SicroError};
use crate::models::Occurrence;
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};
use crate::workspace::validation::ensure_workspace_structure;

pub struct OpenedWorkspace {
    pub manifest: Manifest,
    pub occurrence: Occurrence,
}

pub fn open_workspace(workspace_dir: &Path) -> Result<OpenedWorkspace> {
    let manifest = ensure_workspace_structure(workspace_dir)?;

    let db_path = workspace_dir.join(SQLITE_FILENAME);
    let mut conn = open_connection(&db_path)?;
    // Re-run migrations defensively: idempotent statements + a migrations table
    // make this safe even if the workspace was created by an older build.
    run_migrations(&mut conn)?;

    let occurrence = occurrence_repo::find_by_id(&conn, &manifest.occurrence_id)?
        .ok_or_else(|| {
            SicroError::Workspace(format!(
                "manifest references occurrence {} but it does not exist in the database",
                manifest.occurrence_id
            ))
        })?;

    occurrence_repo::record_audit(
        &conn,
        Some(&occurrence.id),
        "workspace.opened",
        Some("workspace"),
        None,
        None,
        None,
    )?;

    Ok(OpenedWorkspace { manifest, occurrence })
}
