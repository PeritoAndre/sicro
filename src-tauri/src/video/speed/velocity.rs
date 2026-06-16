//! Cálculo de velocidade a partir de pontos no plano do mundo + tempos.
//!
//! Dois modos:
//!
//! ### Velocidade média entre 2 pontos
//!
//! `distance(p1, p2) / (t2 - t1)`. Sem incerteza estatística (apenas 2
//! amostras). Útil quando o perito tem só duas marcações confiáveis e
//! quer um número rápido.
//!
//! ### Regressão por eixo (≥ 3 pontos)
//!
//! Ajusta **duas** retas independentes por mínimos quadrados:
//!
//! ```text
//!   X(t) = vx·t + bx
//!   Y(t) = vy·t + by
//! ```
//!
//! e calcula a rapidez como `v = sqrt(vx² + vy²)`.
//!
//! **Por que não comprimento de caminho acumulado?** Regredir a soma de
//! `|Pᵢ - Pᵢ₋₁|` parece natural, mas tem **viés sistemático para cima**
//! sob ruído de marcação: por Jensen, `E[|segmento ruidoso|] ≥
//! |segmento real|` (a norma é convexa, e perturbar os extremos infla a
//! distância esperada). O viés cresce quando o movimento por quadro é
//! pequeno em relação ao ruído — exatamente o regime de fiscalização.
//! Superestimar velocidade é a direção perigosa em perícia, então o
//! estimador foi trocado pela regressão por eixo, que herda a
//! imparcialidade da regressão linear simples em cada componente.
//! (O termo `vy²` ainda introduz um viés positivo de segunda ordem, mas
//! proporcional à *variância da inclinação* — ordens de magnitude menor
//! que o viés do comprimento de caminho.)
//!
//! O retorno inclui:
//!
//!   - rapidez `v` e componentes `vx`, `vy` (m/s + km/h)
//!   - erro padrão da rapidez (delta method, assumindo ruído isotrópico
//!     no plano do mundo)
//!   - IC 95% via Student's t (tabela para df pequeno; aproximação
//!     normal `z = 1.960` para `df ≥ 30`)
//!   - R² conjunto do ajuste (X + Y)
//!   - resíduo 2D por ponto (distância à trajetória ajustada — útil pra
//!     detectar outliers / curvatura)
//!
//! **Premissa:** movimento aproximadamente retilíneo e uniforme na
//! janela analisada. Trajetórias com curvatura forte violam o modelo
//! linear (R² cai) e devem ser segmentadas pelo perito. A análise de
//! incerteza rigorosa (propagando ruído de pixel pela homografia) vive
//! no módulo `montecarlo`.

/// Velocidade com as duas unidades comuns disponíveis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Velocity {
    /// Metros por segundo (unidade base).
    pub m_per_s: f64,
    /// Quilômetros por hora (conversão direta `* 3.6`).
    pub km_per_h: f64,
}

impl Velocity {
    /// Constrói a partir de m/s.
    pub fn from_m_per_s(v: f64) -> Self {
        Self {
            m_per_s: v,
            km_per_h: v * 3.6,
        }
    }
}

/// Resultado completo da regressão linear de velocidade.
#[derive(Debug, Clone)]
pub struct RegressionResult {
    /// Rapidez estimada `v = sqrt(vx² + vy²)`.
    pub velocity: Velocity,
    /// Componente X da velocidade (m/s), inclinação da regressão `X(t)`.
    pub vx_m_per_s: f64,
    /// Componente Y da velocidade (m/s), inclinação da regressão `Y(t)`.
    pub vy_m_per_s: f64,
    /// Erro padrão da rapidez (m/s), via delta method com variância de
    /// resíduo agrupada entre os eixos (premissa de ruído isotrópico).
    pub se_m_per_s: f64,
    /// IC 95% da rapidez em m/s `(lo, hi)`. Pode conter valores
    /// negativos em cenas extremamente ruidosas (matematicamente válido,
    /// fisicamente improvável).
    pub ci95_m_per_s: (f64, f64),
    /// IC 95% em km/h, igual ao anterior multiplicado por 3.6.
    pub ci95_km_per_h: (f64, f64),
    /// R² conjunto (X + Y), no intervalo [0, 1]. Valores próximos de 1
    /// indicam trajetória bem aproximada por velocidade constante.
    pub r_squared: f64,
    /// Graus de liberdade `2·(n - 2)` (4 parâmetros: vx, bx, vy, by).
    pub degrees_of_freedom: usize,
    /// Resíduo 2D (em metros) de cada ponto em relação à trajetória
    /// ajustada: `||Pᵢ - (v⃗·tᵢ + b⃗)||`.
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

/// Velocidade média escalar entre dois pontos do mundo (em metros) com
/// timestamps em segundos.
///
/// Retorna o módulo `|d| / |Δt|` — não preserva sinal porque uma
/// velocidade "média" entre dois instantes pode ser interpretada como
/// rapidez, e velocidade vetorial não cabe numa interface 1D.
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

/// Regressão por eixo de `X(t)` e `Y(t)` para 3+ pontos.
///
/// Os pontos devem estar na ordem cronológica (idem para `times`).
/// Ajusta `X(t) = vx·t + bx` e `Y(t) = vy·t + by` independentemente por
/// mínimos quadrados, e retorna a rapidez `v = sqrt(vx² + vy²)`.
///
/// Diferente de regredir comprimento de caminho acumulado, este
/// estimador é (de primeira ordem) imparcial sob ruído de marcação —
/// ver doc do módulo para a justificativa via desigualdade de Jensen.
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

