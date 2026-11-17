//! Detectores da redação (§6.4, `modelos-redacao.md` §6): porte de `p3b/ref/detectores.py` (pré-processamento,
//! decodificação das saídas, NMS, score normalizado) e dos três caminhos recomendados (`cabecas`, `rostos`,
//! `placas`). Quadro RGB 8 bits (o pipe do `LeitorPipe`); caixas em pixels do quadro dado. Dono: P4.5.
//!
//! O redimensionamento imita o `cv2.resize` bilinear da referência (centro de pixel em +0,5, coeficientes em ponto
//! fixo de 11 bits e o arredondamento da passada vertical vetorizada), para as caixas baterem com a referência.

use crate::error::{Result, SicroError};
use crate::ia::SessaoIa;
use crate::video::pessoas::yolox::{self, Saida, Yolox};

use super::Imagem;

/// Detecção em pixels: (x1, y1)–(x2, y2), score e classe do modelo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deteccao {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub score: f32,
    pub classe: u16,
}

impl Deteccao {
    pub fn w(&self) -> f32 {
        self.x2 - self.x1
    }

    pub fn h(&self) -> f32 {
        self.y2 - self.y1
    }

    fn area(&self) -> f32 {
        self.w().max(0.0) * self.h().max(0.0)
    }

    pub fn iou(&self, o: &Deteccao) -> f32 {
        let iw = (self.x2.min(o.x2) - self.x1.max(o.x1)).max(0.0);
        let ih = (self.y2.min(o.y2) - self.y1.max(o.y1)).max(0.0);
        let inter = iw * ih;
        inter / (self.area() + o.area() - inter + 1e-9)
    }

    fn deslocar(mut self, dx: f32, dy: f32) -> Self {
        (self.x1, self.x2, self.y1, self.y2) = (self.x1 + dx, self.x2 + dx, self.y1 + dy, self.y2 + dy);
        self
    }
}

fn erro_ort(e: impl std::fmt::Display) -> SicroError {
    SicroError::Validation(format!("onnxruntime: {e}"))
}

/// `round` do Python (metade para o par).
pub fn arred(v: f64) -> i64 {
    v.round_ties_even() as i64
}

// ---------------------------------------------------------------- utilidades

/// NMS guloso por score (estável nos empates); `por_classe` = só suprime dentro da mesma classe.
pub fn nms(mut d: Vec<Deteccao>, thr: f32, por_classe: bool) -> Vec<Deteccao> {
    d.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut vivo = vec![true; d.len()];
    let mut fica = Vec::new();
    for i in 0..d.len() {
        if !vivo[i] {
            continue;
        }
        fica.push(d[i]);
        for j in i + 1..d.len() {
            if vivo[j] && d[i].iou(&d[j]) >= thr && (!por_classe || d[i].classe == d[j].classe) {
                vivo[j] = false;
            }
        }
    }
    fica
}

/// Recorta ao quadro `w`×`h` e descarta o que fica com menos de 1 px de lado.
pub fn recortar(d: Vec<Deteccao>, w: f32, h: f32) -> Vec<Deteccao> {
    d.into_iter()
        .map(|x| Deteccao { x1: x.x1.clamp(0.0, w), x2: x.x2.clamp(0.0, w), y1: x.y1.clamp(0.0, h), y2: x.y2.clamp(0.0, h), ..x })
        .filter(|x| x.w() >= 1.0 && x.h() >= 1.0)
        .collect()
}

/// Score comum entre detectores: 0,5 no limiar de trabalho (`alto`) do modelo, teto 1.
pub fn normalizar(mut d: Vec<Deteccao>, alto: f32) -> Vec<Deteccao> {
    for x in &mut d {
        x.score = (0.5 * x.score / alto).min(1.0);
    }
    d
}

/// Altura e largura arredondadas ao múltiplo de 32 mais próximo (mínimo 32).
pub fn multiplo32(h: usize, w: usize, escala: f64) -> (usize, usize) {
    let m = |v: usize| (arred(v as f64 * escala / 32.0).max(1) * 32) as usize;
    (m(h), m(w))
}

