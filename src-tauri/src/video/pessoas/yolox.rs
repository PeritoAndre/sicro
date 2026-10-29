//! YOLOX (Apache-2.0, Megvii) em ONNX: só a classe "pessoa" (COCO 0).
//! O ffmpeg já entrega o quadro reduzido (lado maior = lado do modelo); aqui só se
//! completa com cinza 114 (letterbox no canto superior esquerdo), BGR 0..255, CHW.
//! O ONNX oficial sai SEM decodificar: grades e passos (8/16/32) são aplicados aqui.

use std::path::Path;
use std::sync::OnceLock;

use crate::error::{Result, SicroError};

/// Uma detecção em pixels do quadro ORIGINAL.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Det {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
}

impl Det {
    pub fn iou(&self, o: &Det) -> f32 {
        let x1 = self.x.max(o.x);
        let y1 = self.y.max(o.y);
        let x2 = (self.x + self.w).min(o.x + o.w);
        let y2 = (self.y + self.h).min(o.y + o.h);
        let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
        let uni = self.w * self.h + o.w * o.h - inter;
        if uni <= 0.0 { 0.0 } else { inter / uni }
    }
}

/// Tamanho do quadro reduzido que o ffmpeg deve entregar para um vídeo `w`×`h`
/// e um modelo de lado `side` (pares, para o filtro `scale`).
pub fn scaled_size(w: u32, h: u32, side: u32) -> (u32, u32) {
    let r = (side as f32 / w as f32).min(side as f32 / h as f32);
    let even = |v: f32| (((v.round() as u32) / 2) * 2).max(2);
    (even(w as f32 * r).min(side), even(h as f32 * r).min(side))
}

/// Monta o tensor NCHW (BGR, 0..255) de um quadro RGB reduzido `sw`×`sh`,
/// preenchendo o resto do `side`×`side` com 114.
pub fn letterbox_bgr_chw(rgb: &[u8], sw: usize, sh: usize, side: usize) -> Vec<f32> {
    let plane = side * side;
    let mut out = vec![114.0f32; 3 * plane];
    for y in 0..sh.min(side) {
        for x in 0..sw.min(side) {
            let i = (y * sw + x) * 3;
            let o = y * side + x;
            out[o] = rgb[i + 2] as f32; // B
            out[plane + o] = rgb[i + 1] as f32; // G
            out[2 * plane + o] = rgb[i] as f32; // R
        }
    }
    out
}

/// Candidatos (x,y,w,h no espaço do tensor) -> detecções filtradas por pessoa,
/// limiar e NMS, em pixels do original (`ratio` = reduzido / original).
/// `out` tem `n × 85` valores: cx, cy, w, h, obj, 80 classes.
pub fn postprocess(out: &[f32], side: usize, ratio: f32, thr: f32, nms_thr: f32) -> Vec<Det> {
    let strides = [8usize, 16, 32];
    let mut dets: Vec<Det> = Vec::new();
    let mut idx = 0usize;
    'outer: for &s in &strides {
        let g = side / s;
        for gy in 0..g {
            for gx in 0..g {
                let base = idx * 85;
                idx += 1;
                if base + 85 > out.len() {
                    break 'outer;
                }
                let score = out[base + 4] * out[base + 5]; // objetividade × classe 0 (pessoa)
                if score < thr {
                    continue;
                }
                let cx = (out[base] + gx as f32) * s as f32;
                let cy = (out[base + 1] + gy as f32) * s as f32;
                let w = out[base + 2].exp() * s as f32;
                let h = out[base + 3].exp() * s as f32;
                dets.push(Det {
                    x: (cx - w / 2.0) / ratio,
                    y: (cy - h / 2.0) / ratio,
                    w: w / ratio,
                    h: h / ratio,
                    score,
                });
            }
        }
    }
    nms(dets, nms_thr)
}

pub fn nms(mut dets: Vec<Det>, thr: f32) -> Vec<Det> {
    dets.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut keep: Vec<Det> = Vec::new();
    for d in dets {
        if keep.iter().all(|k| k.iou(&d) < thr) {
            keep.push(d);
        }
    }
    keep
}

// ---------------------------------------------------------------------------
// Sessão ONNX

static ENV: OnceLock<ort::environment::Environment> = OnceLock::new();

pub struct Yolox {
    session: ort::session::Session,
    pub side: usize,
    input_name: String,
}