    // Médias.
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

    // Inclinações (componentes da velocidade) e interceptos.
    let vx = s_tx / s_tt;
    let vy = s_ty / s_tt;
    let bx = x_mean - vx * t_mean;
    let by = y_mean - vy * t_mean;
    let speed = (vx * vx + vy * vy).sqrt();

    // Resíduos 2D + somas de quadrados (residual e total) por eixo.
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

    // Variância de resíduo AGRUPADA entre os eixos (premissa de ruído
    // isotrópico no plano do mundo). 2n observações - 4 parâmetros.
    // Com ruído isotrópico, var(vx) = var(vy) = pooled_mse/s_tt, e o
    // delta method colapsa elegantemente para SE_v = sqrt(pooled_mse/s_tt)
    // — sem divisão por v (logo sem singularidade quando v → 0).
    let df = 2 * (n - 2);
    let pooled_mse = (ss_res_x + ss_res_y) / df as f64;
    let se_v = (pooled_mse / s_tt).max(0.0).sqrt();

    // R² conjunto.
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

/// Valor crítico de Student's t para IC 95% bilateral (α/2 = 0.025) por
/// graus de liberdade. Tabela explícita para `df ∈ [1, 29]` e
/// aproximação normal `1.960` para `df ≥ 30`.
///
/// Fontes (tabela padrão de Student's t):
///   - NIST Engineering Statistics Handbook §1.3.6.7.2
///   - Tabela tabular típica de livros-texto de estatística.
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

    /// Movimento unidimensional a 10 m/s ao longo do eixo X em 1 segundo.
    #[test]
    fn average_velocity_constant_motion() {
        let v = average_velocity((0.0, 0.0), 0.0, (10.0, 0.0), 1.0).unwrap();
        assert!((v.m_per_s - 10.0).abs() < 1e-12);
        assert!((v.km_per_h - 36.0).abs() < 1e-12);
    }

    /// Trio pitagórico 5-12-13: distância 13m em 1s = 13 m/s.
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

