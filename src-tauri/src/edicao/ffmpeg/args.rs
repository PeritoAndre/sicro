//! Argumentos e escapes (§5.3, regras fixas do gerador): grafo sempre em arquivo, só nomes simples. Dono: P0.4.

use std::path::Path;

use super::sonda::Capacidades;
use crate::error::Result;

/// Argumentos globais de toda execução de render: `-hide_banner -nostdin -y -loglevel verbose -copyts`.
pub fn globais() -> Vec<String> {
    todo!()
}

/// Argumentos de reprodutibilidade da saída (§5.7): `-fflags +bitexact … -map_metadata -1 -map_chapters -1`.
pub fn bitexact() -> Vec<String> {
    todo!()
}

/// Par de argumentos que entrega o grafo em arquivo, conforme a sonda: `-/filter_complex <nome>` ou `-filter_complex_script <nome>`.
pub fn grafo_em_arquivo(cap: &Capacidades, nome: &str) -> Vec<String> {
    todo!()
}

/// Grava um arquivo de texto auxiliar (grafo, `.ffconcat`) em UTF-8, sem BOM, na pasta de trabalho.
pub fn gravar_texto(pasta: &Path, nome: &str, conteudo: &str) -> Result<()> {
    todo!()
}

/// Valor de opção escapado para dentro de um grafo de filtros.
pub fn escapar_filtro(valor: &str) -> String {
    todo!()
}

/// Caminho escapado para uma linha `file '…'` de `.ffconcat`.
pub fn escapar_concat(caminho: &str) -> String {
    todo!()
}

/// Nome simples válido para rótulo e arquivo auxiliar (`c003v`, `b003.txt`): prefixo + contador com 3 dígitos + sufixo.
pub fn nome(prefixo: &str, n: usize, sufixo: &str) -> String {
    todo!()
}

/// Linha de comando inteira em caracteres (limite do Windows: 32 767).
pub fn tamanho_da_linha(programa: &Path, args: &[String]) -> usize {
    todo!()
}
