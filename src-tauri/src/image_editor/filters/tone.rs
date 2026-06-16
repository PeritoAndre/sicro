//! W12 (GIMP-parity) — Operações TONAIS por LUT (lookup table).
//!
//! Inspiração: ferramentas Cores → Níveis / Curvas / Posterizar do GIMP.
//! Todas são DETERMINÍSTICAS, NÃO-destrutivas (operam num clone e entram na
//! pilha de processamento) e auditáveis. Nada fabrica conteúdo: apenas
//! redistribuem a tonalidade já presente (realce de visualização §13).
//!
//! Implementação por LUT de 256 entradas por canal — exata e barata.

use image::{Rgba, RgbaImage};

/// Qual(is) canal(is) RGB a operação tonal afeta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneChannel {
    Rgb,
    R,
    G,
    B,
}

impl ToneChannel {
    pub fn from_str(s: &str) -> Self {
        match s {
            "r" | "red" => Self::R,
            "g" | "green" => Self::G,
            "b" | "blue" => Self::B,
            _ => Self::Rgb,
        }
    }
    #[inline]
    fn affects(self, idx: usize) -> bool {
        match self {
            Self::Rgb => idx < 3,
            Self::R => idx == 0,
            Self::G => idx == 1,
            Self::B => idx == 2,
        }
    }
}

/// Aplica uma LUT de 256 entradas aos canais selecionados (alpha intacto).
fn apply_lut(img: &RgbaImage, lut: &[u8; 256], channel: ToneChannel) -> RgbaImage {
    let mut out = img.clone();
    for px in out.pixels_mut() {
        let Rgba([r, g, b, a]) = *px;
        let nr = if channel.affects(0) { lut[r as usize] } else { r };
        let ng = if channel.affects(1) { lut[g as usize] } else { g };
        let nb = if channel.affects(2) { lut[b as usize] } else { b };
        *px = Rgba([nr, ng, nb, a]);
    }
    out
}

/// **Níveis** (Levels): remapeia [in_black, in_white] → [out_black, out_white]
/// com gama no meio. Igual ao diálogo Cores → Níveis do GIMP.
///
/// - `in_black`/`in_white`: ponto preto/branco de entrada (0..255).
/// - `gamma`: > 1 clareia os meios-tons; < 1 escurece.
/// - `out_black`/`out_white`: faixa de saída (compressão de contraste).
pub fn levels(
    img: &RgbaImage,
    channel: ToneChannel,
    in_black: u8,
    in_white: u8,
    gamma: f32,
    out_black: u8,
    out_white: u8,
) -> RgbaImage {
    let lut = levels_lut(in_black, in_white, gamma, out_black, out_white);
    apply_lut(img, &lut, channel)
}

/// Constrói a LUT de Níveis (exposta para teste).
pub fn levels_lut(
    in_black: u8,
    in_white: u8,
    gamma: f32,
    out_black: u8,
    out_white: u8,
) -> [u8; 256] {
    let ib = in_black as f32;
    let iw = in_white as f32;
    let ob = out_black as f32;
    let ow = out_white as f32;
    // Evita divisão por zero quando in_white <= in_black.
    let span = (iw - ib).max(1.0);
    let inv_gamma = if gamma > 1e-4 { 1.0 / gamma } else { 1.0 };
    let mut lut = [0u8; 256];
    for (v, slot) in lut.iter_mut().enumerate() {
        let mut n = ((v as f32 - ib) / span).clamp(0.0, 1.0);
        if (gamma - 1.0).abs() > 1e-4 {
            n = n.powf(inv_gamma);
        }
        let o = ob + n * (ow - ob);
        *slot = o.round().clamp(0.0, 255.0) as u8;
    }
    lut
}

/// **Curvas** (Curves): LUT por interpolação linear monotônica entre pontos
/// de controle `(x, y)` em 0..255. Determinística e previsível (sem
/// overshoot de spline — importante pra honestidade forense). Pontos fora de
/// ordem são ordenados; <2 pontos → identidade.
pub fn curves(img: &RgbaImage, channel: ToneChannel, points: &[(f32, f32)]) -> RgbaImage {
    let lut = curves_lut(points);
    apply_lut(img, &lut, channel)
}

