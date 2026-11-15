//! IA (§6.5, §6.6): sessão única do onnxruntime com GPU e queda para CPU, aceleração por GPU (item do
//! Gerenciador, autoteste em processo filho) e registro de uso por exportação. Dono: P2.1 (`contratos-fase2.md`).

pub mod gpu;
pub mod pacote;
pub mod sessao;
pub mod uso;

#[cfg(test)]
pub mod testes;
#[cfg(test)]
mod aceite;

pub use sessao::{criar_sessao, criar_sessao_com, criar_sessao_de_bytes, Modelo, Opcoes, Preferencia, Provedor, SessaoIa};

use crate::commands::settings_commands::AiSettings;

/// Código do erro de toda sessão com o interruptor "Usar modelos de IA" desligado (§6.6).
pub const IA_DESLIGADA: &str = crate::edicao::mensagens::codigo::IA_DESLIGADA;

/// Interruptor geral (§6.6). Desligado: nenhuma sessão de modelo, "Transcrever" e locutores somem; a mancha segue.
pub fn ligada(ai: &AiSettings) -> bool {
    !ai.modelos_desligados
}

/// Erro `ia_desligada` quando o interruptor está desligado (transcrição e locutores, que não passam por
/// `criar_sessao`).
pub fn exigir_ligada(ai: &AiSettings) -> crate::error::Result<()> {
    if ligada(ai) {
        Ok(())
    } else {
        Err(crate::error::SicroError::Validation(IA_DESLIGADA.into()))
    }
}
