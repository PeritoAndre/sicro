//! Junção dos clipes dentro do bloco e catálogo de transições (§5.4). Fase 0: corte e `escurecer`. Dono: P0.6a.

use super::grafo::Grafo;
use crate::edicao::modelo::Transicao;
use crate::error::Result;

/// Catálogo de transições aceitas (`Transicao.tipo`); fora dele = erro `transicao`. As do `xfade` entram no P1.1.
/// Se a transição sobrepõe os clipes, quem diz é `tempo::transicao_sobrepoe`.
pub const TRANSICOES: &[&str] = &["escurecer"];

/// Um clipe pronto para juntar: rótulo do vídeo, slots e as transições de entrada e de saída (em slots).
#[derive(Debug, Clone, PartialEq)]
pub struct Peca {
    pub rotulo: String,
    pub n_q: u32,
    pub entrada: Option<(Transicao, u32)>,
    pub saida: Option<(Transicao, u32)>,
}

/// Junta as peças na ordem e devolve o rótulo do vídeo do bloco (`fps` depois do `concat`).
pub fn montar(g: &mut Grafo, pecas: &[Peca], fps: crate::edicao::modelo::Racional) -> Result<String> {
    todo!()
}
