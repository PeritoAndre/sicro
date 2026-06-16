//! Análise de incerteza por Monte Carlo do Calculador de Velocidade.
//!
//! O perito raramente conhece com certeza absoluta:
//!   - a posição em pixel de cada marca de calibração (erro de clique);
//!   - a dimensão real do retângulo de calibração (erro de medição em
//!     campo, em metros);
//!   - a posição em pixel de cada quadro da trajetória do veículo;
//!   - o instante exato de cada quadro (VFR, sincronia do reprodutor).
//!
//! Cada uma dessas fontes é modelada como uma Normal de média 0 e
//! desvio padrão σ (fornecido pelo perito ou estimado em laudo). O
//! módulo amostra `N` realizações independentes do cenário, executa todo
//! o pipeline (homografia → projeção → regressão de velocidade) e
//! resume a distribuição resultante da velocidade.
//!
//! Resultado:
//!   - média, mediana, desvio padrão;
//!   - percentis 2.5/5/95/97.5 (ICs aproximados 95% e 90%);
//!   - amostras brutas para histogramação na UI.
//!
//! As iterações que falham (homografia singular, regressão degenerada)
//! são descartadas e contabilizadas em `failed_iterations` — o perito
//! pode usar isso pra detectar configurações instáveis.

use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};

use crate::video::speed::crossratio::{fit_cross_ratio_homography, CrossRatioReference};
use crate::video::speed::homography::{solve_homography_dlt, HomographyError};
use crate::video::speed::velocity::{regression_velocity, VelocityError};

