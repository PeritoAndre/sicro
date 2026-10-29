//! ByteTrack (MIT, Zhang et al. 2022) reescrito em Rust: filtro de Kalman de
//! velocidade constante + associação por IoU em duas passagens (detecções fortes,
//! depois as fracas), com o mesmo número por pessoa enquanto a trilha viver.
//! O tempo aqui é o índice do quadro ANALISADO (o chamador cuida do passo e do `max_lost`).

use nalgebra::{SMatrix, SVector};

use super::yolox::Det;

type V8 = SVector<f64, 8>;
type M8 = SMatrix<f64, 8, 8>;
type V4 = SVector<f64, 4>;
type M4 = SMatrix<f64, 4, 4>;
type M48 = SMatrix<f64, 4, 8>;

const W_POS: f64 = 1.0 / 20.0;
const W_VEL: f64 = 1.0 / 160.0;

/// Caixa (x, y, w, h) -> medida (cx, cy, razão w/h, h).
fn to_xyah(d: &Det) -> V4 {
    let h = d.h.max(1e-3) as f64;
    V4::new((d.x + d.w / 2.0) as f64, (d.y + d.h / 2.0) as f64, d.w as f64 / h, h)
}

#[derive(Clone)]
struct Kalman {
    mean: V8,
    cov: M8,
}

impl Kalman {
    fn new(m: V4) -> Self {
        let h = m[3];
        let mut mean = V8::zeros();
        mean.fixed_rows_mut::<4>(0).copy_from(&m);
        let std = [
            2.0 * W_POS * h, 2.0 * W_POS * h, 1e-2, 2.0 * W_POS * h,
            10.0 * W_VEL * h, 10.0 * W_VEL * h, 1e-5, 10.0 * W_VEL * h,
        ];
        let cov = M8::from_diagonal(&V8::from_iterator(std.iter().map(|s| s * s)));
        Self { mean, cov }
    }

    fn f() -> M8 {
        let mut f = M8::identity();
        for i in 0..4 {
            f[(i, 4 + i)] = 1.0;
        }
        f
    }

    fn predict(&mut self) {
        let h = self.mean[3].max(1e-3);
        let std = [W_POS * h, W_POS * h, 1e-2, W_POS * h, W_VEL * h, W_VEL * h, 1e-5, W_VEL * h];
        let q = M8::from_diagonal(&V8::from_iterator(std.iter().map(|s| s * s)));
        let f = Self::f();
        self.mean = f * self.mean;
        self.cov = f * self.cov * f.transpose() + q;
    }

    fn update(&mut self, m: V4) {
        let h = self.mean[3].max(1e-3);
        let r = M4::from_diagonal(&V4::new(
            (W_POS * h).powi(2),
            (W_POS * h).powi(2),
            1e-2,
            (W_POS * h).powi(2),
        ));
        let mut hm = M48::zeros();
        for i in 0..4 {
            hm[(i, i)] = 1.0;
        }
        let s = hm * self.cov * hm.transpose() + r;
        let Some(sinv) = s.try_inverse() else { return };
        let k = self.cov * hm.transpose() * sinv;
        self.mean += k * (m - hm * self.mean);
        self.cov -= k * s * k.transpose();
    }

    fn det(&self, score: f32) -> Det {
        let (cx, cy, a, h) = (self.mean[0], self.mean[1], self.mean[2], self.mean[3]);
        let w = a * h;
        Det { x: (cx - w / 2.0) as f32, y: (cy - h / 2.0) as f32, w: w as f32, h: h as f32, score, classe: 0 }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Tracked,
    Lost,
}

struct Track {
    id: u32,
    kf: Kalman,
    state: State,
    activated: bool,
    score: f32,
    last_frame: u32,
    /// 1ª aparição de uma trilha ainda não confirmada (entra no histórico ao confirmar).
    pending: Option<Sample>,
}

/// Ponto da trilha: índice do quadro analisado e caixa em pixels do original.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub frame: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub struct Params {
    /// Detecção "forte" (cria e mantém a trilha).
    pub high: f32,
    /// Piso das fracas (recuperam trilhas em oclusão parcial).
    pub low: f32,
    pub match_thr: f32,
    /// Quadros analisados que uma trilha perdida espera antes de morrer.
    pub max_lost: u32,
}

