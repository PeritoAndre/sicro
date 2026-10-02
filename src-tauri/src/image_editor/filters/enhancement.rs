//! Realce: CLAHE, equalização de histograma, subtract background, auto-levels
//! e white balance.

use image::{Rgba, RgbaImage};

/// Histogram equalization global — aplica na luminância e
/// reconstrói RGB preservando matiz.
pub fn histogram_equalize(img: &RgbaImage) -> RgbaImage {
    let w = img.width();
    let h = img.height();
    let total = (w * h) as f32;

    let mut hist = [0u32; 256];
    for p in img.pixels() {
        let l = (0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32) as usize;
        hist[l.min(255)] += 1;
    }

    let mut cdf = [0u32; 256];
    let mut acc = 0u32;
    for i in 0..256 {
        acc += hist[i];
        cdf[i] = acc;
    }
    let cdf_min = cdf.iter().copied().find(|&v| v > 0).unwrap_or(0) as f32;
    let mut lut = [0u8; 256];
    for i in 0..256 {
        let v = ((cdf[i] as f32 - cdf_min) / (total - cdf_min).max(1.0) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
        lut[i] = v;
    }

    // Remapeia luminância preservando matiz (multiplica RGB pela razão).
    let mut out = RgbaImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels() {
        let l_old =
            0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32;
        let l_new = lut[l_old.clamp(0.0, 255.0) as usize] as f32;
        let ratio = if l_old > 1.0 { l_new / l_old } else { 1.0 };
        let r = (p.0[0] as f32 * ratio).clamp(0.0, 255.0) as u8;
        let g = (p.0[1] as f32 * ratio).clamp(0.0, 255.0) as u8;
        let b = (p.0[2] as f32 * ratio).clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, Rgba([r, g, b, p.0[3]]));
    }
    out
}

/// CLAHE clássico (Zuiderveld): tiles não sobrepostos, clip-limit, interpolação
/// bilinear. Parâmetros espelham o "Enhance Local Contrast" do Fiji (block size,
/// max slope, bins); resultado equivalente, não idêntico (o Fiji usa janela deslizante).
pub fn clahe(img: &RgbaImage, tile_size: u32, clip_limit: f32, bins: u32) -> RgbaImage {
    let ts = tile_size.max(2);
    let cl = clip_limit.max(1.0);
    let nb = (bins.clamp(2, 256)) as usize;
    let w = img.width();
    let h = img.height();
    let tx_count = (w + ts - 1) / ts;
    let ty_count = (h + ts - 1) / ts;

    let bin_of = |l: f32| -> usize { ((l.clamp(0.0, 255.0) as usize) * nb / 256).min(nb - 1) };

    let mut luts = vec![vec![0u8; nb]; (tx_count * ty_count) as usize];
    for ty in 0..ty_count {
        for tx in 0..tx_count {
            let x0 = tx * ts;
            let y0 = ty * ts;
            let x1 = (x0 + ts).min(w);
            let y1 = (y0 + ts).min(h);
            let tile_pixels = ((x1 - x0) * (y1 - y0)) as f32;
            if tile_pixels < 1.0 {
                continue;
            }

            let mut hist = vec![0u32; nb];
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = img.get_pixel(x, y);
                    let l = 0.299 * p.0[0] as f32
                        + 0.587 * p.0[1] as f32
                        + 0.114 * p.0[2] as f32;
                    hist[bin_of(l)] += 1;
                }
            }

            // Clip: redistribui excesso uniformemente entre os bins.
            let clip_count = (cl * tile_pixels / nb as f32).max(1.0) as u32;
            let mut excess = 0u32;
            for h_val in hist.iter_mut() {
                if *h_val > clip_count {
                    excess += *h_val - clip_count;
                    *h_val = clip_count;
                }
            }
            let bonus = excess / nb as u32;
            let remainder = (excess % nb as u32) as usize;
            for h_val in hist.iter_mut() {
                *h_val += bonus;
            }
            for bin in hist.iter_mut().take(remainder) {
                *bin += 1;
            }

            let mut acc = 0u32;
            let lut = &mut luts[(ty * tx_count + tx) as usize];
            for i in 0..nb {
                acc += hist[i];
                lut[i] = ((acc as f32 / tile_pixels) * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }

    // Remap bilinear entre as LUTs dos 4 tiles vizinhos.
    let mut out = RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let p = *img.get_pixel(x, y);
            let l_old = 0.299 * p.0[0] as f32
                + 0.587 * p.0[1] as f32
                + 0.114 * p.0[2] as f32;
            let bin = bin_of(l_old);

            // Coordenada do pixel em "centros de tile".
            let fx = (x as f32 + 0.5) / ts as f32 - 0.5;
            let fy = (y as f32 + 0.5) / ts as f32 - 0.5;
            let tx0 = fx.floor().clamp(0.0, (tx_count - 1) as f32) as u32;
            let ty0 = fy.floor().clamp(0.0, (ty_count - 1) as f32) as u32;
            let tx1 = (tx0 + 1).min(tx_count - 1);
            let ty1 = (ty0 + 1).min(ty_count - 1);
            let dx = (fx - tx0 as f32).clamp(0.0, 1.0);
            let dy = (fy - ty0 as f32).clamp(0.0, 1.0);

            let v00 = luts[(ty0 * tx_count + tx0) as usize][bin] as f32;
            let v10 = luts[(ty0 * tx_count + tx1) as usize][bin] as f32;
            let v01 = luts[(ty1 * tx_count + tx0) as usize][bin] as f32;
            let v11 = luts[(ty1 * tx_count + tx1) as usize][bin] as f32;
            let v = (1.0 - dx) * (1.0 - dy) * v00
                + dx * (1.0 - dy) * v10
                + (1.0 - dx) * dy * v01
                + dx * dy * v11;

            let ratio = if l_old > 1.0 { v / l_old } else { 1.0 };
            out.put_pixel(
                x,
                y,
                Rgba([
                    (p.0[0] as f32 * ratio).clamp(0.0, 255.0) as u8,
                    (p.0[1] as f32 * ratio).clamp(0.0, 255.0) as u8,
                    (p.0[2] as f32 * ratio).clamp(0.0, 255.0) as u8,
                    p.0[3],
                ]),
            );
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Subtract Background (rolling ball, Sternberg 1983) — paridade com o ImageJ/Fiji,
// reimplementado do zero (nada copiado do código GPL). "Rolar a bola por baixo"
// da luminância = abertura morfológica em cinza com elemento esférico.

/// Média 3×3 (borda replicada): pré-suavização para o ruído não "espetar" a bola.
fn mean3x3(buf: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0f32;
            let mut count = 0f32;
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    let sx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                    let sy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                    sum += buf[sy * w + sx];
                    count += 1.0;
                }
            }
            out[y * w + x] = sum / count;
        }
    }
    out
}

