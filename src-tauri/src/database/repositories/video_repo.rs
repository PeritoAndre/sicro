//! Repositories for the Video module (Spike F).
//!
//! Five tables sharing the same shape (occurrence_id + media_hash + JSON
//! blobs). Kept in a single module — splitting into five files would be
//! cargo-cult: the SQL parts are short and almost identical.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{
    VideoEvent, VideoExport, VideoMedia, VideoOperationLog, VideoStoryboardFrame,
};

// ---------------------------------------------------------------------------
// video_media

const MEDIA_COLS: &str = "
    id, occurrence_id, original_path, relative_path, filename, sha256,
    size_bytes, duration_s, codec, width, height, pixel_format,
    fps_declared, avg_frame_rate, r_frame_rate, time_base, frame_count,
    bitrate, raw_probe_json, warnings_json, created_at, updated_at
";

pub fn insert_media(conn: &Connection, m: &VideoMedia) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO video_media ({MEDIA_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)"
        ),
        params![
            m.id.to_string(),
            m.occurrence_id.to_string(),
            m.original_path,
            m.relative_path,
            m.filename,
            m.sha256,
            m.size_bytes as i64,
            m.duration_s,
            m.codec,
            m.width,
            m.height,
            m.pixel_format,
            m.fps_declared,
            m.avg_frame_rate,
            m.r_frame_rate,
            m.time_base,
            m.frame_count,
            m.bitrate,
            m.raw_probe_json,
            m.warnings_json,
            m.created_at.to_rfc3339(),
            m.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_media_for_occurrence(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<VideoMedia>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLS} FROM video_media
         WHERE occurrence_id = ?1
         ORDER BY updated_at DESC"
    ))?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], row_to_media)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn find_media_by_sha256(
    conn: &Connection,
    occurrence_id: &Uuid,
    sha256: &str,
) -> Result<Option<VideoMedia>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLS} FROM video_media
         WHERE occurrence_id = ?1 AND sha256 = ?2
         LIMIT 1"
    ))?;
    let row = stmt
        .query_row([occurrence_id.to_string(), sha256.to_string()], row_to_media)
        .optional()?;
    Ok(row)
}

pub fn find_media_by_id(conn: &Connection, id: &Uuid) -> Result<Option<VideoMedia>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLS} FROM video_media WHERE id = ?1"
    ))?;
    let row = stmt.query_row([id.to_string()], row_to_media).optional()?;
    Ok(row)
}