/// Coeficientes de um eixo do bilinear do OpenCV: (índice, índice vizinho, peso do 1º, peso do 2º) em 1/2048.
fn coeficientes(dst: usize, src: usize) -> Vec<(usize, usize, i32, i32)> {
    let escala = 1.0 / (dst as f64 / src as f64);
    (0..dst)
        .map(|d| {
            let f = ((d as f64 + 0.5) * escala - 0.5) as f32;
            let mut s = f.floor() as i64;
            let mut fr = f - s as f32;
            if s < 0 {
                (s, fr) = (0, 0.0);
            }
            if s >= src as i64 - 1 {
                (s, fr) = (src as i64 - 1, 0.0);
            }
            let s = s as usize;
            let a0 = ((1.0 - fr) * 2048.0).round_ties_even() as i32;
            let a1 = (fr * 2048.0).round_ties_even() as i32;
            (s, (s + 1).min(src - 1), a0, a1)
        })
        .collect()
}

/// Bilinear de 3 canais u8 `sw`×`sh` → `dw`×`dh` (o `cv2.resize` INTER_LINEAR da referência). Mesmo tamanho: cópia.
pub fn redimensionar(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    if (sw, sh) == (dw, dh) {
        return src.to_vec();
    }
    // redução exata de 2× nos dois eixos: o OpenCV troca por INTER_AREA (média de 2×2 com arredondamento)
    if sw == 2 * dw && sh == 2 * dh {
        let mut out = vec![0u8; dw * dh * 3];
        for y in 0..dh {
            for x in 0..dw {
                for c in 0..3 {
                    let p = |yy: usize, xx: usize| src[(yy * sw + xx) * 3 + c] as u32;
                    let s = p(2 * y, 2 * x) + p(2 * y, 2 * x + 1) + p(2 * y + 1, 2 * x) + p(2 * y + 1, 2 * x + 1);
                    out[(y * dw + x) * 3 + c] = ((s + 2) >> 2) as u8;
                }
            }
        }
        return out;
    }
    let cx = coeficientes(dw, sw);
    let cy = coeficientes(dh, sh);
    // passada horizontal de cada linha da fonte usada, em inteiros exatos
    let mut cache: Vec<Option<Vec<i32>>> = vec![None; sh];
    let horizontal = |y: usize, cache: &mut Vec<Option<Vec<i32>>>| {
        if cache[y].is_none() {
            let linha = &src[y * sw * 3..(y + 1) * sw * 3];
            let mut v = vec![0i32; dw * 3];
            for (d, &(s0, s1, a0, a1)) in cx.iter().enumerate() {
                for c in 0..3 {
                    v[d * 3 + c] = linha[s0 * 3 + c] as i32 * a0 + linha[s1 * 3 + c] as i32 * a1;
                }
            }
            cache[y] = Some(v);
        }
    };
    let mut out = vec![0u8; dw * dh * 3];
    for (d, &(s0, s1, b0, b1)) in cy.iter().enumerate() {
        horizontal(s0, &mut cache);
        horizontal(s1, &mut cache);
        let (l0, l1) = (cache[s0].as_ref().unwrap_or_else(|| unreachable!()), cache[s1].as_ref().unwrap_or_else(|| unreachable!()));
        let o = &mut out[d * dw * 3..(d + 1) * dw * 3];
        for x in 0..dw * 3 {
            // VResizeLinearVec_32s8u: (S >> 4) em 16 bits, produto alto, (soma + 2) >> 2
            let t0 = (l0[x] >> 4).clamp(i16::MIN as i32, i16::MAX as i32);
            let t1 = (l1[x] >> 4).clamp(i16::MIN as i32, i16::MAX as i32);
            let r = ((t0 * b0) >> 16) + ((t1 * b1) >> 16);
            o[x] = ((r + 2) >> 2).clamp(0, 255) as u8;
        }
        // linha da fonte que não será mais usada sai do cache
        if d + 1 < cy.len() {
            let prox = cy[d + 1].0;
            for y in s0.min(prox)..prox {
                cache[y] = None;
            }
        }
    }
    out
}

