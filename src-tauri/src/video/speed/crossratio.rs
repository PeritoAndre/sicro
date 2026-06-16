//! Calibração por **razão cruzada** (3º modo) — matemática pura.
//!
//! Cenário: o veículo se move ao longo de uma **linha** (eixo de tráfego)
//! vista em perspectiva. O perito marca `>= 3` pontos de referência
//! **colineares** sobre essa linha, de **posição real conhecida** (em metros,
//! medidos ao longo da própria linha). Não é preciso um plano calibrado
//! inteiro: basta o eixo de movimento.
//!
//! ## Por que funciona
//!
//! A **razão cruzada** (cross-ratio) de 4 pontos colineares é o invariante
//! fundamental da geometria projetiva: ela se conserva sob qualquer
//! projetividade 1D (P¹ → P¹), inclusive a perspectiva que comprime as
//! distâncias ao longo da linha conforme ela se afasta da câmera.
//!
//! Uma projetividade 1D
//!
//! ```text
//!   w = (a·s + b) / (c·s + d)
//! ```
//!
//! (transformação de Möbius real) tem **3 graus de liberdade** (os 4
//! coeficientes a menos de um fator de escala comum). Logo `>= 3`
//! correspondências `imagem ↔ mundo` a determinam: exata para 3, mínimos
//! quadrados para 4+.
//!
//! ## Pipeline
//!
//! ```text
//!   [pontos de referência na imagem (px,py)] ──→ fit_traffic_line (PCA)
//!                                                   │ âncora + direção
//!                                                   ▼
//!   cada (px,py) ──→ project_onto_line ──→ escalar de imagem (px ao longo da linha)
//!                                                   │
//!   [posições reais conhecidas (m)] ────────────────┤
//!                                                   ▼
//!                              fit_1d_projectivity (Möbius)
//!                                                   │
//!   [pixel do veículo] → project_onto_line → s ── map ──→ posição no mundo (m)
//! ```
//!
//! ## Limitações desta fase
//!
//! - Matemática pura: sem DB, sem UI, sem comandos.
//! - Mede posição **1D ao longo da linha** — não a posição 2D no plano.
//!   Velocidade = derivada dessa posição no tempo (feito a jusante por
//!   `velocity`/`montecarlo`, fora deste arquivo).

use nalgebra::{DMatrix, Matrix2, Matrix3};

use crate::video::speed::homography::Homography;

/// Erros da calibração por razão cruzada.
#[derive(Debug, thiserror::Error)]
pub enum CrossRatioError {
    /// Menos correspondências/pontos do que o mínimo necessário (linha exige
    /// 2; projetividade exige 3).
    #[error("pontos de referência insuficientes (recebido {0})")]
    InsufficientPoints(usize),
    /// Vetores de imagem e mundo com tamanhos diferentes.
    #[error("vetores de tamanhos diferentes (imagem {image}, mundo {world})")]
    DimensionMismatch { image: usize, world: usize },
    /// Pontos coincidentes / sem espalhamento — não definem linha nem
    /// projetividade (variância ~0).
    #[error("pontos coincidentes ou degenerados — não definem a transformação")]
    CoincidentPoints,
    /// Projetividade singular: mapeamento degenerado (constante) ou o ponto
    /// caiu no polo (`c·s + d → 0`, ponto no infinito).
    #[error("projetividade singular (mapeamento degenerado ou ponto no infinito)")]
    Singular,
}

/// Razão cruzada de 4 escalares colineares `a, b, c, d`.
///
/// Usa a convenção `(a,b;c,d) = ((a−c)(b−d)) / ((a−d)(b−c))`, que é
/// **invariante sob projetividade 1D** (ver teste `cross_ratio_invariant_*`).
///
/// Pontos coincidentes podem zerar o denominador; nesse caso o resultado é
/// `±∞`/`NaN` (comportamento de ponto flutuante), refletindo a degeneração —
/// o chamador deve garantir pontos distintos.
pub fn cross_ratio(a: f64, b: f64, c: f64, d: f64) -> f64 {
    ((a - c) * (b - d)) / ((a - d) * (b - c))
}

/// Linha de tráfego no plano da imagem: âncora (centroide) + direção unitária.
///
/// A direção é a primeira componente principal (PCA / mínimos quadrados
/// totais) dos pontos de referência — robusta a ruído de marcação, já que os
/// pontos são apenas *nominalmente* colineares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrafficLine {
    /// Ponto de âncora (centroide dos pontos de referência).
    pub anchor: (f64, f64),
    /// Direção unitária da linha (componente principal).
    pub direction: (f64, f64),
}

