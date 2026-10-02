//! Calibração por razão cruzada: `>= 3` pontos colineares de posição real
//! conhecida sobre a linha de tráfego → projetividade 1D `w = (a·s + b)/(c·s + d)`
//! (3 graus de liberdade: exata com 3 pontos, mínimos quadrados com 4+).
//! Mede posição 1D ao longo da linha; a velocidade fica em `velocity`.

use nalgebra::{DMatrix, Matrix2, Matrix3};

use crate::video::speed::homography::Homography;

#[derive(Debug, thiserror::Error)]
pub enum CrossRatioError {
    /// Linha exige 2 pontos; projetividade exige 3.
    #[error("pontos de referência insuficientes (recebido {0})")]
    InsufficientPoints(usize),
    #[error("vetores de tamanhos diferentes (imagem {image}, mundo {world})")]
    DimensionMismatch { image: usize, world: usize },
    /// Variância ~0: não define linha nem projetividade.
    #[error("pontos coincidentes ou degenerados — não definem a transformação")]
    CoincidentPoints,
    /// Mapa constante, ou ponto no polo (`c·s + d → 0`).
    #[error("projetividade singular (mapeamento degenerado ou ponto no infinito)")]
    Singular,
}

/// `(a,b;c,d) = ((a−c)(b−d)) / ((a−d)(b−c))`, invariante sob projetividade 1D.
/// Pontos coincidentes dão `±∞`/`NaN`.
pub fn cross_ratio(a: f64, b: f64, c: f64, d: f64) -> f64 {
    ((a - c) * (b - d)) / ((a - d) * (b - c))
}

/// Linha de tráfego na imagem: centroide + direção unitária (1ª componente
/// principal dos pontos de referência — robusta a ruído de marcação).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrafficLine {
    pub anchor: (f64, f64),
    pub direction: (f64, f64),
}

/// Ajusta a linha por PCA (`>= 2` pontos).
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

    // Autovetor de maior autovalor de [[sxx, sxy], [sxy, syy]], em forma fechada.
    let theta = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let direction = (theta.cos(), theta.sin());

    Ok(TrafficLine {
        anchor: (cx, cy),
        direction,
    })
}

/// Coordenada 1D do pixel sobre a linha (distância com sinal a partir da âncora).
pub fn project_onto_line(line: &TrafficLine, px: f64, py: f64) -> f64 {
    let dx = px - line.anchor.0;
    let dy = py - line.anchor.1;
    dx * line.direction.0 + dy * line.direction.1
}

/// Projetividade 1D `w = (a·s + b) / (c·s + d)` (Möbius), matriz `[[a, b], [c, d]]`
/// definida a menos de escala.
#[derive(Debug, Clone, Copy)]
pub struct Projectivity1D {
    pub m: Matrix2<f64>,
}

impl Projectivity1D {
    pub fn from_matrix(m: Matrix2<f64>) -> Self {
        Self { m }
    }

    /// Escalar de imagem `s` → posição de mundo (m). `Singular` no polo
    /// (`c·s + d ≈ 0`, limiar relativo às magnitudes do denominador).
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

/// Resolve `w = (a·s + b)/(c·s + d)` de pares `(sᵢ, wᵢ)`: cada par dá
/// `[sᵢ, 1, −sᵢ·wᵢ, −wᵢ] · [a, b, c, d]ᵀ = 0`; o núcleo é o autovetor de menor
/// autovalor de `AᵀA` (exato com 3 pontos, mínimos quadrados com 4+).
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

    // Entradas normalizadas (média 0, RMS 1) por estabilidade numérica.
    let (mu_s, sig_s) = mean_scale(image_scalars)?;
    let (mu_w, sig_w) = mean_scale(world_scalars)?;

    let mut a = DMatrix::<f64>::zeros(n, 4);
    for i in 0..n {
        let s_n = (image_scalars[i] - mu_s) / sig_s;
        let w_n = (world_scalars[i] - mu_w) / sig_w;
        a[(i, 0)] = s_n;
        a[(i, 1)] = 1.0;
        a[(i, 2)] = -s_n * w_n;
        a[(i, 3)] = -w_n;
    }

    // Núcleo de A pelo autovetor do menor autovalor de AᵀA (4×4 simétrica PSD).
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
    let m_n = Matrix2::new(h[0], h[1], h[2], h[3]);

    // Desnormalização por composição de Möbius: M = N_w⁻¹ · M_n · N_s, com
    // N_s = [[1/σs, −μs/σs], [0, 1]] e N_w⁻¹ = [[σw, μw], [0, 1]].
    let ns = Matrix2::new(1.0 / sig_s, -mu_s / sig_s, 0.0, 1.0);
    let nw_inv = Matrix2::new(sig_w, mu_w, 0.0, 1.0);
    let raw = nw_inv * m_n * ns;

    // Escala pela maior magnitude (map é invariante à escala); det ~0 ⇒ mapa constante.
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
// Levantamento (lift) da projetividade 1D para uma homografia 3×3

/// Referência colinear: pixel + posição real ao longo da linha (m).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossRatioReference {
    pub px: f64,
    pub py: f64,
    pub world_m: f64,
}

