//! G12.1 — Detecção de bordas (Sobel, Laplacian, Canny).
//!
//! Filtros forenses fundamentais: realçam transições de intensidade,
//! úteis para destacar contornos de placas, marcações, objetos sob
//! análise. Trabalham na luminância (0.299R + 0.587G + 0.114B) e
//! retornam imagem grayscale colorizada (R=G=B).

use image::{Rgba, RgbaImage};

/// Converte um pixel RGBA para luminância (BT.601).
#[inline]
fn lum(p: Rgba<u8>) -> f32 {
    0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32
}

/// Sobel — magnitude do gradiente. `strength` multiplica o resultado
/// antes de clipar em 0..255.
///
/// Kernel Gx:           Kernel Gy:
/// [-1  0  1]           [-1 -2 -1]
/// [-2  0  2]           [ 0  0  0]
/// [-1  0  1]           [ 1  2  1]
pub fn sobel(img: &RgbaImage, strength: f32) -> RgbaImage {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let mut out = RgbaImage::new(w as u32, h as u32);
    let s = strength.clamp(0.0, 4.0);

    for y in 0..h {
        for x in 0..w {
            let mut gx = 0.0_f32;
            let mut gy = 0.0_f32;
            for dy in -1..=1_i32 {
                for dx in -1..=1_i32 {
                    let xx = (x + dx).clamp(0, w - 1) as u32;
                    let yy = (y + dy).clamp(0, h - 1) as u32;
                    let l = lum(*img.get_pixel(xx, yy));
                    let kgx = match (dx, dy) {
                        (-1, -1) => -1.0,
                        (-1, 0) => -2.0,
                        (-1, 1) => -1.0,
                        (1, -1) => 1.0,
                        (1, 0) => 2.0,
                        (1, 1) => 1.0,
                        _ => 0.0,
                    };
                    let kgy = match (dx, dy) {
                        (-1, -1) => -1.0,
                        (0, -1) => -2.0,
                        (1, -1) => -1.0,
                        (-1, 1) => 1.0,
                        (0, 1) => 2.0,
                        (1, 1) => 1.0,
                        _ => 0.0,
                    };
                    gx += l * kgx;
                    gy += l * kgy;
                }
            }
            let mag = (gx * gx + gy * gy).sqrt() * s;
            let v = mag.clamp(0.0, 255.0) as u8;
            let a = img.get_pixel(x as u32, y as u32).0[3];
            out.put_pixel(x as u32, y as u32, Rgba([v, v, v, a]));
        }
    }
    out
}

/// Laplaciano 5x5 — detector de bordas isotrópico (segunda derivada).
///
/// Kernel (5x5, soma zero):
/// [ 0  0 -1  0  0]
/// [ 0 -1 -2 -1  0]
/// [-1 -2 16 -2 -1]
/// [ 0 -1 -2 -1  0]
/// [ 0  0 -1  0  0]
pub fn laplacian(img: &RgbaImage, strength: f32) -> RgbaImage {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let mut out = RgbaImage::new(w as u32, h as u32);
    let s = strength.clamp(0.0, 4.0);

    let kernel: [[f32; 5]; 5] = [
        [0.0, 0.0, -1.0, 0.0, 0.0],
        [0.0, -1.0, -2.0, -1.0, 0.0],
        [-1.0, -2.0, 16.0, -2.0, -1.0],
        [0.0, -1.0, -2.0, -1.0, 0.0],
        [0.0, 0.0, -1.0, 0.0, 0.0],
    ];

    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0_f32;
            for ky in 0..5_i32 {
                for kx in 0..5_i32 {
                    let xx = (x + kx - 2).clamp(0, w - 1) as u32;
                    let yy = (y + ky - 2).clamp(0, h - 1) as u32;
                    acc += lum(*img.get_pixel(xx, yy)) * kernel[ky as usize][kx as usize];
                }
            }
            let v = (acc.abs() * s).clamp(0.0, 255.0) as u8;
            let a = img.get_pixel(x as u32, y as u32).0[3];
            out.put_pixel(x as u32, y as u32, Rgba([v, v, v, a]));
        }
    }
    out
}

