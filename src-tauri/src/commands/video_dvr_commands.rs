//! Gravadores (DVR, P4.6; `video/dvr.rs`): a importação confere o formato antes de registrar (Hikvision antigo é
//! recusado), prepara a cópia de trabalho a partir do arquivo escolhido (avanço no evento `dvr-progress`, cancelável)
//! e, registrado o original, registra a cópia. Erro com código passa pelo catálogo.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::edicao::ffmpeg::sonda;
use crate::edicao::mensagens;
use crate::error::{Result, SicroError};
use crate::video::dvr::{self, CopiaRegistrada, Fase, Formato, Preparo};

/// Evento do avanço da cópia: `{ fase: "copia" | "prova", fracao: 0–1 | null }`.
pub const EVENTO_AVANCO: &str = "dvr-progress";

async fn em_fundo<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| SicroError::Workspace(format!("gravador: {e}")))
        .and_then(|r| r)
        .map_err(mensagens::erro_de_comando)
}

/// Uma cópia de gravador por vez; `video_dvr_cancelar` vale para ela.
static TAREFA: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

struct Tarefa(Arc<AtomicBool>);

impl Tarefa {
    fn nova() -> Result<Self> {
        let mut g = TAREFA.lock().unwrap_or_else(|e| e.into_inner());
        if g.is_some() {
            return Err(SicroError::Validation("cópia de gravador em andamento".into()));
        }
        let f = Arc::new(AtomicBool::new(false));
        *g = Some(f.clone());
        Ok(Tarefa(f))
    }
}

impl Drop for Tarefa {
    fn drop(&mut self) {
        *TAREFA.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

#[derive(Debug, Clone, Serialize)]
struct Avanco {
    fase: Fase,
    fracao: Option<f64>,
}

fn emissor(app: AppHandle) -> impl Fn(Fase, Option<f64>) + Sync {
    move |fase, fracao| {
        let _ = app.emit(EVENTO_AVANCO, Avanco { fase, fracao });
    }
}

/// Formato de gravador do arquivo escolhido (None = vídeo comum). O que o ffmpeg não abre volta como erro
/// `gravador_nao_suportado`, antes de o arquivo entrar no caso.
pub fn conferir(caminho: &std::path::Path) -> Result<Option<Formato>> {
    let f = dvr::reconhecer(caminho)?;
    if let Some(f) = f {
        dvr::exigir_suportado(f)?;
    }
    Ok(f)
}

#[tauri::command]
pub async fn video_dvr_conferir(source_path: String) -> Result<Option<Formato>> {
    em_fundo(move || conferir(&PathBuf::from(source_path))).await
}

/// Cópia de trabalho do arquivo escolhido, feita e provada antes de o original entrar no caso. Cancelada: erro
/// `cancelado` e nada no caso.
#[tauri::command]
pub async fn video_dvr_preparar(app: AppHandle, workspace_path: String, source_path: String) -> Result<Preparo> {
    let tarefa = Tarefa::nova()?;
    em_fundo(move || {
        let cap = sonda::capacidades()?;
        dvr::preparar(&cap, &PathBuf::from(workspace_path), &PathBuf::from(source_path), &tarefa.0, &emissor(app))
    })
    .await
}

/// Registra a cópia preparada (`chave`) para o original `media_id`, que a importação acabou de registrar.
#[tauri::command]
pub async fn video_dvr_registrar(workspace_path: String, media_id: String, chave: String) -> Result<CopiaRegistrada> {
    em_fundo(move || {
        let cap = sonda::capacidades()?;
        dvr::registrar_pendente(&cap, &PathBuf::from(workspace_path), &media_id, &chave)
    })
    .await
}

/// Esquece a cópia preparada (`chave`) cujo original não entrou no caso; o `.parcial` some.
#[tauri::command]
pub async fn video_dvr_descartar(workspace_path: String, chave: String) -> Result<()> {
    em_fundo(move || {
        dvr::descartar(&PathBuf::from(workspace_path), &chave);
        Ok(())
    })
    .await
}

/// Pede o fim da cópia em curso: os ffmpeg morrem e o `.parcial` some.
#[tauri::command]
pub async fn video_dvr_cancelar() -> Result<()> {
    if let Some(f) = TAREFA.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        f.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Cópia de trabalho do vídeo `media_id` do caso (feita, provada e registrada; ou a que já existia).
#[tauri::command]
pub async fn video_dvr_copia(app: AppHandle, workspace_path: String, media_id: String) -> Result<CopiaRegistrada> {
    let tarefa = Tarefa::nova()?;
    em_fundo(move || {
        let cap = sonda::capacidades()?;
        dvr::copia_no_caso_com(&cap, &PathBuf::from(workspace_path), &media_id, &tarefa.0, &emissor(app))
    })
    .await
}
