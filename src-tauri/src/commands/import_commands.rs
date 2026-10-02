//! Comandos Tauri do importador `.sicroapp`; o trabalho pesado fica em `importer`.

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

    state.upsert_recent(
        &result.occurrence,
        &result.workspace_path,
        result.occurrence.id,
    )?;

    Ok(result)
}

/// Linhas de `imports` do workspace (de quais importações a ocorrência veio).
#[tauri::command]
pub async fn list_workspace_imports(workspace_path: String) -> Result<Vec<Import>> {
    let ws = PathBuf::from(&workspace_path);
    // `open_workspace` só para validar a estrutura.
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

