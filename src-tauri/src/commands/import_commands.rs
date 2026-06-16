//! Tauri commands for the `.sicroapp` importer (Spike D).
//!
//! The frontend calls `import_sicroapp` with the path the user picked from
//! the OS dialog. The orchestrator does the heavy lifting; this module is
//! just the Tauri boundary.

use std::path::PathBuf;

use tauri::State;
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::repositories::import_repo;
use crate::error::{Result, SicroError};
use crate::importer::{run_import, ImportRegistry};
use crate::models::{Import, ImportReport, ImportResult, ImportSicroappInput};
use crate::state::AppState;
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};
use crate::workspace::open_workspace;

#[tauri::command]
pub async fn import_sicroapp(
    state: State<'_, AppState>,
    input: ImportSicroappInput,
) -> Result<ImportResult> {
    let registry = ImportRegistry::open(state.config_dir());
    let result = run_import(input, state.default_workspace_parent(), &registry)?;

    // Surface the new workspace on the recents list so the user can reopen
    // it without browsing.
    state.upsert_recent(
        &result.occurrence,
        &result.workspace_path,
        result.occurrence.id,
    )?;

    Ok(result)
}

/// List every `imports` row stored in the workspace's SQLite. Used by the
/// dossier panel to show "this ocorrência originated from the following
/// import(s)".
#[tauri::command]
pub async fn list_workspace_imports(workspace_path: String) -> Result<Vec<Import>> {
    let ws = PathBuf::from(&workspace_path);
    // Reuse the full open path so the workspace structure is validated.
    let _ = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    import_repo::list_all(&conn)
}

#[tauri::command]
pub async fn read_import_report(
    workspace_path: String,
    import_id: String,
) -> Result<ImportReport> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let id = Uuid::parse_str(&import_id)
        .map_err(|e| SicroError::Validation(format!("invalid import_id: {e}")))?;
    let report_path = ws
        .join("imports")
        .join(id.to_string())
        .join("import_report.json");
    let bytes = std::fs::read(&report_path).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot read import_report.json at {}: {}",
            report_path.display(),
            e
        ))
    })?;
    let report: ImportReport = serde_json::from_slice(&bytes)?;
    Ok(report)
}

