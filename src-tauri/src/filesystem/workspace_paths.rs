//! Workspace-relative path resolution (MVP 5).
//!
//! Every module that reads/writes inside a `.sicro` workspace eventually
//! receives a "relative path" from the frontend or from a row in
//! SQLite. Those paths must never escape the workspace folder; failure
//! to enforce that is a directory traversal vulnerability.
//!
//! `sanitize_relative_path` is the *normalization* step (sync, in-memory):
//!   - rejects absolute paths (`/foo`, `\\foo`);
//!   - rejects drive-anchored paths (`C:\foo`);
//!   - rejects `..` segments;
//!   - rejects empty input;
//!   - strips `.` / empty segments and normalizes separators.
//!
//! `resolve_workspace_relative` builds on the above and:
//!   - joins the sanitized relative path under the workspace root;
//!   - returns a `PathBuf` ready for `std::fs::*` calls.
//!
//! Both functions are pure — they do NOT touch the filesystem. The
//! filesystem-aware variant lives in
//! [`resolve_existing_workspace_path`], which additionally requires the
//! resolved path to exist as a file.
//!
//! `RelativeResolution` is returned by the registry / verifier — it tells
//! whether the original input was safe AND whether the file is on disk.
//! This is intentionally cheap (one `metadata` syscall) so the lightweight
//! integrity check can iterate over thousands of rows quickly.

use std::path::{Path, PathBuf};

use crate::error::{Result, SicroError};

/// Outcome of trying to resolve a workspace-relative reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelativeResolution {
    /// Path is safe and the file exists on disk.
    Ok { absolute: PathBuf, size_bytes: u64 },
    /// Path is safe but the file is not on disk.
    Missing { absolute: PathBuf },
    /// Path violates safety rules (traversal, absolute, drive letter).
    Unsafe { reason: String },
    /// The input was empty / null.
    Empty,
}

impl RelativeResolution {
    pub fn status_str(&self) -> &'static str {
        match self {
            RelativeResolution::Ok { .. } => "ok",
            RelativeResolution::Missing { .. } => "missing_file",
            RelativeResolution::Unsafe { .. } => "unsafe_path",
            RelativeResolution::Empty => "unknown",
        }
    }
}

/// Sanitize a workspace-relative path. Returns the normalized PathBuf
/// (with platform separators) when the input is safe; otherwise an
/// error describing the violation.
///
/// This does NOT touch the filesystem.
pub fn sanitize_relative_path(raw: &str) -> Result<PathBuf> {
    if raw.is_empty() {
        return Err(SicroError::Validation(
            "empty relative path".to_string(),
        ));
    }
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err(SicroError::Validation(format!(
            "absolute path rejected: {raw:?}"
        )));
    }
    // Drive-letter check: e.g. "C:\..." or "c:/...".
    if let Some(c) = raw.chars().next() {
        if c.is_ascii_alphabetic() && raw[1..].starts_with(':') {
            return Err(SicroError::Validation(format!(
                "drive-anchored path rejected: {raw:?}"
            )));
        }
    }
    let mut out = PathBuf::new();
    for part in raw.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(SicroError::Validation(format!(
                "path traversal rejected: {raw:?}"
            )));
        }
        out.push(part);
    }
    if out.as_os_str().is_empty() {
        return Err(SicroError::Validation(format!(
            "relative path resolves to empty: {raw:?}"
        )));
    }
    Ok(out)
}

/// Join `relative` under `workspace_root` after sanitizing. Does not
/// touch the filesystem.
pub fn resolve_workspace_relative(
    workspace_root: &Path,
    relative: &str,
) -> Result<PathBuf> {
    let rel = sanitize_relative_path(relative)?;
    Ok(workspace_root.join(rel))
}

