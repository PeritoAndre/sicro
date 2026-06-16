//! Tauri commands that operate on workspaces.
//!
//! Naming convention: command names use snake_case in Rust. The front-end
//! `commands.ts` mirrors them exactly.

use std::path::PathBuf;

use tauri::State;
use uuid::Uuid;

use chrono::Utc;

use crate::database::connection::open_connection;
use crate::database::repositories::occurrence_repo;
use crate::error::{Result, SicroError};
use crate::models::{
    LoadedOccurrence, NewOccurrenceInput, Occurrence, OccurrenceEdit, OccurrenceStatus,
    RecentOccurrence,
};
use crate::state::AppState;
use crate::workspace::manifest::{MANIFEST_FILENAME, SQLITE_FILENAME};
use crate::workspace::{create_workspace, open_workspace};

/// Create a fresh `.sicro` workspace with one initial occurrence row.
#[tauri::command]
pub async fn create_occurrence(
    state: State<'_, AppState>,
    input: NewOccurrenceInput,
) -> Result<LoadedOccurrence> {
    let created = create_workspace(input, state.default_workspace_parent())?;
    let workspace_path = path_to_string(&created.path)?;

    state.upsert_recent(&created.occurrence, &workspace_path, created.manifest.workspace_id)?;

    Ok(LoadedOccurrence {
        occurrence: created.occurrence,
        workspace_path,
    })
}

/// Open an existing `.sicro` workspace by path.
#[tauri::command]
pub async fn open_occurrence(
    state: State<'_, AppState>,
    workspace_path: String,
) -> Result<LoadedOccurrence> {
    let path = PathBuf::from(&workspace_path);
    let opened = open_workspace(&path)?;
    state.upsert_recent(&opened.occurrence, &workspace_path, opened.manifest.workspace_id)?;

    Ok(LoadedOccurrence {
        occurrence: opened.occurrence,
        workspace_path,
    })
}

/// Re-read the occurrence row for a workspace already known to be valid.
/// Used by the front-end when navigating back to a workspace it had loaded.
#[tauri::command]
pub async fn get_occurrence(workspace_path: String) -> Result<Occurrence> {
    let path = PathBuf::from(workspace_path);
    let opened = open_workspace(&path)?;
    Ok(opened.occurrence)
}

/// Atualiza a identificação da ocorrência (cabeçalho do caso). O perito é a
/// PALAVRA FINAL — casos de expediente nascem no Desktop e a coleta de campo é
/// corrigida depois. Sobrescreve os campos editáveis e regrava o `recent.json`
/// (o rótulo/tipo/município mudam). NÃO toca na proveniência nem no pacote
/// `.sicroapp` original (§13: a prova em si permanece imutável; isto é metadado).
#[tauri::command]
pub async fn update_occurrence(
    state: State<'_, AppState>,
    workspace_path: String,
    edit: OccurrenceEdit,
) -> Result<Occurrence> {
    let path = PathBuf::from(&workspace_path);
    let opened = open_workspace(&path)?;
    let mut occ = opened.occurrence;

    // "" (vazio) → NULL, para a UI exibir "—" e não uma string vazia.
    occ.numero_bo = none_if_blank(edit.numero_bo);
    occ.protocolo = none_if_blank(edit.protocolo);
    occ.requisicao = none_if_blank(edit.requisicao);
    occ.oficio = none_if_blank(edit.oficio);
    occ.delegacia = none_if_blank(edit.delegacia);
    occ.tipo_pericia = none_if_blank(edit.tipo_pericia);
    occ.natureza = none_if_blank(edit.natureza);
    occ.resultado = none_if_blank(edit.resultado);
    occ.municipio = none_if_blank(edit.municipio);
    occ.bairro = none_if_blank(edit.bairro);
    occ.logradouro = none_if_blank(edit.logradouro);
    occ.referencia = none_if_blank(edit.referencia);
    occ.latitude = edit.latitude;
    occ.longitude = edit.longitude;
    if let Some(s) = edit.status.as_deref() {
        if let Some(st) = OccurrenceStatus::parse(s) {
            occ.status = st;
        }
    }
    // Mantém a data de encerramento coerente com o status (estampa ao concluir,
    // limpa ao reabrir). Verdade temporal — não inventa data.
    apply_status_side_effects(&mut occ);
    if let Some(peritos) = edit.peritos {
        occ.peritos = peritos
            .into_iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();
    }
    occ.updated_at = Utc::now();

    let conn = open_connection(&path.join(SQLITE_FILENAME))?;
    occurrence_repo::update_full(&conn, &occ)?;
    let _ = occurrence_repo::record_audit(
        &conn,
        Some(&occ.id),
        "occurrence.update",
        Some("dossie"),
        None,
        None,
        None,
    );

    state.upsert_recent(&occ, &workspace_path, opened.manifest.workspace_id)?;
    Ok(occ)
}

/// `""`/whitespace → `None` (a UI mostra "—"); caso contrário, texto aparado.
fn none_if_blank(v: Option<String>) -> Option<String> {
    v.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    })
}

/// Mantém `data_encerramento` coerente com o status: estampa ao concluir (se
/// ainda vazio) e limpa ao reabrir (aberta / em andamento). "Arquivada" preserva
/// o que houver. Não inventa data — usa o relógio do sistema no momento da ação.
fn apply_status_side_effects(occ: &mut Occurrence) {
    match occ.status {
        OccurrenceStatus::Concluida => {
            if occ.data_encerramento.is_none() {
                occ.data_encerramento = Some(Utc::now());
            }
        }
        OccurrenceStatus::Aberta | OccurrenceStatus::EmAndamento => {
            occ.data_encerramento = None;
        }
        OccurrenceStatus::Arquivada => {}
    }
}