fn row_to_media(row: &Row<'_>) -> rusqlite::Result<VideoMedia> {
    Ok(VideoMedia {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        original_path: row.get("original_path")?,
        relative_path: row.get("relative_path")?,
        filename: row.get("filename")?,
        sha256: row.get("sha256")?,
        size_bytes: row.get::<_, i64>("size_bytes")?.max(0) as u64,
        duration_s: row.get("duration_s")?,
        codec: row.get("codec")?,
        width: row
            .get::<_, Option<i64>>("width")?
            .map(|n| n.max(0) as u32),
        height: row
            .get::<_, Option<i64>>("height")?
            .map(|n| n.max(0) as u32),
        pixel_format: row.get("pixel_format")?,
        fps_declared: row.get("fps_declared")?,
        avg_frame_rate: row.get("avg_frame_rate")?,
        r_frame_rate: row.get("r_frame_rate")?,
        time_base: row.get("time_base")?,
        frame_count: row.get("frame_count")?,
        bitrate: row.get("bitrate")?,
        raw_probe_json: row.get("raw_probe_json")?,
        warnings_json: row.get("warnings_json")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
        updated_at: parse_dt(row.get::<_, String>("updated_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// video_events

const EVENT_COLS: &str = "
    id, occurrence_id, media_hash, timestamp_s, timestamp_label,
    frame_observed, pts, time_base, category, title, description,
    reviewed, source, created_at, updated_at
";

pub fn insert_event(conn: &Connection, e: &VideoEvent) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO video_events ({EVENT_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)"
        ),
        params![
            e.id.to_string(),
            e.occurrence_id.to_string(),
            e.media_hash,
            e.timestamp_s,
            e.timestamp_label,
            e.frame_observed,
            e.pts,
            e.time_base,
            e.category,
            e.title,
            e.description,
            e.reviewed as i64,
            e.source,
            e.created_at.to_rfc3339(),
            e.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_events_for_media(
    conn: &Connection,
    occurrence_id: &Uuid,
    media_hash: &str,
) -> Result<Vec<VideoEvent>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLS} FROM video_events
         WHERE occurrence_id = ?1 AND media_hash = ?2
         ORDER BY timestamp_s ASC"
    ))?;
    let rows = stmt
        .query_map(
            [occurrence_id.to_string(), media_hash.to_string()],
            row_to_event,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn update_event(conn: &Connection, e: &VideoEvent) -> Result<()> {
    conn.execute(
        "UPDATE video_events
            SET timestamp_s    = ?2,
                timestamp_label= ?3,
                category       = ?4,
                title          = ?5,
                description    = ?6,
                reviewed       = ?7,
                updated_at     = ?8
          WHERE id = ?1",
        params![
            e.id.to_string(),
            e.timestamp_s,
            e.timestamp_label,
            e.category,
            e.title,
            e.description,
            e.reviewed as i64,
            e.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn delete_event(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute("DELETE FROM video_events WHERE id = ?1", [id.to_string()])?;
    Ok(())
}

pub fn find_event_by_id(conn: &Connection, id: &Uuid) -> Result<Option<VideoEvent>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EVENT_COLS} FROM video_events WHERE id = ?1"
    ))?;
    let row = stmt.query_row([id.to_string()], row_to_event).optional()?;
    Ok(row)
}

fn row_to_event(row: &Row<'_>) -> rusqlite::Result<VideoEvent> {
    Ok(VideoEvent {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        media_hash: row.get("media_hash")?,
        timestamp_s: row.get("timestamp_s")?,
        timestamp_label: row.get("timestamp_label")?,
        frame_observed: row.get("frame_observed")?,
        pts: row.get("pts")?,
        time_base: row.get("time_base")?,
        category: row.get("category")?,
        title: row.get("title")?,
        description: row.get("description")?,
        reviewed: row.get::<_, i64>("reviewed")? != 0,
        source: row.get("source")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
        updated_at: parse_dt(row.get::<_, String>("updated_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// video_exports

const EXPORT_COLS: &str = "
    id, occurrence_id, media_hash, event_id, type,
    requested_timestamp_s, actual_timestamp_s, delta_s,
    output_path, filename, sidecar_json_path, details_json, created_at
";

pub fn insert_export(conn: &Connection, x: &VideoExport) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO video_exports ({EXPORT_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)"
        ),
        params![
            x.id.to_string(),
            x.occurrence_id.to_string(),
            x.media_hash,
            x.event_id.map(|u| u.to_string()),
            x.r#type,
            x.requested_timestamp_s,
            x.actual_timestamp_s,
            x.delta_s,
            x.output_path,
            x.filename,
            x.sidecar_json_path,
            x.details_json,
            x.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_exports_for_media(
    conn: &Connection,
    occurrence_id: &Uuid,
    media_hash: &str,
) -> Result<Vec<VideoExport>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {EXPORT_COLS} FROM video_exports
         WHERE occurrence_id = ?1 AND media_hash = ?2
         ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map(
            [occurrence_id.to_string(), media_hash.to_string()],
            row_to_export,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn row_to_export(row: &Row<'_>) -> rusqlite::Result<VideoExport> {
    let event_id = row
        .get::<_, Option<String>>("event_id")?
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    Ok(VideoExport {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        media_hash: row.get("media_hash")?,
        event_id,
        r#type: row.get("type")?,
        requested_timestamp_s: row.get("requested_timestamp_s")?,
        actual_timestamp_s: row.get("actual_timestamp_s")?,
        delta_s: row.get("delta_s")?,
        output_path: row.get("output_path")?,
        filename: row.get("filename")?,
        sidecar_json_path: row.get("sidecar_json_path")?,
        details_json: row.get("details_json")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// video_storyboard_frames

const STORYBOARD_COLS: &str = "
    id, occurrence_id, media_hash, event_id, export_id, title, caption,
    notes, requested_timestamp_s, actual_timestamp_s, delta_s,
    observed_frame_index, estimated_total_frames, frame_index_is_estimated,
    pts, time_base, output_path, sidecar_json_path, reviewed,
    created_at, updated_at
";

pub fn insert_storyboard_frame(conn: &Connection, f: &VideoStoryboardFrame) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO video_storyboard_frames ({STORYBOARD_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)"
        ),
        params![
            f.id.to_string(),
            f.occurrence_id.to_string(),
            f.media_hash,
            f.event_id.map(|u| u.to_string()),
            f.export_id.map(|u| u.to_string()),
            f.title,
            f.caption,
            f.notes,
            f.requested_timestamp_s,
            f.actual_timestamp_s,
            f.delta_s,
            f.observed_frame_index,
            f.estimated_total_frames,
            f.frame_index_is_estimated as i64,
            f.pts,
            f.time_base,
            f.output_path,
            f.sidecar_json_path,
            f.reviewed as i64,
            f.created_at.to_rfc3339(),
            f.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_storyboard_for_media(
    conn: &Connection,
    occurrence_id: &Uuid,
    media_hash: &str,
) -> Result<Vec<VideoStoryboardFrame>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {STORYBOARD_COLS} FROM video_storyboard_frames
         WHERE occurrence_id = ?1 AND media_hash = ?2
         ORDER BY requested_timestamp_s ASC, created_at ASC"
    ))?;
    let rows = stmt
        .query_map(
            [occurrence_id.to_string(), media_hash.to_string()],
            row_to_storyboard,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn update_storyboard_frame(conn: &Connection, f: &VideoStoryboardFrame) -> Result<()> {
    conn.execute(
        "UPDATE video_storyboard_frames
            SET title      = ?2,
                caption    = ?3,
                notes      = ?4,
                reviewed   = ?5,
                updated_at = ?6
          WHERE id = ?1",
        params![
            f.id.to_string(),
            f.title,
            f.caption,
            f.notes,
            f.reviewed as i64,
            f.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn find_storyboard_by_id(
    conn: &Connection,
    id: &Uuid,
) -> Result<Option<VideoStoryboardFrame>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {STORYBOARD_COLS} FROM video_storyboard_frames WHERE id = ?1"
    ))?;
    let row = stmt
        .query_row([id.to_string()], row_to_storyboard)
        .optional()?;
    Ok(row)
}

pub fn delete_storyboard_frame(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM video_storyboard_frames WHERE id = ?1",
        [id.to_string()],
    )?;
    Ok(())
}

fn row_to_storyboard(row: &Row<'_>) -> rusqlite::Result<VideoStoryboardFrame> {
    let event_id = row
        .get::<_, Option<String>>("event_id")?
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    let export_id = row
        .get::<_, Option<String>>("export_id")?
        .as_deref()
        .and_then(|s| Uuid::parse_str(s).ok());
    Ok(VideoStoryboardFrame {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        media_hash: row.get("media_hash")?,
        event_id,
        export_id,
        title: row.get("title")?,
        caption: row.get("caption")?,
        notes: row.get("notes")?,
        requested_timestamp_s: row.get("requested_timestamp_s")?,
        actual_timestamp_s: row.get("actual_timestamp_s")?,
        delta_s: row.get("delta_s")?,
        observed_frame_index: row.get("observed_frame_index")?,
        estimated_total_frames: row.get("estimated_total_frames")?,
        frame_index_is_estimated: row.get::<_, i64>("frame_index_is_estimated")? != 0,
        pts: row.get("pts")?,
        time_base: row.get("time_base")?,
        output_path: row.get("output_path")?,
        sidecar_json_path: row.get("sidecar_json_path")?,
        reviewed: row.get::<_, i64>("reviewed")? != 0,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
        updated_at: parse_dt(row.get::<_, String>("updated_at")?)?,
    })
}

// ---------------------------------------------------------------------------
// video_operation_logs

pub fn insert_log(
    conn: &Connection,
    occurrence_id: &Uuid,
    media_hash: Option<&str>,
    action: &str,
    details_json: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO video_operation_logs
            (occurrence_id, media_hash, action, details_json, created_at)
         VALUES (?1,?2,?3,?4,?5)",
        params![
            occurrence_id.to_string(),
            media_hash,
            action,
            details_json,
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_logs_for_media(
    conn: &Connection,
    occurrence_id: &Uuid,
    media_hash: &str,
    limit: u32,
) -> Result<Vec<VideoOperationLog>> {
    let mut stmt = conn.prepare(
        "SELECT id, occurrence_id, media_hash, action, details_json, created_at
         FROM video_operation_logs
         WHERE occurrence_id = ?1 AND media_hash = ?2
         ORDER BY id DESC
         LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(
            params![occurrence_id.to_string(), media_hash, limit as i64],
            |row| {
                Ok(VideoOperationLog {
                    id: row.get("id")?,
                    occurrence_id: parse_uuid(row, "occurrence_id")?,
                    media_hash: row.get("media_hash")?,
                    action: row.get("action")?,
                    details_json: row.get("details_json")?,
                    created_at: parse_dt(row.get::<_, String>("created_at")?)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// helpers

fn parse_uuid(row: &Row<'_>, col: &str) -> rusqlite::Result<Uuid> {
    let s: String = row.get(col)?;
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
