//! Tauri commands for the Video module (Spike F).
//!
//! Surface required by the frontend:
//!   - register_video_media    → copia o vídeo, calcula SHA-256, roda ffprobe
//!   - list_video_media        → lista todas as mídias de uma ocorrência
//!   - open_video_media        → VideoBundle agregado para hidratar a UI
//!   - create/update/delete_video_event
//!   - collect_video_frame     → roda ffmpeg, grava PNG + sidecar JSON,
//!                                cria video_export + video_storyboard_frame
//!   - update_storyboard_frame
//!   - delete_storyboard_frame
//!   - list_video_operation_logs
//!   - list/set/delete_video_clock → relógio da câmera (migration 018)
//!   - video_thumbnail             → miniatura (cache do app, não o `.sicro`)
//!   - export_video_clip           → trecho como CÓPIA registrada (migration 019)

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{occurrence_repo, video_repo};
use crate::error::{Result, SicroError};
use crate::hashing::sha256::sha256_file;
use crate::models::{
    CollectFrameInput, CollectFrameResult, CreateVideoEventInput, ExportClipInput,
    ExportClipResult, RegisterVideoInput,
    SetVideoClockInput, UpdateStoryboardFrameInput, UpdateVideoEventInput, VideoBundle,
    VideoClockCalibration, VideoEvent, VideoExport, VideoMedia, VideoOperationLog,
    VideoStoryboardFrame,
};
use crate::video::{extract_frame, probe_media, ExtractFrameOptions};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};
use crate::workspace::open_workspace;

const VIDEOS_SUBDIR: &str = "videos/originais";
const FRAMES_SUBDIR: &str = "videos/storyboards/frames";
const CLIPS_SUBDIR: &str = "videos/trechos";

#[tauri::command]
pub async fn register_video_media(
    workspace_path: String,
    input: RegisterVideoInput,
) -> Result<VideoMedia> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;

    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let source = PathBuf::from(&input.source_path);
    if !source.is_file() {
        return Err(SicroError::Filesystem(format!(
            "video file not found at {}",
            source.display()
        )));
    }

    // 1. Hash the source BEFORE copying — confirms identity even if copy
    //    is interrupted later.
    let sha256 = sha256_file(&source)?;

    // 2. Duplicate check inside this workspace.
    if let Some(existing) =
        video_repo::find_media_by_sha256(&conn, &occurrence_id, &sha256)?
    {
        return Err(SicroError::Validation(format!(
            "video já registrado nesta ocorrência (id {}). Abra a mídia existente em vez de duplicar.",
            existing.id
        )));
    }

    // 3. Copy under videos/originais/, picking a non-colliding filename.
    let videos_dir = ws.join(VIDEOS_SUBDIR);
    std::fs::create_dir_all(&videos_dir).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create {}: {}",
            videos_dir.display(),
            e
        ))
    })?;
    let target_name = pick_unique_filename(&videos_dir, &source);
    let target_path = videos_dir.join(&target_name);
    std::fs::copy(&source, &target_path).map_err(|e| {
        SicroError::Filesystem(format!(
            "could not copy video to {}: {}",
            target_path.display(),
            e
        ))
    })?;

    let size_bytes = std::fs::metadata(&target_path).map(|m| m.len()).unwrap_or(0);

    // 4. Probe metadata via ffprobe (best-effort: failure is recorded as
    //    warning rather than aborting the registration).
    let mut warnings: Vec<String> = Vec::new();
    let mut raw_probe = "{}".to_string();
    let mut duration_s = None;
    let mut codec = None;
    let mut width = None;
    let mut height = None;
    let mut pixel_format = None;
    let mut fps_declared = None;
    let mut avg_frame_rate = None;
    let mut r_frame_rate = None;
    let mut time_base = None;
    let mut frame_count = None;
    let mut bitrate = None;
    match probe_media(&target_path) {
        Ok(p) => {
            raw_probe = p.raw_json;
            duration_s = p.duration_s;
            codec = p.video_codec;
            width = p.width;
            height = p.height;
            pixel_format = p.pixel_format;
            fps_declared = p.fps_declared;
            avg_frame_rate = p.avg_frame_rate;
            r_frame_rate = p.r_frame_rate;
            time_base = p.time_base;
            frame_count = p.frame_count;
            bitrate = p.bitrate;
            warnings.extend(p.warnings);
        }
        Err(e) => {
            warnings.push(format!(
                "ffprobe indisponível ou falhou: {e}. Metadados técnicos NÃO foram obtidos."
            ));
        }
    }

    // 5. Persist.
    let id = Uuid::new_v4();
    let now = Utc::now();
    let relative_path = format!("{VIDEOS_SUBDIR}/{}", target_name);
    let warnings_json =
        serde_json::to_string(&warnings).unwrap_or_else(|_| "[]".to_string());
    let media = VideoMedia {
        id,
        occurrence_id,
        original_path: source.to_str().map(str::to_string),
        relative_path,
        filename: target_name.clone(),
        sha256,
        size_bytes,
        duration_s,
        codec,
        width,
        height,
        pixel_format,
        fps_declared,
        avg_frame_rate,
        r_frame_rate,
        time_base,
        frame_count,
        bitrate,
        raw_probe_json: raw_probe,
        warnings_json,
        created_at: now,
        updated_at: now,
        derived_from_hash: None,
        derivation_json: None,
    };
    video_repo::insert_media(&conn, &media)?;
    video_repo::insert_log(
        &conn,
        &occurrence_id,
        Some(&media.sha256),
        "media.register",
        &json!({
            "media_id": media.id.to_string(),
            "filename": media.filename,
            "size_bytes": media.size_bytes,
            "warnings": warnings,
        })
        .to_string(),
    )?;
    occurrence_repo::record_audit(
        &conn,
        Some(&occurrence_id),
        "video.media.registered",
        Some("video"),
        Some("video_media"),
        Some(&media.id),
        Some(&media.sha256),
    )?;

    Ok(media)
}

