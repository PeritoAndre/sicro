//! Áudio do programa inteiro numa passada (§5.9). Dono: P0.6b.
//! Decisão O6: a passada de áudio e a de medição rodam SEMPRE com `-cpuflags 0`, antes das entradas.

use super::plano::{PassoAudio, Previa};
use super::{Alvo, Contexto};
use crate::edicao::modelo::{LinhaResolvida, Normalizar, ProjetoEdicao};
use crate::error::Result;

/// Primeiros argumentos de toda linha de áudio (O6).
pub const CPUFLAGS: [&str; 2] = ["-cpuflags", "0"];

/// Medição do `loudnorm` (passada 1): volume integrado (LUFS), pico real (dBTP), faixa (LU) e limiar (LUFS).
/// `integrado` None = `-inf` (silêncio): não normaliza.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Loudnorm {
    pub integrado: Option<f64>,
    pub pico: Option<f64>,
    pub faixa: f64,
    pub limiar: Option<f64>,
}

/// Monta o passo de áudio do programa e a prévia de medição. (None, vazio) quando o alvo não tem som (`Alvo::Quadro`).
/// Programa sem `saida.audio`: trilha de silêncio.
pub fn montar(p: &ProjetoEdicao, linha: &LinhaResolvida, ctx: &Contexto, alvo: &Alvo) -> Result<(Option<PassoAudio>, Vec<Previa>)> {
    todo!()
}

/// Lê o JSON que o `loudnorm=print_format=json` imprime no stderr.
pub fn ler_loudnorm(stderr: &str) -> Result<Loudnorm> {
    todo!()
}

/// Ganho em dB da passada 2: min(alvo − integrado, pico alvo − pico medido); None = não normaliza.
pub fn ganho_db(medido: &Loudnorm, alvo: Option<&Normalizar>) -> Option<f64> {
    todo!()
}

/// Grafo final: `PassoAudio.grafo` com `MARCA_GANHO` trocada pelo ganho (0 quando None).
pub fn grafo_com_ganho(passo: &PassoAudio, ganho_db: Option<f64>) -> String {
    todo!()
}
