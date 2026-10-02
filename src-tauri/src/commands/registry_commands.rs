//! Comandos Tauri da Central de Evidências. O `occurrence_id` vem sempre do
//! Manifest, nunca do frontend.

use std::path::PathBuf;

use chrono::Utc;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::error::{Result, SicroError};
use crate::filesystem::{atomic_write_bytes, resolve_workspace_relative};
use crate::models::{
    EvidenceRegistryItem, IntegrityReportArtifact, RegistrySummary, VerifyOptions,
    WorkspaceIntegrityReport,
};
use crate::registry;
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

/// Registro consolidado, sem verificação de disco: barato para chamar a cada render.
#[tauri::command]
pub async fn list_evidence_registry_items(
    workspace_path: String,
) -> Result<Vec<EvidenceRegistryItem>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    registry::build_registry(&conn, &manifest.occurrence_id)
}

/// Contadores da aba Resumo: sonda leve de integridade, sem recomputar hashes.
#[tauri::command]
pub async fn get_evidence_registry_summary(
    workspace_path: String,
) -> Result<RegistrySummary> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let report = registry::verify_workspace(
        &conn,
        &ws,
        &manifest.occurrence_id,
        &VerifyOptions { deep: false },
    )?;
    Ok(report.summary)
}

/// Verificação completa; `deep: true` recomputa os SHA-256.
#[tauri::command]
pub async fn verify_workspace_integrity(
    workspace_path: String,
    options: Option<VerifyOptions>,
) -> Result<WorkspaceIntegrityReport> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    registry::verify_workspace(
        &conn,
        &ws,
        &manifest.occurrence_id,
        &options.unwrap_or_default(),
    )
}

/// Todos os `evidence_links` da ocorrência (a versão por laudo é
/// `list_evidence_links_for_laudo`).
#[tauri::command]
pub async fn list_evidence_links(
    workspace_path: String,
) -> Result<Vec<crate::models::EvidenceLink>> {
    use crate::database::repositories::evidence_link_repo;
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    evidence_link_repo::list_for_occurrence(&conn, &manifest.occurrence_id)
}

/// Abre com o app padrão do SO; o caminho resolvido tem de ficar dentro do workspace.
#[tauri::command]
pub async fn open_evidence_file(
    workspace_path: String,
    relative_path: String,
) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let abs = resolve_workspace_relative(&ws, &relative_path)?;
    if !abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "asset not found at {}",
            abs.display()
        )));
    }
    crate::commands::os_open::open_with_os(&abs)
}

/// Revela no explorador de arquivos (ou abre a pasta, onde "revelar" não existe).
#[tauri::command]
pub async fn reveal_evidence_in_folder(
    workspace_path: String,
    relative_path: String,
) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let abs = resolve_workspace_relative(&ws, &relative_path)?;
    if !abs.exists() {
        return Err(SicroError::Filesystem(format!(
            "asset not found at {}",
            abs.display()
        )));
    }
    reveal_with_os(&abs)
}

/// Verificação completa + relatório HTML gravado em `reports/`.
#[tauri::command]
pub async fn generate_workspace_integrity_report(
    workspace_path: String,
    options: Option<VerifyOptions>,
) -> Result<IntegrityReportArtifact> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let report = registry::verify_workspace(
        &conn,
        &ws,
        &manifest.occurrence_id,
        &options.unwrap_or_default(),
    )?;

    let html = registry::render_html_report(&report);
    let now = Utc::now();
    let rel = crate::registry::report::report_filename(&now);
    let abs = resolve_workspace_relative(&ws, &rel)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SicroError::Filesystem(format!(
                "cannot create reports/ directory: {e}"
            ))
        })?;
    }
    atomic_write_bytes(&abs, html.as_bytes())?;

    Ok(IntegrityReportArtifact {
        relative_path: rel,
        generated_at: now,
        overall_status: report.summary.overall_status,
        item_count: report.summary.total_items,
    })
}

// ---------------------------------------------------------------------------
// Integração com o SO (`open_with_os` vive em `commands::os_open`, compartilhado)

#[cfg(target_os = "windows")]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    crate::tools::command("explorer")
        .args(["/select,", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    crate::tools::command("open")
        .args(["-R", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    // xdg-open não "revela": abre a pasta que contém o arquivo.
    let dir = path.parent().unwrap_or(path);
    crate::tools::command("xdg-open")
        .arg(dir)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}
