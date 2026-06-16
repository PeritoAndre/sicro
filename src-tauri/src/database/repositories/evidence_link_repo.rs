//! Repository for `evidence_links` (MVP 4 — Evidência → Laudo).
//!
//! One row per "evidence X foi inserida em laudo Y". Allows the UI to list
//! `links(laudo)` without parsing the `.sicrodoc`.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{EvidenceLink, EvidenceSourceKind};

const COLUMNS: &str = "
    id, occurrence_id, target_type, target_id, relation_type, source_kind,
    media_asset_id, croqui_id, video_media_hash, video_event_id,
    video_storyboard_frame_id, field_note_id,
    relative_path, source_hash, metadata_json, created_at
";

pub fn insert(conn: &Connection, e: &EvidenceLink) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO evidence_links ({COLUMNS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)"
        ),
        params![
            e.id.to_string(),
            e.occurrence_id.to_string(),
            e.target_type,
            e.target_id,
            e.relation_type,
            e.source_kind.as_str(),
            e.media_asset_id.map(|u| u.to_string()),
            e.croqui_id.map(|u| u.to_string()),
            e.video_media_hash,
            e.video_event_id.map(|u| u.to_string()),
            e.video_storyboard_frame_id.map(|u| u.to_string()),
            e.field_note_id.map(|u| u.to_string()),
            e.relative_path,
            e.source_hash,
            e.metadata_json,
            e.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_for_target(
    conn: &Connection,
    target_type: &str,
    target_id: &str,
) -> Result<Vec<EvidenceLink>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM evidence_links
         WHERE target_type = ?1 AND target_id = ?2
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([target_type, target_id], row_to_link)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// List every evidence link of a given occurrence — used by the
/// Central de Evidências to count how many laudos cite each item.
pub fn list_for_occurrence(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<EvidenceLink>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM evidence_links
         WHERE occurrence_id = ?1
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_link)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_link(row: &Row<'_>) -> rusqlite::Result<EvidenceLink> {
    let id: String = row.get("id")?;
    let id = Uuid::parse_str(&id).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let occurrence_id: String = row.get("occurrence_id")?;
    let occurrence_id = Uuid::parse_str(&occurrence_id).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let source_kind_raw: String = row.get("source_kind")?;
    let source_kind = parse_source_kind(&source_kind_raw)
        .ok_or(rusqlite::Error::InvalidQuery)?;

    Ok(EvidenceLink {
        id,
        occurrence_id,
        target_type: row.get("target_type")?,
        target_id: row.get("target_id")?,
        relation_type: row.get("relation_type")?,
        source_kind,
        media_asset_id: parse_optional_uuid(row, "media_asset_id")?,
        croqui_id: parse_optional_uuid(row, "croqui_id")?,
        video_media_hash: row.get("video_media_hash")?,
        video_event_id: parse_optional_uuid(row, "video_event_id")?,
        video_storyboard_frame_id: parse_optional_uuid(row, "video_storyboard_frame_id")?,
        field_note_id: parse_optional_uuid(row, "field_note_id")?,
        relative_path: row.get("relative_path")?,
        source_hash: row.get("source_hash")?,
        metadata_json: row.get("metadata_json")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

fn parse_source_kind(s: &str) -> Option<EvidenceSourceKind> {
    match s {
        "photo" => Some(EvidenceSourceKind::Photo),
        "croqui" => Some(EvidenceSourceKind::Croqui),
        "video_frame" => Some(EvidenceSourceKind::VideoFrame),
        "video_storyboard" => Some(EvidenceSourceKind::VideoStoryboard),
        "occurrence_field" => Some(EvidenceSourceKind::OccurrenceField),
        "checklist_table" => Some(EvidenceSourceKind::ChecklistTable),
        "traces_table" => Some(EvidenceSourceKind::TracesTable),
        "measurements_table" => Some(EvidenceSourceKind::MeasurementsTable),
        "field_note" => Some(EvidenceSourceKind::FieldNote),
        _ => None,
    }
}

fn parse_optional_uuid(row: &Row<'_>, col: &str) -> rusqlite::Result<Option<Uuid>> {
    let s: Option<String> = row.get(col)?;
    Ok(s.as_deref().and_then(|s| Uuid::parse_str(s).ok()))
}

fn parse_dt(s: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}
