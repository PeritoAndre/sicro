//! Avaliação de um rastreio num quadro (§6.1). Espelho no front: `modelo/rastreio.ts`. Dono: P0.3a.
//! Ordem: âncora no quadro → amostra densa (ok ou incerta) → interpolação entre âncoras →
//! depois da última âncora segura a última (só origem manual) → antes da primeira, nada.

use serde::{Deserialize, Serialize};

use super::ArquivoDenso;
use crate::edicao::modelo::{Caixa, Rastreio};

/// Onde avaliar: tempo (s) na base do rastreio e, quando há mídia, o pts do quadro lido ou exibido.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Instante {
    pub t: f64,
    pub pts: Option<i64>,
}

/// De onde saiu a caixa.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Como {
    Ancora,
    Denso,
    Interpolado,
    Segurado,
}

/// Caixa (0..1, no espaço do rastreio) e como foi obtida; `incerto` = amostra densa de estado incerto.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Avaliado {
    pub caixa: Caixa,
    pub como: Como,
    pub incerto: bool,
}

/// Caixa do rastreio no instante; None = "nada" (sem cobertura). `margem` e `forma` entram depois.
pub fn avaliar(r: &Rastreio, denso: Option<&ArquivoDenso>, em: Instante) -> Option<Avaliado> {
    todo!()
}

/// O instante cai numa ausência aceita pelo perito?
pub fn em_ausencia(r: &Rastreio, t: f64) -> bool {
    todo!()
}
