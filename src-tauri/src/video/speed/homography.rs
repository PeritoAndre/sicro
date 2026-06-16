//! Calibração imagem ↔ mundo para o Calculador de Velocidade.
//!
//! Dois modos de calibração:
//!
//! ### 1. DLT de 4 pontos (`solve_homography_dlt`)
//!
//! Direct Linear Transform clássico (Hartley & Zisserman §4.1). Dado 4
//! pares de correspondência `(xᵢ, yᵢ) ↔ (Xᵢ, Yᵢ)` — pixel ↔ metros —
//! resolve a homografia 3×3 H tal que
//!
//! ```text
//! [X·W]            [x]
//! [Y·W]  =  H  ·   [y]
//! [W  ]            [1]
//! ```
//!
//! O sistema linear 8×9 é resolvido por SVD; a solução é o vetor singular
//! à direita correspondente ao menor valor singular. Funciona pra qualquer
//! plano (faixa de pedestres, lajota da calçada, retângulo demarcado no
//! solo), inclusive com perspectiva.
//!
//! ### 2. Calibração por linha (`line_calibration`)
//!
//! Caso degenerado, sem perspectiva: dado 2 pontos da imagem e a distância
//! real entre eles em metros, produz uma homografia **afim** (escala +
//! rotação + translação, sem termo projetivo). Útil quando a única
//! referência métrica é uma faixa contínua, distância conhecida entre
//! postes, ou similar.
//!
//! Por construção, `p1` é mapeado para `(0, 0)` no mundo e `p2` para
//! `(distance_m, 0)` — ou seja, o eixo X do mundo é alinhado com o
//! sentido p1 → p2.

use nalgebra::{DMatrix, Matrix3, Vector3};

/// Erros da camada de homografia. Cobrem desde validação de entrada
/// (poucos pontos, distância zero) até falhas numéricas do SVD.
#[derive(Debug, thiserror::Error)]
pub enum HomographyError {
    /// SVD da matriz DLT não produziu solução utilizável. Geralmente
    /// indica pontos colineares ou coincidentes na calibração.
    #[error("SVD não convergiu — pontos de calibração podem ser colineares ou coincidentes")]
    SvdFailed,
    /// Após a projeção, o componente W deu 0 (ponto no infinito) ou a
    /// matriz de calibração foi degenerada (escala 0, distância 0).
    #[error("homografia singular (componente projetivo zero)")]
    Singular,
    /// Entrada inválida — distância em metros tem que ser positiva.
    #[error("distância real precisa ser > 0, recebido {0}")]
    InvalidDistance(f64),
}

/// Homografia 3×3 que mapeia coordenadas de **imagem (pixels)** para
/// coordenadas de **mundo (metros)**.
///
/// A multiplicação é aplicada como `[X·W, Y·W, W]ᵀ = H · [x, y, 1]ᵀ`,
/// dividindo por `W` no final para extrair `(X, Y)`.
#[derive(Debug, Clone, Copy)]
pub struct Homography {
    /// Matriz 3×3 da transformação. Acessível para serialização /
    /// inspeção externa, mas em geral use `project` e
    /// `reprojection_residual` em vez de mexer aqui.
    pub h: Matrix3<f64>,
}

impl Homography {
    /// Constrói uma homografia diretamente de uma matriz 3×3.
    /// Útil para testes; produção deve usar `solve_homography_dlt` ou
    /// `line_calibration`.
    pub fn from_matrix(h: Matrix3<f64>) -> Self {
        Self { h }
    }

    /// Aplica a homografia em um pixel `(x, y)` e retorna a coordenada
    /// de mundo `(X, Y)` em metros.
    ///
    /// Retorna `Singular` se o ponto projetar no infinito (W = 0).
    pub fn project(&self, pixel: (f64, f64)) -> Result<(f64, f64), HomographyError> {
        let p = Vector3::new(pixel.0, pixel.1, 1.0);
        let r = self.h * p;
        if r.z.abs() < 1e-12 {
            return Err(HomographyError::Singular);
        }
        Ok((r.x / r.z, r.y / r.z))
    }