impl Yolox {
    /// Carrega a libonnxruntime de `lib` e o modelo. O ambiente do `ort` é um só por processo.
    pub fn load(lib: &Path, model: &Path) -> Result<Self> {
        let err = |e: &dyn std::fmt::Display| SicroError::Validation(format!("onnxruntime: {e}"));
        if ENV.get().is_none() {
            let env = ort::init_from(lib)
                .map_err(|e| err(&e))?
                .build()
                .map_err(|e| err(&e))?;
            let _ = ENV.set(env);
        }
        let env = ENV.get().ok_or_else(|| err(&"ambiente não iniciado"))?;
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(8);
        let session = ort::session::Session::builder(env)
            .map_err(|e| err(&e))?
            .with_intra_threads(threads)
            .map_err(|e| err(&e))?
            .commit_from_file(model)
            .map_err(|e| err(&e))?;
        let input = session
            .inputs()
            .first()
            .ok_or_else(|| err(&"modelo sem entrada"))?;
        let input_name = input.name().to_string();
        let side = match input.dtype() {
            ort::value::ValueType::Tensor { shape, .. } => {
                shape.get(2).copied().filter(|d| *d > 0).unwrap_or(640) as usize
            }
            _ => 640,
        };
        Ok(Self { session, side, input_name })
    }

    /// Detecta pessoas num quadro RGB reduzido `sw`×`sh`; `ratio` = reduzido / original.
    pub fn detect(&mut self, rgb: &[u8], sw: usize, sh: usize, ratio: f32, thr: f32) -> Result<Vec<Det>> {
        let err = |e: &dyn std::fmt::Display| SicroError::Validation(format!("onnxruntime: {e}"));
        let side = self.side;
        let data = letterbox_bgr_chw(rgb, sw, sh, side);
        let tensor = ort::value::Tensor::from_array(([1usize, 3, side, side], data)).map_err(|e| err(&e))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor])
            .map_err(|e| err(&e))?;
        let (_, out) = outputs[0].try_extract_tensor::<f32>().map_err(|e| err(&e))?;
        Ok(postprocess(out, side, ratio, thr, 0.45))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tamanho_reduzido_cabe_no_modelo() {
        assert_eq!(scaled_size(1920, 1080, 640), (640, 360));
        assert_eq!(scaled_size(1080, 1920, 640), (360, 640));
        assert_eq!(scaled_size(320, 240, 640), (640, 480));
    }

    #[test]
    fn letterbox_poe_bgr_e_cinza() {
        // 2x2 reduzido num modelo de lado 4
        let rgb = [10u8, 20, 30, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let t = letterbox_bgr_chw(&rgb, 2, 2, 4);
        assert_eq!(t.len(), 3 * 16);
        assert_eq!(t[0], 30.0); // B do pixel (0,0)
        assert_eq!(t[16], 20.0); // G
        assert_eq!(t[32], 10.0); // R
        assert_eq!(t[3], 114.0); // fora da imagem
        assert_eq!(t[3 * 4], 114.0);
    }

    /// Saída sintética: uma pessoa no passo 8 (grade 5,3) e uma duplicata mais fraca ao lado.
    #[test]
    fn posprocessamento_decodifica_grade_e_aplica_nms() {
        let side = 64usize; // grades 8x8, 4x4, 2x2 = 84 âncoras
        let mut out = vec![0.0f32; 84 * 85];
        let mut set = |i: usize, cx: f32, cy: f32, w: f32, h: f32, obj: f32, cls: f32| {
            let b = i * 85;
            out[b] = cx;
            out[b + 1] = cy;
            out[b + 2] = w.ln() ;
            out[b + 3] = h.ln();
            out[b + 4] = obj;
            out[b + 5] = cls;
        };
        // âncora (gx=5, gy=3) do passo 8 -> índice 3*8+5; centro = (0.5+5)*8, (0.5+3)*8; caixa 16x32 (em passos de 8: 2 e 4)
        set(3 * 8 + 5, 0.5, 0.5, 2.0, 4.0, 0.9, 0.9);
        // vizinha (gx=6) com quase a mesma caixa, mais fraca -> suprimida
        set(3 * 8 + 6, -0.5, 0.5, 2.0, 4.0, 0.6, 0.9);
        let d = postprocess(&out, side, 0.5, 0.3, 0.45);
        assert_eq!(d.len(), 1);
        let p = d[0];
        // centro (44, 28) no tensor, caixa 16x32 -> original = /0.5
        assert!((p.x - (44.0 - 8.0) / 0.5).abs() < 1e-3, "x {}", p.x);
        assert!((p.y - (28.0 - 16.0) / 0.5).abs() < 1e-3);
        assert!((p.w - 32.0).abs() < 1e-3 && (p.h - 64.0).abs() < 1e-3);
        assert!((p.score - 0.81).abs() < 1e-4);
    }

    #[test]
    fn iou_basico() {
        let a = Det { x: 0.0, y: 0.0, w: 10.0, h: 10.0, score: 1.0 };
        let b = Det { x: 5.0, y: 0.0, w: 10.0, h: 10.0, score: 1.0 };
        assert!((a.iou(&b) - 50.0 / 150.0).abs() < 1e-6);
    }
}
