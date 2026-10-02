//! Monte Carlo de distância: espelha o de velocidade (mesmo núcleo
//! `summarize_distribution`, mesma semente), mas o alvo são dois pontos sem
//! tempo. Roda mesmo com σ = 0 (o gate é da camada de comando); calibração
//! por linha fica sem MC, como na velocidade.

use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};

use crate::video::measure::distance::world_distance;
use crate::video::speed::crossratio::{fit_cross_ratio_homography, CrossRatioReference};
use crate::video::speed::homography::solve_homography_dlt;
use crate::video::speed::montecarlo::{summarize_distribution, MonteCarloError};

/// Distribuição da distância (m): p2.5/p97.5 = IC 95%, p5/p95 = IC 90%.
#[derive(Debug, Clone)]
pub struct MonteCarloDistanceResult {
    pub successful_iterations: usize,
    pub failed_iterations: usize,
    pub mean_m: f64,
    pub median_m: f64,
    pub std_m: f64,
    pub p2_5_m: f64,
    pub p5_m: f64,
    pub p95_m: f64,
    pub p97_5_m: f64,
    /// Uma amostra por iteração bem-sucedida, para o histograma.
    pub samples_m: Vec<f64>,
}

fn summarize_distance_samples(
    samples: Vec<f64>,
    failed: usize,
    iterations: usize,
) -> Result<MonteCarloDistanceResult, MonteCarloError> {
    let s = summarize_distribution(samples, failed, iterations)?;
    Ok(MonteCarloDistanceResult {
        successful_iterations: s.successful,
        failed_iterations: s.failed,
        mean_m: s.mean,
        median_m: s.median,
        std_m: s.std,
        p2_5_m: s.p2_5,
        p5_m: s.p5,
        p95_m: s.p95,
        p97_5_m: s.p97_5,
        samples_m: s.samples,
    })
}

/// Modo plano (DLT de 4 cantos). Cada σ é o desvio de uma Normal(0, σ);
/// 0 desliga a fonte; negativo vira 0.
#[derive(Debug, Clone)]
pub struct MonteCarloDistanceConfig {
    pub calibration_image_pts: [(f64, f64); 4],
    /// Em metros.
    pub calibration_world_pts: [(f64, f64); 4],
    pub p1_px: (f64, f64),
    pub p2_px: (f64, f64),
    /// Pixels.
    pub sigma_calibration_px: f64,
    /// Metros.
    pub sigma_world_m: f64,
    /// Pixels, nos dois pontos medidos.
    pub sigma_measure_px: f64,
    /// Mínimo 10.
    pub iterations: usize,
    /// `None` usa entropia do SO.
    pub seed: Option<u64>,
}

