//! Texto pelo Typst (§5.5): sprites em `sprites/<hash>.png` e atlas do carimbo em `atlas/<hash>.png`.
//! Conteúdo, estilo e largura chegam como dado, nunca como código. Dono: P0.5.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::edicao::modelo::{BlocoTexto, EstiloTexto, TamanhoPx};
use crate::edicao::motor::plano::ImagemCamada;
use crate::error::Result;

/// Pedido de sprite (espelho do TS): `largura_max` em fração da largura do canvas; `altura_canvas` em px;
/// `fundo` = desenhar a caixa de fundo dos blocos.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PedidoSprite {
    pub blocos: Vec<BlocoTexto>,
    pub largura_max: f64,
    pub altura_canvas: u32,
    pub fundo: bool,
}

/// Sprite pronto: caminho relativo ao cache da Edição do caso e tamanho em px.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sprite {
    pub relativo: String,
    pub w: u32,
    pub h: u32,
}

/// Atlas do carimbo: `glifos` na ordem das células (cada uma `celula` px), em linha.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Atlas {
    pub relativo: String,
    pub celula: TamanhoPx,
    pub glifos: String,
}

/// De onde `formas::primitivas` tira o que não cabe no efeito: sprites, atlas e tamanho das imagens
/// (o cache real, ou um falso nos testes). No front são os três últimos métodos de `QuadroPalco`.
pub trait FonteSprites {
    /// Sprite do pedido.
    fn sprite(&self, pedido: &PedidoSprite) -> Result<Sprite>;
    /// Atlas do carimbo nesse estilo e altura de canvas (px).
    fn atlas(&self, estilo: &EstiloTexto, altura_canvas: u32) -> Result<Atlas>;
    /// Tamanho (px) da imagem com esse `MidiaRef.id`; None = não é imagem conhecida.
    fn imagem(&self, midia: &str) -> Option<TamanhoPx>;
}

/// `FonteSprites` sobre o cache em disco (`cache` = pasta da Edição do caso) e as imagens do pedido.
pub struct SpritesEmCache<'a> {
    pub cache: &'a Path,
    pub imagens: &'a [ImagemCamada],
}

impl FonteSprites for SpritesEmCache<'_> {
    fn sprite(&self, pedido: &PedidoSprite) -> Result<Sprite> {
        sprite(self.cache, pedido)
    }
    fn atlas(&self, estilo: &EstiloTexto, altura_canvas: u32) -> Result<Atlas> {
        atlas_carimbo(self.cache, estilo, altura_canvas)
    }
    fn imagem(&self, midia: &str) -> Option<TamanhoPx> {
        self.imagens.iter().find(|i| i.midia == midia).map(|i| i.tamanho)
    }
}

/// Rasteriza (ou acha no cache) o sprite do pedido. A chave inclui a altura do canvas.
pub fn sprite(cache: &Path, pedido: &PedidoSprite) -> Result<Sprite> {
    todo!()
}

/// Atlas de glifos do carimbo: dígitos, separadores e os prefixos "Original", "Câmera", "Quadro", "Vídeo".
pub fn atlas_carimbo(cache: &Path, estilo: &EstiloTexto, altura_canvas: u32) -> Result<Atlas> {
    todo!()
}

/// SHA-256 de cada fonte embutida: (nome, sha256). Entra no hash dos pedidos e em `execucao.fontes`.
pub fn fontes() -> Vec<(String, String)> {
    todo!()
}