/// Redução por mínimo em blocos `factor`×`factor` (o fundo é a envoltória inferior).
fn downsample_min(buf: &[f32], w: usize, h: usize, factor: usize) -> (Vec<f32>, usize, usize) {
    let sw = w.div_ceil(factor);
    let sh = h.div_ceil(factor);
    let mut out = vec![f32::INFINITY; sw * sh];
    for y in 0..h {
        for x in 0..w {
            let sx = x / factor;
            let sy = y / factor;
            let v = buf[y * w + x];
            let cell = &mut out[sy * sw + sx];
            if v < *cell {
                *cell = v;
            }
        }
    }
    for cell in out.iter_mut() {
        if !cell.is_finite() {
            *cell = 0.0;
        }
    }
    (out, sw, sh)
}

fn upsample_bilinear(buf: &[f32], sw: usize, sh: usize, w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        let gy = ((y as f32 + 0.5) * sh as f32 / h as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let y0 = gy.floor() as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let fy = gy - y0 as f32;
        for x in 0..w {
            let gx = ((x as f32 + 0.5) * sw as f32 / w as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let x0 = gx.floor() as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let fx = gx - x0 as f32;
            let v00 = buf[y0 * sw + x0];
            let v10 = buf[y0 * sw + x1];
            let v01 = buf[y1 * sw + x0];
            let v11 = buf[y1 * sw + x1];
            out[y * w + x] = (1.0 - fx) * (1.0 - fy) * v00
                + fx * (1.0 - fy) * v10
                + (1.0 - fx) * fy * v01
                + fx * fy * v11;
        }
    }
    out
}

/// Calota esférica (elemento estruturante) num grid (2r+1)²; valor < 0 = fora do disco.
fn build_ball(radius: f32) -> (Vec<f32>, usize) {
    let r = (radius.round().max(1.0)) as usize;
    let diam = 2 * r + 1;
    let r2 = radius * radius;
    let mut ball = vec![-1f32; diam * diam];
    for j in 0..diam {
        for i in 0..diam {
            let dx = i as f32 - r as f32;
            let dy = j as f32 - r as f32;
            let s = r2 - dx * dx - dy * dy;
            if s >= 0.0 {
                ball[j * diam + i] = s.sqrt();
            }
        }
    }
    (ball, r)
}

/// Abertura em cinza: erosão (min de `v - z`) seguida de dilatação (max de `e + z`).
fn ball_open(buf: &[f32], w: usize, h: usize, radius: f32) -> Vec<f32> {
    let (ball, r) = build_ball(radius);
    let diam = 2 * r + 1;
    let ri = r as isize;
    let wi = w as isize;
    let hi = h as isize;

    let mut ero = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut m = f32::INFINITY;
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    let z = ball[((dy + ri) as usize) * diam + (dx + ri) as usize];
                    if z < 0.0 {
                        continue;
                    }
                    let sx = (x as isize + dx).clamp(0, wi - 1) as usize;
                    let sy = (y as isize + dy).clamp(0, hi - 1) as usize;
                    let v = buf[sy * w + sx] - z;
                    if v < m {
                        m = v;
                    }
                }
            }
            ero[y * w + x] = m;
        }
    }

    let mut dil = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut m = f32::NEG_INFINITY;
            for dy in -ri..=ri {
                for dx in -ri..=ri {
                    let z = ball[((dy + ri) as usize) * diam + (dx + ri) as usize];
                    if z < 0.0 {
                        continue;
                    }
                    let sx = (x as isize + dx).clamp(0, wi - 1) as usize;
                    let sy = (y as isize + dy).clamp(0, hi - 1) as usize;
                    let v = ero[sy * w + sx] + z;
                    if v > m {
                        m = v;
                    }
                }
            }
            dil[y * w + x] = m;
        }
    }
    dil
}

