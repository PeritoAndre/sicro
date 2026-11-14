//! Mídia de prévia (§7.1, decisão O8): o `<video>` toca o original, a cópia por remux ou o proxy de CPU. Dono: P0.4.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use super::ffmpeg::sonda::Capacidades;
use super::modelo::{Racional, TamanhoPx};
use super::motor::FonteInfo;
use super::quadros::OrigemTabela;
use crate::error::Result;

/// O que o `<video>` toca (`Origem` = intermediário de origem, Fase 1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TipoPrevia {
    Original,
    Copia,
    Proxy,
    Origem,
}

/// O que o `<video>` toca. `relativo`: ao caso em `Original`; nos demais, ao cache da Edição do caso.
/// `desloc_s` = pts0 da fonte − pts0 do arquivo tocado; `de_s`/`ate_s` = faixa em tempo de fonte (da tabela);
/// `largura`/`altura` em px do arquivo tocado; `pronta` false = ainda sendo feita na fila de fundo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreviaMidia {
    pub relativo: String,
    pub tipo: TipoPrevia,
    pub desloc_s: f64,
    pub de_s: f64,
    pub ate_s: f64,
    pub largura: u32,
    pub altura: u32,
    pub pronta: bool,
}

/// Passo mínimo e máximo entre quadros, em segundos.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PassoS {
    pub min: f64,
    pub max: f64,
}

/// O que a tela sabe de uma mídia (`edicao_midia_info`). `id` = `video_media.id` do caso; `exibido` em px;
/// `de_s`/`ate_s` em tempo de fonte; `n` = quadros; `erros` None = passada de integridade ainda não feita.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MidiaInfo {
    pub id: String,
    pub sha256: String,
    pub nome: String,
    pub exibido: TamanhoPx,
    pub taxa: Racional,
    pub vfr: bool,
    pub passo: PassoS,
    pub de_s: f64,
    pub ate_s: f64,
    pub n: u32,
    pub tem_audio: bool,
    pub previa: PreviaMidia,
    pub tabela: OrigemTabela,
    pub erros: Option<u32>,
}

/// Parâmetros do proxy de CPU (O8): 720p, `libx264 veryfast crf 23`, GOP 12, sem quadros B, pts iguais aos da fonte.
pub const PROXY_ALTURA: u32 = 720;
pub const PROXY_CRF: u32 = 23;
pub const PROXY_GOP: u32 = 12;

/// Tipo que serve a essa fonte: `Original` (MP4/MOV que começa em zero, H.264 8 bits 4:2:0), senão `Copia`;
/// `Proxy` quando o codec não é H.264 8 bits 4:2:0.
pub fn decidir(fonte: &FonteInfo) -> TipoPrevia {
    todo!()
}

/// A prévia que existe agora, sem fazer nada: `pronta` false quando a cópia ou o proxy ainda não está no cache.
/// `ws` = raiz do caso; `cache` = pasta da Edição do caso.
pub fn consultar(ws: &Path, cache: &Path, fonte: &FonteInfo) -> Result<PreviaMidia> {
    todo!()
}

/// Cópia por remux em `previa/<sha256>.mp4` + `.json`, com o deslocamento medido. `progresso` recebe 0..1.
pub fn copiar(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<PreviaMidia> {
    todo!()
}

/// Proxy de CPU em `proxies/<sha256>.720.mp4` + `.json`. `progresso` recebe 0..1.
pub fn proxy_cpu(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<PreviaMidia> {
    todo!()
}

/// Faz o que `decidir` manda; se o remux falhar, vai o proxy. Trabalho da fila de fundo.
pub fn produzir(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<PreviaMidia> {
    todo!()
}

/// `MidiaInfo` de uma fonte já preparada. `id` e `nome` vêm do registro do caso.
pub fn midia_info(id: &str, nome: &str, fonte: &FonteInfo, previa: PreviaMidia) -> MidiaInfo {
    todo!()
}
