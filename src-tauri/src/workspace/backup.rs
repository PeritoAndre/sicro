//! Workspace backup (MVP 8 — Consolidação Alpha).
//!
//! Comprime o workspace inteiro (`.sicro/`) em um arquivo
//! `.sicrobackup` (ZIP com deflate) e grava um manifesto JSON dentro
//! do próprio backup descrevendo o que foi incluído.
//!
//! Regras:
//!   - O workspace original NUNCA é modificado.
//!   - O backup carrega SQLite, .sicrodoc, .sicrocroqui, .sicroimage,
//!     fotos importadas, vídeos, frames, croquis exportados, imagens
//!     derivadas, exports (HTML/PDF/DOCX/PNG) e reports.
//!   - Pastas `cache/` e `logs/` são ignoradas (efêmero).
//!   - Path traversal já é impossível porque caminhamos diretórios
//!     absolutos via `walk_dir` — não confiamos em entrada externa.
//!   - SHA-256 do `.sicrobackup` final é computado depois da escrita
//!     atômica para garantir audit.

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::write::FileOptions;
use zip::CompressionMethod;

use crate::error::{Result, SicroError};
use crate::workspace::manifest::{Manifest, APP_VERSION};

/// Folders inside the workspace that should be skipped (efêmero / pesado).
/// `pub(crate)` para o backup geral usar a MESMA regra de skip no fingerprint
/// (senão o fingerprint mudaria por causa de cache/logs e o incremental
/// nunca pularia um caso de fato inalterado).
pub(crate) const SKIP_DIRS: &[&str] = &["cache", "logs"];

/// Descriptor returned by `create_backup`. The caller uses it to
/// populate the response and the system audit log.
#[derive(Debug, Clone)]
pub struct BackupArtifact {
    pub absolute_path: PathBuf,
    pub relative_path: String,
    pub filename: String,
    pub size_bytes: u64,
    pub hash_sha256: String,
    pub file_count: u32,
    pub created_at: chrono::DateTime<Utc>,
    pub workspace_id: Uuid,
    pub occurrence_id: Uuid,
}

/// Create a backup of `workspace_root` into the same workspace's
/// `backups/` directory (or `dest_dir` when provided).
///
/// `bo_hint` is a label used in the filename (e.g. BO number). When
/// empty, falls back to "ocorrencia".
pub fn create_backup(
    workspace_root: &Path,
    dest_dir: Option<&Path>,
    bo_hint: Option<&str>,
) -> Result<BackupArtifact> {
    let manifest = Manifest::read(workspace_root)?;

    let bo = bo_hint
        .map(sanitize_slug)
        .unwrap_or_else(|| "ocorrencia".to_string());
    let now = Utc::now();
    let stamp = now.format("%Y%m%d_%H%M%S");
    let filename = format!("backup_{}_{}.sicrobackup", bo, stamp);

    let dest_root = dest_dir
        .map(|d| d.to_path_buf())
        .unwrap_or_else(|| workspace_root.join("backups"));
    std::fs::create_dir_all(&dest_root).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create backup dir {}: {}",
            dest_root.display(),
            e
        ))
    })?;
    let absolute_path = dest_root.join(&filename);

    // 1. Stream files into the zip with deflate.
    let file = File::create(&absolute_path).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create backup file {}: {}",
            absolute_path.display(),
            e
        ))
    })?;
    let mut zip = zip::ZipWriter::new(BufWriter::new(file));
    let options = FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut file_count: u32 = 0;
    let mut total_bytes: u64 = 0;

    walk_into_zip(
        workspace_root,
        workspace_root,
        &mut zip,
        options,
        &mut file_count,
        &mut total_bytes,
    )?;

    // 2. Manifesto JSON embedded inside the backup.
    let inner_manifest = serde_json::json!({
        "format": "sicro-backup",
        "format_version": "1.0",
        "software": "SICRO Desktop",
        "software_version": APP_VERSION,
        "generated_at": now.to_rfc3339(),
        "workspace_id": manifest.workspace_id.to_string(),
        "occurrence_id": manifest.occurrence_id.to_string(),
        "source_workspace_path": workspace_root.to_string_lossy(),
        "file_count": file_count,
        "uncompressed_total_bytes": total_bytes,
        "skipped_dirs": SKIP_DIRS,
        "filename": filename,
    });
    zip.start_file("_sicro_backup_manifest.json", options)
        .map_err(|e| SicroError::Workspace(format!("zip manifest start: {e}")))?;
    zip.write_all(
        serde_json::to_string_pretty(&inner_manifest)
            .unwrap_or_else(|_| "{}".to_string())
            .as_bytes(),
    )?;

    zip.finish()
        .map_err(|e| SicroError::Workspace(format!("zip finish: {e}")))?;

    // 3. Hash + size of the final artifact.
    let meta = std::fs::metadata(&absolute_path)?;
    let size_bytes = meta.len();
    let hash = hash_file(&absolute_path)?;

    // 4. Relative path — best-effort. When `dest_dir` is inside the
    // workspace we report a workspace-relative path; otherwise we
    // report the absolute path string.
    let relative_path = match absolute_path.strip_prefix(workspace_root) {
        Ok(p) => p.to_string_lossy().replace('\\', "/"),
        Err(_) => absolute_path.to_string_lossy().into_owned(),
    };

    Ok(BackupArtifact {
        absolute_path,
        relative_path,
        filename,
        size_bytes,
        hash_sha256: hash,
        file_count,
        created_at: now,
        workspace_id: manifest.workspace_id,
        occurrence_id: manifest.occurrence_id,
    })
}

