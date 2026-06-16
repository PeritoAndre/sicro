//! Repositório do módulo Documentoscopia (SQLite via rusqlite).
//!
//! Padrão idêntico aos demais repos: colunas em const, closures `row_to_*`,
//! `insert_*` / `list_*` / `find_*`. Tudo determinístico e auditável.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{
    ComparisonSession, DetectedField, DocumentAnalysis, DocumentCaseFile, DocumentLog,
    DocumentPage, DocumentRegion, OcrRun, OcrTextBlock,
};

// ---------------------------------------------------------------------------
// Helpers de mapeamento

fn parse_uuid(row: &Row<'_>, col: &str) -> rusqlite::Result<Uuid> {
    let s: String = row.get(col)?;
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_opt_uuid(row: &Row<'_>, col: &str) -> rusqlite::Result<Option<Uuid>> {
    let s: Option<String> = row.get(col)?;
    match s {
        Some(s) => Uuid::parse_str(&s).map(Some).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        }),
        None => Ok(None),
    }
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

// ---------------------------------------------------------------------------
// Documentos

const DOC_COLS: &str = "id, occurrence_id, title, original_filename, relative_path, file_type, \
    extension, doc_type, sha256, size_bytes, page_count, has_text_layer, status, metadata_json, \
    notes, imported_by, created_at, updated_at";

fn row_to_document(row: &Row<'_>) -> rusqlite::Result<DocumentCaseFile> {
    Ok(DocumentCaseFile {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        title: row.get("title")?,
        original_filename: row.get("original_filename")?,
        relative_path: row.get("relative_path")?,
        file_type: row.get("file_type")?,
        extension: row.get("extension")?,
        doc_type: row.get("doc_type")?,
        sha256: row.get("sha256")?,
        size_bytes: row.get::<_, i64>("size_bytes")?.max(0) as u64,
        page_count: row.get("page_count")?,
        has_text_layer: row.get::<_, i64>("has_text_layer")? != 0,
        status: row.get("status")?,
        metadata_json: row.get("metadata_json")?,
        notes: row.get("notes")?,
        imported_by: row.get("imported_by")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
        updated_at: parse_dt(row.get::<_, String>("updated_at")?)?,
    })
}

pub fn insert_document(conn: &Connection, d: &DocumentCaseFile) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO doc_documents ({DOC_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)"
        ),
        params![
            d.id.to_string(),
            d.occurrence_id.to_string(),
            d.title,
            d.original_filename,
            d.relative_path,
            d.file_type,
            d.extension,
            d.doc_type,
            d.sha256,
            d.size_bytes as i64,
            d.page_count,
            d.has_text_layer as i64,
            d.status,
            d.metadata_json,
            d.notes,
            d.imported_by,
            d.created_at.to_rfc3339(),
            d.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_documents(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<DocumentCaseFile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {DOC_COLS} FROM doc_documents WHERE occurrence_id = ?1 ORDER BY updated_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_document)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn find_document_by_id(conn: &Connection, id: &Uuid) -> Result<Option<DocumentCaseFile>> {
    let mut stmt =
        conn.prepare(&format!("SELECT {DOC_COLS} FROM doc_documents WHERE id = ?1"))?;
    Ok(stmt
        .query_row([id.to_string()], row_to_document)
        .optional()?)
}

pub fn find_document_by_sha(
    conn: &Connection,
    occurrence_id: &Uuid,
    sha256: &str,
) -> Result<Option<DocumentCaseFile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {DOC_COLS} FROM doc_documents WHERE occurrence_id = ?1 AND sha256 = ?2"
    ))?;
    Ok(stmt
        .query_row(params![occurrence_id.to_string(), sha256], row_to_document)
        .optional()?)
}

