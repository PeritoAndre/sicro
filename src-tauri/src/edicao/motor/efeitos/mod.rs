//! Efeito → mecanismo (§5.4). Fase 0: enquadramento, região (ocultar) e anotação;
//! `lupa.rs` e `realce.rs` nascem na Fase 1. Dono: P0.6a.

use crate::edicao::modelo::{Efeito, Tolerante};

pub mod anotacao;
pub mod enquadramento;
pub mod regiao;

/// Família do efeito: decide o ponto da cadeia em que ele entra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Familia {
    Origem,
    Enquadre,
    Regiao,
    Anotacao,
}

/// Família de um efeito.
pub fn familia(e: &Efeito) -> Familia {
    todo!()
}

/// Efeitos conhecidos e ligados de uma família, na ordem de aplicação.
pub fn ligados(efeitos: &[Tolerante<Efeito>], familia: Familia) -> Vec<&Efeito> {
    todo!()
}

/// O efeito está ativo no tempo `t` (s, base do clipe)? `ativo` None = o clipe inteiro.
pub fn ativo_em(e: &Efeito, t: f64) -> bool {
    todo!()
}
