//! Repositório do módulo Áudio (Camada 1) — espelha `video_repo`.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::models::{
    AudioEnhancement, AudioMarker, AudioMedia, AudioTranscriptSegment, TranscriptSegmentInput,
};

const MEDIA_COLS: &str = "
    id, occurrence_id, kind, original_path, original_relative_path,
    relative_path, filename, sha256, original_sha256, source_video_sha256,
    size_bytes, duration_s, sample_rate, channels, codec, bitrate,
    raw_probe_json, warnings_json, created_at, updated_at
";

pub fn insert_media(conn: &Connection, m: &AudioMedia) -> Result<()> {
    conn.execute(
        &format!(
            "INSERT INTO audio_media ({MEDIA_COLS}) VALUES \
             (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)"
        ),
        params![
            m.id.to_string(),
            m.occurrence_id.to_string(),
            m.kind,
            m.original_path,
            m.original_relative_path,
            m.relative_path,
            m.filename,
            m.sha256,
            m.original_sha256,
            m.source_video_sha256,
            m.size_bytes as i64,
            m.duration_s,
            m.sample_rate.map(|n| n as i64),
            m.channels.map(|n| n as i64),
            m.codec,
            m.bitrate,
            m.raw_probe_json,
            m.warnings_json,
            m.created_at.to_rfc3339(),
            m.updated_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_for_occurrence(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<AudioMedia>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLS} FROM audio_media
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
) -> Result<Option<AudioMedia>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLS} FROM audio_media
         WHERE occurrence_id = ?1 AND sha256 = ?2 LIMIT 1"
    ))?;
    let row = stmt
        .query_row([occurrence_id.to_string(), sha256.to_string()], row_to_media)
        .optional()?;
    Ok(row)
}

pub fn find_media_by_original_sha256(
    conn: &Connection,
    occurrence_id: &Uuid,
    original_sha256: &str,
) -> Result<Option<AudioMedia>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLS} FROM audio_media
         WHERE occurrence_id = ?1 AND original_sha256 = ?2 LIMIT 1"
    ))?;
    let row = stmt
        .query_row(
            [occurrence_id.to_string(), original_sha256.to_string()],
            row_to_media,
        )
        .optional()?;
    Ok(row)
}

pub fn find_media_by_id(conn: &Connection, id: &Uuid) -> Result<Option<AudioMedia>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLS} FROM audio_media WHERE id = ?1"
    ))?;
    let row = stmt.query_row([id.to_string()], row_to_media).optional()?;
    Ok(row)
}