/// Muda APENAS o status da ocorrência (concluir / reabrir) — diferente de
/// `update_occurrence`, que reescreve o cabeçalho inteiro (e zeraria campos não
/// enviados). Estampa/limpa a data de encerramento conforme o status. O perito é
/// a palavra final; a proveniência (import_id, raw_*) nunca é tocada.
#[tauri::command]
pub async fn set_occurrence_status(
    state: State<'_, AppState>,
    workspace_path: String,
    status: String,
) -> Result<Occurrence> {
    let path = PathBuf::from(&workspace_path);
    let opened = open_workspace(&path)?;
    let mut occ = opened.occurrence;

    let st = OccurrenceStatus::parse(&status)
        .ok_or_else(|| SicroError::Validation(format!("status inválido: {status}")))?;
    occ.status = st;
    apply_status_side_effects(&mut occ);
    occ.updated_at = Utc::now();

    let conn = open_connection(&path.join(SQLITE_FILENAME))?;
    occurrence_repo::update_full(&conn, &occ)?;
    let _ = occurrence_repo::record_audit(
        &conn,
        Some(&occ.id),
        "occurrence.status",
        Some("home"),
        None,
        None,
        None,
    );

    state.upsert_recent(&occ, &workspace_path, opened.manifest.workspace_id)?;
    Ok(occ)
}

#[tauri::command]
pub async fn list_recent_occurrences(
    state: State<'_, AppState>,
) -> Result<Vec<RecentOccurrence>> {
    Ok(state.list_recents())
}

#[tauri::command]
pub async fn forget_recent_occurrence(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<()> {
    let id = Uuid::parse_str(&workspace_id)
        .map_err(|e| SicroError::Validation(format!("invalid workspace id: {e}")))?;
    state.forget_recent(id)
}

/// Núcleo testável da exclusão do workspace do disco. Só apaga se a pasta for
/// REALMENTE um workspace `.sicro` (manifesto + banco presentes) — trava de
/// segurança contra remover uma pasta arbitrária por engano.
fn delete_occurrence_impl(workspace_path: &str) -> Result<()> {
    let ws = PathBuf::from(workspace_path);
    if !ws.is_dir() {
        return Err(SicroError::Filesystem(format!(
            "ocorrência não encontrada: {}",
            ws.display()
        )));
    }
    let has_manifest = ws.join(MANIFEST_FILENAME).is_file();
    let has_db = ws.join(SQLITE_FILENAME).is_file();
    if !(has_manifest && has_db) {
        return Err(SicroError::Validation(format!(
            "a pasta não é um workspace .sicro válido (manifesto/banco ausentes): {}",
            ws.display()
        )));
    }
    std::fs::remove_dir_all(&ws).map_err(|e| {
        SicroError::Filesystem(format!("falha ao excluir o workspace: {e}"))
    })?;
    Ok(())
}

/// Exclui PERMANENTEMENTE a pasta `.sicro` do disco (fotos, laudos, croquis —
/// tudo). Operação destrutiva e irreversível; o front exige confirmação
/// explícita (digitar o BO) antes de chamar. A limpeza de recentes/índice é
/// feita pelo front via `forget_recent_occurrence` + `remove_case_index`.
#[tauri::command]
pub async fn delete_occurrence(workspace_path: String) -> Result<()> {
    delete_occurrence_impl(&workspace_path)
}

// ---------------------------------------------------------------------------
// Helpers

fn path_to_string(path: &std::path::Path) -> Result<String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| SicroError::Filesystem(format!("non-UTF8 path: {}", path.display())))
}

// Unused yet, kept here so a future "rename / move workspace" command has a clear home.
#[allow(dead_code)]
fn workspace_db_path(workspace_dir: &std::path::Path) -> PathBuf {
    workspace_dir.join(SQLITE_FILENAME)
}

#[allow(dead_code)]
fn touch_audit_open(workspace_dir: &std::path::Path, occurrence_id: &Uuid) -> Result<()> {
    let conn = open_connection(&workspace_dir.join(SQLITE_FILENAME))?;
    occurrence_repo::record_audit(
        &conn,
        Some(occurrence_id),
        "workspace.opened",
        Some("workspace"),
        None,
        None,
        None,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_rejects_missing_path() {
        assert!(delete_occurrence_impl("/caminho/inexistente/zzz123").is_err());
    }

    #[test]
    fn delete_rejects_non_workspace_folder() {
        // Pasta que existe mas NÃO tem manifesto/banco → recusa (e não apaga).
        let base = std::env::temp_dir()
            .join(format!("sicro-del-reject-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let out = delete_occurrence_impl(&base.to_string_lossy());
        assert!(out.is_err());
        assert!(base.is_dir(), "não deve apagar pasta que não é workspace");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn delete_removes_valid_workspace() {
        let base = std::env::temp_dir()
            .join(format!("sicro-del-ok-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join(MANIFEST_FILENAME), b"{}").unwrap();
        std::fs::write(base.join(SQLITE_FILENAME), b"x").unwrap();
        delete_occurrence_impl(&base.to_string_lossy()).expect("deve excluir");
        assert!(!base.exists());
    }
}
