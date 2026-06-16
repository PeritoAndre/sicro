//! Calculador de Velocidade — Fase 1 (matemática pura).
//!
//! Estimativa pericial de velocidade a partir de vídeo, em modo manual:
//! o perito marca pontos de calibração no plano da via, marca a posição
//! do veículo em N quadros sucessivos, e este módulo calcula a
//! velocidade com análise de incerteza.
//!
//! ## Pipeline
//!
//! ```text
//!   [pontos pixel] ─┐
//!                   ├─→ homography (DLT 4-pts OU calibração por linha)
//!   [pontos m]    ─┘        │
//!                            ▼
//!                  [trajetória em mundo (m)]
//!                            │
//!                            ▼
//!              velocity (média 2-pts OU regressão LS)
//!                            │
//!                            ▼
//!              [velocidade + IC do ajuste]
//!
//!         (em paralelo, perturbando entradas:)
//!                            │
//!                            ▼
//!              montecarlo (N iterações, σ por fonte)
//!                            │
//!                            ▼
//!              [distribuição: média/mediana/percentis]
//! ```
//!
//! ## Limitações desta fase
//!
//! - Modo **manual apenas** (sem tracking automático).
//! - Sem banco, sem UI, sem comandos Tauri, sem ffmpeg.
//! - Sem outlier rejection (RANSAC) — DLT é determinístico em 4 pontos
//!   exatos; o Monte Carlo cobre a incerteza estatística.
//! - IC do `velocity` usa Student's t com tabela embutida; para
//!   inferência sobre a velocidade verdadeira sob ruído de calibração
//!   acoplado, prefira a saída do `montecarlo` (mais conservadora).
//!
//! ## Próximas fases (não implementadas aqui)
//!
//! - Persistência: tabelas novas no `.sicro` (calibrations, trajectories,
//!   speed_estimates).
//! - Comandos Tauri pra integrar com a UI.
//! - UI no editor de vídeo: clique-pra-marcar, overlay de calibração,
//!   resultado com histograma.
//! - Renderer pro laudo (DOCX/PDF) com a seção de velocidade formatada.

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
