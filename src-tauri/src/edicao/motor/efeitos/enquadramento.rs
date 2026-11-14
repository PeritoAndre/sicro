//! `girar` e `enquadramento` (§5.4): fixo por `crop` + `scale` + `pad`; animado por `perspective`. Dono: P0.6a.

use crate::edicao::modelo::{Efeito, TamanhoPx, Tolerante};
use crate::edicao::motor::plano::Canvas;
use crate::error::Result;

/// Filtros (separados por vírgula) que levam a origem (`origem` px) ao canvas W×H, para `n_q` slots.
pub fn filtros(efeitos: &[Tolerante<Efeito>], origem: TamanhoPx, canvas: &Canvas, n_q: u32) -> Result<String> {
    todo!()
}

/// O enquadramento do clipe é animado (usa `perspective`)?
pub fn animado(efeitos: &[Tolerante<Efeito>]) -> bool {
    todo!()
}
