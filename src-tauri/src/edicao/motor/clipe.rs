//! Cadeia de vídeo de um clipe da principal (§5.3): entradas, filtros, pedidos de camada e prévias. Dono: P0.6a.

use super::grafo::Grafo;
use super::plano::{ArquivoTexto, ConferirClipe, Entrada, PedidoCamada, Previa};
use super::{Alvo, Contexto};
use crate::edicao::modelo::{ClipePrincipal, ClipeResolvido, ProjetoEdicao};
use crate::error::Result;

/// Janela de slots do bloco dentro do clipe: [de, ate) em slots locais do clipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Janela {
    pub de: u32,
    pub ate: u32,
}

/// O que um clipe põe no bloco. `saida` = rótulo do vídeo pronto (W×H, yuv420p, `ate − de` quadros);
/// `primeira_entrada` = índice da primeira entrada dele no bloco.
#[derive(Debug, Clone, PartialEq)]
pub struct CadeiaClipe {
    pub saida: String,
    pub entradas: Vec<Entrada>,
    pub camadas: Vec<PedidoCamada>,
    pub previas: Vec<Previa>,
    pub auxiliares: Vec<ArquivoTexto>,
    pub conferir: Option<ConferirClipe>,
}

/// Escreve no grafo a cadeia do clipe para a janela. `n` = número do clipe no bloco (rótulos `c003…`);
/// `primeira_entrada` = índice que a primeira entrada dele terá no ffmpeg.
pub fn cadeia(
    g: &mut Grafo,
    p: &ProjetoEdicao,
    clipe: &ClipePrincipal,
    res: &ClipeResolvido,
    janela: Janela,
    n: usize,
    primeira_entrada: usize,
    ctx: &Contexto,
    alvo: &Alvo,
) -> Result<CadeiaClipe> {
    todo!()
}

/// Argumentos de busca de uma leitura que começa no quadro `j_in`: Some((-ss em s, relativo ao início do arquivo))
/// ou None quando a regra do §5.3 proíbe a busca (a menos de 10 s do início, sem chave conhecida, `SemBusca`).
pub fn busca(tab: &crate::edicao::quadros::TabelaQuadros, inicio_s: f64, j_in: usize) -> Option<f64> {
    todo!()
}
