//! Workspace open / reload.

use std::path::Path;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::occurrence_repo;
use crate::error::{Result, SicroError};
use crate::models::Occurrence;
use crate::workspace::manifest::Manifest;
use crate::workspace::migracao::migrar_layout_antigo;
use crate::workspace::paths::sqlite_path;
use crate::workspace::validation::ensure_workspace_structure;

pub struct OpenedWorkspace {
    pub manifest: Manifest,
    pub occurrence: Occurrence,
}

pub fn open_workspace(workspace_dir: &Path) -> Result<OpenedWorkspace> {
    // Caso antigo: reorganiza antes de abrir o banco.
    migrar_layout_antigo(workspace_dir)?;
    let manifest = ensure_workspace_structure(workspace_dir)?;

    let db_path = sqlite_path(workspace_dir);
    let mut conn = open_connection(&db_path)?;
    // Migrações são idempotentes: garante o schema mesmo em workspace de build antigo.
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
