//! Índice GLOBAL de casos — base das "estatísticas gerais de trabalho".
//!
//! O app é centrado numa ocorrência por vez (cada `.sicro` é separado), então
//! não há de onde tirar visão entre-casos sem abrir cada um. Este índice
//! leve resolve isso: um `case-index.json` no `app_config_dir` (o mesmo
//! "cofrinho" das Configurações) com os metadados de cada caso (tipo, município,
//! status, datas, peritos). É alimentado automaticamente toda vez que um caso
//! vira ativo (criar/abrir/importar) — sem varrer disco.
//!
//! NÃO guarda evidência nem conteúdo pericial; só os campos de cabeçalho da
//! ocorrência, para agregação estatística descritiva.

use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::{Result, SicroError};

const INDEX_FILENAME: &str = "case-index.json";
const SCHEMA_VERSION: &str = "1";

/// Contagens por módulo de um caso, capturadas quando ele fica ativo. São um
/// retrato "da última abertura" — não varrem disco em tempo real.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CaseCounts {
    #[serde(default)]
    pub laudos: u32,
    #[serde(default)]
    pub croquis: u32,
    #[serde(default)]
    pub photos: u32,
    #[serde(default)]
    pub videos: u32,
    #[serde(default)]
    pub image_analyses: u32,
    #[serde(default)]
    pub laudo_exports: u32,
    #[serde(default)]
    pub image_exports: u32,
    /// Quando essas contagens foram capturadas (RFC3339).
    #[serde(default)]
    pub counted_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CaseIndexEntry {
    /// `occurrence.id` — chave de deduplicação.
    pub workspace_id: String,
    #[serde(default)]
    pub workspace_path: String,
    #[serde(default)]
    pub numero_bo: Option<String>,
    #[serde(default)]
    pub tipo_pericia: Option<String>,
    #[serde(default)]
    pub natureza: Option<String>,
    #[serde(default)]
    pub municipio: Option<String>,
    #[serde(default)]
    pub bairro: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub data_fato: Option<String>,
    #[serde(default)]
    pub data_acionamento: Option<String>,
    #[serde(default)]
    pub data_chegada: Option<String>,
    #[serde(default)]
    pub data_encerramento: Option<String>,
    #[serde(default)]
    pub peritos: Vec<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    /// Quando o índice foi atualizado pela última vez (definido pelo backend).
    #[serde(default)]
    pub indexed_at: String,
    /// Contagens por módulo (laudos/croquis/mídias…), best-effort. `None`
    /// quando o caso ainda não foi aberto desde que a feature existe.
    #[serde(default)]
    pub counts: Option<CaseCounts>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CaseIndexFile {
    #[serde(default = "default_schema")]
    schema_version: String,
    #[serde(default)]
    entries: Vec<CaseIndexEntry>,
}

impl Default for CaseIndexFile {
    fn default() -> Self {
        Self {
            schema_version: default_schema(),
            entries: Vec::new(),
        }
    }
}

fn default_schema() -> String {
    SCHEMA_VERSION.to_string()
}

fn index_path(app: &AppHandle) -> Result<PathBuf> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|e| SicroError::Filesystem(format!("config dir: {e}")))?;
    std::fs::create_dir_all(&base).map_err(|e| {
        SicroError::Filesystem(format!("cannot create config dir: {e}"))
    })?;
    Ok(base.join(INDEX_FILENAME))
}

fn read_index(path: &PathBuf) -> CaseIndexFile {
    if !path.is_file() {
        return CaseIndexFile::default();
    }
    match std::fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice::<CaseIndexFile>(&bytes).unwrap_or_default()
        }
        Err(_) => CaseIndexFile::default(),
    }
}

fn write_index(path: &PathBuf, file: &CaseIndexFile) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(file)
        .map_err(|e| SicroError::Workspace(format!("serialize case index: {e}")))?;
    crate::filesystem::atomic_write_bytes(path, &bytes)?;
    Ok(())
}

/// Núcleo testável do upsert: substitui a entrada de mesmo `workspace_id`
/// (ou insere), carimba `indexed_at`, ordena por `workspace_id` para
/// estabilidade e devolve o arquivo resultante.
fn upsert_into(mut file: CaseIndexFile, mut entry: CaseIndexEntry, now: String) -> CaseIndexFile {
    if entry.workspace_id.trim().is_empty() {
        return file; // sem chave, ignora silenciosamente
    }
    entry.indexed_at = now;
    // Preserva as contagens anteriores quando o upsert vem só com o cabeçalho
    // (ex.: editar status) — assim a produção registrada não é apagada.
    if entry.counts.is_none() {
        if let Some(prev) = file
            .entries
            .iter()
            .find(|e| e.workspace_id == entry.workspace_id)
        {
            entry.counts = prev.counts.clone();
        }
    }
    file.entries.retain(|e| e.workspace_id != entry.workspace_id);
    file.entries.push(entry);
    file
}