/// Ajusta a linha de tráfego a partir dos pontos de referência (PCA).
///
/// Exige `>= 2` pontos. A direção é o autovetor de maior autovalor da matriz
/// de covariância 2×2 — i.e., a direção de maior variância, que é o eixo da
/// linha. Se todos os pontos coincidem (variância ~0), retorna
/// `CoincidentPoints`.
pub fn fit_traffic_line(points: &[(f64, f64)]) -> Result<TrafficLine, CrossRatioError> {
    let n = points.len();
    if n < 2 {
        return Err(CrossRatioError::InsufficientPoints(n));
    }
    let nf = n as f64;
    let cx = points.iter().map(|p| p.0).sum::<f64>() / nf;
    let cy = points.iter().map(|p| p.1).sum::<f64>() / nf;

    let mut sxx = 0.0;
    let mut sxy = 0.0;
    let mut syy = 0.0;
    for &(x, y) in points {
        let dx = x - cx;
        let dy = y - cy;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    if sxx + syy < 1e-12 {
        return Err(CrossRatioError::CoincidentPoints);
    }

    // Direção do autovetor de MAIOR autovalor da covariância simétrica
    // [[sxx, sxy], [sxy, syy]]. Fórmula fechada do ângulo do eixo principal.
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let direction = (theta.cos(), theta.sin());

    Ok(TrafficLine {
        anchor: (cx, cy),
        direction,
    })
}

/// Projeta `(px, py)` na linha e devolve a **distância com sinal** ao longo da
/// direção, a partir da âncora. É a coordenada 1D do ponto sobre a linha.
pub fn project_onto_line(line: &TrafficLine, px: f64, py: f64) -> f64 {
    let dx = px - line.anchor.0;
    let dy = py - line.anchor.1;
    dx * line.direction.0 + dy * line.direction.1
}

/// Projetividade 1D `w = (a·s + b) / (c·s + d)`, representada pela matriz 2×2
/// `[[a, b], [c, d]]` (transformação de Möbius real).
#[derive(Debug, Clone, Copy)]
pub struct Projectivity1D {
    /// Matriz `[[a, b], [c, d]]` — definida a menos de escala global.
    pub m: Matrix2<f64>,
}

impl Projectivity1D {
    /// Constrói diretamente de uma matriz 2×2 (testes / inspeção).
    pub fn from_matrix(m: Matrix2<f64>) -> Self {
        Self { m }
    }

    /// Mapeia um escalar de imagem `s` para a posição de mundo `w` (metros).
    ///
    /// Retorna `Singular` se `s` cair no polo da projetividade
    /// (`c·s + d ≈ 0`, ponto no infinito) — usamos limiar relativo às
    /// magnitudes que formam o denominador para detectar o cancelamento.
    pub fn map(&self, s: f64) -> Result<f64, CrossRatioError> {
        let a = self.m[(0, 0)];
        let b = self.m[(0, 1)];
        let c = self.m[(1, 0)];
        let d = self.m[(1, 1)];
        let num = a * s + b;
        let den = c * s + d;
        let den_scale = (c * s).abs() + d.abs();
        if den.abs() <= 1e-12 * den_scale.max(1.0) {
            return Err(CrossRatioError::Singular);
        }
        Ok(num / den)
    }
}

/// Resolve a projetividade 1D `w = (a·s + b)/(c·s + d)` a partir de
/// correspondências `imagem ↔ mundo`.
///
/// Cada par `(sᵢ, wᵢ)` dá a equação linear homogênea
///
/// ```text
///   [sᵢ, 1, −sᵢ·wᵢ, −wᵢ] · [a, b, c, d]ᵀ = 0
/// ```
///
/// (de `wᵢ·(c·sᵢ + d) = a·sᵢ + b`). Resolvemos o sistema homogêneo pelo
/// autovetor do menor autovalor de `AᵀA` (igual ao `homography`),
/// **exato para 3 pontos** (núcleo 1D) e **mínimos quadrados para 4+**.
///
/// Entradas são normalizadas (média 0, RMS 1) por estabilidade numérica; a
/// normalização é absorvida no resultado por composição de Möbius (produto de
/// matrizes 2×2), então o `map` opera direto no escalar bruto.
///
/// # Erros
/// - `DimensionMismatch` — tamanhos diferentes.
/// - `InsufficientPoints` — menos de 3 correspondências.
/// - `CoincidentPoints` — escalares de imagem (ou de mundo) sem espalhamento.
/// - `Singular` — a projetividade ajustada é degenerada (mapa constante).
pub fn fit_1d_projectivity(
    image_scalars: &[f64],
    world_scalars: &[f64],
) -> Result<Projectivity1D, CrossRatioError> {
    if image_scalars.len() != world_scalars.len() {
        return Err(CrossRatioError::DimensionMismatch {
            image: image_scalars.len(),
            world: world_scalars.len(),
        });
    }
    let n = image_scalars.len();
    if n < 3 {
        return Err(CrossRatioError::InsufficientPoints(n));
    }

    // Normalização 1D (média 0, desvio 1) em cada eixo.
    let (mu_s, sig_s) = mean_scale(image_scalars)?;
    let (mu_w, sig_w) = mean_scale(world_scalars)?;

    // Matriz A (n×4) com os pares normalizados.
    let mut a = DMatrix::<f64>::zeros(n, 4);
    for i in 0..n {
        let s_n = (image_scalars[i] - mu_s) / sig_s;
        let w_n = (world_scalars[i] - mu_w) / sig_w;
        a[(i, 0)] = s_n;
        a[(i, 1)] = 1.0;
        a[(i, 2)] = -s_n * w_n;
        a[(i, 3)] = -w_n;
    }

    // Núcleo de A via autovetor do menor autovalor de AᵀA (4×4 simétrica PSD).
    let ata = a.transpose() * a;
    let eig = ata.symmetric_eigen();
    let mut min_idx = 0usize;
    let mut min_val = f64::INFINITY;
    for i in 0..eig.eigenvalues.len() {
        let v = eig.eigenvalues[i];
        if v.is_finite() && v < min_val {
            min_val = v;
            min_idx = i;
        }
    }
    if !min_val.is_finite() {
        return Err(CrossRatioError::Singular);
    }
    let h = eig.eigenvectors.column(min_idx);
    // M no espaço normalizado: w_n = (a_n·s_n + b_n)/(c_n·s_n + d_n).
    let m_n = Matrix2::new(h[0], h[1], h[2], h[3]);

    // Desnormalização por composição de Möbius:
    //   M = N_w⁻¹ · M_n · N_s
    //   N_s   : s ↦ (s − μs)/σs   = [[1/σs, −μs/σs], [0, 1]]
    //   N_w⁻¹ : w_n ↦ σw·w_n + μw = [[σw, μw], [0, 1]]
    let ns = Matrix2::new(1.0 / sig_s, -mu_s / sig_s, 0.0, 1.0);
    let nw_inv = Matrix2::new(sig_w, mu_w, 0.0, 1.0);
    let raw = nw_inv * m_n * ns;

    // Normaliza pela maior magnitude (map é invariante à escala) e checa
    // degeneração: det ~0 ⇒ mapa constante (todos os s vão para um ponto).
    let max_abs = raw.iter().fold(0.0_f64, |acc, &v| acc.max(v.abs()));
    if max_abs < 1e-15 {
        return Err(CrossRatioError::Singular);
    }
    let m = raw / max_abs;
    let det = m[(0, 0)] * m[(1, 1)] - m[(0, 1)] * m[(1, 0)];
    if det.abs() < 1e-9 * m.norm().max(1.0) {
        return Err(CrossRatioError::Singular);
    }

    Ok(Projectivity1D { m })
}

// ---------------------------------------------------------------------------
// Levantamento (lift) da projetividade 1D para uma homografia 3×3.

/// Uma referência colinear: pixel `(px, py)` + posição real `world_m` ao longo
/// da linha (metros). `world_y` é implicitamente 0 (a referência está sobre a
/// linha de tráfego).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossRatioReference {
    pub px: f64,
    pub py: f64,
    /// Posição real ao longo da linha, em metros.
    pub world_m: f64,
}

