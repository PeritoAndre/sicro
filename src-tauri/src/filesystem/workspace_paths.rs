//! Caminhos relativos ao workspace. Tudo que vem do front ou do SQLite passa
//! por `sanitize_relative_path` (rejeita absoluto, letra de drive e `..`) antes
//! de tocar o disco — senão é directory traversal.

use std::path::{Path, PathBuf};

use crate::error::{Result, SicroError};

/// Resultado de resolver uma referência relativa ao workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelativeResolution {
    /// Seguro e existe.
    Ok { absolute: PathBuf, size_bytes: u64 },
    /// Seguro, mas não está no disco.
    Missing { absolute: PathBuf },
    /// Viola a regra (traversal, absoluto, letra de drive).
    Unsafe { reason: String },
    /// Entrada vazia/null.
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

/// Normaliza (separadores, `.`/segmentos vazios) ou erra descrevendo a violação.
/// Não toca o disco.
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
    // Letra de drive: "C:\…" ou "c:/…".
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

/// Junta sob a raiz após sanitizar. Não toca o disco.
pub fn resolve_workspace_relative(
    workspace_root: &Path,
    relative: &str,
) -> Result<PathBuf> {
    let rel = sanitize_relative_path(relative)?;
    Ok(workspace_root.join(rel))
}

/// Existe? Um `metadata` só — a verificação leve itera milhares de linhas.
/// Não faz hash.
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
        // Pasta, symlink quebrado etc. contam como "ausente" para o perito.
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
