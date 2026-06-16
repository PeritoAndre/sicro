//! Pure-pixel operations applied during the export pipeline (MVP 7).
//!
//! Cada função aceita uma `RgbaImage` mutável (ou retorna nova) e
//! representa exatamente uma operação registrada no sidecar.
//!
//! Filosofia:
//!   - **Não destrutivo na sessão**: o `.sicroimage` lista os ajustes;
//!     o original em disco nunca é tocado.
//!   - **Destrutivo no derivado**: ao exportar, o pipeline aplica as
//!     operações em ordem e grava um arquivo novo em `imagens/exports/`.
//!   - **Pure-rust**: só `image` crate. Nada de OpenCV.

use image::{imageops, Rgba, RgbaImage};

use crate::models::{BackendAdjustments, BackendOperation};

// ---------------------------------------------------------------------------
// Visual adjustments

/// Apply all visual adjustments (brightness, contrast, gamma, saturation,
/// grayscale, invert) to an RGBA image in place. Order is deterministic:
/// gamma → brightness → contrast → saturation → grayscale → invert.
pub fn apply_adjustments(img: &mut RgbaImage, adj: &BackendAdjustments) {
    let gamma = if adj.gamma > 0.0 { adj.gamma } else { 1.0 };
    let brightness = adj.brightness.clamp(-100.0, 100.0) * 2.55; // ±255
    let contrast = (adj.contrast.clamp(-100.0, 100.0) + 100.0) / 100.0; // 0..2
    let saturation = (adj.saturation.clamp(-100.0, 100.0) + 100.0) / 100.0; // 0..2

    // W14.2 — Matiz: matriz hueRotate do SVG/CSS (preserva luminância),
    // pré-computada fora do loop. Idêntica ao `hue-rotate()` do preview.
    let hue_active = (adj.hue % 360.0).abs() > 1e-4;
    let hue_m: [[f32; 3]; 3] = if hue_active {
        let a = adj.hue.to_radians();
        let c = a.cos();
        let s = a.sin();
        [
            [
                0.213 + 0.787 * c - 0.213 * s,
                0.715 - 0.715 * c - 0.715 * s,
                0.072 - 0.072 * c + 0.928 * s,
            ],
            [
                0.213 - 0.213 * c + 0.143 * s,
                0.715 + 0.285 * c + 0.140 * s,
                0.072 - 0.072 * c - 0.283 * s,
            ],
            [
                0.213 - 0.213 * c - 0.787 * s,
                0.715 - 0.715 * c + 0.715 * s,
                0.072 + 0.928 * c + 0.072 * s,
            ],
        ]
    } else {
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    };
    // W14.2 — visibilidade de canal: canal desligado → 0 na saída.
    let kr = if adj.channel_r { 1.0 } else { 0.0 };
    let kg = if adj.channel_g { 1.0 } else { 0.0 };
    let kb = if adj.channel_b { 1.0 } else { 0.0 };

    for pixel in img.pixels_mut() {
        let Rgba([r0, g0, b0, a]) = *pixel;

        // Convert to linear [0,1]
        let mut r = r0 as f32 / 255.0;
        let mut g = g0 as f32 / 255.0;
        let mut b = b0 as f32 / 255.0;

        // Gamma
        if (gamma - 1.0).abs() > 1e-4 {
            r = r.powf(1.0 / gamma);
            g = g.powf(1.0 / gamma);
            b = b.powf(1.0 / gamma);
        }

        // Brightness (additive in 0..255 space)
        let mut rr = (r * 255.0) + brightness;
        let mut gg = (g * 255.0) + brightness;
        let mut bb = (b * 255.0) + brightness;

        // Contrast around mid-gray 128.
        if (contrast - 1.0).abs() > 1e-4 {
            rr = ((rr - 128.0) * contrast) + 128.0;
            gg = ((gg - 128.0) * contrast) + 128.0;
            bb = ((bb - 128.0) * contrast) + 128.0;
        }

        // Saturation — mix with luminance.
        if (saturation - 1.0).abs() > 1e-4 {
            let lum = 0.299 * rr + 0.587 * gg + 0.114 * bb;
            rr = lum + (rr - lum) * saturation;
            gg = lum + (gg - lum) * saturation;
            bb = lum + (bb - lum) * saturation;
        }

        // W14.2 — Hue (rotação preservando luminância; igual ao CSS).
        if hue_active {
            let nr = hue_m[0][0] * rr + hue_m[0][1] * gg + hue_m[0][2] * bb;
            let ng = hue_m[1][0] * rr + hue_m[1][1] * gg + hue_m[1][2] * bb;
            let nb = hue_m[2][0] * rr + hue_m[2][1] * gg + hue_m[2][2] * bb;
            rr = nr;
            gg = ng;
            bb = nb;
        }

        // W14.2 — visibilidade de canal (no-op quando todos ligados → k=1).
        rr *= kr;
        gg *= kg;
        bb *= kb;

        // Grayscale (replaces RGB with luminance).
        if adj.grayscale {
            let lum = 0.299 * rr + 0.587 * gg + 0.114 * bb;
            rr = lum;
            gg = lum;
            bb = lum;
        }

        // Invert (after grayscale so both compose).
        if adj.invert {
            rr = 255.0 - rr;
            gg = 255.0 - gg;
            bb = 255.0 - bb;
        }

        *pixel = Rgba([
            rr.clamp(0.0, 255.0) as u8,
            gg.clamp(0.0, 255.0) as u8,
            bb.clamp(0.0, 255.0) as u8,
            a,
        ]);
    }
}

