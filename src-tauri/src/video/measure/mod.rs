//! Medições por fotogrametria sobre uma calibração já criada (`Homography`):
//! distância entre dois pontos + Monte Carlo. Dependência em mão única
//! (measure → speed).

pub mod distance;
pub mod montecarlo;

pub use distance::{world_distance, MeasureError};
pub use montecarlo::{
    monte_carlo_distance, monte_carlo_distance_cross_ratio, MonteCarloCrossRatioDistanceConfig,
    MonteCarloDistanceConfig, MonteCarloDistanceResult,
};
