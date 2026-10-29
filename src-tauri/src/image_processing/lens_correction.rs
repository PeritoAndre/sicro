//! Correção de distorção radial (Brown-Conrady, só k1/k2/k3) da foto de drone
//! usada como fundo de croqui. Com `(u, v)` normalizados pelo centro e `r² = u² + v²`:
//!   `src = p · (1 + k1·r² + k2·r⁴ + k3·r⁶)`
//! k negativo = barril (drone), positivo = almofada. Backward warp bilinear;
//! fora da imagem = preto transparente (o perito recorta os cantos depois).

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

/// Só termos radiais, sem tangenciais.
#[derive(Debug, Clone, Copy)]
pub struct LensCoefficients {
    pub k1: f32,
    pub k2: f32,
    pub k3: f32,
}

impl LensCoefficients {
    pub const ZERO: Self = Self {
        k1: 0.0,
        k2: 0.0,
        k3: 0.0,
    };

    pub fn is_identity(&self) -> bool {
        self.k1.abs() < 1e-6 && self.k2.abs() < 1e-6 && self.k3.abs() < 1e-6
    }
}

/// Slider 0..1 → `k1 = −0.30·t`, `k2 = 0.08·t`, `k3 = 0` (barril típico de drone).
pub fn coefficients_for_intensity(intensity: f32) -> LensCoefficients {
    let t = intensity.clamp(0.0, 1.0);
    LensCoefficients {
        k1: -0.30 * t,
        k2: 0.08 * t,
        k3: 0.0,
    }
}

/// Recorte em pixels sobre a imagem já corrigida.
#[derive(Debug, Clone, Copy)]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl CropRect {
    /// Limita ao tamanho da imagem; `None` se não sobra área.
    pub fn clamped(self, img_w: u32, img_h: u32) -> Option<Self> {
        if img_w == 0 || img_h == 0 {
            return None;
        }
        let x = self.x.min(img_w.saturating_sub(1));
        let y = self.y.min(img_h.saturating_sub(1));
        let max_w = img_w.saturating_sub(x);
        let max_h = img_h.saturating_sub(y);
        let width = self.width.min(max_w);
        let height = self.height.min(max_h);
        if width == 0 || height == 0 {
            return None;
        }
        Some(Self {
            x,
            y,
            width,
            height,
        })
    }
}

/// Backward warp com amostragem bilinear; saída RGBA do mesmo tamanho (os
/// cantos ficam para o perito recortar).
pub fn apply_radial_correction(
    img: &DynamicImage,
    coeffs: LensCoefficients,
) -> RgbaImage {
    apply_radial_correction_centered(img, coeffs, 0.5, 0.5)
}

/// Raio de normalização = meio lado maior (mesmo fator nos cantos em retrato e paisagem).
pub fn norma(w: u32, h: u32) -> f32 {
    (w as f32 / 2.0).max(h as f32 / 2.0)
}

/// Ponto de origem (no quadro de entrada) do pixel `(x, y)` da saída, com o centro óptico
/// em `(cx, cy)` (fração da largura/altura; 0,5 = meio).
pub fn origem(c: LensCoefficients, w: u32, h: u32, cx: f32, cy: f32, x: f32, y: f32) -> (f32, f32) {
    let (mx, my) = (w as f32 * cx, h as f32 * cy);
    let n = norma(w, h);
    let (u, v) = ((x - mx) / n, (y - my) / n);
    let r2 = u * u + v * v;
    let f = 1.0 + c.k1 * r2 + c.k2 * r2 * r2 + c.k3 * r2 * r2 * r2;
    (u * f * n + mx, v * f * n + my)
}

/// Igual a `apply_radial_correction`, com o centro óptico escolhido.
pub fn apply_radial_correction_centered(
    img: &DynamicImage,
    coeffs: LensCoefficients,
    cx: f32,
    cy: f32,
) -> RgbaImage {
    let (w, h) = img.dimensions();
    if coeffs.is_identity() {
        return img.to_rgba8();
    }
    let src = img.to_rgba8();
    let mut out = RgbaImage::new(w, h);

    for yd in 0..h {
        for xd in 0..w {
            let (xs, ys) = origem(coeffs, w, h, cx, cy, xd as f32, yd as f32);
            let pixel = sample_bilinear(&src, xs, ys);
            out.put_pixel(xd, yd, pixel);
        }
    }

    out
}