#[tauri::command]
pub async fn list_video_media(workspace_path: String) -> Result<Vec<VideoMedia>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    video_repo::list_media_for_occurrence(&conn, &manifest.occurrence_id)
}

#[tauri::command]
pub async fn open_video_media(
    workspace_path: String,
    media_id: String,
) -> Result<VideoBundle> {
    let ws = PathBuf::from(&workspace_path);
    let _ = open_workspace(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;

    let id = Uuid::parse_str(&media_id)
        .map_err(|e| SicroError::Validation(format!("invalid media_id: {e}")))?;
    let media = video_repo::find_media_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("video media {} not found", id)))?;

    let events =
        video_repo::list_events_for_media(&conn, &media.occurrence_id, &media.sha256)?;
    let exports =
        video_repo::list_exports_for_media(&conn, &media.occurrence_id, &media.sha256)?;
    let storyboard =
        video_repo::list_storyboard_for_media(&conn, &media.occurrence_id, &media.sha256)?;

    Ok(VideoBundle {
        media,
        events,
        exports,
        storyboard,
    })
}

#[tauri::command]
pub async fn create_video_event(
    workspace_path: String,
    input: CreateVideoEventInput,
) -> Result<VideoEvent> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let now = Utc::now();
    let event = VideoEvent {
        id: Uuid::new_v4(),
        occurrence_id: manifest.occurrence_id,
        media_hash: input.media_hash.clone(),
        timestamp_s: input.timestamp_s.max(0.0),
        timestamp_label: format_seconds(input.timestamp_s),
        frame_observed: None,
        pts: None,
        time_base: None,
        category: validate_category(&input.category)?,
        title: if input.title.trim().is_empty() {
            "Evento sem título".to_string()
        } else {
            input.title.trim().to_string()
        },
        description: input.description.trim().to_string(),
        reviewed: false,
        source: "manual".to_string(),
        created_at: now,
        updated_at: now,
    };
    video_repo::insert_event(&conn, &event)?;
    video_repo::insert_log(
        &conn,
        &event.occurrence_id,
        Some(&event.media_hash),
        "event.create",
        &json!({
            "event_id": event.id.to_string(),
            "category": event.category,
            "timestamp_s": event.timestamp_s,
        })
        .to_string(),
    )?;
    Ok(event)
}

