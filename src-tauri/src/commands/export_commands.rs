//! Tauri commands for the Export Engine (Spike C).
//!
//! Boundary contract:
//!   - `export_laudo_html(workspace, laudo_id, html)`   →  Export row + file in exports/html/
//!   - `export_laudo_pdf(workspace, laudo_id, html)`    →  Export row + file in exports/pdf/
//!   - `export_laudo_docx(workspace, laudo_id)`         →  Export row + file in exports/docx/
//!   - `list_laudo_exports(workspace, laudo_id)`        →  Vec<Export>
//!
//! The DOCX command takes no HTML because it reads the `.sicrodoc` directly
//! and walks the TipTap JSON — the front-end's HTML is intentionally NOT the
//! source of truth for DOCX export.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;
use uuid::Uuid;

/// Injeta `__field_values` no envelope do `.sicrodoc` (somente em memória, nunca
/// gravado de volta) — o mapa {campo: valor} que o FRONT resolveu (metadata +
/// occurrence + catálogo). O walker DOCX usa esse mapa pra trocar as pílulas
/// `{numero_laudo}` etc. pelo valor real, mantendo o resolver do front como
/// única fonte de verdade. None → walker mantém o comportamento de fallback.
fn inject_field_values(envelope: &mut serde_json::Value, field_values: Option<HashMap<String, String>>) {
    if let Some(map) = field_values {
        if let Some(obj) = envelope.as_object_mut() {
            obj.insert(
                "__field_values".to_string(),
                serde_json::to_value(map).unwrap_or(serde_json::Value::Null),
            );
        }
    }
}

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{export_repo, laudo_repo, occurrence_repo};
use crate::error::{Result, SicroError};
use crate::exporters::{docx as docx_export, html as html_export, paths as export_paths, pdf as pdf_export};
use crate::models::{Export, ExportKind};
use crate::workspace::manifest::SQLITE_FILENAME;

#[tauri::command]
pub async fn export_laudo_html(
    workspace_path: String,
    laudo_id: String,
    html: String,
) -> Result<Export> {
    let ws = PathBuf::from(&workspace_path);
    let id = parse_laudo_id(&laudo_id)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let laudo = laudo_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("laudo {} not found", id)))?;

    let target = export_paths::resolve_export_target(&ws, &id, ExportKind::Html)?;
    html_export::write_html(&target.absolute_path, &html)?;

    let file_size = std::fs::metadata(&target.absolute_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let export = Export {
        id: Uuid::new_v4(),
        occurrence_id: laudo.occurrence_id,
        laudo_id: id,
        kind: ExportKind::Html,
        relative_path: target.relative_path,
        file_size,
        created_at: Utc::now(),
    };
    export_repo::insert(&conn, &export)?;
    occurrence_repo::record_audit(
        &conn,
        Some(&laudo.occurrence_id),
        "laudo.exported_html",
        Some("laudo"),
        Some("export"),
        Some(&export.id),
        None,
    )?;

    Ok(export)
}

#[tauri::command]
pub async fn export_laudo_pdf(
    workspace_path: String,
    laudo_id: String,
    html: String,
    // Quando presente, gera o PDF com este rodapé em TODA página (via CDP).
    // O front monta "Folha {pageNumber} de {totalPages}" quando o laudo usa
    // os campos {page}/{pages}. `None` = PDF normal (CLI).
    page_footer: Option<String>,
) -> Result<Export> {
    let ws = PathBuf::from(&workspace_path);
    let id = parse_laudo_id(&laudo_id)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let laudo = laudo_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("laudo {} not found", id)))?;

    let target = export_paths::resolve_export_target(&ws, &id, ExportKind::Pdf)?;
    let cache_dir = ws.join("cache");
    std::fs::create_dir_all(&cache_dir)?;

    pdf_export::render_html_to_pdf(
        &html,
        &cache_dir,
        &target.absolute_path,
        page_footer.as_deref(),
    )?;

    let file_size = std::fs::metadata(&target.absolute_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let export = Export {
        id: Uuid::new_v4(),
        occurrence_id: laudo.occurrence_id,
        laudo_id: id,
        kind: ExportKind::Pdf,
        relative_path: target.relative_path,
        file_size,
        created_at: Utc::now(),
    };
    export_repo::insert(&conn, &export)?;
    occurrence_repo::record_audit(
        &conn,
        Some(&laudo.occurrence_id),
        "laudo.exported_pdf",
        Some("laudo"),
        Some("export"),
        Some(&export.id),
        None,
    )?;

    Ok(export)
}

