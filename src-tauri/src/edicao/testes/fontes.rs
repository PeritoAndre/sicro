//! Matriz de fontes sintéticas (§11; geradores em `cm/00-fontes.sh`, `cm/07c`, `cm/08`, `cm/09-fontes-retomada.sh`).
//! Em toda fonte o quadro N tem luma 16 + N (sem perda), para o medidor achar o quadro de origem.

use std::path::PathBuf;
use std::sync::Arc;

use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::error::Result;

/// As fontes da matriz.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fonte {
    /// MP4 30 qps, base 1/15360.
    Mp4,
    /// MP4 de escala 1000.
    Mp4Escala1000,
    /// MP4 de escala 30.
    Mp4Escala30,
    /// MKV 30 qps (base 1/1000).
    Mkv30,
    /// MKV 23,976 qps.
    Mkv23976,
    /// AVI 25 qps (sem pts nos pacotes).
    Avi25,
    /// TS 59,94 qps com início ≠ 0.
    Ts5994,
    /// MP4 com quadros B.
    Mp4QuadrosB,
    /// MP4 com lista de edição.
    Mp4ListaDeEdicao,
    /// TS começando no meio do GOP.
    TsMeioDoGop,
    MpegPs,
    TaxaVariavel,
    /// Oscilação de ±4 ms nos pts.
    Oscilacao,
    FaixaCheia,
    Entrelacado,
    /// Pixel 10:11.
    PixelNaoQuadrado,
    Giro90,
}

/// Todas, na ordem da matriz.
pub const MATRIZ: &[Fonte] = &[
    Fonte::Mp4,
    Fonte::Mp4Escala1000,
    Fonte::Mp4Escala30,
    Fonte::Mkv30,
    Fonte::Mkv23976,
    Fonte::Avi25,
    Fonte::Ts5994,
    Fonte::Mp4QuadrosB,
    Fonte::Mp4ListaDeEdicao,
    Fonte::TsMeioDoGop,
    Fonte::MpegPs,
    Fonte::TaxaVariavel,
    Fonte::Oscilacao,
    Fonte::FaixaCheia,
    Fonte::Entrelacado,
    Fonte::PixelNaoQuadrado,
    Fonte::Giro90,
];

/// ffmpeg dos testes (`SICRO_FFMPEG` ou o do sistema). None = não há; com `SICRO_EXIGE_FFMPEG=1` é pânico.
pub fn cap() -> Option<Arc<Capacidades>> {
    todo!()
}

/// Caminho da fonte, gerada uma vez por execução dos testes (pasta temporária do processo).
pub fn gerar(fonte: Fonte) -> Result<PathBuf> {
    todo!()
}

/// Amostras reais de gravador (Dahua, Hikvision, LVF, IFV) presentes em `p1b/dl/dvr/amostras`; vazio se não houver.
pub fn amostras_de_gravador() -> Vec<PathBuf> {
    todo!()
}

/// Pasta temporária nova para um teste (apagada no fim do processo).
pub fn pasta_temporaria(nome: &str) -> PathBuf {
    todo!()
}
