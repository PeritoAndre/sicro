//! Indícios de manipulação digital (Fase 5 — Bloco B).
//!
//! ELA (Error Level Analysis) e mapa de ruído. Produzem HEATMAPS
//! **indicativos**: destacam regiões que merecem exame humano. **Não**
//! concluem adulteração e têm falsos-positivos conhecidos — ELA acende
//! naturalmente em bordas/alto contraste e em regiões já recomprimidas; o
//! ruído varia com iluminação, ISO, foco e compressão. §13: a interpretação
//! e a conclusão são exclusivamente do perito. Nenhum "score" é calculado.

use std::collections::{HashMap, HashSet};

use image::{DynamicImage, Rgba, RgbaImage};

/// Error Level Analysis. Recomprime a imagem em JPEG na `quality` dada,
/// compara com a versão atual e amplifica a diferença por canal (`gain`).
/// Regiões inseridas/editadas depois da última gravação tendem a destoar no
/// nível de erro da recompressão. Se a codificação falhar, devolve o original.
pub fn ela(img: &RgbaImage, quality: u8, gain: u32) -> RgbaImage {
    let q = quality.clamp(1, 100);
    let g = gain.clamp(1, 80).max(1);
    let rgb = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q);
        if enc.encode_image(&rgb).is_err() {
            return img.clone();
        }
    }
    let rec = match image::load_from_memory(&buf) {
        Ok(d) => d.to_rgb8(),
        Err(_) => return img.clone(),
    };
    let (w, h) = (img.width(), img.height());
    let mut out = RgbaImage::new(w, h);
    let amp = |a: u8, b: u8| -> u8 {
        (((a as i32 - b as i32).unsigned_abs()) * g).min(255) as u8
    };
    for y in 0..h {
        for x in 0..w {
            let o = rgb.get_pixel(x, y);
            let r = rec.get_pixel(x, y);
            out.put_pixel(
                x,
                y,
                Rgba([amp(o[0], r[0]), amp(o[1], r[1]), amp(o[2], r[2]), 255]),
            );
        }
    }
    out
}

/// Luminância (Rec.601) como vetor `f32` em ordem row-major.
fn to_luma(img: &RgbaImage) -> (Vec<f32>, u32, u32) {
    let (w, h) = (img.width(), img.height());
    let mut l = vec![0f32; (w * h) as usize];
    for (i, p) in img.pixels().enumerate() {
        l[i] = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
    }
    (l, w, h)
}

/// Box-blur separável (média em janela de raio `r`), com borda truncada.
fn box_blur(src: &[f32], w: u32, h: u32, r: u32) -> Vec<f32> {
    let (wi, hi, ri) = (w as i32, h as i32, r as i32);
    let mut tmp = vec![0f32; src.len()];
    for y in 0..hi {
        for x in 0..wi {
            let mut sum = 0f32;
            let mut n = 0f32;
            for dx in -ri..=ri {
                let xx = x + dx;
                if xx >= 0 && xx < wi {
                    sum += src[(y * wi + xx) as usize];
                    n += 1.0;
                }
            }
            tmp[(y * wi + x) as usize] = sum / n;
        }
    }
    let mut out = vec![0f32; src.len()];
    for y in 0..hi {
        for x in 0..wi {
            let mut sum = 0f32;
            let mut n = 0f32;
            for dy in -ri..=ri {
                let yy = y + dy;
                if yy >= 0 && yy < hi {
                    sum += tmp[(yy * wi + x) as usize];
                    n += 1.0;
                }
            }
            out[(y * wi + x) as usize] = sum / n;
        }
    }
    out
}

