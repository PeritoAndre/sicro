//! Path utilities for `.sicro` workspaces.

use std::path::{Path, PathBuf};

use crate::error::{Result, SicroError};
use crate::filesystem::sanitize_folder_name;

/// Nome da pasta do workspace. Com nome do caso, é o próprio nome (colisão é
/// tratada em `unique_workspace_path`); sem, "BO_<bo>_<município>_<id curto>";
/// tudo vazio → "ocorrencia".
pub fn derive_workspace_name(
    titulo: Option<&str>,
    numero_bo: Option<&str>,
    municipio: Option<&str>,
    short_id: &str,
) -> String {
    if let Some(t) = titulo.map(str::trim).filter(|t| !t.is_empty()) {
        return sanitize_folder_name(t);
    }
    let mut parts: Vec<String> = Vec::new();
    if let Some(bo) = numero_bo {
        let bo = bo.trim();
        if !bo.is_empty() {
            parts.push(format!("BO_{}", bo.replace(['/', ' '], "_")));
        }
    }
    if let Some(mun) = municipio {
        let mun = mun.trim();
        if !mun.is_empty() {
            parts.push(mun.replace(' ', "_"));
        }
    }
    parts.push(short_id.to_string());
    sanitize_folder_name(&parts.join("_"))
}

/// Caminho novo que ainda não existe (não sobrescreve vizinho).
pub fn unique_workspace_path(parent: &Path, base_name: &str) -> Result<PathBuf> {
    let mut candidate = parent.join(format!("{base_name}.sicro"));
    let mut suffix = 1;
    while candidate.exists() {
        suffix += 1;
        candidate = parent.join(format!("{base_name}_{suffix}.sicro"));
        if suffix > 999 {
            return Err(SicroError::Workspace(format!(
                "could not find unique workspace name under {}",
                parent.display()
            )));
        }
    }
    Ok(candidate)
}