    /// Cena ideal: 5 pontos em movimento retilíneo a 10 m/s. A regressão
    /// deve recuperar exatamente 10.0 m/s com SE_slope ≈ 0 e R² = 1.
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
        // Componentes: tudo em X.
        assert!((res.vx_m_per_s - 10.0).abs() < 1e-9, "vx = {}", res.vx_m_per_s);
        assert!(res.vy_m_per_s.abs() < 1e-9, "vy = {}", res.vy_m_per_s);
        assert!(res.r_squared > 0.9999999);
        assert!(res.se_m_per_s < 1e-9);
        // Resíduos todos ~zero.
        for r in &res.residuals {
            assert!(r.abs() < 1e-9);
        }
    }

    /// Cena ruidosa controlada: velocidade real 20 m/s + perturbação
    /// determinística pequena. O IC 95% deve conter o valor verdadeiro.
    #[test]
    fn regression_ci_contains_true_velocity_with_small_noise() {
        let true_v = 20.0;
        // 10 amostras, dt = 0.1s. Posição = v·t + ε(i), ε pequeno.
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
        // SE > 0 porque há ruído.
        assert!(res.se_m_per_s > 0.0);
        // R² deve ser alto mas não 1.
        assert!(res.r_squared > 0.99);
    }

    /// Movimento diagonal constante: vx = 3, vy = 4 → rapidez = 5 m/s.
    /// Confirma que a regressão por eixo recupera as componentes e a
    /// magnitude corretamente.
    #[test]
    fn regression_diagonal_constant_velocity() {
        // A cada 0.1s avança 0.3m em X e 0.4m em Y (5 m/s na diagonal).
        let times = vec![0.0, 0.1, 0.2, 0.3, 0.4];
        let points: Vec<(f64, f64)> = times.iter().map(|&t| (3.0 * t, 4.0 * t)).collect();
        let res = regression_velocity(&points, &times).unwrap();
        assert!((res.vx_m_per_s - 3.0).abs() < 1e-9, "vx = {}", res.vx_m_per_s);
        assert!((res.vy_m_per_s - 4.0).abs() < 1e-9, "vy = {}", res.vy_m_per_s);
        assert!((res.velocity.m_per_s - 5.0).abs() < 1e-9, "v = {}", res.velocity.m_per_s);
        assert!((res.velocity.km_per_h - 18.0).abs() < 1e-9);
    }

    /// Regressão por eixo mede a velocidade vetorial líquida (taxa de
    /// deslocamento do melhor ajuste linear), NÃO o comprimento de
    /// caminho. Numa poligonal em L, o estimador antigo (comprimento)
    /// daria 10 m/s (2m / 0.2s); o per-axis dá a magnitude do vetor
    /// velocidade médio. Documenta a mudança de semântica de propósito.
    #[test]
    fn regression_per_axis_measures_net_velocity_not_path_length() {
        let points = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)];
        let times = vec![0.0, 0.1, 0.2];
        let res = regression_velocity(&points, &times).unwrap();
        // X(t): [0,1,1] → vx = 5; Y(t): [0,0,1] → vy = 5; v = sqrt(50) ≈ 7.07.
        assert!(
            (res.velocity.m_per_s - 50.0_f64.sqrt()).abs() < 1e-9,
            "v = {} (esperado sqrt(50) ≈ 7.071)",
            res.velocity.m_per_s
        );
        // R² < 1 porque a poligonal em L não é perfeitamente linear no tempo.
        assert!(res.r_squared < 1.0);
    }

    /// **Teste-chave da troca de modelo**: sob ruído de marcação
    /// simétrico, o estimador por eixo NÃO enviesa para cima como o de
    /// comprimento de caminho acumulado. Roda muitas realizações
    /// ruidosas (seed fixo) e compara as médias dos dois estimadores.
    #[test]
    fn per_axis_does_not_bias_upward_like_path_length() {
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        use rand_distr::{Distribution, Normal};

        // Regime que amplifica o viés do comprimento de caminho:
        // velocidade baixa (2 m/s) → 0.2 m por quadro, e ruído σ=0.1 m
        // por coordenada (grande relativo ao movimento por quadro).
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
            // Perturba ambos os eixos com ruído simétrico (média 0).
            let pts: Vec<(f64, f64)> = true_x
                .iter()
                .map(|&x| (x + noise.sample(&mut rng), noise.sample(&mut rng)))
                .collect();

            // Estimador novo (per-axis).
            let res = regression_velocity(&pts, &times).unwrap();
            sum_per_axis += res.velocity.m_per_s;

            // Estimador antigo (comprimento de caminho), inline pra
            // comparação direta no mesmo conjunto perturbado.
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

        // (a) Per-axis fica próximo do verdadeiro (viés de 2ª ordem só).
        assert!(
            (mean_per_axis - true_v).abs() < 0.2,
            "per-axis enviesado: média = {mean_per_axis}, verdadeiro = {true_v}"
        );
        // (b) Comprimento de caminho superestima claramente o verdadeiro.
        assert!(
            mean_path_length > true_v + 0.1,
            "comprimento de caminho deveria superestimar: média = {mean_path_length}"
        );
        // (c) E é nitidamente mais alto que o per-axis (a tese do ajuste).
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

    /// `Velocity::from_m_per_s` faz a conversão correta m/s → km/h.
    #[test]
    fn velocity_unit_conversion() {
        let v = Velocity::from_m_per_s(10.0);
        assert!((v.km_per_h - 36.0).abs() < 1e-12);
        let v2 = Velocity::from_m_per_s(0.0);
        assert_eq!(v2.km_per_h, 0.0);
    }
}