/// Núcleo testável da remoção: tira a entrada de `workspace_id` do índice.
fn remove_from(mut file: CaseIndexFile, workspace_id: &str) -> CaseIndexFile {
    file.entries.retain(|e| e.workspace_id != workspace_id);
    file
}

/// Lê o índice global de casos.
#[tauri::command]
pub async fn get_case_index(app: AppHandle) -> Result<Vec<CaseIndexEntry>> {
    let path = index_path(&app)?;
    Ok(read_index(&path).entries)
}

/// Insere/atualiza um caso no índice (idempotente por `workspace_id`).
#[tauri::command]
pub async fn upsert_case_index(app: AppHandle, entry: CaseIndexEntry) -> Result<()> {
    let path = index_path(&app)?;
    let file = read_index(&path);
    let updated = upsert_into(file, entry, Utc::now().to_rfc3339());
    write_index(&path, &updated)
}

/// Remove um caso do índice global (NÃO apaga nada do disco — só tira das
/// listas e estatísticas). O caso reaparece se for reaberto.
#[tauri::command]
pub async fn remove_case_index(app: AppHandle, workspace_id: String) -> Result<()> {
    let path = index_path(&app)?;
    let file = read_index(&path);
    let updated = remove_from(file, &workspace_id);
    write_index(&path, &updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, status: &str) -> CaseIndexEntry {
        CaseIndexEntry {
            workspace_id: id.to_string(),
            status: status.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn upsert_inserts_then_replaces_by_id() {
        let mut f = CaseIndexFile::default();
        f = upsert_into(f, entry("a", "aberta"), "t1".into());
        f = upsert_into(f, entry("b", "aberta"), "t2".into());
        assert_eq!(f.entries.len(), 2);
        // re-upsert "a" com novo status → substitui, não duplica
        f = upsert_into(f, entry("a", "concluida"), "t3".into());
        assert_eq!(f.entries.len(), 2);
        let a = f.entries.iter().find(|e| e.workspace_id == "a").unwrap();
        assert_eq!(a.status, "concluida");
        assert_eq!(a.indexed_at, "t3");
    }

    #[test]
    fn upsert_preserves_counts_when_header_only() {
        let mut f = CaseIndexFile::default();
        let mut e = entry("a", "aberta");
        e.counts = Some(CaseCounts {
            laudos: 3,
            ..Default::default()
        });
        f = upsert_into(f, e, "t1".into());

        // Re-upsert só com o cabeçalho (counts None) → mantém as contagens.
        f = upsert_into(f, entry("a", "concluida"), "t2".into());
        let a = f.entries.iter().find(|e| e.workspace_id == "a").unwrap();
        assert_eq!(a.status, "concluida");
        assert_eq!(a.counts.as_ref().unwrap().laudos, 3);

        // Upsert COM novas contagens → substitui.
        let mut e2 = entry("a", "concluida");
        e2.counts = Some(CaseCounts {
            laudos: 5,
            ..Default::default()
        });
        f = upsert_into(f, e2, "t3".into());
        let a = f.entries.iter().find(|e| e.workspace_id == "a").unwrap();
        assert_eq!(a.counts.as_ref().unwrap().laudos, 5);
    }

    #[test]
    fn remove_takes_entry_out_by_id() {
        let mut f = CaseIndexFile::default();
        f = upsert_into(f, entry("a", "aberta"), "t1".into());
        f = upsert_into(f, entry("b", "aberta"), "t2".into());
        f = remove_from(f, "a");
        assert_eq!(f.entries.len(), 1);
        assert_eq!(f.entries[0].workspace_id, "b");
        // remover id inexistente é no-op
        f = remove_from(f, "zzz");
        assert_eq!(f.entries.len(), 1);
    }

    #[test]
    fn upsert_ignores_empty_id() {
        let f = CaseIndexFile::default();
        let f = upsert_into(f, entry("  ", "aberta"), "t1".into());
        assert_eq!(f.entries.len(), 0);
    }

    #[test]
    fn file_roundtrips_and_tolerates_partial_json() {
        let json = r#"{ "entries": [ { "workspace_id": "x", "status": "aberta" } ] }"#;
        let parsed: CaseIndexFile = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.schema_version, "1");
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].workspace_id, "x");
    }
}
