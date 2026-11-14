//! Camada global (§5.3, passo 6): títulos e imagens sobrepostos, por cima da principal já junta. Dono: P0.6a.

use super::grafo::Grafo;
use super::plano::{Entrada, PedidoCamada};
use super::{Alvo, Contexto};
use crate::edicao::modelo::{LinhaResolvida, ProjetoEdicao};
use crate::error::Result;

/// O que a camada global põe no bloco: rótulo de saída, a entrada da camada e o pedido de rasterização (None = nada a sobrepor).
#[derive(Debug, Clone, PartialEq)]
pub struct Sobreposto {
    pub saida: String,
    pub entrada: Option<Entrada>,
    pub camada: Option<PedidoCamada>,
}

/// Aplica sobre `base` os clipes livres visíveis nos slots [de_q, de_q + n_q).
pub fn aplicar(
    g: &mut Grafo,
    base: &str,
    p: &ProjetoEdicao,
    linha: &LinhaResolvida,
    de_q: u64,
    n_q: u32,
    indice_entrada: usize,
    ctx: &Contexto,
    alvo: &Alvo,
) -> Result<Sobreposto> {
    todo!()
}