#[derive(Debug, thiserror::Error)]
pub enum MonteCarloError {
    #[error("configuração inválida: {0}")]
    InvalidConfig(String),
    #[error("todas as {0} iterações falharam — verifique se a calibração não é degenerada")]
    AllIterationsFailed(usize),
    #[error(transparent)]
    Homography(#[from] HomographyError),
    #[error(transparent)]
    Velocity(#[from] VelocityError),
}

/// Configuração do experimento Monte Carlo.
///
/// Todos os `sigma_*` são desvios padrão (σ) de uma distribuição Normal
/// com média 0. Valores 0 desligam ruído naquela fonte. Valores
/// negativos são tratados como 0 (sem panic).
#[derive(Debug, Clone)]
pub struct MonteCarloConfig {
    /// 4 pontos de calibração em pixel.
    pub calibration_image_pts: [(f64, f64); 4],
    /// 4 pontos correspondentes no mundo (metros).
    pub calibration_world_pts: [(f64, f64); 4],
    /// Trajetória do alvo em pixel (≥ 3 amostras).
    pub trajectory_image_pts: Vec<(f64, f64)>,
    /// Tempos correspondentes em segundos (mesmo tamanho de
    /// `trajectory_image_pts`).
    pub trajectory_times: Vec<f64>,
    /// σ de erro de marcação nos pontos de calibração (em pixels).
    pub sigma_calibration_px: f64,
    /// σ de erro de medição das dimensões reais (em metros),
    /// aplicado isotropicamente em cada coordenada de `calibration_world_pts`.
    pub sigma_world_m: f64,
    /// σ de erro de marcação nos pontos da trajetória (em pixels).
    pub sigma_trajectory_px: f64,
    /// σ de incerteza temporal (em segundos), por quadro.
    pub sigma_time_s: f64,
    /// Número de iterações Monte Carlo. Mínimo 10 (validado).
    pub iterations: usize,
    /// Seed do RNG pra reprodutibilidade. `None` usa entropia do SO.
    pub seed: Option<u64>,
}

/// Estatísticas da distribuição da velocidade estimada pelo Monte Carlo.
#[derive(Debug, Clone)]
pub struct MonteCarloResult {
    /// Iterações que produziram velocidade finita.
    pub successful_iterations: usize,
    /// Iterações descartadas (singularidade ou erro).
    pub failed_iterations: usize,
    /// Média da velocidade (m/s).
    pub mean_m_per_s: f64,
    /// Mediana (m/s).
    pub median_m_per_s: f64,
    /// Desvio padrão (m/s).
    pub std_m_per_s: f64,
    /// Percentil 2.5% (limite inferior do IC 95%) em m/s.
    pub p2_5_m_per_s: f64,
    /// Percentil 5% (limite inferior do IC 90%) em m/s.
    pub p5_m_per_s: f64,
    /// Percentil 95% (limite superior do IC 90%) em m/s.
    pub p95_m_per_s: f64,
    /// Percentil 97.5% (limite superior do IC 95%) em m/s.
    pub p97_5_m_per_s: f64,
    /// Conversões em km/h (= valor em m/s × 3.6).
    pub mean_km_per_h: f64,
    pub median_km_per_h: f64,
    pub p2_5_km_per_h: f64,
    pub p5_km_per_h: f64,
    pub p95_km_per_h: f64,
    pub p97_5_km_per_h: f64,
    /// Amostras brutas da velocidade (m/s) — uma por iteração bem-sucedida.
    /// Útil pra plot de histograma na UI.
    pub samples_m_per_s: Vec<f64>,
}

/// Executa o experimento Monte Carlo.
///
/// Para cada iteração:
///   1. Perturba os 4 pontos de calibração (imagem) com Normal(0, σ_cal_px)
///   2. Perturba os 4 pontos de calibração (mundo) com Normal(0, σ_world_m)
///   3. Resolve a homografia perturbada (DLT)
///   4. Perturba cada ponto da trajetória (imagem) com Normal(0, σ_traj_px)
///   5. Perturba cada timestamp com Normal(0, σ_time_s)
///   6. Projeta a trajetória para o mundo via homografia perturbada
///   7. Calcula a velocidade por regressão
///   8. Registra a velocidade da iteração (se finita)
///
/// Iterações que falham em algum passo (ex: homografia singular após
/// perturbação) são descartadas e contadas em `failed_iterations`.
pub fn monte_carlo_velocity(
    config: &MonteCarloConfig,
) -> Result<MonteCarloResult, MonteCarloError> {
    if config.iterations < 10 {
        return Err(MonteCarloError::InvalidConfig(format!(
            "iterations precisa ser >= 10 (recebido {})",
            config.iterations
        )));
    }
    if config.trajectory_image_pts.len() != config.trajectory_times.len() {
        return Err(MonteCarloError::InvalidConfig(format!(
            "tamanhos divergentes: pts = {}, times = {}",
            config.trajectory_image_pts.len(),
            config.trajectory_times.len()
        )));
    }
    if config.trajectory_image_pts.len() < 3 {
        return Err(MonteCarloError::InvalidConfig(
            "trajetória precisa de pelo menos 3 pontos para regressão".into(),
        ));
    }

    // RNG semeado pra reprodutibilidade. Sem seed, usa entropia do SO.
    let mut rng: StdRng = match config.seed {
        Some(s) => StdRng::seed_from_u64(s),
        None => StdRng::from_entropy(),
    };

    // Distribuições. Normal::new(0, σ) com σ = 0 é constante 0 (rand_distr
    // aceita); valores negativos são saneados para 0.
    let n_cal = Normal::new(0.0, config.sigma_calibration_px.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_calibration_px inválido: {e}")))?;
    let n_world = Normal::new(0.0, config.sigma_world_m.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_world_m inválido: {e}")))?;
    let n_traj = Normal::new(0.0, config.sigma_trajectory_px.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_trajectory_px inválido: {e}")))?;
    let n_time = Normal::new(0.0, config.sigma_time_s.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_time_s inválido: {e}")))?;

    let mut samples: Vec<f64> = Vec::with_capacity(config.iterations);
    let mut failed = 0_usize;

    for _ in 0..config.iterations {
        // 1+2. Calibração perturbada.
        let mut cal_img = config.calibration_image_pts;
        let mut cal_world = config.calibration_world_pts;
        for i in 0..4 {
            cal_img[i].0 += n_cal.sample(&mut rng);
            cal_img[i].1 += n_cal.sample(&mut rng);
            cal_world[i].0 += n_world.sample(&mut rng);
            cal_world[i].1 += n_world.sample(&mut rng);
        }

        // 3. Homografia perturbada.
        let h = match solve_homography_dlt(&cal_img, &cal_world) {
            Ok(h) => h,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        // 4+5+6. Trajetória + tempos perturbados → projeção para mundo.
        let n_pts = config.trajectory_image_pts.len();
        let mut world_pts = Vec::with_capacity(n_pts);
        let mut times = Vec::with_capacity(n_pts);
        let mut projection_failed = false;
        for i in 0..n_pts {
            let (px, py) = config.trajectory_image_pts[i];
            let perturbed = (
                px + n_traj.sample(&mut rng),
                py + n_traj.sample(&mut rng),
            );
            match h.project(perturbed) {
                Ok(pt) => world_pts.push(pt),
                Err(_) => {
                    projection_failed = true;
                    break;
                }
            }
            times.push(config.trajectory_times[i] + n_time.sample(&mut rng));
        }
        if projection_failed {
            failed += 1;
            continue;
        }

        // 7. Velocidade via regressão.
        let res = match regression_velocity(&world_pts, &times) {
            Ok(r) => r,
            Err(_) => {
                failed += 1;
                continue;
            }
        };

        let v = res.velocity.m_per_s;
        if !v.is_finite() {
            failed += 1;
            continue;
        }
        samples.push(v);
    }

    summarize_samples(samples, failed, config.iterations)
}

/// Estatística bruta de uma distribuição amostral, AGNÓSTICA À UNIDADE:
/// média/mediana/desvio + percentis 2.5/5/95/97.5 sobre as amostras cruas.
/// É o núcleo compartilhado por velocidade (m/s → km/h, aqui) e distância (m,
/// em `video::measure::montecarlo`), para que ambas usem exatamente a mesma
/// matemática de resumo e a mesma regra de "todas as iterações falharam".
///
/// `pub(crate)` porque o módulo `measure` consome este núcleo (dependência em
/// MÃO ÚNICA measure → speed); `speed` nunca importa `measure`.
pub(crate) struct DistributionStats {
    pub(crate) successful: usize,
    pub(crate) failed: usize,
    pub(crate) mean: f64,
    pub(crate) median: f64,
    pub(crate) std: f64,
    pub(crate) p2_5: f64,
    pub(crate) p5: f64,
    pub(crate) p95: f64,
    pub(crate) p97_5: f64,
    pub(crate) samples: Vec<f64>,
}

/// Calcula a `DistributionStats` a partir das amostras finitas coletadas.
/// Erro `AllIterationsFailed` se nenhuma iteração produziu amostra.
pub(crate) fn summarize_distribution(
    samples: Vec<f64>,
    failed: usize,
    iterations: usize,
) -> Result<DistributionStats, MonteCarloError> {
    if samples.is_empty() {
        return Err(MonteCarloError::AllIterationsFailed(iterations));
    }

    let mut sorted = samples.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("amostras finitas filtradas acima"));

    let n_eff = sorted.len() as f64;
    let mean = sorted.iter().sum::<f64>() / n_eff;
    let variance = sorted.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n_eff;
    let std = variance.sqrt();

    Ok(DistributionStats {
        successful: samples.len(),
        failed,
        mean,
        median: percentile_linear(&sorted, 50.0),
        std,
        p2_5: percentile_linear(&sorted, 2.5),
        p5: percentile_linear(&sorted, 5.0),
        p95: percentile_linear(&sorted, 95.0),
        p97_5: percentile_linear(&sorted, 97.5),
        samples,
    })
}

/// Resume as amostras de velocidade (m/s) numa `MonteCarloResult`, anexando a
/// conversão km/h. Compartilhado pelas variantes `monte_carlo_velocity*` para
/// garantir a mesma estatística (núcleo em `summarize_distribution`).
fn summarize_samples(
    samples: Vec<f64>,
    failed: usize,
    iterations: usize,
) -> Result<MonteCarloResult, MonteCarloError> {
    let s = summarize_distribution(samples, failed, iterations)?;
    Ok(MonteCarloResult {
        successful_iterations: s.successful,
        failed_iterations: s.failed,
        mean_m_per_s: s.mean,
        median_m_per_s: s.median,
        std_m_per_s: s.std,
        p2_5_m_per_s: s.p2_5,
        p5_m_per_s: s.p5,
        p95_m_per_s: s.p95,
        p97_5_m_per_s: s.p97_5,
        mean_km_per_h: s.mean * 3.6,
        median_km_per_h: s.median * 3.6,
        p2_5_km_per_h: s.p2_5 * 3.6,
        p5_km_per_h: s.p5 * 3.6,
        p95_km_per_h: s.p95 * 3.6,
        p97_5_km_per_h: s.p97_5 * 3.6,
        samples_m_per_s: s.samples,
    })
}

/// Percentil por interpolação linear (método 7 do NIST / "linear"
/// pra `numpy.percentile`).
///
/// `sorted` deve estar ordenado ascendente. `p` em `[0, 100]`.
fn percentile_linear(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    let frac = rank - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

// ===========================================================================
// Monte Carlo — variante para calibração por RAZÃO CRUZADA.

/// Configuração do Monte Carlo no modo razão cruzada. Diferente do modo plano,
/// a calibração não é um quadrilátero de 4 cantos, mas `>= 3` referências
/// **colineares** ao longo do eixo de tráfego (cada uma com sua posição real
/// `world_m`). Por isso `solve_homography_dlt` NÃO serve aqui — re-ajustamos
/// linha + projetividade 1D e re-levantamos a 3×3 a cada iteração.
#[derive(Debug, Clone)]
pub struct MonteCarloCrossRatioConfig {
    /// Referências colineares (pixel + posição real em metros).
    pub references: Vec<CrossRatioReference>,
    /// Trajetória do alvo em pixel (≥ 3 amostras).
    pub trajectory_image_pts: Vec<(f64, f64)>,
    /// Tempos correspondentes em segundos.
    pub trajectory_times: Vec<f64>,
    /// σ de marcação dos pontos de referência (em pixels).
    pub sigma_calibration_px: f64,
    /// σ da posição real das referências (em metros).
    pub sigma_world_m: f64,
    /// σ de marcação dos pontos da trajetória (em pixels).
    pub sigma_trajectory_px: f64,
    /// σ de incerteza temporal (em segundos), por quadro.
    pub sigma_time_s: f64,
    /// Número de iterações Monte Carlo. Mínimo 10.
    pub iterations: usize,
    /// Seed do RNG para reprodutibilidade. `None` usa entropia do SO.
    pub seed: Option<u64>,
}

/// Executa o Monte Carlo no modo razão cruzada, espelhando
/// `monte_carlo_velocity` mas perturbando as **referências colineares**
/// (pixel + posição real), re-ajustando linha + projetividade 1D,
/// re-levantando a 3×3 (`fit_cross_ratio_homography`) e reprojetando a
/// trajetória perturbada. Mesma disciplina de semente e mesmo resumo
/// estatístico (`summarize_samples`).
pub fn monte_carlo_velocity_cross_ratio(
    config: &MonteCarloCrossRatioConfig,
) -> Result<MonteCarloResult, MonteCarloError> {
    if config.iterations < 10 {
        return Err(MonteCarloError::InvalidConfig(format!(
            "iterations precisa ser >= 10 (recebido {})",
            config.iterations
        )));
    }
    if config.trajectory_image_pts.len() != config.trajectory_times.len() {
        return Err(MonteCarloError::InvalidConfig(format!(
            "tamanhos divergentes: pts = {}, times = {}",
            config.trajectory_image_pts.len(),
            config.trajectory_times.len()
        )));
    }
    if config.trajectory_image_pts.len() < 3 {
        return Err(MonteCarloError::InvalidConfig(
            "trajetória precisa de pelo menos 3 pontos para regressão".into(),
        ));
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
    let n_traj = Normal::new(0.0, config.sigma_trajectory_px.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_trajectory_px inválido: {e}")))?;
    let n_time = Normal::new(0.0, config.sigma_time_s.max(0.0))
        .map_err(|e| MonteCarloError::InvalidConfig(format!("sigma_time_s inválido: {e}")))?;

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

        // 3. Trajetória + tempos perturbados → projeção (s, 0) para o mundo.
        let n_pts = config.trajectory_image_pts.len();
        let mut world_pts = Vec::with_capacity(n_pts);
        let mut times = Vec::with_capacity(n_pts);
        let mut projection_failed = false;
        for i in 0..n_pts {
            let (px, py) = config.trajectory_image_pts[i];
            let perturbed_pt = (px + n_traj.sample(&mut rng), py + n_traj.sample(&mut rng));
            match h.project(perturbed_pt) {
                Ok(pt) => world_pts.push(pt),
                Err(_) => {
                    projection_failed = true;
                    break;
                }
            }
            times.push(config.trajectory_times[i] + n_time.sample(&mut rng));
        }
        if projection_failed {
            failed += 1;
            continue;
        }

        // 4. Velocidade via regressão (a 3×3 já projeta para (s, 0)).
        let res = match regression_velocity(&world_pts, &times) {
            Ok(r) => r,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let v = res.velocity.m_per_s;
        if !v.is_finite() {
            failed += 1;
            continue;
        }
        samples.push(v);
    }

    summarize_samples(samples, failed, config.iterations)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cenário-base realista: calibração 100 px = 1 m (1 px/cm — escala
    /// típica de câmera de fiscalização a 30m do local), retângulo de
    /// referência 10×10 metros, trajetória retilínea constante.
    fn build_base_config(true_v: f64, n_traj: usize, dt: f64) -> MonteCarloConfig {
        // Retângulo 10×10 m no mundo, 1000×1000 px na imagem (escala 100 px/m).
        const PX_PER_M: f64 = 100.0;
        let cal_img = [
            (0.0, 0.0),
            (10.0 * PX_PER_M, 0.0),
            (10.0 * PX_PER_M, 10.0 * PX_PER_M),
            (0.0, 10.0 * PX_PER_M),
        ];
        let cal_world = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];

        // Veículo no meio da pista (y=5m = 500 px), movendo-se em X.
        let trajectory_image_pts: Vec<(f64, f64)> = (0..n_traj)
            .map(|i| (i as f64 * true_v * dt * PX_PER_M, 5.0 * PX_PER_M))
            .collect();
        let trajectory_times: Vec<f64> = (0..n_traj).map(|i| i as f64 * dt).collect();

        MonteCarloConfig {
            calibration_image_pts: cal_img,
            calibration_world_pts: cal_world,
            trajectory_image_pts,
            trajectory_times,
            sigma_calibration_px: 0.0,
            sigma_world_m: 0.0,
            sigma_trajectory_px: 0.0,
            sigma_time_s: 0.0,
            iterations: 200,
            seed: Some(42),
        }
    }

    /// Com `sigma = 0` em todas as fontes, todas as iterações produzem
    /// exatamente a mesma velocidade.
    #[test]
    fn no_noise_is_deterministic() {
        let true_v = 10.0;
        let cfg = build_base_config(true_v, 6, 0.1);
        let r = monte_carlo_velocity(&cfg).unwrap();
        assert_eq!(r.successful_iterations, 200);
        assert_eq!(r.failed_iterations, 0);
        assert!((r.mean_m_per_s - true_v).abs() < 1e-9, "mean = {}", r.mean_m_per_s);
        assert!(r.std_m_per_s < 1e-9, "std = {}", r.std_m_per_s);
        assert!((r.p2_5_m_per_s - true_v).abs() < 1e-9);
        assert!((r.p97_5_m_per_s - true_v).abs() < 1e-9);
        // Conversão km/h consistente.
        assert!((r.mean_km_per_h - true_v * 3.6).abs() < 1e-9);
    }

    /// Cenário com ruído moderado: o IC 95% (p2.5 .. p97.5) deve conter
    /// a velocidade verdadeira, e a média deve estar próxima dela.
    #[test]
    fn ci95_contains_true_velocity_under_noise() {
        let true_v = 25.0; // m/s = 90 km/h
        let mut cfg = build_base_config(true_v, 8, 0.1);
        cfg.sigma_calibration_px = 0.5;
        cfg.sigma_world_m = 0.05;
        cfg.sigma_trajectory_px = 0.5;
        cfg.sigma_time_s = 0.005;
        cfg.iterations = 2000;

        let r = monte_carlo_velocity(&cfg).unwrap();
        assert!(
            r.successful_iterations >= 1900,
            "successful = {}, failed = {}",
            r.successful_iterations,
            r.failed_iterations
        );
        // Média próxima do verdadeiro (tolerância ampla — depende do σ).
        assert!(
            (r.mean_m_per_s - true_v).abs() < 1.0,
            "mean = {}, true = {}",
            r.mean_m_per_s,
            true_v
        );
        // IC 95% contém o verdadeiro.
        assert!(
            r.p2_5_m_per_s <= true_v && true_v <= r.p97_5_m_per_s,
            "true_v = {true_v} fora de IC95 ({}, {})",
            r.p2_5_m_per_s,
            r.p97_5_m_per_s,
        );
        // IC 90% também (sanity: deve ser subconjunto de IC 95%).
        assert!(r.p5_m_per_s >= r.p2_5_m_per_s);
        assert!(r.p95_m_per_s <= r.p97_5_m_per_s);
    }

    /// Reprodutibilidade: mesma seed deve gerar exatamente as mesmas
    /// amostras (até precisão de ponto flutuante).
    #[test]
    fn same_seed_produces_same_samples() {
        let true_v = 15.0;
        let mut cfg = build_base_config(true_v, 6, 0.1);
        cfg.sigma_trajectory_px = 1.0;
        cfg.sigma_time_s = 0.01;
        cfg.iterations = 100;
        cfg.seed = Some(12345);

        let r1 = monte_carlo_velocity(&cfg).unwrap();
        let r2 = monte_carlo_velocity(&cfg).unwrap();

        assert_eq!(r1.samples_m_per_s.len(), r2.samples_m_per_s.len());
        for (a, b) in r1.samples_m_per_s.iter().zip(r2.samples_m_per_s.iter()) {
            assert!((a - b).abs() < 1e-12, "amostras diferentes: {a} vs {b}");
        }
    }

    /// Sementes diferentes devem gerar amostras diferentes (sanity).
    #[test]
    fn different_seeds_produce_different_samples() {
        let true_v = 15.0;
        let mut cfg = build_base_config(true_v, 6, 0.1);
        cfg.sigma_trajectory_px = 1.0;
        cfg.iterations = 100;

        cfg.seed = Some(1);
        let r1 = monte_carlo_velocity(&cfg).unwrap();
        cfg.seed = Some(2);
        let r2 = monte_carlo_velocity(&cfg).unwrap();

        // Improvável que duas seeds resultem em médias idênticas com 100
        // amostras ruidosas. Diferença deve ser detectável.
        assert!((r1.mean_m_per_s - r2.mean_m_per_s).abs() > 1e-9);
    }

    #[test]
    fn rejects_too_few_iterations() {
        let mut cfg = build_base_config(10.0, 5, 0.1);
        cfg.iterations = 5;
        assert!(matches!(
            monte_carlo_velocity(&cfg),
            Err(MonteCarloError::InvalidConfig(_))
        ));
    }

    #[test]
    fn rejects_too_few_trajectory_points() {
        let mut cfg = build_base_config(10.0, 2, 0.1);
        cfg.iterations = 100;
        assert!(matches!(
            monte_carlo_velocity(&cfg),
            Err(MonteCarloError::InvalidConfig(_))
        ));
    }

    #[test]
    fn rejects_dimension_mismatch() {
        let mut cfg = build_base_config(10.0, 5, 0.1);
        cfg.trajectory_times.pop(); // tamanhos divergentes agora
        cfg.iterations = 100;
        assert!(matches!(
            monte_carlo_velocity(&cfg),
            Err(MonteCarloError::InvalidConfig(_))
        ));
    }

    /// Aumentar `sigma_trajectory_px` deve aumentar a largura do IC 95%.
    /// Sanity: o experimento responde monotonicamente ao ruído de entrada.
    #[test]
    fn larger_noise_yields_wider_ci() {
        let true_v = 20.0;
        let mut cfg = build_base_config(true_v, 6, 0.1);
        cfg.iterations = 1000;

        cfg.sigma_trajectory_px = 0.5;
        let r_small = monte_carlo_velocity(&cfg).unwrap();
        let ci_small = r_small.p97_5_m_per_s - r_small.p2_5_m_per_s;

        cfg.sigma_trajectory_px = 3.0;
        let r_big = monte_carlo_velocity(&cfg).unwrap();
        let ci_big = r_big.p97_5_m_per_s - r_big.p2_5_m_per_s;

        assert!(
            ci_big > ci_small,
            "IC com σ=3.0 ({ci_big}) deveria ser maior que σ=0.5 ({ci_small})",
        );
    }
}
