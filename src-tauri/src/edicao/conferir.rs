//! Conferência da saída e hashes de conteúdo (D11, §5.8). Dono: P0.7.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use super::ffmpeg::sonda::Capacidades;
use crate::error::Result;

/// Resultado de decodificar o arquivo entregue: quadros contados, erros do decodificador,
/// hash do decodificado e o arquivo `quadros.decodificados.sha256` (um hash por quadro).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConferenciaSaida {
    pub quadros: u64,
    pub erros: u32,
    pub decodificado_sha256: String,
    pub decodificados: PathBuf,
}

/// Decodifica `arquivo` em CPU com `-xerror`, grava os hashes por quadro em `trabalho` e exige `total_q` quadros sem erro.
/// `progresso` recebe os quadros já decodificados.
pub fn conferir(
    cap: &Capacidades,
    arquivo: &Path,
    total_q: u64,
    trabalho: &Path,
    cancelar: &AtomicBool,
    progresso: &mut dyn FnMut(u64),
) -> Result<ConferenciaSaida> {
    todo!()
}

/// Coluna de hash de um arquivo `framehash` (uma linha por quadro; as colunas de pts ficam de fora).
pub fn coluna_de_hash(framehash: &str) -> Vec<String> {
    todo!()
}

/// Hash de conteúdo: SHA-256 do texto com um hash por linha (terminada em `\n`), na ordem dos slots.
pub fn hash_de_conteudo(hashes: &[String]) -> String {
    todo!()
}

/// Junta os `framehash` dos blocos (em ordem), grava `destino` (um hash por slot) e devolve o hash do composto.
pub fn hash_do_composto(framehashes: &[PathBuf], destino: &Path) -> Result<String> {
    todo!()
}
