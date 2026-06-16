//! Read/write helpers for `media_assets` (Spike D).

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{MediaAsset, MediaAssetType};

const COLUMNS: &str = "
    id, import_id, occurrence_id, original_id, type,
    relative_path, original_package_path, original_filename,
    mime_type, size_bytes, sha256, captured_at, imported_at,
    category, caption, raw_json
";

pub fn insert(conn: &Connection, asset: &MediaAsset) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO media_assets ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)"
        ),
        params![
            asset.id.to_string(),
            asset.import_id.to_string(),
            asset.occurrence_id.to_string(),
            asset.original_id,
            asset.r#type.as_str(),
            asset.relative_path,
            asset.original_package_path,
            asset.original_filename,
            asset.mime_type,
            asset.size_bytes as i64,
            asset.sha256,
            asset.captured_at.map(|d| d.to_rfc3339()),
            asset.imported_at.to_rfc3339(),
            asset.category,
            asset.caption,
            asset.raw_json,
        ],
    )?;
    Ok(())
}

pub fn list_by_occurrence(conn: &Connection, occurrence_id: &Uuid) -> Result<Vec<MediaAsset>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM media_assets
         WHERE occurrence_id = ?1
         ORDER BY captured_at ASC, imported_at ASC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_media_asset)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_media_asset(row: &Row<'_>) -> rusqlite::Result<MediaAsset> {
    let id = parse_uuid(row, "id")?;
    let import_id = parse_uuid(row, "import_id")?;
    let occurrence_id = parse_uuid(row, "occurrence_id")?;

    let type_str: String = row.get("type")?;
    let r#type = match type_str.as_str() {
        "photo" => MediaAssetType::Photo,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };

    let captured_at: Option<String> = row.get("captured_at")?;
    let captured_at = captured_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc));

    let imported_at: String = row.get("imported_at")?;
    let imported_at = DateTime::parse_from_rfc3339(&imported_at)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                12,
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?
        .with_timezone(&Utc);

    let size_bytes_i64: i64 = row.get("size_bytes")?;

    Ok(MediaAsset {
        id,
        import_id,
        occurrence_id,
        original_id: row.get("original_id")?,
        r#type,
        relative_path: row.get("relative_path")?,
        original_package_path: row.get("original_package_path")?,
        original_filename: row.get("original_filename")?,
        mime_type: row.get("mime_type")?,
        size_bytes: size_bytes_i64.max(0) as u64,
        sha256: row.get("sha256")?,
        captured_at,
        imported_at,
        category: row.get("category")?,
        caption: row.get("caption")?,
        raw_json: row.get("raw_json")?,
    })
}

fn parse_uuid(row: &Row<'_>, name: &str) -> rusqlite::Result<Uuid> {
    let s: String = row.get(name)?;
    Uuid::parse_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}
