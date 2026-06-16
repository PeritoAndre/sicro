//! G12.6 — Correção de perspectiva (homografia 4-point).
//!
//! Resolve um sistema 8x8 para encontrar a matriz 3x3 que mapeia
//! `src` (4 pontos no plano observado, ex.: 4 cantos de um documento
//! fotografado de viés) em `dst` (4 cantos do retângulo de saída).
//! Depois aplica warp inverso bilinear: para cada pixel do output,
//! encontra a posição correspondente no input e interpola.

use image::{Rgba, RgbaImage};

/// Resolve sistema linear via eliminação gaussiana com pivoteamento parcial.
///
/// `a` é matriz NxN, `b` vetor N. Modifica in-place; retorna `Some(x)` se
/// resolvido ou `None` se singular.
fn solve_linear_system(a: &mut [[f32; 8]; 8], b: &mut [f32; 8]) -> Option<[f32; 8]> {
    const N: usize = 8;
    for i in 0..N {
        // Pivot.
        let mut max_row = i;
        for r in (i + 1)..N {
            if a[r][i].abs() > a[max_row][i].abs() {
                max_row = r;
            }
        }
        if a[max_row][i].abs() < 1e-9 {
            return None;
        }
        a.swap(i, max_row);
        b.swap(i, max_row);

        // Eliminate.
        for r in (i + 1)..N {
            let factor = a[r][i] / a[i][i];
            for c in i..N {
                a[r][c] -= factor * a[i][c];
            }
            b[r] -= factor * b[i];
        }
    }

    // Back-substitute.
    let mut x = [0.0_f32; 8];
    for i in (0..N).rev() {
        let mut sum = b[i];
        for c in (i + 1)..N {
            sum -= a[i][c] * x[c];
        }
        x[i] = sum / a[i][i];
    }
    Some(x)
}

/// Computa a homografia 3x3 (com h33=1) que mapeia src → dst.
///
/// Cada par (x_s, y_s) → (x_d, y_d) gera 2 equações:
///   x_d = (h11*x_s + h12*y_s + h13) / (h31*x_s + h32*y_s + 1)
///   y_d = (h21*x_s + h22*y_s + h23) / (h31*x_s + h32*y_s + 1)
///
/// Reescrevendo:
///   h11*x_s + h12*y_s + h13 - h31*x_s*x_d - h32*y_s*x_d = x_d
///   h21*x_s + h22*y_s + h23 - h31*x_s*y_d - h32*y_s*y_d = y_d
///
/// 4 pontos → 8 equações → resolve linha [h11..h32].
fn compute_homography(src: &[[f32; 2]; 4], dst: &[[f32; 2]; 4]) -> Option<[f32; 9]> {
    let mut a = [[0.0_f32; 8]; 8];
    let mut b = [0.0_f32; 8];
    for i in 0..4 {
        let (xs, ys) = (src[i][0], src[i][1]);
        let (xd, yd) = (dst[i][0], dst[i][1]);
        let row1 = 2 * i;
        let row2 = 2 * i + 1;
        a[row1] = [xs, ys, 1.0, 0.0, 0.0, 0.0, -xs * xd, -ys * xd];
        b[row1] = xd;
        a[row2] = [0.0, 0.0, 0.0, xs, ys, 1.0, -xs * yd, -ys * yd];
        b[row2] = yd;
    }
    let x = solve_linear_system(&mut a, &mut b)?;
    Some([x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7], 1.0])
}

/// Inverte uma matriz 3x3 (homografia). Retorna `None` se singular.
fn invert_3x3(m: &[f32; 9]) -> Option<[f32; 9]> {
    let det = m[0] * (m[4] * m[8] - m[5] * m[7])
        - m[1] * (m[3] * m[8] - m[5] * m[6])
        + m[2] * (m[3] * m[7] - m[4] * m[6]);
    if det.abs() < 1e-10 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        (m[4] * m[8] - m[5] * m[7]) * inv_det,
        (m[2] * m[7] - m[1] * m[8]) * inv_det,
        (m[1] * m[5] - m[2] * m[4]) * inv_det,
        (m[5] * m[6] - m[3] * m[8]) * inv_det,
        (m[0] * m[8] - m[2] * m[6]) * inv_det,
        (m[2] * m[3] - m[0] * m[5]) * inv_det,
        (m[3] * m[7] - m[4] * m[6]) * inv_det,
        (m[1] * m[6] - m[0] * m[7]) * inv_det,
        (m[0] * m[4] - m[1] * m[3]) * inv_det,
    ])
}