/// Mapa de ruído: energia local de alta frequência. Calcula o resíduo
/// (luma − luma borrada) e a média local de |resíduo| numa janela; normaliza
/// para 0..255 por um teto robusto (média + 3·desvio). Claro = mais
/// ruído/textura. Saltos bruscos de ruído entre regiões PODEM sugerir
/// composição — mas variam naturalmente com luz, foco e compressão.
pub fn noise_map(img: &RgbaImage, window: u32) -> RgbaImage {
    let (luma, w, h) = to_luma(img);
    let win = window.clamp(1, 32);
    let low = box_blur(&luma, w, h, 2);
    let mut resid = vec![0f32; luma.len()];
    for i in 0..luma.len() {
        resid[i] = (luma[i] - low[i]).abs();
    }
    let energy = box_blur(&resid, w, h, win);

    // Teto robusto: média + 3·desvio (evita um único pico saturar o mapa).
    let n = energy.len().max(1) as f32;
    let mean = energy.iter().sum::<f32>() / n;
    let var = energy.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
    let hi = mean + 3.0 * var.sqrt();
    let scale = if hi > 0.001 { 255.0 / hi } else { 0.0 };

    let mut out = RgbaImage::new(w, h);
    for (i, p) in out.pixels_mut().enumerate() {
        let v = (energy[i] * scale).clamp(0.0, 255.0) as u8;
        *p = Rgba([v, v, v, 255]);
    }
    out
}

/// Detecção de **copy-move** (regiões clonadas dentro da MESMA imagem).
///
/// Blocos sobrepostos viram um descritor de baixa frequência (média 4×4);
/// blocos quase idênticos que estão a um **mesmo vetor de deslocamento** votam
/// nesse vetor. Só vetores com **muitos pares** (região, não coincidência) e
/// **magnitude mínima** (ignora vizinhança trivial) são aceitos; blocos
/// **lisos** (baixa variância — papel em branco, céu) são descartados, pois
/// casam com tudo. Saída: cinza esmaecido + blocos clonados tintados, uma cor
/// por família (fonte e cópia partilham a cor). §13: indício, não conclusão —
/// texturas repetitivas (grades, tramas) geram falso-positivo.
pub fn copy_move(img: &RgbaImage, block: u32, step: u32) -> RgbaImage {
    let (luma, w, h) = to_luma(img);
    let block = block.clamp(8, 64);
    let step = step.clamp(4, block).max(1);
    let cell = (block / 4).max(1);

    struct B {
        x: u32,
        y: u32,
        feat: [i32; 16],
    }
    let mut blocks: Vec<B> = Vec::new();
    if w > block && h > block {
        let mut by = 0u32;
        while by + block <= h {
            let mut bx = 0u32;
            while bx + block <= w {
                let mut feat = [0i32; 16];
                let mut sum = 0f32;
                let mut sumsq = 0f32;
                let mut k = 0usize;
                for cyi in 0..4u32 {
                    for cxi in 0..4u32 {
                        let mut acc = 0f32;
                        let mut n = 0f32;
                        for yy in 0..cell {
                            for xx in 0..cell {
                                let px = bx + cxi * cell + xx;
                                let py = by + cyi * cell + yy;
                                if px < w && py < h {
                                    let v = luma[(py * w + px) as usize];
                                    acc += v;
                                    n += 1.0;
                                    sum += v;
                                    sumsq += v * v;
                                }
                            }
                        }
                        feat[k] = (acc / n.max(1.0)) as i32;
                        k += 1;
                    }
                }
                let cnt = (block * block) as f32;
                let mean = sum / cnt;
                let var = (sumsq / cnt - mean * mean).max(0.0);
                if var.sqrt() >= 7.0 {
                    blocks.push(B { x: bx, y: by, feat });
                }
                bx += step;
            }
            by += step;
        }
    }

    blocks.sort_by(|a, b| a.feat.cmp(&b.feat));

    let min_mag2 = (block as i64) * (block as i64);
    let feat_thresh = 110i32;
    let neighbor = 3usize;
    let mut families: HashMap<(i32, i32), Vec<(usize, usize)>> = HashMap::new();
    for i in 0..blocks.len() {
        for j in (i + 1)..(i + 1 + neighbor).min(blocks.len()) {
            let mut d = 0i32;
            for t in 0..16 {
                d += (blocks[i].feat[t] - blocks[j].feat[t]).abs();
            }
            if d > feat_thresh {
                continue;
            }
            let dx = blocks[j].x as i32 - blocks[i].x as i32;
            let dy = blocks[j].y as i32 - blocks[i].y as i32;
            if (dx as i64 * dx as i64 + dy as i64 * dy as i64) < min_mag2 {
                continue;
            }
            let key = if (dy, dx) < (0, 0) { (-dx, -dy) } else { (dx, dy) };
            families.entry(key).or_default().push((i, j));
        }
    }

    const MIN_PAIRS: usize = 8;
    let mut dominant: Vec<(&(i32, i32), &Vec<(usize, usize)>)> = families
        .iter()
        .filter(|(_, v)| v.len() >= MIN_PAIRS)
        .collect();
    dominant.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    let mut out = RgbaImage::new(w, h);
    for (i, p) in out.pixels_mut().enumerate() {
        let g = (luma[i] * 0.42) as u8;
        *p = Rgba([g, g, g, 255]);
    }
    const PALETTE: [[u8; 3]; 6] = [
        [235, 50, 50],
        [50, 200, 90],
        [70, 130, 245],
        [240, 170, 30],
        [200, 70, 220],
        [40, 200, 210],
    ];
    for (fi, (_key, pairs)) in dominant.iter().enumerate() {
        let col = PALETTE[fi % PALETTE.len()];
        let mut marked: HashSet<usize> = HashSet::new();
        for &(a, b) in pairs.iter() {
            marked.insert(a);
            marked.insert(b);
        }
        for bi in marked {
            let (bx, by) = (blocks[bi].x, blocks[bi].y);
            for yy in 0..block {
                for xx in 0..block {
                    let px = bx + xx;
                    let py = by + yy;
                    if px < w && py < h {
                        let p = out.get_pixel_mut(px, py);
                        p[0] = (p[0] as f32 * 0.4 + col[0] as f32 * 0.6) as u8;
                        p[1] = (p[1] as f32 * 0.4 + col[1] as f32 * 0.6) as u8;
                        p[2] = (p[2] as f32 * 0.4 + col[2] as f32 * 0.6) as u8;
                    }
                }
            }
        }
    }
    out
}

