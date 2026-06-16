//! Documentoscopia — Fases 3 (layout) e 4 (pré-processamento).
//!
//! §13 (apoio determinístico): este módulo NÃO interpreta autenticidade. Ele só
//! (a) **pré-processa** a imagem para facilitar a leitura — sempre sobre cópias
//! derivadas, nunca o original — e (b) **detecta elementos verificáveis**:
//! QR/códigos de barras (decodificando o conteúdo, que o perito pode conferir)
//! e *candidatos* a tabela (por linhas). Tudo é reproduzível.

use image::{Rgba, RgbaImage};

use crate::image_editor::filters::{enhancement, misc};

#[inline]
fn lum(p: &Rgba<u8>) -> f32 {
    0.299 * p.0[0] as f32 + 0.587 * p.0[1] as f32 + 0.114 * p.0[2] as f32
}

// ===========================================================================
// Fase 4 — Pré-processamento
// ===========================================================================

/// Operações de realce/normalização, aplicadas na ordem fornecida.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreOp {
    Grayscale,
    Deskew,
    Clahe,
    AutoLevels,
    Binarize,
    Invert,
}

impl PreOp {
    pub fn from_id(s: &str) -> Option<PreOp> {
        match s.trim().to_ascii_lowercase().as_str() {
            "grayscale" | "cinza" => Some(PreOp::Grayscale),
            "deskew" | "endireitar" => Some(PreOp::Deskew),
            "clahe" | "contraste" => Some(PreOp::Clahe),
            "autolevels" | "auto_levels" | "niveis" => Some(PreOp::AutoLevels),
            "binarize" | "binarizar" | "otsu" => Some(PreOp::Binarize),
            "invert" | "inverter" => Some(PreOp::Invert),
            _ => None,
        }
    }
}

/// Converte ids vindos do frontend em operações conhecidas (ignora desconhecidas).
pub fn parse_ops(ids: &[String]) -> Vec<PreOp> {
    ids.iter().filter_map(|s| PreOp::from_id(s)).collect()
}

/// Aplica as operações em sequência e devolve a imagem processada.
pub fn apply_ops(img: &RgbaImage, ops: &[PreOp]) -> RgbaImage {
    let mut cur = img.clone();
    for op in ops {
        cur = match op {
            PreOp::Grayscale => grayscale(&cur),
            PreOp::Deskew => deskew(&cur),
            PreOp::Clahe => enhancement::clahe(&cur, 64, 2.5, 256),
            PreOp::AutoLevels => enhancement::auto_levels(&cur, 1.0, 99.0),
            PreOp::Binarize => binarize_otsu(&cur),
            PreOp::Invert => invert(&cur),
        };
    }
    cur
}

fn grayscale(img: &RgbaImage) -> RgbaImage {
    let mut out = RgbaImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        let v = lum(p).round().clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, Rgba([v, v, v, p.0[3]]));
    }
    out
}

fn invert(img: &RgbaImage) -> RgbaImage {
    let mut out = RgbaImage::new(img.width(), img.height());
    for (x, y, p) in img.enumerate_pixels() {
        out.put_pixel(
            x,
            y,
            Rgba([255 - p.0[0], 255 - p.0[1], 255 - p.0[2], p.0[3]]),
        );
    }
    out
}

/// Limiar de Otsu sobre o histograma de luminância (maximiza a variância
/// entre classes). Determinístico.
pub fn otsu_threshold(img: &RgbaImage) -> u8 {
    let mut hist = [0u32; 256];
    for p in img.pixels() {
        hist[lum(p).clamp(0.0, 255.0) as usize] += 1;
    }
    let total: u32 = hist.iter().sum();
    if total == 0 {
        return 128;
    }
    let sum_total: f64 = (0..256).map(|i| i as f64 * hist[i] as f64).sum();
    let mut sum_b = 0.0f64;
    let mut w_b = 0u32;
    let mut max_var = -1.0f64;
    let mut thresh = 128u8;
    for i in 0..256 {
        w_b += hist[i];
        if w_b == 0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0 {
            break;
        }
        sum_b += i as f64 * hist[i] as f64;
        let m_b = sum_b / w_b as f64;
        let m_f = (sum_total - sum_b) / w_f as f64;
        let var_between = w_b as f64 * w_f as f64 * (m_b - m_f) * (m_b - m_f);
        if var_between > max_var {
            max_var = var_between;
            thresh = i as u8;
        }
    }
    thresh
}