/// Quadro RGB inteiro corrigido (bordas replicadas, sem transparência): é o que alimenta o
/// realce e a exportação de quadros.
pub fn corrigir_rgb(rgb: &[u8], w: u32, h: u32, c: LensCoefficients, cx: f32, cy: f32) -> Vec<u8> {
    if c.is_identity() {
        return rgb.to_vec();
    }
    let (wu, hu) = (w as usize, h as usize);
    let mut out = vec![0u8; rgb.len()];
    for y in 0..hu {
        for x in 0..wu {
            let (xs, ys) = origem(c, w, h, cx, cy, x as f32, y as f32);
            let xs = xs.clamp(0.0, (wu - 1) as f32);
            let ys = ys.clamp(0.0, (hu - 1) as f32);
            let (x0, y0) = (xs.floor() as usize, ys.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(wu - 1), (y0 + 1).min(hu - 1));
            let (fx, fy) = (xs - x0 as f32, ys - y0 as f32);
            for ch in 0..3 {
                let g = |xx: usize, yy: usize| rgb[(yy * wu + xx) * 3 + ch] as f32;
                let a = g(x0, y0) * (1.0 - fx) + g(x1, y0) * fx;
                let b = g(x0, y1) * (1.0 - fx) + g(x1, y1) * fx;
                out[(y * wu + x) * 3 + ch] = (a * (1.0 - fy) + b * fy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Fora da imagem = preto transparente.
fn sample_bilinear(src: &RgbaImage, x: f32, y: f32) -> Rgba<u8> {
    let (w, h) = src.dimensions();
    if x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32 {
        return Rgba([0, 0, 0, 0]);
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let dx = x - x0 as f32;
    let dy = y - y0 as f32;

    let p00 = src.get_pixel(x0, y0);
    let p10 = src.get_pixel(x1, y0);
    let p01 = src.get_pixel(x0, y1);
    let p11 = src.get_pixel(x1, y1);

    let lerp = |a: u8, b: u8, t: f32| -> u8 {
        (a as f32 * (1.0 - t) + b as f32 * t).round().clamp(0.0, 255.0) as u8
    };
    let mut out = [0u8; 4];
    for c in 0..4 {
        let top = lerp(p00.0[c], p10.0[c], dx);
        let bot = lerp(p01.0[c], p11.0[c], dx);
        out[c] = lerp(top, bot, dy);
    }
    Rgba(out)
}

/// Recorta (rect limitado à imagem); `None` se o recorte fica vazio.
pub fn crop(img: RgbaImage, rect: CropRect) -> Option<RgbaImage> {
    let (w, h) = img.dimensions();
    let bound = rect.clamped(w, h)?;
    let mut out = RgbaImage::new(bound.width, bound.height);
    for y in 0..bound.height {
        for x in 0..bound.width {
            let src = img.get_pixel(bound.x + x, bound.y + y);
            out.put_pixel(x, y, *src);
        }
    }
    Some(out)
}

/// Coeficientes equivalentes para o filtro `lenscorrection` do ffmpeg, que normaliza o raio pela
/// meia-diagonal em vez do meio lado maior. Fora de [-1, 1] o ffmpeg não aceita: `None`.
pub fn para_ffmpeg(c: LensCoefficients, w: u32, h: u32) -> Option<(f64, f64)> {
    let n = norma(w, h) as f64;
    let hd = ((w as f64 / 2.0).powi(2) + (h as f64 / 2.0).powi(2)).sqrt();
    let q = (hd / n).powi(2);
    let (k1, k2) = (c.k1 as f64 * q, c.k2 as f64 * q * q);
    (k1.abs() <= 1.0 && k2.abs() <= 1.0).then_some((k1, k2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// O filtro do ffmpeg com os coeficientes convertidos dá a mesma imagem (erro pequeno).
    #[test]
    fn equivale_ao_lenscorrection_do_ffmpeg() {
        let Some(ff) = crate::tools::find_ffmpeg_tool("ffmpeg") else { return };
        let (w, h) = (640u32, 360u32);
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let grade = (x % 40 < 3) || (y % 40 < 3);
                let v = if grade { 255 } else { ((x * 255 / w) as u8) / 2 + ((y * 255 / h) as u8) / 4 };
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let (src, dst) = (dir.path().join("a.png"), dir.path().join("b.png"));
        img.save(&src).unwrap();
        let c = LensCoefficients { k1: -0.25, k2: 0.05, k3: 0.0 };
        let (k1, k2) = para_ffmpeg(c, w, h).unwrap();
        let st = crate::tools::command(&ff).args(["-v", "error", "-y", "-i"]).arg(&src).args(["-vf", &format!("lenscorrection=k1={a}:k2={b}:i=bilinear")]).arg(&dst).status().unwrap();
            assert!(st.success());
            let theirs = image::open(&dst).unwrap().to_rgb8();
            let ours = apply_radial_correction(&DynamicImage::ImageRgba8(img.clone()), c);
            let (mut s, mut n) = (0.0f64, 0u64);
            for y in 20..h - 20 { for x in 20..w - 20 { let (p, q) = (ours.get_pixel(x, y).0[0] as f64, theirs.get_pixel(x, y).0[0] as f64); s += (p - q).powi(2); n += 1; } }
            println!("{nome}: {:.2}", (s / n as f64).sqrt());
        }
        let st = crate::tools::command(ff)
            .args(["-v", "error", "-y", "-i"])
            .arg(&src)
            .args(["-vf", &format!("lenscorrection=k1={k1}:k2={k2}:i=bilinear")])
            .arg(&dst)
            .status()
            .unwrap();
        assert!(st.success());
        let theirs = image::open(&dst).unwrap().to_rgb8();
        let ours = apply_radial_correction(&DynamicImage::ImageRgba8(img), c);
        let (mut s, mut n) = (0.0f64, 0u64);
        for y in 20..h - 20 {
            for x in 20..w - 20 {
                let (a, b) = (ours.get_pixel(x, y).0[0] as f64, theirs.get_pixel(x, y).0[0] as f64);
                s += (a - b).powi(2);
                n += 1;
            }
        }
        let rms = (s / n as f64).sqrt();
        println!("RMS ffmpeg x SICRO: {rms:.2}");
        // sem conversão o erro fica em ~70 (sem correção) ou ~59 (k cru)
        assert!(rms < 30.0, "rms {rms}");
    }

    #[test]
    fn identity_when_intensity_zero() {
        let coeffs = coefficients_for_intensity(0.0);
        assert!(coeffs.is_identity());
    }

    #[test]
    fn negative_k1_for_positive_intensity() {
        let coeffs = coefficients_for_intensity(0.5);
        assert!(coeffs.k1 < 0.0);
        assert!(coeffs.k2 > 0.0);
        assert_eq!(coeffs.k3, 0.0);
    }

    #[test]
    fn intensity_clamps_above_one() {
        let coeffs = coefficients_for_intensity(2.5);
        let expected = coefficients_for_intensity(1.0);
        assert_eq!(coeffs.k1, expected.k1);
        assert_eq!(coeffs.k2, expected.k2);
    }

    #[test]
    fn intensity_clamps_below_zero() {
        let coeffs = coefficients_for_intensity(-1.0);
        assert!(coeffs.is_identity());
    }

    #[test]
    fn correction_preserves_dimensions() {
        let img = DynamicImage::new_rgba8(64, 48);
        let out = apply_radial_correction(
            &img,
            coefficients_for_intensity(0.5),
        );
        assert_eq!(out.dimensions(), (64, 48));
    }

    #[test]
    fn correction_with_zero_coefficients_returns_input() {
        let mut img = RgbaImage::new(8, 8);
        img.put_pixel(3, 3, Rgba([255, 0, 0, 255]));
        let dyn_img = DynamicImage::ImageRgba8(img.clone());
        let out = apply_radial_correction(&dyn_img, LensCoefficients::ZERO);
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(out.get_pixel(x, y), img.get_pixel(x, y));
            }
        }
    }

    #[test]
    fn crop_returns_correct_dimensions() {
        let img = RgbaImage::new(100, 50);
        let cropped = crop(
            img,
            CropRect {
                x: 10,
                y: 5,
                width: 30,
                height: 20,
            },
        )
        .expect("crop should succeed");
        assert_eq!(cropped.dimensions(), (30, 20));
    }

    #[test]
    fn crop_clamps_oversize_rect() {
        let img = RgbaImage::new(10, 10);
        let cropped = crop(
            img,
            CropRect {
                x: 5,
                y: 5,
                width: 100,
                height: 100,
            },
        )
        .expect("clamped crop should succeed");
        assert_eq!(cropped.dimensions(), (5, 5));
    }

    #[test]
    fn crop_returns_none_when_offscreen() {
        let img = RgbaImage::new(10, 10);
        let cropped = crop(
            img,
            CropRect {
                x: 50,
                y: 50,
                width: 5,
                height: 5,
            },
        );
        // x vira 9 e sobra 1×1; o caso realmente vazio é largura/altura 0.
        let img2 = RgbaImage::new(10, 10);
        let cropped2 = crop(
            img2,
            CropRect {
                x: 50,
                y: 50,
                width: 0,
                height: 0,
            },
        );
        assert!(cropped2.is_none());
        assert!(cropped.is_some());
    }

    #[test]
    fn crop_rect_clamped_corner_case_zero_size() {
        let rect = CropRect {
            x: 5,
            y: 5,
            width: 0,
            height: 10,
        };
        assert!(rect.clamped(20, 20).is_none());
    }
}
