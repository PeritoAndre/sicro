//! Selos de edição na imagem (§2.1 `Selos`, §10.2 `feito.roteiro[].selos`): velocidade, ampliação e realce, mais o
//! selo fixo da média de quadros. Entram na camada global (`motor/sobrepor.rs`). Dono: P1.R.

use crate::edicao::modelo::{LinhaResolvida, ProjetoEdicao, TamanhoPx};
use crate::edicao::motor::plano::ItemCamada;
use crate::error::Result;

/// Itens dos selos nos slots [de_q, de_q + n_q) do bloco, um por clipe que pede selo, em px do canvas `canvas`
/// (`ItemCamada.de` relativo a `de_q`). Fase 0: nenhum (os selos ainda não são desenhados).
pub fn itens(p: &ProjetoEdicao, linha: &LinhaResolvida, de_q: u64, n_q: u32, canvas: TamanhoPx) -> Result<Vec<ItemCamada>> {
    let _ = (p, linha, de_q, n_q, canvas);
    Ok(Vec::new())
}

/// Nomes dos selos que o clipe `id` leva (os de `feito.roteiro[].selos`). Fase 0: nenhum.
pub fn do_clipe(p: &ProjetoEdicao, linha: &LinhaResolvida, id: &str) -> Vec<String> {
    let _ = (p, linha, id);
    Vec::new()
}
