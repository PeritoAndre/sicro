//! `tecnico.json` (§10.2), esquema `sicro.edicao.tecnico/2`: sai do MESMO `Plano` que gerou o render. Dono: P0.R.
//! `feito` é determinístico; `execucao` guarda o que varia. Nenhum caminho absoluto, nenhum nome de rastreio.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::edicao::executar::{Ambiente, Resultado};
use crate::edicao::motor::plano::Plano;
use crate::error::Result;

/// Valor do campo `esquema` do `tecnico.json`.
pub const ESQUEMA_TECNICO: &str = "sicro.edicao.tecnico/2";

/// Conferência das ocultações no arquivo exportado: feita ou não, por quem e quando (RFC 3339).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConferenciaOcultacoes {
    pub conferida: bool,
    pub por: String,
    pub em: String,
}

/// O que só se sabe ao fechar a exportação: SHA-256 do `projeto.json` do retrato, caminho do arquivo final
/// relativo ao caso e a conferência das ocultações (None = a exportação não tem ocultação nem silêncio).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Fecho {
    pub projeto_sha256: String,
    pub arquivo_rel: String,
    pub ocultacoes: Option<ConferenciaOcultacoes>,
}

/// O `tecnico.json` inteiro: `{ esquema, feito, execucao, renderizado_em }`.
pub fn montar(plano: &Plano, res: &Resultado, amb: &Ambiente, fecho: &Fecho) -> Result<Value> {
    todo!()
}

/// Só a parte `feito` (mesmo projeto e mesmas fontes = mesmos bytes).
pub fn feito(plano: &Plano, amb: &Ambiente, fecho: &Fecho) -> Result<Value> {
    todo!()
}

/// Bytes do JSON como vão para o disco (serialização estável: chaves em ordem, 2 espaços, `\n` no fim).
pub fn bytes(tecnico: &Value) -> Vec<u8> {
    todo!()
}

/// Grava `<Título>.tecnico.json` em `pasta` (com `create_new`) e devolve o SHA-256.
pub fn gravar(pasta: &Path, titulo: &str, tecnico: &Value) -> Result<String> {
    todo!()
}
