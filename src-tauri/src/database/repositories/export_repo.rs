//! Read/write helpers for the `exports` table.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{Export, ExportKind};

const COLUMNS: &str = "
    id, occurrence_id, laudo_id, kind, relative_path, file_size, created_at
";

pub fn insert(conn: &Connection, export: &Export) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO exports ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        ),
        params![
            export.id.to_string(),
            export.occurrence_id.to_string(),
            export.laudo_id.to_string(),
            export.kind.as_str(),
            export.relative_path,
            export.file_size,
            export.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_by_laudo(conn: &Connection, laudo_id: &Uuid) -> Result<Vec<Export>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM exports
         WHERE laudo_id = ?1
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([laudo_id.to_string()], row_to_export)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn list_by_occurrence(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<Export>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM exports
         WHERE occurrence_id = ?1
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_export)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_export(row: &Row<'_>) -> rusqlite::Result<Export> {
    let id = parse_uuid(row, "id")?;
    let occurrence_id = parse_uuid(row, "occurrence_id")?;
    let laudo_id = parse_uuid(row, "laudo_id")?;

    let kind_str: String = row.get("kind")?;
    let kind = ExportKind::parse(&kind_str).ok_or(rusqlite::Error::InvalidQuery)?;

    let created_at: String = row.get("created_at")?;
    let created_at = DateTime::parse_from_rfc3339(&created_at)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            Box::new(e),
        ))?
        .with_timezone(&Utc);

    Ok(Export {
        id,
        occurrence_id,
        laudo_id,
        kind,
        relative_path: row.get("relative_path")?,
        file_size: row.get("file_size")?,
        created_at,
    })
}

fn parse_uuid(row: &Row<'_>, name: &str) -> rusqlite::Result<Uuid> {
    let s: String = row.get(name)?;
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}
