//! Lupa (§5.3 `split=<1 + lupas>`, §5.4): cópia da base ANTES do enquadramento (pixels da origem do clipe) →
//! `crop` com x,y em árvore de `t` (pares) → `scale` (interpolação do efeito) → `alphamerge` com o PNG da forma →
//! `overlay` no canvas. Moldura e ligação vão na camada de anotação (`camada/lupa.rs`). Dono: P1.2.

use crate::edicao::camada::formas::QuadroCamada;
use crate::edicao::modelo::{EfLupa, TamanhoPx};
use crate::edicao::motor::clipe::Montador;
use crate::error::Result;

/// Detalhe do `nao_suportado` enquanto a lupa não renderiza. Uma regra só: `validar` e `clipe::cadeia` perguntam
/// aqui (contratos (h) 1). None = renderiza.
pub fn recusa(e: &EfLupa) -> Option<&'static str> {
    let _ = e;
    Some("lupa")
}

/// Aplica uma lupa sobre `atual` (já no canvas, depois do holofote). `copia` = rótulo da cópia da base tirada antes do
/// enquadramento (px de `origem`); `quadros` = um por slot da janela, com o rastreio do alvo avaliado em
/// `rastreios[e.rastreio]` e `A(τ)` em `a`. Pede ao montador a camada da forma. Devolve o rótulo novo.
/// Só é chamada quando `recusa(e)` é None.
pub fn aplicar(m: &mut dyn Montador, atual: &str, copia: &str, e: &EfLupa, quadros: &[QuadroCamada], origem: TamanhoPx) -> Result<String> {
    let _ = (m, atual, copia, e, quadros, origem);
    todo!("P1.2")
}
