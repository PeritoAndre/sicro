//! Medições por fotogrametria — Fase 1 (matemática pura).
//!
//! Consome uma calibração JÁ criada pelo Calculador de Velocidade (uma
//! [`crate::video::speed::homography::Homography`]) e mede grandezas reais a
//! partir de pontos marcados na cena. Nunca calibra a cena por conta própria
//! — a calibração é a geometria da cena (uma só) e a medição é um consumo
//! dela (muitos), espelhando a separação calibração↔cálculo da velocidade.
//!
//! Por ora: distância entre dois pontos. A análise de incerteza
//! (Monte Carlo de distância) vive em `measure/montecarlo.rs`, que reusa o
//! núcleo estatístico de `speed::montecarlo` (`summarize_distribution`) e a
//! mesma disciplina de semente — dependência em MÃO ÚNICA (measure → speed).

pub mod distance;
pub mod montecarlo;

pub use distance::{world_distance, MeasureError};
pub use montecarlo::{
    monte_carlo_distance, monte_carlo_distance_cross_ratio, MonteCarloCrossRatioDistanceConfig,
    MonteCarloDistanceConfig, MonteCarloDistanceResult,
};
