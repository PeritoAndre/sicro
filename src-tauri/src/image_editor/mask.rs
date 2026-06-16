//! W20 (S2) — Máscara de seleção: confina uma operação a uma região.
//!
//! Filosofia (§13): SUPORTE e reprodutível. O filtro é aplicado à imagem
//! inteira numa cópia e o resultado é composto SÓ dentro da máscara; fora
//! dela o pixel original é preservado. A máscara chega em coordenadas
//! **normalizadas** `[0,1]` (ver `crate::models::MaskSpec`), então a MESMA
//! geometria rasteriza corretamente tanto no preview reduzido quanto no
//! export em resolução cheia. Borda DURA nesta fase (sem feather) — limite
//! honesto, documentado para o perito.
//!
//! As geometrias (rect/elipse/polígono) e o `inverted` vivem em
//! `MaskSpec::contains_base`; aqui ficam só o ray-casting e o composite, que
//! dependem do `image` crate (mantém `models` livre dessa dependência).

use image::RgbaImage;

use crate::models::MaskSpec;

/// Ray-casting em coordenadas (quaisquer — aqui normalizadas): o ponto
/// `(x, y)` está dentro do polígono `poly`? Polígono com < 3 vértices nunca
/// contém nada.
pub fn point_in_polygon(x: f32, y: f32, poly: &[[f32; 2]]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (poly[i][0], poly[i][1]);
        let (xj, yj) = (poly[j][0], poly[j][1]);
        // Aresta cruza a horizontal em y? E o cruzamento fica à direita de x?
        let intersects = (yi > y) != (yj > y)
            && x < (xj - xi) * (y - yi) / (yj - yi) + xi;
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Composita `filtered` sobre `base` apenas na região da `mask`. Para cada
/// pixel: dentro da seleção → `filtered`; fora → `base`. `inverted` troca
/// dentro/fora. Devolve uma nova imagem.
///
/// Se as dimensões de `filtered` divergirem de `base` (op interna mudou o
/// tamanho — ex.: crop/resize/rotação), não há como compor pixel-a-pixel;
/// devolve `filtered` como fallback seguro (o front evita esse caso só
/// oferecendo escopo "seleção" para filtros que preservam dimensão).
pub fn composite_with_mask(
    base: &RgbaImage,
    filtered: &RgbaImage,
    mask: &MaskSpec,
) -> RgbaImage {
    let (w, h) = (base.width(), base.height());
    if filtered.width() != w || filtered.height() != h {
        return filtered.clone();
    }
    if w == 0 || h == 0 {
        return base.clone();
    }
    let inv = mask.inverted();
    let mut out = base.clone();
    let wf = w as f32;
    let hf = h as f32;
    for y in 0..h {
        // Centro do pixel em coordenadas normalizadas [0,1].
        let ny = (y as f32 + 0.5) / hf;
        for x in 0..w {
            let nx = (x as f32 + 0.5) / wf;
            // `inside` = está na região onde o filtro vale (após inverter).
            let inside = mask.contains_base(nx, ny) != inv;
            if inside {
                out.put_pixel(x, y, *filtered.get_pixel(x, y));
            }
        }
    }
    out
}

/// W20 (S3) — Caixa delimitadora (px inteiros, clampada à imagem) que contém a
/// região da máscara. Para máscara INVERTIDA a região é "tudo menos a forma",
/// então a caixa é a imagem inteira.
pub fn mask_bbox_px(mask: &MaskSpec, w: u32, h: u32) -> (u32, u32, u32, u32) {
    if w == 0 || h == 0 {
        return (0, 0, w.max(1), h.max(1));
    }
    if mask.inverted() {
        return (0, 0, w, h);
    }
    let (nx0, ny0, nx1, ny1) = match mask {
        MaskSpec::Rect {
            x,
            y,
            width,
            height,
            ..
        }
        | MaskSpec::Ellipse {
            x,
            y,
            width,
            height,
            ..
        } => (*x, *y, *x + *width, *y + *height),
        MaskSpec::Polygon { points, .. } => {
            if points.is_empty() {
                return (0, 0, w, h);
            }
            let mut minx = f32::MAX;
            let mut miny = f32::MAX;
            let mut maxx = f32::MIN;
            let mut maxy = f32::MIN;
            for p in points {
                minx = minx.min(p[0]);
                miny = miny.min(p[1]);
                maxx = maxx.max(p[0]);
                maxy = maxy.max(p[1]);
            }
            (minx, miny, maxx, maxy)
        }
    };
    let wf = w as f32;
    let hf = h as f32;
    let x0 = (nx0 * wf).floor().clamp(0.0, wf - 1.0) as u32;
    let y0 = (ny0 * hf).floor().clamp(0.0, hf - 1.0) as u32;
    let x1 = (nx1 * wf).ceil().clamp(0.0, wf) as u32;
    let y1 = (ny1 * hf).ceil().clamp(0.0, hf) as u32;
    let bw = x1.saturating_sub(x0).max(1);
    let bh = y1.saturating_sub(y0).max(1);
    (x0, y0, bw, bh)
}

/// W20 (S3) — Recorta a região da máscara de `img` para uma nova imagem do
/// tamanho da bbox; pixels FORA da seleção ficam transparentes (alpha 0).
/// Devolve a imagem recortada + o offset `(x, y)` da bbox na imagem original
/// (para posicionar a camada de pixels). Coords da máscara normalizadas `[0,1]`.
pub fn crop_masked(img: &RgbaImage, mask: &MaskSpec) -> (RgbaImage, u32, u32) {
    let (w, h) = (img.width(), img.height());
    let (bx, by, bw, bh) = mask_bbox_px(mask, w, h);
    let mut out = RgbaImage::new(bw, bh); // transparente por padrão
    if w == 0 || h == 0 {
        return (out, bx, by);
    }
    let inv = mask.inverted();
    let wf = w as f32;
    let hf = h as f32;
    for oy in 0..bh {
        let fy = by + oy;
        if fy >= h {
            continue;
        }
        let ny = (fy as f32 + 0.5) / hf;
        for ox in 0..bw {
            let fx = bx + ox;
            if fx >= w {
                continue;
            }
            let nx = (fx as f32 + 0.5) / wf;
            let inside = mask.contains_base(nx, ny) != inv;
            if inside {
                out.put_pixel(ox, oy, *img.get_pixel(fx, fy));
            }
        }
    }
    (out, bx, by)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn solid(w: u32, h: u32, color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba(color))
    }

    #[test]
    fn point_in_polygon_basic_square() {
        // Quadrado unitário [0.2,0.8]².
        let sq = [[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]];
        assert!(point_in_polygon(0.5, 0.5, &sq), "centro dentro");
        assert!(!point_in_polygon(0.1, 0.5, &sq), "esquerda fora");
        assert!(!point_in_polygon(0.9, 0.5, &sq), "direita fora");
        assert!(!point_in_polygon(0.5, 0.9, &sq), "abaixo fora");
    }

    #[test]
    fn degenerate_polygon_contains_nothing() {
        let line = [[0.0, 0.0], [1.0, 1.0]];
        assert!(!point_in_polygon(0.5, 0.5, &line));
    }

    #[test]
    fn rect_mask_composites_only_inside() {
        // base preta, filtrado branco; máscara = metade esquerda (x 0..0.5).
        let base = solid(4, 4, [0, 0, 0, 255]);
        let filtered = solid(4, 4, [255, 255, 255, 255]);
        let mask = MaskSpec::Rect {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 1.0,
            inverted: false,
        };
        let out = composite_with_mask(&base, &filtered, &mask);
        // Coluna 0 e 1 (centros 0.125, 0.375 < 0.5) → branco.
        assert_eq!(out.get_pixel(0, 0).0, [255, 255, 255, 255]);
        assert_eq!(out.get_pixel(1, 2).0, [255, 255, 255, 255]);
        // Coluna 2 e 3 (centros 0.625, 0.875 > 0.5) → preto (base).
        assert_eq!(out.get_pixel(2, 0).0, [0, 0, 0, 255]);
        assert_eq!(out.get_pixel(3, 3).0, [0, 0, 0, 255]);
    }

    #[test]
    fn inverted_rect_mask_flips_region() {
        let base = solid(4, 4, [0, 0, 0, 255]);
        let filtered = solid(4, 4, [255, 255, 255, 255]);
        let mask = MaskSpec::Rect {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 1.0,
            inverted: true,
        };
        let out = composite_with_mask(&base, &filtered, &mask);
        // Agora a metade esquerda fica com a base (preto) e a direita filtrada.
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 0, 255]);
        assert_eq!(out.get_pixel(3, 3).0, [255, 255, 255, 255]);
    }

    #[test]
    fn ellipse_mask_center_inside_corner_outside() {
        let base = solid(8, 8, [0, 0, 0, 255]);
        let filtered = solid(8, 8, [255, 255, 255, 255]);
        let mask = MaskSpec::Ellipse {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            inverted: false,
        };
        let out = composite_with_mask(&base, &filtered, &mask);
        // Centro (3,3)/(4,4) dentro da elipse → branco.
        assert_eq!(out.get_pixel(4, 4).0, [255, 255, 255, 255]);
        // Canto (0,0) fora da elipse inscrita → base preta.
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 0, 255]);
    }

    #[test]
    fn crop_masked_rect_returns_bbox_and_opaque_inside() {
        // imagem 10x10 vermelha; máscara rect cobrindo metade esquerda-cima.
        let img = solid(10, 10, [255, 0, 0, 255]);
        let mask = MaskSpec::Rect {
            x: 0.0,
            y: 0.0,
            width: 0.5,
            height: 0.4,
            inverted: false,
        };
        let (crop, x, y) = crop_masked(&img, &mask);
        assert_eq!((x, y), (0, 0));
        assert_eq!((crop.width(), crop.height()), (5, 4));
        // dentro: vermelho opaco.
        assert_eq!(crop.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(crop.get_pixel(4, 3).0, [255, 0, 0, 255]);
    }

    #[test]
    fn crop_masked_polygon_cuts_outside_to_transparent() {
        // triângulo grande; canto da bbox deve ficar transparente (alpha 0).
        let img = solid(20, 20, [0, 200, 0, 255]);
        let mask = MaskSpec::Polygon {
            points: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            inverted: false,
        };
        let (crop, _x, _y) = crop_masked(&img, &mask);
        assert_eq!((crop.width(), crop.height()), (20, 20));
        // canto inferior-direito está fora do triângulo → transparente.
        assert_eq!(crop.get_pixel(19, 19).0[3], 0, "fora do polígono = alpha 0");
        // canto superior-esquerdo dentro → opaco.
        assert_eq!(crop.get_pixel(0, 0).0[3], 255, "dentro = opaco");
    }

    #[test]
    fn mask_bbox_inverted_is_full_image() {
        let mask = MaskSpec::Rect {
            x: 0.25,
            y: 0.25,
            width: 0.5,
            height: 0.5,
            inverted: true,
        };
        assert_eq!(mask_bbox_px(&mask, 40, 30), (0, 0, 40, 30));
    }

    #[test]
    fn dimension_mismatch_returns_filtered() {
        let base = solid(4, 4, [0, 0, 0, 255]);
        let filtered = solid(2, 2, [255, 255, 255, 255]); // crop simulado
        let mask = MaskSpec::Rect {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            inverted: false,
        };
        let out = composite_with_mask(&base, &filtered, &mask);
        assert_eq!((out.width(), out.height()), (2, 2));
    }
}
