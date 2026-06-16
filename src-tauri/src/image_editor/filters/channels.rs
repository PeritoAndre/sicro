//! W12 (GIMP-parity) — Decomposição de CANAIS + falsa-cor.
//!
//! Inspiração: GIMP Cores → Componentes → Decompor / Extrair componente.
//! A ferramenta forense MAIS pedida (segundo a pesquisa do GIMP/GEGL): ver um
//! único canal de um espaço de cor diferente revela o que o RGB esconde —
//! re-coloração, splicing, tinta apagada, marcas latentes, anomalias de croma.
//!
//! Tudo é PROJEÇÃO EXATA dos dados existentes (não fabrica nada — §13). As
//! conversões são feitas em f32 a partir do sRGB codificado (mesma convenção
//! do resto do módulo: `apply_adjustments` opera em sRGB-encoded, não linear).
//! Saída: imagem em tons de cinza (R=G=B=valor do canal), alpha preservado.

use image::{Rgba, RgbaImage};

/// Extrai um canal e devolve uma imagem em tons de cinza.
///
/// Canais: `r` `g` `b` · `luminance`(Rec.709) `luma`(Rec.601) ·
/// `h` `s` `v` (HSV) · `y` `cb` `cr` (YCbCr Rec.601 full-range) ·
/// `l_lab` `a_lab` `b_lab` (CIELAB D65). Desconhecido → luma.
pub fn extract_channel(img: &RgbaImage, channel: &str) -> RgbaImage {
    let mut out = img.clone();
    for px in out.pixels_mut() {
        let Rgba([r, g, b, a]) = *px;
        let v = channel_value(r, g, b, channel);
        *px = Rgba([v, v, v, a]);
    }
    out
}

/// Valor 0..255 de um canal para um pixel sRGB. Exposto para teste.
pub fn channel_value(r: u8, g: u8, b: u8, channel: &str) -> u8 {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let v = match channel {
        "r" | "red" => rf,
        "g" | "green" => gf,
        "b" | "blue" => bf,
        // Luminância linear-light Rec.709 (sobre sRGB-encoded — aproximação
        // documentada: usamos os pesos 709 direto sobre o valor codificado).
        "luminance" => 0.2126 * rf + 0.7152 * gf + 0.0722 * bf,
        // Luma perceptual Rec.601 (sobre gamma).
        "luma" => 0.299 * rf + 0.587 * gf + 0.114 * bf,
        "h" | "s" | "v" => {
            let (h, s, vv) = rgb_to_hsv(rf, gf, bf);
            match channel {
                "h" => h / 360.0,
                "s" => s,
                _ => vv,
            }
        }
        "y" | "cb" | "cr" => {
            let (y, cb, cr) = rgb_to_ycbcr(rf, gf, bf);
            match channel {
                "y" => y,
                "cb" => cb,
                _ => cr,
            }
        }
        "l_lab" | "a_lab" | "b_lab" => {
            let (l, aa, bb) = rgb_to_lab(rf, gf, bf);
            match channel {
                // L* 0..100 → 0..255
                "l_lab" => l / 100.0,
                // a*/b* ~ -128..127 → +128 e /255
                "a_lab" => (aa + 128.0) / 255.0,
                _ => (bb + 128.0) / 255.0,
            }
        }
        _ => 0.299 * rf + 0.587 * gf + 0.114 * bf,
    };
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// **Falsa-cor**: mapeia a luminância (Rec.601) de cada pixel por um colormap,
/// tornando perceptíveis diferenças tonais sutis. Reversível e auditável (o
/// mapa é fixo e registrado). Colormaps: `viridis` `jet` `ironbow` `grayscale`.
pub fn false_color(img: &RgbaImage, colormap: &str) -> RgbaImage {
    let lut = colormap_lut(colormap);
    let mut out = img.clone();
    for px in out.pixels_mut() {
        let Rgba([r, g, b, a]) = *px;
        let lum = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32)
            .round()
            .clamp(0.0, 255.0) as usize;
        let [cr, cg, cb] = lut[lum];
        *px = Rgba([cr, cg, cb, a]);
    }
    out
}

/// LUT 256×RGB do colormap. Exposta para teste.
pub fn colormap_lut(colormap: &str) -> [[u8; 3]; 256] {
    // Pontos de ancoragem (t, [r,g,b]); interpolação linear entre eles.
    let stops: &[(f32, [f32; 3])] = match colormap {
        "jet" => &[
            (0.0, [0.0, 0.0, 128.0]),
            (0.125, [0.0, 0.0, 255.0]),
            (0.375, [0.0, 255.0, 255.0]),
            (0.625, [255.0, 255.0, 0.0]),
            (0.875, [255.0, 0.0, 0.0]),
            (1.0, [128.0, 0.0, 0.0]),
        ],
        "ironbow" => &[
            (0.0, [0.0, 0.0, 0.0]),
            (0.25, [60.0, 0.0, 110.0]),
            (0.5, [180.0, 30.0, 90.0]),
            (0.75, [245.0, 130.0, 20.0]),
            (0.9, [255.0, 220.0, 90.0]),
            (1.0, [255.0, 255.0, 255.0]),
        ],
        "grayscale" => &[(0.0, [0.0, 0.0, 0.0]), (1.0, [255.0, 255.0, 255.0])],
        // viridis (aprox. por âncoras perceptuais)
        _ => &[
            (0.0, [68.0, 1.0, 84.0]),
            (0.25, [59.0, 82.0, 139.0]),
            (0.5, [33.0, 145.0, 140.0]),
            (0.75, [94.0, 201.0, 98.0]),
            (1.0, [253.0, 231.0, 37.0]),
        ],
    };
    let mut lut = [[0u8; 3]; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let t = i as f32 / 255.0;
        *slot = sample_stops(stops, t);
    }
    lut
}

