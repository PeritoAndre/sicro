//! Calibração imagem ↔ mundo: homografia 3×3 por DLT de 4 pontos (Hartley &
//! Zisserman §4.1) ou homografia afim por linha (2 pontos + distância real).

use nalgebra::{DMatrix, Matrix3, Vector3};

#[derive(Debug, thiserror::Error)]
pub enum HomographyError {
    /// Sem solução numérica: pontos colineares ou coincidentes.
    #[error("SVD não convergiu — pontos de calibração podem ser colineares ou coincidentes")]
    SvdFailed,
    /// W = 0 na projeção (ponto no infinito) ou calibração degenerada.
    #[error("homografia singular (componente projetivo zero)")]
    Singular,
    #[error("distância real precisa ser > 0, recebido {0}")]
    InvalidDistance(f64),
}

/// Homografia pixel → metros: `[X·W, Y·W, W]ᵀ = H · [x, y, 1]ᵀ`, dividindo por W.
#[derive(Debug, Clone, Copy)]
pub struct Homography {
    pub h: Matrix3<f64>,
}

impl Homography {
    pub fn from_matrix(h: Matrix3<f64>) -> Self {
        Self { h }
    }

    /// Pixel → mundo (m). `Singular` se W ≈ 0.
    pub fn project(&self, pixel: (f64, f64)) -> Result<(f64, f64), HomographyError> {
        let p = Vector3::new(pixel.0, pixel.1, 1.0);
        let r = self.h * p;
        if r.z.abs() < 1e-12 {
            return Err(HomographyError::Singular);
        }
        Ok((r.x / r.z, r.y / r.z))
    }

    /// Distância (m) entre o mundo observado e a projeção do pixel.
    pub fn reprojection_residual(
        &self,
        image_pt: (f64, f64),
        world_pt: (f64, f64),
    ) -> Result<f64, HomographyError> {
        let (x, y) = self.project(image_pt)?;
        let dx = x - world_pt.0;
        let dy = y - world_pt.1;
        Ok((dx * dx + dy * dy).sqrt())
    }
}

/// DLT com 4 correspondências (pixel ↔ metros, mesma ordem): sistema 8×9
/// `A·h = 0`, `h` = autovetor do menor autovalor de `AᵀA`; normalizada para
/// `h33 = 1` quando possível.
pub fn solve_homography_dlt(
    image_pts: &[(f64, f64); 4],
    world_pts: &[(f64, f64); 4],
) -> Result<Homography, HomographyError> {
    // Normalização de Hartley (H&Z §4.4.4): centroide na origem, distância média
    // √2. Sem isso o DLT fica instável com escalas tão diferentes (pixel vs metro).
    let (t_img, img_n) = hartley_normalize(image_pts);
    let (t_world, world_n) = hartley_normalize(world_pts);

    // Linhas de A por correspondência (H&Z §4.1, wᵢ' = 1):
    //   [   0,   0,  0, -xᵢ, -yᵢ, -1,  Yᵢ·xᵢ,  Yᵢ·yᵢ,  Yᵢ ]
    //   [  xᵢ,  yᵢ,  1,   0,   0,  0, -Xᵢ·xᵢ, -Xᵢ·yᵢ, -Xᵢ ]
    let mut a = DMatrix::<f64>::zeros(8, 9);
    for i in 0..4 {
        let (xp, yp) = img_n[i];
        let (xw, yw) = world_n[i];
        let r1 = 2 * i;
        let r2 = r1 + 1;

        a[(r1, 3)] = -xp;
        a[(r1, 4)] = -yp;
        a[(r1, 5)] = -1.0;
        a[(r1, 6)] = yw * xp;
        a[(r1, 7)] = yw * yp;
        a[(r1, 8)] = yw;

        a[(r2, 0)] = xp;
        a[(r2, 1)] = yp;
        a[(r2, 2)] = 1.0;
        a[(r2, 6)] = -xw * xp;
        a[(r2, 7)] = -xw * yp;
        a[(r2, 8)] = -xw;
    }

    // Núcleo de A pelo autovetor do menor autovalor de AᵀA (equivale ao SVD).
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
        return Err(HomographyError::SvdFailed);
    }
    let h_vec = eig.eigenvectors.column(min_idx);

    let h_normalized_space = Matrix3::new(
        h_vec[0], h_vec[1], h_vec[2], h_vec[3], h_vec[4], h_vec[5], h_vec[6], h_vec[7], h_vec[8],
    );

    // Desnormalização: H = T_world⁻¹ · H_n · T_img.
    let t_world_inv = t_world
        .try_inverse()
        .ok_or(HomographyError::SvdFailed)?;
    let h_denorm = t_world_inv * h_normalized_space * t_img;

    // Fixa a escala (h e λ·h são a mesma homografia) em h33 = 1; se h33 ~ 0,
    // pela maior magnitude da última linha.
    let h22 = h_denorm[(2, 2)];
    let h_final = if h22.abs() > 1e-12 {
        h_denorm / h22
    } else {
        let max_last_row = h_denorm[(2, 0)]
            .abs()
            .max(h_denorm[(2, 1)].abs())
            .max(h22.abs());
        if max_last_row > 1e-12 {
            h_denorm / max_last_row
        } else {
            h_denorm
        }
    };

    Ok(Homography { h: h_final })
}