/// Recorte `[x1, x2) × [y1, y2)` de um quadro RGB (cópia).
pub fn recorte(q: &Imagem, x1: usize, y1: usize, x2: usize, y2: usize) -> Vec<u8> {
    let w = q.w as usize;
    let mut v = Vec::with_capacity((x2 - x1) * (y2 - y1) * 3);
    for y in y1..y2 {
        v.extend_from_slice(&q.rgb[(y * w + x1) * 3..(y * w + x2) * 3]);
    }
    v
}

/// NCHW f32 em BGR, menos `media` (por canal B, G, R), com a imagem RGB `w`×`h` no canto superior esquerdo de um
/// tensor `th`×`tw` preenchido com `fundo`.
fn tensor_bgr(rgb: &[u8], w: usize, h: usize, th: usize, tw: usize, fundo: f32, media: [f32; 3]) -> Vec<f32> {
    let plano = th * tw;
    let mut t = vec![fundo; 3 * plano];
    for y in 0..h.min(th) {
        for x in 0..w.min(tw) {
            let i = (y * w + x) * 3;
            let o = y * tw + x;
            t[o] = rgb[i + 2] as f32 - media[0];
            t[plano + o] = rgb[i + 1] as f32 - media[1];
            t[2 * plano + o] = rgb[i] as f32 - media[2];
        }
    }
    t
}

fn rodar_um(s: &mut SessaoIa, forma: [usize; 4], dados: Vec<f32>) -> Result<ort::session::SessionOutputs<'_>> {
    let nome = s.session.inputs().first().ok_or_else(|| erro_ort("modelo sem entrada"))?.name().to_string();
    let t = ort::value::Tensor::from_array((forma, dados)).map_err(erro_ort)?;
    s.session.run(ort::inputs![nome.as_str() => t]).map_err(erro_ort)
}

fn tensor(saidas: &ort::session::SessionOutputs<'_>, i: usize) -> Result<Vec<f32>> {
    let (_, v) = saidas[i].try_extract_tensor::<f32>().map_err(erro_ort)?;
    Ok(v.to_vec())
}

// ---------------------------------------------------------------- YuNet (rostos)

/// YuNet-n (BSD-3), entrada dinâmica múltipla de 32: BGR 0..255 sem média, completado com zero à direita e abaixo.
pub struct YuNet {
    pub s: SessaoIa,
}

impl YuNet {
    pub fn detectar(&mut self, q: &Imagem, thr: f32, nms_thr: f32, lado_max: usize, ampliar: bool) -> Result<Vec<Deteccao>> {
        let (w, h) = (q.w as usize, q.h as usize);
        let mut r = lado_max as f64 / w.max(h) as f64;
        if r > 1.0 && !ampliar {
            r = 1.0;
        }
        let (nw, nh) = ((arred(w as f64 * r).max(1)) as usize, (arred(h as f64 * r).max(1)) as usize);
        let red = redimensionar(q.rgb, w, h, nw, nh);
        let (th, tw) = (nh.div_ceil(32) * 32, nw.div_ceil(32) * 32);
        let dados = tensor_bgr(&red, nw, nh, th, tw, 0.0, [0.0; 3]);
        let saidas = rodar_um(&mut self.s, [1, 3, th, tw], dados)?;
        let mut d = Vec::new();
        for passo in [8usize, 16, 32] {
            let ler = |n: &str| -> Result<Vec<f32>> {
                let (_, v) = saidas[format!("{n}_{passo}").as_str()].try_extract_tensor::<f32>().map_err(erro_ort)?;
                Ok(v.to_vec())
            };
            let (cls, obj, bb) = (ler("cls")?, ler("obj")?, ler("bbox")?);
            let cols = tw / passo;
            let p = passo as f32;
            for k in 0..cls.len().min(obj.len()).min(bb.len() / 4) {
                let score = (cls[k].clamp(0.0, 1.0) * obj[k].clamp(0.0, 1.0)).sqrt();
                if score < thr {
                    continue;
                }
                let (col, lin) = ((k % cols) as f32, (k / cols) as f32);
                let cx = (col + bb[4 * k]) * p;
                let cy = (lin + bb[4 * k + 1]) * p;
                let bw = bb[4 * k + 2].exp() * p;
                let bh = bb[4 * k + 3].exp() * p;
                let r = r as f32;
                d.push(Deteccao { x1: (cx - bw / 2.0) / r, y1: (cy - bh / 2.0) / r, x2: (cx + bw / 2.0) / r, y2: (cy + bh / 2.0) / r, score, classe: 0 });
            }
        }
        Ok(recortar(nms(d, nms_thr, true), w as f32, h as f32))
    }
}

