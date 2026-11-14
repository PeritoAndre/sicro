//! Medidores dos testes com ffmpeg: quadro de origem por slot, marcas coloridas e cor. Medem identidade de quadro,
//! pixel e cor, nunca só a duração.

use std::path::Path;

use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::error::Result;

/// Um quadro decodificado em RGB 8 bits, por linhas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuadroRgb {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

/// Quadro de origem (N da fonte sintética, pela luma 16 + N) de cada quadro do vídeo, na ordem.
pub fn quadros_de_origem(cap: &Capacidades, video: &Path) -> Result<Vec<u32>> {
    todo!()
}

/// pts de cada quadro do vídeo pelo decodificador, na base do fluxo.
pub fn pts(cap: &Capacidades, video: &Path) -> Result<Vec<i64>> {
    todo!()
}

/// Decodifica o quadro de índice `n` do vídeo.
pub fn quadro(cap: &Capacidades, video: &Path, n: u32) -> Result<QuadroRgb> {
    todo!()
}

/// Caixa [x, y, w, h] em px dos pixels a até `tolerancia` (0..255 por canal) da cor `rgb`; None = nenhum.
pub fn marca(q: &QuadroRgb, rgb: [u8; 3], tolerancia: u8) -> Option<[u32; 4]> {
    todo!()
}

/// Cor média (RGB) da região [x, y, w, h] em px.
pub fn cor_media(q: &QuadroRgb, regiao: [u32; 4]) -> [f64; 3] {
    todo!()
}

/// Pixels diferentes entre dois quadros do mesmo tamanho (qualquer canal além de `tolerancia`).
pub fn pixels_diferentes(a: &QuadroRgb, b: &QuadroRgb, tolerancia: u8) -> u64 {
    todo!()
}

/// Amostras do PCM s16 estéreo 48 kHz do arquivo.
pub fn amostras(cap: &Capacidades, arquivo: &Path) -> Result<u64> {
    todo!()
}
