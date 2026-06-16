//! Integrity verifier (MVP 5).
//!
//! Operates in two modes:
//!   - **lightweight**: for each item with a `relative_path`, probe
//!     existence via `probe_workspace_relative`. Optionally check the
//!     sidecar JSON (currently only `storyboard_frame` has one).
//!   - **deep**: in addition, recompute SHA-256 for items that store a
//!     hash and compare. Heavy — only runs when the caller asks for it.
//!
//! Failure to read or hash never crashes the verifier — we record the
//! item as `Unknown` or `HashMismatch` with a `integrity_detail`
//! message and move on. This matches the briefing's "não travar se
//! houver erro".

use std::path::Path;

use rusqlite::Connection;
use uuid::Uuid;

use crate::error::Result;
use crate::filesystem::{probe_workspace_relative, RelativeResolution};
use crate::hashing::sha256::sha256_file;
use crate::models::{
    EvidenceRegistryItem, IntegrityStatus, RegistrySummary, VerifyOptions,
    WorkspaceIntegrityReport,
};
use crate::workspace::manifest::APP_VERSION;

use super::aggregator::{build_registry, build_summary};
use super::broken_links::detect_broken_laudo_links;

/// Build the registry, run the integrity check, count summaries and
/// detect broken laudo links — return everything bundled for the UI.
pub fn verify_workspace(
    conn: &Connection,
    workspace_root: &Path,
    occurrence_id: &Uuid,
    options: &VerifyOptions,
) -> Result<WorkspaceIntegrityReport> {
    let mut items = build_registry(conn, occurrence_id)?;
    let mut warnings: Vec<String> = Vec::new();

    for item in items.iter_mut() {
        let (status, detail) =
            verify_one(workspace_root, item, options.deep);
        item.integrity_status = status;
        item.integrity_detail = detail;
    }

    let broken_links = match detect_broken_laudo_links(workspace_root, &items) {
        Ok(b) => b,
        Err(e) => {
            warnings.push(format!(
                "detector de links quebrados falhou: {}",
                e
            ));
            Vec::new()
        }
    };

    let mut summary: RegistrySummary = build_summary(&items);
    // Roll the broken laudo links into the summary so the "Resumo"
    // tab shows the actual number instead of only the per-item flag.
    let extra_broken_links = broken_links
        .iter()
        .filter(|b| {
            matches!(
                b.status,
                IntegrityStatus::BrokenLink
                    | IntegrityStatus::MissingFile
                    | IntegrityStatus::UnsafePath
            )
        })
        .count() as u32;
    summary.broken_links = summary.broken_links.saturating_add(extra_broken_links);
    summary.overall_status = summary.aggregate_status().to_string();

    Ok(WorkspaceIntegrityReport {
        occurrence_id: *occurrence_id,
        workspace_path: workspace_root.to_string_lossy().into_owned(),
        generated_at: chrono::Utc::now(),
        app_version: APP_VERSION.to_string(),
        summary,
        items,
        broken_laudo_links: broken_links,
        warnings,
        deep_check_executed: options.deep,
    })
}

/// Resolve the verification verdict for a single item.
///
/// Items without a `relative_path` are left as `Unknown` — they may be
/// "pure database" items (e.g. dossiê fields are not in scope of
/// MVP 5).
pub fn verify_one(
    workspace_root: &Path,
    item: &EvidenceRegistryItem,
    deep: bool,
) -> (IntegrityStatus, Option<String>) {
    // 1. Probe the primary path.
    let probe = probe_workspace_relative(
        workspace_root,
        item.relative_path.as_deref(),
    );
    let (mut status, mut detail): (IntegrityStatus, Option<String>) = match probe {
        RelativeResolution::Ok { .. } => (IntegrityStatus::Ok, None),
        RelativeResolution::Missing { absolute } => (
            IntegrityStatus::MissingFile,
            Some(format!("não existe em {}", absolute.display())),
        ),
        RelativeResolution::Unsafe { reason } => (
            IntegrityStatus::UnsafePath,
            Some(reason),
        ),
        RelativeResolution::Empty => (IntegrityStatus::Unknown, None),
    };

    // 2. Sidecar check, when expected.
    if matches!(status, IntegrityStatus::Ok) {
        if let Some(sidecar) = item.sidecar_relative_path.as_deref() {
            let sidecar_probe =
                probe_workspace_relative(workspace_root, Some(sidecar));
            if !matches!(sidecar_probe, RelativeResolution::Ok { .. }) {
                status = IntegrityStatus::MissingSidecar;
                detail = Some(format!(
                    "sidecar ausente: {}",
                    sidecar
                ));
            }
        }
    }

    // 3. Deep hash check.
    if deep && matches!(status, IntegrityStatus::Ok) {
        if let (Some(expected_hash), Some(rel)) =
            (item.hash_sha256.as_ref(), item.relative_path.as_ref())
        {
            let abs = workspace_root.join(rel.replace(
                '/',
                std::path::MAIN_SEPARATOR.encode_utf8(&mut [0; 4]),
            ));
            match sha256_file(&abs) {
                Ok(actual) if actual.eq_ignore_ascii_case(expected_hash) => {}
                Ok(actual) => {
                    status = IntegrityStatus::HashMismatch;
                    detail = Some(format!(
                        "hash divergente — esperado {}, calculado {}",
                        short_hash(expected_hash),
                        short_hash(&actual),
                    ));
                }
                Err(e) => {
                    status = IntegrityStatus::HashMismatch;
                    detail = Some(format!(
                        "falha ao calcular hash: {e}"
                    ));
                }
            }
        }
    }

    (status, detail)
}

