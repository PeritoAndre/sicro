//! Gravadores (DVR, P4.6; `video/dvr.rs`): a importação confere o formato antes de registrar (Hikvision antigo é
//! recusado) e, registrado o original, pede a cópia de trabalho. Erro com código passa pelo catálogo.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use crate::edicao::ffmpeg::sonda;
use crate::edicao::mensagens;
use crate::error::{Result, SicroError};
use crate::video::dvr::{self, CopiaRegistrada, Formato};

async fn em_fundo<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| SicroError::Workspace(format!("gravador: {e}")))
        .and_then(|r| r)
        .map_err(mensagens::erro_de_comando)
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

/// Cópia de trabalho do vídeo `media_id` do caso (feita, provada e registrada; ou a que já existia).
#[tauri::command]
pub async fn video_dvr_copia(workspace_path: String, media_id: String) -> Result<CopiaRegistrada> {
    em_fundo(move || {
        let cap = sonda::capacidades()?;
        dvr::copia_no_caso(&cap, &PathBuf::from(workspace_path), &media_id, &AtomicBool::new(false))
    })
    .await
}
