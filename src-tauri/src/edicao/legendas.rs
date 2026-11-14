//! Degravação → legendas e trechos de fala (§5.1 `legendas.rs`; conferência §4): lê a degravação do som da mídia
//! no banco do caso (`audio_transcript_segments`) e devolve os trechos em TEMPO DE FONTE da mídia. A tela faz deles
//! `Legenda` (P1.6, "Da degravação…" das legendas) ou `Silencio` (P1.5, "Silenciar trecho → Da degravação…").
//! Dono: P1.6.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::edicao::modelo::OrigemLegendas;
use crate::error::Result;

/// Trecho de fala em tempo de fonte (s). `ia` = rascunho da IA não reescrito pelo perito (vira legenda `ia`, não
/// conferida); `locutor` vazio = sem locutor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrechoFala {
    pub de: f64,
    pub ate: f64,
    pub texto: String,
    pub locutor: Option<String>,
    pub ia: bool,
}

/// Degravação da mídia: de onde veio (vai para `TrilhaLegendas.origem`) e os trechos em ordem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Degravacao {
    pub origem: OrigemLegendas,
    pub trechos: Vec<TrechoFala>,
}

/// `edicao_degravacao`: degravação do som da mídia `sha256` do caso. Erro `degravacao_ausente` se não há.
pub fn degravacao(ws: &Path, sha256: &str) -> Result<Degravacao> {
    let _ = (ws, sha256);
    todo!("P1.6")
}