fn binarize_otsu(img: &RgbaImage) -> RgbaImage {
    let t = otsu_threshold(img);
    misc::threshold(img, t)
}

// --- Deskew (endireitar) ---

/// Estima a inclinação (graus; + = horário) por perfil de projeção: para cada
/// ângulo candidato, projeta a tinta de uma miniatura binarizada nas linhas e
/// mede a "nitidez" (Σ linha²). O ângulo que deixa as linhas mais nítidas é a
/// inclinação. Faixa ±10° em passos de 0,25°.
pub fn estimate_skew_deg(img: &RgbaImage) -> f32 {
    let target_w = 900u32;
    let scale = if img.width() > target_w {
        target_w as f32 / img.width() as f32
    } else {
        1.0
    };
    let tw = ((img.width() as f32) * scale).round().max(1.0) as u32;
    let th = ((img.height() as f32) * scale).round().max(1.0) as u32;
    let small = image::imageops::resize(img, tw, th, image::imageops::FilterType::Triangle);
    let t = otsu_threshold(&small) as f32;

    // Coordenadas (x,y) dos pixels de tinta (escuros).
    let mut ink: Vec<(f32, f32)> = Vec::new();
    for (x, y, p) in small.enumerate_pixels() {
        if lum(p) < t {
            ink.push((x as f32, y as f32));
        }
    }
    if ink.len() < 50 {
        return 0.0;
    }
    let cx = tw as f32 / 2.0;
    let cy = th as f32 / 2.0;

    let mut best_angle = 0.0f32;
    let mut best_score = -1.0f64;
    let mut a = -10.0f32;
    while a <= 10.0001 {
        let rad = (-a).to_radians(); // rotaciona -a (endireitar)
        let (s, c) = (rad.sin(), rad.cos());
        let mut rows = vec![0u32; th as usize + 2];
        for &(x, y) in &ink {
            let yy = cy + (x - cx) * s + (y - cy) * c;
            let r = yy.round();
            if r >= 0.0 && (r as u32) < th {
                rows[r as usize] += 1;
            }
        }
        let score: f64 = rows.iter().map(|&v| v as f64 * v as f64).sum();
        if score > best_score {
            best_score = score;
            best_angle = a;
        }
        a += 0.25;
    }
    best_angle
}

/// Endireita o documento: rotaciona pelo negativo da inclinação estimada.
pub fn deskew(img: &RgbaImage) -> RgbaImage {
    let a = estimate_skew_deg(img);
    if a.abs() < 0.1 {
        return img.clone();
    }
    rotate(img, -a)
}

/// Rotaciona `deg` graus (horário) ao redor do centro, expandindo a tela para
/// caber tudo; fundo branco; amostragem bilinear.
pub fn rotate(img: &RgbaImage, deg: f32) -> RgbaImage {
    let rad = deg.to_radians();
    let (s, c) = (rad.sin(), rad.cos());
    let w = img.width() as f32;
    let h = img.height() as f32;
    let nw = (w * c.abs() + h * s.abs()).ceil().max(1.0);
    let nh = (w * s.abs() + h * c.abs()).ceil().max(1.0);
    let out_w = nw as u32;
    let out_h = nh as u32;
    let mut out = RgbaImage::from_pixel(out_w, out_h, Rgba([255, 255, 255, 255]));
    let scx = w / 2.0;
    let scy = h / 2.0;
    let ocx = nw / 2.0;
    let ocy = nh / 2.0;
    for oy in 0..out_h {
        for ox in 0..out_w {
            // mapeia saída → origem (rotação inversa)
            let dx = ox as f32 - ocx;
            let dy = oy as f32 - ocy;
            let sx = scx + dx * c + dy * s;
            let sy = scy - dx * s + dy * c;
            if let Some(px) = sample_bilinear(img, sx, sy) {
                out.put_pixel(ox, oy, px);
            }
        }
    }
    out
}