/// Fundo via bola rolante; encolhe a imagem para raios grandes (como o ImageJ).
fn rolling_ball_background(
    work: &[f32],
    w: usize,
    h: usize,
    radius: f32,
    smooth: bool,
) -> Vec<f32> {
    let base = if smooth { mean3x3(work, w, h) } else { work.to_vec() };

    // Fator de encolhimento por faixa de raio (paridade ImageJ).
    let shrink = if radius <= 10.0 {
        1
    } else if radius <= 30.0 {
        2
    } else if radius <= 100.0 {
        4
    } else {
        8
    };

    let bg_full = if shrink > 1 {
        let (small, sw, sh) = downsample_min(&base, w, h, shrink);
        let ball_r = (radius / shrink as f32).max(1.0);
        let small_bg = ball_open(&small, sw, sh, ball_r);
        upsample_bilinear(&small_bg, sw, sh, w, h)
    } else {
        ball_open(&base, w, h, radius.max(1.0))
    };

    // Overshoot da interpolação não pode pôr o fundo acima da superfície.
    let mut out = bg_full;
    for i in 0..w * h {
        if out[i] > work[i] {
            out[i] = work[i];
        }
    }
    out
}

/// `light_background`: objeto escuro sobre fundo claro (inverte antes e depois).
/// Trabalha na luminância e reaplica a R/G/B por razão (preserva a cor).
pub fn subtract_background(
    img: &RgbaImage,
    radius: f32,
    light_background: bool,
    disable_smoothing: bool,
) -> RgbaImage {
    let w = img.width() as usize;
    let h = img.height() as usize;
    if w == 0 || h == 0 {
        return img.clone();
    }
    let radius = radius.max(1.0);
    let n = w * h;

    let mut work = vec![0f32; n];
    for (i, p) in img.pixels().enumerate() {
        let l = 0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32;
        work[i] = if light_background { 255.0 - l } else { l };
    }

    let bg = rolling_ball_background(&work, w, h, radius, !disable_smoothing);

    let mut out = RgbaImage::new(img.width(), img.height());
    for (i, p) in img.pixels().enumerate() {
        let orig_l = 0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32;
        let fg = (work[i] - bg[i]).max(0.0);
        let new_l = if light_background { 255.0 - fg } else { fg };
        let ratio = if orig_l > 1.0 { new_l / orig_l } else { 1.0 };
        let x = (i % w) as u32;
        let y = (i / w) as u32;
        if orig_l > 1.0 {
            out.put_pixel(
                x,
                y,
                Rgba([
                    (p.0[0] as f32 * ratio).clamp(0.0, 255.0) as u8,
                    (p.0[1] as f32 * ratio).clamp(0.0, 255.0) as u8,
                    (p.0[2] as f32 * ratio).clamp(0.0, 255.0) as u8,
                    p.0[3],
                ]),
            );
        } else {
            // Quase preto: a razão seria instável; grava a luminância como cinza.
            let v = new_l.clamp(0.0, 255.0) as u8;
            out.put_pixel(x, y, Rgba([v, v, v, p.0[3]]));
        }
    }
    out
}

