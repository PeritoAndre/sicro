//! Comandos Tauri do módulo Vídeo: registro de mídia (hash + ffprobe), eventos,
//! coleta de frames (storyboard), relógio da câmera, trechos e miniatura.

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
use crate::workspace::manifest::Manifest;
use crate::workspace::paths::{REL_MONTAGENS_SAIDA, REL_VIDEOS_FRAMES, REL_VIDEOS_ORIGINAIS, REL_VIDEOS_TRECHOS, dir_montagens_json, dir_montagens_saida, dir_videos_frames, dir_videos_originais, dir_videos_trechos, resolver_relativo, sqlite_path};
use crate::workspace::open_workspace;


#[tauri::command]
pub async fn register_video_media(
    workspace_path: String,
    input: RegisterVideoInput,
) -> Result<VideoMedia> {
    registrar_video(&PathBuf::from(&workspace_path), input, &|de, para| {
        std::fs::copy(de, para).map(|_| ())
    })
}

type Copiar<'a> = &'a dyn Fn(&Path, &Path) -> std::io::Result<()>;

fn registrar_video(ws: &Path, input: RegisterVideoInput, copiar: Copiar<'_>) -> Result<VideoMedia> {
    let ws = ws.to_path_buf();
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;

    let mut conn = open_connection(&sqlite_path(&ws))?;
    run_migrations(&mut conn)?;

    let source = PathBuf::from(&input.source_path);
    if !source.is_file() {
        return Err(SicroError::Filesystem(format!(
            "video file not found at {}",
            source.display()
        )));
    }

    // Hash da origem ANTES de copiar: identidade garantida mesmo se a cópia falhar.
    let sha256 = sha256_file(&source)?;

    if let Some(existing) =
        video_repo::find_media_by_sha256(&conn, &occurrence_id, &sha256)?
    {
        if !input.allow_duplicate {
            // O front reconhece o prefixo e pergunta se anexa de novo.
            return Err(SicroError::Validation(format!(
                "VIDEO_DUPLICADO:{}: vídeo já registrado nesta ocorrência.",
                existing.id
            )));
        }
    }

    let videos_dir = dir_videos_originais(&ws);
    std::fs::create_dir_all(&videos_dir).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create {}: {}",
            videos_dir.display(),
            e
        ))
    })?;
    // Anexo repetido aponta para o arquivo já copiado (mesmo hash): sem segunda cópia.
    let reuse = video_repo::find_media_by_sha256(&conn, &occurrence_id, &sha256)?
        .filter(|m| resolver_relativo(&ws, &m.relative_path).is_file());
    let (target_name, target_path) = match &reuse {
        Some(m) => (m.filename.clone(), resolver_relativo(&ws, &m.relative_path)),
        None => {
            let name = pick_unique_filename(&videos_dir, &source);
            let path = videos_dir.join(&name);
            copiar_conferindo(&source, &path, &sha256, copiar)?;
            (name, path)
        }
    };

    let size_bytes = std::fs::metadata(&target_path).map(|m| m.len()).unwrap_or(0);

    // ffprobe é best-effort: falha vira warning, não aborta o registro.
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

    let id = Uuid::new_v4();
    let now = Utc::now();
    let relative_path = match &reuse {
        Some(m) => m.relative_path.clone(),
        None => format!("{REL_VIDEOS_ORIGINAIS}/{}", target_name),
    };
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
            "anexo_repetido": reuse.is_some(),
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
    let mut conn = open_connection(&sqlite_path(&ws))?;
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
    let conn = open_connection(&sqlite_path(&ws))?;

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
    let mut conn = open_connection(&sqlite_path(&ws))?;
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
    let mut conn = open_connection(&sqlite_path(&ws))?;
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
    let mut conn = open_connection(&sqlite_path(&ws))?;
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
    let mut conn = open_connection(&sqlite_path(&ws))?;
    run_migrations(&mut conn)?;

    let media = video_repo::find_media_by_sha256(&conn, &occurrence_id, &input.media_hash)?
        .ok_or_else(|| {
            SicroError::Validation(format!(
                "mídia com sha256 {} não encontrada nesta ocorrência",
                input.media_hash
            ))
        })?;
    let video_abs = resolver_relativo(&ws, &media.relative_path);
    if !video_abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "arquivo de vídeo ausente em {}",
            video_abs.display()
        )));
    }

    let frames_dir = dir_videos_frames(&ws);
    std::fs::create_dir_all(&frames_dir).ok();
    let ts_for_name = ((input.timestamp_s * 1000.0).round() as i64).max(0);
    let stamp = Utc::now().format("%Y%m%d_%H%M%S");
    let png_name = format!("frame_{stamp}_{ts_for_name}ms.png");
    let png_target = frames_dir.join(&png_name);
    let sidecar_name = format!("frame_{stamp}_{ts_for_name}ms.json");
    let sidecar_target = frames_dir.join(&sidecar_name);

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
        output_path: format!("{REL_VIDEOS_FRAMES}/{png_name}"),
        filename: png_name.clone(),
        sidecar_json_path: Some(format!("{REL_VIDEOS_FRAMES}/{sidecar_name}")),
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
    let mut conn = open_connection(&sqlite_path(&ws))?;
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
    let mut conn = open_connection(&sqlite_path(&ws))?;
    run_migrations(&mut conn)?;

    let id = Uuid::parse_str(&frame_id)
        .map_err(|e| SicroError::Validation(format!("invalid frame_id: {e}")))?;
    let frame = video_repo::find_storyboard_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation(format!("storyboard frame {} not found", id)))?;

    if delete_png.unwrap_or(false) {
        let png_abs = resolver_relativo(&ws, &frame.output_path);
        let _ = std::fs::remove_file(&png_abs);
        if let Some(sidecar) = &frame.sidecar_json_path {
            let _ = std::fs::remove_file(resolver_relativo(&ws, &sidecar));
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
    let conn = open_connection(&sqlite_path(&ws))?;
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
    let mut conn = open_connection(&sqlite_path(&ws))?;
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
    let mut conn = open_connection(&sqlite_path(&ws))?;
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
    let mut conn = open_connection(&sqlite_path(&ws))?;
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

/// Grava o trecho como ARQUIVO NOVO em `videos/trechos/` (original intacto), com
/// JSON ao lado, registrado como vídeo do caso ligado à origem (`derived_from_hash`).
#[tauri::command]
pub async fn export_video_clip(
    workspace_path: String,
    input: ExportClipInput,
) -> Result<ExportClipResult> {
    use crate::video::clip::{clip_filename, export_clip, ClipMode, ClipOptions};

    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occurrence_id = manifest.occurrence_id;
    let mut conn = open_connection(&sqlite_path(&ws))?;
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
    let source_path = resolver_relativo(&ws, &source.relative_path);
    if !source_path.is_file() {
        return Err(SicroError::Filesystem(format!("vídeo de origem ausente: {}", source.relative_path)));
    }

    let clips_dir = dir_videos_trechos(&ws);
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
        relative_path: format!("{REL_VIDEOS_TRECHOS}/{target_name}"),
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
    let conn = open_connection(&sqlite_path(&ws))?;
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
        let video = resolver_relativo(&ws, &media.relative_path);
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

/// Copia e confere o SHA-256 da cópia com o do original; se diverge, a cópia sai.
fn copiar_conferindo(de: &Path, para: &Path, sha_original: &str, copiar: Copiar<'_>) -> Result<()> {
    let apagar = || {
        let _ = std::fs::remove_file(para);
    };
    copiar(de, para).map_err(|e| {
        apagar();
        SicroError::Filesystem(format!("could not copy video to {}: {}", para.display(), e))
    })?;
    let sha_copia = sha256_file(para).inspect_err(|_| apagar())?;
    if sha_copia != sha_original {
        apagar();
        return Err(SicroError::Filesystem(format!(
            "a cópia de {} não confere com o original (SHA-256 {sha_copia}, esperado {sha_original})",
            de.display()
        )));
    }
    Ok(())
}

fn pick_unique_filename(dir: &Path, src: &Path) -> String {
    let raw = src
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("video.bin");
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

// ---------------------------------------------------------------------------
// Excluir vídeo

/// Remove o registro e, se nenhum outro registro tiver o mesmo hash, tudo que foi
/// marcado nele (eventos, quadros, medições…) e o arquivo copiado. O log fica.
#[tauri::command]
pub async fn delete_video_media(workspace_path: String, media_id: String) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occ = manifest.occurrence_id;
    let mut conn = open_connection(&sqlite_path(&ws))?;
    run_migrations(&mut conn)?;

    let id = Uuid::parse_str(&media_id)
        .map_err(|e| SicroError::Validation(format!("invalid media_id: {e}")))?;
    let media = video_repo::find_media_by_id(&conn, &id)?
        .ok_or_else(|| SicroError::Validation("vídeo não encontrado".into()))?;
    let outros: Vec<VideoMedia> = video_repo::list_media_for_occurrence(&conn, &occ)?
        .into_iter()
        .filter(|m| m.id != id)
        .collect();
    let hash_em_uso = outros.iter().any(|m| m.sha256 == media.sha256);
    let arquivo_em_uso = outros.iter().any(|m| m.relative_path == media.relative_path);

    // Arquivos dos quadros/exportações antes de apagar as linhas que os apontam.
    let mut arquivos: Vec<String> = Vec::new();
    if !hash_em_uso {
        for tabela in ["video_exports", "video_storyboard_frames"] {
            let mut st = conn.prepare(&format!(
                "SELECT output_path, sidecar_json_path FROM {tabela} WHERE occurrence_id = ?1 AND media_hash = ?2"
            ))?;
            let linhas = st.query_map([occ.to_string(), media.sha256.clone()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
            })?;
            for l in linhas {
                let (a, b) = l?;
                arquivos.push(a);
                arquivos.extend(b);
            }
        }
    }

    let tx = conn.unchecked_transaction()?;
    let mut apagados: Vec<String> = Vec::new();
    if !hash_em_uso {
        // Toda tabela com media_hash + occurrence_id, menos o log de operações.
        let nomes: Vec<String> = {
            let mut st = tx.prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")?;
            let r = st.query_map([], |r| r.get::<_, String>(0))?;
            r.collect::<std::result::Result<_, _>>()?
        };
        for nome in nomes {
            if nome == "video_operation_logs" {
                continue;
            }
            let colunas: Vec<String> = {
                let mut st = tx.prepare("SELECT name FROM pragma_table_info(?1)")?;
                let r = st.query_map([&nome], |r| r.get::<_, String>(0))?;
                r.collect::<std::result::Result<_, _>>()?
            };
            if colunas.iter().any(|c| c == "media_hash") && colunas.iter().any(|c| c == "occurrence_id") {
                let n = tx.execute(
                    &format!("DELETE FROM \"{nome}\" WHERE occurrence_id = ?1 AND media_hash = ?2"),
                    [occ.to_string(), media.sha256.clone()],
                )?;
                if n > 0 {
                    apagados.push(format!("{nome}: {n}"));
                }
            }
        }
    }
    tx.execute("DELETE FROM video_media WHERE id = ?1", [id.to_string()])?;
    tx.commit()?;

    let mut arquivos_removidos = 0;
    if !arquivo_em_uso {
        arquivos.push(media.relative_path.clone());
    }
    for rel in arquivos {
        let p = resolver_relativo(&ws, &rel);
        if rel.contains("..") || !p.is_file() {
            continue;
        }
        if std::fs::remove_file(&p).is_ok() {
            arquivos_removidos += 1;
        }
    }

    video_repo::insert_log(
        &conn,
        &occ,
        Some(&media.sha256),
        "media.delete",
        &json!({
            "media_id": media.id.to_string(),
            "filename": media.filename,
            "relative_path": media.relative_path,
            "dados_removidos": !hash_em_uso,
            "linhas_removidas": apagados,
            "arquivos_removidos": arquivos_removidos,
        })
        .to_string(),
    )?;
    occurrence_repo::record_audit(
        &conn,
        Some(&occ),
        "video.media.deleted",
        Some("video"),
        Some("video_media"),
        Some(&media.id),
        Some(&media.sha256),
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Montagem


fn id_seguro(id: &str) -> Result<&str> {
    if !id.is_empty() && id.len() <= 80 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        Ok(id)
    } else {
        Err(SicroError::Validation("id de montagem inválido".into()))
    }
}

fn montagem_path(ws: &Path, id: &str) -> Result<PathBuf> {
    Ok(dir_montagens_json(&ws).join(format!("{}.json", id_seguro(id)?)))
}

#[tauri::command]
pub async fn list_montagens(workspace_path: String) -> Result<Vec<crate::models::Montagem>> {
    let dir = dir_montagens_json(Path::new(&workspace_path));
    let Ok(rd) = std::fs::read_dir(&dir) else { return Ok(Vec::new()) };
    let mut v: Vec<crate::models::Montagem> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            n.ends_with(".json") && !n.contains(".relatorio.")
        })
        .filter_map(|p| serde_json::from_slice(&std::fs::read(p).ok()?).ok())
        .collect();
    v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(v)
}

#[tauri::command]
pub async fn save_montagem(
    workspace_path: String,
    montagem: crate::models::Montagem,
) -> Result<crate::models::Montagem> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let mut m = montagem;
    m.updated_at = Utc::now().to_rfc3339();
    let path = montagem_path(&ws, &m.id)?;
    std::fs::create_dir_all(dir_montagens_json(&ws))?;
    // Grava num temporário e renomeia: queda no meio não deixa o JSON pela metade.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&m)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(m)
}

#[tauri::command]
pub async fn delete_montagem(workspace_path: String, montagem_id: String) -> Result<()> {
    let ws = PathBuf::from(&workspace_path);
    let path = montagem_path(&ws, &montagem_id)?;
    if path.is_file() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

#[derive(serde::Serialize, Clone)]
struct MontagemProgresso<'a> {
    montagem_id: &'a str,
    pct: f64,
}

/// Renderiza a montagem num MP4 novo (H.264/AAC) com relatório ao lado. Os originais não são tocados.
#[tauri::command]
pub async fn render_montagem(
    app: tauri::AppHandle,
    workspace_path: String,
    montagem_id: String,
    previa: Option<bool>,
) -> Result<crate::models::RenderMontagemResult> {
    use crate::video::montagem::{construir, fonte_padrao, Fonte};
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use tauri::Emitter;

    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let occ = manifest.occurrence_id;
    let mut conn = open_connection(&sqlite_path(&ws))?;
    run_migrations(&mut conn)?;

    let mpath = montagem_path(&ws, &montagem_id)?;
    let mut montagem: crate::models::Montagem = serde_json::from_slice(
        &std::fs::read(&mpath).map_err(|e| SicroError::Filesystem(format!("montagem não encontrada: {e}")))?,
    )?;
    // Prévia rápida: mesmo pipeline em 854x480 (a proporção da saída final não é mantida).
    let previa = previa.unwrap_or(false);
    if previa {
        montagem.saida.largura = 854;
        montagem.saida.altura = 480;
    }

    // Resolve cada vídeo usado (e guarda o hash do original para o relatório).
    let mut usados: std::collections::HashMap<String, VideoMedia> = std::collections::HashMap::new();
    for it in &montagem.itens {
        let mid = match it {
            crate::models::MontagemItem::Trecho { media_id, .. }
            | crate::models::MontagemItem::Congelar { media_id, .. } => media_id,
            _ => continue,
        };
        if usados.contains_key(mid) {
            continue;
        }
        let uid = Uuid::parse_str(mid).map_err(|_| SicroError::Validation("vídeo inválido na montagem".into()))?;
        let m = video_repo::find_media_by_id(&conn, &uid)?
            .ok_or_else(|| SicroError::Validation("um vídeo da montagem não está mais no caso".into()))?;
        usados.insert(mid.clone(), m);
    }
    let resolver = |id: &str| -> Result<Fonte> {
        let m = usados.get(id).ok_or_else(|| SicroError::Validation("vídeo da montagem ausente".into()))?;
        let path = resolver_relativo(&ws, &m.relative_path);
        if !path.is_file() {
            return Err(SicroError::Filesystem(format!("vídeo ausente: {}", m.relative_path)));
        }
        let tem_audio = serde_json::from_str::<serde_json::Value>(&m.raw_probe_json)
            .ok()
            .and_then(|j| {
                j.get("streams")?
                    .as_array()?
                    .iter()
                    .any(|s| s.get("codec_type").and_then(|c| c.as_str()) == Some("audio"))
                    .then_some(())
            })
            .is_some();
        Ok(Fonte { path, tem_audio })
    };
    let resolver_audio = |rel: &str| -> Result<PathBuf> {
        if rel.contains("..") {
            return Err(SicroError::Validation("caminho de áudio inválido".into()));
        }
        let path = resolver_relativo(&ws, rel);
        if !path.is_file() {
            return Err(SicroError::Filesystem(format!("áudio ausente: {rel}")));
        }
        Ok(path)
    };
    let plano = construir(&montagem, &fonte_padrao(), &resolver, &resolver_audio)?;

    let ffmpeg = crate::tools::find_ffmpeg_tool("ffmpeg")
        .ok_or_else(|| SicroError::Workspace("ffmpeg não encontrado".into()))?;
    let dir = if previa { crate::workspace::paths::dir_cache(&ws) } else { dir_montagens_saida(&ws) };
    std::fs::create_dir_all(&dir)?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let out_name = if previa {
        format!("montagem-previa-{}.mp4", montagem.id)
    } else {
        format!("{}_{stamp}.mp4", montagem.id)
    };
    let out_path = dir.join(&out_name);

    // Pasta de trabalho com os textos (o ffmpeg roda nela: nomes simples no filtro).
    let work = dir.join(format!(".render-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&work)?;
    for (nome, conteudo) in &plano.textos {
        std::fs::write(work.join(nome), conteudo)?;
    }
    for (nome, bytes) in &plano.imagens {
        std::fs::write(work.join(nome), bytes)?;
    }
    let log_path = work.join("ffmpeg.log");

    let mut args: Vec<String> = ["-y", "-nostdin", "-hide_banner", "-loglevel", "error", "-progress", "pipe:1", "-nostats"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for i in &plano.inputs {
        args.push("-i".into());
        args.push(i.to_string_lossy().into_owned());
    }
    let (preset, crf) = if previa { ("ultrafast", "28") } else { ("medium", "18") };
    let fim: Vec<String> = [
        "-filter_complex", plano.filter_complex.as_str(), "-map", "[v]", "-map", "[a]",
        "-c:v", "libx264", "-preset", preset, "-crf", crf, "-pix_fmt", "yuv420p",
        "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.extend(fim);
    args.push(out_path.to_string_lossy().into_owned());

    let (app2, mid, total, work2, log2) = (app.clone(), montagem.id.clone(), plano.duracao_s, work.clone(), log_path.clone());
    let args2 = args.clone();
    let status = tauri::async_runtime::spawn_blocking(move || -> Result<bool> {
        let log = std::fs::File::create(&log2)?;
        let mut child = crate::tools::command(&ffmpeg)
            .args(&args2)
            .current_dir(&work2)
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .spawn()
            .map_err(|e| SicroError::Workspace(format!("não foi possível iniciar o ffmpeg: {e}")))?;
        if let Some(out) = child.stdout.take() {
            for linha in BufReader::new(out).lines().map_while(|l| l.ok()) {
                if let Some(us) = linha.strip_prefix("out_time_us=").or_else(|| linha.strip_prefix("out_time_ms=")) {
                    if let Ok(us) = us.trim().parse::<f64>() {
                        let pct = (us / 1_000_000.0 / total.max(0.001) * 100.0).clamp(0.0, 99.0);
                        let _ = app2.emit("montagem:progresso", MontagemProgresso { montagem_id: &mid, pct });
                    }
                }
            }
        }
        Ok(child.wait()?.success())
    })
    .await
    .map_err(|e| SicroError::Workspace(format!("render task: {e}")))??;

    if !status {
        let msg = std::fs::read_to_string(&log_path).unwrap_or_default();
        let _ = std::fs::remove_file(&out_path);
        let _ = std::fs::remove_dir_all(&work);
        let tail: String = msg.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
        let dica = if tail.contains("drawtext") || tail.contains("ont") {
            " (o ffmpeg desta máquina pode estar sem fonte para os textos)"
        } else {
            ""
        };
        return Err(SicroError::Workspace(format!("a renderização falhou{dica}: {tail}")));
    }
    let _ = std::fs::remove_dir_all(&work);
    let _ = app.emit("montagem:progresso", MontagemProgresso { montagem_id: &montagem.id, pct: 100.0 });

    let sha256 = sha256_file(&out_path)?;
    let duracao_s = probe_media(&out_path).ok().and_then(|p| p.duration_s).unwrap_or(plano.duracao_s);
    if previa {
        let relativo = format!("{}/{out_name}", crate::workspace::paths::REL_CACHE);
        return Ok(crate::models::RenderMontagemResult { output_relative_path: relativo, sha256, duracao_s });
    }
    let relativo = format!("{REL_MONTAGENS_SAIDA}/{out_name}");

    let itens: Vec<serde_json::Value> = montagem
        .itens
        .iter()
        .map(|it| {
            let mut v = serde_json::to_value(it).unwrap_or_default();
            let mid = v.get("media_id").and_then(|x| x.as_str()).map(str::to_string);
            if let (Some(mid), Some(o)) = (mid, v.as_object_mut()) {
                if let Some(m) = usados.get(&mid) {
                    o.insert("origem".into(), json!({ "arquivo": m.filename, "sha256": m.sha256 }));
                }
            }
            v
        })
        .collect();
    let relatorio = json!({
        "montagem": { "id": montagem.id, "titulo": montagem.titulo },
        "saida": {
            "arquivo": out_name,
            "sha256": sha256,
            "duracao_s": duracao_s,
            "largura": montagem.saida.largura,
            "altura": montagem.saida.altura,
            "fps": montagem.saida.fps,
            "incluir_audio": montagem.saida.incluir_audio,
        },
        "itens": itens,
        "volume_original": montagem.volume_original,
        "narracoes": montagem
            .narracoes
            .iter()
            .map(|n| {
                let sha = sha256_file(&resolver_relativo(&ws, &n.arquivo_relativo)).ok();
                let mut v = serde_json::to_value(n).unwrap_or_default();
                if let Some(o) = v.as_object_mut() {
                    o.insert("sha256".into(), json!(sha));
                }
                v
            })
            .collect::<Vec<_>>(),
        "renderizado_em": Utc::now().to_rfc3339(),
        "sicro_versao": env!("CARGO_PKG_VERSION"),
        "ffmpeg_args": args,
        "nota": "Vídeo novo gerado a partir de cópias de trabalho; os originais não foram alterados.",
    });
    let rel_path = dir.join(format!("{out_name}.relatorio.json"));
    std::fs::write(&rel_path, serde_json::to_vec_pretty(&relatorio)?)
        .map_err(|e| SicroError::Filesystem(format!("cannot write {}: {e}", rel_path.display())))?;

    video_repo::insert_log(
        &conn,
        &occ,
        None,
        "montagem.render",
        &json!({
            "montagem_id": montagem.id,
            "saida": relativo,
            "sha256": sha256,
            "itens": montagem.itens.len(),
            "duracao_s": duracao_s,
        })
        .to_string(),
    )?;

    Ok(crate::models::RenderMontagemResult { output_relative_path: relativo, sha256, duracao_s })
}

// ---------------------------------------------------------------------------
// Áudios da montagem (narração gravada ou importada)

#[derive(serde::Serialize)]
pub struct AudioMontagem {
    pub arquivo_relativo: String,
    pub nome: String,
}

fn guardar_audio_montagem(ws: &Path, nome: &str, ext: &str, escrever: impl FnOnce(&Path) -> Result<()>) -> Result<AudioMontagem> {
    let ext = ext.to_ascii_lowercase();
    if !["mp3", "wav", "m4a", "ogg", "webm", "aac", "flac", "opus", "mp4"].contains(&ext.as_str()) {
        return Err(SicroError::Validation("formato de áudio não suportado".into()));
    }
    let dir = dir_montagens_json(ws).join("audios");
    std::fs::create_dir_all(&dir)?;
    let arquivo = format!("{}.{ext}", Uuid::new_v4());
    escrever(&dir.join(&arquivo))?;
    Ok(AudioMontagem {
        arquivo_relativo: format!("{}/audios/{arquivo}", crate::workspace::paths::REL_MONTAGENS_JSON),
        nome: nome.to_string(),
    })
}

/// Copia um arquivo de áudio escolhido pelo usuário para a pasta de áudios das montagens.
#[tauri::command]
pub async fn importar_audio_montagem(workspace_path: String, origem: String) -> Result<AudioMontagem> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let src = PathBuf::from(&origem);
    if !src.is_file() {
        return Err(SicroError::Filesystem("arquivo de áudio não encontrado".into()));
    }
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
    let nome = src.file_stem().and_then(|n| n.to_str()).unwrap_or("áudio").to_string();
    guardar_audio_montagem(&ws, &nome, &ext, |dest| {
        std::fs::copy(&src, dest)?;
        Ok(())
    })
}

/// Grava os bytes de uma narração captada no microfone (MediaRecorder).
#[tauri::command]
pub async fn gravar_audio_montagem(workspace_path: String, dados: Vec<u8>, extensao: String, nome: String) -> Result<AudioMontagem> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    if dados.is_empty() {
        return Err(SicroError::Validation("gravação vazia".into()));
    }
    guardar_audio_montagem(&ws, &nome, &extensao, |dest| {
        std::fs::write(dest, &dados)?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::NewOccurrenceInput;

    fn caso() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let ws = crate::workspace::create::create_workspace(NewOccurrenceInput::default(), tmp.path())
            .unwrap()
            .path;
        let origem = tmp.path().join("camera 1.dav");
        std::fs::write(&origem, (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect::<Vec<_>>()).unwrap();
        (tmp, ws, origem)
    }

    fn entrada(origem: &Path) -> RegisterVideoInput {
        serde_json::from_value(json!({ "source_path": origem.to_string_lossy() })).unwrap()
    }

    fn arquivos(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn copia_que_diverge_do_original_e_recusada_e_apagada() {
        let (_tmp, ws, origem) = caso();
        let corromper = |de: &Path, para: &Path| -> std::io::Result<()> {
            let mut b = std::fs::read(de)?;
            b[1000] ^= 1;
            std::fs::write(para, b)
        };
        let r = registrar_video(&ws, entrada(&origem), &corromper);
        assert!(matches!(&r, Err(SicroError::Filesystem(m)) if m.contains("SHA-256")), "{r:?}");
        assert!(arquivos(&dir_videos_originais(&ws)).is_empty(), "cópia ficou");
        let conn = open_connection(&sqlite_path(&ws)).unwrap();
        let occ = Manifest::read(&ws).unwrap().occurrence_id;
        assert!(video_repo::list_media_for_occurrence(&conn, &occ).unwrap().is_empty());
    }

    #[test]
    fn copia_fiel_e_registrada_com_o_hash_do_original() {
        let (_tmp, ws, origem) = caso();
        let m = registrar_video(&ws, entrada(&origem), &|de, para| std::fs::copy(de, para).map(|_| ())).unwrap();
        assert_eq!(m.sha256, sha256_file(&origem).unwrap());
        assert_eq!(sha256_file(&resolver_relativo(&ws, &m.relative_path)).unwrap(), m.sha256);
    }
}