fn sample_bilinear(img: &RgbaImage, x: f32, y: f32) -> Option<Rgba<u8>> {
    let w = img.width();
    let h = img.height();
    if x < 0.0 || y < 0.0 || x > (w - 1) as f32 || y > (h - 1) as f32 {
        return None;
    }
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let p00 = img.get_pixel(x0, y0).0;
    let p10 = img.get_pixel(x1, y0).0;
    let p01 = img.get_pixel(x0, y1).0;
    let p11 = img.get_pixel(x1, y1).0;
    let mut o = [0u8; 4];
    for ch in 0..4 {
        let v = (1.0 - fx) * (1.0 - fy) * p00[ch] as f32
            + fx * (1.0 - fy) * p10[ch] as f32
            + (1.0 - fx) * fy * p01[ch] as f32
            + fx * fy * p11[ch] as f32;
        o[ch] = v.round().clamp(0.0, 255.0) as u8;
    }
    Some(Rgba(o))
}

// ===========================================================================
// Fase 3 — Detecção de layout (determinística)
// ===========================================================================

/// Região detectada automaticamente (bbox normalizado 0..1).
#[derive(Debug, Clone)]
pub struct DetectedRegion {
    pub region_type: String,
    pub label: String,
    pub bbox: [f64; 4],
    pub confidence: Option<f64>,
}

/// Amplia a imagem até a maior dimensão atingir `min_long` (ajuda QR/1D fracos).
fn upscale_min(img: &RgbaImage, min_long: u32) -> RgbaImage {
    let long = img.width().max(img.height());
    if long == 0 || long >= min_long {
        return img.clone();
    }
    let scale = min_long as f32 / long as f32;
    let w = ((img.width() as f32) * scale).round().max(1.0) as u32;
    let h = ((img.height() as f32) * scale).round().max(1.0) as u32;
    image::imageops::resize(img, w, h, image::imageops::FilterType::Lanczos3)
}

/// Decodifica o primeiro QR/código de barras achado → (region_type, "KIND: texto").
fn decode_first(img: &RgbaImage) -> Option<(String, String)> {
    if img.width() == 0 || img.height() == 0 {
        return None;
    }
    let luma: Vec<u8> = img.pixels().map(|p| lum(p).clamp(0.0, 255.0) as u8).collect();
    let results = rxing::helpers::detect_multiple_in_luma(luma, img.width(), img.height())
        .unwrap_or_default();
    let r = results.first()?;
    let fmt = r.getBarcodeFormat();
    let is_qr = matches!(fmt, rxing::BarcodeFormat::QR_CODE);
    let rt = if is_qr { "qrcode" } else { "barcode" }.to_string();
    let kind = if is_qr { "QR".to_string() } else { format!("{fmt:?}") };
    Some((rt, format!("{kind}: {}", r.getText())))
}

/// Decodifica um QR/código de barras num RECORTE isolado — bem mais robusto que
/// na página inteira (amplia e realça o recorte). Devolve (region_type, rótulo).
pub fn decode_crop(crop: &RgbaImage) -> Option<(String, String)> {
    decode_first(crop).or_else(|| {
        let up = upscale_min(crop, 1400);
        decode_first(&up)
            .or_else(|| decode_first(&enhancement::auto_levels(&up, 1.0, 99.0)))
    })
}

