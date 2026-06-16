//! Cabeçalhos oficiais — armazenamento em PASTA DEDICADA.
//!
//! Cada cabeçalho é um arquivo `<id>.json` em `<app_config_dir>/cabecalhos/`
//! (ex.: `%APPDATA%\br.org\SICRO\SICRO\cabecalhos\builtin-pcap.json`). Substitui
//! o antigo array `header_templates` embutido no `app-settings.json`.
//!
//! O backend é deliberadamente "burro": só lê/grava/apaga arquivos JSON, sem
//! conhecer a forma exata do template (o front é o dono do schema do conteúdo
//! ProseMirror). O "de fábrica" (`builtin-pcap`) é materializado como arquivo
//! pelo front no primeiro uso, então fica editável/atualizável. Escrita atômica.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::error::{Result, SicroError};
use crate::filesystem::atomic_write_bytes;

const SUBDIR: &str = "cabecalhos";

/// Pasta `<app_config_dir>/cabecalhos/`, criada se preciso.
fn templates_dir(app: &AppHandle) -> Result<PathBuf> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|e| SicroError::Filesystem(format!("config dir: {e}")))?
        .join(SUBDIR);
    std::fs::create_dir_all(&base).map_err(|e| {
        SicroError::Filesystem(format!("não consegui criar {}: {}", base.display(), e))
    })?;
    Ok(base)
}

/// Id seguro como nome de arquivo (sem path traversal): alfanumérico + `-`/`_`.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Lista todos os cabeçalhos salvos (cada `<id>.json` vira um objeto). Arquivos
/// ilegíveis são ignorados (best-effort). O front ordena/coerções.
#[tauri::command]
pub async fn list_header_templates(app: AppHandle) -> Result<Vec<serde_json::Value>> {
    let dir = templates_dir(&app)?;
    let mut out: Vec<serde_json::Value> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(bytes) = std::fs::read(&p) {
                    if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        out.push(v);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Grava/atualiza um cabeçalho. Exige `template.id` (string segura). Atômico.
#[tauri::command]
pub async fn save_header_template(app: AppHandle, template: serde_json::Value) -> Result<()> {
    let id = template.get("id").and_then(|v| v.as_str()).unwrap_or("");
    if !is_safe_id(id) {
        return Err(SicroError::Validation(format!(
            "id de cabeçalho inválido: '{id}'"
        )));
    }
    let dir = templates_dir(&app)?;
    let bytes = serde_json::to_vec_pretty(&template)?;
    atomic_write_bytes(&dir.join(format!("{id}.json")), &bytes)?;
    Ok(())
}

/// Remove um cabeçalho salvo. Idempotente (arquivo ausente = ok).
#[tauri::command]
pub async fn delete_header_template(app: AppHandle, template_id: String) -> Result<()> {
    if !is_safe_id(&template_id) {
        return Err(SicroError::Validation(format!(
            "id de cabeçalho inválido: '{template_id}'"
        )));
    }
    let dir = templates_dir(&app)?;
    let path = dir.join(format!("{template_id}.json"));
    if path.is_file() {
        std::fs::remove_file(&path).map_err(|e| {
            SicroError::Filesystem(format!("não consegui remover {}: {}", path.display(), e))
        })?;
    }
    Ok(())
}