// ---------------------------------------------------------------- RetinaFace (rostos)

/// RetinaFace R34 (MIT), entrada dinâmica: BGR menos (104, 117, 123); âncoras por passo 8/16/32 com dois tamanhos.
pub struct RetinaFace {
    pub s: SessaoIa,
    ancoras: Option<((usize, usize), Vec<[f32; 4]>)>,
}

const TAMANHOS_RETINA: [[f32; 2]; 3] = [[16.0, 32.0], [64.0, 128.0], [256.0, 512.0]];

/// Âncoras (cx, cy, w, h) de uma entrada `h`×`w`: célula a célula (linha a linha), os dois tamanhos de cada passo.
pub fn ancoras_retina(h: usize, w: usize) -> Vec<[f32; 4]> {
    let mut a = Vec::new();
    for (passo, tams) in [8usize, 16, 32].into_iter().zip(TAMANHOS_RETINA) {
        let (fh, fw) = (h.div_ceil(passo), w.div_ceil(passo));
        for lin in 0..fh {
            for col in 0..fw {
                for t in tams {
                    a.push([((col as f64 + 0.5) * passo as f64) as f32, ((lin as f64 + 0.5) * passo as f64) as f32, t, t]);
                }
            }
        }
    }
    a
}

impl RetinaFace {
    pub fn nova(s: SessaoIa) -> Self {
        Self { s, ancoras: None }
    }

    pub fn detectar(&mut self, q: &Imagem, thr: f32, nms_thr: f32, lado_max: usize, ampliar: bool) -> Result<Vec<Deteccao>> {
        let (w, h) = (q.w as usize, q.h as usize);
        let mut r = lado_max as f64 / w.max(h) as f64;
        if r > 1.0 && !ampliar {
            r = 1.0;
        }
        let (nw, nh) = ((arred(w as f64 * r).max(1)) as usize, (arred(h as f64 * r).max(1)) as usize);
        let red = redimensionar(q.rgb, w, h, nw, nh);
        let dados = tensor_bgr(&red, nw, nh, nh, nw, 0.0, [104.0, 117.0, 123.0]);
        let (loc, conf) = {
            let saidas = rodar_um(&mut self.s, [1, 3, nh, nw], dados)?;
            (tensor(&saidas, 0)?, tensor(&saidas, 1)?)
        };
        if self.ancoras.as_ref().is_none_or(|(k, _)| *k != (nh, nw)) {
            self.ancoras = Some(((nh, nw), ancoras_retina(nh, nw)));
        }
        let a = &self.ancoras.as_ref().unwrap_or_else(|| unreachable!()).1;
        let n = a.len().min(loc.len() / 4).min(conf.len() / 2);
        let r = r as f32;
        let mut d = Vec::new();
        for k in 0..n {
            let score = conf[2 * k + 1];
            if score < thr {
                continue;
            }
            let (an, l) = (a[k], &loc[4 * k..4 * k + 4]);
            let cx = an[0] + l[0] * 0.1 * an[2];
            let cy = an[1] + l[1] * 0.1 * an[3];
            let bw = an[2] * (l[2] * 0.2).exp();
            let bh = an[3] * (l[3] * 0.2).exp();
            d.push(Deteccao { x1: (cx - bw / 2.0) / r, y1: (cy - bh / 2.0) / r, x2: (cx + bw / 2.0) / r, y2: (cy + bh / 2.0) / r, score, classe: 0 });
        }
        Ok(recortar(nms(d, nms_thr, true), w as f32, h as f32))
    }
}

