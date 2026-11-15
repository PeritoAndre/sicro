//! "Conferir ocultações" (§5.10, §10): os intervalos ocultados no ARQUIVO exportado, em tempo do arquivo, com os
//! trechos fracos (quadros `incerto` e interpolados) primeiro. Lê só o retrato da saída (O7). Dono: P2.6.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::modelo::ConteudoOculto;
use crate::error::{Result, SicroError};

/// Como o quadro fraco foi coberto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComoFraco {
    Incerto,
    Interpolado,
}

/// Faixa de quadros fracos, em segundos do arquivo exportado.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FaixaFraca {
    pub de_s: f64,
    pub ate_s: f64,
    pub como: ComoFraco,
}

/// Um intervalo em que uma ocultação aparece no arquivo (segundos do arquivo, `[de_s, ate_s)`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrechoConferencia {
    pub efeito: String,
    pub conteudo: ConteudoOculto,
    pub de_s: f64,
    pub ate_s: f64,
    pub fracos: Vec<FaixaFraca>,
}

/// Trechos da saída `saida_id` (estado "a conferir" ou concluída), ordenados: com fracos primeiro, depois por
/// tempo. Saída sem ocultação = lista vazia. Abertura: recusa `nao_suportado: conferencia` até o P2.6 entregar.
pub fn trechos(ws: &Path, saida_id: &str) -> Result<Vec<TrechoConferencia>> {
    let _ = (ws, saida_id);
    Err(SicroError::Validation(format!("{}: conferencia", super::mensagens::codigo::NAO_SUPORTADO)))
}