/// Round-trip JPEG (codifica na qualidade `q` e decodifica de volta) — "assa"
/// o histórico de compressão nos pixels. Usado pelo gerador de amostra-teste.
fn jpeg_roundtrip(img: &RgbaImage, q: u8) -> Option<RgbaImage> {
    let rgb = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q);
        enc.encode_image(&rgb).ok()?;
    }
    Some(image::load_from_memory(&buf).ok()?.to_rgba8())
}

/// Posição/tamanho do enxerto plantado na amostra-teste (px da imagem 640×480).
pub const SAMPLE_PATCH: [u32; 4] = [380, 120, 180, 120];
pub const SAMPLE_SIZE: [u32; 2] = [640, 480];

/// Gera uma amostra-teste **determinística** para validar o ELA (caso-controle
/// positivo). Fundo de baixa frequência é "assado" em JPEG (ganha histórico de
/// compressão); depois um enxerto de alta frequência é colado fresco (sem esse
/// histórico). Ao salvar o conjunto como JPEG e rodar o ELA, o enxerto destoa.
/// É um arquivo sintético de teste — não um documento real.
pub fn synth_ela_sample() -> RgbaImage {
    let (w, h) = (SAMPLE_SIZE[0], SAMPLE_SIZE[1]);
    let mut base = RgbaImage::new(w, h);
    for (x, y, p) in base.enumerate_pixels_mut() {
        let fx = x as f32 / w as f32;
        let fy = y as f32 / h as f32;
        let r = (60.0 + 120.0 * fx) as u8;
        let g = (80.0 + 100.0 * fy) as u8;
        let b = (160.0 - 70.0 * fx).max(0.0) as u8;
        *p = Rgba([r, g, b, 255]);
    }
    let mut out = jpeg_roundtrip(&base, 85).unwrap_or(base);
    let [px, py, pw, ph] = SAMPLE_PATCH;
    for yy in py..(py + ph).min(h) {
        for xx in px..(px + pw).min(w) {
            let v = if ((xx / 3) + (yy / 3)) % 2 == 0 { 20 } else { 235 };
            out.put_pixel(xx, yy, Rgba([v, v, v, 255]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([rgb[0], rgb[1], rgb[2], 255]))
    }

    fn avg(img: &RgbaImage) -> f32 {
        let mut s = 0u64;
        for p in img.pixels() {
            s += p[0] as u64 + p[1] as u64 + p[2] as u64;
        }
        s as f32 / (img.width() * img.height() * 3) as f32
    }

    #[test]
    fn ela_preserves_dimensions() {
        let img = solid(24, 18, [120, 120, 120]);
        let out = ela(&img, 90, 15);
        assert_eq!(out.dimensions(), (24, 18));
    }

    #[test]
    fn ela_of_uniform_image_is_dark() {
        // Imagem chapada recomprime quase sem erro → ELA ~ preto.
        let img = solid(32, 32, [128, 128, 128]);
        let out = ela(&img, 90, 15);
        assert!(avg(&out) < 20.0, "ELA de imagem uniforme deveria ser escuro");
    }

    #[test]
    fn noise_map_of_uniform_image_is_black() {
        let img = solid(32, 32, [90, 90, 90]);
        let out = noise_map(&img, 4);
        assert_eq!(avg(&out), 0.0);
    }

    #[test]
    fn synth_sample_lights_patch_under_ela() {
        let img = synth_ela_sample();
        let e = ela(&img, 90, 15);
        let mean = |x0: u32, y0: u32, w: u32, h: u32| -> f64 {
            let mut s = 0u64;
            let mut n = 0u64;
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let p = e.get_pixel(x, y);
                    s += p[0] as u64 + p[1] as u64 + p[2] as u64;
                    n += 3;
                }
            }
            s as f64 / n.max(1) as f64
        };
        let [px, py, pw, ph] = SAMPLE_PATCH;
        let patch = mean(px + 10, py + 10, pw - 20, ph - 20);
        let bg = mean(40, 320, 150, 90);
        assert!(
            patch > bg,
            "enxerto deveria acender mais que o fundo (patch={patch}, bg={bg})"
        );
    }

    #[test]
    fn copy_move_flags_cloned_region() {
        let (w, h) = (256u32, 256u32);
        let mut img = RgbaImage::new(w, h);
        // fundo gradiente (baixa variância → blocos ignorados)
        for (x, _y, p) in img.enumerate_pixels_mut() {
            let v = (40.0 + 80.0 * (x as f32 / w as f32)) as u8;
            *p = Rgba([v, v, v, 255]);
        }
        // textura distinta 48×48 (alta variância, não auto-similar)
        let ps = 48u32;
        let mut patch = vec![0u8; (ps * ps) as usize];
        for v in 0..ps {
            for u in 0..ps {
                patch[(v * ps + u) as usize] =
                    ((u * 37 + v * 101 + ((u * v) % 17) * 23) % 256) as u8;
            }
        }
        let stamp = |img: &mut RgbaImage, ox: u32, oy: u32| {
            for v in 0..ps {
                for u in 0..ps {
                    let g = patch[(v * ps + u) as usize];
                    img.put_pixel(ox + u, oy + v, Rgba([g, g, g, 255]));
                }
            }
        };
        // offset (128,128): múltiplo do passo → grades de blocos alinham
        stamp(&mut img, 32, 32);
        stamp(&mut img, 160, 160);
        let out = copy_move(&img, 16, 8);
        let colored = |x0: u32, y0: u32| -> u32 {
            let mut c = 0;
            for y in y0..y0 + ps {
                for x in x0..x0 + ps {
                    let p = out.get_pixel(x, y);
                    if p[0] != p[2] || p[1] != p[2] {
                        c += 1;
                    }
                }
            }
            c
        };
        assert!(colored(32, 32) > 0, "região-fonte clonada deveria ser marcada");
        assert!(colored(160, 160) > 0, "região-cópia deveria ser marcada");
    }

    #[test]
    fn copy_move_uniform_has_no_marks() {
        let img = solid(128, 128, [110, 110, 110]);
        let out = copy_move(&img, 16, 8);
        // imagem uniforme: nenhum bloco marcado → tudo cinza (R==G==B)
        let mut colored = 0;
        for p in out.pixels() {
            if p[0] != p[2] || p[1] != p[2] {
                colored += 1;
            }
        }
        assert_eq!(colored, 0, "imagem uniforme não deveria gerar marcações");
    }

    #[test]
    fn noise_map_detects_high_frequency() {
        // Tabuleiro de xadrez 1px = altíssima frequência → mapa com sinal.
        let mut img = RgbaImage::new(32, 32);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let v = if (x + y) % 2 == 0 { 0 } else { 255 };
            *p = Rgba([v, v, v, 255]);
        }
        let out = noise_map(&img, 3);
        let mut mx = 0u8;
        for p in out.pixels() {
            mx = mx.max(p[0]);
        }
        assert!(mx > 0, "mapa de ruído deveria ter sinal em alta frequência");
    }
}
