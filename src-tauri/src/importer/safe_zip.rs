//! Sanitização de caminhos do ZIP: recusa `..`, caminhos absolutos e letras de
//! drive; devolve só componentes `Normal`.

use std::path::{Component, Path, PathBuf};

use crate::error::{Result, SicroError};

/// Caminho relativo canônico da entrada, ou erro se não for seguro extrair.
pub fn sanitize_zip_path(entry_name: &str) -> Result<PathBuf> {
    if entry_name.is_empty() {
        return Err(SicroError::Validation(
            "empty ZIP entry name".to_string(),
        ));
    }

    // Caracteres de controle confundem as APIs NTFS.
    if entry_name.chars().any(|c| c == '\u{0}' || c.is_control()) {
        return Err(SicroError::Validation(format!(
            "ZIP entry name contains control characters: {entry_name:?}"
        )));
    }

    // Mesmo caminho nos dois sistemas (zip-rs guarda com `/`).
    let normalised = entry_name.replace('\\', "/");

    if normalised.starts_with('/') {
        return Err(SicroError::Validation(format!(
            "absolute ZIP entry rejected: {entry_name:?}"
        )));
    }

    // Letra de drive do Windows.
    if let Some(c) = normalised.chars().next() {
        if c.is_alphabetic() && normalised[1..].starts_with(":/") {
            return Err(SicroError::Validation(format!(
                "drive-anchored ZIP entry rejected: {entry_name:?}"
            )));
        }
    }

    let mut out = PathBuf::new();
    for raw in normalised.split('/') {
        if raw.is_empty() || raw == "." {
            continue;
        }
        if raw == ".." {
            return Err(SicroError::Validation(format!(
                "ZIP entry uses '..' traversal: {entry_name:?}"
            )));
        }
        // Também neutraliza fragmentos tipo `C:`.
        let fragment = Path::new(raw);
        for comp in fragment.components() {
            match comp {
                Component::Normal(s) => out.push(s),
                _ => {
                    return Err(SicroError::Validation(format!(
                        "ZIP entry contains a special component: {entry_name:?}"
                    )))
                }
            }
        }
    }

    if out.as_os_str().is_empty() {
        return Err(SicroError::Validation(format!(
            "ZIP entry sanitised to empty path: {entry_name:?}"
        )));
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_typical_zip_entries() {
        assert_eq!(
            sanitize_zip_path("manifest.json").unwrap(),
            PathBuf::from("manifest.json")
        );
        assert_eq!(
            sanitize_zip_path("fotos/foto_123.jpg").unwrap(),
            PathBuf::from("fotos").join("foto_123.jpg")
        );
        assert_eq!(
            sanitize_zip_path("fotos\\foto_123.jpg").unwrap(),
            PathBuf::from("fotos").join("foto_123.jpg")
        );
    }

    #[test]
    fn rejects_dot_dot_traversal() {
        assert!(sanitize_zip_path("../etc/passwd").is_err());
        assert!(sanitize_zip_path("fotos/../../../boom.txt").is_err());
    }

    #[test]
    fn rejects_absolute_paths() {
        assert!(sanitize_zip_path("/etc/passwd").is_err());
        assert!(sanitize_zip_path("\\Windows\\System32\\evil.exe").is_err());
        assert!(sanitize_zip_path("C:/Windows/System32/evil.exe").is_err());
    }

    #[test]
    fn rejects_empty_and_control_chars() {
        assert!(sanitize_zip_path("").is_err());
        assert!(sanitize_zip_path("\u{0}").is_err());
        assert!(sanitize_zip_path("foo\nbar").is_err());
    }

    #[test]
    fn trims_redundant_dots_and_slashes() {
        assert_eq!(
            sanitize_zip_path("./fotos//foto_123.jpg").unwrap(),
            PathBuf::from("fotos").join("foto_123.jpg")
        );
    }
}
