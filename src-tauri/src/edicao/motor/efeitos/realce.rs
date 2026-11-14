//! Efeitos de origem (§5.3, §5.4): `bwdif` (desentrelaçar), `vidstabtransform` (estabilizar), `lenscorrection`
//! (lente), `eq` (ajustes), `unsharp` (nitidez), `tmix` (média de quadros), entre o `ocultar` e o mapa de quadros.
//! O `.trf` sai por `Previa::Vidstab` (só com `trf_sha256` None) e o intermediário de origem por `edicao/origem.rs`.
//! Dono: P1.4.

use std::borrow::Cow;

use crate::edicao::modelo::{Efeito, Tolerante};
use crate::edicao::motor::clipe::Montador;
use crate::edicao::motor::efeitos::{self, Familia};
use crate::edicao::motor::FonteInfo;
use crate::edicao::quadros::TabelaQuadros;
use crate::error::Result;

/// Detalhe do `nao_suportado` de um efeito de origem que o motor ainda não aplica; None = aplica (ou não é de
/// origem). `com_fonte` = clipe que lê quadros da fonte (trecho, congelado); foto e título passam `false`.
/// Uma regra só: `validar` e `clipe::cadeia` perguntam aqui (contratos (h) 1).
pub fn recusa(e: &Efeito, com_fonte: bool) -> Option<&'static str> {
    let _ = com_fonte;
    (efeitos::familia(e) == Familia::Origem).then(|| efeitos::tipo(e))
}

/// Tabela que o mapa de quadros do clipe usa: a da fonte; com `desentrelacar` por campo, uma entrada por campo
/// (base dobrada, §5.4). Quem chama: `tempo::resolver` (via `medir`) e `clipe::cadeia`. Fase 0: a da fonte.
pub fn tabela<'a>(efeitos: &[Tolerante<Efeito>], fonte: &'a TabelaQuadros) -> Cow<'a, TabelaQuadros> {
    let _ = efeitos;
    Cow::Borrowed(fonte)
}

/// O que a cadeia lê da fonte para entregar os quadros `[ja, jb)` da tabela do clipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Leitura {
    /// Quadros da FONTE lidos (corte por pts, `showinfo`, máscara do `ocultar`): [de, ate). Média lê N−1 antes,
    /// desentrelace 2, estabilizar desde `analise.de_pts` (§5.3, "Janela").
    pub ler: (usize, usize),
    /// Quadros da fonte do clipe inteiro (recorte e raio da ocultação, que não podem variar com a janela).
    pub clipe: (usize, usize),
    /// Quadros que chegam ao `fps@` (a conferência espera `lidos + 1`).
    pub lidos: u32,
}

/// Leitura dos quadros `faixa` = [ja, jb) da tabela do clipe; `clipe` = [j_in, j_out) do mapa. Fase 0: a própria faixa.
pub fn leitura(efeitos: &[Tolerante<Efeito>], fonte: &TabelaQuadros, tabela: &TabelaQuadros, faixa: (usize, usize), clipe: (usize, usize)) -> Result<Leitura> {
    let _ = (efeitos, fonte, tabela);
    Ok(Leitura { ler: faixa, clipe, lidos: (faixa.1 - faixa.0) as u32 })
}

/// Há efeito de origem ligado que o motor aplica? (Fase 0: nunca: todos são recusados antes.)
pub fn ativos(efeitos: &[Tolerante<Efeito>], com_fonte: bool) -> bool {
    efeitos::ligados(efeitos, Familia::Origem).into_iter().any(|e| recusa(e, com_fonte).is_none())
}

/// Cadeia de origem sobre `base` (quadros `leitura.ler`, já na cor interna e ocultados). Devolve o rótulo com só os
/// quadros que o mapa usa (`leitura.lidos`), com os pts da tabela do clipe. Pode pedir `Previa::Vidstab` e arquivos
/// auxiliares ao montador. Só é chamada quando `ativos` é verdadeiro.
pub fn aplicar(m: &mut dyn Montador, base: &str, efeitos: &[Tolerante<Efeito>], fonte: &FonteInfo, leitura: &Leitura) -> Result<String> {
    let _ = (m, base, efeitos, fonte, leitura);
    todo!("P1.4")
}
