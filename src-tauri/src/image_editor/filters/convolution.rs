//! Convolução genérica NxN com kernel, divisor e offset informados pelo perito.
//! `out = offset + (1/divisor)·Σ K[i,j]·I(x+i−c, y+j−c)`; borda por clamp; alpha preservado.

use image::{Rgba, RgbaImage};

/// `kernel` tem `size*size` elementos (size ímpar). `divisor <= 0` → soma do
/// kernel (ou 1 se soma 0). `offset` é somado ao resultado (ex.: 128 para emboss).
pub fn convolve(img: &RgbaImage, kernel: &[f32], size: u32, divisor: f32, offset: f32) -> RgbaImage {
    let n = size.clamp(1, 9) as i32;
    // Kernel inválido → identidade.
    if kernel.len() != (n * n) as usize || n % 2 == 0 {
        return img.clone();
    }
    let c = n / 2;
    let sum: f32 = kernel.iter().sum();
    let div = if divisor.abs() > 1e-6 {
        divisor
    } else if sum.abs() > 1e-6 {
        sum
    } else {
        1.0
    };

    let w = img.width() as i32;
    let h = img.height() as i32;
    let mut out = RgbaImage::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            for ky in 0..n {
                for kx in 0..n {
                    let xx = (x + kx - c).clamp(0, w - 1) as u32;
                    let yy = (y + ky - c).clamp(0, h - 1) as u32;
                    let k = kernel[(ky * n + kx) as usize];
                    let p = img.get_pixel(xx, yy).0;
                    acc[0] += k * p[0] as f32;
                    acc[1] += k * p[1] as f32;
                    acc[2] += k * p[2] as f32;
                }
            }
            let a = img.get_pixel(x as u32, y as u32).0[3];
            out.put_pixel(
                x as u32,
                y as u32,
                Rgba([
                    (acc[0] / div + offset).round().clamp(0.0, 255.0) as u8,
                    (acc[1] / div + offset).round().clamp(0.0, 255.0) as u8,
                    (acc[2] / div + offset).round().clamp(0.0, 255.0) as u8,
                    a,
                ]),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> RgbaImage {
        let mut img = RgbaImage::new(5, 5);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let v = ((x + y) * 20) as u8;
            *p = Rgba([v, v, v, 255]);
        }
        img
    }

    #[test]
    fn identity_kernel_keeps_image() {
        let img = ramp();
        let k = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let out = convolve(&img, &k, 3, 1.0, 0.0);
        assert_eq!(out.get_pixel(2, 2).0, img.get_pixel(2, 2).0);
    }

    #[test]
    fn box_blur_averages() {
        // Kernel de 1s, divisor 9 → média 3x3. No centro de uma rampa, a
        // média ~ valor central.
        let img = ramp();
        let k = [1.0; 9];
        let out = convolve(&img, &k, 3, 9.0, 0.0);
        let center = out.get_pixel(2, 2).0[0] as i32;
        let orig = img.get_pixel(2, 2).0[0] as i32;
        assert!((center - orig).abs() <= 2);
    }

    #[test]
    fn invalid_kernel_is_identity() {
        let img = ramp();
        // size 3 mas só 4 elementos → identidade.
        let out = convolve(&img, &[1.0, 2.0, 3.0, 4.0], 3, 1.0, 0.0);
        assert_eq!(out.get_pixel(1, 1).0, img.get_pixel(1, 1).0);
    }

    #[test]
    fn divisor_zero_falls_back_to_kernel_sum() {
        let img = RgbaImage::from_pixel(3, 3, Rgba([100, 100, 100, 255]));
        let k = [1.0; 9]; // soma 9
        let out = convolve(&img, &k, 3, 0.0, 0.0);
        // média de área chapada = 100.
        assert_eq!(out.get_pixel(1, 1).0[0], 100);
    }
}
