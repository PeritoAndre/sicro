//! Proveniência da mídia na análise (F6, O3): "Formato do gravador" (Sim, Não), "Observação", "Desvio do relógio (s)"
//! e "Gravador", nas colunas da migração 027 de `video_media` (`recebido_nativo`, `recebido_obs`, `relogio_desvio_s`,
//! `gravador`). A Edição copia para o `MidiaRef` ao trazer a mídia (`recebido`, `relogio_desvio_s`, `gravador`).
//! O `occurrence_id` vem do Manifest. Dono: P1.10.

use serde::{Deserialize, Serialize};

use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};

/// Até o P1.10: recusa com código (um `todo!()` num comando `async` fora de `spawn_blocking` deixaria a promessa da
/// tela sem resposta).
fn ainda_nao() -> SicroError {
    SicroError::Validation(format!("{}: proveniencia", codigo::NAO_SUPORTADO))
}

/// Os quatro campos; None = não informado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProvenienciaMidia {
    pub recebido_nativo: Option<bool>,
    pub recebido_obs: String,
    pub relogio_desvio_s: Option<f64>,
    pub gravador: Option<String>,
}

/// Proveniência gravada da mídia `media_id` (`video_media.id`) do caso.
#[tauri::command]
pub async fn video_proveniencia(workspace_path: String, media_id: String) -> Result<ProvenienciaMidia> {
    let _ = (workspace_path, media_id);
    Err(ainda_nao())
}

/// Grava a proveniência (audita `video.proveniencia`) e devolve o que ficou gravado.
#[tauri::command]
pub async fn video_proveniencia_salvar(workspace_path: String, media_id: String, proveniencia: ProvenienciaMidia) -> Result<ProvenienciaMidia> {
    let _ = (workspace_path, media_id, proveniencia);
    Err(ainda_nao())
}
