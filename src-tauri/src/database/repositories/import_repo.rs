//! Read/write helpers for the `imports` table (Spike D — .sicroapp importer).
//!
//! Schema lives in `migrations/004_imports.sql`. This module is the only place
//! that touches raw SQL for that table — `commands/import_commands.rs` calls
//! these functions instead of constructing statements itself.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{Import, ImportStatus};

const COLUMNS: &str = "
    id, package_relative_path, original_filename, package_sha256,
    format, schema_version, app_name, app_version,
    mobile_occurrence_id, status,
    warnings_json, errors_json, raw_manifest_json, imported_at
";

pub fn insert(conn: &Connection, import: &Import) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO imports ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
        ),
        params![
            import.id.to_string(),
            import.package_relative_path,
            import.original_filename,
            import.package_sha256,
            import.format,
            import.schema_version,
            import.app_name,
            import.app_version,
            import.mobile_occurrence_id,
            import.status.as_str(),
            import.warnings_json,
            import.errors_json,
            import.raw_manifest_json,
            import.imported_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

/// Return the existing import for a given package hash, if any. Used to
/// implement the "block reimport" rule from doc §8.
pub fn find_by_package_sha256(conn: &Connection, sha256: &str) -> Result<Option<Import>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM imports WHERE package_sha256 = ?1"
    ))?;
    let row = stmt.query_row([sha256], row_to_import).optional()?;
    Ok(row)
}

/// Patch the row in place — used by the orchestrator after the occurrence and
/// media rows are written, so the import is born with empty warnings/errors
/// and finalised once the side-effects are complete.
pub fn update_status_and_warnings(
    conn: &Connection,
    id: &Uuid,
    status: ImportStatus,
    warnings_json: &str,
    errors_json: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE imports
            SET status = ?2,
                warnings_json = ?3,
                errors_json = ?4
          WHERE id = ?1",
        params![id.to_string(), status.as_str(), warnings_json, errors_json],
    )?;
    Ok(())
}

pub fn list_all(conn: &Connection) -> Result<Vec<Import>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM imports ORDER BY imported_at DESC"
    ))?;
    let rows = stmt
        .query_map([], row_to_import)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_import(row: &Row<'_>) -> rusqlite::Result<Import> {
    let id_str: String = row.get("id")?;
    let id = Uuid::parse_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let status_str: String = row.get("status")?;
    let status = ImportStatus::parse(&status_str).ok_or(rusqlite::Error::InvalidQuery)?;

    let imported_at: String = row.get("imported_at")?;
    let imported_at = DateTime::parse_from_rfc3339(&imported_at)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                13,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?
        .with_timezone(&Utc);

    Ok(Import {
        id,
        package_relative_path: row.get("package_relative_path")?,
        original_filename: row.get("original_filename")?,
        package_sha256: row.get("package_sha256")?,
        format: row.get("format")?,
        schema_version: row.get("schema_version")?,
        app_name: row.get("app_name")?,
        app_version: row.get("app_version")?,
        mobile_occurrence_id: row.get("mobile_occurrence_id")?,
        status,
        warnings_json: row.get("warnings_json")?,
        errors_json: row.get("errors_json")?,
        raw_manifest_json: row.get("raw_manifest_json")?,
        imported_at,
    })
}
