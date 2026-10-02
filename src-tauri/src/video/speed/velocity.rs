//! Velocidade a partir de pontos no plano do mundo + tempos: média entre 2
//! pontos, ou regressão por eixo (`X(t) = vx·t + bx`, `Y(t) = vy·t + by`,
//! `v = sqrt(vx² + vy²)`) com `>= 3` pontos.
//!
//! Por que não regredir o comprimento de caminho acumulado: por Jensen,
//! `E[|segmento ruidoso|] >= |segmento real|` — superestima a velocidade
//! (a direção perigosa em perícia), e piora quando o movimento por quadro é
//! pequeno frente ao ruído. A regressão por eixo herda a imparcialidade da
//! regressão linear simples (resta só um viés de 2ª ordem em `vy²`).
//!
//! Premissa: movimento retilíneo e uniforme na janela (R² baixo → segmentar).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Velocity {
    pub m_per_s: f64,
    pub km_per_h: f64,
}

impl Velocity {
    pub fn from_m_per_s(v: f64) -> Self {
        Self {
            m_per_s: v,
            km_per_h: v * 3.6,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RegressionResult {
    /// Rapidez `sqrt(vx² + vy²)`.
    pub velocity: Velocity,
    pub vx_m_per_s: f64,
    pub vy_m_per_s: f64,
    /// Erro padrão da rapidez (delta method, ruído isotrópico).
    pub se_m_per_s: f64,
    /// IC 95% (Student's t); pode ficar negativo em cena muito ruidosa.
    pub ci95_m_per_s: (f64, f64),
    pub ci95_km_per_h: (f64, f64),
    /// R² conjunto (X + Y), em [0, 1].
    pub r_squared: f64,
    /// `2·(n − 2)` (4 parâmetros).
    pub degrees_of_freedom: usize,
    /// Resíduo 2D (m) de cada ponto à trajetória ajustada.
    pub residuals: Vec<f64>,
}

#[derive(Debug, thiserror::Error)]
pub enum VelocityError {
    #[error("regressão exige pelo menos 3 pontos (recebido {0})")]
    InsufficientPoints(usize),
    #[error("amplitude temporal zero — todos os tempos coincidem")]
    ZeroTimeSpread,
    #[error("vetores de pontos e tempos com tamanhos diferentes ({pts} vs {times})")]
    DimensionMismatch { pts: usize, times: usize },
}

/// Rapidez média `|d| / |Δt|` entre dois pontos (m, s).
pub fn average_velocity(
    p1: (f64, f64),
    t1: f64,
    p2: (f64, f64),
    t2: f64,
) -> Result<Velocity, VelocityError> {
    let dt = (t2 - t1).abs();
    if dt < 1e-12 {
        return Err(VelocityError::ZeroTimeSpread);
    }
    let dx = p2.0 - p1.0;
    let dy = p2.1 - p1.1;
    let distance = (dx * dx + dy * dy).sqrt();
    Ok(Velocity::from_m_per_s(distance / dt))
}

/// Regressão por eixo (`X(t)`, `Y(t)` por mínimos quadrados) para `>= 3`
/// pontos em ordem cronológica.
pub fn regression_velocity(
    points: &[(f64, f64)],
    times: &[f64],
) -> Result<RegressionResult, VelocityError> {
    if points.len() != times.len() {
        return Err(VelocityError::DimensionMismatch {
            pts: points.len(),
            times: times.len(),
        });
    }
    let n = points.len();
    if n < 3 {
        return Err(VelocityError::InsufficientPoints(n));
    }

    let n_f = n as f64;
    let t_mean: f64 = times.iter().sum::<f64>() / n_f;
    let x_mean: f64 = points.iter().map(|p| p.0).sum::<f64>() / n_f;
    let y_mean: f64 = points.iter().map(|p| p.1).sum::<f64>() / n_f;

    // Σ(tᵢ-t̄)², Σ(tᵢ-t̄)(xᵢ-x̄), Σ(tᵢ-t̄)(yᵢ-ȳ).
    let mut s_tt = 0.0;
    let mut s_tx = 0.0;
    let mut s_ty = 0.0;
    for i in 0..n {
        let dt = times[i] - t_mean;
        s_tt += dt * dt;
        s_tx += dt * (points[i].0 - x_mean);
        s_ty += dt * (points[i].1 - y_mean);
    }
    if s_tt.abs() < 1e-12 {
        return Err(VelocityError::ZeroTimeSpread);
    }

    let vx = s_tx / s_tt;
    let vy = s_ty / s_tt;
    let bx = x_mean - vx * t_mean;
    let by = y_mean - vy * t_mean;
    let speed = (vx * vx + vy * vy).sqrt();

    let mut residuals = Vec::with_capacity(n);
    let mut ss_res_x = 0.0;
    let mut ss_res_y = 0.0;
    let mut ss_tot_x = 0.0;
    let mut ss_tot_y = 0.0;
    for i in 0..n {
        let rx = points[i].0 - (vx * times[i] + bx);
        let ry = points[i].1 - (vy * times[i] + by);
        residuals.push((rx * rx + ry * ry).sqrt());
        ss_res_x += rx * rx;
        ss_res_y += ry * ry;
        let ddx = points[i].0 - x_mean;
        let ddy = points[i].1 - y_mean;
        ss_tot_x += ddx * ddx;
        ss_tot_y += ddy * ddy;
    }

    // Variância agrupada entre os eixos (ruído isotrópico), 2n obs − 4 parâmetros.
    // Com var(vx) = var(vy) = mse/s_tt o delta method dá SE_v = sqrt(mse/s_tt)
    // — sem dividir por v (sem singularidade em v → 0).
    let df = 2 * (n - 2);
    let pooled_mse = (ss_res_x + ss_res_y) / df as f64;
    let se_v = (pooled_mse / s_tt).max(0.0).sqrt();

    let ss_tot = ss_tot_x + ss_tot_y;
    let ss_res = ss_res_x + ss_res_y;
    let r_squared = if ss_tot > 1e-12 {
        (1.0 - ss_res / ss_tot).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let t_crit = t_critical_95(df);
    let margin = t_crit * se_v;
    let ci_lo = speed - margin;
    let ci_hi = speed + margin;

    Ok(RegressionResult {
        velocity: Velocity::from_m_per_s(speed),
        vx_m_per_s: vx,
        vy_m_per_s: vy,
        se_m_per_s: se_v,
        ci95_m_per_s: (ci_lo, ci_hi),
        ci95_km_per_h: (ci_lo * 3.6, ci_hi * 3.6),
        r_squared,
        degrees_of_freedom: df,
        residuals,
    })
}

/// t de Student bilateral 95% (NIST Handbook §1.3.6.7.2); z = 1.960 para df ≥ 30.
fn t_critical_95(df: usize) -> f64 {
    match df {
        1 => 12.706,
        2 => 4.303,
        3 => 3.182,
        4 => 2.776,
        5 => 2.571,
        6 => 2.447,
        7 => 2.365,
        8 => 2.306,
        9 => 2.262,
        10 => 2.228,
        11 => 2.201,
        12 => 2.179,
        13 => 2.160,
        14 => 2.145,
        15 => 2.131,
        16 => 2.120,
        17 => 2.110,
        18 => 2.101,
        19 => 2.093,
        20 => 2.086,
        21 => 2.080,
        22 => 2.074,
        23 => 2.069,
        24 => 2.064,
        25 => 2.060,
        26 => 2.056,
        27 => 2.052,
        28 => 2.048,
        29 => 2.045,
        _ => 1.960,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_velocity_constant_motion() {
        let v = average_velocity((0.0, 0.0), 0.0, (10.0, 0.0), 1.0).unwrap();
        assert!((v.m_per_s - 10.0).abs() < 1e-12);
        assert!((v.km_per_h - 36.0).abs() < 1e-12);
    }

    #[test]
    fn average_velocity_diagonal_pythagorean() {
        let v = average_velocity((0.0, 0.0), 0.0, (5.0, 12.0), 1.0).unwrap();
        assert!((v.m_per_s - 13.0).abs() < 1e-12);
    }

    #[test]
    fn average_velocity_rejects_zero_time() {
        let r = average_velocity((0.0, 0.0), 1.0, (1.0, 0.0), 1.0);
        assert!(matches!(r, Err(VelocityError::ZeroTimeSpread)));
    }

    #[test]
    fn regression_recovers_exact_constant_velocity() {
        let points = vec![
            (0.0, 0.0),
            (1.0, 0.0),
            (2.0, 0.0),
            (3.0, 0.0),
            (4.0, 0.0),
        ];
        let times = vec![0.0, 0.1, 0.2, 0.3, 0.4];
        let res = regression_velocity(&points, &times).unwrap();
        assert!((res.velocity.m_per_s - 10.0).abs() < 1e-9, "v = {}", res.velocity.m_per_s);
        assert!((res.velocity.km_per_h - 36.0).abs() < 1e-9);
        assert!((res.vx_m_per_s - 10.0).abs() < 1e-9, "vx = {}", res.vx_m_per_s);
        assert!(res.vy_m_per_s.abs() < 1e-9, "vy = {}", res.vy_m_per_s);
        assert!(res.r_squared > 0.9999999);
        assert!(res.se_m_per_s < 1e-9);
        for r in &res.residuals {
            assert!(r.abs() < 1e-9);
        }
    }

    #[test]
    fn regression_ci_contains_true_velocity_with_small_noise() {
        let true_v = 20.0;
        // Posição = v·t + ε(i), ε pequeno e determinístico.
        let n = 10;
        let times: Vec<f64> = (0..n).map(|i| i as f64 * 0.1).collect();
        let points: Vec<(f64, f64)> = times
            .iter()
            .enumerate()
            .map(|(i, &t)| {
                let noise = 0.02 * ((i as f64).sin() + 0.3 * (i as f64 * 1.7).cos());
                (true_v * t + noise, 0.0)
            })
            .collect();
        let res = regression_velocity(&points, &times).unwrap();
        let (lo, hi) = res.ci95_m_per_s;
        assert!(
            lo <= true_v && true_v <= hi,
            "true_v = {true_v} fora de IC ({lo}, {hi})",
        );
        assert!(res.se_m_per_s > 0.0);
        assert!(res.r_squared > 0.99);
    }

    #[test]
    fn regression_diagonal_constant_velocity() {
        // vx = 3, vy = 4 → 5 m/s.
        let times = vec![0.0, 0.1, 0.2, 0.3, 0.4];
        let points: Vec<(f64, f64)> = times.iter().map(|&t| (3.0 * t, 4.0 * t)).collect();
        let res = regression_velocity(&points, &times).unwrap();
        assert!((res.vx_m_per_s - 3.0).abs() < 1e-9, "vx = {}", res.vx_m_per_s);
        assert!((res.vy_m_per_s - 4.0).abs() < 1e-9, "vy = {}", res.vy_m_per_s);
        assert!((res.velocity.m_per_s - 5.0).abs() < 1e-9, "v = {}", res.velocity.m_per_s);
        assert!((res.velocity.km_per_h - 18.0).abs() < 1e-9);
    }

    /// Poligonal em L: a regressão por eixo dá a velocidade vetorial líquida
    /// (sqrt(50) ≈ 7.07 m/s), não o comprimento de caminho (10 m/s).
    #[test]
    fn regression_per_axis_measures_net_velocity_not_path_length() {
        let points = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)];
        let times = vec![0.0, 0.1, 0.2];
        let res = regression_velocity(&points, &times).unwrap();
        // X(t): [0,1,1] → vx = 5; Y(t): [0,0,1] → vy = 5.
        assert!(
            (res.velocity.m_per_s - 50.0_f64.sqrt()).abs() < 1e-9,
            "v = {} (esperado sqrt(50) ≈ 7.071)",
            res.velocity.m_per_s
        );
        assert!(res.r_squared < 1.0);
    }

    /// Sob ruído simétrico, a regressão por eixo não enviesa para cima como o
    /// comprimento de caminho (compara as médias dos dois estimadores).
    #[test]
    fn per_axis_does_not_bias_upward_like_path_length() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        use rand_distr::{Distribution, Normal};

        // Regime que amplifica o viés: 0.2 m por quadro com σ = 0.1 m por coordenada.
        let true_v = 2.0;
        let n = 8;
        let dt = 0.1;
        let sigma = 0.1;

        let times: Vec<f64> = (0..n).map(|i| i as f64 * dt).collect();
        let true_x: Vec<f64> = times.iter().map(|&t| true_v * t).collect();

        let mut rng = StdRng::seed_from_u64(2024);
        let noise = Normal::new(0.0, sigma).unwrap();

        let trials = 4000;
        let mut sum_per_axis = 0.0;
        let mut sum_path_length = 0.0;

        for _ in 0..trials {
            let pts: Vec<(f64, f64)> = true_x
                .iter()
                .map(|&x| (x + noise.sample(&mut rng), noise.sample(&mut rng)))
                .collect();

            let res = regression_velocity(&pts, &times).unwrap();
            sum_per_axis += res.velocity.m_per_s;

            // Comprimento de caminho acumulado, no mesmo conjunto perturbado.
            let mut dist = vec![0.0; n];
            for i in 1..n {
                let dx = pts[i].0 - pts[i - 1].0;
                let dy = pts[i].1 - pts[i - 1].1;
                dist[i] = dist[i - 1] + (dx * dx + dy * dy).sqrt();
            }
            let t_mean = times.iter().sum::<f64>() / n as f64;
            let d_mean = dist.iter().sum::<f64>() / n as f64;
            let mut s_td = 0.0;
            let mut s_tt = 0.0;
            for i in 0..n {
                let dtt = times[i] - t_mean;
                s_td += dtt * (dist[i] - d_mean);
                s_tt += dtt * dtt;
            }
            sum_path_length += s_td / s_tt;
        }

        let mean_per_axis = sum_per_axis / trials as f64;
        let mean_path_length = sum_path_length / trials as f64;

        assert!(
            (mean_per_axis - true_v).abs() < 0.2,
            "per-axis enviesado: média = {mean_per_axis}, verdadeiro = {true_v}"
        );
        assert!(
            mean_path_length > true_v + 0.1,
            "comprimento de caminho deveria superestimar: média = {mean_path_length}"
        );
        assert!(
            mean_path_length > mean_per_axis + 0.1,
            "comprimento ({mean_path_length}) deveria ser bem acima do per-axis ({mean_per_axis})"
        );
    }

    #[test]
    fn regression_rejects_too_few_points() {
        let points = vec![(0.0, 0.0), (1.0, 0.0)];
        let times = vec![0.0, 0.1];
        assert!(matches!(
            regression_velocity(&points, &times),
            Err(VelocityError::InsufficientPoints(2))
        ));
    }

    #[test]
    fn regression_rejects_size_mismatch() {
        let points = vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
        let times = vec![0.0, 0.1];
        assert!(matches!(
            regression_velocity(&points, &times),
            Err(VelocityError::DimensionMismatch { pts: 3, times: 2 })
        ));
    }

    #[test]
    fn regression_rejects_zero_time_spread() {
        let points = vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
        let times = vec![0.5, 0.5, 0.5];
        assert!(matches!(
            regression_velocity(&points, &times),
            Err(VelocityError::ZeroTimeSpread)
        ));
    }

    #[test]
    fn velocity_unit_conversion() {
        let v = Velocity::from_m_per_s(10.0);
        assert!((v.km_per_h - 36.0).abs() < 1e-12);
        let v2 = Velocity::from_m_per_s(0.0);
        assert_eq!(v2.km_per_h, 0.0);
    }
}