#[tauri::command]
pub async fn update_video_event(
    workspace_path: String,
    event_id: String,
    input: UpdateVideoEventInput,
) -> Result<VideoEvent> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let id = Uuid::parse_str(&event_id)
        .map_err(|e| SicroError::Validation(format!("invalid event_id: {e}")))?;
    let mut event = video_repo::find_event_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("event {} not found", id)))?;

    if let Some(t) = input.title {
        event.title = t.trim().to_string();
    }
    if let Some(d) = input.description {
        event.description = d.trim().to_string();
    }
    if let Some(c) = input.category {
        event.category = validate_category(&c)?;
    }
    if let Some(ts) = input.timestamp_s {
        event.timestamp_s = ts.max(0.0);
        event.timestamp_label = format_seconds(event.timestamp_s);
    }
    if let Some(r) = input.reviewed {
        event.reviewed = r;
    }
    event.updated_at = Utc::now();

    video_repo::update_event(&conn, &event)?;
    video_repo::insert_log(
        &conn,
        &event.occurrence_id,
        Some(&event.media_hash),
        "event.update",
        &json!({
            "event_id": event.id.to_string(),
            "category": event.category,
            "timestamp_s": event.timestamp_s,
            "reviewed": event.reviewed,
        })
        .to_string(),
    )?;
    Ok(event)
}

#[tauri::command]
pub async fn delete_video_event(
    workspace_path: String,
    event_id: String,
) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let id = Uuid::parse_str(&event_id)
        .map_err(|e| SicroError::Validation(format!("invalid event_id: {e}")))?;
    let event = video_repo::find_event_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("event {} not found", id)))?;
    video_repo::delete_event(&conn, &id)?;
    video_repo::insert_log(
        &conn,
        &event.occurrence_id,
        Some(&event.media_hash),
        "event.delete",
        &json!({ "event_id": event.id.to_string() }).to_string(),
    )?;
    Ok(())
}

