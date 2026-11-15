//! Fusão entre duas âncoras (§6.2): corrida para a frente da primeira e para trás da segunda, média ponderada pela
//! distância a cada âncora. Usada por `arquivo::juntar` quando a faixa tem corridas dos dois lados. Dono: P2.2.

use super::AmostraCorrida;

/// As duas corridas da faixa `[de_pts, ate_pts]` (pts das duas âncoras) → uma amostra por quadro, em ordem de pts.
/// Quadro que só um lado tem fica com ele; nenhum lado = ausente.
pub fn fundir(frente: &[AmostraCorrida], tras: &[AmostraCorrida], de_pts: i64, ate_pts: i64) -> Vec<AmostraCorrida> {
    let _ = (frente, tras, de_pts, ate_pts);
    Vec::new()
}