// ---------------------------------------------------------------- YOLOX (corpo/cabeça/rosto; placas; veículos)

/// Como o quadro entra no YOLOX: esticado para `h`×`w` (modelos do PINTO) ou com letterbox no canto superior
/// esquerdo, cinza 114 (Megvii, Autolane).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entrada {
    Esticar,
    Letterbox,
}

/// YOLOX de n classes por `video::pessoas::yolox` (saída crua).
pub struct YoloxN {
    pub y: Yolox,
    pub entrada: Entrada,
}

impl YoloxN {
    pub fn nova(s: SessaoIa, entrada: Entrada) -> Result<Self> {
        Ok(Self { y: Yolox::de_sessao(s)?, entrada })
    }

    /// `classes` = as que contam (vazio = todas); `hw` = entrada do tensor (None = a fixa do grafo, senão 640×640).
    pub fn detectar(&mut self, q: &Imagem, thr: f32, nms_thr: f32, classes: &[u16], hw: Option<(usize, usize)>) -> Result<Vec<Deteccao>> {
        Ok(self.saida(q, hw)?.deteccoes(thr, nms_thr, classes))
    }

    /// Só a inferência: a saída crua serve a mais de uma escolha de classes (cabeças e corpos do mesmo quadro).
    pub fn saida(&mut self, q: &Imagem, hw: Option<(usize, usize)>) -> Result<SaidaYolox> {
        let (w, h) = (q.w as usize, q.h as usize);
        let (th, tw) = hw.or(self.y.fixo).unwrap_or((640, 640));
        let (dados, escala) = match self.entrada {
            Entrada::Esticar => {
                let red = redimensionar(q.rgb, w, h, tw, th);
                (tensor_bgr(&red, tw, th, th, tw, 0.0, [0.0; 3]), ((tw as f64 / w as f64) as f32, (th as f64 / h as f64) as f32))
            }
            Entrada::Letterbox => {
                let r = (th as f64 / h as f64).min(tw as f64 / w as f64);
                let (nw, nh) = ((arred(w as f64 * r).max(1)) as usize, (arred(h as f64 * r).max(1)) as usize);
                let red = redimensionar(q.rgb, w, h, nw, nh);
                (tensor_bgr(&red, nw, nh, th, tw, 114.0, [0.0; 3]), (r as f32, r as f32))
            }
        };
        let out = self.y.rodar(dados, th, tw)?;
        let ancoras = (th / 8 * (tw / 8) + th / 16 * (tw / 16) + th / 32 * (tw / 32)).max(1);
        Ok(SaidaYolox { s: Saida { colunas: out.len() / ancoras, h: th, w: tw, cru: true }, out, escala, w, h })
    }
}

/// Saída crua de um YOLOX num quadro `w`×`h`.
pub struct SaidaYolox {
    s: Saida,
    out: Vec<f32>,
    escala: (f32, f32),
    w: usize,
    h: usize,
}

impl SaidaYolox {
    pub fn deteccoes(&self, thr: f32, nms_thr: f32, classes: &[u16]) -> Vec<Deteccao> {
        let nc = self.s.colunas.saturating_sub(5);
        let mascara: Vec<bool> = (0..nc).map(|c| classes.is_empty() || classes.contains(&(c as u16))).collect();
        let d = yolox::posprocessar(&self.out, &self.s, self.escala, thr, nms_thr, &mascara)
            .into_iter()
            .map(|x| Deteccao { x1: x.x, y1: x.y, x2: x.x + x.w, y2: x.y + x.h, score: x.score, classe: x.classe })
            .collect();
        recortar(d, self.w as f32, self.h as f32)
    }
}

// ---------------------------------------------------------------- RT-DETRv2 (placas)

