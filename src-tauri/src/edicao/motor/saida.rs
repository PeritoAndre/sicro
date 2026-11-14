//! Fecho do bloco e predefinições de codificação (§5.7). Fase 0: júri e prévia do render. Dono: P0.6a.

use super::grafo::Grafo;
use super::plano::Canvas;
use super::Alvo;
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::error::Result;

/// Codificação escolhida para a exportação inteira (um codificador só). `nome` = "h264_nvenc" | "libx264" | "ffv1"…;
/// `video` = argumentos do codificador de vídeo; `audio` = codec e taxa do áudio na junção; `extensao` = do bloco e da saída;
/// `gpu` = usa codificador de GPU; `timescale` = `-video_track_timescale`.
#[derive(Debug, Clone, PartialEq)]
pub struct Codificacao {
    pub nome: String,
    pub video: Vec<String>,
    pub audio: Vec<String>,
    pub extensao: String,
    pub gpu: bool,
    pub timescale: i64,
}

/// Codificação do alvo: predefinição + sonda (GPU aprovada no tamanho do projeto, senão `libx264`).
pub fn codificacao(alvo: &Alvo, canvas: &Canvas, cap: &Capacidades) -> Result<Codificacao> {
    todo!()
}

/// Fecha o vídeo do bloco: `format`, etiquetas de cor e `split` em `[v]` e `[h]` (o do `framehash`; só `[v]` sem ele).
pub fn fecho(g: &mut Grafo, video: &str, com_framehash: bool) {
    todo!()
}

/// A predefinição perde informação na codificação? (júri e WhatsApp sim; preservação não)
pub fn com_perdas(alvo: &Alvo) -> bool {
    todo!()
}