// ---------------------------------------------------------------------------
// Geometric operations

/// Apply one geometric operation, returning a new image (some operations
/// can't be done in place because they change dimensions).
///
/// G12 — agora também despacha para os filtros forenses adicionados ao
/// enum `BackendOperation`. Filtros pesados (CLAHE, Bilateral, Median
/// com raio grande) podem demorar em imagens grandes; o caller deve
/// rodar fora do thread principal Tauri.
pub fn apply_operation(img: RgbaImage, op: &BackendOperation) -> RgbaImage {
    use super::filters;
    match op {
        // -- Geometric (MVP 7)
        BackendOperation::Rotate90Cw => imageops::rotate90(&img),
        BackendOperation::Rotate90Ccw => imageops::rotate270(&img),
        BackendOperation::Rotate180 => imageops::rotate180(&img),
        BackendOperation::FlipHorizontal => imageops::flip_horizontal(&img),
        BackendOperation::FlipVertical => imageops::flip_vertical(&img),
        BackendOperation::Crop {
            x,
            y,
            width,
            height,
        } => crop_safe(&img, *x, *y, *width, *height),
        BackendOperation::Resize { width, height } => imageops::resize(
            &img,
            (*width).max(1),
            (*height).max(1),
            imageops::FilterType::Lanczos3,
        ),

        // -- G12.1 Edge detection
        BackendOperation::EdgeSobel { strength } => {
            filters::edges::sobel(&img, *strength)
        }
        BackendOperation::EdgeLaplacian { strength } => {
            filters::edges::laplacian(&img, *strength)
        }
        BackendOperation::EdgeCanny {
            low_threshold,
            high_threshold,
        } => filters::edges::canny(&img, *low_threshold, *high_threshold),

        // -- G12.2 Blur/denoise
        BackendOperation::BlurGaussian { sigma } => {
            filters::blur::gaussian(&img, *sigma)
        }
        BackendOperation::BlurMedian { radius } => {
            filters::blur::median(&img, *radius)
        }
        BackendOperation::BlurBilateral {
            sigma_space,
            sigma_color,
        } => filters::blur::bilateral(&img, *sigma_space, *sigma_color),

        // -- G12.3 Enhancement
        BackendOperation::Clahe {
            tile_size,
            clip_limit,
            bins,
        } => filters::enhancement::clahe(&img, *tile_size, *clip_limit, *bins),
        BackendOperation::SubtractBackground {
            radius,
            light_background,
            disable_smoothing,
        } => filters::enhancement::subtract_background(
            &img,
            *radius,
            *light_background,
            *disable_smoothing,
        ),
        BackendOperation::HistogramEqualize => {
            filters::enhancement::histogram_equalize(&img)
        }
        BackendOperation::AutoLevels {
            percentile_low,
            percentile_high,
        } => filters::enhancement::auto_levels(&img, *percentile_low, *percentile_high),
        BackendOperation::WhiteBalanceGrayWorld => {
            filters::enhancement::white_balance_gray_world(&img)
        }

        // -- G12.4 Morphology
        BackendOperation::Dilate { radius } => {
            filters::morphology::dilate(&img, *radius)
        }
        BackendOperation::Erode { radius } => {
            filters::morphology::erode(&img, *radius)
        }
        BackendOperation::Open { radius } => filters::morphology::open(&img, *radius),
        BackendOperation::Close { radius } => filters::morphology::close(&img, *radius),

        // -- G12.6 Perspective
        BackendOperation::Perspective {
            src,
            dst,
            output_width,
            output_height,
        } => filters::geometric::perspective_correct(
            &img,
            src,
            dst,
            *output_width,
            *output_height,
        ),

        // -- G12 Extras
        BackendOperation::UnsharpMask { sigma, amount } => {
            filters::misc::unsharp_mask(&img, *sigma, *amount)
        }
        BackendOperation::Threshold { value } => filters::misc::threshold(&img, *value),
        BackendOperation::Pixelize {
            x,
            y,
            width,
            height,
            block_size,
        } => filters::misc::pixelize_region(&img, *x, *y, *width, *height, *block_size),

        // -- W12 (GIMP-parity) — Tonais
        BackendOperation::Levels {
            channel,
            in_black,
            in_white,
            gamma,
            out_black,
            out_white,
        } => filters::tone::levels(
            &img,
            filters::tone::ToneChannel::from_str(channel),
            *in_black,
            *in_white,
            *gamma,
            *out_black,
            *out_white,
        ),
        BackendOperation::Curves { channel, points } => {
            let pts: Vec<(f32, f32)> = points.iter().map(|p| (p[0], p[1])).collect();
            filters::tone::curves(&img, filters::tone::ToneChannel::from_str(channel), &pts)
        }
        BackendOperation::Posterize { levels } => filters::tone::posterize(&img, *levels),

        // -- W12 — Canais / falsa-cor
        BackendOperation::ExtractChannel { channel } => {
            filters::channels::extract_channel(&img, channel)
        }
        BackendOperation::FalseColor { colormap } => {
            filters::channels::false_color(&img, colormap)
        }

        // -- W12 — Forense por comparação / cor
        BackendOperation::Ela { quality, scale } => filters::compare::ela(&img, *quality, *scale),
        BackendOperation::DifferenceOfGaussians {
            sigma1,
            sigma2,
            gain,
        } => filters::edges::difference_of_gaussians(&img, *sigma1, *sigma2, *gain),
        BackendOperation::LuminanceGradient { strength } => {
            filters::edges::luminance_gradient(&img, *strength)
        }
        BackendOperation::DecorrelationStretch {
            target_sigma,
            target_mean,
        } => filters::decorrelation::decorrelation_stretch(&img, *target_sigma, *target_mean),

        // -- W12 — Geométrica / genérica
        BackendOperation::RotateArbitrary { degrees, expand } => {
            filters::geometric::rotate_arbitrary(&img, *degrees, *expand)
        }
        BackendOperation::Convolve {
            kernel,
            size,
            divisor,
            offset,
        } => filters::convolution::convolve(&img, kernel, *size, *divisor, *offset),

        // -- W20 (S2) — Operação confinada à seleção (estilo Photoshop).
        // Aplica o `op` interno numa cópia e compõe SÓ dentro da `mask`;
        // fora dela preserva o pixel original. Recursivo (o op interno passa
        // pelo mesmo dispatch).
        BackendOperation::Masked { op, mask } => {
            let base = img.clone();
            let filtered = apply_operation(img, op);
            super::mask::composite_with_mask(&base, &filtered, mask)
        }
    }
}