#[tauri::command]
pub async fn collect_video_frame(
    workspace_path: String,
    input: CollectFrameInput,
) -> Result<CollectFrameResult> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    // Find the media row by hash within this occurrence.
    let media = video_repo::find_media_by_sha256(&conn, &occurrence_id, &input.media_hash)?
        .ok_or_else(|| {
            SicroError::Validation(format!(
                "mídia com sha256 {} não encontrada nesta ocorrência",
                input.media_hash
            ))
        })?;
    let video_abs = ws.join(&media.relative_path);
    if !video_abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "arquivo de vídeo ausente em {}",
            video_abs.display()
        )));
    }

    // Build the target paths under videos/storyboards/frames/.
    let frames_dir = ws.join(FRAMES_SUBDIR);
    std::fs::create_dir_all(&frames_dir).ok();
    let ts_for_name = ((input.timestamp_s * 1000.0).round() as i64).max(0);
    let stamp = Utc::now().format("%Y%m%d_%H%M%S");
    let png_name = format!("frame_{stamp}_{ts_for_name}ms.png");
    let png_target = frames_dir.join(&png_name);
    let sidecar_name = format!("frame_{stamp}_{ts_for_name}ms.json");
    let sidecar_target = frames_dir.join(&sidecar_name);

    // Sidecar extra context — domain data the video module knows about.
    let sidecar_extra = json!({
        "media_id": media.id.to_string(),
        "media_sha256": media.sha256,
        "media_filename": media.filename,
        "event_id": input.event_id.map(|u| u.to_string()),
        "fps_declared": media.fps_declared,
        "frame_count": media.frame_count,
        "avg_frame_rate": media.avg_frame_rate,
        "r_frame_rate": media.r_frame_rate,
        "time_base": media.time_base,
        "frame_index_is_estimated": true,
        "estimated_frame_index": estimate_frame_index(input.timestamp_s, media.fps_declared),
    });

    let extracted = extract_frame(ExtractFrameOptions {
        video_path: &video_abs,
        timestamp_s: input.timestamp_s,
        out_png: &png_target,
        sidecar_json: Some(&sidecar_target),
        sidecar_extra,
        start_time_s: crate::video::probe::container_start_time(&media.raw_probe_json),
    })?;

    // Persist video_exports + video_storyboard_frames.
    let now = Utc::now();
    let export = VideoExport {
        id: Uuid::new_v4(),
        occurrence_id,
        media_hash: media.sha256.clone(),
        event_id: input.event_id,
        r#type: "frame_png".to_string(),
        requested_timestamp_s: extracted.requested_timestamp_s,
        actual_timestamp_s: extracted.actual_timestamp_s,
        delta_s: extracted.delta_s,
        output_path: format!("{FRAMES_SUBDIR}/{png_name}"),
        filename: png_name.clone(),
        sidecar_json_path: Some(format!("{FRAMES_SUBDIR}/{sidecar_name}")),
        details_json: serde_json::to_string(&json!({
            "size_bytes": extracted.size_bytes,
            "ffmpeg_version": extracted.ffmpeg_version,
            "extracted_at": extracted.extracted_at.to_rfc3339(),
        }))
        .unwrap_or_else(|_| "{}".into()),
        created_at: now,
    };
    video_repo::insert_export(&conn, &export)?;

    let frame_idx_est = estimate_frame_index(input.timestamp_s, media.fps_declared);
    let storyboard_frame = VideoStoryboardFrame {
        id: Uuid::new_v4(),
        occurrence_id,
        media_hash: media.sha256.clone(),
        event_id: input.event_id,
        export_id: Some(export.id),
        title: input
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("Frame coletado")
            .to_string(),
        caption: input.caption.unwrap_or_default(),
        notes: input.notes.unwrap_or_default(),
        requested_timestamp_s: extracted.requested_timestamp_s,
        actual_timestamp_s: extracted.actual_timestamp_s,
        delta_s: extracted.delta_s,
        observed_frame_index: frame_idx_est,
        estimated_total_frames: media.frame_count,
        frame_index_is_estimated: true,
        pts: None,
        time_base: media.time_base.clone(),
        output_path: export.output_path.clone(),
        sidecar_json_path: export.sidecar_json_path.clone(),
        reviewed: false,
        created_at: now,
        updated_at: now,
    };
    video_repo::insert_storyboard_frame(&conn, &storyboard_frame)?;

    video_repo::insert_log(
        &conn,
        &occurrence_id,
        Some(&media.sha256),
        "frame.collect",
        &json!({
            "export_id": export.id.to_string(),
            "storyboard_frame_id": storyboard_frame.id.to_string(),
            "requested_timestamp_s": export.requested_timestamp_s,
            "actual_timestamp_s": export.actual_timestamp_s,
            "delta_s": export.delta_s,
        })
        .to_string(),
    )?;

    let mut warnings = Vec::new();
    if let Some(delta) = extracted.delta_s {
        if delta.abs() > 0.5 {
            warnings.push(format!(
                "delta de {:.3}s entre timestamp solicitado e frame entregue (ffmpeg pode ter snapped no keyframe)",
                delta
            ));
        }
    }

    Ok(CollectFrameResult {
        export,
        storyboard_frame,
        warnings,
    })
}

#[tauri::command]
pub async fn update_storyboard_frame(
    workspace_path: String,
    frame_id: String,
    input: UpdateStoryboardFrameInput,
) -> Result<VideoStoryboardFrame> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let id = Uuid::parse_str(&frame_id)
        .map_err(|e| SicroError::Validation(format!("invalid frame_id: {e}")))?;
    let mut frame = video_repo::find_storyboard_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("storyboard frame {} not found", id)))?;
    if let Some(t) = input.title {
        frame.title = t.trim().to_string();
    }
    if let Some(c) = input.caption {
        frame.caption = c;
    }
    if let Some(n) = input.notes {
        frame.notes = n;
    }
    if let Some(r) = input.reviewed {
        frame.reviewed = r;
    }
    frame.updated_at = Utc::now();
    video_repo::update_storyboard_frame(&conn, &frame)?;
    Ok(frame)
}

