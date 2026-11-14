//! Backup do workspace: ZIP (deflate) `.sicrobackup` com manifesto JSON embutido.
//! O workspace original nunca é modificado; `Sistema/cache`, `Sistema/logs` e
//! `Exportações/Backups` ficam de fora (e os equivalentes do layout antigo);
//! o SHA-256 do arquivo final é calculado depois da escrita.

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
use crate::workspace::paths::{PASTAS_FORA_DO_BACKUP, dir_backups, fora_do_backup};


/// Devolvido por `create_backup`.
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

/// Gera o backup em `backups/` do próprio workspace (ou em `dest_dir`).
/// `bo_hint` entra no nome do arquivo; vazio → "ocorrencia".
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
        .unwrap_or_else(|| dir_backups(workspace_root));
    std::fs::create_dir_all(&dest_root).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create backup dir {}: {}",
            dest_root.display(),
            e
        ))
    })?;
    let absolute_path = dest_root.join(&filename);

    let file = File::create(&absolute_path).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot create backup file {}: {}",
            absolute_path.display(),
            e
        ))
    })?;
    let mut zip = zip::ZipWriter::new(BufWriter::new(file));
    let options = opcoes_zip();

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
        "skipped_dirs": PASTAS_FORA_DO_BACKUP,
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

    let meta = std::fs::metadata(&absolute_path)?;
    let size_bytes = meta.len();
    let hash = hash_file(&absolute_path)?;

    // Relativo ao workspace quando `dest_dir` está dentro dele; senão, absoluto.
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

/// ZIP64 em toda entrada: sem isso o zip recusa arquivo de mais de 4 GiB
/// (exportação sem perda, vídeo de DVR).
fn opcoes_zip() -> FileOptions {
    FileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644)
        .large_file(true)
}

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

        // Pastas efêmeras e a própria pasta de backups.
        if file_type.is_dir() && fora_do_backup(&rel_de(root, &path)) {
            continue;
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
        // Symlinks e outros tipos são ignorados.
    }
    Ok(())
}

