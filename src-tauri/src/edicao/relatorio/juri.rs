//! Dados do relatório do júri (§10.3, contrato `sicro.relatorio_juri/2` = o `/1` de `pdf-relatorio.md` §8 mais
//! cobertura por gravação, resumo de IA, selos desligados e comandos de conferência), a partir do `tecnico.json`
//! FINAL; as frases vêm de `frases.rs`. Formatação pt-BR (vírgula decimal, datas, "0:07,500") fica aqui. Dono: P1.R.

use serde_json::Value;

use crate::edicao::modelo::ModoPdf;
use crate::error::Result;
use crate::relatorio_pdf::Entrada;

/// Esquema dos dados do PDF.
pub const ESQUEMA_JURI: &str = "sicro.relatorio_juri/2";

/// Miniatura de um item do roteiro: quadro final do primeiro slot (`Alvo::Quadro`, 480 px), em JPEG.
#[derive(Debug, Clone, PartialEq)]
pub struct Miniatura {
    pub n: u32,
    pub jpeg: Vec<u8>,
}

/// Arquivo do pacote listado no anexo A (o PDF é a raiz): caminho relativo à pasta da exportação e SHA-256.
#[derive(Debug, Clone, PartialEq)]
pub struct ArquivoDoPacote {
    pub rel: String,
    pub sha256: String,
}

/// `dados.json` do modelo, com todos os valores já em texto pronto para imprimir.
pub fn dados(tecnico: &Value, miniaturas: &[Miniatura], arquivos: &[ArquivoDoPacote]) -> Result<Value> {
    let _ = (tecnico, miniaturas, arquivos);
    todo!("P1.R")
}

/// Entrada do `relatorio_pdf::gerar`: dados, miniaturas, anexos (`tecnico.json` e `projeto.json`; nenhum em
/// `a2_sem_anexos`), `momento` = `renderizado_em` e `ident` = SHA-256 do vídeo (refazer devolve o mesmo PDF).
pub fn entrada(tecnico: &Value, dados: Value, miniaturas: &[Miniatura], projeto_json: &[u8], pdf: ModoPdf) -> Result<Entrada> {
    let _ = (tecnico, dados, miniaturas, projeto_json, pdf);
    todo!("P1.R")
}