impl Default for Params {
    fn default() -> Self {
        Self { high: 0.5, low: 0.1, match_thr: 0.8, max_lost: 30 }
    }
}

pub struct ByteTrack {
    p: Params,
    frame: u32,
    next_id: u32,
    tracks: Vec<Track>,
    pub history: std::collections::BTreeMap<u32, Vec<Sample>>,
}

impl ByteTrack {
    pub fn new(p: Params) -> Self {
        Self { p, frame: 0, next_id: 1, tracks: Vec::new(), history: Default::default() }
    }

    /// Processa as detecções de UM quadro analisado; devolve (id, caixa) das trilhas ativas nele.
    pub fn update(&mut self, dets: &[Det]) -> Vec<(u32, Det)> {
        self.frame += 1;
        let frame = self.frame;
        let high: Vec<usize> = (0..dets.len()).filter(|&i| dets[i].score >= self.p.high).collect();
        let low: Vec<usize> = (0..dets.len())
            .filter(|&i| dets[i].score > self.p.low && dets[i].score < self.p.high)
            .collect();

        for t in &mut self.tracks {
            t.kf.predict();
        }
        let pool: Vec<usize> = (0..self.tracks.len()).filter(|&i| self.tracks[i].activated).collect();
        let unconf: Vec<usize> = (0..self.tracks.len()).filter(|&i| !self.tracks[i].activated).collect();
        let mut matched_det = vec![false; dets.len()];
        let mut touched: Vec<usize> = Vec::new();

        // 1ª passagem: fortes x (rastreadas + perdidas)
        let cost = self.cost(&pool, &high, dets, true);
        let (m1, um_tracks, um_dets) = associate(&cost, high.len(), self.p.match_thr);
        for (ti, di) in m1 {
            let (t, d) = (pool[ti], high[di]);
            self.apply(t, &dets[d], frame);
            matched_det[d] = true;
            touched.push(t);
        }
        // 2ª passagem: fracas x rastreadas que sobraram
        let rem: Vec<usize> = um_tracks
            .iter()
            .map(|&i| pool[i])
            .filter(|&t| self.tracks[t].state == State::Tracked)
            .collect();
        let cost2 = self.cost(&rem, &low, dets, false);
        let (m2, um2, _) = associate(&cost2, low.len(), 0.5);
        for (ti, di) in m2 {
            let (t, d) = (rem[ti], low[di]);
            self.apply(t, &dets[d], frame);
            touched.push(t);
        }
        for i in um2 {
            self.tracks[rem[i]].state = State::Lost;
        }
        // não confirmadas x fortes que sobraram
        let rest: Vec<usize> = um_dets.iter().map(|&i| high[i]).collect();
        let cost3 = self.cost(&unconf, &rest, dets, true);
        let (m3, um3, um_d3) = associate(&cost3, rest.len(), 0.7);
        let mut dead: Vec<usize> = Vec::new();
        for (ti, di) in m3 {
            let (t, d) = (unconf[ti], rest[di]);
            self.apply(t, &dets[d], frame);
            matched_det[d] = true;
            touched.push(t);
        }
        for i in um3 {
            dead.push(unconf[i]);
        }
        // trilhas novas: fortes sem par e com folga sobre o limiar
        for di in um_d3 {
            let d = rest[di];
            if dets[d].score < self.p.high + 0.1 && frame > 1 {
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            self.tracks.push(Track {
                id,
                kf: Kalman::new(to_xyah(&dets[d])),
                state: State::Tracked,
                activated: frame == 1,
                score: dets[d].score,
                last_frame: frame,
                pending: (frame > 1).then(|| Sample {
                    frame, x: dets[d].x, y: dets[d].y, w: dets[d].w, h: dets[d].h, score: dets[d].score,
                }),
            });
            let t = self.tracks.len() - 1;
            touched.push(t);
        }
        // perdidas demais morrem
        let max_lost = self.p.max_lost;
        for (i, t) in self.tracks.iter().enumerate() {
            if t.state == State::Lost && frame - t.last_frame > max_lost {
                dead.push(i);
            }
        }
        // saída do quadro (antes de remover)
        let mut out = Vec::new();
        for &i in &touched {
            let t = &self.tracks[i];
            if t.activated && t.state == State::Tracked && t.last_frame == frame {
                let d = t.kf.det(t.score);
                out.push((t.id, d));
                self.history.entry(t.id).or_default().push(Sample {
                    frame, x: d.x, y: d.y, w: d.w, h: d.h, score: t.score,
                });
            }
        }
        dead.sort_unstable();
        dead.dedup();
        for i in dead.into_iter().rev() {
            self.tracks.remove(i);
        }
        out
    }

    fn apply(&mut self, t: usize, d: &Det, frame: u32) {
        let tr = &mut self.tracks[t];
        if let Some(s) = tr.pending.take() {
            self.history.entry(tr.id).or_default().push(s);
        }
        tr.kf.update(to_xyah(d));
        tr.state = State::Tracked;
        tr.activated = true;
        tr.score = d.score;
        tr.last_frame = frame;
    }

    /// Custo 1 − IoU (com a pontuação fundida, como no ByteTrack original, nas fortes).
    fn cost(&self, tracks: &[usize], dets_idx: &[usize], dets: &[Det], fuse: bool) -> Vec<Vec<f32>> {
        tracks
            .iter()
            .map(|&t| {
                let pb = self.tracks[t].kf.det(0.0);
                dets_idx
                    .iter()
                    .map(|&d| {
                        let iou = pb.iou(&dets[d]);
                        1.0 - if fuse { iou * dets[d].score } else { iou }
                    })
                    .collect()
            })
            .collect()
    }
}

/// Atribuição ótima; pares com custo > `thr` ficam de fora.
/// Devolve (pares, linhas sem par, colunas sem par).
fn associate(cost: &[Vec<f32>], n_cols: usize, thr: f32) -> (Vec<(usize, usize)>, Vec<usize>, Vec<usize>) {
    let n = cost.len();
    let m = n_cols;
    if n == 0 || m == 0 {
        return (vec![], (0..n).collect(), (0..m).collect());
    }
    let assign = hungarian(cost);
    let mut pairs = Vec::new();
    let mut row_used = vec![false; n];
    let mut col_used = vec![false; m];
    for (r, c) in assign.into_iter().enumerate() {
        if let Some(c) = c {
            if cost[r][c] <= thr {
                pairs.push((r, c));
                row_used[r] = true;
                col_used[c] = true;
            }
        }
    }
    (
        pairs,
        (0..n).filter(|&i| !row_used[i]).collect(),
        (0..m).filter(|&j| !col_used[j]).collect(),
    )
}

/// Húngaro (potenciais, O(n²m)) para matriz retangular; linhas ↔ colunas.
fn hungarian(cost: &[Vec<f32>]) -> Vec<Option<usize>> {
    let n0 = cost.len();
    let m0 = cost[0].len();
    let transposed = n0 > m0;
    let (n, m) = if transposed { (m0, n0) } else { (n0, m0) };
    let c = |i: usize, j: usize| -> f64 {
        (if transposed { cost[j][i] } else { cost[i][j] }) as f64
    };
    let inf = f64::INFINITY;
    let mut u = vec![0.0; n + 1];
    let mut v = vec![0.0; m + 1];
    let mut p = vec![0usize; m + 1];
    let mut way = vec![0usize; m + 1];
    for i in 1..=n {
        p[0] = i;
        let mut j0 = 0usize;
        let mut minv = vec![inf; m + 1];
        let mut used = vec![false; m + 1];
        loop {
            used[j0] = true;
            let i0 = p[j0];
            let mut delta = inf;
            let mut j1 = 0usize;
            for j in 1..=m {
                if !used[j] {
                    let cur = c(i0 - 1, j - 1) - u[i0] - v[j];
                    if cur < minv[j] {
                        minv[j] = cur;
                        way[j] = j0;
                    }
                    if minv[j] < delta {
                        delta = minv[j];
                        j1 = j;
                    }
                }
            }
            for j in 0..=m {
                if used[j] {
                    u[p[j]] += delta;
                    v[j] -= delta;
                } else {
                    minv[j] -= delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut res = vec![None; n0];
    for j in 1..=m {
        if p[j] != 0 {
            let (r, col) = (p[j] - 1, j - 1);
            if transposed {
                res[col] = Some(r);
            } else {
                res[r] = Some(col);
            }
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(x: f32, y: f32, score: f32) -> Det {
        Det { x, y, w: 40.0, h: 100.0, score, classe: 0 }
    }

    #[test]
    fn hungarian_acha_o_minimo() {
        let c = vec![vec![4.0, 1.0, 3.0], vec![2.0, 0.0, 5.0], vec![3.0, 2.0, 2.0]];
        let a = hungarian(&c);
        let total: f32 = a.iter().enumerate().map(|(r, c0)| c[r][c0.unwrap()]).sum();
        assert_eq!(total, 5.0);
        // retangular
        let c = vec![vec![1.0, 9.0], vec![9.0, 1.0], vec![5.0, 5.0]];
        let a = hungarian(&c);
        assert_eq!(a[0], Some(0));
        assert_eq!(a[1], Some(1));
        assert_eq!(a[2], None);
    }

    /// Duas pessoas andando em sentidos opostos, cruzando no meio: números não trocam.
    #[test]
    fn cruzamento_mantem_os_numeros() {
        let mut bt = ByteTrack::new(Params::default());
        let mut ids_a = vec![];
        let mut ids_b = vec![];
        for f in 0..40 {
            let xa = 100.0 + f as f32 * 12.0; // vai para a direita
            let xb = 560.0 - f as f32 * 12.0; // vai para a esquerda
            let out = bt.update(&[det(xa, 100.0, 0.9), det(xb, 104.0, 0.85)]);
            for (id, d) in out {
                if (d.x - xa).abs() < 30.0 && (d.x - xb).abs() > (d.x - xa).abs() {
                    ids_a.push(id);
                } else {
                    ids_b.push(id);
                }
            }
        }
        // cada uma segue um único número ao longo do vídeo, e eles são diferentes
        ids_a.dedup();
        ids_b.dedup();
        assert_eq!(bt.history.len(), 2, "só duas trilhas: {:?}", bt.history.keys().collect::<Vec<_>>());
        assert_eq!(ids_a.len(), 1, "A trocou de número: {ids_a:?}");
        assert_ne!(ids_a[0], ids_b[0]);
    }

    /// Oclusão curta (3 quadros sem detecção): volta com o mesmo número.
    #[test]
    fn oclusao_curta_mantem_o_numero() {
        let mut bt = ByteTrack::new(Params::default());
        let mut ids = vec![];
        for f in 0..30 {
            let x = 100.0 + f as f32 * 6.0;
            let dets = if (10..13).contains(&f) { vec![] } else { vec![det(x, 100.0, 0.9)] };
            for (id, _) in bt.update(&dets) {
                ids.push(id);
            }
        }
        ids.dedup();
        assert_eq!(ids.len(), 1, "{ids:?}");
        // e a trilha não tem amostra nos quadros ocultos
        let h = &bt.history[&ids[0]];
        assert_eq!(h.len(), 27);
    }

    /// Entrada tardia e saída: a 2ª pessoa só nasce quando aparece e morre depois do prazo.
    #[test]
    fn entrada_e_saida() {
        let mut bt = ByteTrack::new(Params { max_lost: 5, ..Params::default() });
        for f in 0..40 {
            let mut dets = vec![det(100.0, 100.0, 0.9)];
            if (10..20).contains(&f) {
                dets.push(det(400.0, 100.0, 0.9));
            }
            bt.update(&dets);
        }
        assert_eq!(bt.history.len(), 2);
        let b = &bt.history[&2];
        assert_eq!(b.first().unwrap().frame, 11); // confirmada no 2º quadro em que aparece
        assert_eq!(b.last().unwrap().frame, 20);
        assert!(bt.tracks.iter().all(|t| t.id != 2), "a trilha 2 devia ter sido removida");
    }

    #[test]
    fn detecao_fraca_nao_cria_trilha() {
        let mut bt = ByteTrack::new(Params::default());
        for _ in 0..10 {
            bt.update(&[det(100.0, 100.0, 0.3)]);
        }
        assert!(bt.history.is_empty());
    }
}