    /// Resíduo de reprojeção: distância euclidiana (em metros) entre o
    /// ponto de mundo observado e o que sai ao projetar o pixel via H.
    ///
    /// Para uma calibração exata (DLT em 4 pontos sem ruído), o resíduo
    /// nos 4 pontos de calibração deve ser ~0. Para calibração ruidosa
    /// ou com mais de 4 pontos (não suportado nesta fase), o resíduo
    /// quantifica a qualidade do ajuste.
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

/// Resolve a homografia 3×3 por DLT a partir de 4 correspondências.
///
/// Cada correspondência contribui 2 equações lineares na incógnita
/// `h = [h11, h12, h13, h21, h22, h23, h31, h32, h33]ᵀ`, totalizando uma
/// matriz 8×9. A solução é o vetor singular à direita do menor valor
/// singular do SVD (i.e., o vetor que minimiza `||A·h||` sujeito a
/// `||h|| = 1`). Resultado é normalizado para `h33 = 1` quando possível.
///
/// # Parâmetros
/// * `image_pts[i]` — pixel `(x, y)` do `i`-ésimo ponto de calibração.
/// * `world_pts[i]` — metros `(X, Y)` do mesmo ponto no plano do mundo.
///
/// Ordem das correspondências deve casar entre os dois arrays.
pub fn solve_homography_dlt(
    image_pts: &[(f64, f64); 4],
    world_pts: &[(f64, f64); 4],
) -> Result<Homography, HomographyError> {
    // Normalização de Hartley (H&Z §4.4.4): translada cada conjunto pra
    // centroide na origem e escala pra distância média = sqrt(2). Sem
    // isso, o DLT é numericamente instável quando as coordenadas estão
    // em ranges muito diferentes (típico de pixel vs metro).
    let (t_img, img_n) = hartley_normalize(image_pts);
    let (t_world, world_n) = hartley_normalize(world_pts);

    // Constrói matriz A 8×9 com pontos normalizados. Cada par i de
    // correspondência (xᵢ, yᵢ) ↔ (Xᵢ, Yᵢ) contribui as duas linhas abaixo
    // (forma padrão H&Z §4.1 com wᵢ' = 1):
    //
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

    // Resolvemos `A·h = 0` via eigendecomposição de `Aᵀ·A` (9×9
    // simétrica positiva semi-definida). O autovetor correspondente ao
    // menor autovalor é o `h` procurado.
    //
    // Equivalente ao "right singular vector do menor valor singular" do
    // SVD de A, e numericamente estável para o caso DLT (matriz bem
    // condicionada pela escala já normalizada).
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
    // (Pontos normalizados → H_n → pontos normalizados; precisamos
    // mapear pixel cru → metro cru.)
    let t_world_inv = t_world
        .try_inverse()
        .ok_or(HomographyError::SvdFailed)?;
    let h_denorm = t_world_inv * h_normalized_space * t_img;

    // Normaliza pro elemento [2][2] = 1 quando possível. Isso fixa a
    // ambiguidade de escala global (h e λ·h representam a mesma
    // homografia). Se [2][2] ~ 0 (caso afim degenerado), normaliza pela
    // maior magnitude da última linha pra manter números bem
    // condicionados.
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

/// Normalização de Hartley para um conjunto de 4 pontos.
///
/// Calcula a transformação afim T (3×3) tal que:
///   - centroide dos pontos transformados é a origem;
///   - distância média dos pontos transformados à origem é `sqrt(2)`.
///
/// Retorna `(T, pontos_normalizados)` para uso no DLT. A inversa de T
/// é necessária na desnormalização do resultado.
fn hartley_normalize(pts: &[(f64, f64); 4]) -> (Matrix3<f64>, [(f64, f64); 4]) {
    // Centroide.
    let cx: f64 = pts.iter().map(|p| p.0).sum::<f64>() / 4.0;
    let cy: f64 = pts.iter().map(|p| p.1).sum::<f64>() / 4.0;

    // Distância média ao centroide.
    let mean_dist: f64 = pts
        .iter()
        .map(|p| ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt())
        .sum::<f64>()
        / 4.0;

    // Fator de escala (s ≈ sqrt(2)/mean_dist). Se a distância média é
    // zero (todos os pontos coincidentes), `s = 1` evita divisão por
    // zero — o DLT a jusante vai falhar de qualquer forma e o erro
    // chega ao chamador.
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

/// Calibração afim por linha: 2 pontos de imagem + distância real.
///
/// Produz uma homografia tal que:
///   - `p1_image` ↦ `(0, 0)` no mundo
///   - `p2_image` ↦ `(distance_m, 0)` no mundo
///
/// Equivale a uma composição `escala · rotação(-θ) · translação(-p1)`,
/// onde `θ` é o ângulo do segmento p1→p2 no plano da imagem e a escala
/// é `distance_m / |p2 - p1|_pixels` (metros por pixel).
///
/// O resultado é uma homografia afim (última linha `[0, 0, 1]`), sem
/// componente projetivo — não há como modelar perspectiva apenas com
/// uma linha de referência.
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

    // Escala (metros por pixel) e ângulo do segmento na imagem.
    let s = distance_m / d_pixels;
    let theta = dy.atan2(dx);
    let (sin_t, cos_t) = theta.sin_cos();

    // H = S · R(-θ) · T(-p1), expandido em forma matricial 3×3.
    // Derivação completa nos comentários do módulo + testes verificam
    // o mapeamento de p1 e p2.
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

    /// Calibração escala-identidade: 100 pixels = 1 metro. Espera-se
    /// uma homografia diagonal com escala 0.01 e h33 = 1.
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
        // Off-diagonais devem ser ~0
        assert!(h.h[(0, 1)].abs() < 1e-9);
        assert!(h.h[(1, 0)].abs() < 1e-9);
    }

    /// Projetar o centro do retângulo de calibração deve dar o centro
    /// do mundo (0.5, 0.5).
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

    /// Para uma calibração exata (DLT em 4 pontos), o resíduo de
    /// reprojeção nos 4 pontos deve ser ~zero (precisão de máquina).
    #[test]
    fn dlt_zero_residual_at_calibration_points() {
        // Quadrilátero arbitrário (perspectiva) → retângulo 5×4 no mundo.
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

    /// `line_calibration` deve mapear o primeiro ponto à origem do mundo.
    #[test]
    fn line_calibration_maps_p1_to_origin() {
        let h = line_calibration((100.0, 200.0), (200.0, 200.0), 10.0).unwrap();
        let (x, y) = h.project((100.0, 200.0)).unwrap();
        assert!(x.abs() < 1e-9, "x = {x}");
        assert!(y.abs() < 1e-9, "y = {y}");
    }

    /// `line_calibration` deve mapear o segundo ponto exatamente para
    /// `(distance_m, 0)`.
    #[test]
    fn line_calibration_maps_p2_to_distance() {
        let h = line_calibration((100.0, 200.0), (200.0, 200.0), 10.0).unwrap();
        let (x, y) = h.project((200.0, 200.0)).unwrap();
        assert!((x - 10.0).abs() < 1e-9, "x = {x}");
        assert!(y.abs() < 1e-9, "y = {y}");
    }

    /// Linha vertical em pixel: o midpoint pixel deve cair em metade da
    /// distância real, ainda sobre o eixo X do mundo (Y = 0).
    #[test]
    fn line_calibration_vertical_segment() {
        let h = line_calibration((100.0, 100.0), (100.0, 200.0), 5.0).unwrap();
        let (x, y) = h.project((100.0, 150.0)).unwrap();
        assert!((x - 2.5).abs() < 1e-9, "x = {x}");
        assert!(y.abs() < 1e-9, "y = {y}");
    }

    /// Calibração por linha diagonal: ponto perpendicular ao segmento
    /// fica no eixo Y do mundo.
    #[test]
    fn line_calibration_diagonal_segment_perpendicular_y() {
        // p1 = (0,0), p2 = (100, 100) — diagonal de 45°.
        // Distância real = sqrt(2) * 5 = ~7.071 m (10m por pixel * 100 px na diagonal? não, é 10 m total)
        let h = line_calibration((0.0, 0.0), (100.0, 100.0), 10.0).unwrap();
        // Midpoint sobre a linha deve ser (5, 0).
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

    /// Verifica reprojeção em uma cena com perspectiva real: trapézio na
    /// imagem (câmera olhando o chão de viés) → retângulo 4×3 no mundo.
    /// Projetar pontos arbitrários e verificar via projeção inversa em
    /// dois pontos colineares.
    #[test]
    fn dlt_perspective_quad_round_trip() {
        // Trapézio na imagem (visualização em perspectiva de um retângulo).
        let image_pts = [
            (100.0, 200.0),
            (300.0, 200.0),
            (350.0, 100.0),
            (50.0, 100.0),
        ];
        // Retângulo 4×3 no mundo (em metros).
        let world_pts = [(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 3.0)];
        let h = solve_homography_dlt(&image_pts, &world_pts).unwrap();
        for i in 0..4 {
            let r = h.reprojection_residual(image_pts[i], world_pts[i]).unwrap();
            assert!(r < 1e-9, "resíduo no canto {i} = {r}");
        }
    }

    /// **Validação real da perspectiva**: parte de uma homografia
    /// verdadeira COM termo projetivo (`h31, h32 ≠ 0`), gera 4 cantos +
    /// 1 ponto INTERIOR com coordenadas de mundo conhecidas, resolve o
    /// DLT só com os 4 cantos, e verifica que o ponto interior — que
    /// NÃO entrou no ajuste — projeta no valor esperado.
    ///
    /// Os outros testes checam resíduo só nos 4 pontos de calibração, o
    /// que é ~0 por construção (sistema exato). Este teste falha se o
    /// DLT recuperar apenas uma aproximação afim, porque o ponto
    /// interior sofre encurtamento perspectivo que só a homografia
    /// completa reproduz.
    #[test]
    fn dlt_recovers_perspective_at_unseen_interior_point() {
        use nalgebra::Matrix3;

        // Homografia verdadeira imagem→mundo, com perspectiva real.
        let h_true = Homography::from_matrix(Matrix3::new(
            0.0200, 0.0010, 0.50, 0.0015, 0.0180, 0.30, 0.0001, 0.0002, 1.0,
        ));

        // 4 cantos (usados na calibração) + 1 ponto interior (reservado).
        let corners_img = [
            (100.0, 100.0),
            (500.0, 120.0),
            (480.0, 400.0),
            (90.0, 420.0),
        ];
        let interior_img = (300.0, 260.0);

        // Coordenadas de mundo VERDADEIRAS via H_true.
        let corners_world = [
            h_true.project(corners_img[0]).unwrap(),
            h_true.project(corners_img[1]).unwrap(),
            h_true.project(corners_img[2]).unwrap(),
            h_true.project(corners_img[3]).unwrap(),
        ];
        let interior_world_true = h_true.project(interior_img).unwrap();

        // Resolve o DLT usando SÓ os 4 cantos.
        let h_est = solve_homography_dlt(&corners_img, &corners_world).unwrap();

        // Sanidade: resíduo ~0 nos cantos (exato por construção).
        for i in 0..4 {
            let r = h_est
                .reprojection_residual(corners_img[i], corners_world[i])
                .unwrap();
            assert!(r < 1e-9, "resíduo no canto {i} = {r}");
        }

        // CHAVE: o ponto interior (não usado no ajuste) projeta no
        // valor verdadeiro — só possível se a perspectiva foi recuperada.
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
