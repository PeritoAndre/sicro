//! Calculador de Velocidade (matemática pura): calibração (`homography`,
//! `crossratio`) → trajetória no mundo → `velocity` (média ou regressão) e
//! `montecarlo` (incerteza por σ de cada fonte; mais conservador que o IC do ajuste).

pub mod crossratio;
pub mod homography;
pub mod montecarlo;
pub mod velocity;

pub use crossratio::{
    cross_ratio, fit_1d_projectivity, fit_cross_ratio_homography, fit_traffic_line,
    lift_projectivity_to_homography, project_onto_line, CrossRatioError, CrossRatioReference,
    Projectivity1D, TrafficLine,
};
pub use homography::{
    line_calibration, solve_homography_dlt, Homography, HomographyError,
};
pub use montecarlo::{
    monte_carlo_velocity, monte_carlo_velocity_cross_ratio, MonteCarloConfig,
    MonteCarloCrossRatioConfig, MonteCarloError, MonteCarloResult,
};
pub use velocity::{
    average_velocity, regression_velocity, RegressionResult, Velocity, VelocityError,
};