/// RT-DETRv2-R18 de placas (Apache-2.0): entrada fixa 640×640, RGB ÷ 255 esticado; score = sigmoide; caixas
/// (cx, cy, w, h) normalizadas.
pub struct RtDetr {
    pub s: SessaoIa,
}

impl RtDetr {
    pub fn detectar(&mut self, q: &Imagem, thr: f32, nms_thr: f32) -> Result<Vec<Deteccao>> {
        let (w, h) = (q.w as usize, q.h as usize);
        let red = redimensionar(q.rgb, w, h, 640, 640);
        let plano = 640 * 640;
        let mut dados = vec![0f32; 3 * plano];
        for (i, px) in red.chunks_exact(3).enumerate() {
            for c in 0..3 {
                dados[c * plano + i] = (px[c] as f64 / 255.0) as f32;
            }
        }
        let (logits, caixas) = {
            let saidas = rodar_um(&mut self.s, [1, 3, 640, 640], dados)?;
            (tensor(&saidas, 0)?, tensor(&saidas, 1)?)
        };
        let (wf, hf) = (w as f32, h as f32);
        let mut d = Vec::new();
        for k in 0..logits.len().min(caixas.len() / 4) {
            let score = 1.0 / (1.0 + (-logits[k]).exp());
            if score < thr {
                continue;
            }
            let b = &caixas[4 * k..4 * k + 4];
            d.push(Deteccao { x1: (b[0] - b[2] / 2.0) * wf, y1: (b[1] - b[3] / 2.0) * hf, x2: (b[0] + b[2] / 2.0) * wf, y2: (b[1] + b[3] / 2.0) * hf, score, classe: 0 });
        }
        Ok(recortar(nms(d, nms_thr, true), wf, hf))
    }
}

// ---------------------------------------------------------------- caminhos recomendados (dossiê)

/// (alto, baixo) de cada detector: `alto` cria a trilha (vira 0,5 no score normalizado); `baixo` só a mantém.
pub const ROSTO_RETINAFACE: (f32, f32) = (0.60, 0.30);
pub const ROSTO_YUNET: (f32, f32) = (0.70, 0.35);
pub const CABECA: (f32, f32) = (0.35, 0.15);
pub const VEICULO: (f32, f32) = (0.35, 0.10);
pub const PLACA_RTDETR: (f32, f32) = (0.09, 0.045);
pub const PLACA_YOLOX: (f32, f32) = (0.18, 0.09);
/// Classes do YOLOX WholeBody12 e do COCO usadas.
pub const CORPO: u16 = 0;
pub const CABECA_WB12: u16 = 2;
pub const ROSTO_WB12: u16 = 3;
pub const VEICULOS_COCO: [u16; 4] = [2, 3, 5, 7];

/// Cabeças com o YOLOX WholeBody12 na resolução do quadro (esticado ao múltiplo de 32), score normalizado.
pub fn cabecas(corpo: &mut YoloxN, q: &Imagem, escala: f64) -> Result<Vec<Deteccao>> {
    let (alto, baixo) = CABECA;
    let hw = multiplo32(q.h as usize, q.w as usize, escala);
    Ok(normalizar(corpo.detectar(q, baixo, 0.45, &[CABECA_WB12], Some(hw))?, alto))
}

/// Corpos (classe 0 do WholeBody12), como as cabeças.
pub fn corpos(corpo: &mut YoloxN, q: &Imagem, escala: f64) -> Result<Vec<Deteccao>> {
    let (alto, baixo) = CABECA;
    let hw = multiplo32(q.h as usize, q.w as usize, escala);
    Ok(normalizar(corpo.detectar(q, baixo, 0.45, &[CORPO], Some(hw))?, alto))
}

/// Detector de rosto: RetinaFace (GPU) ou YuNet (CPU).
pub enum Rosto {
    Retina(RetinaFace),
    YuNet(YuNet),
}