pub fn insert_log(
    conn: &Connection,
    occurrence_id: &Uuid,
    media_hash: Option<&str>,
    action: &str,
    details_json: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO audio_operation_logs
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

// ---------------------------------------------------------------------------
// audio_markers

const MARKER_COLS: &str = "id, occurrence_id, audio_sha256, t_seconds, label, created_at";

pub fn insert_marker(conn: &Connection, m: &AudioMarker) -> Result<()> {
    conn.execute(
        &format!("INSERT INTO audio_markers ({MARKER_COLS}) VALUES (?1,?2,?3,?4,?5,?6)"),
        params![
            m.id.to_string(),
            m.occurrence_id.to_string(),
            m.audio_sha256,
            m.t_seconds,
            m.label,
            m.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_markers_for_audio(
    conn: &Connection,
    occurrence_id: &Uuid,
    audio_sha256: &str,
) -> Result<Vec<AudioMarker>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MARKER_COLS} FROM audio_markers
         WHERE occurrence_id = ?1 AND audio_sha256 = ?2
         ORDER BY t_seconds ASC"
    ))?;
    let rows = stmt
        .query_map(params![occurrence_id.to_string(), audio_sha256], row_to_marker)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_marker(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute("DELETE FROM audio_markers WHERE id = ?1", [id.to_string()])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// audio_enhancements

pub fn insert_enhancement(conn: &Connection, e: &AudioEnhancement) -> Result<()> {
    conn.execute(
        "INSERT INTO audio_enhancements
            (id, occurrence_id, source_audio_sha256, output_audio_sha256, filters_json, created_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            e.id.to_string(),
            e.occurrence_id.to_string(),
            e.source_audio_sha256,
            e.output_audio_sha256,
            e.filters_json,
            e.created_at.to_rfc3339(),
        ],
    )?;
    Ok(())
}

pub fn list_enhancements_for_occurrence(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<AudioEnhancement>> {
    let mut stmt = conn.prepare(
        "SELECT id, occurrence_id, source_audio_sha256, output_audio_sha256, filters_json, created_at
         FROM audio_enhancements WHERE occurrence_id = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt
        .query_map([occurrence_id.to_string()], |row| {
            Ok(AudioEnhancement {
                id: parse_uuid(row, "id")?,
                occurrence_id: parse_uuid(row, "occurrence_id")?,
                source_audio_sha256: row.get("source_audio_sha256")?,
                output_audio_sha256: row.get("output_audio_sha256")?,
                filters_json: row.get("filters_json")?,
                created_at: parse_dt(row.get::<_, String>("created_at")?)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// audio_transcript_segments (degravação manual — replace-all por áudio)

const SEGMENT_COLS: &str =
    "id, occurrence_id, audio_sha256, idx, t_start, t_end, speaker, text, created_at";

pub fn list_segments(
    conn: &Connection,
    occurrence_id: &Uuid,
    audio_sha256: &str,
) -> Result<Vec<AudioTranscriptSegment>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SEGMENT_COLS} FROM audio_transcript_segments
         WHERE occurrence_id = ?1 AND audio_sha256 = ?2
         ORDER BY idx ASC"
    ))?;
    let rows = stmt
        .query_map(params![occurrence_id.to_string(), audio_sha256], |row| {
            Ok(AudioTranscriptSegment {
                id: parse_uuid(row, "id")?,
                occurrence_id: parse_uuid(row, "occurrence_id")?,
                audio_sha256: row.get("audio_sha256")?,
                idx: row.get("idx")?,
                t_start: row.get("t_start")?,
                t_end: row.get("t_end")?,
                speaker: row.get("speaker")?,
                text: row.get("text")?,
                created_at: parse_dt(row.get::<_, String>("created_at")?)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Substitui TODA a degravação de um áudio pelos segmentos informados
/// (transação: apaga os existentes e insere os novos). Ids são gerados aqui.
pub fn replace_segments(
    conn: &mut Connection,
    occurrence_id: &Uuid,
    audio_sha256: &str,
    inputs: &[TranscriptSegmentInput],
) -> Result<()> {
    let tx = conn.transaction()?;
    tx.execute(
        "DELETE FROM audio_transcript_segments WHERE occurrence_id = ?1 AND audio_sha256 = ?2",
        params![occurrence_id.to_string(), audio_sha256],
    )?;
    let now = Utc::now().to_rfc3339();
    for s in inputs {
        tx.execute(
            &format!(
                "INSERT INTO audio_transcript_segments ({SEGMENT_COLS}) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)"
            ),
            params![
                Uuid::new_v4().to_string(),
                occurrence_id.to_string(),
                audio_sha256,
                s.idx,
                s.t_start,
                s.t_end,
                s.speaker,
                s.text,
                now,
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn row_to_marker(row: &Row<'_>) -> rusqlite::Result<AudioMarker> {
    Ok(AudioMarker {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        audio_sha256: row.get("audio_sha256")?,
        t_seconds: row.get("t_seconds")?,
        label: row.get("label")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
    })
}

fn row_to_media(row: &Row<'_>) -> rusqlite::Result<AudioMedia> {
    Ok(AudioMedia {
        id: parse_uuid(row, "id")?,
        occurrence_id: parse_uuid(row, "occurrence_id")?,
        kind: row.get("kind")?,
        original_path: row.get("original_path")?,
        original_relative_path: row.get("original_relative_path")?,
        relative_path: row.get("relative_path")?,
        filename: row.get("filename")?,
        sha256: row.get("sha256")?,
        original_sha256: row.get("original_sha256")?,
        source_video_sha256: row.get("source_video_sha256")?,
        size_bytes: row.get::<_, i64>("size_bytes")?.max(0) as u64,
        duration_s: row.get("duration_s")?,
        sample_rate: row
            .get::<_, Option<i64>>("sample_rate")?
            .map(|n| n.max(0) as u32),
        channels: row
            .get::<_, Option<i64>>("channels")?
            .map(|n| n.max(0) as u32),
        codec: row.get("codec")?,
        bitrate: row.get("bitrate")?,
        raw_probe_json: row.get("raw_probe_json")?,
        warnings_json: row.get("warnings_json")?,
        created_at: parse_dt(row.get::<_, String>("created_at")?)?,
        updated_at: parse_dt(row.get::<_, String>("updated_at")?)?,
    })
}

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
