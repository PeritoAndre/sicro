//! Relatórios em PDF sem programa externo (§10.3; protótipo e testes em `p4/integracao/src-tauri/src/relatorio_pdf/`):
//! modelo Typst embutido (`juri.typ`) + dados em JSON, fontes dentro do binário, nada lê relógio nem sistema
//! (mesma entrada, mesmos bytes). O texto do perito entra como dado, nunca como código Typst. Dono: P1.R.

use chrono::{DateTime, Utc};

use crate::error::Result;

/// Modelo do documento.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modelo {
    /// Relatório do pacote do júri.
    Juri,
}

/// O que vira o PDF. `dados` vira `dados.json` (contrato `relatorio::juri::ESQUEMA_JURI`); `arquivos` = (caminho
/// virtual com `/`, bytes): miniaturas (`img/1.jpg`) e anexos (`tecnico.json`, `projeto.json`); `momento` = data do
/// documento (a do render); `ident` = SHA-256 do vídeo; `pdf_a` = PDF/A-3a + PDF/UA-1 (false = PDF/A-2b sem anexos).
pub struct Entrada {
    pub modelo: Modelo,
    pub dados: serde_json::Value,
    pub arquivos: Vec<(String, Vec<u8>)>,
    pub momento: DateTime<Utc>,
    pub ident: String,
    pub pdf_a: bool,
}

/// PDF pronto; `avisos` = o que mostrar ao perito (caractere sem glifo trocado por □).
#[derive(Debug)]
pub struct Saida {
    pub pdf: Vec<u8>,
    pub paginas: usize,
    pub avisos: Vec<String>,
}

/// Gera o PDF (bloqueia: chamar de `spawn_blocking`). Ao fim, `typst::comemo::evict(0)`.
pub fn gerar(e: &Entrada) -> Result<Saida> {
    let _ = e;
    todo!("P1.R")
}
