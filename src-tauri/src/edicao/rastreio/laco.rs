//! Laço do SICRO (porte de `p3a/referencia/laco_sicro.py`, §6.2): decide por quadro se aceita, avisa, procura ou
//! para. Reprocura (VitTrack): ok ≥ 0,45, incerto 0,30..0,45, perdido < 0,30 ou salto > 1 diagonal ou tamanho
//! variando mais de 1,6×; reprocura em duas passadas; para depois de 2 s perdido. Presença (EdgeTAM): ok ≥ 0,95,
//! incerto 0,5..0,95, ausente < 0,5; para depois de 3 s sem o alvo. Dono: P2.2. Abertura: recusa.

use std::sync::atomic::AtomicBool;

use super::{recusa, AmostraCorrida, Avanco, Laco, LeitorQuadros, PedidoCorrida, Rastreador, ResultadoCorrida};
use crate::edicao::modelo::TamanhoPx;
use crate::error::Result;

/// Limiares do laço por reprocura (`laco_sicro.py`), ponto de partida medido em 38 sequências (§12).
pub const T_PERDA: f64 = 0.30;
pub const T_INCERTO: f64 = 0.45;
pub const REENCONTRO_REL: f64 = 0.85;
pub const T_REENCONTRO_MIN: f64 = 0.35;
pub const PACIENCIA_S: f64 = 2.0;
pub const SALTO_MAX: f64 = 1.0;
pub const RAZAO_MAX: f64 = 1.6;
pub const RAZAO_REENCONTRO: f64 = 2.0;
pub const CRESCE_BUSCA: f64 = 0.10;
pub const BUSCA_MAX: f64 = 3.0;
/// Laço por presença.
pub const P_PRESENTE: f64 = 0.5;
pub const P_INCERTO: f64 = 0.95;
pub const PACIENCIA_MEMORIA_S: f64 = 3.0;

/// A corrida inteira pelo laço que o motor pede (`Rastreador::laco`).
pub fn correr(
    p: &PedidoCorrida,
    leitor: &mut dyn LeitorQuadros,
    r: &mut dyn Rastreador,
    cancelar: &AtomicBool,
    avanco: &mut dyn FnMut(Avanco),
) -> Result<ResultadoCorrida> {
    let _ = (p, leitor, r, cancelar, avanco, Laco::Reprocura);
    Err(recusa("rastreio"))
}

/// Quadros-chave (Douglas-Peucker em x, y, w, h): a caixa interpolada fica a menos de `tolerancia` × lado médio
/// (mínimo `piso_px`) de toda amostra; buraco vira fim e início de trecho. Devolve índices em `amostras`.
pub fn quadros_chave(amostras: &[AmostraCorrida], pixel: TamanhoPx, tolerancia: f64, piso_px: f64) -> Vec<usize> {
    let _ = (amostras, pixel, tolerancia, piso_px);
    Vec::new()
}
