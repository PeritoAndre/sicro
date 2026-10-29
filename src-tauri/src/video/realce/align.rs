//! Alinhamento de recortes de quadros: correlação de fase (deslocamento inteiro + parábola
//! para o subpixel) e refinamento por Gauss-Newton/LM, em translação (2 parâmetros) ou
//! homografia (8). Tudo em Rust puro (rustfft + nalgebra), sem OpenCV.

use nalgebra::{DMatrix, DVector};
use rustfft::{num_complex::Complex, FftPlanner};

/// Imagem em tons de cinza, linha a linha.
#[derive(Debug, Clone)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub d: Vec<f32>,
}

impl Gray {
    pub fn de_rgb(rgb: &[u8], w: usize, h: usize) -> Self {
        let mut d = Vec::with_capacity(w * h);
        for p in rgb.chunks_exact(3) {
            d.push(0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32);
        }
        Self { w, h, d }
    }

    /// Amostra bilinear; `None` fora da imagem.
    pub fn amostra(&self, x: f32, y: f32) -> Option<f32> {
        if x < 0.0 || y < 0.0 || x > (self.w - 1) as f32 || y > (self.h - 1) as f32 {
            return None;
        }
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let g = |xx: usize, yy: usize| self.d[yy * self.w + xx];
        let a = g(x0, y0) * (1.0 - fx) + g(x1, y0) * fx;
        let b = g(x0, y1) * (1.0 - fx) + g(x1, y1) * fx;
        Some(a * (1.0 - fy) + b * fy)
    }

    /// Borrão gaussiano separável (suaviza o ruído antes de alinhar).
    pub fn borrar(&self, sigma: f32) -> Gray {
        if sigma <= 0.0 {
            return self.clone();
        }
        let r = (sigma * 3.0).ceil() as i32;
        let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let s: f32 = k.iter().sum();
        let k: Vec<f32> = k.iter().map(|v| v / s).collect();
        let (w, h) = (self.w as i32, self.h as i32);
        let mut tmp = vec![0.0f32; self.d.len()];
        for y in 0..h {
            for x in 0..w {
                let mut a = 0.0;
                for (i, kv) in k.iter().enumerate() {
                    let xx = (x + i as i32 - r).clamp(0, w - 1);
                    a += kv * self.d[(y * w + xx) as usize];
                }
                tmp[(y * w + x) as usize] = a;
            }
        }
        let mut out = vec![0.0f32; self.d.len()];
        for y in 0..h {
            for x in 0..w {
                let mut a = 0.0;
                for (i, kv) in k.iter().enumerate() {
                    let yy = (y + i as i32 - r).clamp(0, h - 1);
                    a += kv * tmp[(yy * w + x) as usize];
                }
                out[(y * w + x) as usize] = a;
            }
        }
        Gray { w: self.w, h: self.h, d: out }
    }
}

fn fft2(buf: &mut [Complex<f32>], w: usize, h: usize, inverso: bool, planner: &mut FftPlanner<f32>) {
    let (fr, fc) = if inverso {
        (planner.plan_fft_inverse(w), planner.plan_fft_inverse(h))
    } else {
        (planner.plan_fft_forward(w), planner.plan_fft_forward(h))
    };
    for row in buf.chunks_exact_mut(w) {
        fr.process(row);
    }
    let mut col = vec![Complex::new(0.0, 0.0); h];
    for x in 0..w {
        for y in 0..h {
            col[y] = buf[y * w + x];
        }
        fc.process(&mut col);
        for y in 0..h {
            buf[y * w + x] = col[y];
        }
    }
}

fn janela(g: &Gray) -> Vec<Complex<f32>> {
    let media = g.d.iter().sum::<f32>() / g.d.len() as f32;
    let han = |i: usize, n: usize| if n < 2 { 1.0 } else { 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (n - 1) as f32).cos()) };
    let mut v = Vec::with_capacity(g.d.len());
    for y in 0..g.h {
        let wy = han(y, g.h);
        for x in 0..g.w {
            v.push(Complex::new((g.d[y * g.w + x] - media) * wy * han(x, g.w), 0.0));
        }
    }
    v
}

