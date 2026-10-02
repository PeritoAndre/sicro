//! Morfologia em luminância (dilatação, erosão, abertura, fechamento) com
//! janela quadrada de raio `radius`.

use image::{Rgba, RgbaImage};

#[inline]
fn lum_u8(p: Rgba<u8>) -> u8 {
    (0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32) as u8
}

/// Máximo da vizinhança.
pub fn dilate(img: &RgbaImage, radius: u32) -> RgbaImage {
    apply_morph(img, radius, true)
}

/// Mínimo da vizinhança.
pub fn erode(img: &RgbaImage, radius: u32) -> RgbaImage {
    apply_morph(img, radius, false)
}

/// Erosão → dilatação: remove ruído branco pontual.
pub fn open(img: &RgbaImage, radius: u32) -> RgbaImage {
    let e = erode(img, radius);
    dilate(&e, radius)
}

/// Dilatação → erosão: preenche buracos pretos pequenos.
pub fn close(img: &RgbaImage, radius: u32) -> RgbaImage {
    let d = dilate(img, radius);
    erode(&d, radius)
}

fn apply_morph(img: &RgbaImage, radius: u32, dilation: bool) -> RgbaImage {
    let r = radius.max(1) as i32;
    let w = img.width() as i32;
    let h = img.height() as i32;
    let mut out = RgbaImage::new(w as u32, h as u32);

    for y in 0..h {
        for x in 0..w {
            let mut best: u8 = if dilation { 0 } else { 255 };
            for dy in -r..=r {
                for dx in -r..=r {
                    let xx = (x + dx).clamp(0, w - 1) as u32;
                    let yy = (y + dy).clamp(0, h - 1) as u32;
                    let v = lum_u8(*img.get_pixel(xx, yy));
                    if dilation {
                        if v > best {
                            best = v;
                        }
                    } else if v < best {
                        best = v;
                    }
                }
            }
            let a = img.get_pixel(x as u32, y as u32).0[3];
            out.put_pixel(x as u32, y as u32, Rgba([best, best, best, a]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring_image() -> RgbaImage {
        // 5x5 com pixel branco isolado no centro.
        let mut img = RgbaImage::from_pixel(5, 5, Rgba([0, 0, 0, 255]));
        img.put_pixel(2, 2, Rgba([255, 255, 255, 255]));
        img
    }

    #[test]
    fn dilate_grows_white_region() {
        let img = ring_image();
        let out = dilate(&img, 1);
        // Após dilate raio 1, o branco vira 3x3.
        assert_eq!(out.get_pixel(1, 2).0[0], 255);
        assert_eq!(out.get_pixel(2, 1).0[0], 255);
        assert_eq!(out.get_pixel(3, 3).0[0], 255);
    }

    #[test]
    fn erode_shrinks_white_region() {
        // Imagem 5x5 toda branca exceto borda preta.
        let mut img = RgbaImage::from_pixel(5, 5, Rgba([255, 255, 255, 255]));
        for x in 0..5 {
            img.put_pixel(x, 0, Rgba([0, 0, 0, 255]));
            img.put_pixel(x, 4, Rgba([0, 0, 0, 255]));
        }
        for y in 0..5 {
            img.put_pixel(0, y, Rgba([0, 0, 0, 255]));
            img.put_pixel(4, y, Rgba([0, 0, 0, 255]));
        }
        let out = erode(&img, 1);
        // Erode raio 1 reduz o branco para 1x1 no centro.
        assert_eq!(out.get_pixel(2, 2).0[0], 255);
        assert_eq!(out.get_pixel(1, 1).0[0], 0);
    }

    #[test]
    fn open_removes_isolated_noise() {
        let img = ring_image();
        let out = open(&img, 1);
        // Open com raio 1 elimina spike isolado.
        assert_eq!(out.get_pixel(2, 2).0[0], 0);
    }

    #[test]
    fn close_fills_isolated_hole() {
        // 5x5 todo branco com 1 pixel preto isolado.
        let mut img = RgbaImage::from_pixel(5, 5, Rgba([255, 255, 255, 255]));
        img.put_pixel(2, 2, Rgba([0, 0, 0, 255]));
        let out = close(&img, 1);
        // Close raio 1 preenche o buraco.
        assert_eq!(out.get_pixel(2, 2).0[0], 255);
    }
}