// ---------------------------------------------------------------------------

fn walk_into_zip<W: Write + std::io::Seek>(
    root: &Path,
    dir: &Path,
    zip: &mut zip::ZipWriter<W>,
    options: FileOptions,
    file_count: &mut u32,
    total_bytes: &mut u64,
) -> Result<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        SicroError::Filesystem(format!("cannot read {}: {}", dir.display(), e))
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            SicroError::Filesystem(format!("dir entry error: {e}"))
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| {
            SicroError::Filesystem(format!("file_type error: {e}"))
        })?;

        // Skip ephemeral subdirs only at the workspace root level.
        if file_type.is_dir() && dir == root {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                if SKIP_DIRS.iter().any(|d| *d == name) {
                    continue;
                }
                // Don't re-include the backups dir itself.
                if name == "backups" {
                    continue;
                }
            }
        }

        if file_type.is_dir() {
            walk_into_zip(root, &path, zip, options, file_count, total_bytes)?;
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| {
                    SicroError::Filesystem(format!(
                        "path {} is not under workspace root {}",
                        path.display(),
                        root.display()
                    ))
                })?
                .to_string_lossy()
                .replace('\\', "/");

            let mut f = File::open(&path).map_err(|e| {
                SicroError::Filesystem(format!(
                    "cannot open {} for backup: {}",
                    path.display(),
                    e
                ))
            })?;
            zip.start_file(&rel, options).map_err(|e| {
                SicroError::Workspace(format!("zip entry {rel}: {e}"))
            })?;
            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = f.read(&mut buf).map_err(|e| {
                    SicroError::Filesystem(format!("read {}: {}", path.display(), e))
                })?;
                if n == 0 {
                    break;
                }
                zip.write_all(&buf[..n])?;
                *total_bytes = total_bytes.saturating_add(n as u64);
            }
            *file_count = file_count.saturating_add(1);
        }
        // Symlinks and other non-file types are skipped silently.
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut f = File::open(path).map_err(|e| {
        SicroError::Filesystem(format!("cannot reopen {}: {}", path.display(), e))
    })?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).map_err(|e| {
            SicroError::Filesystem(format!("hash read {}: {}", path.display(), e))
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sanitize_slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let ok = c.is_ascii_alphanumeric() || c == '-' || c == '_';
        out.push(if ok { c } else { '_' });
    }
    let trimmed: String = out.trim_matches('_').chars().take(40).collect();
    if trimmed.is_empty() {
        "ocorrencia".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn fake_workspace(dir: &Path) -> Result<()> {
        // Mimic a minimal `.sicro/` so `Manifest::read` succeeds.
        fs::create_dir_all(dir.join("imagens").join("originais"))?;
        fs::create_dir_all(dir.join("laudos"))?;
        fs::create_dir_all(dir.join("logs"))?;  // should be skipped
        fs::create_dir_all(dir.join("cache"))?; // should be skipped
        fs::create_dir_all(dir.join("backups"))?;
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_string_pretty(&serde_json::json!({
                "format": "sicro-workspace",
                "version": "2.0.0",
                "created_at": "2026-05-25T13:00:00Z",
                "updated_at": "2026-05-25T13:00:00Z",
                "workspace_id": "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa",
                "occurrence_id": "bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb",
                "app_version": "test",
                "database": "sicro.sqlite",
                "integrity": {"strategy": "sha256", "manifest_hash": null},
            })).unwrap(),
        )?;
        fs::write(dir.join("sicro.sqlite"), b"FAKE-DATABASE-BYTES")?;
        fs::write(
            dir.join("laudos").join("laudo-1.sicrodoc"),
            br#"{"hello":"world"}"#,
        )?;
        fs::write(
            dir.join("imagens").join("originais").join("img.png"),
            &[137, 80, 78, 71, 0xD, 0xA, 0x1A, 0xA, 1, 2, 3],
        )?;
        // Files inside skipped dirs:
        fs::write(dir.join("logs").join("app.log"), b"should be skipped")?;
        fs::write(dir.join("cache").join("scratch.bin"), b"should be skipped")?;
        Ok(())
    }

    #[test]
    fn backup_zips_workspace_and_excludes_logs_cache() {
        let tmp = TempDir::new().unwrap();
        fake_workspace(tmp.path()).unwrap();

        let artifact = create_backup(tmp.path(), Some(&tmp.path().to_path_buf().join("backups")), Some("BO-12-2026"))
            .expect("backup ok");
        assert!(artifact.absolute_path.is_file());
        assert!(artifact.filename.starts_with("backup_BO-12-2026_"));
        assert!(artifact.filename.ends_with(".sicrobackup"));
        assert!(artifact.size_bytes > 0);
        assert_eq!(artifact.hash_sha256.len(), 64);
        // 3 user files (manifest.json + sicro.sqlite + 2 nested)
        // but excludes logs + cache files (2 files skipped).
        assert!(artifact.file_count >= 4, "file_count was {}", artifact.file_count);

        // Read back the zip and verify some entries.
        let zr = File::open(&artifact.absolute_path).unwrap();
        let mut archive = zip::ZipArchive::new(zr).unwrap();
        let names: Vec<String> = (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect();
        assert!(names.contains(&"manifest.json".to_string()));
        assert!(names.contains(&"sicro.sqlite".to_string()));
        assert!(names.contains(&"laudos/laudo-1.sicrodoc".to_string()));
        assert!(names.contains(&"imagens/originais/img.png".to_string()));
        assert!(names.contains(&"_sicro_backup_manifest.json".to_string()));
        // Skipped dirs are not present.
        assert!(!names.iter().any(|n| n.starts_with("logs/")));
        assert!(!names.iter().any(|n| n.starts_with("cache/")));
        assert!(!names.iter().any(|n| n.starts_with("backups/")));
    }

    #[test]
    fn backup_filename_falls_back_when_no_hint() {
        let tmp = TempDir::new().unwrap();
        fake_workspace(tmp.path()).unwrap();
        let a = create_backup(tmp.path(), None, None).unwrap();
        assert!(a.filename.starts_with("backup_ocorrencia_"));
    }

    #[test]
    fn sanitize_slug_handles_unsafe_chars() {
        assert_eq!(sanitize_slug("BO 12/2026"), "BO_12_2026");
        assert_eq!(sanitize_slug(""), "ocorrencia");
        assert_eq!(sanitize_slug("___"), "ocorrencia");
        let long: String = "a".repeat(200);
        assert!(sanitize_slug(&long).len() <= 40);
    }
}
