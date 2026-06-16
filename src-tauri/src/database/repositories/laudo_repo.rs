//! Read/write helpers for the `laudos` table.
//!
//! The `.sicrodoc` file itself is NOT handled here — see
//! `commands/laudo_commands.rs`. This repository only owns the index row.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{Laudo, LaudoStatus};

const COLUMNS: &str = "
    id, occurrence_id, title, template_id, relative_path, status,
    created_at, updated_at, last_export_pdf, last_export_docx
";

pub fn insert(conn: &Connection, laudo: &Laudo) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO laudos ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ),
        params![
            laudo.id.to_string(),
            laudo.occurrence_id.to_string(),
            laudo.title,
            laudo.template_id,
            laudo.relative_path,
            laudo.status.as_str(),
            laudo.created_at.to_rfc3339(),
            laudo.updated_at.to_rfc3339(),
            laudo.last_export_pdf.map(|d| d.to_rfc3339()),
            laudo.last_export_docx.map(|d| d.to_rfc3339()),
        ],
    )?;
    Ok(())
}

pub fn list_by_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<Laudo>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM laudos
         WHERE occurrence_id = ?1
         ORDER BY updated_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_laudo)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn find_by_id(conn: &Connection, id: &Uuid) -> Result<Option<Laudo>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM laudos WHERE id = ?1"
    ))?;
    let row = stmt
        .query_row([id.to_string()], row_to_laudo)
        .optional()?;
    Ok(row)
}

pub fn touch_updated_at(conn: &Connection, id: &Uuid, when: DateTime<Utc>) -> Result<()> {
    conn.execute(
        "UPDATE laudos SET updated_at = ?2 WHERE id = ?1",
        params![id.to_string(), when.to_rfc3339()],
    )?;
    Ok(())
}

/// Remove a linha do laudo da tabela. NÃO mexe no `.sicrodoc` em disco —
/// quem orquestra a exclusão (apagar arquivo + auditoria) é o command em
/// `commands/laudo_commands.rs`.
pub fn delete(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM laudos WHERE id = ?1",
        [id.to_string()],
    )?;
    Ok(())
}

fn row_to_laudo(row: &Row<'_>) -> rusqlite::Result<Laudo> {
    let id_str: String = row.get("id")?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
    let occurrence_id_str: String = row.get("occurrence_id")?;
    let occurrence_id = Uuid::parse_str(&occurrence_id_str)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;

    let status_str: String = row.get("status")?;
    let status = LaudoStatus::parse(&status_str).unwrap_or_default();

    Ok(Laudo {
        id,
        occurrence_id,
        title: row.get("title")?,
        template_id: row.get("template_id")?,
        relative_path: row.get("relative_path")?,
        status,
        created_at: parse_dt(row.get::<_, String>("created_at")?)
            .ok_or(rusqlite::Error::InvalidQuery)?,
        updated_at: parse_dt(row.get::<_, String>("updated_at")?)
            .ok_or(rusqlite::Error::InvalidQuery)?,
        last_export_pdf: parse_optional_dt(row.get::<_, Option<String>>("last_export_pdf")?),
        last_export_docx: parse_optional_dt(row.get::<_, Option<String>>("last_export_docx")?),
        // H — populated by `list_laudos` command after DB read (lê
        // `.sicrodoc` para extrair `finalization.signature.type`).
        signature_type: None,
    })
}

fn parse_dt(s: String) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn parse_optional_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.and_then(parse_dt)
}
