//! Estatísticas — persistência das exportações do dashboard.
//!
//! O dashboard de Estatísticas é montado no FRONT, compondo comandos de leitura
//! já existentes (dossiê, registro de evidências, laudos, vídeo, velocidade,
//! distância) e agregando em memória. Não há agregador no backend.
//!
//! A única coisa que mora aqui é a ESCRITA da exportação que o front gera
//! (HTML/CSV/JSON), salva em `<workspace>/exports/estatisticas/` com escrita
//! atômica — mesmo lar das outras exportações do app.

use std::path::PathBuf;

use chrono::Utc;
use tauri::{AppHandle, Manager};

use crate::error::{Result, SicroError};

fn ext_for(format: &str) -> Result<&'static str> {
    match format {
        "html" => Ok("html"),
        "csv" => Ok("csv"),
        "json" => Ok("json"),
        other => Err(SicroError::Validation(format!(
            "formato de exportação não suportado: {other}"
        ))),
    }
}

/// Núcleo testável (síncrono) da escrita. Devolve o caminho RELATIVO ao
/// workspace do arquivo criado.
fn save_statistics_export_impl(
    workspace_path: &str,
    format: &str,
    content: &str,
) -> Result<String> {
    let ws = PathBuf::from(workspace_path);
    if !ws.is_dir() {
        return Err(SicroError::Filesystem(format!(
            "workspace inválido: {}",
            ws.display()
        )));
    }
    let ext = ext_for(format)?;

    let dir = ws.join("exports").join("estatisticas");
    std::fs::create_dir_all(&dir).map_err(|e| {
        SicroError::Filesystem(format!(
            "não foi possível criar a pasta de exportação: {e}"
        ))
    })?;

    let stamp = Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("estatisticas_{stamp}.{ext}");
    let target = dir.join(&filename);
    crate::filesystem::atomic_write_bytes(&target, content.as_bytes())?;

    Ok(format!("exports/estatisticas/{filename}"))
}

/// Grava o conteúdo de uma exportação de estatísticas no workspace e devolve
/// o caminho RELATIVO (sob o workspace) do arquivo criado.
#[tauri::command]
pub async fn save_statistics_export(
    workspace_path: String,
    format: String,
    content: String,
) -> Result<String> {
    save_statistics_export_impl(&workspace_path, &format, &content)
}

/// Grava uma exportação das estatísticas GERAIS (entre casos). Como não há
/// ocorrência associada, salva em `Documentos/SICRO/estatisticas-gerais/` e
/// devolve o caminho ABSOLUTO (para revelar no explorador).
#[tauri::command]
pub async fn save_general_statistics_export(
    app: AppHandle,
    format: String,
    content: String,
) -> Result<String> {
    let ext = ext_for(&format)?;
    let docs = app
        .path()
        .document_dir()
        .map_err(|e| SicroError::Filesystem(format!("pasta Documentos: {e}")))?;
    let dir = docs.join("SICRO").join("estatisticas-gerais");
    std::fs::create_dir_all(&dir).map_err(|e| {
        SicroError::Filesystem(format!(
            "não foi possível criar a pasta de exportação: {e}"
        ))
    })?;
    let stamp = Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("estatisticas_gerais_{stamp}.{ext}");
    let target = dir.join(&filename);
    crate::filesystem::atomic_write_bytes(&target, content.as_bytes())?;
    Ok(target.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_format() {
        let dir = std::env::temp_dir();
        let out =
            save_statistics_export_impl(&dir.to_string_lossy(), "xlsx", "x");
        assert!(out.is_err());
    }

    #[test]
    fn rejects_missing_workspace() {
        let out = save_statistics_export_impl(
            "/caminho/que/nao/existe/xyz123",
            "json",
            "{}",
        );
        assert!(out.is_err());
    }

    #[test]
    fn writes_json_and_returns_relative_path() {
        let base = std::env::temp_dir()
            .join(format!("sicro-stats-test-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let rel = save_statistics_export_impl(
            &base.to_string_lossy(),
            "json",
            "{\"ok\":true}",
        )
        .expect("should write");
        assert!(rel.starts_with("exports/estatisticas/"));
        assert!(rel.ends_with(".json"));
        let abs = base.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        assert!(abs.is_file());
        let body = std::fs::read_to_string(&abs).unwrap();
        assert_eq!(body, "{\"ok\":true}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
