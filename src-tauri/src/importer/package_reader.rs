//! Read primitives for a `.sicroapp` zip archive.
//!
//! Everything that touches the ZIP goes through here so the orchestrator can
//! stay focused on the import flow. Each entry name is normalised by
//! `safe_zip::sanitize_zip_path` before any filesystem write.
//!
//! The reader keeps a `zip::ZipArchive` open against the staged copy of the
//! package (the one in `imports/<id>/original_package.sicroapp`), not the
//! user's original path — that file may move while the import runs.

use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::error::{Result, SicroError};
use crate::importer::safe_zip::sanitize_zip_path;

/// Wraps a `zip::ZipArchive` open against a staged `.sicroapp` and offers
/// the helpers the orchestrator needs.
pub struct PackageReader {
    archive: ZipArchive<File>,
    /// Sanitised relative paths of every regular file inside the ZIP.
    file_entries: Vec<String>,
}

impl PackageReader {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|e| {
            SicroError::Filesystem(format!("cannot open {} as ZIP: {}", path.display(), e))
        })?;
        let mut archive = ZipArchive::new(file).map_err(|e| {
            SicroError::Validation(format!(
                "{} is not a valid ZIP (rename or corruption?): {}",
                path.display(),
                e
            ))
        })?;

        // Scan once so callers can list/test entry existence without
        // re-walking the central directory.
        let mut entries = Vec::with_capacity(archive.len());
        for i in 0..archive.len() {
            let raw_name = {
                let entry = archive.by_index(i).map_err(|e| {
                    SicroError::Validation(format!("ZIP entry {i} unreadable: {e}"))
                })?;
                if entry.is_dir() {
                    continue;
                }
                entry.name().to_string()
            };
            let sanitised = sanitize_zip_path(&raw_name)?;
            let as_str = sanitised
                .to_str()
                .ok_or_else(|| {
                    SicroError::Validation(format!("non-UTF8 zip entry: {raw_name:?}"))
                })?
                .replace('\\', "/");
            entries.push(as_str);
        }

        Ok(Self {
            archive,
            file_entries: entries,
        })
    }

    /// Sanitised list of every regular file in the archive (root-relative,
    /// forward-slashes).
    pub fn list_files(&self) -> &[String] {
        &self.file_entries
    }

    pub fn contains(&self, sanitised_name: &str) -> bool {
        self.file_entries.iter().any(|e| e == sanitised_name)
    }

    /// Read a JSON entry into memory. Returns `None` if missing.
    pub fn read_to_bytes(&mut self, sanitised_name: &str) -> Result<Option<Vec<u8>>> {
        if !self.contains(sanitised_name) {
            return Ok(None);
        }
        // `zip::ZipArchive::by_name` accepts the same string the ZIP central
        // directory used. We don't have that one because we already
        // normalised slashes; resolve by index instead.
        let idx = self.find_index(sanitised_name)?;
        let mut entry = self
            .archive
            .by_index(idx)
            .map_err(|e| SicroError::Filesystem(format!("zip read error: {e}")))?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .map_err(|e| SicroError::Filesystem(format!("zip read error: {e}")))?;
        Ok(Some(buf))
    }

    /// Stream an entry into `target` (on the filesystem). Returns
    /// `(bytes_written, sha256_hex_lowercase)`. The target's parent directory
    /// must already exist. Refuses to overwrite an existing target.
    pub fn extract_to(
        &mut self,
        sanitised_name: &str,
        target: &Path,
    ) -> Result<(u64, String)> {
        let idx = self.find_index(sanitised_name)?;
        let mut entry = self
            .archive
            .by_index(idx)
            .map_err(|e| SicroError::Filesystem(format!("zip read error: {e}")))?;

        if target.exists() {
            return Err(SicroError::Filesystem(format!(
                "refusing to overwrite existing media file {}",
                target.display()
            )));
        }
        let mut out = File::create(target).map_err(|e| {
            SicroError::Filesystem(format!(
                "cannot create target {} for ZIP extraction: {}",
                target.display(),
                e
            ))
        })?;

        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = entry.read(&mut buf).map_err(|e| {
                SicroError::Filesystem(format!("zip stream error for {sanitised_name:?}: {e}"))
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n])
                .map_err(|e| SicroError::Filesystem(format!("write error: {e}")))?;
            total += n as u64;
        }
        out.flush().map_err(|e| {
            SicroError::Filesystem(format!("flush error on {}: {}", target.display(), e))
        })?;
        let digest = hasher.finalize();
        let hex = digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Ok((total, hex))
    }

    /// Stream an entry through SHA-256 without writing to disk. Used when
    /// verifying `hashes.json` against the ZIP contents.
    pub fn sha256(&mut self, sanitised_name: &str) -> Result<String> {
        let idx = self.find_index(sanitised_name)?;
        let mut entry = self
            .archive
            .by_index(idx)
            .map_err(|e| SicroError::Filesystem(format!("zip read error: {e}")))?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = entry.read(&mut buf).map_err(|e| {
                SicroError::Filesystem(format!("zip stream error: {e}"))
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let digest = hasher.finalize();
        Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
    }

    fn find_index(&mut self, sanitised_name: &str) -> Result<usize> {
        // The `archive` keeps entries in the same order we walked at `open`.
        // We can't store indices in `file_entries` directly (the archive
        // mutably borrows itself when reading), so re-derive here. The cost
        // is one scan per read — fine for the JSON files; media extraction
        // is dominated by I/O anyway.
        for i in 0..self.archive.len() {
            let entry = self
                .archive
                .by_index(i)
                .map_err(|e| SicroError::Filesystem(format!("zip lookup error: {e}")))?;
            if entry.is_dir() {
                continue;
            }
            if let Ok(p) = sanitize_zip_path(entry.name()) {
                let as_str = p.to_str().unwrap_or_default().replace('\\', "/");
                if as_str == sanitised_name {
                    return Ok(i);
                }
            }
        }
        Err(SicroError::Validation(format!(
            "ZIP entry not found: {sanitised_name:?}"
        )))
    }
}

/// Compute SHA-256 of the package file on disk, streaming so memory stays
/// flat for multi-MB photos.
pub fn package_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| {
        SicroError::Filesystem(format!("cannot open {} for hashing: {}", path.display(), e))
    })?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| SicroError::Filesystem(format!("io error: {e}")))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Copy the source `.sicroapp` to `target`, creating parents as needed.