/// Por iteração: perturba calibração (pixel + mundo) → DLT → perturba os dois
/// pontos → `world_distance`. Iterações que falham são contadas.
pub fn monte_carlo_distance(
    config: &MonteCarloDistanceConfig,
) -> Result<MonteCarloDistanceResult, MonteCarloError> {
    if config.iterations < 10 {
        return Err(MonteCarloError::InvalidConfig(format!(
            "iterations precisa ser >= 10 (recebido {})",
            config.iterations
        )));
    }

    let mut rng: StdRng = match config.seed {
        Some(s) => StdRng::seed_from_u64(s),
        None => StdRng::from_entropy(),
    };

    let n_cal = Normal::new(0.0, config.sigma_calibration_px.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_calibration_px inválido: {e}")))?;
    let n_world = Normal::new(0.0, config.sigma_world_m.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_world_m inválido: {e}")))?;
    let n_meas = Normal::new(0.0, config.sigma_measure_px.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_measure_px inválido: {e}")))?;

    let mut samples: Vec<f64> = Vec::with_capacity(config.iterations);
    let mut failed = 0_usize;

    for _ in 0..config.iterations {
        let mut cal_img = config.calibration_image_pts;
        let mut cal_world = config.calibration_world_pts;
        for i in 0..4 {
            cal_img[i].0 += n_cal.sample(&mut rng);
            cal_img[i].1 += n_cal.sample(&mut rng);
            cal_world[i].0 += n_world.sample(&mut rng);
            cal_world[i].1 += n_world.sample(&mut rng);
        }

        let h = match solve_homography_dlt(&cal_img, &cal_world) {
            Ok(h) => h,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        let p1 = (
            config.p1_px.0 + n_meas.sample(&mut rng),
            config.p1_px.1 + n_meas.sample(&mut rng),
        );
        let p2 = (
            config.p2_px.0 + n_meas.sample(&mut rng),
            config.p2_px.1 + n_meas.sample(&mut rng),
        );

        let d = match world_distance(&h, p1, p2) {
            Ok(d) => d,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        if !d.is_finite() {
            failed += 1;
            continue;
        }
        samples.push(d);
    }

    summarize_distance_samples(samples, failed, config.iterations)
}

/// Modo razão cruzada: `>= 3` referências colineares, re-ajustadas a cada iteração.
#[derive(Debug, Clone)]
pub struct MonteCarloCrossRatioDistanceConfig {
    /// Pixel + posição real (m) ao longo da linha.
    pub references: Vec<CrossRatioReference>,
    pub p1_px: (f64, f64),
    pub p2_px: (f64, f64),
    /// Pixels.
    pub sigma_calibration_px: f64,
    /// Metros.
    pub sigma_world_m: f64,
    /// Pixels, nos dois pontos medidos.
    pub sigma_measure_px: f64,
    /// Mínimo 10.
    pub iterations: usize,
    /// `None` usa entropia do SO.
    pub seed: Option<u64>,
}

/// Como `monte_carlo_distance`, perturbando as referências colineares e
/// refazendo `fit_cross_ratio_homography` por iteração; a 3×3 projeta para
/// `(s, 0)`, então a distância sai ao longo da linha.
pub fn monte_carlo_distance_cross_ratio(
    config: &MonteCarloCrossRatioDistanceConfig,
) -> Result<MonteCarloDistanceResult, MonteCarloError> {
    if config.iterations < 10 {
        return Err(MonteCarloError::InvalidConfig(format!(
            "iterations precisa ser >= 10 (recebido {})",
            config.iterations
        )));
    }
    if config.references.len() < 3 {
        return Err(MonteCarloError::InvalidConfig(
            "razão cruzada precisa de pelo menos 3 referências".into(),
        ));
    }

    let mut rng: StdRng = match config.seed {
        Some(s) => StdRng::seed_from_u64(s),
        None => StdRng::from_entropy(),
    };

    let n_cal = Normal::new(0.0, config.sigma_calibration_px.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_calibration_px inválido: {e}")))?;
    let n_world = Normal::new(0.0, config.sigma_world_m.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_world_m inválido: {e}")))?;
    let n_meas = Normal::new(0.0, config.sigma_measure_px.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_measure_px inválido: {e}")))?;

    let mut samples: Vec<f64> = Vec::with_capacity(config.iterations);
    let mut failed = 0_usize;

    for _ in 0..config.iterations {
        let perturbed: Vec<CrossRatioReference> = config
            .references
            .iter()
            .map(|r| CrossRatioReference {
                px: r.px + n_cal.sample(&mut rng),
                py: r.py + n_cal.sample(&mut rng),
                world_m: r.world_m + n_world.sample(&mut rng),
            })
            .collect();

        let h = match fit_cross_ratio_homography(&perturbed) {
            Ok(h) => h,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        let p1 = (
            config.p1_px.0 + n_meas.sample(&mut rng),
            config.p1_px.1 + n_meas.sample(&mut rng),
        );
        let p2 = (
            config.p2_px.0 + n_meas.sample(&mut rng),
            config.p2_px.1 + n_meas.sample(&mut rng),
        );

        let d = match world_distance(&h, p1, p2) {
            Ok(d) => d,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        if !d.is_finite() {
            failed += 1;
            continue;
        }
        samples.push(d);
    }

    summarize_distance_samples(samples, failed, config.iterations)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Retângulo 10×10 m a 100 px/m; pontos (1 m, 5 m) e (6 m, 5 m) → 5 m.
    fn build_distance_config() -> MonteCarloDistanceConfig {
        const PX_PER_M: f64 = 100.0;
        let cal_img = [
            (0.0, 0.0),
            (10.0 * PX_PER_M, 0.0),
            (10.0 * PX_PER_M, 10.0 * PX_PER_M),
            (0.0, 10.0 * PX_PER_M),
        ];
        let cal_world = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        MonteCarloDistanceConfig {
            calibration_image_pts: cal_img,
            calibration_world_pts: cal_world,
            p1_px: (100.0, 500.0),
            p2_px: (600.0, 500.0),
            sigma_calibration_px: 0.0,
            sigma_world_m: 0.0,
            sigma_measure_px: 0.0,
            iterations: 200,
            seed: Some(42),
        }
    }

    #[test]
    fn distance_no_noise_is_exact() {
        let cfg = build_distance_config();
        let r = monte_carlo_distance(&cfg).unwrap();
        assert_eq!(r.successful_iterations, 200);
        assert_eq!(r.failed_iterations, 0);
        assert!((r.mean_m - 5.0).abs() < 1e-9, "mean = {}", r.mean_m);
        assert!(r.std_m < 1e-9, "std = {}", r.std_m);
        assert!((r.p2_5_m - 5.0).abs() < 1e-9);
        assert!((r.p97_5_m - 5.0).abs() < 1e-9);
    }

    #[test]
    fn distance_same_seed_reproduces() {
        let mut cfg = build_distance_config();
        cfg.sigma_calibration_px = 0.5;
        cfg.sigma_world_m = 0.02;
        cfg.sigma_measure_px = 1.0;
        cfg.iterations = 300;
        cfg.seed = Some(2024);

        let r1 = monte_carlo_distance(&cfg).unwrap();
        let r2 = monte_carlo_distance(&cfg).unwrap();
        assert_eq!(r1.samples_m.len(), r2.samples_m.len());
        for (a, b) in r1.samples_m.iter().zip(r2.samples_m.iter()) {
            assert!((a - b).abs() < 1e-12, "amostras diferentes: {a} vs {b}");
        }
        assert!(r1.std_m > 0.0, "esperado dispersão > 0");
        assert!((r1.mean_m - 5.0).abs() < 0.5, "mean = {}", r1.mean_m);
    }

    #[test]
    fn distance_cross_ratio_no_noise_is_exact() {
        let cfg = MonteCarloCrossRatioDistanceConfig {
            references: vec![
                CrossRatioReference { px: 100.0, py: 200.0, world_m: 0.0 },
                CrossRatioReference { px: 200.0, py: 200.0, world_m: 5.0 },
                CrossRatioReference { px: 300.0, py: 200.0, world_m: 10.0 },
            ],
            p1_px: (150.0, 200.0), // 2.5 m
            p2_px: (350.0, 200.0), // 12.5 m
            sigma_calibration_px: 0.0,
            sigma_world_m: 0.0,
            sigma_measure_px: 0.0,
            iterations: 100,
            seed: Some(7),
        };
        let r = monte_carlo_distance_cross_ratio(&cfg).unwrap();
        assert_eq!(r.failed_iterations, 0);
        assert!((r.mean_m - 10.0).abs() < 1e-6, "mean = {}", r.mean_m);
        assert!(r.std_m < 1e-6, "std = {}", r.std_m);
    }

    #[test]
    fn distance_cross_ratio_reproducible() {
        let cfg = MonteCarloCrossRatioDistanceConfig {
            references: vec![
                CrossRatioReference { px: 100.0, py: 200.0, world_m: 0.0 },
                CrossRatioReference { px: 200.0, py: 205.0, world_m: 5.0 },
                CrossRatioReference { px: 300.0, py: 198.0, world_m: 10.0 },
                CrossRatioReference { px: 400.0, py: 203.0, world_m: 15.0 },
            ],
            p1_px: (150.0, 201.0),
            p2_px: (350.0, 200.0),
            sigma_calibration_px: 0.4,
            sigma_world_m: 0.03,
            sigma_measure_px: 0.8,
            iterations: 150,
            seed: Some(99),
        };
        let r1 = monte_carlo_distance_cross_ratio(&cfg).unwrap();
        let r2 = monte_carlo_distance_cross_ratio(&cfg).unwrap();
        assert_eq!(r1.samples_m.len(), r2.samples_m.len());
        for (a, b) in r1.samples_m.iter().zip(r2.samples_m.iter()) {
            assert!((a - b).abs() < 1e-12, "{a} vs {b}");
        }
    }

    #[test]
    fn distance_rejects_too_few_iterations() {
        let mut cfg = build_distance_config();
        cfg.iterations = 5;
        assert!(matches!(
            monte_carlo_distance(&cfg),
            Err(MonteCarloError::InvalidConfig(_))
        ));
    }
}