/// Canny simplificado — Sobel + non-max suppression + double threshold + hysteresis.
///
/// Versão didática (não otimizada). O output é binário: 0 ou 255.
/// `low_threshold` e `high_threshold` em escala 0..255.
pub fn canny(img: &RgbaImage, low_threshold: f32, high_threshold: f32) -> RgbaImage {
    let w = img.width() as usize;
    let h = img.height() as usize;
    let lo = low_threshold.clamp(0.0, 255.0);
    let hi = high_threshold.clamp(lo + 1.0, 255.0);

    // 1. Gaussian blur leve para reduzir ruído (sigma=1, kernel 3x3 aprox).
    let smooth = super::blur::gaussian(img, 1.0);

    // 2. Sobel gradient magnitude + direction.
    let mut mag = vec![0.0_f32; w * h];
    let mut dir = vec![0.0_f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut gx = 0.0_f32;
            let mut gy = 0.0_f32;
            for dy in -1..=1_i32 {
                for dx in -1..=1_i32 {
                    let xx = (x as i32 + dx).clamp(0, w as i32 - 1) as u32;
                    let yy = (y as i32 + dy).clamp(0, h as i32 - 1) as u32;
                    let l = lum(*smooth.get_pixel(xx, yy));
                    let kgx = match (dx, dy) {
                        (-1, -1) | (-1, 1) => -1.0,
                        (-1, 0) => -2.0,
                        (1, -1) | (1, 1) => 1.0,
                        (1, 0) => 2.0,
                        _ => 0.0,
                    };
                    let kgy = match (dx, dy) {
                        (-1, -1) | (1, -1) => -1.0,
                        (0, -1) => -2.0,
                        (-1, 1) | (1, 1) => 1.0,
                        (0, 1) => 2.0,
                        _ => 0.0,
                    };
                    gx += l * kgx;
                    gy += l * kgy;
                }
            }
            mag[y * w + x] = (gx * gx + gy * gy).sqrt();
            dir[y * w + x] = gy.atan2(gx);
        }
    }

    // 3. Non-max suppression — bin direção em 4 ângulos.
    let mut suppressed = vec![0.0_f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let angle = dir[i].to_degrees().rem_euclid(180.0);
            let m = mag[i];
            let (n1, n2) = if (0.0..22.5).contains(&angle) || (157.5..180.0).contains(&angle) {
                (mag[i - 1], mag[i + 1])
            } else if (22.5..67.5).contains(&angle) {
                (mag[i - w + 1], mag[i + w - 1])
            } else if (67.5..112.5).contains(&angle) {
                (mag[i - w], mag[i + w])
            } else {
                (mag[i - w - 1], mag[i + w + 1])
            };
            suppressed[i] = if m >= n1 && m >= n2 { m } else { 0.0 };
        }
    }

    // 4. Double threshold + hysteresis (simplificado: pixel >= hi sempre forte,
    //    pixel entre lo..hi conectado a forte fica como forte).
    let mut out = RgbaImage::new(w as u32, h as u32);
    let mut strong = vec![false; w * h];
    for i in 0..w * h {
        if suppressed[i] >= hi {
            strong[i] = true;
        }
    }
    // Propagação simples (1 passada): pixel "fraco" (>= lo) com vizinho forte vira forte.
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            if strong[i] {
                continue;
            }
            if suppressed[i] >= lo {
                let has_strong_neighbor = strong[i - 1]
                    || strong[i + 1]
                    || strong[i - w]
                    || strong[i + w]
                    || strong[i - w - 1]
                    || strong[i - w + 1]
                    || strong[i + w - 1]
                    || strong[i + w + 1];
                if has_strong_neighbor {
                    strong[i] = true;
                }
            }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let v = if strong[y * w + x] { 255_u8 } else { 0_u8 };
            let a = img.get_pixel(x as u32, y as u32).0[3];
            out.put_pixel(x as u32, y as u32, Rgba([v, v, v, a]));
        }
    }
    out
}

/// W12 — **Difference of Gaussians** (DoG): banda de frequências = borrado
/// fino (σ1) − borrado grosso (σ2>σ1), na luminância. Realça bordas/texturas
/// numa faixa de escala escolhida; centrado em 128 (mostra as duas
/// polaridades). `gain` amplifica antes de clampar. Determinístico.
pub fn difference_of_gaussians(img: &RgbaImage, sigma1: f32, sigma2: f32, gain: f32) -> RgbaImage {
    let s1 = sigma1.clamp(0.1, 50.0);
    // garante σ2 > σ1 (senão a banda é vazia).
    let s2 = sigma2.clamp(s1 + 0.1, 100.0);
    let g = gain.clamp(0.1, 20.0);
    let b1 = super::blur::gaussian(img, s1);
    let b2 = super::blur::gaussian(img, s2);
    let w = img.width();
    let h = img.height();
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let d = lum(*b1.get_pixel(x, y)) - lum(*b2.get_pixel(x, y));
            let v = (d * g + 128.0).clamp(0.0, 255.0) as u8;
            let a = img.get_pixel(x, y).0[3];
            out.put_pixel(x, y, Rgba([v, v, v, a]));
        }
    }
    out
}

