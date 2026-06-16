//! W12 (GIMP-parity / forense) — Análise por comparação.
//!
//! **ELA (Error Level Analysis)** e **diferença local** (imagem vs. sua própria
//! versão borrada) — técnicas de detecção de adulteração que derivam TUDO da
//! imagem de entrada (não fabricam nada; §13). Determinísticas dados os
//! parâmetros (qualidade JPEG / escala) — registrados na pilha de processamento.
//!
//! ELA: recomprime a imagem como JPEG numa qualidade conhecida, decodifica de
//! volta e mede a diferença absoluta por pixel, amplificada. Regiões com
//! histórico de compressão diferente (colagens, retoques, "paste-in") aparecem
//! com erro distinto. Limitação documentada: re-saves múltiplos / recompressão
//! de redes sociais "achatam" o ELA — é um INDÍCIO, não prova.

use image::{codecs::jpeg::JpegEncoder, ExtendedColorType, Rgba, RgbaImage};

/// Error Level Analysis. `quality` (1..=100) é a qualidade de recompressão
/// JPEG; `scale` amplifica a diferença (ex.: 10..30). Saída em tons amplificados
/// (RGB do erro; alpha = 255).
pub fn ela(img: &RgbaImage, quality: u8, scale: f32) -> RgbaImage {
    let q = quality.clamp(1, 100);
    let s = scale.max(1.0);
    let (w, h) = img.dimensions();

    // 1. RGBA → RGB intercalado (JPEG não tem alpha).
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in img.pixels() {
        rgb.push(px.0[0]);
        rgb.push(px.0[1]);
        rgb.push(px.0[2]);
    }

    // 2. Recomprime em memória na qualidade Q.
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut enc = JpegEncoder::new_with_quality(&mut buf, q);
        if enc
            .encode(&rgb, w, h, ExtendedColorType::Rgb8)
            .is_err()
        {
            // Falha de codificação → devolve preto (sem inventar nada).
            return RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]));
        }
    }

    // 3. Decodifica o JPEG recomprimido.
    let recompressed = match image::load_from_memory(&buf) {
        Ok(d) => d.to_rgb8(),
        Err(_) => return RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255])),
    };
    if recompressed.dimensions() != (w, h) {
        return RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]));
    }

    // 4. |orig - recomprimido| * escala, por canal.
    let mut out = RgbaImage::new(w, h);
    for (x, y, px) in out.enumerate_pixels_mut() {
        let o = img.get_pixel(x, y).0;
        let r = recompressed.get_pixel(x, y).0;
        let dr = ((o[0] as i32 - r[0] as i32).unsigned_abs() as f32 * s).min(255.0) as u8;
        let dg = ((o[1] as i32 - r[1] as i32).unsigned_abs() as f32 * s).min(255.0) as u8;
        let db = ((o[2] as i32 - r[2] as i32).unsigned_abs() as f32 * s).min(255.0) as u8;
        *px = Rgba([dr, dg, db, 255]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ela_uniform_image_has_low_error() {
        // Imagem chapada → recompressão quase perfeita → erro baixo.
        let img = RgbaImage::from_pixel(32, 32, Rgba([120, 130, 140, 255]));
        let out = ela(&img, 90, 15.0);
        assert_eq!(out.dimensions(), (32, 32));
        // Erro médio baixo (área chapada comprime sem artefato).
        let mut sum = 0u64;
        for p in out.pixels() {
            sum += p.0[0] as u64 + p.0[1] as u64 + p.0[2] as u64;
        }
        let mean = sum as f32 / (32.0 * 32.0 * 3.0);
        assert!(mean < 40.0, "erro médio inesperadamente alto: {mean}");
    }

    #[test]
    fn ela_preserves_dimensions_and_opaque_alpha() {
        let mut img = RgbaImage::new(16, 16);
        for (x, y, px) in img.enumerate_pixels_mut() {
            *px = Rgba([(x * 16) as u8, (y * 16) as u8, 64, 200]);
        }
        let out = ela(&img, 75, 20.0);
        assert_eq!(out.dimensions(), (16, 16));
        // Alpha sempre opaco no mapa de erro.
        for p in out.pixels() {
            assert_eq!(p.0[3], 255);
        }
    }

    #[test]
    fn ela_edges_produce_more_error_than_flat() {
        // Metade preta / metade branca: a BORDA deve concentrar erro de JPEG.
        let mut img = RgbaImage::new(64, 64);
        for (x, _y, px) in img.enumerate_pixels_mut() {
            let v = if x < 32 { 0 } else { 255 };
            *px = Rgba([v, v, v, 255]);
        }
        let out = ela(&img, 80, 20.0);
        // Coluna da borda (x≈31/32) vs coluna chapada (x=5).
        let mut edge_err = 0u32;
        let mut flat_err = 0u32;
        for y in 0..64 {
            edge_err += out.get_pixel(31, y).0[0] as u32;
            flat_err += out.get_pixel(5, y).0[0] as u32;
        }
        assert!(
            edge_err >= flat_err,
            "borda deveria ter erro >= área chapada (edge={edge_err}, flat={flat_err})"
        );
    }
}