/// Estica o histograma por canal entre os percentis `pct_low` e `pct_high` (0..100).
pub fn auto_levels(img: &RgbaImage, pct_low: f32, pct_high: f32) -> RgbaImage {
    let pct_lo = pct_low.clamp(0.0, 49.0);
    let pct_hi = pct_high.clamp(pct_lo + 1.0, 100.0);
    let total = (img.width() * img.height()) as u32;
    let target_lo = (total as f32 * pct_lo / 100.0) as u32;
    let target_hi = (total as f32 * pct_hi / 100.0) as u32;

    let mut bounds = [(0u8, 255u8); 3];
    for ch in 0..3_usize {
        let mut hist = [0u32; 256];
        for p in img.pixels() {
            hist[p.0[ch] as usize] += 1;
        }
        let mut acc = 0u32;
        let mut lo = 0u8;
        let mut hi = 255u8;
        for i in 0..256 {
            acc += hist[i];
            if acc >= target_lo {
                lo = i as u8;
                break;
            }
        }
        acc = 0;
        for i in 0..256 {
            acc += hist[i];
            if acc >= target_hi {
                hi = i as u8;
                break;
            }
        }
        // `saturating_add`: `lo` pode ser 255 (imagem toda branca ou estourada).
        bounds[ch] = (lo, hi.max(lo.saturating_add(1)));
    }

    let mut out = RgbaImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        let mut o = [0u8; 4];
        for ch in 0..3 {
            let (lo, hi) = bounds[ch];
            let v = p.0[ch] as f32;
            let stretched =
                ((v - lo as f32) / (hi - lo).max(1) as f32) * 255.0;
            o[ch] = stretched.clamp(0.0, 255.0) as u8;
        }
        o[3] = p.0[3];
        out.put_pixel(x, y, Rgba(o));
    }
    out
}

