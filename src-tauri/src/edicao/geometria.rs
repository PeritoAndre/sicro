//! Espaços (§3.2), transformação origem → canvas `A(τ)`, caixas e enquadre.
//! Espelho no front: `modelo/geometria.ts`. Dono: P0.3a.

use serde::{Deserialize, Serialize};

use super::modelo::{Caixa, Efeito, Ponto, TamanhoPx, Tolerante};

/// Afim em 0..1, na ordem do canvas 2D (`setTransform(a, b, c, d, e, f)`):
/// x' = a·x + c·y + e; y' = b·x + d·y + f. É o `QuadroPalco.A` do front.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Afim(pub [f64; 6]);

impl Afim {
    /// Não muda nada.
    pub const IDENTIDADE: Afim = Afim([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// Ponto transformado (0..1 da origem → 0..1 do canvas).
    pub fn ponto(&self, p: Ponto) -> Ponto {
        todo!()
    }

    /// Caixa envolvente da caixa transformada.
    pub fn caixa(&self, c: Caixa) -> Caixa {
        todo!()
    }

    /// Composição: aplica `self` e depois `depois`.
    pub fn seguida_de(&self, depois: &Afim) -> Afim {
        todo!()
    }

    /// Inversa (canvas → origem); None se degenerada.
    pub fn inversa(&self) -> Option<Afim> {
        todo!()
    }
}

/// `A(τ)`: giro e espelho do `girar` + região do `enquadramento` em `tau` (s, base do clipe) + ajuste.
/// `origem` = tamanho exibido do espaço de origem (px); `canvas` = saída (px).
pub fn a_de(efeitos: &[Tolerante<Efeito>], tau: f64, origem: TamanhoPx, canvas: TamanhoPx) -> Afim {
    todo!()
}

/// Região do enquadramento em `tau` (0..1 da origem já girada); sem o efeito, a imagem inteira.
pub fn regiao_em(efeitos: &[Tolerante<Efeito>], tau: f64) -> Caixa {
    todo!()
}

/// Caixa da origem (0..1) levada ao canvas (0..1). Espelho de `caixaNoCanvas`.
pub fn caixa_no_canvas(a: &Afim, c: Caixa) -> Caixa {
    todo!()
}

/// Identificador do espaço de origem do clipe: "original" ou "origem:<sha256>" dos efeitos geométricos (§3.2).
pub fn espaco_de_origem(midia_sha256: &str, efeitos: &[Tolerante<Efeito>]) -> String {
    todo!()
}

/// Caixa 0..1 em pixels pares dentro de `tamanho`: [x, y, w, h], que cobre a caixa inteira.
pub fn caixa_px_par(c: Caixa, tamanho: TamanhoPx) -> [u32; 4] {
    todo!()
}

/// Caixa com a margem aplicada (fração do lado), limitada a 0..1.
pub fn com_margem(c: Caixa, margem: f64) -> Caixa {
    todo!()
}
