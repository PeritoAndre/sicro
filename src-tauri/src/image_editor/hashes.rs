//! G12.8 — Múltiplos hashes para chain of custody pericial.
//!
//! MD5 e SHA-1 estão criptograficamente quebrados (collision attacks)
//! mas continuam sendo exigidos por convenção em muitos laudos. SHA-256
//! é o padrão atual; SHA-3-256 (Keccak) é a próxima geração.
//!
//! Este módulo computa os 4 num único pass do arquivo, evitando 4x I/O.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use sha3::Sha3_256;

use crate::error::{Result, SicroError};
use crate::models::ImageHashSet;

/// Computa os 4 hashes do arquivo num único pass.
///
/// Para arquivos grandes, lê em chunks de 64 KB e atualiza todos os
/// digests em paralelo (na verdade sequencialmente mas no mesmo loop).
pub fn compute_all_hashes(path: &Path) -> Result<ImageHashSet> {
    let file = File::open(path).map_err(|e| {
        SicroError::Filesystem(format!("não consegui abrir {}: {}", path.display(), e))
    })?;
    let mut reader = BufReader::new(file);

    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut sha3 = Sha3_256::new();

    let mut buf = [0u8; 65536];
    loop {
        let n = reader.read(&mut buf).map_err(|e| {
            SicroError::Filesystem(format!("read error: {e}"))
        })?;
        if n == 0 {
            break;
        }
        md5.update(&buf[..n]);
        sha1.update(&buf[..n]);
        sha256.update(&buf[..n]);
        sha3.update(&buf[..n]);
    }

    Ok(ImageHashSet {
        md5: format!("{:x}", md5.finalize()),
        sha1: format!("{:x}", sha1.finalize()),
        sha256: format!("{:x}", sha256.finalize()),
        sha3_256: format!("{:x}", sha3.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn known_message_hashes_match_reference() {
        // Mensagem "abc" tem hashes conhecidos em todas as 4 famílias.
        let mut tf = NamedTempFile::new().unwrap();
        tf.write_all(b"abc").unwrap();
        let hashes = compute_all_hashes(tf.path()).unwrap();
        assert_eq!(hashes.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hashes.sha1, "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            hashes.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hashes.sha3_256,
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
        );
    }

    #[test]
    fn missing_file_returns_error() {
        let result = compute_all_hashes(Path::new("/nonexistent/path/xyz"));
        assert!(result.is_err());
    }
}
