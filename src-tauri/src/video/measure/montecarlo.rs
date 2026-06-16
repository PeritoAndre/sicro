//! Monte Carlo de DISTÂNCIA (fotogrametria) — propagação de incerteza.
//!
//! Espelha o Monte Carlo de velocidade (`video::speed::montecarlo`), mas o
//! "alvo" são apenas DOIS pontos marcados (sem tempo), e a grandeza propagada
//! é a distância real entre eles ([`world_distance`]). Mesma disciplina de
//! semente (explícita, persistível como `i64` no comando) e mesmo NÚCLEO
//! estatístico: reusa `speed::montecarlo::summarize_distribution`.
//!
//! Direção da dependência: **measure → speed** (mão única). Este módulo
//! importa de `speed` (homografia, razão cruzada, núcleo estatístico) e de
//! `measure::distance`; `speed` nunca importa `measure`. Como na velocidade, a
//! função roda mesmo com σ = 0 (distribuição degenerada na distância exata) —
//! o gate de "só roda MC se σ > 0" é decisão da camada de comando, não daqui.
//! Suporta plano (DLT) e razão cruzada (fit + lift); calibração por linha fica
//! sem MC, igual à velocidade.

use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};

use crate::video::measure::distance::world_distance;
use crate::video::speed::crossratio::{fit_cross_ratio_homography, CrossRatioReference};
use crate::video::speed::homography::solve_homography_dlt;
use crate::video::speed::montecarlo::{summarize_distribution, MonteCarloError};

/// Estatística da distribuição da distância estimada (tudo em METROS — sem a
/// conversão km/h, que não faz sentido para comprimento).
#[derive(Debug, Clone)]
pub struct MonteCarloDistanceResult {
    /// Iterações que produziram uma distância finita.
    pub successful_iterations: usize,
    /// Iterações descartadas (calibração singular após perturbação, etc.).
    pub failed_iterations: usize,
    /// Média da distância (m).
    pub mean_m: f64,
    /// Mediana (m).
    pub median_m: f64,
    /// Desvio padrão (m).
    pub std_m: f64,
    /// Percentil 2.5% (limite inferior do IC 95%) em m.
    pub p2_5_m: f64,
    /// Percentil 5% (limite inferior do IC 90%) em m.
    pub p5_m: f64,
    /// Percentil 95% (limite superior do IC 90%) em m.
    pub p95_m: f64,
    /// Percentil 97.5% (limite superior do IC 95%) em m.
    pub p97_5_m: f64,
    /// Amostras brutas (m) — uma por iteração bem-sucedida, para histograma.
    pub samples_m: Vec<f64>,
}

/// Resume amostras de distância (m) numa `MonteCarloDistanceResult`. Reusa o
/// mesmo núcleo de `summarize_distribution` da velocidade — apenas sem km/h.
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

/// Configuração do Monte Carlo de distância no modo PLANO (calibração DLT de
/// 4 cantos). Os σ são desvios padrão de Normais de média 0; 0 desliga aquela
/// fonte; negativos são saneados para 0.
#[derive(Debug, Clone)]
pub struct MonteCarloDistanceConfig {
    /// 4 pontos de calibração em pixel.
    pub calibration_image_pts: [(f64, f64); 4],
    /// 4 pontos correspondentes no mundo (metros).
    pub calibration_world_pts: [(f64, f64); 4],
    /// Primeiro ponto medido, em pixel.
    pub p1_px: (f64, f64),
    /// Segundo ponto medido, em pixel.
    pub p2_px: (f64, f64),
    /// σ de marcação dos pontos de calibração (em pixels).
    pub sigma_calibration_px: f64,
    /// σ de medição das dimensões reais da calibração (em metros).
    pub sigma_world_m: f64,
    /// σ de marcação dos DOIS pontos medidos (em pixels).
    pub sigma_measure_px: f64,
    /// Número de iterações Monte Carlo. Mínimo 10 (validado).
    pub iterations: usize,
    /// Seed do RNG para reprodutibilidade. `None` usa entropia do SO.
    pub seed: Option<u64>,
}