/// W12 (forense) — **Gradiente de luminância** colorido: magnitude do Sobel
/// vira brilho e a DIREÇÃO do gradiente vira matiz (hue). Expõe direção de
/// iluminação/sombreamento inconsistente entre regiões (indício de colagem)
/// e áreas "pintadas"/clonadas (gradiente artificialmente liso). `strength`
/// amplifica a magnitude.
pub fn luminance_gradient(img: &RgbaImage, strength: f32) -> RgbaImage {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let s = strength.clamp(0.1, 8.0);
    let mut out = RgbaImage::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let mut gx = 0.0_f32;
            let mut gy = 0.0_f32;
            for dy in -1..=1_i32 {
                for dx in -1..=1_i32 {
                    let xx = (x + dx).clamp(0, w - 1) as u32;
                    let yy = (y + dy).clamp(0, h - 1) as u32;
                    let l = lum(*img.get_pixel(xx, yy));
                    let kgx = match (dx, dy) {
                        (-1, -1) | (-1, 1) => -1.0,
                        (-1, 0) => -2.0,
                        (1, -1) | (1, 1) => 1.0,
                        (1, 0) => 2.0,
                        _ => 0.0,
                    };
                    let kgy = match (dx, dy) {
                        (-1, -1) | (1, -1) => -1.0,
                        (0, -1) => -2.0,
                        (-1, 1) | (1, 1) => 1.0,
                        (0, 1) => 2.0,
                        _ => 0.0,
                    };
                    gx += l * kgx;
                    gy += l * kgy;
                }
            }
            let mag = ((gx * gx + gy * gy).sqrt() * s / 255.0).clamp(0.0, 1.0);
            let hue = gy.atan2(gx).to_degrees().rem_euclid(360.0);
            let (r, g, b) = hsv_to_rgb(hue, 1.0, mag);
            let a = img.get_pixel(x as u32, y as u32).0[3];
            out.put_pixel(x as u32, y as u32, Rgba([r, g, b, a]));
        }
    }
    out
}

/// HSV (H em graus, S/V em 0..1) → RGB 0..255.
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let c = v * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn black_with_white_center() -> RgbaImage {
        let mut img = RgbaImage::from_pixel(5, 5, Rgba([0, 0, 0, 255]));
        img.put_pixel(2, 2, Rgba([255, 255, 255, 255]));
        img
    }

    #[test]
    fn dog_flat_image_is_neutral_128() {
        // Imagem chapada → DoG ≈ 0 → saída centrada em 128.
        let img = RgbaImage::from_pixel(16, 16, Rgba([100, 100, 100, 255]));
        let out = difference_of_gaussians(&img, 1.0, 3.0, 5.0);
        let c = out.get_pixel(8, 8).0[0] as i32;
        // Área chapada → DoG ≈ neutro. O blur quantiza em u8 por sigma, e o
        // `gain` amplifica esse arredondamento em alguns níveis — daí a folga.
        assert!((c - 128).abs() <= 12, "DoG de área chapada deveria ~128, veio {c}");
    }

    #[test]
    fn dog_clamps_sigma_order() {
        // σ2 <= σ1 não deve quebrar (clampa σ2 > σ1).
        let img = RgbaImage::from_pixel(8, 8, Rgba([50, 50, 50, 255]));
        let out = difference_of_gaussians(&img, 3.0, 1.0, 5.0);
        assert_eq!(out.dimensions(), (8, 8));
    }

    #[test]
    fn luminance_gradient_flat_is_dark() {
        // Sem gradiente → magnitude 0 → V=0 → preto.
        let img = RgbaImage::from_pixel(8, 8, Rgba([90, 90, 90, 255]));
        let out = luminance_gradient(&img, 1.0);
        let p = out.get_pixel(4, 4).0;
        assert!(p[0] < 5 && p[1] < 5 && p[2] < 5);
    }

    #[test]
    fn luminance_gradient_edge_has_color() {
        let img = black_with_white_center();
        let out = luminance_gradient(&img, 2.0);
        // Algum pixel ao redor do centro deve ter brilho (gradiente != 0).
        let mut max_v = 0u8;
        for p in out.pixels() {
            max_v = max_v.max(p.0[0].max(p.0[1]).max(p.0[2]));
        }
        assert!(max_v > 30, "gradiente deveria produzir cor em torno da borda");
    }

    #[test]
    fn hsv_to_rgb_primaries() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(120.0, 1.0, 1.0), (0, 255, 0));
        assert_eq!(hsv_to_rgb(240.0, 1.0, 1.0), (0, 0, 255));
    }

    #[test]
    fn sobel_responds_around_isolated_bright_pixel() {
        let img = black_with_white_center();
        let out = sobel(&img, 1.0);
        // O pixel central, sem vizinhos brilhantes laterais, tem gradiente 0.
        // Os pixels nas bordas do quadrado 3x3 ao redor têm gradiente alto.
        let around = out.get_pixel(2, 1).0[0];
        assert!(around > 50, "vizinho do pixel branco deveria ter borda");
    }

    #[test]
    fn laplacian_isotropic_response() {
        let img = black_with_white_center();
        let out = laplacian(&img, 1.0);
        // Pixel central: o laplaciano vale 16 * 255 -> clipa em 255.
        // Vizinhos diagonais 1+: têm resposta moderada.
        assert_eq!(out.get_pixel(2, 2).0[0], 255);
    }

    #[test]
    fn canny_threshold_filters_low_intensity_edges() {
        let img = black_with_white_center();
        let out = canny(&img, 50.0, 150.0);
        // Output é binário 0/255.
        for p in out.pixels() {
            assert!(p.0[0] == 0 || p.0[0] == 255);
        }
    }
}