/// Detecta e **decodifica** QR/códigos de barras (`rxing`/ZXing). Tenta a página
/// como está e, se nada decodificar, repete numa versão ampliada/realçada — o
/// que costuma resgatar códigos de fotos/escaneados de baixa qualidade.
pub fn detect_codes(img: &RgbaImage) -> Vec<DetectedRegion> {
    let out = detect_codes_on(img);
    if !out.is_empty() {
        return out;
    }
    let enhanced = enhancement::auto_levels(&upscale_min(img, 1800), 1.0, 99.0);
    detect_codes_on(&enhanced)
}

fn detect_codes_on(img: &RgbaImage) -> Vec<DetectedRegion> {
    let w = img.width();
    let h = img.height();
    if w == 0 || h == 0 {
        return vec![];
    }
    let luma: Vec<u8> = img.pixels().map(|p| lum(p).clamp(0.0, 255.0) as u8).collect();

    // `detect_multiple_in_luma` já habilita TryHarder por padrão (melhor em fotos).
    let results = rxing::helpers::detect_multiple_in_luma(luma, w, h).unwrap_or_default();

    let fw = w as f64;
    let fh = h as f64;
    let mut out = Vec::new();
    for r in &results {
        let pts = r.getPoints();
        if pts.is_empty() {
            continue;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for p in pts {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        // padding leve (8% da maior dimensão da caixa) para envolver o código.
        let pad = ((max_x - min_x).max(max_y - min_y) * 0.08).max(2.0);
        let x0 = ((min_x - pad) as f64 / fw).clamp(0.0, 1.0);
        let y0 = ((min_y - pad) as f64 / fh).clamp(0.0, 1.0);
        let x1 = ((max_x + pad) as f64 / fw).clamp(0.0, 1.0);
        let y1 = ((max_y + pad) as f64 / fh).clamp(0.0, 1.0);

        let fmt = r.getBarcodeFormat();
        let is_qr = matches!(fmt, rxing::BarcodeFormat::QR_CODE);
        let region_type = if is_qr { "qrcode" } else { "barcode" };
        let kind_label = if is_qr { "QR".to_string() } else { format!("{fmt:?}") };
        out.push(DetectedRegion {
            region_type: region_type.to_string(),
            label: format!("{kind_label}: {}", r.getText()),
            bbox: [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)],
            confidence: Some(1.0),
        });
    }
    out
}

/// Detector **conservador** de tabela por linhas: binariza, acha linhas
/// horizontais e verticais longas (≥35% da dimensão) e, se houver grade
/// suficiente (≥3 horizontais e ≥2 verticais), devolve UM candidato com o
/// bounding box das linhas. Confiança baixa — é só um indício a revisar.
pub fn detect_table_regions(img: &RgbaImage) -> Vec<DetectedRegion> {
    let w = img.width();
    let h = img.height();
    if w < 40 || h < 40 {
        return vec![];
    }
    // miniatura para acelerar
    let target_w = 1000u32;
    let scale = if w > target_w {
        target_w as f32 / w as f32
    } else {
        1.0
    };
    let tw = ((w as f32) * scale).round().max(1.0) as u32;
    let th = ((h as f32) * scale).round().max(1.0) as u32;
    let small = image::imageops::resize(img, tw, th, image::imageops::FilterType::Triangle);
    let t = otsu_threshold(&small) as f32;
    let dark = |x: u32, y: u32| lum(small.get_pixel(x, y)) < t;

    let min_h_run = (tw as f32 * 0.35) as u32;
    let min_v_run = (th as f32 * 0.35) as u32;

    // Linhas horizontais: maior sequência de pixels escuros por linha.
    let mut h_line = vec![false; th as usize];
    for y in 0..th {
        let mut run = 0u32;
        let mut best = 0u32;
        for x in 0..tw {
            if dark(x, y) {
                run += 1;
                best = best.max(run);
            } else {
                run = 0;
            }
        }
        h_line[y as usize] = best >= min_h_run;
    }
    // Linhas verticais.
    let mut v_line = vec![false; tw as usize];
    for x in 0..tw {
        let mut run = 0u32;
        let mut best = 0u32;
        for y in 0..th {
            if dark(x, y) {
                run += 1;
                best = best.max(run);
            } else {
                run = 0;
            }
        }
        v_line[x as usize] = best >= min_v_run;
    }

    let count_groups = |v: &[bool]| -> (usize, usize, usize) {
        // (nº de grupos, primeiro índice, último índice)
        let mut groups = 0usize;
        let mut first = usize::MAX;
        let mut last = 0usize;
        let mut prev = false;
        for (i, &b) in v.iter().enumerate() {
            if b {
                if !prev {
                    groups += 1;
                }
                first = first.min(i);
                last = last.max(i);
            }
            prev = b;
        }
        (groups, first, last)
    };
    let (hg, hy0, hy1) = count_groups(&h_line);
    let (vg, vx0, vx1) = count_groups(&v_line);

    if hg < 3 || vg < 2 {
        return vec![];
    }
    let x0 = vx0 as f64 / tw as f64;
    let y0 = hy0 as f64 / th as f64;
    let x1 = (vx1 + 1) as f64 / tw as f64;
    let y1 = (hy1 + 1) as f64 / th as f64;
    vec![DetectedRegion {
        region_type: "tabela".to_string(),
        label: format!("estrutura tabular (candidata: {hg} linhas × {vg} colunas)"),
        bbox: [x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0)],
        confidence: Some(0.3),
    }]
}

