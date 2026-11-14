//! Máscaras (§5.5): cinza 8 bits, branco = aplica; borda suave cresce PARA FORA. Dono: P0.5.

use super::formas::Primitiva;
use super::Bitmap;
use crate::edicao::modelo::TamanhoPx;
use crate::error::Result;

/// Máscara das primitivas `regiao` num cinza do tamanho de `caixa_px` ([x, y, w, h], px de `espaco`).
/// `espaco` = fonte exibida (máscara por quadro lido) ou canvas (por slot). `inverter` já aplicado.
pub fn mascara(regioes: &[Primitiva], espaco: TamanhoPx, caixa_px: [u32; 4]) -> Result<Bitmap> {
    todo!()
}

/// União, em px pares de `espaco`, das caixas (forma + margem + borda suave) das regiões de todos os quadros: [x, y, w, h].
pub fn uniao_px(quadros: &[Vec<Primitiva>], espaco: TamanhoPx) -> Option<[u32; 4]> {
    todo!()
}

/// Forma PNG da lupa (círculo ou retângulo) para o `alphamerge`: cinza `lado`×`lado` px.
pub fn forma_lupa(forma: crate::edicao::modelo::FormaLupa, lado: u32) -> Result<Bitmap> {
    todo!()
}