/// **Levanta** a projetividade 1D (`w = (a·s+b)/(c·s+d)`) sobre a linha para
/// uma homografia 3×3 que reusa exatamente a infraestrutura de `Homography`:
///
/// ```text
///   L = [dir.x, dir.y, −(dir·anchor)]   (L·[px,py,1] = s, o escalar na linha)
///   H.row0 = a·L + b·e3                 (= a·s + b)
///   H.row1 = [0, 0, 0]                  (= 0)
///   H.row2 = c·L + d·e3                 (= c·s + d)
/// ```
///
/// Assim `H.project((px,py))` devolve `((a·s+b)/(c·s+d), 0) = (posição no
/// mundo, 0)` — exatamente o que o caminho de regressão de `compute_speed`
/// espera, sem precisar de coluna/serialização nova.
pub fn lift_projectivity_to_homography(line: &TrafficLine, proj: &Projectivity1D) -> Homography {
    let (dx, dy) = line.direction;
    let (ax, ay) = line.anchor;
    let lz = -(dx * ax + dy * ay);
    let a = proj.m[(0, 0)];
    let b = proj.m[(0, 1)];
    let c = proj.m[(1, 0)];
    let d = proj.m[(1, 1)];
    let m = Matrix3::new(
        a * dx, a * dy, a * lz + b,
        0.0, 0.0, 0.0,
        c * dx, c * dy, c * lz + d,
    );
    Homography::from_matrix(m)
}

