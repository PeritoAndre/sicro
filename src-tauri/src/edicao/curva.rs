//! `Animavel`: avaliar, amostrar e simplificar (§5.11). Espelho no front: `modelo/curva.ts`. Dono: P0.3a.

use super::modelo::{Animavel, Caixa, Chave, Ponto};

/// Valor que se mistura entre duas chaves.
pub trait Interpolavel: Clone {
    /// Mistura linear: f = 0 dá `a`, f = 1 dá `b`.
    fn misturar(a: &Self, b: &Self, f: f64) -> Self;
}

impl Interpolavel for f64 {
    fn misturar(a: &Self, b: &Self, f: f64) -> Self {
        todo!()
    }
}

impl Interpolavel for Ponto {
    fn misturar(a: &Self, b: &Self, f: f64) -> Self {
        todo!()
    }
}

impl Interpolavel for Caixa {
    fn misturar(a: &Self, b: &Self, f: f64) -> Self {
        todo!()
    }
}

/// Valor em `t` (s, na base do dono da curva). Fora das chaves vale a mais próxima.
/// Lista de chaves vazia é erro de validação; aqui é pânico.
pub fn avaliar<T: Interpolavel>(a: &Animavel<T>, t: f64) -> T {
    todo!()
}

/// Um valor por instante de `tempos` (s, crescentes).
pub fn amostrar<T: Interpolavel>(a: &Animavel<T>, tempos: &[f64]) -> Vec<T> {
    todo!()
}

/// Fração 0..1 da curva "suave" (entrada e saída suaves) para a fração linear `f`.
pub fn suave(f: f64) -> f64 {
    todo!()
}

/// Douglas-Peucker sobre caixas: devolve as chaves que ficam. `tolerancia` em 0..1 (mesma unidade das caixas).
pub fn simplificar(chaves: &[Chave<Caixa>], tolerancia: f64) -> Vec<Chave<Caixa>> {
    todo!()
}
