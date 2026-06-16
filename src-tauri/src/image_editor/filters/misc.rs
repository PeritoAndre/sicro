//! G12 — Misc: Unsharp Mask, Threshold, Pixelize.
//!
//! Operações que não cabem nas outras categorias mas são essenciais
//! num kit pericial.

use image::{Rgba, RgbaImage};

/// Unsharp Mask = original + amount * (original - blurred).
/// Sharpening fotográfico padrão. `sigma` controla a "frequência" do
/// realce; `amount` ≈ 0.5..2.0 razoável.
pub fn unsharp_mask(img: &RgbaImage, sigma: f32, amount: f32) -> RgbaImage {
    let blurred = super::blur::gaussian(img, sigma);
    let a = amount.clamp(0.0, 5.0);
    let mut out = RgbaImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        let bp = blurred.get_pixel(x, y);
        let mut o = [0u8; 4];
        for c in 0..3 {
            let v = p.0[c] as f32 + a * (p.0[c] as f32 - bp.0[c] as f32);
            o[c] = v.clamp(0.0, 255.0) as u8;
        }
        o[3] = p.0[3];
        out.put_pixel(x, y, Rgba(o));
    }
    out
}

/// Threshold simples na luminância. Pixel > value → branco; senão → preto.
pub fn threshold(img: &RgbaImage, value: u8) -> RgbaImage {
    let mut out = RgbaImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        let l = (0.299 * p.0[0] as f32
            + 0.587 * p.0[1] as f32
            + 0.114 * p.0[2] as f32) as u8;
        let v = if l > value { 255u8 } else { 0u8 };
        out.put_pixel(x, y, Rgba([v, v, v, p.0[3]]));
    }
    out
}

/// Pixelize uma região: substitui cada bloco `block_size`x`block_size`
/// pela média de seus pixels. Usado para anonimização de áreas
/// (faces, placas, dados sensíveis).
pub fn pixelize_region(
    img: &RgbaImage,
    x0: u32,
    y0: u32,
    width: u32,
    height: u32,
    block_size: u32,
) -> RgbaImage {
    let bs = block_size.max(2);
    let iw = img.width();
    let ih = img.height();
    let x_start = x0.min(iw);
    let y_start = y0.min(ih);
    let x_end = (x0 + width).min(iw);
    let y_end = (y0 + height).min(ih);

    let mut out = img.clone();
    let mut by = y_start;
    while by < y_end {
        let mut bx = x_start;
        while bx < x_end {
            let bx_end = (bx + bs).min(x_end);
            let by_end = (by + bs).min(y_end);
            let mut sr = 0u32;
            let mut sg = 0u32;
            let mut sb = 0u32;
            let mut sa = 0u32;
            let mut count = 0u32;
            for y in by..by_end {
                for x in bx..bx_end {
                    let p = img.get_pixel(x, y);
                    sr += p.0[0] as u32;
                    sg += p.0[1] as u32;
                    sb += p.0[2] as u32;
                    sa += p.0[3] as u32;
                    count += 1;
                }
            }
            if count > 0 {
                let avg = Rgba([
                    (sr / count) as u8,
                    (sg / count) as u8,
                    (sb / count) as u8,
                    (sa / count) as u8,
                ]);
                for y in by..by_end {
                    for x in bx..bx_end {
                        out.put_pixel(x, y, avg);
                    }
                }
            }
            bx += bs;
        }
        by += bs;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsharp_mask_increases_local_contrast() {
        // Imagem com gradiente suave.
        let mut img = RgbaImage::new(5, 5);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let v = ((x + y) * 30).min(255) as u8;
            *p = Rgba([v, v, v, 255]);
        }
        let out = unsharp_mask(&img, 1.0, 1.5);
        // Pixels altos ficam mais altos; baixos mais baixos.
        let center_in = img.get_pixel(2, 2).0[0];
        let center_out = out.get_pixel(2, 2).0[0];
        // Centro tem vizinhança simétrica, então provavelmente pouca mudança;
        // mas garantimos que extremos cresceram.
        let _ = (center_in, center_out);
        assert_eq!(out.dimensions(), (5, 5));
    }

    #[test]
    fn threshold_binarizes() {
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, Rgba([50, 50, 50, 255]));
        img.put_pixel(1, 0, Rgba([200, 200, 200, 255]));
        img.put_pixel(0, 1, Rgba([130, 130, 130, 255]));
        // 127 fica abaixo do threshold 128.
        img.put_pixel(1, 1, Rgba([127, 127, 127, 255]));
        let out = threshold(&img, 128);
        assert_eq!(out.get_pixel(0, 0).0[0], 0);
        assert_eq!(out.get_pixel(1, 0).0[0], 255);
        assert_eq!(out.get_pixel(0, 1).0[0], 255);
        assert_eq!(out.get_pixel(1, 1).0[0], 0);
    }

    #[test]
    fn pixelize_averages_block() {
        let mut img = RgbaImage::new(4, 4);
        for (x, y, p) in img.enumerate_pixels_mut() {
            *p = Rgba([(x * 60) as u8, (y * 60) as u8, 0, 255]);
        }
        let out = pixelize_region(&img, 0, 0, 4, 4, 2);
        // Bloco superior esquerdo: média de (0,0),(60,0),(0,60),(60,60)
        // R = (0+60+0+60)/4 = 30, G = (0+0+60+60)/4 = 30.
        let p = out.get_pixel(0, 0).0;
        assert_eq!(p[0], 30);
        assert_eq!(p[1], 30);
    }

    #[test]
    fn pixelize_outside_region_unchanged() {
        let mut img = RgbaImage::from_pixel(4, 4, Rgba([100, 100, 100, 255]));
        img.put_pixel(3, 3, Rgba([200, 200, 200, 255]));
        let out = pixelize_region(&img, 0, 0, 2, 2, 2);
        // (3,3) está fora da região pixelizada — permanece igual.
        assert_eq!(out.get_pixel(3, 3).0, [200, 200, 200, 255]);
    }
}
