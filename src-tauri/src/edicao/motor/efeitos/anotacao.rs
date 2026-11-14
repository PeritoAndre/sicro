//! Anotações (§5.4): `overlay` da camada por slot sobre a base já enquadrada. Dono: P0.6a.

use crate::edicao::motor::grafo::Grafo;
use crate::error::Result;

/// Sobrepõe a camada (`camada` = rótulo do fluxo RGBA, já na cor de camada) em (`x`, `y`) px pares e devolve o rótulo de saída.
pub fn sobrepor(g: &mut Grafo, base: &str, camada: &str, x: u32, y: u32) -> Result<String> {
    todo!()
}

/// Cadeia de entrada de uma camada por slot: `settb`, `setpts=N`, `trim=end_frame=<n_q>` e a cor de camada (ou `format=gray`).
pub fn entrada_por_slot(g: &mut Grafo, entrada: &str, fps: crate::edicao::modelo::Racional, n_q: u32, cinza: bool) -> String {
    todo!()
}

/// Cadeia de entrada de uma máscara por quadro lido: `format=gray`, `settb=<tb>` e pts imposto por árvore sobre `N`.
pub fn entrada_por_quadro(g: &mut Grafo, entrada: &str, tb: crate::edicao::modelo::Racional, pts: &[i64]) -> String {
    todo!()
}