/// Roda todos os detectores determinísticos sobre a imagem.
pub fn detect_layout(img: &RgbaImage) -> Vec<DetectedRegion> {
    let mut out = detect_codes(img);
    out.extend(detect_table_regions(img));
    out
}

// ===========================================================================
// Testes
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, v: u8) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([v, v, v, 255]))
    }

    #[test]
    fn parse_ops_maps_known_ids() {
        let ops = parse_ops(&[
            "cinza".into(),
            "endireitar".into(),
            "otsu".into(),
            "desconhecida".into(),
        ]);
        assert_eq!(ops, vec![PreOp::Grayscale, PreOp::Deskew, PreOp::Binarize]);
    }

    #[test]
    fn otsu_splits_bimodal() {
        // metade escura (60) e metade clara (200) → limiar separa os dois grupos
        // (imagem não-degenerada; {0,255} teria platô de variância e pegaria 0).
        let mut img = solid(20, 20, 60);
        for (x, _y, p) in img.enumerate_pixels_mut() {
            if x >= 10 {
                *p = Rgba([200, 200, 200, 255]);
            }
        }
        let t = otsu_threshold(&img);
        assert!((55..=200).contains(&t), "otsu = {t}");
    }

    #[test]
    fn grayscale_and_invert_roundtrip_channels() {
        let img = solid(4, 4, 100);
        let g = grayscale(&img);
        assert_eq!(g.get_pixel(0, 0).0[0], 100);
        let inv = invert(&g);
        assert_eq!(inv.get_pixel(0, 0).0[0], 155);
    }

    #[test]
    fn deskew_of_straight_image_is_noop_ish() {
        // imagem em branco com linhas horizontais retas → skew ~0
        let mut img = solid(200, 120, 255);
        for y in (10..120).step_by(20) {
            for x in 5..195 {
                img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        let a = estimate_skew_deg(&img);
        assert!(a.abs() <= 1.0, "esperado ~0, obtido {a}");
    }

    #[test]
    fn rotate_expands_canvas() {
        let img = solid(100, 50, 255);
        let r = rotate(&img, 90.0);
        // 90° troca largura/altura (com arredondamento)
        assert!(r.width() >= 49 && r.width() <= 51);
        assert!(r.height() >= 99 && r.height() <= 101);
    }

    #[test]
    fn empty_image_has_no_codes() {
        let img = solid(80, 80, 255);
        // branco liso não tem QR/códigos nem tabela
        assert!(detect_codes(&img).is_empty());
        assert!(detect_table_regions(&img).is_empty());
    }
}
