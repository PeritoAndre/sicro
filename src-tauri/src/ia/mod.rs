//! IA (§6.5, §6.6): sessão única do onnxruntime com GPU e queda para CPU, aceleração por GPU (item do
//! Gerenciador, autoteste em processo filho) e registro de uso por exportação. Dono: P2.1 (`contratos-fase2.md`).
//! Abertura da Fase 2: tipos e assinaturas; os corpos recusam com `nao_suportado: ia` até o P2.1 entregar.

pub mod gpu;
pub mod sessao;
pub mod uso;

#[cfg(test)]
pub mod testes;

pub use sessao::{criar_sessao, criar_sessao_com, criar_sessao_de_bytes, Modelo, Opcoes, Preferencia, Provedor, SessaoIa};

use crate::commands::settings_commands::AiSettings;

/// Código do erro de toda sessão com o interruptor "Usar modelos de IA" desligado (§6.6).
pub const IA_DESLIGADA: &str = crate::edicao::mensagens::codigo::IA_DESLIGADA;

/// Interruptor geral (§6.6). Desligado: nenhuma sessão de modelo, "Transcrever" e locutores somem; a mancha segue.
pub fn ligada(ai: &AiSettings) -> bool {
    !ai.modelos_desligados
}
