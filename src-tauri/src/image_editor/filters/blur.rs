//! G12.2 — Suavização / Denoise (Gaussian, Median, Bilateral).
//!
//! Família de filtros para limpar ruído antes de detectar bordas,
//! suavizar transições ou preservar bordas.

use image::{Rgba, RgbaImage};

/// Calcula kernel gaussiano 1D normalizado de `radius` raio com `sigma`.
fn gaussian_kernel_1d(sigma: f32) -> Vec<f32> {
    let s = sigma.max(0.1);
    let radius = (3.0 * s).ceil() as i32;
    let size = (2 * radius + 1) as usize;
    let mut kernel = vec![0.0_f32; size];
    let mut sum = 0.0_f32;
    let two_sigma2 = 2.0 * s * s;
    for i in 0..size as i32 {
        let x = (i - radius) as f32;
        let v = (-x * x / two_sigma2).exp();
        kernel[i as usize] = v;
        sum += v;
    }
    for k in kernel.iter_mut() {
        *k /= sum;
    }
    kernel
}

/// Gaussian blur separável (1D horizontal + 1D vertical). Sigma = spread.
pub fn gaussian(img: &RgbaImage, sigma: f32) -> RgbaImage {
    if sigma < 0.05 {
        return img.clone();
    }
    let kernel = gaussian_kernel_1d(sigma);
    let radius = (kernel.len() / 2) as i32;
    let w = img.width() as i32;
    let h = img.height() as i32;

    // Passada horizontal.
    let mut tmp = RgbaImage::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let mut r = 0.0_f32;
            let mut g = 0.0_f32;
            let mut b = 0.0_f32;
            let mut a = 0.0_f32;
            for k in -radius..=radius {
                let xx = (x + k).clamp(0, w - 1) as u32;
                let p = img.get_pixel(xx, y as u32);
                let wgt = kernel[(k + radius) as usize];
                r += p.0[0] as f32 * wgt;
                g += p.0[1] as f32 * wgt;
                b += p.0[2] as f32 * wgt;
                a += p.0[3] as f32 * wgt;
            }
            tmp.put_pixel(
                x as u32,
                y as u32,
                Rgba([r as u8, g as u8, b as u8, a as u8]),
            );
        }
    }
    // Passada vertical.
    let mut out = RgbaImage::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let mut r = 0.0_f32;
            let mut g = 0.0_f32;
            let mut b = 0.0_f32;
            let mut a = 0.0_f32;
            for k in -radius..=radius {
                let yy = (y + k).clamp(0, h - 1) as u32;
                let p = tmp.get_pixel(x as u32, yy);
                let wgt = kernel[(k + radius) as usize];
                r += p.0[0] as f32 * wgt;
                g += p.0[1] as f32 * wgt;
                b += p.0[2] as f32 * wgt;
                a += p.0[3] as f32 * wgt;
            }
            out.put_pixel(
                x as u32,
                y as u32,
                Rgba([r as u8, g as u8, b as u8, a as u8]),
            );
        }
    }
    out
}

/// Median filter — remove "salt & pepper" preservando bordas.
/// `radius` em pixels; janela = 2*radius+1.
pub fn median(img: &RgbaImage, radius: u32) -> RgbaImage {
    let r = radius.max(1) as i32;
    let w = img.width() as i32;
    let h = img.height() as i32;
    let mut out = RgbaImage::new(w as u32, h as u32);
    let cap = ((2 * r + 1) * (2 * r + 1)) as usize;
    let mut buf_r: Vec<u8> = Vec::with_capacity(cap);
    let mut buf_g: Vec<u8> = Vec::with_capacity(cap);
    let mut buf_b: Vec<u8> = Vec::with_capacity(cap);

    for y in 0..h {
        for x in 0..w {
            buf_r.clear();
            buf_g.clear();
            buf_b.clear();
            for dy in -r..=r {
                for dx in -r..=r {
                    let xx = (x + dx).clamp(0, w - 1) as u32;
                    let yy = (y + dy).clamp(0, h - 1) as u32;
                    let p = img.get_pixel(xx, yy);
                    buf_r.push(p.0[0]);
                    buf_g.push(p.0[1]);
                    buf_b.push(p.0[2]);
                }
            }
            buf_r.sort_unstable();
            buf_g.sort_unstable();
            buf_b.sort_unstable();
            let mid = buf_r.len() / 2;
            let a = img.get_pixel(x as u32, y as u32).0[3];
            out.put_pixel(
                x as u32,
                y as u32,
                Rgba([buf_r[mid], buf_g[mid], buf_b[mid], a]),
            );
        }
    }
    out
}

