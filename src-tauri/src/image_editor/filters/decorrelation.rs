//! W12 (forense) — **Decorrelation stretch** (estilo "DStretch").
//!
//! Amplifica diferenças de cor SUTIS, invisíveis no RGB: tinta apagada,
//! marcas latentes, hematomas, texto de fundo, pigmentos próximos. Técnica
//! consagrada em arqueologia (pinturas rupestres) e documentoscopia.
//!
//! Pipeline (determinístico, derivado 100% da imagem — §13):
//!   1. média µ e covariância Σ (3×3) dos canais R,G,B sobre a imagem;
//!   2. autodecomposição Σ = R·Λ·Rᵀ (componentes principais);
//!   3. transforma cada pixel: y = µ_alvo + R·S·Rᵀ·(x − µ),
//!      com S = diag(σ_alvo / √λ_i) — equaliza a variância de cada PC,
//!      "esticando" os eixos de menor variância (onde moram as diferenças sutis);
//!   4. clampa a [0,255].
//!
//! A matriz de transformação e a região-fonte ficam registráveis no log
//! (reprodutível). NÃO inventa conteúdo: é uma transformação afim linear da
//! cor existente.

use image::{Rgba, RgbaImage};
use nalgebra::{Matrix3, SymmetricEigen, Vector3};

/// Aplica decorrelation stretch. `target_sigma` controla a intensidade do
/// realce (desvio-padrão alvo por componente; ~30..80). `target_mean` é o
/// nível central de saída (tipicamente 128).
pub fn decorrelation_stretch(img: &RgbaImage, target_sigma: f32, target_mean: f32) -> RgbaImage {
    let sigma = target_sigma.clamp(5.0, 127.0);
    let mean_out = target_mean.clamp(0.0, 255.0);

    let (mu, cov) = mean_and_covariance(img);
    let (transform, offset) = build_transform(mu, cov, sigma, mean_out);

    let mut out = img.clone();
    for px in out.pixels_mut() {
        let Rgba([r, g, b, a]) = *px;
        let v = Vector3::new(r as f32, g as f32, b as f32);
        let y = transform * v + offset;
        *px = Rgba([
            y.x.round().clamp(0.0, 255.0) as u8,
            y.y.round().clamp(0.0, 255.0) as u8,
            y.z.round().clamp(0.0, 255.0) as u8,
            a,
        ]);
    }
    out
}

/// Média (µ) e covariância (Σ) dos canais RGB sobre todos os pixels.
fn mean_and_covariance(img: &RgbaImage) -> (Vector3<f32>, Matrix3<f32>) {
    let n = (img.width() as f32 * img.height() as f32).max(1.0);
    let mut sum = Vector3::zeros();
    for px in img.pixels() {
        sum += Vector3::new(px.0[0] as f32, px.0[1] as f32, px.0[2] as f32);
    }
    let mu = sum / n;

    let mut cov = Matrix3::zeros();
    for px in img.pixels() {
        let d = Vector3::new(px.0[0] as f32, px.0[1] as f32, px.0[2] as f32) - mu;
        cov += d * d.transpose();
    }
    cov /= n;
    (mu, cov)
}

/// Constrói (T, offset) tal que y = T·x + offset = µ_alvo + R·S·Rᵀ·(x − µ).
fn build_transform(
    mu: Vector3<f32>,
    cov: Matrix3<f32>,
    target_sigma: f32,
    target_mean: f32,
) -> (Matrix3<f32>, Vector3<f32>) {
    let eig = SymmetricEigen::new(cov);
    let r = eig.eigenvectors; // colunas = componentes principais
    // S = diag(σ_alvo / √λ_i). Eigenvalues podem ser ~0 (canal degenerado) →
    // clampa para evitar explosão; um eixo sem variância não é amplificado.
    let mut s = Matrix3::zeros();
    for i in 0..3 {
        let lambda = eig.eigenvalues[i].max(1e-3);
        s[(i, i)] = target_sigma / lambda.sqrt();
    }
    let transform = r * s * r.transpose();
    let mean_target = Vector3::new(target_mean, target_mean, target_mean);
    let offset = mean_target - transform * mu;
    (transform, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cria imagem com gradiente sutil num eixo de cor de baixa variância.
    fn subtle_image() -> RgbaImage {
        let mut img = RgbaImage::new(32, 32);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            // Base cinza ~ 120; pequena modulação no canal R (±3).
            let r = (120 + (x as i32 % 6) - 3) as u8;
            *px = Rgba([r, 120, 120, 255]);
        }
        img
    }

    #[test]
    fn stretch_amplifies_subtle_variation() {
        let img = subtle_image();
        let out = decorrelation_stretch(&img, 50.0, 128.0);
        assert_eq!(out.dimensions(), (32, 32));

        // Amplitude do canal R ANTES é minúscula; DEPOIS deve ser bem maior.
        let r_range = |im: &RgbaImage| {
            let (mut lo, mut hi) = (255i32, 0i32);
            for p in im.pixels() {
                lo = lo.min(p.0[0] as i32);
                hi = hi.max(p.0[0] as i32);
            }
            hi - lo
        };
        let before = r_range(&img);
        let after = r_range(&out);
        assert!(
            after > before,
            "decorrelation deveria ampliar a variação sutil (antes={before}, depois={after})"
        );
    }

    #[test]
    fn stretch_preserves_dimensions_and_alpha() {
        let mut img = RgbaImage::new(8, 8);
        for (i, px) in img.pixels_mut().enumerate() {
            *px = Rgba([(i * 3) as u8, (i * 2) as u8, i as u8, 111]);
        }
        let out = decorrelation_stretch(&img, 40.0, 128.0);
        assert_eq!(out.dimensions(), (8, 8));
        for p in out.pixels() {
            assert_eq!(p.0[3], 111);
        }
    }

    #[test]
    fn flat_image_does_not_panic() {
        // Imagem perfeitamente chapada → covariância nula. Não pode quebrar.
        let img = RgbaImage::from_pixel(10, 10, Rgba([90, 90, 90, 255]));
        let out = decorrelation_stretch(&img, 50.0, 128.0);
        assert_eq!(out.dimensions(), (10, 10));
    }
}