/// Deslocamento `(dx, dy)` tal que `cur(x) ≈ ref(x − d)`, em pixels com fração; `max` limita a
/// busca. Devolve também o pico normalizado (0..1, confiança da correlação).
pub fn correlacao_de_fase(refe: &Gray, cur: &Gray, max: usize) -> (f32, f32, f32) {
    let (w, h) = (refe.w, refe.h);
    let mut planner = FftPlanner::<f32>::new();
    let mut a = janela(refe);
    let mut b = janela(cur);
    fft2(&mut a, w, h, false, &mut planner);
    fft2(&mut b, w, h, false, &mut planner);
    let mut c: Vec<Complex<f32>> = a
        .iter()
        .zip(&b)
        .map(|(fa, fb)| {
            let p = fb * fa.conj();
            // fase pura é frágil com ruído: peso intermediário (|F|^0,5 em vez de |F|^0)
            let n = p.norm().sqrt();
            if n > 1e-9 { p / n } else { Complex::new(0.0, 0.0) }
        })
        .collect();
    fft2(&mut c, w, h, true, &mut planner);
    let norm = (w * h) as f32;
    let val = |x: i64, y: i64| c[(y.rem_euclid(h as i64) as usize) * w + x.rem_euclid(w as i64) as usize].re / norm;
    let m = max as i64;
    let (mut melhor, mut bx, mut by) = (f32::MIN, 0i64, 0i64);
    for dy in -m.min(h as i64 / 2 - 1)..=m.min(h as i64 / 2 - 1) {
        for dx in -m.min(w as i64 / 2 - 1)..=m.min(w as i64 / 2 - 1) {
            let v = val(dx, dy);
            if v > melhor {
                (melhor, bx, by) = (v, dx, dy);
            }
        }
    }
    let parab = |l: f32, c0: f32, r: f32| {
        let den = l - 2.0 * c0 + r;
        if den.abs() < 1e-12 { 0.0 } else { (0.5 * (l - r) / den).clamp(-0.5, 0.5) }
    };
    let fx = parab(val(bx - 1, by), melhor, val(bx + 1, by));
    let fy = parab(val(bx, by - 1), melhor, val(bx, by + 1));
    (bx as f32 + fx, by as f32 + fy, melhor.clamp(0.0, 1.0))
}

/// Mapa de coordenadas da referência para as do quadro atual: homografia
/// `[[1+p0, p2, p4], [p1, 1+p3, p5], [p6, p7, 1]]` (p = 0 é a identidade).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Warp {
    pub p: [f64; 8],
}

impl Warp {
    pub fn identidade() -> Self {
        Self { p: [0.0; 8] }
    }
    pub fn translacao(tx: f64, ty: f64) -> Self {
        let mut p = [0.0; 8];
        p[4] = tx;
        p[5] = ty;
        Self { p }
    }
    pub fn aplica(&self, x: f64, y: f64) -> (f64, f64) {
        let p = &self.p;
        let s = 1.0 + p[6] * x + p[7] * y;
        (((1.0 + p[0]) * x + p[2] * y + p[4]) / s, (p[1] * x + (1.0 + p[3]) * y + p[5]) / s)
    }
}