#[inline]
fn apply_homography(m: &[f32; 9], x: f32, y: f32) -> (f32, f32) {
    let denom = m[6] * x + m[7] * y + m[8];
    let xp = (m[0] * x + m[1] * y + m[2]) / denom;
    let yp = (m[3] * x + m[4] * y + m[5]) / denom;
    (xp, yp)
}

/// Sample bilinear at (fx, fy) in source image. Out-of-bounds → transparente.
fn sample_bilinear(img: &RgbaImage, fx: f32, fy: f32) -> Rgba<u8> {
    let w = img.width() as f32;
    let h = img.height() as f32;
    if fx < 0.0 || fy < 0.0 || fx >= w - 1.0 || fy >= h - 1.0 {
        return Rgba([0, 0, 0, 0]);
    }
    let x0 = fx.floor() as u32;
    let y0 = fy.floor() as u32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let dx = fx - x0 as f32;
    let dy = fy - y0 as f32;
    let p00 = img.get_pixel(x0, y0).0;
    let p10 = img.get_pixel(x1, y0).0;
    let p01 = img.get_pixel(x0, y1).0;
    let p11 = img.get_pixel(x1, y1).0;
    let mut out = [0u8; 4];
    for c in 0..4 {
        let v = (1.0 - dx) * (1.0 - dy) * p00[c] as f32
            + dx * (1.0 - dy) * p10[c] as f32
            + (1.0 - dx) * dy * p01[c] as f32
            + dx * dy * p11[c] as f32;
        out[c] = v.clamp(0.0, 255.0) as u8;
    }
    Rgba(out)
}

/// Aplica correção de perspectiva: pixels de `src` (input) mapeados para
/// rectângulo `[0, output_width] x [0, output_height]` via `src`→`dst`.
///
/// `dst` é tipicamente o retângulo `[(0,0),(W,0),(W,H),(0,H)]` em ordem
/// horária. `src` são os 4 cantos correspondentes no input.
pub fn perspective_correct(
    img: &RgbaImage,
    src: &[[f32; 2]; 4],
    dst: &[[f32; 2]; 4],
    output_width: u32,
    output_height: u32,
) -> RgbaImage {
    let w = output_width.max(1);
    let h = output_height.max(1);
    let mut out = RgbaImage::new(w, h);

    let h_fwd = match compute_homography(src, dst) {
        Some(m) => m,
        None => return out, // degenerate input — output transparente.
    };
    let h_inv = match invert_3x3(&h_fwd) {
        Some(m) => m,
        None => return out,
    };

    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = apply_homography(&h_inv, x as f32, y as f32);
            out.put_pixel(x, y, sample_bilinear(img, sx, sy));
        }
    }
    out
}

