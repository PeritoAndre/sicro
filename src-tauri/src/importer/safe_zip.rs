//! ZIP path safety.
//!
//! A malicious `.sicroapp` could include entries like `../../../etc/passwd` or
//! absolute paths like `C:\Windows\System32\foo.exe`. Without sanitization the
//! `zip` crate would happily extract those wherever the OS lets us write. The
//! helpers here turn every entry name into a *strictly relative* `PathBuf`
//! whose components are all `Normal(_)` — anything else is rejected.

use std::path::{Component, Path, PathBuf};

use crate::error::{Result, SicroError};

/// Validate that `entry_name` from a ZIP central directory describes a file
/// that can be safely extracted into a target directory.
///
/// Returns the canonical relative path (forward slashes only) if accepted.
pub fn sanitize_zip_path(entry_name: &str) -> Result<PathBuf> {
    if entry_name.is_empty() {
        return Err(SicroError::Validation(
            "empty ZIP entry name".to_string(),
        ));
    }

    // Reject NUL and embedded control chars defensively — they can confuse
    // Windows when the path hits NTFS APIs.
    if entry_name.chars().any(|c| c == '\u{0}' || c.is_control()) {
        return Err(SicroError::Validation(format!(
            "ZIP entry name contains control characters: {entry_name:?}"
        )));
    }

    // Normalise the separator so the same path looks identical on Windows
    // and Unix (`zip-rs` stores entries with `/`).
    let normalised = entry_name.replace('\\', "/");

    if normalised.starts_with('/') {
        return Err(SicroError::Validation(format!(
            "absolute ZIP entry rejected: {entry_name:?}"
        )));
    }

    // Windows drive letters embedded in the name (rare but not impossible).
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
        // Reject anything that doesn't look like a plain segment after
        // PathBuf parsing — this also defangs `C:` style fragments.
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
