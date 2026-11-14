//! Cor (§3.3): leitura de "#rrggbb[aa]", RGB → YUV BT.709 limitada e as receitas de filtro. Dono: P0.3a.

use super::modelo::{Faixa, Matriz};
use crate::error::Result;

/// sRGB, alfa reto, 8 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Lê "#rrggbb" (alfa ff) ou "#rrggbbaa". Erro `cor` fora do formato.
pub fn ler(cor: &str) -> Result<Rgba> {
    todo!()
}

/// (Y, U, V) em BT.709, faixa limitada, 8 bits.
pub fn yuv_bt709(c: Rgba) -> (u8, u8, u8) {
    todo!()
}

/// Matriz presumida de fonte sem etiqueta: altura ≤ 576 → BT.601, senão BT.709.
pub fn matriz_presumida(altura: u32) -> Matriz {
    todo!()
}

/// Cadeia de filtros que normaliza a fonte para yuv420p BT.709 limitada (texto de grafo, sem vírgula nas pontas).
pub fn filtro_fonte(matriz: Matriz, faixa: Faixa) -> String {
    todo!()
}

/// Cadeia de filtros de uma camada RGBA para yuva420p BT.709 limitada.
pub fn filtro_camada() -> String {
    todo!()
}

/// `lutyuv=y=<Y>:u=<U>:v=<V>` da cor cheia (sólido do `ocultar`, fundo).
pub fn filtro_cor_cheia(c: Rgba) -> String {
    todo!()
}
