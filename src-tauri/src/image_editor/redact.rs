//! Tarjas de anonimização aplicadas nos pixels finais da exportação: desfoque, pixelização ou
//! preta, em retângulo, elipse ou contorno livre. Mesmas proporções da prévia do editor.

use image::RgbaImage;

use crate::models::RedactionSpec;

/// Raio do desfoque / lado do bloco em px, proporcionais à área e à intensidade (10–100).
fn amount(style: &str, w: f64, h: f64, strength: f64) -> u32 {
    let m = w.min(h);
    let k = strength.clamp(10.0, 100.0) / 100.0;
    if style == "blur" {
        (m * (0.03 + 0.12 * k)).round().max(3.0) as u32
    } else {
        (m * (0.04 + 0.16 * k)).round().max(4.0) as u32
    }
}

fn bbox(r: &RedactionSpec) -> (f64, f64, f64, f64) {
    if r.shape == "free" && r.points.len() >= 3 {
        let xs = r.points.iter().map(|p| p.x);
        let ys = r.points.iter().map(|p| p.y);
        let x0 = xs.clone().fold(f64::INFINITY, f64::min);
        let x1 = xs.fold(f64::NEG_INFINITY, f64::max);
        let y0 = ys.clone().fold(f64::INFINITY, f64::min);
        let y1 = ys.fold(f64::NEG_INFINITY, f64::max);
        return (x0, y0, x1 - x0, y1 - y0);
    }
    let (x, w) = if r.width < 0.0 { (r.x + r.width, -r.width) } else { (r.x, r.width) };
    let (y, h) = if r.height < 0.0 { (r.y + r.height, -r.height) } else { (r.y, r.height) };
    (x, y, w, h)
}

