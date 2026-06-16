//! Tauri commands for the Central de Evidências (MVP 5).
//!
//! All commands resolve `workspace_path` → `Manifest` → `occurrence_id`
//! the same way the other modules do. They never trust the frontend
//! with an `occurrence_id` because the manifest IS the source of truth.

use std::path::PathBuf;
use std::process::Command;

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

/// Build the consolidated registry. Lightweight — no filesystem
/// verification, so it's safe to call on every render.
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

/// Return the counters used by the "Resumo" tab. Runs the lightweight
/// integrity probe so the UI can show "12 fotos, 0 ausentes, status
/// íntegro" without paying for a deep hash check.
#[tauri::command]
pub async fn get_evidence_registry_summary(
    workspace_path: String,
) -> Result<RegistrySummary> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    // Probe lightly: build items, run verify with deep=false, summarise.
    let report = registry::verify_workspace(
        &conn,
        &ws,
        &manifest.occurrence_id,
        &VerifyOptions { deep: false },
    )?;
    Ok(report.summary)
}

/// Full integrity verification. Pass `deep: true` to recompute SHA-256s.
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

/// List every `evidence_links` row of the active occurrence.
///
/// Complements `list_evidence_links_for_laudo` (MVP 4) — that one is
/// laudo-scoped, this one is occurrence-scoped (used by the "Laudos e
/// vínculos" tab of the Central).
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

/// Open an evidence file with the OS default handler. Path safety is
/// enforced — the resolved absolute path must be under `workspace_path`.
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
    open_with_os(&abs)
}

/// Reveal an evidence file in the platform's file explorer.
/// Falls back to "open the containing folder" when "reveal" isn't supported.
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

/// Run a full verification and persist the HTML report under
/// `reports/`. Returns a descriptor so the UI can open the report.
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
// OS integration helpers

#[cfg(target_os = "windows")]
fn open_with_os(path: &std::path::Path) -> Result<()> {
    Command::new("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao abrir: {e}")))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_with_os(path: &std::path::Path) -> Result<()> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao abrir: {e}")))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_with_os(path: &std::path::Path) -> Result<()> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao abrir: {e}")))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    Command::new("explorer")
        .args(["/select,", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    Command::new("open")
        .args(["-R", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    // Best-effort: open the containing folder.
    let dir = path.parent().unwrap_or(path);
    Command::new("xdg-open")
        .arg(dir)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}
