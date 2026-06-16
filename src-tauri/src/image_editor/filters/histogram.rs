//! G12.9 — Cálculo de histograma + estatísticas.
//!
//! Função pura sobre RgbaImage. Retorna 4 vetores de 256 bins (R, G, B,
//! Luminância) + estatísticas por canal.

use image::RgbaImage;

use crate::models::{HistogramStats, ImageHistogram};

pub fn compute(img: &RgbaImage) -> ImageHistogram {
    let mut red = vec![0u32; 256];
    let mut green = vec![0u32; 256];
    let mut blue = vec![0u32; 256];
    let mut lum = vec![0u32; 256];

    let mut sum_r = 0f64;
    let mut sum_g = 0f64;
    let mut sum_b = 0f64;
    let mut sum_l = 0f64;
    let mut sum_r2 = 0f64;
    let mut sum_g2 = 0f64;
    let mut sum_b2 = 0f64;
    let mut sum_l2 = 0f64;
    let mut min_l: u8 = 255;
    let mut max_l: u8 = 0;

    for p in img.pixels() {
        let r = p.0[0];
        let g = p.0[1];
        let b = p.0[2];
        let l = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32)
            .clamp(0.0, 255.0) as u8;
        red[r as usize] += 1;
        green[g as usize] += 1;
        blue[b as usize] += 1;
        lum[l as usize] += 1;

        sum_r += r as f64;
        sum_g += g as f64;
        sum_b += b as f64;
        sum_l += l as f64;
        sum_r2 += (r as f64) * (r as f64);
        sum_g2 += (g as f64) * (g as f64);
        sum_b2 += (b as f64) * (b as f64);
        sum_l2 += (l as f64) * (l as f64);

        if l < min_l {
            min_l = l;
        }
        if l > max_l {
            max_l = l;
        }
    }

    let total = (img.width() * img.height()) as u32;
    let n = total as f64;
    let mean_r = (sum_r / n) as f32;
    let mean_g = (sum_g / n) as f32;
    let mean_b = (sum_b / n) as f32;
    let mean_lum = (sum_l / n) as f32;
    let var_r = (sum_r2 / n) - (sum_r / n).powi(2);
    let var_g = (sum_g2 / n) - (sum_g / n).powi(2);
    let var_b = (sum_b2 / n) - (sum_b / n).powi(2);
    let var_l = (sum_l2 / n) - (sum_l / n).powi(2);

    ImageHistogram {
        red,
        green,
        blue,
        luminance: lum,
        stats: HistogramStats {
            mean_r,
            mean_g,
            mean_b,
            mean_lum,
            stddev_r: var_r.max(0.0).sqrt() as f32,
            stddev_g: var_g.max(0.0).sqrt() as f32,
            stddev_b: var_b.max(0.0).sqrt() as f32,
            stddev_lum: var_l.max(0.0).sqrt() as f32,
            min_lum: min_l,
            max_lum: max_l,
            total_pixels: total,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn solid_red_image_histogram_concentrates_in_255_0_0() {
        let img = RgbaImage::from_pixel(4, 4, Rgba([255, 0, 0, 255]));
        let h = compute(&img);
        assert_eq!(h.red[255], 16);
        assert_eq!(h.red[0], 0);
        assert_eq!(h.green[0], 16);
        assert_eq!(h.blue[0], 16);
        // Luminância de vermelho puro = 0.299*255 ≈ 76.
        assert_eq!(h.luminance[76], 16);
        assert!((h.stats.mean_r - 255.0).abs() < 0.1);
        assert!((h.stats.mean_lum - 76.0).abs() < 1.0);
        assert_eq!(h.stats.total_pixels, 16);
    }

    #[test]
    fn min_max_luminance_correctly_detected() {
        let mut img = RgbaImage::from_pixel(4, 4, Rgba([100, 100, 100, 255]));
        img.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        img.put_pixel(1, 1, Rgba([255, 255, 255, 255]));
        let h = compute(&img);
        assert_eq!(h.stats.min_lum, 0);
        assert_eq!(h.stats.max_lum, 255);
    }
}