#[tauri::command]
pub async fn export_laudo_docx(
    workspace_path: String,
    laudo_id: String,
    // Mapa {campo: valor} resolvido pelo front (metadata + occurrence). Usado
    // pra trocar as pílulas `{numero_laudo}` etc. pelo valor real no .docx.
    field_values: Option<HashMap<String, String>>,
) -> Result<Export> {
    let ws = PathBuf::from(&workspace_path);
    let id = parse_laudo_id(&laudo_id)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let laudo = laudo_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("laudo {} not found", id)))?;

    // Read the .sicrodoc — it's the source of truth, not the HTML.
    let sicrodoc_path = ws.join(&laudo.relative_path);
    let bytes = std::fs::read(&sicrodoc_path).map_err(|e| {
        SicroError::Workspace(format!(
            "could not read .sicrodoc at {}: {}",
            sicrodoc_path.display(),
            e
        ))
    })?;
    let mut envelope: serde_json::Value = serde_json::from_slice(&bytes)?;
    inject_field_values(&mut envelope, field_values);

    let target = export_paths::resolve_export_target(&ws, &id, ExportKind::Docx)?;
    // Hand the workspace root to the walker so figure/storyboard nodes with
    // `relative_path` get their PNG/JPG bytes embedded as real images. The
    // walker degrades to a placeholder paragraph if a file is missing.
    docx_export::render_doc_to_docx(&envelope, &target.absolute_path, Some(&ws))?;

    let file_size = std::fs::metadata(&target.absolute_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let export = Export {
        id: Uuid::new_v4(),
        occurrence_id: laudo.occurrence_id,
        laudo_id: id,
        kind: ExportKind::Docx,
        relative_path: target.relative_path,
        file_size,
        created_at: Utc::now(),
    };
    export_repo::insert(&conn, &export)?;
    occurrence_repo::record_audit(
        &conn,
        Some(&laudo.occurrence_id),
        "laudo.exported_docx",
        Some("laudo"),
        Some("export"),
        Some(&export.id),
        None,
    )?;

    Ok(export)
}

/// Exporta o laudo em PDF VIA LIBREOFFICE: gera o `.docx` (diagramação fiel) e
/// o converte com o LibreOffice headless. Resultado: numeração de página no
/// lugar, em tabela e em cabeçalho que se repete — o que o motor do navegador
/// não faz. Requer o LibreOffice instalado (Configurações › Dependências).
#[tauri::command]
pub async fn export_laudo_pdf_libreoffice(
    workspace_path: String,
    laudo_id: String,
    // Quando true, gera PDF/A (ISO 19005, arquivamento de longo prazo) em vez
    // do PDF comum. Mesmo pipeline, só muda o filtro do LibreOffice.
    pdf_a: Option<bool>,
    // Mapa {campo: valor} resolvido pelo front (ver export_laudo_docx).
    field_values: Option<HashMap<String, String>>,
) -> Result<Export> {
    let pdf_a = pdf_a.unwrap_or(false);
    let ws = PathBuf::from(&workspace_path);
    let id = parse_laudo_id(&laudo_id)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    let laudo = laudo_repo::find_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("laudo {} not found", id)))?;

    // 1. Gera o .docx (fonte da diagramação) num temporário no cache.
    let sicrodoc_path = ws.join(&laudo.relative_path);
    let bytes = std::fs::read(&sicrodoc_path).map_err(|e| {
        SicroError::Workspace(format!(
            "could not read .sicrodoc at {}: {}",
            sicrodoc_path.display(),
            e
        ))
    })?;
    let mut envelope: serde_json::Value = serde_json::from_slice(&bytes)?;
    inject_field_values(&mut envelope, field_values);
    let cache_dir = ws.join("cache");
    std::fs::create_dir_all(&cache_dir)?;
    let docx_tmp = cache_dir.join(format!("lo_{id}.docx"));
    docx_export::render_doc_to_docx(&envelope, &docx_tmp, Some(&ws))?;

    // 2. Converte DOCX → PDF via LibreOffice headless (thread bloqueante).
    let target = export_paths::resolve_export_target(&ws, &id, ExportKind::Pdf)?;
    let docx_for_task = docx_tmp.clone();
    let pdf_for_task = target.absolute_path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::commands::libreoffice_commands::convert_docx_to_pdf(
            &docx_for_task,
            &pdf_for_task,
            pdf_a,
        )
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa de conversão: {e}")))??;
    let _ = std::fs::remove_file(&docx_tmp);

    let file_size = std::fs::metadata(&target.absolute_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let export = Export {
        id: Uuid::new_v4(),
        occurrence_id: laudo.occurrence_id,
        laudo_id: id,
        kind: ExportKind::Pdf,
        relative_path: target.relative_path,
        file_size,
        created_at: Utc::now(),
    };
    export_repo::insert(&conn, &export)?;
    occurrence_repo::record_audit(
        &conn,
        Some(&laudo.occurrence_id),
        if pdf_a {
            "laudo.exported_pdf_a"
        } else {
            "laudo.exported_pdf_libreoffice"
        },
        Some("laudo"),
        Some("export"),
        Some(&export.id),
        None,
    )?;

    Ok(export)
}

#[tauri::command]
pub async fn list_laudo_exports(
    workspace_path: String,
    laudo_id: String,
) -> Result<Vec<Export>> {
    let ws = PathBuf::from(&workspace_path);
    let id = parse_laudo_id(&laudo_id)?;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    export_repo::list_by_laudo(&conn, &id)
}

fn parse_laudo_id(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value)
        .map_err(|e| SicroError::Validation(format!("invalid laudo id: {e}")))
}