/// Probe a workspace-relative reference for existence in a single,
/// allocation-light pass. Used by the lightweight integrity check —
/// it intentionally does not hash the bytes (see `verify_file_hash`).
pub fn probe_workspace_relative(
    workspace_root: &Path,
    relative: Option<&str>,
) -> RelativeResolution {
    let Some(raw) = relative else {
        return RelativeResolution::Empty;
    };
    if raw.is_empty() {
        return RelativeResolution::Empty;
    }
    let rel = match sanitize_relative_path(raw) {
        Ok(p) => p,
        Err(e) => {
            return RelativeResolution::Unsafe {
                reason: e.to_string(),
            };
        }
    };
    let abs = workspace_root.join(&rel);
    match std::fs::metadata(&abs) {
        Ok(meta) if meta.is_file() => RelativeResolution::Ok {
            absolute: abs,
            size_bytes: meta.len(),
        },
        // Anything else (missing, directory, symlink not pointing to a
        // file, etc.) is treated as "missing" — for the audit purposes of
        // MVP 5 the perito needs to see this as "arquivo ausente".
        Ok(_) | Err(_) => RelativeResolution::Missing { absolute: abs },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn empty_rejected() {
        assert!(sanitize_relative_path("").is_err());
    }

    #[test]
    fn absolute_unix_rejected() {
        assert!(sanitize_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn absolute_windows_rejected() {
        assert!(sanitize_relative_path("\\Windows\\System32").is_err());
    }

    #[test]
    fn drive_letter_rejected() {
        assert!(sanitize_relative_path("C:\\Users\\Public").is_err());
        assert!(sanitize_relative_path("d:/tmp").is_err());
    }

    #[test]
    fn dotdot_rejected_anywhere() {
        assert!(sanitize_relative_path("..").is_err());
        assert!(sanitize_relative_path("foo/../bar").is_err());
        assert!(sanitize_relative_path("foo/bar/..").is_err());
        assert!(sanitize_relative_path("..\\evil").is_err());
    }

    #[test]
    fn simple_path_accepted() {
        let p = sanitize_relative_path("imports/photos/IMG_001.jpg").unwrap();
        // Path components present in correct order.
        let comps: Vec<_> = p
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            comps,
            vec!["imports", "photos", "IMG_001.jpg"],
        );
    }

    #[test]
    fn redundant_separators_collapse() {
        let p = sanitize_relative_path("imports///photos/./IMG.jpg").unwrap();
        let comps: Vec<_> = p
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert_eq!(comps, vec!["imports", "photos", "IMG.jpg"]);
    }

    #[test]
    fn mixed_separators_accepted() {
        let p = sanitize_relative_path("a\\b/c").unwrap();
        let comps: Vec<_> = p
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert_eq!(comps, vec!["a", "b", "c"]);
    }

    #[test]
    fn resolve_joins_under_workspace() {
        let tmp = TempDir::new().unwrap();
        let result =
            resolve_workspace_relative(tmp.path(), "imports/photos/IMG.jpg").unwrap();
        assert!(result.starts_with(tmp.path()));
    }

    #[test]
    fn resolve_rejects_traversal_even_with_existing_workspace() {
        let tmp = TempDir::new().unwrap();
        let err =
            resolve_workspace_relative(tmp.path(), "../../etc/passwd").unwrap_err();
        match err {
            SicroError::Validation(_) => {}
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn probe_returns_ok_for_existing_file() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("imports").join("photos");
        fs::create_dir_all(&sub).unwrap();
        let file = sub.join("IMG.jpg");
        fs::write(&file, b"hello world").unwrap();

        match probe_workspace_relative(
            tmp.path(),
            Some("imports/photos/IMG.jpg"),
        ) {
            RelativeResolution::Ok { size_bytes, .. } => assert_eq!(size_bytes, 11),
            other => panic!("expected Ok, got {other:?}"),
        }
    }

    #[test]
    fn probe_returns_missing_for_absent_file() {
        let tmp = TempDir::new().unwrap();
        match probe_workspace_relative(tmp.path(), Some("missing.bin")) {
            RelativeResolution::Missing { .. } => {}
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    #[test]
    fn probe_returns_unsafe_for_traversal() {
        let tmp = TempDir::new().unwrap();
        match probe_workspace_relative(
            tmp.path(),
            Some("../../etc/passwd"),
        ) {
            RelativeResolution::Unsafe { .. } => {}
            other => panic!("expected Unsafe, got {other:?}"),
        }
    }

    #[test]
    fn probe_returns_empty_for_none_or_empty_string() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(
            probe_workspace_relative(tmp.path(), None),
            RelativeResolution::Empty,
        );
        assert_eq!(
            probe_workspace_relative(tmp.path(), Some("")),
            RelativeResolution::Empty,
        );
    }
}
