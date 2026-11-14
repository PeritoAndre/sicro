//! Rasterizador do SICRO (§5.5): de um `PedidoCamada` para PNGs + lista. Dono: P0.5
//! (`selos.rs` é do P1.R e `medidas.rs` do P1.3: não entram na Fase 0).
//! Anotação: PNG RGBA 8 bits, alfa reto. Máscara: PNG cinza 8 bits (branco = aplica).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use super::motor::plano::PedidoCamada;
use crate::error::Result;

pub mod formas;
pub mod lista;
pub mod mascara;
pub mod texto;
pub mod titulo;

/// Entra no `hash` de todo `PedidoCamada`: sobe quando o desenho muda um pixel.
pub const VERSAO_RASTERIZADOR: u32 = 1;

/// Pixels crus: `canais` = 1 (cinza) ou 4 (RGBA, alfa reto); `px` por linhas, sem preenchimento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub w: u32,
    pub h: u32,
    pub canais: u8,
    pub px: Vec<u8>,
}

impl Bitmap {
    /// PNG determinístico (mesma entrada, mesmos bytes).
    pub fn png(&self) -> Result<Vec<u8>> {
        todo!()
    }
}

/// Um PNG da camada: nome simples na pasta da camada e SHA-256 (vai para `execucao.camadas`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArquivoCamada {
    pub arquivo: String,
    pub sha256: String,
}

/// Camada rasterizada em `camadas/<hash>/` (ou `mascaras/<hash>/`). `lista` = `.ffconcat` com uma entrada por
/// slot ou por quadro lido; `fixa` = um PNG só (entra com `-loop 1`), sem lista.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CamadaPronta {
    pub id: String,
    pub pasta: PathBuf,
    pub lista: Option<String>,
    pub fixa: bool,
    pub arquivos: Vec<ArquivoCamada>,
}

/// `hash` de um pedido: SHA-256 do pedido canônico (sem o próprio campo) + `VERSAO_RASTERIZADOR` + SHA-256 das fontes.
pub fn hash_do_pedido(pedido: &PedidoCamada) -> String {
    todo!()
}

/// Rasteriza o pedido no cache (`cache` = pasta da Edição do caso); pronto no cache = só devolve.
pub fn rasterizar(pedido: &PedidoCamada, cache: &Path, cancelar: &AtomicBool) -> Result<CamadaPronta> {
    todo!()
}

/// Rasteriza vários pedidos em paralelo; devolve na ordem dos pedidos. `feito` recebe quantos acabaram.
pub fn rasterizar_todos(
    pedidos: &[PedidoCamada],
    cache: &Path,
    cancelar: &AtomicBool,
    feito: &mut dyn FnMut(usize),
) -> Result<Vec<CamadaPronta>> {
    todo!()
}