#[tauri::command]
pub async fn delete_storyboard_frame(
    workspace_path: String,
    frame_id: String,
    delete_png: Option<bool>,
) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let id = Uuid::parse_str(&frame_id)
        .map_err(|e| SicroError::Validation(format!("invalid frame_id: {e}")))?;
    let frame = video_repo::find_storyboard_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("storyboard frame {} not found", id)))?;

    if delete_png.unwrap_or(false) {
        let png_abs = ws.join(&frame.output_path);
        let _ = std::fs::remove_file(&png_abs);
        if let Some(sidecar) = &frame.sidecar_json_path {
            let _ = std::fs::remove_file(ws.join(sidecar));
        }
    }

    video_repo::delete_storyboard_frame(&conn, &id)?;
    video_repo::insert_log(
        &conn,
        &frame.occurrence_id,
        Some(&frame.media_hash),
        "storyboard_frame.delete",
        &json!({
            "storyboard_frame_id": frame.id.to_string(),
            "deleted_png": delete_png.unwrap_or(false),
        })
        .to_string(),
    )?;
    Ok(())
}

#[tauri::command]
pub async fn list_video_operation_logs(
    workspace_path: String,
    media_hash: String,
    limit: Option<u32>,
) -> Result<Vec<VideoOperationLog>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    video_repo::list_logs_for_media(
        &conn,
        &manifest.occurrence_id,
        &media_hash,
        limit.unwrap_or(100),
    )
}

// ---------------------------------------------------------------------------
// Relógio da câmera

/// Todos os vínculos de relógio da ocorrência (o player usa o do vídeo aberto;
/// a comparação de câmeras usa os dois para sincronizar pelo horário).
#[tauri::command]
pub async fn list_video_clocks(workspace_path: String) -> Result<Vec<VideoClockCalibration>> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    video_repo::list_clocks_for_occurrence(&conn, &manifest.occurrence_id)
}

#[tauri::command]
pub async fn set_video_clock(
    workspace_path: String,
    input: SetVideoClockInput,
) -> Result<VideoClockCalibration> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    if !input.media_time_s.is_finite() || input.media_time_s < 0.0 {
        return Err(SicroError::Validation("instante do vídeo inválido".into()));
    }
    if !input.clock_seconds.is_finite() || !(0.0..86_400.0).contains(&input.clock_seconds) {
        return Err(SicroError::Validation("horário da câmera inválido".into()));
    }
    let clock_date = match input.clock_date.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(d) => Some(
            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                .map_err(|_| SicroError::Validation(format!("data da câmera inválida: {d}")))?
                .format("%Y-%m-%d")
                .to_string(),
        ),
    };
    video_repo::find_media_by_sha256(&conn, &manifest.occurrence_id, &input.media_hash)?
        .ok_or_else(|| SicroError::Validation("vídeo não encontrado nesta ocorrência".into()))?;

    let now = Utc::now();
    let previous = video_repo::find_clock(&conn, &manifest.occurrence_id, &input.media_hash)?;
    let clock = VideoClockCalibration {
        id: previous.as_ref().map(|p| p.id).unwrap_or_else(Uuid::new_v4),
        occurrence_id: manifest.occurrence_id,
        media_hash: input.media_hash.clone(),
        media_time_s: input.media_time_s,
        clock_seconds: input.clock_seconds,
        clock_date,
        clock_label: input.clock_label.trim().to_string(),
        note: input.note.trim().to_string(),
        created_at: previous.as_ref().map(|p| p.created_at).unwrap_or(now),
        updated_at: now,
    };
    video_repo::upsert_clock(&conn, &clock)?;
    video_repo::insert_log(
        &conn,
        &clock.occurrence_id,
        Some(&clock.media_hash),
        "clock.set",
        &json!({
            "media_time_s": clock.media_time_s,
            "clock_seconds": clock.clock_seconds,
            "clock_date": clock.clock_date,
            "clock_label": clock.clock_label,
            "replaced": previous.map(|p| json!({
                "media_time_s": p.media_time_s,
                "clock_seconds": p.clock_seconds,
                "clock_label": p.clock_label,
            })),
        })
        .to_string(),
    )?;
    Ok(clock)
}