/// Constrói a LUT de Curvas (exposta para teste).
pub fn curves_lut(points: &[(f32, f32)]) -> [u8; 256] {
    let mut lut = [0u8; 256];
    if points.len() < 2 {
        for (v, slot) in lut.iter_mut().enumerate() {
            *slot = v as u8;
        }
        return lut;
    }
    let mut pts: Vec<(f32, f32)> = points
        .iter()
        .map(|&(x, y)| (x.clamp(0.0, 255.0), y.clamp(0.0, 255.0)))
        .collect();
    pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    for (v, slot) in lut.iter_mut().enumerate() {
        let x = v as f32;
        // Antes do primeiro / depois do último ponto: segura no extremo.
        if x <= pts[0].0 {
            *slot = pts[0].1.round() as u8;
            continue;
        }
        if x >= pts[pts.len() - 1].0 {
            *slot = pts[pts.len() - 1].1.round() as u8;
            continue;
        }
        // Encontra o segmento [p0, p1] que contém x e interpola linearmente.
        let mut y = pts[pts.len() - 1].1;
        for w in pts.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            if x >= x0 && x <= x1 {
                let t = if (x1 - x0).abs() < 1e-4 {
                    0.0
                } else {
                    (x - x0) / (x1 - x0)
                };
                y = y0 + t * (y1 - y0);
                break;
            }
        }
        *slot = y.round().clamp(0.0, 255.0) as u8;
    }
    lut
}

/// **Posterizar** (Posterize): reduz cada canal a `levels` níveis igualmente
/// espaçados (2..=255). Útil pra evidenciar bandas/contornos e degradês
/// artificiais (ex.: re-compressão, gradientes inseridos).
pub fn posterize(img: &RgbaImage, levels: u8) -> RgbaImage {
    let lut = posterize_lut(levels);
    apply_lut(img, &lut, ToneChannel::Rgb)
}

/// Constrói a LUT de Posterização (exposta para teste).
pub fn posterize_lut(levels: u8) -> [u8; 256] {
    let n = levels.clamp(2, 255) as f32;
    let mut lut = [0u8; 256];
    for (v, slot) in lut.iter_mut().enumerate() {
        // Quantiza para `n` níveis e re-expande para 0..255.
        let step = (v as f32 / 255.0 * (n - 1.0)).round();
        *slot = (step / (n - 1.0) * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(r: u8, g: u8, b: u8) -> RgbaImage {
        RgbaImage::from_pixel(2, 2, Rgba([r, g, b, 255]))
    }

    #[test]
    fn levels_identity_keeps_values() {
        let lut = levels_lut(0, 255, 1.0, 0, 255);
        assert_eq!(lut[0], 0);
        assert_eq!(lut[128], 128);
        assert_eq!(lut[255], 255);
    }

    #[test]
    fn levels_black_point_clips_shadows() {
        // in_black=50 → tudo <=50 vira 0.
        let lut = levels_lut(50, 255, 1.0, 0, 255);
        assert_eq!(lut[40], 0);
        assert_eq!(lut[50], 0);
        assert!(lut[255] == 255);
    }

    #[test]
    fn levels_gamma_gt_1_brightens_midtones() {
        let lut = levels_lut(0, 255, 2.0, 0, 255);
        // gamma 2 → meio-tom sobe (128 → ~188).
        assert!(lut[128] > 150);
    }

    #[test]
    fn levels_output_range_compresses() {
        let lut = levels_lut(0, 255, 1.0, 30, 200);
        assert_eq!(lut[0], 30);
        assert_eq!(lut[255], 200);
    }

    #[test]
    fn levels_channel_only_affects_target() {
        let img = solid(100, 100, 100);
        let out = levels(&img, ToneChannel::R, 0, 255, 1.0, 0, 100);
        let p = out.get_pixel(0, 0).0;
        assert!(p[0] < 60); // R comprimido
        assert_eq!(p[1], 100); // G intacto
        assert_eq!(p[2], 100); // B intacto
    }

    #[test]
    fn curves_two_points_linear() {
        // Inverte: (0,255)-(255,0).
        let lut = curves_lut(&[(0.0, 255.0), (255.0, 0.0)]);
        assert_eq!(lut[0], 255);
        assert_eq!(lut[255], 0);
        assert!((lut[128] as i32 - 127).abs() <= 1);
    }

    #[test]
    fn curves_identity_when_one_point() {
        let lut = curves_lut(&[(128.0, 200.0)]);
        assert_eq!(lut[10], 10);
        assert_eq!(lut[200], 200);
    }

    #[test]
    fn posterize_reduces_distinct_values() {
        let lut = posterize_lut(2);
        // 2 níveis → só 0 e 255.
        let distinct: std::collections::BTreeSet<u8> = lut.iter().copied().collect();
        assert_eq!(distinct.len(), 2);
        assert!(distinct.contains(&0));
        assert!(distinct.contains(&255));
    }

    #[test]
    fn posterize_four_levels() {
        let lut = posterize_lut(4);
        let distinct: std::collections::BTreeSet<u8> = lut.iter().copied().collect();
        assert_eq!(distinct.len(), 4);
    }
}