/// Refina `warp` minimizando a soma dos quadrados das diferenças dentro de `roi`
/// (x, y, w, h no sistema do recorte). `homografia` libera os 8 parâmetros; senão só a translação.
/// Devolve o warp e o erro médio quadrático final.
pub fn refinar(refe: &Gray, cur: &Gray, roi: (usize, usize, usize, usize), inicial: Warp, homografia: bool, iter: usize) -> (Warp, f32) {
    let ativos: Vec<usize> = if homografia { (0..8).collect() } else { vec![4, 5] };
    let n = ativos.len();
    let (rx, ry, rw, rh) = roi;
    let mut warp = inicial;
    let custo = |wp: &Warp| -> (f64, usize) {
        let (mut s, mut c) = (0.0f64, 0usize);
        for y in ry..ry + rh {
            for x in rx..rx + rw {
                let (u, v) = wp.aplica(x as f64, y as f64);
                if let Some(i) = cur.amostra(u as f32, v as f32) {
                    let e = (i - refe.d[y * refe.w + x]) as f64;
                    s += e * e;
                    c += 1;
                }
            }
        }
        (s / c.max(1) as f64, c)
    };
    let (mut atual, _) = custo(&warp);
    let mut lambda = 1e-3f64;
    for _ in 0..iter {
        let mut hh = DMatrix::<f64>::zeros(n, n);
        let mut g = DVector::<f64>::zeros(n);
        let mut cont = 0usize;
        for y in ry..ry + rh {
            for x in rx..rx + rw {
                let (xf, yf) = (x as f64, y as f64);
                let p = &warp.p;
                let s = 1.0 + p[6] * xf + p[7] * yf;
                let (u, v) = warp.aplica(xf, yf);
                let (uf, vf) = (u as f32, v as f32);
                let Some(i0) = cur.amostra(uf, vf) else { continue };
                let (Some(ixp), Some(ixm), Some(iyp), Some(iym)) =
                    (cur.amostra(uf + 1.0, vf), cur.amostra(uf - 1.0, vf), cur.amostra(uf, vf + 1.0), cur.amostra(uf, vf - 1.0))
                else {
                    continue;
                };
                let (ix, iy) = (((ixp - ixm) * 0.5) as f64, ((iyp - iym) * 0.5) as f64);
                let e = (i0 - refe.d[y * refe.w + x]) as f64;
                let du = [xf / s, 0.0, yf / s, 0.0, 1.0 / s, 0.0, -u * xf / s, -u * yf / s];
                let dv = [0.0, xf / s, 0.0, yf / s, 0.0, 1.0 / s, -v * xf / s, -v * yf / s];
                let j: Vec<f64> = ativos.iter().map(|&k| ix * du[k] + iy * dv[k]).collect();
                for a in 0..n {
                    g[a] += j[a] * e;
                    for b in 0..n {
                        hh[(a, b)] += j[a] * j[b];
                    }
                }
                cont += 1;
            }
        }
        if cont < n * 4 {
            break;
        }
        let mut melhorou = false;
        for _ in 0..6 {
            let mut m = hh.clone();
            for a in 0..n {
                m[(a, a)] += lambda * (1.0 + hh[(a, a)]);
            }
            let Some(sol) = m.lu().solve(&(-&g)) else {
                lambda *= 10.0;
                continue;
            };
            let mut novo = warp;
            for (a, &k) in ativos.iter().enumerate() {
                novo.p[k] += sol[a];
            }
            let (c, _) = custo(&novo);
            if c < atual {
                let passo = sol.norm();
                warp = novo;
                atual = c;
                lambda = (lambda / 3.0).max(1e-9);
                melhorou = true;
                if passo < 1e-4 {
                    return (warp, atual as f32);
                }
                break;
            }
            lambda *= 10.0;
        }
        if !melhorou {
            break;
        }
    }
    (warp, atual as f32)
}

/// Correlação cruzada normalizada (−1..1) entre dois vetores do mesmo tamanho.
pub fn ncc(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let (ma, mb) = (a.iter().take(n).sum::<f32>() / n as f32, b.iter().take(n).sum::<f32>() / n as f32);
    let (mut sab, mut saa, mut sbb) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let (x, y) = ((a[i] - ma) as f64, (b[i] - mb) as f64);
        sab += x * y;
        saa += x * x;
        sbb += y * y;
    }
    if saa < 1e-9 || sbb < 1e-9 { 0.0 } else { (sab / (saa * sbb).sqrt()) as f32 }
}