fn sample_stops(stops: &[(f32, [f32; 3])], t: f32) -> [u8; 3] {
    if t <= stops[0].0 {
        return to_u8(stops[0].1);
    }
    if t >= stops[stops.len() - 1].0 {
        return to_u8(stops[stops.len() - 1].1);
    }
    for w in stops.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        if t >= t0 && t <= t1 {
            let f = if (t1 - t0).abs() < 1e-6 {
                0.0
            } else {
                (t - t0) / (t1 - t0)
            };
            return to_u8([
                c0[0] + f * (c1[0] - c0[0]),
                c0[1] + f * (c1[1] - c0[1]),
                c0[2] + f * (c1[2] - c0[2]),
            ]);
        }
    }
    to_u8(stops[stops.len() - 1].1)
}

#[inline]
fn to_u8(c: [f32; 3]) -> [u8; 3] {
    [
        c[0].round().clamp(0.0, 255.0) as u8,
        c[1].round().clamp(0.0, 255.0) as u8,
        c[2].round().clamp(0.0, 255.0) as u8,
    ]
}

// ---------------------------------------------------------------------------
// Conversões de espaço de cor (entrada/saída em [0,1], sRGB-encoded)

/// RGB → HSV. H em graus [0,360), S e V em [0,1].
pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let v = max;
    let s = if max <= 0.0 { 0.0 } else { delta / max };
    let h = if delta < 1e-6 {
        0.0
    } else if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    (h, s, v)
}

/// RGB → YCbCr (Rec.601 full-range). Todos em [0,1].
pub fn rgb_to_ycbcr(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let cb = 0.5 + (b - y) / 1.772;
    let cr = 0.5 + (r - y) / 1.402;
    (y, cb, cr)
}

/// sRGB → CIELAB (D65). L* em [0,100], a*/b* ~ [-128,127].
pub fn rgb_to_lab(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    // 1. sRGB-encoded → linear.
    let lin = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let (rl, gl, bl) = (lin(r), lin(g), lin(b));
    // 2. linear RGB → XYZ (D65).
    let x = 0.4124 * rl + 0.3576 * gl + 0.1805 * bl;
    let y = 0.2126 * rl + 0.7152 * gl + 0.0722 * bl;
    let z = 0.0193 * rl + 0.1192 * gl + 0.9505 * bl;
    // 3. normaliza pelo branco D65.
    let (xn, yn, zn) = (0.95047, 1.0, 1.08883);
    let f = |t: f32| {
        const D: f32 = 6.0 / 29.0;
        if t > D * D * D {
            t.cbrt()
        } else {
            t / (3.0 * D * D) + 4.0 / 29.0
        }
    };
    let (fx, fy, fz) = (f(x / xn), f(y / yn), f(z / zn));
    let l = 116.0 * fy - 16.0;
    let a = 500.0 * (fx - fy);
    let bb = 200.0 * (fy - fz);
    (l, a, bb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_red_of_red_is_max() {
        assert_eq!(channel_value(255, 0, 0, "r"), 255);
        assert_eq!(channel_value(255, 0, 0, "g"), 0);
    }

    #[test]
    fn luma_of_white_is_255_black_is_0() {
        assert_eq!(channel_value(255, 255, 255, "luma"), 255);
        assert_eq!(channel_value(0, 0, 0, "luma"), 0);
    }

    #[test]
    fn hsv_saturation_of_gray_is_zero() {
        // Cinza → S=0.
        assert_eq!(channel_value(128, 128, 128, "s"), 0);
        // Vermelho puro → S=255.
        assert_eq!(channel_value(255, 0, 0, "s"), 255);
    }

    #[test]
    fn hsv_hue_red_near_zero() {
        let h = channel_value(255, 0, 0, "h");
        // H≈0 para vermelho.
        assert!(h <= 2);
    }

    #[test]
    fn ycbcr_gray_has_neutral_chroma() {
        // Cinza neutro → Cb=Cr=128.
        assert!((channel_value(128, 128, 128, "cb") as i32 - 128).abs() <= 1);
        assert!((channel_value(128, 128, 128, "cr") as i32 - 128).abs() <= 1);
    }

    #[test]
    fn lab_l_of_white_is_max() {
        // L* do branco ≈ 100 → 255.
        assert!(channel_value(255, 255, 255, "l_lab") >= 254);
        // L* do preto ≈ 0.
        assert_eq!(channel_value(0, 0, 0, "l_lab"), 0);
    }

    #[test]
    fn lab_neutral_gray_has_centered_ab() {
        // Cinza → a*≈0,b*≈0 → ~128 após offset.
        assert!((channel_value(128, 128, 128, "a_lab") as i32 - 128).abs() <= 2);
        assert!((channel_value(128, 128, 128, "b_lab") as i32 - 128).abs() <= 2);
    }

    #[test]
    fn colormap_endpoints() {
        let jet = colormap_lut("jet");
        // jet começa azul-escuro e termina vermelho-escuro.
        assert!(jet[0][2] > jet[0][0]); // mais azul que vermelho no início
        assert!(jet[255][0] > jet[255][2]); // mais vermelho que azul no fim
        let gray = colormap_lut("grayscale");
        assert_eq!(gray[128], [128, 128, 128]);
    }

    #[test]
    fn false_color_preserves_alpha() {
        let img = RgbaImage::from_pixel(1, 1, Rgba([100, 100, 100, 77]));
        let out = false_color(&img, "viridis");
        assert_eq!(out.get_pixel(0, 0).0[3], 77);
    }
}
