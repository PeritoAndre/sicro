//! Repositories for the Image Editor (MVP 7).
//!
//! Três tabelas (migration 009), três pequenos módulos lógicos no
//! mesmo arquivo — mantém a árvore enxuta e segue o padrão do
//! `video_repo.rs`.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{
    ImageAnalysis, ImageExport, ImageOperationLog, ImageSourceKind,
};

// ---------------------------------------------------------------------------
// image_analyses

const ANALYSIS_COLS: &str = "
    id, occurrence_id, title, source_kind, source_id,
    original_relative_path, original_hash_sha256,
    analysis_relative_path, last_export_relative_path,
    status, metadata_json, created_at, updated_at
";

pub fn insert(conn: &Connection, a: &ImageAnalysis) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO image_analyses ({ANALYSIS_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
        ),
        params![
            a.id.to_string(),
            a.occurrence_id.to_string(),
            a.title,
            a.source_kind.as_str(),
            a.source_id,
            a.original_relative_path,
            a.original_hash_sha256,
            a.analysis_relative_path,
            a.last_export_relative_path,
            a.status,
            a.metadata_json,
            a.created_at.to_rfc3339(),
            a.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_by_occurrence(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<ImageAnalysis>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ANALYSIS_COLS} FROM image_analyses
         WHERE occurrence_id = ?1
         ORDER BY updated_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_analysis)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn find_by_id(conn: &Connection, id: &Uuid) -> Result<Option<ImageAnalysis>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {ANALYSIS_COLS} FROM image_analyses WHERE id = ?1"
    ))?;
    let row = stmt
        .query_row([id.to_string()], row_to_analysis)
        .optional()?;
    Ok(row)
}

pub fn touch_updated(
    conn: &Connection,
    id: &Uuid,
    when: DateTime<Utc>,
    last_export: Option<&str>,
) -> Result<()> {
    if let Some(rel) = last_export {
        conn.execute(
            "UPDATE image_analyses
             SET updated_at = ?1, last_export_relative_path = ?2
             WHERE id = ?3",
            params![when.to_rfc3339(), rel, id.to_string()],
        )?;
    } else {
        conn.execute(
            "UPDATE image_analyses SET updated_at = ?1 WHERE id = ?2",
            params![when.to_rfc3339(), id.to_string()],
        )?;
    }
    Ok(())
}

fn row_to_analysis(row: &Row<'_>) -> rusqlite::Result<ImageAnalysis> {
    let id = parse_uuid(row, "id")?;
    let occurrence_id = parse_uuid(row, "occurrence_id")?;
    let source_kind_raw: String = row.get("source_kind")?;
    let source_kind = ImageSourceKind::parse(&source_kind_raw)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    Ok(ImageAnalysis {
        id,
        occurrence_id,
        title: row.get("title")?,
        source_kind,
        source_id: row.get("source_id")?,
        original_relative_path: row.get("original_relative_path")?,
        original_hash_sha256: row.get("original_hash_sha256")?,
        analysis_relative_path: row.get("analysis_relative_path")?,
        last_export_relative_path: row.get("last_export_relative_path")?,
        status: row.get("status")?,
        metadata_json: row.get("metadata_json")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
        updated_at: parse_dt(row.get::<_, String>("updated_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// image_exports

const EXPORT_COLS: &str = "
    id, occurrence_id, image_analysis_id, output_relative_path,
    sidecar_relative_path, hash_sha256, width, height, format,
    created_at, operation_summary_json
";

pub fn insert_export(conn: &Connection, e: &ImageExport) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO image_exports ({EXPORT_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)"
        ),
        params![
            e.id.to_string(),
            e.occurrence_id.to_string(),
            e.image_analysis_id.to_string(),
            e.output_relative_path,
            e.sidecar_relative_path,
            e.hash_sha256,
            e.width,
            e.height,
            e.format,
            e.created_at.to_rfc3339(),
            e.operation_summary_json,
        ],
    )?;
    Ok(())
}

pub fn list_exports_by_occurrence(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<ImageExport>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EXPORT_COLS} FROM image_exports
         WHERE occurrence_id = ?1
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_export)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_export(row: &Row<'_>) -> rusqlite::Result<ImageExport> {
    Ok(ImageExport {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        image_analysis_id: parse_uuid(row, "image_analysis_id")?,
        output_relative_path: row.get("output_relative_path")?,
        sidecar_relative_path: row.get("sidecar_relative_path")?,
        hash_sha256: row.get("hash_sha256")?,
        width: row.get("width")?,
        height: row.get("height")?,
        format: row.get("format")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
        operation_summary_json: row.get("operation_summary_json")?,
    })
}

// ---------------------------------------------------------------------------
// image_operation_logs

pub fn insert_log(
    conn: &Connection,
    occurrence_id: &Uuid,
    image_analysis_id: &Uuid,
    action: &str,
    details_json: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO image_operation_logs
         (occurrence_id, image_analysis_id, action, details_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            occurrence_id.to_string(),
            image_analysis_id.to_string(),
            action,
            details_json,
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_logs_for_analysis(
    conn: &Connection,
    image_analysis_id: &Uuid,
    limit: i64,
) -> Result<Vec<ImageOperationLog>> {
    let mut stmt = conn.prepare(
        "SELECT id, occurrence_id, image_analysis_id, action, details_json, created_at
         FROM image_operation_logs
         WHERE image_analysis_id = ?1
         ORDER BY created_at DESC, id DESC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(
            params![image_analysis_id.to_string(), limit.max(1)],
            row_to_log,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_log(row: &Row<'_>) -> rusqlite::Result<ImageOperationLog> {
    Ok(ImageOperationLog {
        id: row.get("id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        image_analysis_id: parse_uuid(row, "image_analysis_id")?,
        action: row.get("action")?,
        details_json: row.get("details_json")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// Helpers

fn parse_uuid(row: &Row<'_>, name: &str) -> rusqlite::Result<Uuid> {
    let s: String = row.get(name)?;
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}
