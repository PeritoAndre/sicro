//! Rastreio manual de mancha: o perito desenha uma caixa num quadro e o Rust segue aquele
//! pedaço de pixels por correlação cruzada normalizada (ZNCC, implementação própria), com o
//! modelo atualizado devagar e a busca numa janela em torno da posição prevista.
//! Para quando a confiança cai, avisando onde parou. Não depende da rede neural.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Result, SicroError};

use super::decode::FrameReader;
use super::yolox::scaled_size;
use super::Amostra;

/// Imagem em tons de cinza.
pub struct Cinza {
    pub w: usize,
    pub h: usize,
    pub d: Vec<u8>,
}

impl Cinza {
    pub fn de_rgb(rgb: &[u8], w: usize, h: usize) -> Self {
        let d = rgb
            .chunks_exact(3)
            .map(|p| ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8)
            .collect();
        Self { w, h, d }
    }
}

/// Pontos de amostragem do modelo por correlação (limita o custo em caixas grandes).
const MAX_AMOSTRAS: usize = 1200;
const ATUALIZACAO: f32 = 0.1;
const CONF_ATUALIZA: f32 = 0.7;
/// Correlação mínima com o modelo ORIGINAL (marcado pelo perito): impede de passar, aos poucos,
/// para outra coisa (alguém que cruza na frente) sem que a confiança com o modelo atual caia.
const CONF_ANCORA: f32 = 0.33;

pub struct Seguidor {
    tw: usize,
    th: usize,
    modelo: Vec<f32>,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    passo: usize,
    // modelo amostrado, sem a média, e sua norma
    amostras: Vec<(usize, usize, f32)>,
    norma: f32,
    ancora: Vec<(usize, usize, f32)>,
    norma_ancora: f32,
}

impl Seguidor {
    /// `x, y, w, h` em pixels da imagem; `None` se a caixa é pequena demais para correlacionar.
    pub fn novo(img: &Cinza, x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        let tw = (w.round() as usize).min(img.w);
        let th = (h.round() as usize).min(img.h);
        if tw < 6 || th < 6 {
            return None;
        }
        let x = x.clamp(0.0, (img.w - tw) as f32);
        let y = y.clamp(0.0, (img.h - th) as f32);
        let mut s = Self {
            tw,
            th,
            modelo: vec![0.0; tw * th],
            x,
            y,
            vx: 0.0,
            vy: 0.0,
            passo: ((tw * th) as f32 / MAX_AMOSTRAS as f32).sqrt().ceil().max(1.0) as usize,
            amostras: Vec::new(),
            norma: 0.0,
            ancora: Vec::new(),
            norma_ancora: 0.0,
        };
        let (ix, iy) = (x.round() as usize, y.round() as usize);
        for j in 0..th {
            for i in 0..tw {
                s.modelo[j * tw + i] = img.d[(iy + j) * img.w + ix + i] as f32;
            }
        }
        s.preparar();
        s.ancora = s.amostras.clone();
        s.norma_ancora = s.norma;
        if s.norma < 1e-3 {
            return None; // caixa lisa: não há o que seguir
        }
        Some(s)
    }

    fn preparar(&mut self) {
        let p = self.passo;
        let pts: Vec<(usize, usize, f32)> = (0..self.th)
            .step_by(p)
            .flat_map(|j| (0..self.tw).step_by(p).map(move |i| (i, j)))
            .map(|(i, j)| (i, j, self.modelo[j * self.tw + i]))
            .collect();
        let media = pts.iter().map(|a| a.2).sum::<f32>() / pts.len() as f32;
        self.amostras = pts.into_iter().map(|(i, j, v)| (i, j, v - media)).collect();
        self.norma = self.amostras.iter().map(|a| a.2 * a.2).sum::<f32>().sqrt();
    }

