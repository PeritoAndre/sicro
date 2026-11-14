//! Pacote do júri (§10.1): `Relatório.pdf` (a raiz), `mapa.csv`, `quadros.*.sha256`, `audio.flac`, `projeto/`,
//! `originais/` e `SHA256SUMS.txt` (de tudo menos o PDF), na pasta da exportação, depois do `tecnico.json` final.
//! Chamado pela publicação (`fila::publicar`) e pela conferência (`concluir_conferencia`). Dono: P1.8.

use std::path::Path;

use serde_json::Value;

use crate::edicao::executar::{Ambiente, Resultado};
use crate::edicao::modelo::Pacote;
use crate::edicao::motor::plano::Plano;
use crate::error::Result;

/// O que o pacote precisa. `pasta` = pasta da exportação; `base` = nome saneado dos arquivos; `tecnico` = o final.
pub struct PedidoPacote<'a> {
    pub ws: &'a Path,
    pub pasta: &'a Path,
    pub base: &'a str,
    pub pacote: Pacote,
    pub plano: &'a Plano,
    pub res: &'a Resultado,
    pub amb: &'a Ambiente,
    pub tecnico: &'a Value,
    pub projeto: &'a Value,
}

/// Monta o pacote; devolve os arquivos criados (relativos à pasta, com `/`). Somente leitura no fim.
pub fn montar(p: &PedidoPacote, cancelar: &std::sync::atomic::AtomicBool) -> Result<Vec<String>> {
    let _ = (p, cancelar);
    todo!("P1.8")
}

/// Texto do `SHA256SUMS.txt` (formato do `sha256sum`), em ordem de caminho, sem o PDF.
pub fn sha256sums(pasta: &Path, arquivos: &[String]) -> Result<String> {
    let _ = (pasta, arquivos);
    todo!("P1.8")
}