/// Bilateral filter (versão simplificada O(w*h*kernel²)).
/// Suaviza preservando bordas — peso = gaussian_space * gaussian_color.
pub fn bilateral(img: &RgbaImage, sigma_space: f32, sigma_color: f32) -> RgbaImage {
    let ss = sigma_space.max(0.5);
    let sc = sigma_color.max(1.0);
    let radius = (2.0 * ss).ceil() as i32;
    let w = img.width() as i32;
    let h = img.height() as i32;
    let two_ss2 = 2.0 * ss * ss;
    let two_sc2 = 2.0 * sc * sc;

    let mut out = RgbaImage::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let pc = *img.get_pixel(x as u32, y as u32);
            let mut wsum = 0.0_f32;
            let mut r_sum = 0.0_f32;
            let mut g_sum = 0.0_f32;
            let mut b_sum = 0.0_f32;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let xx = (x + dx).clamp(0, w - 1) as u32;
                    let yy = (y + dy).clamp(0, h - 1) as u32;
                    let pp = *img.get_pixel(xx, yy);
                    let dist2 = (dx * dx + dy * dy) as f32;
                    let dr = pp.0[0] as f32 - pc.0[0] as f32;
                    let dg = pp.0[1] as f32 - pc.0[1] as f32;
                    let db = pp.0[2] as f32 - pc.0[2] as f32;
                    let color2 = dr * dr + dg * dg + db * db;
                    let wgt = (-dist2 / two_ss2 - color2 / two_sc2).exp();
                    wsum += wgt;
                    r_sum += pp.0[0] as f32 * wgt;
                    g_sum += pp.0[1] as f32 * wgt;
                    b_sum += pp.0[2] as f32 * wgt;
                }
            }
            out.put_pixel(
                x as u32,
                y as u32,
                Rgba([
                    (r_sum / wsum) as u8,
                    (g_sum / wsum) as u8,
                    (b_sum / wsum) as u8,
                    pc.0[3],
                ]),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaussian_with_zero_sigma_returns_clone() {
        let img = RgbaImage::from_pixel(3, 3, Rgba([100, 100, 100, 255]));
        let out = gaussian(&img, 0.0);
        assert_eq!(out.get_pixel(1, 1).0, [100, 100, 100, 255]);
    }

    #[test]
    fn gaussian_smooths_isolated_spike() {
        let mut img = RgbaImage::from_pixel(5, 5, Rgba([0, 0, 0, 255]));
        img.put_pixel(2, 2, Rgba([255, 255, 255, 255]));
        let out = gaussian(&img, 1.0);
        // Pixel central perde brilho (spread); vizinhos ganham.
        assert!(out.get_pixel(2, 2).0[0] < 255);
        assert!(out.get_pixel(1, 2).0[0] > 0);
    }

    #[test]
    fn median_removes_isolated_outlier() {
        let mut img = RgbaImage::from_pixel(5, 5, Rgba([50, 50, 50, 255]));
        img.put_pixel(2, 2, Rgba([255, 255, 255, 255])); // outlier
        let out = median(&img, 1);
        // Median 3x3 — o outlier some, valor central = mediana (50).
        assert_eq!(out.get_pixel(2, 2).0[0], 50);
    }

    #[test]
    fn bilateral_preserves_edges() {
        let mut img = RgbaImage::new(8, 4);
        for y in 0..4 {
            for x in 0..4 {
                img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
            for x in 4..8 {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
        let out = bilateral(&img, 1.5, 30.0);
        // Borda em x=3..4 deve continuar contrastante (sigma_color baixo
        // impede mistura entre regiões muito diferentes).
        let dark = out.get_pixel(2, 2).0[0];
        let bright = out.get_pixel(5, 2).0[0];
        assert!(bright as i32 - dark as i32 > 200);
    }
}