pub fn update_document_meta(
    conn: &Connection,
    id: &Uuid,
    title: &str,
    doc_type: &str,
    notes: &str,
    updated_at: &DateTime<Utc>,
) -> Result<()> {
    conn.execute(
        "UPDATE doc_documents SET title = ?2, doc_type = ?3, notes = ?4, updated_at = ?5 \
         WHERE id = ?1",
        params![
            id.to_string(),
            title,
            doc_type,
            notes,
            updated_at.to_rfc3339()
        ],
    )?;
    Ok(())
}

pub fn update_document_status(
    conn: &Connection,
    id: &Uuid,
    status: &str,
    updated_at: &DateTime<Utc>,
) -> Result<()> {
    conn.execute(
        "UPDATE doc_documents SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id.to_string(), status, updated_at.to_rfc3339()],
    )?;
    Ok(())
}

pub fn update_document_pageinfo(
    conn: &Connection,
    id: &Uuid,
    page_count: i64,
    has_text_layer: bool,
    metadata_json: &str,
    updated_at: &DateTime<Utc>,
) -> Result<()> {
    conn.execute(
        "UPDATE doc_documents SET page_count = ?2, has_text_layer = ?3, metadata_json = ?4, \
         updated_at = ?5 WHERE id = ?1",
        params![
            id.to_string(),
            page_count,
            has_text_layer as i64,
            metadata_json,
            updated_at.to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Remove o documento e TODOS os seus derivados (páginas, OCR, blocos, campos,
/// regiões, análises, logs, comparações). O arquivo em disco é removido pelo
/// comando (camada de filesystem), não aqui.
pub fn delete_document(conn: &Connection, id: &Uuid) -> Result<()> {
    let sid = id.to_string();
    conn.execute("DELETE FROM doc_text_blocks WHERE document_id = ?1", [&sid])?;
    conn.execute("DELETE FROM doc_ocr_runs WHERE document_id = ?1", [&sid])?;
    conn.execute("DELETE FROM doc_fields WHERE document_id = ?1", [&sid])?;
    conn.execute("DELETE FROM doc_regions WHERE document_id = ?1", [&sid])?;
    conn.execute("DELETE FROM doc_analyses WHERE document_id = ?1", [&sid])?;
    conn.execute("DELETE FROM doc_pages WHERE document_id = ?1", [&sid])?;
    conn.execute("DELETE FROM doc_logs WHERE document_id = ?1", [&sid])?;
    conn.execute(
        "DELETE FROM doc_comparisons WHERE questioned_document_id = ?1 OR reference_document_id = ?1",
        [&sid],
    )?;
    conn.execute("DELETE FROM doc_documents WHERE id = ?1", [&sid])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Páginas

const PAGE_COLS: &str = "id, document_id, page_number, width, height, rotation, dpi, \
    rendered_path, thumbnail_path, created_at";

fn row_to_page(row: &Row<'_>) -> rusqlite::Result<DocumentPage> {
    Ok(DocumentPage {
        id: parse_uuid(row, "id")?,
        document_id: parse_uuid(row, "document_id")?,
        page_number: row.get("page_number")?,
        width: row.get("width")?,
        height: row.get("height")?,
        rotation: row.get("rotation")?,
        dpi: row.get("dpi")?,
        rendered_path: row.get("rendered_path")?,
        thumbnail_path: row.get("thumbnail_path")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

pub fn insert_page(conn: &Connection, p: &DocumentPage) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO doc_pages ({PAGE_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"
        ),
        params![
            p.id.to_string(),
            p.document_id.to_string(),
            p.page_number,
            p.width,
            p.height,
            p.rotation,
            p.dpi,
            p.rendered_path,
            p.thumbnail_path,
            p.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_pages(conn: &Connection, document_id: &Uuid) -> Result<Vec<DocumentPage>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PAGE_COLS} FROM doc_pages WHERE document_id = ?1 ORDER BY page_number"
    ))?;
    let rows = stmt
        .query_map([document_id.to_string()], row_to_page)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Execuções de OCR

const RUN_COLS: &str = "id, document_id, page_number, engine, engine_version, language, mode, \
    status, avg_confidence, block_count, parameters_json, created_at";

fn row_to_run(row: &Row<'_>) -> rusqlite::Result<OcrRun> {
    Ok(OcrRun {
        id: parse_uuid(row, "id")?,
        document_id: parse_uuid(row, "document_id")?,
        page_number: row.get("page_number")?,
        engine: row.get("engine")?,
        engine_version: row.get("engine_version")?,
        language: row.get("language")?,
        mode: row.get("mode")?,
        status: row.get("status")?,
        avg_confidence: row.get("avg_confidence")?,
        block_count: row.get("block_count")?,
        parameters_json: row.get("parameters_json")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

pub fn insert_ocr_run(conn: &Connection, r: &OcrRun) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO doc_ocr_runs ({RUN_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
        ),
        params![
            r.id.to_string(),
            r.document_id.to_string(),
            r.page_number,
            r.engine,
            r.engine_version,
            r.language,
            r.mode,
            r.status,
            r.avg_confidence,
            r.block_count,
            r.parameters_json,
            r.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_ocr_runs(conn: &Connection, document_id: &Uuid) -> Result<Vec<OcrRun>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {RUN_COLS} FROM doc_ocr_runs WHERE document_id = ?1 ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([document_id.to_string()], row_to_run)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Blocos de texto

const BLK_COLS: &str = "id, ocr_run_id, document_id, page_number, text, confidence, bbox_x, \
    bbox_y, bbox_w, bbox_h, block_type, reading_order, corrected_text, reviewed, created_at";

fn row_to_block(row: &Row<'_>) -> rusqlite::Result<OcrTextBlock> {
    Ok(OcrTextBlock {
        id: parse_uuid(row, "id")?,
        ocr_run_id: parse_uuid(row, "ocr_run_id")?,
        document_id: parse_uuid(row, "document_id")?,
        page_number: row.get("page_number")?,
        text: row.get("text")?,
        confidence: row.get("confidence")?,
        bbox_x: row.get("bbox_x")?,
        bbox_y: row.get("bbox_y")?,
        bbox_w: row.get("bbox_w")?,
        bbox_h: row.get("bbox_h")?,
        block_type: row.get("block_type")?,
        reading_order: row.get("reading_order")?,
        corrected_text: row.get("corrected_text")?,
        reviewed: row.get::<_, i64>("reviewed")? != 0,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

pub fn insert_text_block(conn: &Connection, b: &OcrTextBlock) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO doc_text_blocks ({BLK_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)"
        ),
        params![
            b.id.to_string(),
            b.ocr_run_id.to_string(),
            b.document_id.to_string(),
            b.page_number,
            b.text,
            b.confidence,
            b.bbox_x,
            b.bbox_y,
            b.bbox_w,
            b.bbox_h,
            b.block_type,
            b.reading_order,
            b.corrected_text,
            b.reviewed as i64,
            b.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_blocks_for_run(conn: &Connection, run_id: &Uuid) -> Result<Vec<OcrTextBlock>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {BLK_COLS} FROM doc_text_blocks WHERE ocr_run_id = ?1 \
         ORDER BY page_number, reading_order"
    ))?;
    let rows = stmt
        .query_map([run_id.to_string()], row_to_block)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn set_block_review(
    conn: &Connection,
    block_id: &Uuid,
    corrected_text: Option<&str>,
    reviewed: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE doc_text_blocks SET corrected_text = ?2, reviewed = ?3 WHERE id = ?1",
        params![block_id.to_string(), corrected_text, reviewed as i64],
    )?;
    Ok(())
}

pub fn delete_text_block(conn: &Connection, block_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM doc_text_blocks WHERE id = ?1",
        params![block_id.to_string()],
    )?;
    Ok(())
}

pub fn set_block_bbox(
    conn: &Connection,
    block_id: &Uuid,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Result<()> {
    conn.execute(
        "UPDATE doc_text_blocks SET bbox_x = ?2, bbox_y = ?3, bbox_w = ?4, bbox_h = ?5 \
         WHERE id = ?1",
        params![block_id.to_string(), x, y, w, h],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Campos detectados

const FIELD_COLS: &str = "id, document_id, page_number, field_name, field_value, confidence, \
    source, bbox_x, bbox_y, bbox_w, bbox_h, reviewed, corrected_value, created_at";

fn row_to_field(row: &Row<'_>) -> rusqlite::Result<DetectedField> {
    Ok(DetectedField {
        id: parse_uuid(row, "id")?,
        document_id: parse_uuid(row, "document_id")?,
        page_number: row.get("page_number")?,
        field_name: row.get("field_name")?,
        field_value: row.get("field_value")?,
        confidence: row.get("confidence")?,
        source: row.get("source")?,
        bbox_x: row.get("bbox_x")?,
        bbox_y: row.get("bbox_y")?,
        bbox_w: row.get("bbox_w")?,
        bbox_h: row.get("bbox_h")?,
        reviewed: row.get::<_, i64>("reviewed")? != 0,
        corrected_value: row.get("corrected_value")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

pub fn insert_field(conn: &Connection, f: &DetectedField) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO doc_fields ({FIELD_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"
        ),
        params![
            f.id.to_string(),
            f.document_id.to_string(),
            f.page_number,
            f.field_name,
            f.field_value,
            f.confidence,
            f.source,
            f.bbox_x,
            f.bbox_y,
            f.bbox_w,
            f.bbox_h,
            f.reviewed as i64,
            f.corrected_value,
            f.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_fields(conn: &Connection, document_id: &Uuid) -> Result<Vec<DetectedField>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {FIELD_COLS} FROM doc_fields WHERE document_id = ?1 ORDER BY created_at"
    ))?;
    let rows = stmt
        .query_map([document_id.to_string()], row_to_field)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Remove campos de uma origem específica (ex.: re-rodar heurística sem apagar
/// os campos inseridos manualmente pelo perito).
pub fn delete_fields_by_source(
    conn: &Connection,
    document_id: &Uuid,
    source: &str,
) -> Result<()> {
    conn.execute(
        "DELETE FROM doc_fields WHERE document_id = ?1 AND source = ?2",
        params![document_id.to_string(), source],
    )?;
    Ok(())
}

pub fn set_field_review(
    conn: &Connection,
    field_id: &Uuid,
    corrected_value: Option<&str>,
    reviewed: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE doc_fields SET corrected_value = ?2, reviewed = ?3 WHERE id = ?1",
        params![field_id.to_string(), corrected_value, reviewed as i64],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Regiões

const REGION_COLS: &str = "id, document_id, page_number, region_type, bbox_x, bbox_y, bbox_w, \
    bbox_h, label, confidence, notes, created_at";

fn row_to_region(row: &Row<'_>) -> rusqlite::Result<DocumentRegion> {
    Ok(DocumentRegion {
        id: parse_uuid(row, "id")?,
        document_id: parse_uuid(row, "document_id")?,
        page_number: row.get("page_number")?,
        region_type: row.get("region_type")?,
        bbox_x: row.get("bbox_x")?,
        bbox_y: row.get("bbox_y")?,
        bbox_w: row.get("bbox_w")?,
        bbox_h: row.get("bbox_h")?,
        label: row.get("label")?,
        confidence: row.get("confidence")?,
        notes: row.get("notes")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

pub fn insert_region(conn: &Connection, r: &DocumentRegion) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO doc_regions ({REGION_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)"
        ),
        params![
            r.id.to_string(),
            r.document_id.to_string(),
            r.page_number,
            r.region_type,
            r.bbox_x,
            r.bbox_y,
            r.bbox_w,
            r.bbox_h,
            r.label,
            r.confidence,
            r.notes,
            r.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_regions(conn: &Connection, document_id: &Uuid) -> Result<Vec<DocumentRegion>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {REGION_COLS} FROM doc_regions WHERE document_id = ?1 ORDER BY created_at"
    ))?;
    let rows = stmt
        .query_map([document_id.to_string()], row_to_region)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_region(conn: &Connection, region_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM doc_regions WHERE id = ?1",
        [region_id.to_string()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Análises

const ANALYSIS_COLS: &str =
    "id, document_id, analysis_type, status, parameters_json, result_json, summary, created_at";

fn row_to_analysis(row: &Row<'_>) -> rusqlite::Result<DocumentAnalysis> {
    Ok(DocumentAnalysis {
        id: parse_uuid(row, "id")?,
        document_id: parse_uuid(row, "document_id")?,
        analysis_type: row.get("analysis_type")?,
        status: row.get("status")?,
        parameters_json: row.get("parameters_json")?,
        result_json: row.get("result_json")?,
        summary: row.get("summary")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

pub fn insert_analysis(conn: &Connection, a: &DocumentAnalysis) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO doc_analyses ({ANALYSIS_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)"
        ),
        params![
            a.id.to_string(),
            a.document_id.to_string(),
            a.analysis_type,
            a.status,
            a.parameters_json,
            a.result_json,
            a.summary,
            a.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_analyses(conn: &Connection, document_id: &Uuid) -> Result<Vec<DocumentAnalysis>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ANALYSIS_COLS} FROM doc_analyses WHERE document_id = ?1 ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([document_id.to_string()], row_to_analysis)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Comparações

const CMP_COLS: &str = "id, occurrence_id, questioned_document_id, reference_document_id, \
    comparison_type, results_json, summary, created_at";

fn row_to_comparison(row: &Row<'_>) -> rusqlite::Result<ComparisonSession> {
    Ok(ComparisonSession {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        questioned_document_id: parse_uuid(row, "questioned_document_id")?,
        reference_document_id: parse_uuid(row, "reference_document_id")?,
        comparison_type: row.get("comparison_type")?,
        results_json: row.get("results_json")?,
        summary: row.get("summary")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

pub fn insert_comparison(conn: &Connection, c: &ComparisonSession) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO doc_comparisons ({CMP_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)"
        ),
        params![
            c.id.to_string(),
            c.occurrence_id.to_string(),
            c.questioned_document_id.to_string(),
            c.reference_document_id.to_string(),
            c.comparison_type,
            c.results_json,
            c.summary,
            c.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_comparisons(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<ComparisonSession>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {CMP_COLS} FROM doc_comparisons WHERE occurrence_id = ?1 ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_comparison)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Logs / auditoria

const LOG_COLS: &str = "id, document_id, occurrence_id, action, parameters_json, result, \
    source_hash, output_hash, actor, created_at";

fn row_to_log(row: &Row<'_>) -> rusqlite::Result<DocumentLog> {
    Ok(DocumentLog {
        id: parse_uuid(row, "id")?,
        document_id: parse_opt_uuid(row, "document_id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        action: row.get("action")?,
        parameters_json: row.get("parameters_json")?,
        result: row.get("result")?,
        source_hash: row.get("source_hash")?,
        output_hash: row.get("output_hash")?,
        actor: row.get("actor")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

pub fn insert_log(conn: &Connection, l: &DocumentLog) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO doc_logs ({LOG_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"
        ),
        params![
            l.id.to_string(),
            l.document_id.map(|u| u.to_string()),
            l.occurrence_id.to_string(),
            l.action,
            l.parameters_json,
            l.result,
            l.source_hash,
            l.output_hash,
            l.actor,
            l.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_logs(conn: &Connection, document_id: &Uuid) -> Result<Vec<DocumentLog>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {LOG_COLS} FROM doc_logs WHERE document_id = ?1 ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([document_id.to_string()], row_to_log)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
