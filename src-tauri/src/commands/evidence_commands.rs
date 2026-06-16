//! Tauri commands for the Evidência → Laudo bridge (MVP 4).
//!
//! Three responsibilities:
//!   - `record_evidence_link`         → grava a linha em `evidence_links`
//!   - `list_evidence_links_for_laudo`→ lista para o painel do Inspector
//!   - `read_evidence_asset`          → lê bytes do asset (PNG/JPG) e
//!                                      devolve base64 — usado pelo
//!                                      renderer HTML/PDF e pelo DOCX walker.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use chrono::Utc;
use uuid::Uuid;

use crate::database::connection::open_connection;
use crate::database::migrations::run_migrations;
use crate::database::repositories::{evidence_link_repo, occurrence_repo};
use crate::error::{Result, SicroError};
use crate::filesystem::sanitize_relative_path as shared_sanitize_relative_path;
use crate::models::{EvidenceAsset, EvidenceLink, RecordEvidenceLinkInput};
use crate::workspace::manifest::{Manifest, SQLITE_FILENAME};

#[tauri::command]
pub async fn record_evidence_link(
    workspace_path: String,
    input: RecordEvidenceLinkInput,
) -> Result<EvidenceLink> {
    let ws = PathBuf::from(&workspace_path);
    let manifest = Manifest::read(&ws)?;
    let mut conn = open_connection(&ws.join(SQLITE_FILENAME))?;
    run_migrations(&mut conn)?;

    let link = EvidenceLink {
        id: Uuid::new_v4(),
        occurrence_id: manifest.occurrence_id,
        target_type: input.target_type.clone(),
        target_id: input.target_id.clone(),
        relation_type: input.relation_type,
        source_kind: input.source_kind,
        media_asset_id: input.media_asset_id,
        croqui_id: input.croqui_id,
        video_media_hash: input.video_media_hash,
        video_event_id: input.video_event_id,
        video_storyboard_frame_id: input.video_storyboard_frame_id,
        field_note_id: input.field_note_id,
        relative_path: input.relative_path,
        source_hash: input.source_hash,
        metadata_json: input.metadata_json,
        created_at: Utc::now(),
    };
    evidence_link_repo::insert(&conn, &link)?;
    occurrence_repo::record_audit(
        &conn,
        Some(&link.occurrence_id),
        "evidence.linked",
        Some("laudo"),
        Some(input.target_type.as_str()),
        None,
        Some(link.source_kind.as_str()),
    )?;
    Ok(link)
}


/// Read an evidence asset (photo, croqui PNG, video frame) and return it
/// base64-encoded. The renderer uses this to inline images as data URIs in
/// HTML/PDF; the DOCX walker reuses it to embed binary streams.
///
/// `relative_path` MUST be a workspace-relative path. Absolute paths and
/// `..` traversal are rejected.
#[tauri::command]
pub async fn read_evidence_asset(
    workspace_path: String,
    relative_path: String,
) -> Result<EvidenceAsset> {
    let ws = PathBuf::from(&workspace_path);
    let _ = Manifest::read(&ws)?;
    let safe = shared_sanitize_relative_path(&relative_path)?;
    let abs = ws.join(&safe);
    if !abs.is_file() {
        return Err(SicroError::Filesystem(format!(
            "asset not found at {}",
            abs.display()
        )));
    }
    let bytes = std::fs::read(&abs).map_err(|e| {
        SicroError::Filesystem(format!("cannot read {}: {}", abs.display(), e))
    })?;
    let size_bytes = bytes.len() as u64;
    let mime_type = guess_mime(&safe).unwrap_or_else(|| "application/octet-stream".to_string());
    let base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

    Ok(EvidenceAsset {
        relative_path,
        mime_type,
        base64,
        size_bytes,
    })
}

fn guess_mime(p: &Path) -> Option<String> {
    let ext = p
        .extension()
        .and_then(|s| s.to_str())?
        .to_ascii_lowercase();
    Some(
        match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "heic" => "image/heic",
            "svg" => "image/svg+xml",
            _ => return None,
        }
        .to_string(),
    )
}
