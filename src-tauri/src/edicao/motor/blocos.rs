//! Divisão em blocos e junção final (§5.8). Dono: P0.6a.

use super::plano::{Bloco, Canvas, Juncao, PassoAudio};
use super::saida::Codificacao;
use super::Alvo;
use crate::edicao::modelo::{LinhaResolvida, ProjetoEdicao};

/// Limites de um bloco: entradas de mídia e duração (s).
pub const MAX_ENTRADAS: usize = 12;
pub const MAX_DURACAO_S: f64 = 300.0;

/// Janela de slots de um bloco: [de_q, de_q + n_q).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JanelaBloco {
    pub de_q: u64,
    pub n_q: u32,
}

/// Divide o programa em blocos: em corte seco quando houver, senão em fronteira de slot fora de transição.
/// `Alvo::Previa` e `Alvo::Quadro` dão uma janela só.
pub fn dividir(p: &ProjetoEdicao, linha: &LinhaResolvida, alvo: &Alvo) -> Vec<JanelaBloco> {
    todo!()
}

/// SHA-256 do bloco: grafo, argumentos, SHA-256 das mídias, `hash` dos pedidos de camada e SHA-256 do ffmpeg.
pub fn chave(grafo: &str, args: &[String], midias: &[String], camadas: &[String], ffmpeg_sha256: &str) -> String {
    todo!()
}

/// Lista do concat e argumentos do mux final.
pub fn juncao(blocos: &[Bloco], canvas: &Canvas, cod: &Codificacao, audio: Option<&PassoAudio>, alvo: &Alvo) -> Juncao {
    todo!()
}