/// Executa o Monte Carlo de distância no modo plano. Para cada iteração:
/// perturba os 4 pontos de calibração (pixel + mundo), re-resolve a homografia
/// (DLT), perturba os dois pontos medidos (pixel) e recomputa
/// [`world_distance`]. Iterações que falham (homografia singular, projeção no
/// infinito) são descartadas e contadas.
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
        // 1. Calibração perturbada (pixel + mundo).
        let mut cal_img = config.calibration_image_pts;
        let mut cal_world = config.calibration_world_pts;
        for i in 0..4 {
            cal_img[i].0 += n_cal.sample(&mut rng);
            cal_img[i].1 += n_cal.sample(&mut rng);
            cal_world[i].0 += n_world.sample(&mut rng);
            cal_world[i].1 += n_world.sample(&mut rng);
        }

        // 2. Homografia perturbada.
        let h = match solve_homography_dlt(&cal_img, &cal_world) {
            Ok(h) => h,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        // 3. Pontos medidos perturbados.
        let p1 = (
            config.p1_px.0 + n_meas.sample(&mut rng),
            config.p1_px.1 + n_meas.sample(&mut rng),
        );
        let p2 = (
            config.p2_px.0 + n_meas.sample(&mut rng),
            config.p2_px.1 + n_meas.sample(&mut rng),
        );

        // 4. Distância real (m).
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

/// Configuração do Monte Carlo de distância no modo RAZÃO CRUZADA. Como na
/// velocidade, a calibração é `>= 3` referências colineares (re-ajustadas e
/// re-levantadas a cada iteração), não um quadrilátero.
#[derive(Debug, Clone)]
pub struct MonteCarloCrossRatioDistanceConfig {
    /// Referências colineares (pixel + posição real em metros).
    pub references: Vec<CrossRatioReference>,
    /// Primeiro ponto medido, em pixel.
    pub p1_px: (f64, f64),
    /// Segundo ponto medido, em pixel.
    pub p2_px: (f64, f64),
    /// σ de marcação dos pontos de referência (em pixels).
    pub sigma_calibration_px: f64,
    /// σ da posição real das referências (em metros).
    pub sigma_world_m: f64,
    /// σ de marcação dos DOIS pontos medidos (em pixels).
    pub sigma_measure_px: f64,
    /// Número de iterações Monte Carlo. Mínimo 10.
    pub iterations: usize,
    /// Seed do RNG para reprodutibilidade. `None` usa entropia do SO.
    pub seed: Option<u64>,
}

/// Executa o Monte Carlo de distância no modo razão cruzada, espelhando
/// `monte_carlo_distance` mas perturbando as referências colineares (pixel +
/// posição real), re-ajustando linha + projetividade 1D e re-levantando a 3×3
/// (`fit_cross_ratio_homography`) a cada iteração. A distância sai
/// naturalmente "ao longo da linha" porque a 3×3 projeta para `(s, 0)`.
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
        // 1. Referências perturbadas (pixel + posição real).
        let perturbed: Vec<CrossRatioReference> = config
            .references
            .iter()
            .map(|r| CrossRatioReference {
                px: r.px + n_cal.sample(&mut rng),
                py: r.py + n_cal.sample(&mut rng),
                world_m: r.world_m + n_world.sample(&mut rng),
            })
            .collect();

        // 2. Re-ajuste (linha + projetividade) + lift → 3×3.
        let h = match fit_cross_ratio_homography(&perturbed) {
            Ok(h) => h,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        // 3. Pontos medidos perturbados.
        let p1 = (
            config.p1_px.0 + n_meas.sample(&mut rng),
            config.p1_px.1 + n_meas.sample(&mut rng),
        );
        let p2 = (
            config.p2_px.0 + n_meas.sample(&mut rng),
            config.p2_px.1 + n_meas.sample(&mut rng),
        );

        // 4. Distância ao longo da linha (a 3×3 projeta para (s, 0)).
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

    /// Calibração 100 px = 1 m (retângulo 10×10 m). Dois pontos a 5 m de
    /// distância: (1m, 5m) = (100, 500) px e (6m, 5m) = (600, 500) px.
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

    /// σ = 0 ⇒ distribuição degenerada na distância exata (5 m).
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

    /// Reprodutibilidade: a mesma seed reproduz exatamente as amostras.
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
        // Com ruído há dispersão e a média fica perto de 5 m.
        assert!(r1.std_m > 0.0, "esperado dispersão > 0");
        assert!((r1.mean_m - 5.0).abs() < 0.5, "mean = {}", r1.mean_m);
    }

    /// Razão cruzada, σ = 0: distância exata ao longo da linha (10 m).
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

    /// Razão cruzada com ruído: a mesma seed reproduz exatamente as amostras.
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