/// Gray-world: assume média da cena cinza e iguala a média dos canais.
pub fn white_balance_gray_world(img: &RgbaImage) -> RgbaImage {
    let mut sum_r = 0u64;
    let mut sum_g = 0u64;
    let mut sum_b = 0u64;
    for p in img.pixels() {
        sum_r += p.0[0] as u64;
        sum_g += p.0[1] as u64;
        sum_b += p.0[2] as u64;
    }
    let n = (img.width() * img.height()) as f32;
    let mean_r = sum_r as f32 / n;
    let mean_g = sum_g as f32 / n;
    let mean_b = sum_b as f32 / n;
    let gray = (mean_r + mean_g + mean_b) / 3.0;
    let kr = if mean_r > 0.5 { gray / mean_r } else { 1.0 };
    let kg = if mean_g > 0.5 { gray / mean_g } else { 1.0 };
    let kb = if mean_b > 0.5 { gray / mean_b } else { 1.0 };

    let mut out = RgbaImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        out.put_pixel(
            x,
            y,
            Rgba([
                (p.0[0] as f32 * kr).clamp(0.0, 255.0) as u8,
                (p.0[1] as f32 * kg).clamp(0.0, 255.0) as u8,
                (p.0[2] as f32 * kb).clamp(0.0, 255.0) as u8,
                p.0[3],
            ]),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_equalize_stretches_dynamic_range() {
        // Imagem cinza em [50..100] — equalize deve expandir para ~[0..255].
        let mut img = RgbaImage::new(10, 10);
        for (i, p) in img.pixels_mut().enumerate() {
            let v = (50 + (i % 50)) as u8;
            *p = Rgba([v, v, v, 255]);
        }
        let out = histogram_equalize(&img);
        let mut max_v = 0u8;
        let mut min_v = 255u8;
        for p in out.pixels() {
            max_v = max_v.max(p.0[0]);
            min_v = min_v.min(p.0[0]);
        }
        assert!(max_v - min_v > 200, "dinâmica deveria expandir");
    }

    #[test]
    fn auto_levels_handles_constant_image() {
        // Imagem totalmente cinza → não pode dividir por zero.
        let img = RgbaImage::from_pixel(4, 4, Rgba([128, 128, 128, 255]));
        let out = auto_levels(&img, 1.0, 99.0);
        for p in out.pixels() {
            for ch in 0..3 {
                assert!(p.0[ch] <= 255);
            }
        }
    }

    #[test]
    fn white_balance_gray_world_neutralizes_colour_cast() {
        // Imagem inteira com cast vermelho.
        let img = RgbaImage::from_pixel(4, 4, Rgba([200, 100, 100, 255]));
        let out = white_balance_gray_world(&img);
        let p = out.get_pixel(0, 0).0;
        // Após gray-world, todos os canais ficam próximos.
        let max = p[0].max(p[1]).max(p[2]) as i32;
        let min = p[0].min(p[1]).min(p[2]) as i32;
        assert!(max - min < 15, "white balance deveria neutralizar");
    }

    #[test]
    fn clahe_small_image_does_not_panic() {
        let img = RgbaImage::from_pixel(4, 4, Rgba([100, 100, 100, 255]));
        let out = clahe(&img, 4, 2.0, 256);
        assert_eq!(out.dimensions(), (4, 4));
    }

    #[test]
    fn clahe_bins_param_changes_result_and_is_bounded() {
        // Gradiente diagonal — dá histograma rico por tile.
        let mut img = RgbaImage::new(32, 32);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let v = (((x + y) * 255) / 64) as u8;
            *p = Rgba([v, v, v, 255]);
        }
        let full = clahe(&img, 8, 3.0, 256);
        let coarse = clahe(&img, 8, 3.0, 32);
        // Menos bins (32) deve produzir um resultado DIFERENTE de 256 bins…
        let differs = full
            .pixels()
            .zip(coarse.pixels())
            .any(|(a, b)| a.0 != b.0);
        assert!(differs, "bins deveria alterar o resultado do CLAHE");
        // …e ambos permanecem em 8 bits válidos (sem panic / overflow).
        assert_eq!(coarse.dimensions(), (32, 32));
    }

    /// Rampa suave sem feições: fundo escuro nunca clareia um pixel e a
    /// luminância média cai bastante.
    #[test]
    fn subtract_background_flattens_smooth_gradient() {
        let (w, h) = (60u32, 40u32);
        let mut img = RgbaImage::new(w, h);
        for (x, _y, p) in img.enumerate_pixels_mut() {
            // rampa horizontal 20..200
            let v = (20 + (x * 180 / w)) as u8;
            *p = Rgba([v, v, v, 255]);
        }
        let lum = |p: &Rgba<u8>| {
            0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32
        };
        let mean_in: f32 =
            img.pixels().map(lum).sum::<f32>() / (w * h) as f32;
        let out = subtract_background(&img, 25.0, false, false);

        // (1) Nenhum pixel clareou.
        let no_brightening = img
            .pixels()
            .zip(out.pixels())
            .all(|(a, b)| lum(b) <= lum(a) + 1.0);
        assert!(no_brightening, "fundo escuro não deveria clarear nenhum pixel");

        // (2) A média despencou.
        let mean_out: f32 = out.pixels().map(lum).sum::<f32>() / (w * h) as f32;
        assert!(
            mean_out < mean_in * 0.5,
            "gradiente deveria ser muito reduzido (média {mean_in:.0} → {mean_out:.0})"
        );
    }

    /// Fundo claro: resultado em fundo branco quase uniforme, objeto ainda escuro.
    #[test]
    fn subtract_background_light_yields_white_field_dark_object() {
        let (w, h) = (40u32, 40u32);
        // Fundo claro 210; um quadradinho escuro (40) no centro.
        let mut img = RgbaImage::from_pixel(w, h, Rgba([210, 210, 210, 255]));
        for y in 18..22 {
            for x in 18..22 {
                img.put_pixel(x, y, Rgba([40, 40, 40, 255]));
            }
        }
        let out = subtract_background(&img, 15.0, true, false);
        let lum = |x: u32, y: u32| {
            let p = out.get_pixel(x, y).0;
            0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32
        };
        // Canto (fundo) deve virar quase branco.
        assert!(lum(0, 0) > 230.0, "fundo deveria clarear (canto={})", lum(0, 0));
        // Centro (objeto) deve permanecer claramente mais escuro que o fundo.
        assert!(
            lum(20, 20) < lum(0, 0) - 60.0,
            "objeto deveria continuar escuro (centro={}, canto={})",
            lum(20, 20),
            lum(0, 0)
        );
    }

    #[test]
    fn subtract_background_constant_image_is_safe() {
        let img = RgbaImage::from_pixel(8, 8, Rgba([128, 128, 128, 255]));
        let out = subtract_background(&img, 10.0, false, false);
        assert_eq!(out.dimensions(), (8, 8));
    }
}