    pub fn caixa(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.tw as f32, self.th as f32)
    }

    /// Correlação (−1..1) do modelo com a imagem na posição inteira (`px`, `py`).
    fn correlacao(&self, img: &Cinza, px: usize, py: usize) -> f32 {
        Self::zncc(img, px, py, &self.amostras, self.norma)
    }

    fn zncc(img: &Cinza, px: usize, py: usize, amostras: &[(usize, usize, f32)], norma: f32) -> f32 {
        let n = amostras.len() as f32;
        let (mut si, mut sii, mut sit) = (0.0f32, 0.0f32, 0.0f32);
        for &(i, j, t) in amostras {
            let v = img.d[(py + j) * img.w + px + i] as f32;
            si += v;
            sii += v * v;
            sit += v * t;
        }
        let var = sii - si * si / n;
        if var < 1e-3 * n {
            return 0.0;
        }
        sit / (var.sqrt() * norma)
    }

    /// Procura o modelo no quadro seguinte. Devolve (x, y, confiança) e avança o estado,
    /// ou `None` se o melhor encaixe não chega a `min_conf` (o estado fica onde estava).
    pub fn passo(&mut self, img: &Cinza, min_conf: f32) -> Option<(f32, f32, f32)> {
        let vel = self.vx.hypot(self.vy);
        let lado = self.tw.max(self.th) as f32;
        let margem = (0.3 * lado).max(8.0) + (1.5 * vel).min(0.5 * lado);
        let (cx, cy) = (self.x + self.vx, self.y + self.vy);
        let xmax = (img.w - self.tw) as f32;
        let ymax = (img.h - self.th) as f32;
        let (x0, x1) = ((cx - margem).clamp(0.0, xmax) as usize, (cx + margem).clamp(0.0, xmax).ceil() as usize);
        let (y0, y1) = ((cy - margem).clamp(0.0, ymax) as usize, (cy + margem).clamp(0.0, ymax).ceil() as usize);
        let larg = x1 - x0 + 1;
        let mut mapa = Vec::with_capacity(larg * (y1 - y0 + 1));
        let mut melhor = (f32::MIN, x0, y0);
        for py in y0..=y1 {
            for px in x0..=x1 {
                let c = self.correlacao(img, px, py);
                mapa.push(c);
                if c > melhor.0 {
                    melhor = (c, px, py);
                }
            }
        }
        // encaixe ambíguo: outro pico, longe do melhor, quase tão bom (cruzou alguém parecido)
        if melhor.0 < 0.85 {
            let raio = (0.4 * lado) as i64;
            let segundo = mapa
                .iter()
                .enumerate()
                .filter(|(k, _)| {
                    let (qx, qy) = ((x0 + k % larg) as i64, (y0 + k / larg) as i64);
                    (qx - melhor.1 as i64).abs().max((qy - melhor.2 as i64).abs()) > raio
                })
                .map(|(_, &c)| c)
                .fold(f32::MIN, f32::max);
            if segundo >= melhor.0 - 0.08 {
                return None;
            }
        }
        let (conf, px, py) = melhor;
        if conf < min_conf || Self::zncc(img, px, py, &self.ancora, self.norma_ancora) < CONF_ANCORA {
            return None;
        }
        self.vx = 0.5 * self.vx + 0.5 * (px as f32 - self.x);
        self.vy = 0.5 * self.vy + 0.5 * (py as f32 - self.y);
        self.x = px as f32;
        self.y = py as f32;
        if conf >= CONF_ATUALIZA {
            for j in 0..self.th {
                for i in 0..self.tw {
                    let v = img.d[(py + j) * img.w + px + i] as f32;
                    let m = &mut self.modelo[j * self.tw + i];
                    *m += ATUALIZACAO * (v - *m);
                }
            }
            self.preparar();
        }
        Some((self.x, self.y, conf))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Motivo {
    FimDoVideo,
    Confianca,
    SaiuDoQuadro,
    Limite,
}

#[derive(Debug, Clone, Copy)]
pub struct OpcoesMancha {
    /// Abaixo desta correlação o rastreio para.
    pub min_conf: f32,
    /// Máximo de quadros seguidos além do inicial.
    pub max_quadros: u32,
}

impl Default for OpcoesMancha {
    fn default() -> Self {
        Self { min_conf: 0.55, max_quadros: 600 }
    }
}

#[derive(Debug)]
pub struct Resultado {
    /// Em ordem de tempo, incluindo a caixa do quadro inicial.
    pub amostras: Vec<Amostra>,
    /// Instante do primeiro quadro em que a mancha se perdeu (se parou por isso).
    pub parou_em: Option<f64>,
    pub motivo: Motivo,
    pub conf_media: f32,
}

const CHUNK: usize = 100;

/// Segue a caixa (`x, y, w, h` em pixels do vídeo exibido, `dw`×`dh`) a partir do quadro `idx0`.
#[allow(clippy::too_many_arguments)]
pub fn seguir(
    video: &Path,
    times: &[f64],
    dw: u32,
    dh: u32,
    idx0: usize,
    caixa: (f32, f32, f32, f32),
    para_frente: bool,
    op: OpcoesMancha,
    cancel: &AtomicBool,
    progresso: &mut dyn FnMut(u32),
) -> Result<Resultado> {
    if idx0 >= times.len() {
        return Err(SicroError::Validation("instante fora do vídeo".into()));
    }
    // escala de trabalho: até 960 px no lado maior, subindo (até o original) se a caixa for pequena
    let menor = caixa.2.min(caixa.3).max(1.0);
    let esc = (960.0 / dw.max(dh) as f32).max(28.0 / menor).min(1.0);
    let (sw, sh) = scaled_size(dw, dh, (dw.max(dh) as f32 * esc).round().max(64.0) as u32);
    let (kx, ky) = (sw as f32 / dw as f32, sh as f32 / dh as f32);
    let (ux, uy) = (dw as f32 / sw as f32, dh as f32 / sh as f32);
    let ler = |r: &mut FrameReader| -> Result<Option<Cinza>> {
        Ok(r.next()?.map(|f| Cinza::de_rgb(f, sw as usize, sh as usize)))
    };
    let inicio = |i: usize| times[i] - 0.0005;

    let mut r0 = FrameReader::open_from(video, inicio(idx0), sw, sh)?;
    let Some(img0) = ler(&mut r0)? else {
        return Err(SicroError::Validation("não foi possível ler o quadro".into()));
    };
    let mut seg = Seguidor::novo(&img0, caixa.0 * kx, caixa.1 * ky, caixa.2 * kx, caixa.3 * ky)
        .ok_or_else(|| SicroError::Validation("a caixa é pequena demais ou lisa demais para ser seguida".into()))?;
    let em_pixels = |s: &Seguidor, t: f64| {
        let (x, y, w, h) = s.caixa();
        Amostra { t, x: x * ux, y: y * uy, w: w * ux, h: h * uy }
    };
    let mut saida = vec![Amostra { t: times[idx0], x: caixa.0, y: caixa.1, w: caixa.2, h: caixa.3 }];
    let (mut soma, mut n) = (0.0f32, 0u32);
    let mut motivo = Motivo::FimDoVideo;
    let mut parou_em = None;
    let mut feitos = 0u32;

    // processa um quadro; devolve true para continuar
    let mut um = |seg: &mut Seguidor, img: &Cinza, idx: usize, saida: &mut Vec<Amostra>| -> bool {
        match seg.passo(img, op.min_conf) {
            None => {
                motivo = Motivo::Confianca;
                parou_em = Some(times[idx]);
                false
            }
            Some((x, y, c)) => {
                soma += c;
                n += 1;
                let a = em_pixels(seg, times[idx]);
                let centro_fora = a.x + a.w / 2.0 <= 0.0
                    || a.y + a.h / 2.0 <= 0.0
                    || a.x + a.w / 2.0 >= dw as f32
                    || a.y + a.h / 2.0 >= dh as f32;
                let colado = x <= 0.0 || y <= 0.0 || x + seg.tw as f32 >= sw as f32 || y + seg.th as f32 >= sh as f32;
                if colado && centro_fora {
                    motivo = Motivo::SaiuDoQuadro;
                    parou_em = Some(times[idx]);
                    return false;
                }
                saida.push(a);
                true
            }
        }
    };

    if para_frente {
        let fim = (idx0 + op.max_quadros as usize).min(times.len() - 1);
        let mut idx = idx0;
        'f: while idx < fim {
            if cancel.load(Ordering::Relaxed) {
                return Err(SicroError::Validation("cancelado".into()));
            }
            let Some(img) = ler(&mut r0)? else { break };
            idx += 1;
            if !um(&mut seg, &img, idx, &mut saida) {
                break 'f;
            }
            feitos += 1;
            if feitos % 8 == 0 {
                progresso(feitos);
            }
        }
        if motivo == Motivo::FimDoVideo && idx >= idx0 + op.max_quadros as usize && idx < times.len() - 1 {
            motivo = Motivo::Limite;
        }
    } else {
        drop(r0);
        let limite = idx0.saturating_sub(op.max_quadros as usize);
        let mut topo = idx0; // próximo a processar é topo-1
        'b: while topo > limite {
            let ini = topo.saturating_sub(CHUNK).max(limite);
            let mut r = FrameReader::open_from(video, inicio(ini), sw, sh)?;
            let mut bloco = Vec::with_capacity(topo - ini);
            for _ in ini..topo {
                match ler(&mut r)? {
                    Some(f) => bloco.push(f),
                    None => break,
                }
            }
            for (k, img) in bloco.iter().enumerate().rev() {
                if cancel.load(Ordering::Relaxed) {
                    return Err(SicroError::Validation("cancelado".into()));
                }
                if !um(&mut seg, img, ini + k, &mut saida) {
                    break 'b;
                }
                feitos += 1;
                if feitos % 8 == 0 {
                    progresso(feitos);
                }
            }
            topo = ini;
        }
        if motivo == Motivo::FimDoVideo && topo > 0 && topo <= limite {
            motivo = Motivo::Limite;
        }
        saida.reverse();
    }
    progresso(feitos);
    Ok(Resultado { amostras: saida, parou_em, motivo, conf_media: if n > 0 { soma / n as f32 } else { 0.0 } })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Textura repetível sem estrutura simples (hash por pixel + suavização leve).
    fn ruido(x: i32, y: i32, sem: u32) -> f32 {
        let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ sem.wrapping_mul(2246822519);
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        ((h ^ (h >> 16)) & 255) as f32
    }

    /// Fundo fixo + um bloco texturizado (textura própria, presa ao bloco) no canto (bx, by).
    fn cena(bx: i32, by: i32, visivel: bool) -> Cinza {
        let (w, h) = (320usize, 240usize);
        let mut d = vec![0u8; w * h];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let mut v = 0.5 * ruido(x / 2, y / 2, 1) + 40.0;
                if visivel && x >= bx && x < bx + 40 && y >= by && y < by + 60 {
                    v = ruido(x - bx, y - by, 7);
                }
                d[(y as usize) * w + x as usize] = v as u8;
            }
        }
        Cinza { w, h, d }
    }

    #[test]
    fn segue_o_bloco_que_anda_e_para_quando_some() {
        let mut s = Seguidor::novo(&cena(50, 80, true), 50.0, 80.0, 40.0, 60.0).unwrap();
        for k in 1..=60 {
            let (bx, by) = (50 + 3 * k, 80 + (k / 4));
            let (x, y, c) = s.passo(&cena(bx, by, true), 0.5).unwrap_or_else(|| panic!("perdeu no quadro {k}"));
            assert!((x - bx as f32).abs() <= 1.0 && (y - by as f32).abs() <= 1.0, "quadro {k}: ({x},{y}) vs ({bx},{by})");
            assert!(c > 0.9);
        }
        // o bloco some: sobra só o fundo
        assert!(s.passo(&cena(0, 0, false), 0.5).is_none());
    }

    #[test]
    fn caixa_lisa_ou_minuscula_nao_e_seguida() {
        let lisa = Cinza { w: 100, h: 100, d: vec![90; 10000] };
        assert!(Seguidor::novo(&lisa, 10.0, 10.0, 30.0, 30.0).is_none());
        assert!(Seguidor::novo(&cena(50, 80, true), 50.0, 80.0, 4.0, 4.0).is_none());
    }
}