/// W12 — **Rotação por ângulo arbitrário** (não só 90°), com amostragem
/// bilinear. `degrees` horário. Se `expand`, a tela cresce para caber a
/// imagem inteira (cantos novos = transparentes); senão mantém o tamanho e
/// recorta. Determinístico; preserva os pixels (sem inventar — bordas vazias
/// ficam transparentes).
pub fn rotate_arbitrary(img: &RgbaImage, degrees: f32, expand: bool) -> RgbaImage {
    let w = img.width() as f32;
    let h = img.height() as f32;
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();

    let (out_w, out_h) = if expand {
        // `round` (não `ceil`): cos(90°) em f32 ≈ 6e-8, então `w*cos` daria
        // 4.0000006 e `ceil` cresceria a tela 1px à toa. round descarta o épsilon.
        let nw = (w * cos.abs() + h * sin.abs()).round().max(1.0);
        let nh = (w * sin.abs() + h * cos.abs()).round().max(1.0);
        (nw, nh)
    } else {
        (w, h)
    };
    let mut out = RgbaImage::new(out_w as u32, out_h as u32);

    let (cx, cy) = (w / 2.0, h / 2.0);
    let (ocx, ocy) = (out_w / 2.0, out_h / 2.0);

    for oy in 0..out.height() {
        for ox in 0..out.width() {
            let dx = ox as f32 + 0.5 - ocx;
            let dy = oy as f32 + 0.5 - ocy;
            // Rotação inversa (-θ) para achar a fonte.
            let sx = dx * cos + dy * sin + cx;
            let sy = -dx * sin + dy * cos + cy;
            out.put_pixel(ox, oy, sample_bilinear(img, sx - 0.5, sy - 0.5));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_arbitrary_0_is_near_identity() {
        let mut img = RgbaImage::new(8, 8);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgba([(x * 30) as u8, (y * 30) as u8, 90, 255]);
        }
        let out = rotate_arbitrary(&img, 0.0, false);
        assert_eq!(out.dimensions(), (8, 8));
        // 0° → praticamente igual no centro.
        assert_eq!(out.get_pixel(4, 4).0[3], 255);
    }

    #[test]
    fn rotate_arbitrary_90_expand_swaps_dims() {
        let img = RgbaImage::from_pixel(10, 4, Rgba([200, 100, 50, 255]));
        let out = rotate_arbitrary(&img, 90.0, true);
        // 90° com expand → dimensões trocam (≈4x10).
        assert_eq!((out.width(), out.height()), (4, 10));
    }

    #[test]
    fn rotate_arbitrary_45_expand_grows_canvas() {
        let img = RgbaImage::from_pixel(10, 10, Rgba([100, 100, 100, 255]));
        let out = rotate_arbitrary(&img, 45.0, true);
        // diagonal ≈ 14.1 → tela ~15x15.
        assert!(out.width() >= 14 && out.height() >= 14);
        // Cantos viram transparentes.
        assert_eq!(out.get_pixel(0, 0).0[3], 0);
    }

    #[test]
    fn homography_identity_maps_unit_square_to_itself() {
        let src = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let dst = src;
        let m = compute_homography(&src, &dst).unwrap();
        // h11=1, h22=1, h33=1, resto ~0.
        assert!((m[0] - 1.0).abs() < 1e-4);
        assert!((m[4] - 1.0).abs() < 1e-4);
    }

    #[test]
    fn homography_translation_works() {
        let src = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let dst = [[5.0, 5.0], [15.0, 5.0], [15.0, 15.0], [5.0, 15.0]];
        let m = compute_homography(&src, &dst).unwrap();
        let (tx, ty) = apply_homography(&m, 0.0, 0.0);
        assert!((tx - 5.0).abs() < 1e-3);
        assert!((ty - 5.0).abs() < 1e-3);
    }

    #[test]
    fn perspective_identity_preserves_pixels() {
        // Imagem 4x4 colorida.
        let mut img = RgbaImage::new(4, 4);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgba([(x * 60) as u8, (y * 60) as u8, 100, 255]);
        }
        let src = [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]];
        let dst = src;
        let out = perspective_correct(&img, &src, &dst, 4, 4);
        assert_eq!(out.dimensions(), (4, 4));
        // Pixel central deve ser próximo do original.
        let center = out.get_pixel(2, 2).0;
        assert!(center[0] > 0);
    }

    #[test]
    fn perspective_degenerate_input_returns_transparent() {
        // 4 pontos colineares — homografia indefinida.
        let src = [[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        let dst = [[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0]];
        let img = RgbaImage::from_pixel(4, 4, Rgba([100, 100, 100, 255]));
        let out = perspective_correct(&img, &src, &dst, 4, 4);
        // Esperamos transparência total.
        assert_eq!(out.get_pixel(0, 0).0[3], 0);
    }
}
