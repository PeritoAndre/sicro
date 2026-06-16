//! Aggregator — projects every module's rows into
//! `EvidenceRegistryItem`s.
//!
//! Each kind has its own builder function that takes the rows it needs
//! and emits items. The main `build_registry` function calls every
//! builder and concatenates the result. Items are NOT verified here —
//! `integrity::verify_workspace` is the only place that does
//! filesystem I/O.
//!
//! Synthetic IDs follow the pattern `<kind>:<repo-id>`. They are
//! intentionally NOT UUIDs because the same workspace can hold multiple
//! rows of different kinds with overlapping UUID spaces, and we want
//! the UI to identify an item by `(kind, source-id)` without ambiguity.

use std::collections::HashMap;

use rusqlite::Connection;
use uuid::Uuid;

use crate::database::repositories::{
    audio_repo, croqui_repo, documentoscopia_repo, evidence_link_repo, export_repo,
    image_analysis_repo, import_repo, laudo_repo, media_asset_repo, video_repo,
};
use crate::error::Result;
use crate::models::{
    EvidenceKind, EvidenceRegistryItem, IntegrityStatus, RegistrySummary,
};

pub fn build_registry(
    conn: &Connection,
    occurrence_id: &Uuid,
) -> Result<Vec<EvidenceRegistryItem>> {
    let mut out: Vec<EvidenceRegistryItem> = Vec::new();

    // ------------------------------------------------------------------
    // Photos (media_assets, type=photo)
    let media = media_asset_repo::list_by_occurrence(conn, occurrence_id)?;
    for m in &media {
        out.push(EvidenceRegistryItem {
            id: format!("photo:{}", m.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::Photo,
            subtype: m.mime_type.clone(),
            title: m
                .caption
                .clone()
                .or_else(|| m.original_filename.clone())
                .or_else(|| m.original_id.clone()),
            description: m.category.clone(),
            source_module: "importer".to_string(),
            original_id: m.original_id.clone(),
            relative_path: Some(m.relative_path.clone()),
            sidecar_relative_path: None,
            hash_sha256: m.sha256.clone(),
            size_bytes: Some(m.size_bytes),
            mime_type: m.mime_type.clone(),
            created_at: Some(m.imported_at),
            updated_at: m.captured_at,
            status: Some("imported".to_string()),
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: m.raw_json.clone(),
        });
    }

    // ------------------------------------------------------------------
    // Croquis (.sicrocroqui) — one item per croqui row.
    // The PNG export, when present, becomes a separate item below.
    let croquis = croqui_repo::list_by_occurrence(conn, occurrence_id)?;
    for c in &croquis {
        out.push(EvidenceRegistryItem {
            id: format!("croqui:{}", c.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::Croqui,
            subtype: Some("application/sicrocroqui".to_string()),
            title: Some(c.title.clone()),
            description: None,
            source_module: "croqui".to_string(),
            original_id: None,
            relative_path: Some(c.relative_path.clone()),
            sidecar_relative_path: None,
            hash_sha256: None,
            size_bytes: None,
            mime_type: Some("application/json".to_string()),
            created_at: Some(c.created_at),
            updated_at: Some(c.updated_at),
            status: Some(c.status.as_str().to_string()),
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: "{}".to_string(),
        });

        if let Some(png_rel) = c.last_export_relative_path.as_ref() {
            out.push(EvidenceRegistryItem {
                id: format!("croqui_export:{}", c.id),
                occurrence_id: *occurrence_id,
                kind: EvidenceKind::CroquiExport,
                subtype: Some("image/png".to_string()),
                title: Some(format!("{} (PNG)", c.title)),
                description: Some("Último PNG exportado".to_string()),
                source_module: "croqui".to_string(),
                original_id: Some(c.id.to_string()),
                relative_path: Some(png_rel.clone()),
                sidecar_relative_path: None,
                hash_sha256: None,
                size_bytes: None,
                mime_type: Some("image/png".to_string()),
                created_at: Some(c.updated_at),
                updated_at: Some(c.updated_at),
                status: Some(c.status.as_str().to_string()),
                integrity_status: IntegrityStatus::Unknown,
                integrity_detail: None,
                linked_laudos_count: 0,
                metadata_json: "{}".to_string(),
            });
        }
    }

    // ------------------------------------------------------------------
    // Videos
    let videos = video_repo::list_media_for_occurrence(conn, occurrence_id)?;
    for v in &videos {
        out.push(EvidenceRegistryItem {
            id: format!("video:{}", v.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::Video,
            subtype: v.codec.clone(),
            title: Some(v.filename.clone()),
            description: build_video_description(v),
            source_module: "video".to_string(),
            original_id: None,
            relative_path: Some(v.relative_path.clone()),
            sidecar_relative_path: None,
            hash_sha256: Some(v.sha256.clone()),
            size_bytes: Some(v.size_bytes),
            mime_type: guess_video_mime(&v.filename),
            created_at: Some(v.created_at),
            updated_at: Some(v.updated_at),
            status: None,
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: v.raw_probe_json.clone(),
        });

        // Storyboard frames belonging to this video.
        let frames =
            video_repo::list_storyboard_for_media(conn, occurrence_id, &v.sha256)?;
        for f in &frames {
            out.push(EvidenceRegistryItem {
                id: format!("storyboard_frame:{}", f.id),
                occurrence_id: *occurrence_id,
                kind: EvidenceKind::StoryboardFrame,
                subtype: Some("image/png".to_string()),
                title: if f.title.trim().is_empty() {
                    Some(format!("Frame {}", short_id(f.id)))
                } else {
                    Some(f.title.clone())
                },
                description: if f.caption.trim().is_empty() {
                    None
                } else {
                    Some(f.caption.clone())
                },
                source_module: "video".to_string(),
                original_id: Some(v.sha256.clone()),
                relative_path: Some(f.output_path.clone()),
                sidecar_relative_path: f.sidecar_json_path.clone(),
                hash_sha256: None,
                size_bytes: None,
                mime_type: Some("image/png".to_string()),
                created_at: Some(f.created_at),
                updated_at: Some(f.updated_at),
                status: if f.reviewed { Some("reviewed".to_string()) } else { Some("pending".to_string()) },
                integrity_status: IntegrityStatus::Unknown,
                integrity_detail: None,
                linked_laudos_count: 0,
                metadata_json: "{}".to_string(),
            });
        }
    }

    // ------------------------------------------------------------------
    // Laudos
    let laudos = laudo_repo::list_by_occurrence(conn, occurrence_id)?;
    for l in &laudos {
        out.push(EvidenceRegistryItem {
            id: format!("laudo:{}", l.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::Laudo,
            subtype: Some(l.template_id.clone()),
            title: Some(l.title.clone()),
            description: None,
            source_module: "laudo".to_string(),
            original_id: None,
            relative_path: Some(l.relative_path.clone()),
            sidecar_relative_path: None,
            hash_sha256: None,
            size_bytes: None,
            mime_type: Some("application/json".to_string()),
            created_at: Some(l.created_at),
            updated_at: Some(l.updated_at),
            status: Some(l.status.as_str().to_string()),
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: "{}".to_string(),
        });
    }

    // ------------------------------------------------------------------
    // Laudo exports (HTML/PDF/DOCX)
    let exports = export_repo::list_by_occurrence(conn, occurrence_id)?;
    for e in &exports {
        out.push(EvidenceRegistryItem {
            id: format!("laudo_export:{}", e.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::LaudoExport,
            subtype: Some(e.kind.as_str().to_string()),
            title: Some(format!(
                "Exportação {} do laudo {}",
                e.kind.as_str().to_uppercase(),
                short_id(e.laudo_id),
            )),
            description: None,
            source_module: "export".to_string(),
            original_id: Some(e.laudo_id.to_string()),
            relative_path: Some(e.relative_path.clone()),
            sidecar_relative_path: None,
            hash_sha256: None,
            size_bytes: if e.file_size > 0 {
                Some(e.file_size as u64)
            } else {
                None
            },
            mime_type: Some(match e.kind.as_str() {
                "pdf" => "application/pdf",
                "html" => "text/html",
                "docx" => {
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                }
                _ => "application/octet-stream",
            }
            .to_string()),
            created_at: Some(e.created_at),
            updated_at: Some(e.created_at),
            status: None,
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: "{}".to_string(),
        });
    }

    // ------------------------------------------------------------------
    // Image analyses + derived exports (MVP 7)
    // ------------------------------------------------------------------
    // Áudio (módulo Áudio). Para importado, a evidência é o ORIGINAL
    // preservado; para extraído, o WAV derivado (a origem é o vídeo, já
    // listado acima). A integridade é verificada genericamente pelo verifier.
    let audios = audio_repo::list_for_occurrence(conn, occurrence_id)?;
    for a in &audios {
        let imported = a.kind == "importado";
        let rel = if imported {
            a.original_relative_path.clone().or_else(|| Some(a.relative_path.clone()))
        } else {
            Some(a.relative_path.clone())
        };
        let hash = if imported {
            a.original_sha256.clone().or_else(|| Some(a.sha256.clone()))
        } else {
            Some(a.sha256.clone())
        };
        out.push(EvidenceRegistryItem {
            id: format!("audio:{}", a.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::Audio,
            subtype: a.codec.clone(),
            title: Some(a.filename.clone()),
            description: Some(match a.kind.as_str() {
                "importado" => "Áudio importado".to_string(),
                "realce" => "Áudio realçado (derivado)".to_string(),
                "recorte" => "Trecho recortado de áudio".to_string(),
                "compilacao" => "Compilação rotulada de trechos".to_string(),
                _ => "Áudio extraído de vídeo".to_string(),
            }),
            source_module: "audio".to_string(),
            original_id: a.source_video_sha256.clone(),
            relative_path: rel,
            sidecar_relative_path: None,
            hash_sha256: hash,
            size_bytes: Some(a.size_bytes),
            mime_type: None,
            created_at: Some(a.created_at),
            updated_at: Some(a.updated_at),
            status: None,
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: a.raw_probe_json.clone(),
        });
    }

    let analyses = image_analysis_repo::list_by_occurrence(conn, occurrence_id)?;
    for a in &analyses {
        out.push(EvidenceRegistryItem {
            id: format!("image_analysis:{}", a.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::ImageAnalysis,
            subtype: Some(a.source_kind.as_str().to_string()),
            title: Some(a.title.clone()),
            description: Some(format!("Sessão de análise — {}", a.source_kind.as_str())),
            source_module: "image_editor".to_string(),
            original_id: a.source_id.clone(),
            relative_path: Some(a.analysis_relative_path.clone()),
            sidecar_relative_path: None,
            hash_sha256: a.original_hash_sha256.clone(),
            size_bytes: None,
            mime_type: Some("application/json".to_string()),
            created_at: Some(a.created_at),
            updated_at: Some(a.updated_at),
            status: Some(a.status.clone()),
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: a.metadata_json.clone(),
        });
    }
    let image_exports = image_analysis_repo::list_exports_by_occurrence(
        conn, occurrence_id,
    )?;
    for e in &image_exports {
        out.push(EvidenceRegistryItem {
            id: format!("image_export:{}", e.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::ImageExport,
            subtype: Some(e.format.clone()),
            title: Some(format!(
                "Imagem derivada {}",
                short_id(e.image_analysis_id)
            )),
            description: Some(format!(
                "{} × {}",
                e.width.unwrap_or(0),
                e.height.unwrap_or(0)
            )),
            source_module: "image_editor".to_string(),
            original_id: Some(e.image_analysis_id.to_string()),
            relative_path: Some(e.output_relative_path.clone()),
            sidecar_relative_path: e.sidecar_relative_path.clone(),
            hash_sha256: e.hash_sha256.clone(),
            size_bytes: None,
            mime_type: Some(match e.format.as_str() {
                "png" => "image/png".to_string(),
                "jpg" | "jpeg" => "image/jpeg".to_string(),
                _ => "application/octet-stream".to_string(),
            }),
            created_at: Some(e.created_at),
            updated_at: Some(e.created_at),
            status: None,
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: e.operation_summary_json.clone(),
        });
    }

    // ------------------------------------------------------------------
    // Imported packages (.sicroapp)
    //
    // `imports` is workspace-scoped (one row per package brought in).
    // The current `import_repo::list_all` lists every import in the
    // SQLite db; since each workspace has its own SQLite, that is
    // effectively per-occurrence.
    let imports = import_repo::list_all(conn)?;
    for imp in &imports {
        out.push(EvidenceRegistryItem {
            id: format!("imported_package:{}", imp.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::ImportedPackage,
            subtype: Some(imp.format.clone()),
            title: imp
                .original_filename
                .clone()
                .or_else(|| Some(format!("import-{}", short_id(imp.id)))),
            description: imp.app_name.clone(),
            source_module: "importer".to_string(),
            original_id: imp.mobile_occurrence_id.clone(),
            relative_path: Some(imp.package_relative_path.clone()),
            sidecar_relative_path: None,
            hash_sha256: Some(imp.package_sha256.clone()),
            size_bytes: None,
            mime_type: Some("application/zip".to_string()),
            created_at: Some(imp.imported_at),
            updated_at: Some(imp.imported_at),
            status: Some(imp.status.as_str().to_string()),
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: imp.raw_manifest_json.clone(),
        });
    }

    // ------------------------------------------------------------------
    // Documentos (Documentoscopia) — um item por documento importado.
    let documents = documentoscopia_repo::list_documents(conn, occurrence_id)?;
    for d in &documents {
        let title = if d.title.trim().is_empty() {
            d.original_filename.clone()
        } else {
            d.title.clone()
        };
        out.push(EvidenceRegistryItem {
            id: format!("document:{}", d.id),
            occurrence_id: *occurrence_id,
            kind: EvidenceKind::Document,
            subtype: Some(d.doc_type.clone()),
            title: Some(title),
            description: Some(d.doc_type.clone()),
            source_module: "documentoscopia".to_string(),
            original_id: None,
            relative_path: Some(d.relative_path.clone()),
            sidecar_relative_path: None,
            hash_sha256: Some(d.sha256.clone()),
            size_bytes: Some(d.size_bytes),
            mime_type: Some(d.file_type.clone()),
            created_at: Some(d.created_at),
            updated_at: Some(d.updated_at),
            status: Some(d.status.clone()),
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: d.metadata_json.clone(),
        });
    }

    // ------------------------------------------------------------------
    // Fold evidence_links → linked_laudos_count
    //
    // For each registry item we count how many DISTINCT laudos cite it
    // (target_type=laudo). Counting per-laudo (rather than per-link)
    // avoids inflating when the perito inserts the same photo twice in
    // the same laudo.
    let links = evidence_link_repo::list_for_occurrence(conn, occurrence_id)?;
    let counts = laudo_counts_per_source(&links);
    for item in out.iter_mut() {
        if let Some(c) = counts.get(item.id.as_str()) {
            item.linked_laudos_count = *c;
        }
    }

    Ok(out)
}

pub fn build_summary(items: &[EvidenceRegistryItem]) -> RegistrySummary {
    let mut s = RegistrySummary::default();
    s.total_items = items.len() as u32;

    for item in items {
        match item.kind {
            EvidenceKind::Photo => s.photos += 1,
            EvidenceKind::Croqui => s.croquis += 1,
            EvidenceKind::CroquiExport => s.croqui_exports += 1,
            EvidenceKind::Video => s.videos += 1,
            EvidenceKind::VideoFrame => s.video_frames += 1,
            EvidenceKind::StoryboardFrame => s.storyboard_frames += 1,
            EvidenceKind::Laudo => s.laudos += 1,
            EvidenceKind::LaudoExport => s.laudo_exports += 1,
            EvidenceKind::ImportedPackage => s.imported_packages += 1,
            EvidenceKind::ImageAnalysis => s.image_analyses += 1,
            EvidenceKind::ImageExport => s.image_exports += 1,
            EvidenceKind::Audio => {}
            EvidenceKind::Document => {}
            EvidenceKind::Other => {}
        }
        if item.relative_path.is_some() {
            s.items_with_relative_path += 1;
        }
        if item.linked_laudos_count > 0 {
            s.linked_in_laudos += 1;
        }
        match item.integrity_status {
            IntegrityStatus::Ok => s.files_ok += 1,
            IntegrityStatus::MissingFile => s.files_missing += 1,
            IntegrityStatus::UnsafePath => s.unsafe_paths += 1,
            IntegrityStatus::BrokenLink => s.broken_links += 1,
            IntegrityStatus::HashMismatch => s.hash_mismatches += 1,
            _ => {}
        }
    }

    s.overall_status = s.aggregate_status().to_string();
    s
}

/// Build a `registry-id → distinct-laudo-count` map from the links
/// table. Each link has either a `media_asset_id`, `croqui_id`,
/// `video_storyboard_frame_id` or `video_media_hash` — we project that
/// back to the synthetic registry id used by [`build_registry`].
fn laudo_counts_per_source(
    links: &[crate::models::EvidenceLink],
) -> HashMap<String, u32> {
    use std::collections::HashSet;
    let mut bag: HashMap<String, HashSet<String>> = HashMap::new();
    for l in links {
        if l.target_type != "laudo" {
            continue;
        }
        let key = synthetic_id_for_link(l);
        let Some(k) = key else { continue };
        bag.entry(k).or_default().insert(l.target_id.clone());
    }
    bag.into_iter()
        .map(|(k, set)| (k, set.len() as u32))
        .collect()
}

fn synthetic_id_for_link(
    link: &crate::models::EvidenceLink,
) -> Option<String> {
    use crate::models::EvidenceSourceKind;
    match link.source_kind {
        EvidenceSourceKind::Photo => {
            link.media_asset_id.map(|u| format!("photo:{u}"))
        }
        EvidenceSourceKind::Croqui => {
            link.croqui_id.map(|u| format!("croqui_export:{u}"))
        }
        EvidenceSourceKind::VideoFrame
        | EvidenceSourceKind::VideoStoryboard => link
            .video_storyboard_frame_id
            .map(|u| format!("storyboard_frame:{u}")),
        _ => None,
    }
}

fn build_video_description(v: &crate::models::VideoMedia) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let (Some(w), Some(h)) = (v.width, v.height) {
        parts.push(format!("{w}×{h}"));
    }
    if let Some(d) = v.duration_s {
        parts.push(format!("{d:.1}s"));
    }
    if let Some(codec) = v.codec.as_ref() {
        parts.push(codec.clone());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

fn guess_video_mime(filename: &str) -> Option<String> {
    let ext = filename
        .rsplit('.')
        .next()
        .map(|s| s.to_ascii_lowercase())?;
    Some(
        match ext.as_str() {
            "mp4" | "m4v" => "video/mp4",
            "mov" => "video/quicktime",
            "mkv" => "video/x-matroska",
            "webm" => "video/webm",
            "avi" => "video/x-msvideo",
            _ => return None,
        }
        .to_string(),
    )
}

fn short_id(u: Uuid) -> String {
    u.to_string().chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{EvidenceLink, EvidenceSourceKind};
    use chrono::Utc;

    fn fixture_item(id: &str, kind: EvidenceKind) -> EvidenceRegistryItem {
        EvidenceRegistryItem {
            id: id.to_string(),
            occurrence_id: Uuid::nil(),
            kind,
            subtype: None,
            title: None,
            description: None,
            source_module: "test".to_string(),
            original_id: None,
            relative_path: None,
            sidecar_relative_path: None,
            hash_sha256: None,
            size_bytes: None,
            mime_type: None,
            created_at: None,
            updated_at: None,
            status: None,
            integrity_status: IntegrityStatus::Unknown,
            integrity_detail: None,
            linked_laudos_count: 0,
            metadata_json: "{}".to_string(),
        }
    }

    fn link(kind: EvidenceSourceKind, target_id: &str, src_uuid: Uuid) -> EvidenceLink {
        EvidenceLink {
            id: Uuid::new_v4(),
            occurrence_id: Uuid::nil(),
            target_type: "laudo".to_string(),
            target_id: target_id.to_string(),
            relation_type: "inserted_in_laudo".to_string(),
            source_kind: kind,
            media_asset_id: matches!(kind, EvidenceSourceKind::Photo)
                .then_some(src_uuid),
            croqui_id: matches!(kind, EvidenceSourceKind::Croqui).then_some(src_uuid),
            video_media_hash: None,
            video_event_id: None,
            video_storyboard_frame_id: matches!(
                kind,
                EvidenceSourceKind::VideoFrame | EvidenceSourceKind::VideoStoryboard
            )
            .then_some(src_uuid),
            field_note_id: None,
            relative_path: None,
            source_hash: None,
            metadata_json: "{}".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn summary_counts_kinds() {
        let items = vec![
            fixture_item("photo:a", EvidenceKind::Photo),
            fixture_item("photo:b", EvidenceKind::Photo),
            fixture_item("croqui:c", EvidenceKind::Croqui),
            fixture_item("video:v", EvidenceKind::Video),
        ];
        let s = build_summary(&items);
        assert_eq!(s.photos, 2);
        assert_eq!(s.croquis, 1);
        assert_eq!(s.videos, 1);
        assert_eq!(s.total_items, 4);
        assert_eq!(s.overall_status, "ok");
    }

    #[test]
    fn summary_aggregates_status_warning_on_missing() {
        let mut item = fixture_item("photo:a", EvidenceKind::Photo);
        item.integrity_status = IntegrityStatus::MissingFile;
        let s = build_summary(&[item]);
        assert_eq!(s.files_missing, 1);
        assert_eq!(s.overall_status, "warning");
    }

    #[test]
    fn summary_aggregates_status_critical_on_unsafe_path() {
        let mut item = fixture_item("photo:a", EvidenceKind::Photo);
        item.integrity_status = IntegrityStatus::UnsafePath;
        let s = build_summary(&[item]);
        assert_eq!(s.unsafe_paths, 1);
        assert_eq!(s.overall_status, "critical");
    }

    #[test]
    fn link_counter_groups_by_distinct_laudo() {
        let media_uuid = Uuid::new_v4();
        let links = vec![
            link(EvidenceSourceKind::Photo, "laudo-1", media_uuid),
            link(EvidenceSourceKind::Photo, "laudo-1", media_uuid),
            link(EvidenceSourceKind::Photo, "laudo-2", media_uuid),
        ];
        let counts = laudo_counts_per_source(&links);
        // Two distinct laudos cite the same photo → count == 2 even
        // though there are three rows.
        assert_eq!(
            counts.get(&format!("photo:{}", media_uuid)).copied(),
            Some(2)
        );
    }

    #[test]
    fn link_counter_handles_croqui_export_synthetic_id() {
        let croqui_uuid = Uuid::new_v4();
        let links = vec![link(EvidenceSourceKind::Croqui, "laudo-1", croqui_uuid)];
        let counts = laudo_counts_per_source(&links);
        // Note the synthetic id for croqui evidence is the EXPORT (PNG),
        // not the .sicrocroqui — what actually ends up in the laudo.
        assert_eq!(
            counts.get(&format!("croqui_export:{}", croqui_uuid)).copied(),
            Some(1)
        );
    }
}