/// Faixas [x0, x1) cobertas pela forma na linha `py` (centro do pixel), em px da imagem.
fn row_spans(r: &RedactionSpec, bx: f64, by: f64, bw: f64, bh: f64, py: f64) -> Vec<(f64, f64)> {
    match r.shape.as_str() {
        "ellipse" => {
            let (cx, cy, rx, ry) = (bx + bw / 2.0, by + bh / 2.0, bw / 2.0, bh / 2.0);
            let t = 1.0 - ((py - cy) / ry).powi(2);
            if t <= 0.0 || rx <= 0.0 {
                return vec![];
            }
            let dx = rx * t.sqrt();
            vec![(cx - dx, cx + dx)]
        }
        "free" if r.points.len() >= 3 => {
            // Par-ímpar: interseções das arestas com a linha horizontal.
            let n = r.points.len();
            let mut xs: Vec<f64> = Vec::new();
            for i in 0..n {
                let a = &r.points[i];
                let b = &r.points[(i + 1) % n];
                if (a.y <= py && b.y > py) || (b.y <= py && a.y > py) {
                    xs.push(a.x + (py - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
            xs.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
            xs.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect()
        }
        _ => vec![(bx, bx + bw)],
    }
}

fn box_blur(buf: &mut [u8], w: usize, h: usize, r: usize) {
    let mut tmp = vec![0u8; buf.len()];
    let div = (2 * r + 1) as u32;
    let pass = |src: &[u8], dst: &mut [u8], horizontal: bool| {
        let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
        for o in 0..outer {
            let at = |i: usize| if horizontal { (o * w + i) * 4 } else { (i * w + o) * 4 };
            for c in 0..4 {
                let mut acc: u32 = 0;
                for i in -(r as isize)..=(r as isize) {
                    acc += src[at(i.clamp(0, inner as isize - 1) as usize) + c] as u32;
                }
                for i in 0..inner {
                    dst[at(i) + c] = (acc / div) as u8;
                    let add = (i + r + 1).min(inner - 1);
                    let sub = i.saturating_sub(r);
                    acc = acc + src[at(add) + c] as u32 - src[at(sub) + c] as u32;
                }
            }
        }
    };
    for _ in 0..3 {
        pass(buf, &mut tmp, true);
        pass(&tmp, buf, false);
    }
}

fn pixelate(buf: &mut [u8], w: usize, h: usize, block: usize, ox: usize, oy: usize) {
    let start = |o: usize| -((block - o % block) as isize % block as isize);
    let mut by = start(oy);
    while by < h as isize {
        let mut bx = start(ox);
        while bx < w as isize {
            let (xs, ys) = (bx.max(0) as usize, by.max(0) as usize);
            let (xe, ye) = (((bx + block as isize) as usize).min(w), ((by + block as isize) as usize).min(h));
            let mut sum = [0u64; 4];
            let mut n = 0u64;
            for y in ys..ye {
                for x in xs..xe {
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        sum[c] += buf[i + c] as u64;
                    }
                    n += 1;
                }
            }
            if n > 0 {
                for y in ys..ye {
                    for x in xs..xe {
                        let i = (y * w + x) * 4;
                        for c in 0..4 {
                            buf[i + c] = (sum[c] / n) as u8;
                        }
                    }
                }
            }
            bx += block as isize;
        }
        by += block as isize;
    }
}

pub fn apply(img: &mut RgbaImage, r: &RedactionSpec) {
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    let (bx, by, bw, bh) = bbox(r);
    if bw < 1.0 || bh < 1.0 {
        return;
    }
    let amt = amount(&r.style, bw, bh, r.strength) as i64;
    let margin = if r.style == "blur" { amt * 3 } else { 0 };
    // Recorte com margem (o desfoque lê vizinhos reais), limitado à imagem.
    let sx = ((bx.floor() as i64) - margin).clamp(0, iw);
    let sy = ((by.floor() as i64) - margin).clamp(0, ih);
    let ex = ((bx + bw).ceil() as i64 + margin).clamp(0, iw);
    let ey = ((by + bh).ceil() as i64 + margin).clamp(0, ih);
    let (cw, ch) = ((ex - sx) as usize, (ey - sy) as usize);
    if cw == 0 || ch == 0 {
        return;
    }
    let mut work = vec![0u8; cw * ch * 4];
    for y in 0..ch {
        for x in 0..cw {
            let p = img.get_pixel((sx as usize + x) as u32, (sy as usize + y) as u32).0;
            work[(y * cw + x) * 4..(y * cw + x) * 4 + 4].copy_from_slice(&p);
        }
    }
    match r.style.as_str() {
        "blur" => box_blur(&mut work, cw, ch, amt.max(1) as usize),
        "pixelate" => pixelate(
            &mut work,
            cw,
            ch,
            amt.max(2) as usize,
            (bx.floor() as i64 - sx).max(0) as usize,
            (by.floor() as i64 - sy).max(0) as usize,
        ),
        _ => work.chunks_mut(4).for_each(|p| p.copy_from_slice(&[0, 0, 0, 255])),
    }
    // Copia de volta só o que está dentro da forma.
    let y0 = (by.floor() as i64).clamp(0, ih);
    let y1 = ((by + bh).ceil() as i64).clamp(0, ih);
    for py in y0..y1 {
        for (a, b) in row_spans(r, bx, by, bw, bh, py as f64 + 0.5) {
            let xa = (a.round() as i64).clamp(0, iw);
            let xb = (b.round() as i64).clamp(0, iw);
            for px in xa..xb {
                let (lx, ly) = ((px - sx) as usize, (py - sy) as usize);
                if lx < cw && ly < ch {
                    let i = (ly * cw + lx) * 4;
                    img.put_pixel(px as u32, py as u32, image::Rgba([work[i], work[i + 1], work[i + 2], work[i + 3]]));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RedactionPoint, RedactionSpec};

    fn spec(style: &str, shape: &str) -> RedactionSpec {
        RedactionSpec { style: style.into(), shape: shape.into(), x: 10.0, y: 10.0, width: 20.0, height: 20.0, points: vec![], strength: 60.0 }
    }

    #[test]
    fn preta_retangulo_so_dentro() {
        let mut img = RgbaImage::from_pixel(40, 40, image::Rgba([200, 200, 200, 255]));
        apply(&mut img, &spec("solid", "rect"));
        assert_eq!(img.get_pixel(15, 15).0, [0, 0, 0, 255]);
        assert_eq!(img.get_pixel(5, 5).0, [200, 200, 200, 255]);
    }

    #[test]
    fn livre_respeita_o_contorno() {
        let mut img = RgbaImage::from_pixel(40, 40, image::Rgba([200, 200, 200, 255]));
        let mut s = spec("solid", "free");
        s.points = vec![RedactionPoint { x: 10.0, y: 10.0 }, RedactionPoint { x: 30.0, y: 10.0 }, RedactionPoint { x: 10.0, y: 30.0 }];
        apply(&mut img, &s);
        assert_eq!(img.get_pixel(12, 12).0, [0, 0, 0, 255]); // dentro do triângulo
        assert_eq!(img.get_pixel(28, 28).0, [200, 200, 200, 255]); // fora (outra metade do quadrado)
    }

    #[test]
    fn desfoque_espalha_borda() {
        let mut img = RgbaImage::from_fn(40, 40, |x, _| if x < 20 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) });
        apply(&mut img, &spec("blur", "rect"));
        let v = img.get_pixel(20, 20).0[0];
        assert!(v > 30 && v < 225, "borda deveria ficar intermediária: {v}");
    }
}