fn crop_safe(img: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
    let iw = img.width();
    let ih = img.height();
    let x = x.min(iw.saturating_sub(1));
    let y = y.min(ih.saturating_sub(1));
    let w = w.min(iw - x).max(1);
    let h = h.min(ih - y).max(1);
    imageops::crop_imm(img, x, y, w, h).to_image()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_image() -> RgbaImage {
        // 2x2 mosaico: vermelho, verde, azul, branco.
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([0, 255, 0, 255]));
        img.put_pixel(0, 1, Rgba([0, 0, 255, 255]));
        img.put_pixel(1, 1, Rgba([255, 255, 255, 255]));
        img
    }

    #[test]
    fn invert_flips_channels() {
        let mut img = sample_image();
        apply_adjustments(
            &mut img,
            &BackendAdjustments {
                invert: true,
                ..Default::default()
            },
        );
        // Vermelho 255,0,0 → 0,255,255 (ciano).
        assert_eq!(img.get_pixel(0, 0).0[0], 0);
        assert_eq!(img.get_pixel(0, 0).0[1], 255);
        assert_eq!(img.get_pixel(0, 0).0[2], 255);
        // Branco → preto.
        let bw = img.get_pixel(1, 1).0;
        assert_eq!(bw[0], 0);
        assert_eq!(bw[1], 0);
        assert_eq!(bw[2], 0);
    }

    #[test]
    fn grayscale_equals_channels() {
        let mut img = sample_image();
        apply_adjustments(
            &mut img,
            &BackendAdjustments {
                grayscale: true,
                ..Default::default()
            },
        );
        for p in img.pixels() {
            assert_eq!(p.0[0], p.0[1]);
            assert_eq!(p.0[1], p.0[2]);
        }
    }

    #[test]
    fn channel_isolate_zeros_disabled_channels() {
        // W14.2 — só o canal azul visível (GIMP-style): R e G zeram.
        let mut img = RgbaImage::from_pixel(1, 1, Rgba([200, 150, 100, 255]));
        apply_adjustments(
            &mut img,
            &BackendAdjustments {
                channel_r: false,
                channel_g: false,
                channel_b: true,
                ..Default::default()
            },
        );
        let p = img.get_pixel(0, 0).0;
        assert_eq!(p[0], 0, "R deve zerar");
        assert_eq!(p[1], 0, "G deve zerar");
        assert_eq!(p[2], 100, "B preservado");
        assert_eq!(p[3], 255, "alfa intacto");
    }

    #[test]
    fn hue_zero_is_identity() {
        let mut img = sample_image();
        let orig = sample_image();
        apply_adjustments(
            &mut img,
            &BackendAdjustments {
                hue: 0.0,
                ..Default::default()
            },
        );
        for (a, b) in img.pixels().zip(orig.pixels()) {
            assert_eq!(a.0, b.0, "hue=0 não deve alterar nada");
        }
    }

    #[test]
    fn hue_preserves_gray() {
        // Matriz hueRotate (linhas somam 1.0) → cinza permanece cinza em
        // qualquer ângulo.
        let mut img = RgbaImage::from_pixel(1, 1, Rgba([128, 128, 128, 255]));
        apply_adjustments(
            &mut img,
            &BackendAdjustments {
                hue: 120.0,
                ..Default::default()
            },
        );
        let p = img.get_pixel(0, 0).0;
        assert!((p[0] as i32 - 128).abs() <= 1);
        assert!((p[1] as i32 - 128).abs() <= 1);
        assert!((p[2] as i32 - 128).abs() <= 1);
    }

    #[test]
    fn brightness_positive_lifts_dark_pixels() {
        let mut img = RgbaImage::from_pixel(1, 1, Rgba([20, 20, 20, 255]));
        apply_adjustments(
            &mut img,
            &BackendAdjustments {
                brightness: 50.0,
                ..Default::default()
            },
        );
        let after = img.get_pixel(0, 0).0[0];
        assert!(after > 20, "brightness +50 deveria aumentar luminância");
    }

    #[test]
    fn rotate90cw_swaps_dimensions() {
        let img = sample_image();
        let rotated = apply_operation(img, &BackendOperation::Rotate90Cw);
        assert_eq!((rotated.width(), rotated.height()), (2, 2)); // quadrada
        // Pixel (0,0) original (vermelho) deve ir para (1,0) após 90° CW.
        assert_eq!(rotated.get_pixel(1, 0).0, [255, 0, 0, 255]);
    }

    #[test]
    fn flip_horizontal_mirrors_columns() {
        let img = sample_image();
        let flipped = apply_operation(img, &BackendOperation::FlipHorizontal);
        assert_eq!(flipped.get_pixel(0, 0).0, [0, 255, 0, 255]);
        assert_eq!(flipped.get_pixel(1, 0).0, [255, 0, 0, 255]);
    }

    #[test]
    fn crop_returns_subregion() {
        let img = sample_image();
        let cropped = apply_operation(
            img,
            &BackendOperation::Crop {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
        );
        assert_eq!((cropped.width(), cropped.height()), (1, 1));
        assert_eq!(cropped.get_pixel(0, 0).0, [255, 0, 0, 255]);
    }

    #[test]
    fn crop_clamps_oversize_request() {
        let img = sample_image();
        let cropped = apply_operation(
            img,
            &BackendOperation::Crop {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            },
        );
        // Não estoura: clamp ao tamanho real.
        assert_eq!((cropped.width(), cropped.height()), (2, 2));
    }

    #[test]
    fn resize_changes_dimensions() {
        let img = sample_image();
        let resized = apply_operation(
            img,
            &BackendOperation::Resize {
                width: 4,
                height: 4,
            },
        );
        assert_eq!((resized.width(), resized.height()), (4, 4));
    }

    /// Imagem com gradiente diagonal — dá conteúdo para os filtros agirem.
    fn gradient_image(w: u32, h: u32) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = (((x + y) * 255) / (w + h).max(1)) as u8;
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }
        img
    }

    /// Espelha a sequência que o preview ao vivo monta: um `resize` (downscale)
    /// seguido dos filtros, aplicados EM ORDEM pelo mesmo `apply_operation`.
    /// Garante que (1) o downscale dita as dimensões finais e (2) os filtros
    /// realmente alteram os pixels (preview ≠ simples redução). Regressão do
    /// bug "filtros não dão resultado".
    #[test]
    fn preview_like_stack_resizes_then_applies_filters() {
        let original = gradient_image(64, 64);

        // Só o downscale, sem filtros — referência.
        let resized_only = apply_operation(
            original.clone(),
            &BackendOperation::Resize {
                width: 32,
                height: 32,
            },
        );

        // Pilha do preview: resize → Sobel → limiar.
        let mut img = apply_operation(
            original,
            &BackendOperation::Resize {
                width: 32,
                height: 32,
            },
        );
        img = apply_operation(img, &BackendOperation::EdgeSobel { strength: 1.0 });
        img = apply_operation(img, &BackendOperation::Threshold { value: 128 });

        // (1) o tamanho é o do downscale.
        assert_eq!((img.width(), img.height()), (32, 32));

        // (2) os filtros mudaram a imagem em relação ao só-resize.
        let changed = img
            .pixels()
            .zip(resized_only.pixels())
            .any(|(a, b)| a.0 != b.0);
        assert!(
            changed,
            "a pilha de filtros do preview deveria alterar os pixels"
        );
    }

    /// W20 (S2) — o op `masked` (wrapper) precisa (1) desserializar do JSON do
    /// front com o op interno aninhado + a máscara normalizada e (2) ao aplicar,
    /// alterar SÓ a região da máscara (fora dela o pixel original é preservado).
    /// Trava tanto o enum aninhado (serde internally-tagged) quanto o composite.
    #[test]
    fn masked_op_deserializes_and_composites_only_inside() {
        let json = r#"{
            "kind":"masked",
            "op":{"kind":"threshold","value":128},
            "mask":{"shape":"rect","x":0.0,"y":0.0,"width":0.5,"height":1.0,"inverted":false}
        }"#;
        let op: BackendOperation = serde_json::from_str(json).unwrap();
        assert!(matches!(op, BackendOperation::Masked { .. }));

        // Imagem cinza uniforme — o threshold a binariza (muda os pixels).
        let img = RgbaImage::from_pixel(4, 4, Rgba([200, 200, 200, 255]));
        let out = apply_operation(img, &op);
        assert_eq!((out.width(), out.height()), (4, 4), "dimensão preservada");
        // Metade esquerda (col 0/1, centros < 0.5) → filtrada (≠ original).
        assert_ne!(
            out.get_pixel(0, 0).0,
            [200, 200, 200, 255],
            "dentro da máscara deve ser filtrado"
        );
        // Metade direita (col 3, centro 0.875 > 0.5) → original intacto.
        assert_eq!(
            out.get_pixel(3, 0).0,
            [200, 200, 200, 255],
            "fora da máscara deve preservar o original"
        );
    }

    /// Regressão: as operações de rotação fixa têm dígitos no nome
    /// (`Rotate90Cw`), e o front manda `rotate_90_cw`. Os testes que
    /// constroem a variante direto NÃO exercitam a tag serde — então a
    /// rotação fixa "não funcionava" (op não desserializava) enquanto o
    /// resto funcionava. Trava a tag exata que o front envia.
    #[test]
    fn rotate_ops_deserialize_from_frontend_kind_strings() {
        let cw: BackendOperation =
            serde_json::from_str(r#"{"kind":"rotate_90_cw"}"#).unwrap();
        assert!(matches!(cw, BackendOperation::Rotate90Cw));
        let ccw: BackendOperation =
            serde_json::from_str(r#"{"kind":"rotate_90_ccw"}"#).unwrap();
        assert!(matches!(ccw, BackendOperation::Rotate90Ccw));
        let r180: BackendOperation =
            serde_json::from_str(r#"{"kind":"rotate_180"}"#).unwrap();
        assert!(matches!(r180, BackendOperation::Rotate180));
        // E continua emitindo a mesma tag (serialização na volta).
        let json = serde_json::to_string(&BackendOperation::Rotate90Cw).unwrap();
        assert!(json.contains("\"rotate_90_cw\""), "serializou como {json}");
    }
}
