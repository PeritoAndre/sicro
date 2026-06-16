//! Read/write helpers for `evidence_items` (Spike D).

use rusqlite::{params, Connection};

use crate::error::Result;
use crate::models::EvidenceItem;

const COLUMNS: &str = "
    id, occurrence_id, media_asset_id, type, title, description,
    source_module, captured_at, metadata_json, created_at
";

pub fn insert(conn: &Connection, item: &EvidenceItem) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO evidence_items ({COLUMNS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ),
        params![
            item.id.to_string(),
            item.occurrence_id.to_string(),
            item.media_asset_id.map(|u| u.to_string()),
            item.r#type,
            item.title,
            item.description,
            item.source_module,
            item.captured_at.map(|d| d.to_rfc3339()),
            item.metadata_json,
            item.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}