/// Rostos com lado de entrada `lado` (= maior lado do quadro × escala; acima dele amplia), score normalizado.
pub fn rostos(det: &mut Rosto, q: &Imagem, lado: usize) -> Result<Vec<Deteccao>> {
    let ampliar = lado > (q.w.max(q.h) as usize);
    match det {
        Rosto::Retina(m) => {
            let (alto, baixo) = ROSTO_RETINAFACE;
            Ok(normalizar(m.detectar(q, baixo, 0.4, lado, ampliar)?, alto))
        }
        Rosto::YuNet(m) => {
            let (alto, baixo) = ROSTO_YUNET;
            Ok(normalizar(m.detectar(q, baixo, 0.3, lado, ampliar)?, alto))
        }
    }
}

/// Lado de entrada do detector de rosto: maior lado × `escala` (`rostos()` da referência: `round`).
pub fn lado_do_rosto(q: &Imagem, escala: f64) -> usize {
    arred(q.w.max(q.h) as f64 * escala) as usize
}

/// Folga em volta do veículo, lado mínimo dele e o teto de veículos por quadro (`placas()` da referência).
pub const FOLGA_VEICULO: f32 = 0.15;
pub const LADO_MIN_VEICULO: f32 = 60.0;
pub const MAX_VEICULOS: usize = 40;

