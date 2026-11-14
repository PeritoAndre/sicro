//! Refazer e comparar (§10.4): lê só o retrato da exportação (O7), confere os hashes do manifesto, recompila com o
//! mesmo alvo, compara o plano, renderiza em `render/<tarefa>/` e compara `quadros.compostos.sha256` e o PCM.
//! Resultado na saída nova (`video_edit_outputs.refeita_de`) e no log `edicao.refazer`. Dono: P1.8.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::edicao::mensagens::codigo;
use crate::error::{Result, SicroError};

/// Resultado da comparação. `Diferente.tempo` em s do programa (o slot `primeiro_q`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Comparacao {
    Igual,
    ConteudoIgual { diferencas: Vec<String> },
    Diferente { primeiro_q: u64, tempo: f64 },
    NaoComparavel { motivo: String },
}

/// `edicao_refazer`: põe na fila de render uma tarefa `Refazer` da saída `saida_id` e devolve o id dela.
/// Fase 0: `nao_suportado`.
pub fn pedir(ws: &Path, saida_id: &str) -> Result<String> {
    let _ = (ws, saida_id);
    Err(SicroError::Validation(format!("{}: refazer", codigo::NAO_SUPORTADO)))
}