/// Relativo à raiz, com `/`.
pub(crate) fn rel_de(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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

    /// Caso mínimo para `Manifest::read` funcionar; `legacy` = layout antigo.
    fn fake_workspace(dir: &Path, legacy: bool) -> Result<()> {
        let (sis, img, lau, bkp) = if legacy {
            ("", "imagens/originais", "laudos", "backups")
        } else {
            ("Sistema/", "Mídias/Imagens", "Sistema/laudos", "Exportações/Backups")
        };
        for sub in [img, lau, bkp] {
            fs::create_dir_all(dir.join(sub))?;
        }
        fs::create_dir_all(dir.join(format!("{sis}logs")))?; // should be skipped
        fs::create_dir_all(dir.join(format!("{sis}cache")))?; // should be skipped
        fs::write(
            dir.join(format!("{sis}manifest.json")),
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
        fs::write(dir.join(format!("{sis}sicro.sqlite")), b"FAKE-DATABASE-BYTES")?;
        fs::write(dir.join(lau).join("laudo-1.sicrodoc"), br#"{"hello":"world"}"#)?;
        fs::write(
            dir.join(img).join("img.png"),
            &[137, 80, 78, 71, 0xD, 0xA, 0x1A, 0xA, 1, 2, 3],
        )?;
        // Dentro das pastas puladas:
        fs::write(dir.join(format!("{sis}logs/app.log")), b"should be skipped")?;
        fs::write(dir.join(format!("{sis}cache/scratch.bin")), b"should be skipped")?;
        fs::write(dir.join(bkp).join("velho.sicrobackup"), b"should be skipped")?;
        Ok(())
    }

    fn nomes_do_zip(p: &Path) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(File::open(p).unwrap()).unwrap();
        (0..archive.len())
            .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
            .collect()
    }

    #[test]
    fn backup_zips_workspace_and_excludes_logs_cache() {
        let tmp = TempDir::new().unwrap();
        fake_workspace(tmp.path(), false).unwrap();

        let artifact = create_backup(tmp.path(), Some(&tmp.path().join("Exportações/Backups")), Some("BO-12-2026"))
            .expect("backup ok");
        assert!(artifact.absolute_path.is_file());
        assert!(artifact.filename.starts_with("backup_BO-12-2026_"));
        assert!(artifact.filename.ends_with(".sicrobackup"));
        assert!(artifact.size_bytes > 0);
        assert_eq!(artifact.hash_sha256.len(), 64);
        // manifest.json + sicro.sqlite + 2 aninhados; logs, cache e backups ficam de fora.
        assert_eq!(artifact.file_count, 4, "file_count was {}", artifact.file_count);

        let names = nomes_do_zip(&artifact.absolute_path);
        assert!(names.contains(&"Sistema/manifest.json".to_string()));
        assert!(names.contains(&"Sistema/sicro.sqlite".to_string()));
        assert!(names.contains(&"Sistema/laudos/laudo-1.sicrodoc".to_string()));
        assert!(names.contains(&"Mídias/Imagens/img.png".to_string()));
        assert!(names.contains(&"_sicro_backup_manifest.json".to_string()));
        assert!(!names.iter().any(|n| n.contains("logs/")));
        assert!(!names.iter().any(|n| n.contains("cache/")));
        assert!(!names.iter().any(|n| n.contains("Backups/")));
    }

    #[test]
    fn backup_de_caso_antigo_tambem_pula_cache_e_logs() {
        let tmp = TempDir::new().unwrap();
        fake_workspace(tmp.path(), true).unwrap();
        let a = create_backup(tmp.path(), Some(&tmp.path().join("backups")), None).unwrap();
        let names = nomes_do_zip(&a.absolute_path);
        assert!(names.contains(&"manifest.json".to_string()));
        assert!(names.contains(&"imagens/originais/img.png".to_string()));
        assert!(!names.iter().any(|n| n.starts_with("logs/") || n.starts_with("cache/") || n.starts_with("backups/")));
    }

    #[test]
    fn backup_filename_falls_back_when_no_hint() {
        let tmp = TempDir::new().unwrap();
        fake_workspace(tmp.path(), false).unwrap();
        let a = create_backup(tmp.path(), None, None).unwrap();
        assert!(a.filename.starts_with("backup_ocorrencia_"));
    }

    #[test]
    fn opcoes_do_zip_usam_zip64() {
        // Cabeçalho local de entrada ZIP64: tamanhos 0xFFFFFFFF e extra de 20 bytes.
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        zip.start_file("a", opcoes_zip()).unwrap();
        zip.write_all(b"x").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert_eq!(&bytes[0..4], b"PK\x03\x04");
        assert_eq!(&bytes[18..26], &[0xFF; 8], "tamanhos no extra ZIP64");
        assert_eq!(u16::from_le_bytes([bytes[28], bytes[29]]), 20);
    }

    /// Prova de verdade do ZIP64 (sem ele falha em 4 GiB). Ignorado: ~80 s em debug
    /// (deflate de 5 GiB). `cargo test --lib backup_com_arquivo_de_5_gib -- --ignored`.
    #[test]
    #[ignore = "lento: ~80 s em debug"]
    fn backup_com_arquivo_de_5_gib() {
        // Arquivo esparso: 5 GiB lógicos, quase nada em disco; o zip de zeros fica com poucos MB.
        const TAM: u64 = 5 << 30;
        let tmp = TempDir::new().unwrap();
        fake_workspace(tmp.path(), false).unwrap();
        let grande = tmp.path().join("Mídias/Vídeos/dvr.dav");
        fs::create_dir_all(grande.parent().unwrap()).unwrap();
        File::create(&grande).unwrap().set_len(TAM).unwrap();

        let a = create_backup(tmp.path(), Some(&tmp.path().join("Exportações/Backups")), None)
            .expect("backup com arquivo > 4 GiB");
        assert_eq!(a.file_count, 5);
        let mut zip = zip::ZipArchive::new(File::open(&a.absolute_path).unwrap()).unwrap();
        assert_eq!(zip.by_name("Mídias/Vídeos/dvr.dav").unwrap().size(), TAM);
        assert!(zip.by_name("Sistema/laudos/laudo-1.sicrodoc").is_ok());
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
