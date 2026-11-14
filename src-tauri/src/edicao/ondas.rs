//! Forma de onda (`receitas-audio-saida.md` §10): `-ac 1 -ar 8000 -f s16le` por pipe, mínimo e máximo por balde de
//! 10 ms, em cache por mídia (`ondas/<sha256>.pk`), feita na fila de fundo (`TipoFundo::Onda`). Dono: P1.9.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::motor::FonteInfo;
use crate::error::Result;

/// Baldes por segundo da onda.
pub const POR_S: u32 = 100;

/// Picos do som da mídia: `min[i]`/`max[i]` = amostras extremas (s16, mono) do balde `i`, que começa em
/// `inicio_s + i / por_s` (tempo de fonte).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Onda {
    pub inicio_s: f64,
    pub por_s: u32,
    pub min: Vec<i16>,
    pub max: Vec<i16>,
}

/// Onda já no cache; None = falta (quem chama pede à fila de fundo).
pub fn consultar(cache: &Path, sha256: &str) -> Option<Onda> {
    let _ = (cache, sha256);
    todo!("P1.9")
}

/// Calcula e guarda a onda (bloqueia a thread). `progresso` em 0..1.
pub fn produzir(cap: &Capacidades, cache: &Path, fonte: &FonteInfo, cancelar: &AtomicBool, progresso: &mut dyn FnMut(f32)) -> Result<Onda> {
    let _ = (cap, cache, fonte, cancelar, progresso);
    todo!("P1.9")
}

/// `edicao_onda`: a onda do cache, ou None depois de pedir à fila de fundo (chave `onda:<sha256>`; o fim chega pelo
/// evento `edicao:fundo` e a tela pede de novo).
pub fn pedir(ws: &Path, cache: &Path, sha256: &str) -> Result<Option<Onda>> {
    let _ = (ws, cache, sha256);
    todo!("P1.9")
}