/// Ajusta a calibração por razão cruzada a partir de `>= 3` referências
/// colineares e devolve a homografia 3×3 levantada (linha + projetividade 1D
/// → `lift_projectivity_to_homography`).
///
/// `references[i].world_m` é a posição real ao longo da linha (m).
pub fn fit_cross_ratio_homography(
    references: &[CrossRatioReference],
) -> Result<Homography, CrossRatioError> {
    if references.len() < 3 {
        return Err(CrossRatioError::InsufficientPoints(references.len()));
    }
    let image_pts: Vec<(f64, f64)> = references.iter().map(|r| (r.px, r.py)).collect();
    let world_scalars: Vec<f64> = references.iter().map(|r| r.world_m).collect();
    let line = fit_traffic_line(&image_pts)?;
    let image_scalars: Vec<f64> = image_pts
        .iter()
        .map(|&(px, py)| project_onto_line(&line, px, py))
        .collect();
    let proj = fit_1d_projectivity(&image_scalars, &world_scalars)?;
    Ok(lift_projectivity_to_homography(&line, &proj))
}

/// Média e desvio-padrão (RMS em torno da média). Erro se o desvio for ~0
/// (todos os valores coincidem → eixo degenerado).
fn mean_scale(xs: &[f64]) -> Result<(f64, f64), CrossRatioError> {
    let n = xs.len() as f64;
    let mu = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mu).powi(2)).sum::<f64>() / n;
    let sig = var.sqrt();
    if sig < 1e-12 {
        return Err(CrossRatioError::CoincidentPoints);
    }
    Ok((mu, sig))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Aplica uma Möbius `w = (a·s+b)/(c·s+d)` (helper de teste).
    fn mob(s: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
        (a * s + b) / (c * s + d)
    }

    /// A razão cruzada é invariante sob uma projetividade 1D conhecida.
    #[test]
    fn cross_ratio_invariant_under_projectivity() {
        let (a, b, c, d) = (1.0, 2.0, 4.0, 8.0);
        let cr0 = cross_ratio(a, b, c, d);

        // Möbius arbitrária não-degenerada (det = 2·5 − 3·1 = 7 ≠ 0).
        let (ma, mb, mc, md) = (2.0, 3.0, 1.0, 5.0);
        let wa = mob(a, ma, mb, mc, md);
        let wb = mob(b, ma, mb, mc, md);
        let wc = mob(c, ma, mb, mc, md);
        let wd = mob(d, ma, mb, mc, md);
        let cr1 = cross_ratio(wa, wb, wc, wd);

        assert!(
            (cr0 - cr1).abs() < 1e-12,
            "razão cruzada não invariante: {cr0} vs {cr1}"
        );
    }

    /// Valor numérico conhecido da razão cruzada (sanidade da convenção).
    #[test]
    fn cross_ratio_known_value() {
        // ((1−4)(2−8)) / ((1−8)(2−4)) = (−3·−6)/(−7·−2) = 18/14 = 9/7.
        let cr = cross_ratio(1.0, 2.0, 4.0, 8.0);
        assert!((cr - 9.0 / 7.0).abs() < 1e-12, "cr = {cr}");
    }

    /// Projetividade recuperada de 3 correspondências mapeia EXATO um 4º ponto.
    #[test]
    fn projectivity_from_three_maps_fourth_exactly() {
        // Mapa verdadeiro imagem→mundo, com termo projetivo (c ≠ 0).
        let (ta, tb, tc, td) = (2.0, 1.0, 0.5, 1.0); // det = 2 − 0.5 = 1.5 ≠ 0
        let img = [1.0, 3.0, 7.0];
        let world: Vec<f64> = img.iter().map(|&s| mob(s, ta, tb, tc, td)).collect();

        let p = fit_1d_projectivity(&img, &world).unwrap();

        let s4 = 5.0;
        let expected = mob(s4, ta, tb, tc, td);
        let got = p.map(s4).unwrap();
        assert!(
            (got - expected).abs() < 1e-9,
            "4º ponto: esperado {expected}, obtido {got}"
        );
    }

    /// Cenário sintético de perspectiva AO LONGO DA LINHA: posições de mundo
    /// conhecidas projetadas à imagem por uma projetividade 1D conhecida;
    /// dadas 3 referências, o pipeline (linha + projeção + projetividade)
    /// recupera EXATO a posição de mundo de um 4º ponto (o "veículo").
    #[test]
    fn perspective_along_line_recovers_vehicle_world_position() {
        // Mapa verdadeiro MUNDO→ESCALAR-DE-IMAGEM (perspectiva, c ≠ 0):
        //   s(w) = (1·w + 0) / (0.01·w + 1)
        let s_of_w = |w: f64| mob(w, 1.0, 0.0, 0.01, 1.0);

        // Linha de tráfego arbitrária na imagem (direção não-eixo).
        let inv_len = 1.0 / (10.0_f64).sqrt();
        let dir = (3.0 * inv_len, 1.0 * inv_len);
        let origin = (100.0, 200.0);
        let img_point = |w: f64| {
            let s = s_of_w(w);
            (origin.0 + s * dir.0, origin.1 + s * dir.1)
        };

        // 3 referências de mundo conhecidas + 1 veículo.
        let w_refs = [0.0, 8.0, 20.0];
        let w_vehicle = 35.0;

        let ref_points: Vec<(f64, f64)> = w_refs.iter().map(|&w| img_point(w)).collect();
        let vehicle_point = img_point(w_vehicle);

        // 1) ajusta a linha pelos pontos de referência.
        let line = fit_traffic_line(&ref_points).unwrap();

        // 2) projeta cada referência → escalar de imagem.
        let img_scalars: Vec<f64> = ref_points
            .iter()
            .map(|&(px, py)| project_onto_line(&line, px, py))
            .collect();

        // 3) ajusta a projetividade imagem→mundo.
        let p = fit_1d_projectivity(&img_scalars, &w_refs).unwrap();

        // 4) projeta o veículo e mapeia para o mundo.
        let s_vehicle = project_onto_line(&line, vehicle_point.0, vehicle_point.1);
        let got = p.map(s_vehicle).unwrap();

        assert!(
            (got - w_vehicle).abs() < 1e-7,
            "posição do veículo: esperado {w_vehicle} m, obtido {got} m"
        );
    }

    /// Caso afim (sem perspectiva): a projetividade reduz a uma escala linear
    /// (`c ≈ 0`) e mapeia corretamente.
    #[test]
    fn affine_case_reduces_to_linear_scale() {
        // Mapa afim verdadeiro: w = 3·s + 2 (sem termo projetivo).
        let img = [0.0, 1.0, 2.0];
        let world = [2.0, 5.0, 8.0];

        let p = fit_1d_projectivity(&img, &world).unwrap();

        // Mapeia um 4º ponto exatamente.
        let got = p.map(4.0).unwrap();
        assert!((got - 14.0).abs() < 1e-9, "afim: esperado 14.0, obtido {got}");

        // Termo projetivo (c) deve ser desprezível frente a d.
        let c = p.m[(1, 0)].abs();
        let d = p.m[(1, 1)].abs().max(1.0);
        assert!(
            c < 1e-6 * d,
            "esperado caso afim (c≈0), mas c={c}, d={}",
            p.m[(1, 1)]
        );
    }

    #[test]
    fn fit_traffic_line_recovers_direction() {
        // Pontos colineares ao longo de (3,1)/√10.
        let inv_len = 1.0 / (10.0_f64).sqrt();
        let dir = (3.0 * inv_len, 1.0 * inv_len);
        let pts: Vec<(f64, f64)> = [0.0, 5.0, 11.0, 20.0]
            .iter()
            .map(|&t| (10.0 + t * dir.0, 50.0 + t * dir.1))
            .collect();
        let line = fit_traffic_line(&pts).unwrap();
        // Direção pode vir com sinal invertido (ambiguidade do autovetor);
        // o |produto interno| com a direção verdadeira deve ser ~1.
        let dot = line.direction.0 * dir.0 + line.direction.1 * dir.1;
        assert!(dot.abs() > 1.0 - 1e-9, "direção divergente: dot={dot}");
        // A âncora é o centroide.
        assert!((line.anchor.0 - (10.0 + 9.0 * dir.0)).abs() < 1e-9);
    }

    #[test]
    fn fit_rejects_too_few_points() {
        let r = fit_1d_projectivity(&[1.0, 2.0], &[1.0, 2.0]);
        assert!(matches!(r, Err(CrossRatioError::InsufficientPoints(2))));
    }

    #[test]
    fn fit_rejects_dimension_mismatch() {
        let r = fit_1d_projectivity(&[1.0, 2.0, 3.0], &[1.0, 2.0]);
        assert!(matches!(
            r,
            Err(CrossRatioError::DimensionMismatch { image: 3, world: 2 })
        ));
    }

    #[test]
    fn fit_rejects_coincident_world() {
        // Todas as posições de mundo iguais → eixo de mundo degenerado.
        let r = fit_1d_projectivity(&[1.0, 2.0, 3.0], &[5.0, 5.0, 5.0]);
        assert!(matches!(r, Err(CrossRatioError::CoincidentPoints)));
    }

    #[test]
    fn fit_traffic_line_rejects_coincident_points() {
        let r = fit_traffic_line(&[(3.0, 4.0), (3.0, 4.0), (3.0, 4.0)]);
        assert!(matches!(r, Err(CrossRatioError::CoincidentPoints)));
    }

    #[test]
    fn map_rejects_point_at_infinity() {
        // w = (1·s + 0)/(1·s − 2): polo em s = 2 (det = −2 ≠ 0, não-degenerada).
        let p = Projectivity1D::from_matrix(Matrix2::new(1.0, 0.0, 1.0, -2.0));
        assert!(matches!(p.map(2.0), Err(CrossRatioError::Singular)));
        // Fora do polo, mapeia normalmente.
        assert!((p.map(0.0).unwrap() - 0.0).abs() < 1e-12);
    }

    /// A 3×3 levantada projeta cada referência para `(world_m, 0)` com resíduo
    /// ~0, e um ponto interior conhecido (não usado no ajuste) para a posição
    /// esperada — exato no caso de perspectiva.
    #[test]
    fn lifted_homography_projects_references_and_interior() {
        // Mapa verdadeiro MUNDO→ESCALAR-DE-IMAGEM (perspectiva, c ≠ 0).
        let s_of_w = |w: f64| mob(w, 1.0, 0.0, 0.01, 1.0);
        let inv = 1.0 / (10.0_f64).sqrt();
        let dir = (3.0 * inv, 1.0 * inv);
        let origin = (120.0, 220.0);
        let img = |w: f64| {
            let s = s_of_w(w);
            (origin.0 + s * dir.0, origin.1 + s * dir.1)
        };

        let refs = [
            CrossRatioReference { px: img(0.0).0, py: img(0.0).1, world_m: 0.0 },
            CrossRatioReference { px: img(8.0).0, py: img(8.0).1, world_m: 8.0 },
            CrossRatioReference { px: img(20.0).0, py: img(20.0).1, world_m: 20.0 },
        ];
        let h = fit_cross_ratio_homography(&refs).unwrap();

        // Cada referência projeta para (world_m, 0).
        for r in &refs {
            let (x, y) = h.project((r.px, r.py)).unwrap();
            assert!((x - r.world_m).abs() < 1e-7, "ref x={x}, esperado {}", r.world_m);
            assert!(y.abs() < 1e-7, "ref y={y} deveria ser ~0");
        }

        // Ponto interior conhecido (não usado no ajuste) → exato.
        let w_interior = 35.0;
        let (px, py) = img(w_interior);
        let (x, y) = h.project((px, py)).unwrap();
        assert!((x - w_interior).abs() < 1e-7, "interior x={x}, esperado {w_interior}");
        assert!(y.abs() < 1e-7, "interior y={y} deveria ser ~0");
    }
}