/// Placas em duas etapas: quadro inteiro e recorte de cada veículo (YOLOX-S COCO), nos detectores de placa dados
/// (`rtdetr` None = só o YOLOX, a queda de CPU). Devolve (placas com score normalizado, veículos).
pub fn placas(rtdetr: Option<&mut RtDetr>, placa: &mut YoloxN, coco: &mut YoloxN, q: &Imagem) -> Result<(Vec<Deteccao>, Vec<Deteccao>)> {
    let (w, h) = (q.w as usize, q.h as usize);
    let v = coco.detectar(q, VEICULO.0, 0.45, &VEICULOS_COCO, None)?;
    // carro × caminhão no mesmo veículo
    let v: Vec<Deteccao> = nms(v, 0.6, false).into_iter().filter(|b| b.w() >= LADO_MIN_VEICULO).take(MAX_VEICULOS).collect();
    let mut rtdetr = rtdetr;
    let mut rodar = |m: &Imagem, partes: &mut Vec<Deteccao>, dx: f32, dy: f32| -> Result<()> {
        let (alto, baixo) = PLACA_YOLOX;
        partes.extend(normalizar(placa.detectar(m, baixo, 0.45, &[], None)?, alto).into_iter().map(|d| d.deslocar(dx, dy)));
        if let Some(rt) = rtdetr.as_deref_mut() {
            let (alto, baixo) = PLACA_RTDETR;
            partes.extend(normalizar(rt.detectar(m, baixo, 0.5)?, alto).into_iter().map(|d| d.deslocar(dx, dy)));
        }
        Ok(())
    };
    let mut partes = Vec::new();
    rodar(q, &mut partes, 0.0, 0.0)?;
    for b in &v {
        let (fx, fy) = (b.w() * FOLGA_VEICULO, b.h() * FOLGA_VEICULO);
        let x1 = (b.x1 - fx).max(0.0) as usize;
        let y1 = (b.y1 - fy).max(0.0) as usize;
        let x2 = ((b.x2 + fx).min(w as f32) as usize).min(w);
        let y2 = ((b.y2 + fy).min(h as f32) as usize).min(h);
        if x2 <= x1 || y2 <= y1 || (x2 - x1).min(y2 - y1) < 24 {
            continue;
        }
        let rgb = recorte(q, x1, y1, x2, y2);
        let r = Imagem { rgb: &rgb, w: (x2 - x1) as u32, h: (y2 - y1) as u32 };
        rodar(&r, &mut partes, x1 as f32, y1 as f32)?;
    }
    Ok((nms(partes, 0.5, false), v))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(x1: f32, y1: f32, x2: f32, y2: f32, score: f32, classe: u16) -> Deteccao {
        Deteccao { x1, y1, x2, y2, score, classe }
    }

    #[test]
    fn nms_por_classe_e_entre_classes() {
        let a = d(0.0, 0.0, 10.0, 10.0, 0.9, 0);
        let b = d(1.0, 0.0, 11.0, 10.0, 0.8, 1);
        let c = d(1.0, 0.0, 11.0, 10.0, 0.7, 0);
        assert_eq!(nms(vec![c, b, a], 0.45, true), vec![a, b]);
        assert_eq!(nms(vec![c, b, a], 0.45, false), vec![a]);
        // empate: fica o que veio antes
        let e = d(0.0, 0.0, 10.0, 10.0, 0.9, 0);
        assert_eq!(nms(vec![e.deslocar(0.5, 0.0), e], 0.45, true).len(), 1);
        assert_eq!(nms(vec![e.deslocar(0.5, 0.0), e], 0.45, true)[0].x1, 0.5);
    }

    #[test]
    fn recorta_e_normaliza() {
        let r = recortar(vec![d(-5.0, 2.0, 20.0, 8.0, 0.5, 0), d(99.5, 0.0, 120.0, 5.0, 0.5, 0)], 100.0, 50.0);
        assert_eq!(r, vec![d(0.0, 2.0, 20.0, 8.0, 0.5, 0)]);
        let n = normalizar(vec![d(0.0, 0.0, 1.0, 1.0, 0.09, 0), d(0.0, 0.0, 1.0, 1.0, 0.5, 0)], 0.09);
        assert_eq!((n[0].score, n[1].score), (0.5, 1.0));
        assert_eq!(multiplo32(1080, 1920, 1.0), (1088, 1920));
        assert_eq!(multiplo32(1067, 1600, 1.0), (1056, 1600));
        assert_eq!(multiplo32(10, 10, 1.0), (32, 32));
        // o `round` do Python: metade vai para o par
        assert_eq!((arred(2.5), arred(3.5), arred(-0.5)), (2, 4, 0));
    }

    /// O bilinear do OpenCV nos casos que dá para conferir de cabeça: mesma imagem, ampliação 2× de um degrau
    /// (centro de pixel em +0,5: 0, 0,25, 0,75, 1 do degrau) e redução que cai no meio de dois pixels.
    #[test]
    fn bilinear_como_o_opencv() {
        let px = |v: &[u8]| v.iter().flat_map(|&x| [x, x, x]).collect::<Vec<u8>>();
        let src = px(&[0, 200]);
        assert_eq!(redimensionar(&src, 2, 1, 2, 1), src);
        let up = redimensionar(&src, 2, 1, 4, 1);
        assert_eq!(up.iter().step_by(3).copied().collect::<Vec<_>>(), vec![0, 50, 150, 200]);
        let down = redimensionar(&px(&[0, 100, 200, 250]), 4, 1, 2, 1);
        assert_eq!(down.iter().step_by(3).copied().collect::<Vec<_>>(), vec![50, 225]);
        // vertical igual ao horizontal
        let col = redimensionar(&src, 1, 2, 1, 4);
        assert_eq!(col.iter().step_by(3).copied().collect::<Vec<_>>(), vec![0, 50, 150, 200]);
        // 2× exato nos dois eixos: média de 2×2 (1 + 2 + 3 + 5 = 11 → 3)
        let q = redimensionar(&px(&[1, 2, 3, 5]), 2, 2, 1, 1);
        assert_eq!(q, vec![3, 3, 3]);
    }

    #[test]
    fn ancoras_da_retinaface() {
        let a = ancoras_retina(17, 33);
        // passo 8: 3 × 5 células × 2; 16: 2 × 3 × 2; 32: 1 × 2 × 2
        assert_eq!(a.len(), 30 + 12 + 4);
        assert_eq!(a[0], [4.0, 4.0, 16.0, 16.0]);
        assert_eq!(a[1], [4.0, 4.0, 32.0, 32.0]);
        assert_eq!(a[2], [12.0, 4.0, 16.0, 16.0]);
        assert_eq!(a[10], [4.0, 12.0, 16.0, 16.0]);
        assert_eq!(a[30], [8.0, 8.0, 64.0, 64.0]);
        assert_eq!(a[45], [48.0, 16.0, 512.0, 512.0]);
    }
}