/// Normalização de Hartley: T afim leva o centroide à origem e a distância
/// média a √2. Devolve `(T, pontos normalizados)`.
fn hartley_normalize(pts: &[(f64, f64); 4]) -> (Matrix3<f64>, [(f64, f64); 4]) {
    let cx: f64 = pts.iter().map(|p| p.0).sum::<f64>() / 4.0;
    let cy: f64 = pts.iter().map(|p| p.1).sum::<f64>() / 4.0;

    let mean_dist: f64 = pts
        .iter()
        .map(|p| ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt())
        .sum::<f64>()
        / 4.0;

    // Pontos coincidentes: s = 1 evita divisão por zero; o DLT falha a jusante.
    let s = if mean_dist > 1e-12 {
        std::f64::consts::SQRT_2 / mean_dist
    } else {
        1.0
    };

    let t = Matrix3::new(s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0);

    let normalized = [
        (s * (pts[0].0 - cx), s * (pts[0].1 - cy)),
        (s * (pts[1].0 - cx), s * (pts[1].1 - cy)),
        (s * (pts[2].0 - cx), s * (pts[2].1 - cy)),
        (s * (pts[3].0 - cx), s * (pts[3].1 - cy)),
    ];

    (t, normalized)
}

/// Homografia afim de 2 pontos + distância: `p1 ↦ (0, 0)`, `p2 ↦ (distance_m, 0)`.
/// `H = S · R(−θ) · T(−p1)`, escala `distance_m / |p2 − p1|` (m/px); sem
/// termo projetivo (uma linha não modela perspectiva).
pub fn line_calibration(
    p1_image: (f64, f64),
    p2_image: (f64, f64),
    distance_m: f64,
) -> Result<Homography, HomographyError> {
    if distance_m <= 0.0 || !distance_m.is_finite() {
        return Err(HomographyError::InvalidDistance(distance_m));
    }
    let dx = p2_image.0 - p1_image.0;
    let dy = p2_image.1 - p1_image.1;
    let d_pixels = (dx * dx + dy * dy).sqrt();
    if d_pixels < 1e-9 {
        return Err(HomographyError::Singular);
    }

    let s = distance_m / d_pixels;
    let theta = dy.atan2(dx);
    let (sin_t, cos_t) = theta.sin_cos();

    let h = Matrix3::new(
        s * cos_t,
        s * sin_t,
        -s * (cos_t * p1_image.0 + sin_t * p1_image.1),
        -s * sin_t,
        s * cos_t,
        s * (sin_t * p1_image.0 - cos_t * p1_image.1),
        0.0,
        0.0,
        1.0,
    );

    Ok(Homography { h })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 px = 1 m → diagonal 0.01, h33 = 1.
    #[test]
    fn dlt_recovers_identity_scale() {
        let image_pts = [
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
        ];
        let world_pts = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let h = solve_homography_dlt(&image_pts, &world_pts).unwrap();
        assert!((h.h[(0, 0)] - 0.01).abs() < 1e-9, "h00 = {}", h.h[(0, 0)]);
        assert!((h.h[(1, 1)] - 0.01).abs() < 1e-9, "h11 = {}", h.h[(1, 1)]);
        assert!((h.h[(2, 2)] - 1.0).abs() < 1e-9);
        assert!(h.h[(0, 1)].abs() < 1e-9);
        assert!(h.h[(1, 0)].abs() < 1e-9);
    }

    #[test]
    fn dlt_projects_center() {
        let image_pts = [
            (0.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
        ];
        let world_pts = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let h = solve_homography_dlt(&image_pts, &world_pts).unwrap();
        let (x, y) = h.project((50.0, 50.0)).unwrap();
        assert!((x - 0.5).abs() < 1e-9, "x = {}", x);
        assert!((y - 0.5).abs() < 1e-9, "y = {}", y);
    }

    #[test]
    fn dlt_zero_residual_at_calibration_points() {
        // Quadrilátero em perspectiva → retângulo 5×4 m.
        let image_pts = [
            (10.0, 10.0),
            (200.0, 15.0),
            (210.0, 180.0),
            (5.0, 200.0),
        ];
        let world_pts = [(0.0, 0.0), (5.0, 0.0), (5.0, 4.0), (0.0, 4.0)];
        let h = solve_homography_dlt(&image_pts, &world_pts).unwrap();
        for i in 0..4 {
            let r = h.reprojection_residual(image_pts[i], world_pts[i]).unwrap();
            assert!(r < 1e-9, "resíduo[{i}] = {r}");
        }
    }

    #[test]
    fn line_calibration_maps_p1_to_origin() {
        let h = line_calibration((100.0, 200.0), (200.0, 200.0), 10.0).unwrap();
        let (x, y) = h.project((100.0, 200.0)).unwrap();
        assert!(x.abs() < 1e-9, "x = {x}");
        assert!(y.abs() < 1e-9, "y = {y}");
    }

    #[test]
    fn line_calibration_maps_p2_to_distance() {
        let h = line_calibration((100.0, 200.0), (200.0, 200.0), 10.0).unwrap();
        let (x, y) = h.project((200.0, 200.0)).unwrap();
        assert!((x - 10.0).abs() < 1e-9, "x = {x}");
        assert!(y.abs() < 1e-9, "y = {y}");
    }

    /// Segmento vertical: o ponto médio cai em metade da distância, com Y = 0.
    #[test]
    fn line_calibration_vertical_segment() {
        let h = line_calibration((100.0, 100.0), (100.0, 200.0), 5.0).unwrap();
        let (x, y) = h.project((100.0, 150.0)).unwrap();
        assert!((x - 2.5).abs() < 1e-9, "x = {x}");
        assert!(y.abs() < 1e-9, "y = {y}");
    }

    #[test]
    fn line_calibration_diagonal_segment_perpendicular_y() {
        // Diagonal de 45°, 10 m no total: o ponto médio deve dar (5, 0).
        let h = line_calibration((0.0, 0.0), (100.0, 100.0), 10.0).unwrap();
        let (x, y) = h.project((50.0, 50.0)).unwrap();
        assert!((x - 5.0).abs() < 1e-9, "x = {x}");
        assert!(y.abs() < 1e-9, "y = {y}");
    }

    #[test]
    fn line_calibration_rejects_zero_distance() {
        let result = line_calibration((0.0, 0.0), (100.0, 0.0), 0.0);
        assert!(matches!(result, Err(HomographyError::InvalidDistance(_))));
    }

    #[test]
    fn line_calibration_rejects_negative_distance() {
        let result = line_calibration((0.0, 0.0), (100.0, 0.0), -5.0);
        assert!(matches!(result, Err(HomographyError::InvalidDistance(_))));
    }

    #[test]
    fn line_calibration_rejects_coincident_points() {
        let result = line_calibration((100.0, 100.0), (100.0, 100.0), 10.0);
        assert!(matches!(result, Err(HomographyError::Singular)));
    }

    #[test]
    fn dlt_perspective_quad_round_trip() {
        // Trapézio na imagem → retângulo 4×3 m.
        let image_pts = [
            (100.0, 200.0),
            (300.0, 200.0),
            (350.0, 100.0),
            (50.0, 100.0),
        ];
        let world_pts = [(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 3.0)];
        let h = solve_homography_dlt(&image_pts, &world_pts).unwrap();
        for i in 0..4 {
            let r = h.reprojection_residual(image_pts[i], world_pts[i]).unwrap();
            assert!(r < 1e-9, "resíduo no canto {i} = {r}");
        }
    }

    /// Homografia verdadeira com termo projetivo (`h31, h32 ≠ 0`): o DLT dos 4
    /// cantos tem de reproduzir um ponto interior que não entrou no ajuste —
    /// resíduo nos cantos é ~0 por construção; só a perspectiva completa acerta o interior.
    #[test]
    fn dlt_recovers_perspective_at_unseen_interior_point() {
        use nalgebra::Matrix3;

        let h_true = Homography::from_matrix(Matrix3::new(
            0.0200, 0.0010, 0.50, 0.0015, 0.0180, 0.30, 0.0001, 0.0002, 1.0,
        ));

        let corners_img = [
            (100.0, 100.0),
            (500.0, 120.0),
            (480.0, 400.0),
            (90.0, 420.0),
        ];
        let interior_img = (300.0, 260.0);

        let corners_world = [
            h_true.project(corners_img[0]).unwrap(),
            h_true.project(corners_img[1]).unwrap(),
            h_true.project(corners_img[2]).unwrap(),
            h_true.project(corners_img[3]).unwrap(),
        ];
        let interior_world_true = h_true.project(interior_img).unwrap();

        let h_est = solve_homography_dlt(&corners_img, &corners_world).unwrap();

        for i in 0..4 {
            let r = h_est
                .reprojection_residual(corners_img[i], corners_world[i])
                .unwrap();
            assert!(r < 1e-9, "resíduo no canto {i} = {r}");
        }

        let interior_est = h_est.project(interior_img).unwrap();
        let dx = interior_est.0 - interior_world_true.0;
        let dy = interior_est.1 - interior_world_true.1;
        let residual = (dx * dx + dy * dy).sqrt();
        assert!(
            residual < 1e-9,
            "resíduo no ponto interior não-visto = {residual} (esperado ~0); \
             interior_est = {interior_est:?}, verdadeiro = {interior_world_true:?}"
        );
    }
}
