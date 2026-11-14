//! Títulos (cartela, abertura, terço inferior, encerramento) como primitivas. Dono: P0.5.

use super::formas::{Primitiva, QuadroCamada};
use super::texto::{FonteSprites, PedidoSprite};
use crate::edicao::modelo::{TamanhoPx, Titulo};
use crate::error::Result;

/// Primitivas do título no quadro: fundo (cor ou imagem) e um sprite por bloco, em 0..1 do canvas.
pub fn primitivas(titulo: &Titulo, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    todo!()
}

/// Pedido de sprite de um bloco do título para esse canvas.
pub fn pedido(titulo: &Titulo, bloco: usize, canvas: TamanhoPx) -> PedidoSprite {
    todo!()
}