fn short_hash(h: &str) -> String {
    if h.len() <= 12 {
        h.to_string()
    } else {
        format!("{}…", &h[..12])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::EvidenceKind;
    use std::fs;
    use tempfile::TempDir;

    fn item_with_path(rel: Option<&str>) -> EvidenceRegistryItem {
        EvidenceRegistryItem {
            id: "test:1".to_string(),
            occurrence_id: Uuid::nil(),
            kind: EvidenceKind::Photo,
            subtype: None,
            title: None,
            description: None,
            source_module: "test".to_string(),
            original_id: None,
            relative_path: rel.map(String::from),
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

    #[test]
    fn ok_when_file_exists() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("imports").join("photos");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("a.jpg"), b"x").unwrap();
        let item = item_with_path(Some("imports/photos/a.jpg"));
        let (s, _) = verify_one(tmp.path(), &item, false);
        assert_eq!(s, IntegrityStatus::Ok);
    }

    #[test]
    fn missing_when_file_absent() {
        let tmp = TempDir::new().unwrap();
        let item = item_with_path(Some("does/not/exist.png"));
        let (s, _) = verify_one(tmp.path(), &item, false);
        assert_eq!(s, IntegrityStatus::MissingFile);
    }

    #[test]
    fn unsafe_when_path_escapes() {
        let tmp = TempDir::new().unwrap();
        let item = item_with_path(Some("../etc/passwd"));
        let (s, _) = verify_one(tmp.path(), &item, false);
        assert_eq!(s, IntegrityStatus::UnsafePath);
    }

    #[test]
    fn unknown_when_no_path() {
        let tmp = TempDir::new().unwrap();
        let item = item_with_path(None);
        let (s, _) = verify_one(tmp.path(), &item, false);
        assert_eq!(s, IntegrityStatus::Unknown);
    }

    #[test]
    fn missing_sidecar_flags_when_sidecar_absent() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("videos").join("frames");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("f.png"), b"x").unwrap();
        let mut item = item_with_path(Some("videos/frames/f.png"));
        item.sidecar_relative_path =
            Some("videos/frames/f.json".to_string()); // does NOT exist
        let (s, _) = verify_one(tmp.path(), &item, false);
        assert_eq!(s, IntegrityStatus::MissingSidecar);
    }

    #[test]
    fn deep_hash_mismatch_detected() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("p");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("f.bin"), b"hello").unwrap();
        let mut item = item_with_path(Some("p/f.bin"));
        item.hash_sha256 = Some("deadbeef".repeat(8));
        let (s, _) = verify_one(tmp.path(), &item, true);
        assert_eq!(s, IntegrityStatus::HashMismatch);
    }

    #[test]
    fn deep_hash_match_keeps_ok() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("p");
        fs::create_dir_all(&sub).unwrap();
        let bytes = b"hello";
        fs::write(sub.join("f.bin"), bytes).unwrap();
        let mut item = item_with_path(Some("p/f.bin"));
        // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        item.hash_sha256 = Some(
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string(),
        );
        let (s, _) = verify_one(tmp.path(), &item, true);
        assert_eq!(s, IntegrityStatus::Ok);
    }
}