/// Levanta a projetividade 1D para uma 3×3 que `Homography::project` já usa:
/// `L = [dir.x, dir.y, −dir·anchor]` (L·[px,py,1] = s) e
/// `H = [a·L + b·e3; 0; c·L + d·e3]`, logo `project` devolve `((a·s+b)/(c·s+d), 0)`.
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

/// Linha (PCA) + projetividade 1D + lift → homografia 3×3, de `>= 3` referências.
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

/// Média e desvio (RMS); `CoincidentPoints` se o desvio for ~0.
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

    fn mob(s: f64, a: f64, b: f64, c: f64, d: f64) -> f64 {
        (a * s + b) / (c * s + d)
    }

    #[test]
    fn cross_ratio_invariant_under_projectivity() {
        let (a, b, c, d) = (1.0, 2.0, 4.0, 8.0);
        let cr0 = cross_ratio(a, b, c, d);

        // Möbius não-degenerada (det = 2·5 − 3·1 = 7 ≠ 0).
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

    #[test]
    fn cross_ratio_known_value() {
        // ((1−4)(2−8)) / ((1−8)(2−4)) = (−3·−6)/(−7·−2) = 18/14 = 9/7.
        let cr = cross_ratio(1.0, 2.0, 4.0, 8.0);
        assert!((cr - 9.0 / 7.0).abs() < 1e-12, "cr = {cr}");
    }

    #[test]
    fn projectivity_from_three_maps_fourth_exactly() {
        // Mapa verdadeiro com termo projetivo (c ≠ 0).
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

    /// Pipeline completo (linha + projeção + projetividade) com 3 referências
    /// recupera exato a posição de mundo de um 4º ponto (o "veículo").
    #[test]
    fn perspective_along_line_recovers_vehicle_world_position() {
        // Mundo → escalar de imagem com perspectiva: s(w) = w / (0.01·w + 1).
        let s_of_w = |w: f64| mob(w, 1.0, 0.0, 0.01, 1.0);

        // Linha de tráfego fora dos eixos.
        let inv_len = 1.0 / (10.0_f64).sqrt();
        let dir = (3.0 * inv_len, 1.0 * inv_len);
        let origin = (100.0, 200.0);
        let img_point = |w: f64| {
            let s = s_of_w(w);
            (origin.0 + s * dir.0, origin.1 + s * dir.1)
        };

        let w_refs = [0.0, 8.0, 20.0];
        let w_vehicle = 35.0;

        let ref_points: Vec<(f64, f64)> = w_refs.iter().map(|&w| img_point(w)).collect();
        let vehicle_point = img_point(w_vehicle);

        let line = fit_traffic_line(&ref_points).unwrap();

        let img_scalars: Vec<f64> = ref_points
            .iter()
            .map(|&(px, py)| project_onto_line(&line, px, py))
            .collect();

        let p = fit_1d_projectivity(&img_scalars, &w_refs).unwrap();

        let s_vehicle = project_onto_line(&line, vehicle_point.0, vehicle_point.1);
        let got = p.map(s_vehicle).unwrap();

        assert!(
            (got - w_vehicle).abs() < 1e-7,
            "posição do veículo: esperado {w_vehicle} m, obtido {got} m"
        );
    }

    #[test]
    fn affine_case_reduces_to_linear_scale() {
        // w = 3·s + 2 (sem termo projetivo).
        let img = [0.0, 1.0, 2.0];
        let world = [2.0, 5.0, 8.0];

        let p = fit_1d_projectivity(&img, &world).unwrap();

        let got = p.map(4.0).unwrap();
        assert!((got - 14.0).abs() < 1e-9, "afim: esperado 14.0, obtido {got}");

        // c desprezível frente a d.
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
        // O sinal da direção é ambíguo (autovetor): |dot| ~ 1.
        let dot = line.direction.0 * dir.0 + line.direction.1 * dir.1;
        assert!(dot.abs() > 1.0 - 1e-9, "direção divergente: dot={dot}");
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
        // w = s/(s − 2): polo em s = 2 (det = −2 ≠ 0).
        let p = Projectivity1D::from_matrix(Matrix2::new(1.0, 0.0, 1.0, -2.0));
        assert!(matches!(p.map(2.0), Err(CrossRatioError::Singular)));
        assert!((p.map(0.0).unwrap() - 0.0).abs() < 1e-12);
    }

    /// A 3×3 levantada projeta cada referência para `(world_m, 0)` e um ponto
    /// interior (fora do ajuste) para a posição esperada.
    #[test]
    fn lifted_homography_projects_references_and_interior() {
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

        for r in &refs {
            let (x, y) = h.project((r.px, r.py)).unwrap();
            assert!((x - r.world_m).abs() < 1e-7, "ref x={x}, esperado {}", r.world_m);
            assert!(y.abs() < 1e-7, "ref y={y} deveria ser ~0");
        }

        let w_interior = 35.0;
        let (px, py) = img(w_interior);
        let (x, y) = h.project((px, py)).unwrap();
        assert!((x - w_interior).abs() < 1e-7, "interior x={x}, esperado {w_interior}");
        assert!(y.abs() < 1e-7, "interior y={y} deveria ser ~0");
    }
}