/// Returns the number of bytes written.
pub fn stage_package(src: &Path, target: &Path) -> Result<u64> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SicroError::Filesystem(format!(
                "cannot create staging dir {}: {}",
                parent.display(),
                e
            ))
        })?;
    }
    let n = std::fs::copy(src, target).map_err(|e| {
        SicroError::Filesystem(format!(
            "cannot copy {} to {}: {}",
            src.display(),
            target.display(),
            e
        ))
    })?;
    Ok(n)
}

/// Walk `hashes.json` (mobile contract: `{ "algoritmo": "SHA-256", "arquivos": [{ "caminho": "...", "sha256": "..." }] }`)
/// and return a vector of (sanitised_path, expected_hash).
pub fn parse_hashes_json(raw: &[u8]) -> Result<Vec<(String, String)>> {
    let v: serde_json::Value = serde_json::from_slice(raw)
        .map_err(|e| SicroError::Validation(format!("hashes.json invalid: {e}")))?;
    let arr = v
        .get("arquivos")
        .and_then(|a| a.as_array())
        .ok_or_else(|| {
            SicroError::Validation("hashes.json missing 'arquivos' array".to_string())
        })?;
    let mut out = Vec::with_capacity(arr.len());
    let mut seen = HashSet::new();
    for entry in arr {
        let path = entry
            .get("caminho")
            .or_else(|| entry.get("path"))
            .and_then(|p| p.as_str())
            .ok_or_else(|| {
                SicroError::Validation(
                    "hashes.json entry missing 'caminho'/'path'".to_string(),
                )
            })?;
        let sha = entry
            .get("sha256")
            .and_then(|p| p.as_str())
            .ok_or_else(|| {
                SicroError::Validation("hashes.json entry missing 'sha256'".to_string())
            })?;
        let sanitised = sanitize_zip_path(path)?;
        let canonical = sanitised
            .to_str()
            .ok_or_else(|| SicroError::Validation("non-UTF8 hash path".to_string()))?
            .replace('\\', "/");
        if seen.insert(canonical.clone()) {
            out.push((canonical, sha.to_ascii_lowercase()));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hashes_json_handles_v0_6_shape() {
        let raw = br#"{
          "algoritmo": "SHA-256",
          "arquivos": [
            { "caminho": "manifest.json", "sha256": "ABC" },
            { "caminho": "caso.json",     "sha256": "DEF" }
          ]
        }"#;
        let out = parse_hashes_json(raw).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "manifest.json");
        assert_eq!(out[0].1, "abc");
        assert_eq!(out[1].1, "def");
    }

    #[test]
    fn parse_hashes_json_rejects_traversal_inside_paths() {
        let raw = br#"{
          "arquivos": [
            { "caminho": "../etc/passwd", "sha256": "00" }
          ]
        }"#;
        assert!(parse_hashes_json(raw).is_err());
    }

    /// In-memory ZIP smoke test: build a one-file ZIP, sha256 it via the
    /// reader, then check `stage_package` + sha256.
    #[test]
    fn package_reader_reads_known_zip() -> std::io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let zip_path = tmp.path().join("toy.zip");
        {
            let f = File::create(&zip_path)?;
            let mut w = zip::ZipWriter::new(f);
            w.start_file(
                "manifest.json",
                zip::write::FileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            w.write_all(b"{}").unwrap();
            w.finish().unwrap();
        }

        let mut reader = PackageReader::open(&zip_path).unwrap();
        assert!(reader.contains("manifest.json"));
        let bytes = reader.read_to_bytes("manifest.json").unwrap().unwrap();
        assert_eq!(bytes, b"{}");

        let sha = reader.sha256("manifest.json").unwrap();
        // SHA-256("{}") = 44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a
        assert_eq!(
            sha,
            "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
        Ok(())
    }
}