#[tauri::command]
pub async fn delete_video_clock(workspace_path: String, media_hash: String) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;
    if let Some(prev) = video_repo::find_clock(&conn, &manifest.occurrence_id, &media_hash)? {
        video_repo::delete_clock(&conn, &manifest.occurrence_id, &media_hash)?;
        video_repo::insert_log(
            &conn,
            &manifest.occurrence_id,
            Some(&media_hash),
            "clock.delete",
            &json!({
                "media_time_s": prev.media_time_s,
                "clock_seconds": prev.clock_seconds,
                "clock_label": prev.clock_label,
            })
            .to_string(),
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Exportar trecho

/// Grava parte de um vídeo do caso como ARQUIVO NOVO em `videos/trechos/`
/// (o original não é tocado), com JSON ao lado, e registra a cópia como vídeo
/// do caso ligado à origem (`derived_from_hash`). Ver `video::clip` para a
/// diferença entre "copy" (sem recompressão) e "reencode".
#[tauri::command]
pub async fn export_video_clip(
    workspace_path: String,
    input: ExportClipInput,
) -> Result<ExportClipResult> {
    use crate::video::clip::{clip_filename, export_clip, ClipMode, ClipOptions};

    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let mode = match input.mode.as_str() {
        "copy" => ClipMode::Copy,
        "reencode" => ClipMode::Reencode,
        other => return Err(SicroError::Validation(format!("modo de trecho inválido: {other}"))),
    };
    if !input.start_s.is_finite() || !input.end_s.is_finite() || input.start_s < 0.0 {
        return Err(SicroError::Validation("tempos do trecho inválidos".into()));
    }
    if input.end_s - input.start_s < 0.04 {
        return Err(SicroError::Validation("o trecho precisa ter pelo menos um quadro".into()));
    }
    let source = video_repo::find_media_by_sha256(&conn, &occurrence_id, &input.media_hash)?
        .ok_or_else(|| SicroError::Validation("vídeo de origem não encontrado nesta ocorrência".into()))?;
    let source_path = ws.join(&source.relative_path);
    if !source_path.is_file() {
        return Err(SicroError::Filesystem(format!("vídeo de origem ausente: {}", source.relative_path)));
    }

    let clips_dir = ws.join(CLIPS_SUBDIR);
    std::fs::create_dir_all(&clips_dir)
        .map_err(|e| SicroError::Filesystem(format!("cannot create {}: {e}", clips_dir.display())))?;
    let wanted = clip_filename(&source_path, input.start_s, input.end_s, mode);
    let target_name = pick_unique_filename(&clips_dir, &wanted);
    let target_path = clips_dir.join(&target_name);

    // ffmpeg fora do runtime assíncrono (pode levar segundos em trecho longo).
    let (src_c, out_c) = (source_path.clone(), target_path.clone());
    let (start, end, audio) = (input.start_s, input.end_s, input.include_audio);
    let clip = tauri::async_runtime::spawn_blocking(move || {
        export_clip(&ClipOptions {
            video: &src_c,
            out: &out_c,
            start_s: start,
            end_s: end,
            mode,
            include_audio: audio,
        })
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("clip task: {e}")))??;

    let sha256 = sha256_file(&target_path)?;
    let size_bytes = std::fs::metadata(&target_path).map(|m| m.len()).unwrap_or(0);
    let probe = probe_media(&target_path).ok();
    let actual_end = clip.actual_end_s;

    let mut warnings: Vec<String> = Vec::new();
    let recuo = input.start_s - clip.actual_start_s;
    if mode == ClipMode::Copy && recuo > 0.001 {
        warnings.push(format!(
            "Sem recompressão, o trecho começa no quadro-chave anterior: {:.3} s antes do ponto marcado ({} em vez de {}).",
            recuo,
            format_seconds(clip.actual_start_s),
            format_seconds(input.start_s),
        ));
    }
    let excesso = clip.actual_end_s - clip.marked_end_s;
    if mode == ClipMode::Copy && excesso > 0.001 {
        warnings.push(format!(
            "Sem recompressão, o trecho também vai {:.3} s além da saída marcada (termina em {}).",
            excesso,
            format_seconds(clip.actual_end_s),
        ));
    }
    if mode == ClipMode::Reencode {
        warnings.push("Trecho recomprimido (H.264, CRF 16): a imagem foi recodificada — os pixels não são os do original.".into());
    }

    // Mesmo trecho já exportado antes (bytes idênticos) → devolve o existente.
    if let Some(existing) = video_repo::find_media_by_sha256(&conn, &occurrence_id, &sha256)? {
        let _ = std::fs::remove_file(&target_path);
        return Ok(ExportClipResult {
            media: existing,
            actual_start_s: clip.actual_start_s,
            actual_end_s: actual_end,
            already_existed: true,
            warnings,
        });
    }

    let now = Utc::now();
    let derivation = json!({
        "kind": "clip",
        "source": {
            "sha256": source.sha256,
            "filename": source.filename,
            "relative_path": source.relative_path,
        },
        "requested": { "start_s": input.start_s, "end_s": input.end_s },
        "marked_frames_end_s": clip.marked_end_s,
        "actual": { "start_s": clip.actual_start_s, "end_s": actual_end },
        "mode": mode.as_str(),
        "include_audio": input.include_audio,
        "ffmpeg_command": clip.command,
        "created_at": now.to_rfc3339(),
        "note": "Cópia de parte do vídeo de origem; o original não foi alterado.",
    });
    // JSON ao lado do trecho (quem abrir a pasta do caso entende o arquivo).
    let sidecar = target_path.with_extension(format!(
        "{}.json",
        target_path.extension().and_then(|e| e.to_str()).unwrap_or("mp4")
    ));
    std::fs::write(
        &sidecar,
        serde_json::to_vec_pretty(&json!({ "sha256": sha256, "derivation": derivation }))
            .unwrap_or_default(),
    )
    .map_err(|e| SicroError::Filesystem(format!("cannot write {}: {e}", sidecar.display())))?;

    let mut probe_warnings = Vec::new();
    let media = VideoMedia {
        id: Uuid::new_v4(),
        occurrence_id,
        original_path: None,
        relative_path: format!("{CLIPS_SUBDIR}/{target_name}"),
        filename: target_name.clone(),
        sha256: sha256.clone(),
        size_bytes,
        duration_s: probe.as_ref().and_then(|p| p.duration_s),
        codec: probe.as_ref().and_then(|p| p.video_codec.clone()),
        width: probe.as_ref().and_then(|p| p.width),
        height: probe.as_ref().and_then(|p| p.height),
        pixel_format: probe.as_ref().and_then(|p| p.pixel_format.clone()),
        fps_declared: probe.as_ref().and_then(|p| p.fps_declared),
        avg_frame_rate: probe.as_ref().and_then(|p| p.avg_frame_rate.clone()),
        r_frame_rate: probe.as_ref().and_then(|p| p.r_frame_rate.clone()),
        time_base: probe.as_ref().and_then(|p| p.time_base.clone()),
        frame_count: probe.as_ref().and_then(|p| p.frame_count),
        bitrate: probe.as_ref().and_then(|p| p.bitrate),
        raw_probe_json: probe.as_ref().map(|p| p.raw_json.clone()).unwrap_or_else(|| "{}".into()),
        warnings_json: {
            if let Some(p) = &probe {
                probe_warnings.extend(p.warnings.clone());
            }
            serde_json::to_string(&probe_warnings).unwrap_or_else(|_| "[]".into())
        },
        created_at: now,
        updated_at: now,
        derived_from_hash: Some(source.sha256.clone()),
        derivation_json: Some(derivation.to_string()),
    };
    video_repo::insert_media(&conn, &media)?;
    video_repo::insert_log(
        &conn,
        &occurrence_id,
        Some(&source.sha256),
        "clip.export",
        &json!({
            "clip_media_id": media.id.to_string(),
            "clip_sha256": media.sha256,
            "filename": media.filename,
            "derivation": derivation,
        })
        .to_string(),
    )?;
    occurrence_repo::record_audit(
        &conn,
        Some(&occurrence_id),
        "video.clip.exported",
        Some("video"),
        Some("video_media"),
        Some(&media.id),
        Some(&media.sha256),
    )?;

    Ok(ExportClipResult {
        media,
        actual_start_s: clip.actual_start_s,
        actual_end_s: actual_end,
        already_existed: false,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Miniatura

/// Caminho absoluto da miniatura do vídeo (gera na primeira vez). Fica no
/// cache do app, nomeada pelo SHA-256 — o `.sicro` não é tocado.
#[tauri::command]
pub async fn video_thumbnail(
    app: tauri::AppHandle,
    workspace_path: String,
    media_id: String,
) -> Result<String> {
    use tauri::Manager;
    let ws = PathBuf::from(&workspace_path);
    let conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    let id = Uuid::parse_str(&media_id)
        .map_err(|e| SicroError::Validation(format!("invalid media_id: {e}")))?;
    let media = video_repo::find_media_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("media {id} not found")))?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| SicroError::Filesystem(format!("cache dir: {e}")))?;
    let out = cache.join("video_thumbs").join(format!("{}.jpg", media.sha256));
    if !out.is_file() {
        let video = ws.join(&media.relative_path);
        // ~15% do vídeo (máx. 10 s): passa do eventual trecho vazio do início.
        let ts = media.duration_s.map(|d| (d * 0.15).min(10.0)).unwrap_or(1.0);
        tauri::async_runtime::spawn_blocking(move || {
            crate::video::thumbnail::make_thumbnail(&video, &out, ts).map(|_| out)
        })
        .await
        .map_err(|e| SicroError::Workspace(format!("thumbnail task: {e}")))??;
    }
    let out = cache.join("video_thumbs").join(format!("{}.jpg", media.sha256));
    Ok(out.to_string_lossy().into_owned())
}

// ---------------------------------------------------------------------------
// helpers

fn pick_unique_filename(dir: &Path, src: &Path) -> String {
    let raw = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("video.bin");
    // Run through the same sanitizer used by workspaces.
    let safe = crate::filesystem::sanitize_folder_name(raw);
    let target = dir.join(&safe);
    if !target.exists() {
        return safe;
    }
    let stem = Path::new(&safe)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("video");
    let ext = Path::new(&safe)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("bin");
    for n in 2..=999 {
        let cand = format!("{stem}_{n}.{ext}");
        if !dir.join(&cand).exists() {
            return cand;
        }
    }
    format!("{stem}_{}.{ext}", Uuid::new_v4())
}

const ALLOWED_CATEGORIES: &[&str] = &[
    "colisao",
    "frenagem",
    "impacto",
    "reacao",
    "semaforo",
    "mudanca_faixa",
    "outro",
];

fn validate_category(raw: &str) -> Result<String> {
    let c = raw.trim().to_ascii_lowercase();
    if c.is_empty() {
        return Ok("outro".to_string());
    }
    if !ALLOWED_CATEGORIES.contains(&c.as_str()) {
        return Err(SicroError::Validation(format!(
            "categoria desconhecida: {c}. Aceitas: {ALLOWED_CATEGORIES:?}"
        )));
    }
    Ok(c)
}

fn format_seconds(s: f64) -> String {
    let s = s.max(0.0);
    let total_ms = (s * 1000.0).round() as i64;
    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let sec = total_s % 60;
    let min = (total_s / 60) % 60;
    let hr = total_s / 3600;
    format!("{hr:02}:{min:02}:{sec:02}.{ms:03}")
}

fn estimate_frame_index(ts: f64, fps: Option<f64>) -> Option<i64> {
    let fps = fps?;
    if !fps.is_finite() || fps <= 0.0 {
        return None;
    }
    Some((ts * fps).round() as i64)
}
