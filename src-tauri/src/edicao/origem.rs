//! Intermediário de origem (§7.5): clipe com efeito de origem materializado em cache (`origem/<hash>.mp4` + `.json`),
//! com os pts da fonte, até 1080p, pela receita de proxy. Palco e rastreador leem dele; o render nunca (§7.6).
//! Faz também o `.trf` da estabilização quando o efeito ainda não tem (contratos-fase1 (b)). Dono: P1.4.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::edicao::executar::TrfNovo;
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::ClipeTrecho;
use crate::edicao::motor::FonteInfo;
use crate::edicao::previa::PreviaMidia;
use crate::error::Result;

/// Situação do intermediário. `Pronto.trf`: `.trf` feito agora (já em `vidstab/` do projeto); o front grava
/// `trf_sha256` e `zoom_pct` no efeito por `alterar()` (um escritor só, D2).
// CONTRATO: o §7.5 diz `Pronto(PreviaMidia)`; `trf` leva à tela o `.trf` novo (contratos (c) 16).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EstadoOrigem {
    Fazendo { tarefa: String, feito: f32 },
    Pronto { previa: PreviaMidia, trf: Option<TrfNovo> },
}

/// Chave do intermediário: SHA-256 de (mídia, faixa lida, efeitos de origem ligados com os parâmetros, ffmpeg).
pub fn chave(clipe: &ClipeTrecho, fonte: &FonteInfo, cap: &Capacidades) -> String {
    let _ = (clipe, fonte, cap);
    todo!("P1.4")
}

/// Pede o intermediário do clipe: o pronto do cache, ou o trabalho na fila de fundo (`TipoFundo::Origem`, chave
/// `origem:<chave>`). `ws` = caso; `cache` = cache da Edição do caso; `pasta_projeto` para o `.trf`.
pub fn pedir(ws: &Path, cache: &Path, pasta_projeto: &Path, cap: &Capacidades, clipe: &ClipeTrecho, fonte: &FonteInfo) -> Result<EstadoOrigem> {
    let _ = (ws, cache, pasta_projeto, cap, clipe, fonte);
    todo!("P1.4")
}

/// `edicao_origem_pedir`: acha o clipe `clipe_id` no `projeto` (JSON da tela), prepara a fonte e chama `pedir`.
/// Clipe sem efeito de origem: `nao_suportado`.
pub fn pedir_do_projeto(ws: &Path, cache: &Path, projeto: &Value, clipe_id: &str) -> Result<EstadoOrigem> {
    let _ = (ws, cache, projeto, clipe_id);
    todo!("P1.4")
}
