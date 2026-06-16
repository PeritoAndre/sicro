//! SHA-256 helpers used by the .sicroapp importer.
//!
//! Two flavours:
//!   - `sha256_bytes`  — for short payloads already in memory (manifest, JSONs).
//!   - `sha256_file`   — streams the file in 64 KiB chunks; safe for large
//!                       photos / videos because it never reads the whole
//!                       file into memory.
//!
//! Output format: hex lowercase, matching what the SICRO Operacional mobile
//! writes into `hashes.json`. The mobile uses Dart's `crypto.sha256.convert`
//! which also returns hex lowercase — so byte-equality of strings is the
//! contract.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::{Result, SicroError};

/// Hash a byte slice. Convenient for hashing JSON payloads.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

/// Stream a file from disk and return its SHA-256 in hex lowercase.
/// 64 KiB buffer keeps peak memory bounded for photos / videos.
pub fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| {
        SicroError::Filesystem(format!("cannot open {} for hashing: {}", path.display(), e))
    })?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| {
            SicroError::Filesystem(format!("read error on {}: {}", path.display(), e))
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(digest: &[u8]) -> String {
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_matches_canonical_value() {
        // SHA-256 of empty input is the well-known constant below.
        assert_eq!(
            sha256_bytes(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn known_short_string() {
        // SHA-256("abc") — canonical test vector from FIPS PUB 180-4.
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn file_hash_matches_bytes_hash() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("payload.bin");
        let payload = b"SICRO payload for hashing test";
        std::fs::write(&path, payload).expect("write tmp");
        assert_eq!(sha256_file(&path).unwrap(), sha256_bytes(payload));
    }
}
