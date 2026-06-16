//! Tauri commands para a consolidação Alpha (MVP 8).
//!
//! Três commands:
//!   - `generate_workspace_backup` — gera o `.sicrobackup` (zip).
//!   - `get_system_health_snapshot` — devolve o snapshot JSON
//!     (rápido, sem escrever em disco).
//!   - `generate_system_health_report` — grava o HTML do snapshot
//!     dentro de `<workspace>/reports/`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::occurrence_repo;
use crate::error::{Result, SicroError};
use crate::state::AppState;
use crate::workspace::{
    create_backup, open_workspace, run_global_backup, run_restore, BackupArtifact,
    GlobalBackupReport, GlobalCaseInput, HealthReportArtifact, RestoreReport,
    SystemHealthSnapshot, WorkspaceCounters,
};
use crate::workspace::{build_snapshot, count_occurrence_entities, render_and_save};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupArtifactJson {
    pub absolute_path: String,
    pub relative_path: String,
    pub filename: String,
    pub size_bytes: u64,
    pub hash_sha256: String,
    pub file_count: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<BackupArtifact> for BackupArtifactJson {
    fn from(a: BackupArtifact) -> Self {
        Self {
            absolute_path: a.absolute_path.to_string_lossy().into_owned(),
            relative_path: a.relative_path,
            filename: a.filename,
            size_bytes: a.size_bytes,
            hash_sha256: a.hash_sha256,
            file_count: a.file_count,
            created_at: a.created_at,
        }
    }
}

#[tauri::command]
pub async fn generate_workspace_backup(
    workspace_path: String,
    destination: Option<String>,
    bo_label: Option<String>,
) -> Result<BackupArtifactJson> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let dest = destination.map(PathBuf::from);

    let artifact = create_backup(&ws, dest.as_deref(), bo_label.as_deref())?;

    // Audit log (best-effort).
    if let Ok(conn) = open_connection(&ws.join(SQLITE_FILENAME)) {
        let _ = occurrence_repo::record_audit(
            &conn,
            Some(&manifest.occurrence_id),
            "workspace.backup_generated",
            Some("alpha"),
            Some("backup"),
            None,
            Some(&artifact.hash_sha256),
        );
    }

    Ok(BackupArtifactJson::from(artifact))
}

/// Backup geral (todos os casos) — incremental, 1 `.sicrobackup` por caso,
/// numa pasta-espelho no `destination`. Emite `global-backup-progress` por
/// caso para a UI acompanhar. A varredura/zip roda em `spawn_blocking` para
/// não travar o event loop.
#[tauri::command]
pub async fn generate_global_backup(
    app: AppHandle,
    cases: Vec<GlobalCaseInput>,
    destination: String,
) -> Result<GlobalBackupReport> {
    let dest = PathBuf::from(&destination);
    // Snapshot do app-settings.json (perfil/instituição/cabeçalhos) para o
    // conjunto carregar a "vida pericial" completa — não só os casos. Lido aqui
    // (lado async, com AppHandle) e gravado dentro do spawn_blocking. Best-effort:
    // usuário novo pode ainda não ter o arquivo.
    let settings_bytes = crate::commands::settings_commands::settings_path(&app)
        .ok()
        .and_then(|p| std::fs::read(&p).ok());
    tauri::async_runtime::spawn_blocking(move || {
        let report = run_global_backup(&cases, &dest, |p| {
            let _ = app.emit("global-backup-progress", p);
        })?;
        // config/app-settings.json no conjunto (estrutura v2).
        if let Some(bytes) = &settings_bytes {
            let cfg_dir = dest.join("config");
            let _ = std::fs::create_dir_all(&cfg_dir);
            if let Err(e) = std::fs::write(cfg_dir.join("app-settings.json"), bytes) {
                eprintln!("aviso: não consegui gravar config no backup: {e}");
            }
        }
        Ok(report)
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("backup geral (join): {e}")))?
}

/// Restaura um conjunto de backup (estrutura v2: `config/` + `casos/`) de
/// QUALQUER origem — HD externo, pendrive, nuvem, rede. Descompacta cada caso na
/// pasta local de casos (default: a pasta padrão do app), opcionalmente restaura
/// a config (perfil/instituição/cabeçalhos), e reindexa os recentes. A origem
/// nunca é tocada (§13); casos já existentes no destino são preservados (a menos
/// que `overwrite`). Emite `restore-backup-progress` por caso.
#[tauri::command]
pub async fn restore_backup(
    app: AppHandle,
    state: State<'_, AppState>,
    source_dir: String,
    cases_parent: Option<String>,
    restore_config: bool,
    overwrite: bool,
) -> Result<RestoreReport> {
    let source = PathBuf::from(&source_dir);
    let parent = cases_parent
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| state.default_workspace_parent().to_path_buf());

    // (1) Descompacta os casos (roda em spawn_blocking; emite progresso).
    let app_evt = app.clone();
    let source_cl = source.clone();
    let parent_cl = parent.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        run_restore(&source_cl, &parent_cl, overwrite, |p| {
            let _ = app_evt.emit("restore-backup-progress", p);
        })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("restaurar (join): {e}")))??;

    // (2) Config (perfil/instituição/cabeçalhos) — opcional.
    if restore_config {
        let cfg_src = source.join("config").join("app-settings.json");
        if cfg_src.is_file() {
            if let (Ok(dst), Ok(bytes)) = (
                crate::commands::settings_commands::settings_path(&app),
                std::fs::read(&cfg_src),
            ) {
                let _ = crate::filesystem::atomic_write_bytes(&dst, &bytes);
            }
        }
    }

    // (3) Reindexa: cada caso restaurado entra nos recentes (aparece na Home).
    for c in report.cases.iter().filter(|c| c.status == "restored") {
        if let Some(path) = &c.restored_path {
            if let Ok(opened) = open_workspace(&PathBuf::from(path)) {
                let _ = state.upsert_recent(
                    &opened.occurrence,
                    path.as_str(),
                    opened.manifest.workspace_id,
                );
            }
        }
    }

    Ok(report)
}

#[tauri::command]
pub async fn get_system_health_snapshot(
    workspace_path: Option<String>,
) -> Result<SystemHealthSnapshot> {
    let ws = workspace_path.map(PathBuf::from);
    build_snapshot(ws.as_deref())
}

/// Contagens por módulo de UM caso (leve: só consulta o banco). Alimenta o
/// índice global para os KPIs de produção da Home.
#[tauri::command]
pub async fn get_occurrence_counts(
    workspace_path: String,
) -> Result<WorkspaceCounters> {
    count_occurrence_entities(&PathBuf::from(&workspace_path))
}

#[tauri::command]
pub async fn generate_system_health_report(
    workspace_path: Option<String>,
) -> Result<HealthReportArtifact> {
    let ws = workspace_path.map(PathBuf::from);
    let snapshot = build_snapshot(ws.as_deref())?;
    let artifact = render_and_save(ws.as_deref(), &snapshot)?;

    // Audit log (best-effort, only when workspace given).
    if let Some(path) = ws.as_deref() {
        if let Ok(manifest) = Manifest::read(path) {
            if let Ok(mut conn) = open_connection(&path.join(SQLITE_FILENAME)) {
                let _ = run_migrations(&mut conn);
                let _ = occurrence_repo::record_audit(
                    &conn,
                    Some(&manifest.occurrence_id),
                    "system.health_report_generated",
                    Some("alpha"),
                    Some("health_report"),
                    None,
                    Some(&artifact.relative_path),
                );
            }
        }
    }
    Ok(artifact)
}
