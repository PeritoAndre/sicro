//! Distância real entre dois pixels via `Homography::project`. Plano (DLT):
//! euclidiana no solo; linha (afim): na escala da referência; razão cruzada:
//! a 3×3 projeta para `(s, 0)`, logo sai `|s₂ − s₁|` ao longo da linha.

use crate::video::speed::homography::{Homography, HomographyError};

#[derive(Debug, thiserror::Error)]
pub enum MeasureError {
    /// Algum ponto projetou no infinito.
    #[error(transparent)]
    Homography(#[from] HomographyError),
}

/// Distância (m) entre dois pixels; `Singular` se algum projeta no infinito.
pub fn world_distance(
    homography: &Homography,
    p1_px: (f64, f64),
    p2_px: (f64, f64),
) -> Result<f64, MeasureError> {
    let (x1, y1) = homography.project(p1_px)?;
    let (x2, y2) = homography.project(p2_px)?;
    let dx = x2 - x1;
    let dy = y2 - y1;
    Ok((dx * dx + dy * dy).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::speed::crossratio::{fit_cross_ratio_homography, CrossRatioReference};
    use crate::video::speed::homography::{line_calibration, solve_homography_dlt};
    use nalgebra::Matrix3;

    #[test]
    fn identity_scale_recovers_known_distance() {
        let image = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let world = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let h = solve_homography_dlt(&image, &world).unwrap();

        // (20,30) → (0.2,0.3) ; (80,70) → (0.8,0.7) ; Δ = (0.6, 0.4).
        let d = world_distance(&h, (20.0, 30.0), (80.0, 70.0)).unwrap();
        let expected = (0.6_f64.powi(2) + 0.4_f64.powi(2)).sqrt();
        assert!((d - expected).abs() < 1e-9, "d = {d}, esperado {expected}");
    }

    #[test]
    fn identity_scale_axis_aligned_one_meter() {
        let image = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let world = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let h = solve_homography_dlt(&image, &world).unwrap();
        let d = world_distance(&h, (0.0, 0.0), (100.0, 0.0)).unwrap();
        assert!((d - 1.0).abs() < 1e-9, "d = {d}");
    }

    /// Homografia verdadeira com perspectiva: o DLT dos 4 cantos tem de medir
    /// certo entre dois pixels que não entraram no ajuste.
    #[test]
    fn perspective_plane_recovers_known_distance() {
        let h_true = Homography::from_matrix(Matrix3::new(
            0.0200, 0.0010, 0.50, 0.0015, 0.0180, 0.30, 0.0001, 0.0002, 1.0,
        ));
        let corners_img = [(100.0, 100.0), (500.0, 120.0), (480.0, 400.0), (90.0, 420.0)];
        let corners_world = [
            h_true.project(corners_img[0]).unwrap(),
            h_true.project(corners_img[1]).unwrap(),
            h_true.project(corners_img[2]).unwrap(),
            h_true.project(corners_img[3]).unwrap(),
        ];
        let h_est = solve_homography_dlt(&corners_img, &corners_world).unwrap();

        let a = (220.0, 180.0);
        let b = (430.0, 350.0);
        let (ax, ay) = h_true.project(a).unwrap();
        let (bx, by) = h_true.project(b).unwrap();
        let expected = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();

        let got = world_distance(&h_est, a, b).unwrap();
        assert!((got - expected).abs() < 1e-9, "got = {got}, esperado {expected}");
    }

    /// Razão cruzada mede ao longo da linha: 2.5 m → 12.5 m = 10 m.
    #[test]
    fn cross_ratio_distance_along_line() {
        let refs = vec![
            CrossRatioReference { px: 100.0, py: 200.0, world_m: 0.0 },
            CrossRatioReference { px: 200.0, py: 200.0, world_m: 5.0 },
            CrossRatioReference { px: 300.0, py: 200.0, world_m: 10.0 },
        ];
        let h = fit_cross_ratio_homography(&refs).unwrap();

        let (mx, my) = h.project((200.0, 200.0)).unwrap();
        assert!((mx - 5.0).abs() < 1e-6, "mx = {mx}");
        assert!(my.abs() < 1e-6, "my = {my}");

        // 150 px → 2.5 m ; 350 px → 12.5 m ; distância 10 m.
        let d = world_distance(&h, (150.0, 200.0), (350.0, 200.0)).unwrap();
        assert!((d - 10.0).abs() < 1e-6, "d = {d}");
    }

    #[test]
    fn line_calibration_distance_in_scale() {
        let h = line_calibration((100.0, 200.0), (300.0, 200.0), 8.0).unwrap();
        let d = world_distance(&h, (100.0, 200.0), (300.0, 200.0)).unwrap();
        assert!((d - 8.0).abs() < 1e-9, "d = {d}");
    }

    /// Última linha `[0, 0, 0]` zera o denominador projetivo.
    #[test]
    fn point_at_infinity_errors() {
        let h = Homography::from_matrix(Matrix3::new(
            1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0,
        ));
        let r = world_distance(&h, (10.0, 10.0), (20.0, 20.0));
        assert!(matches!(r, Err(MeasureError::Homography(HomographyError::Singular))));
    }
}
