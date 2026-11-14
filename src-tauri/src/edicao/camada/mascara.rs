//! Máscaras (§5.5): cinza 8 bits, branco = aplica; borda suave cresce PARA FORA. Dono: P0.5.
//!
//! Valor de cada pixel: 255 quando o bloco 2×2 de posição par (no espaço) que o contém toca a forma; o croma do
//! 4:2:0 é por bloco, então nada do croma do que é ocultado vaza na borda. Fora disso, com borda suave
//! B = `borda_suave` × lado menor da forma (px): 255·(1 − suave(d/B)) pela distância d do centro do pixel à forma,
//! e 0 além de B. Toda a forma (a caixa mais a margem) fica 255; o gradiente começa depois dela (E14).
//! `inverter` troca na origem (255 − valor). Várias regiões: vale o maior valor.
//! Ocultação invertida (desfoque, mosaico, sólido): o que se esconde é o lado de FORA, então é ele que fica todo
//! em 255, com os blocos 2×2 que a borda corta, e o gradiente corre para DENTRO da forma (o complemento simples
//! deixava aparecer, a até B px da forma, o que o perito mandou esconder). No holofote vale o complemento.

use super::formas::{envolvente_forma, poligonos, ModoRegiao, Primitiva};
use super::Bitmap;
use crate::edicao::curva;
use crate::edicao::geometria;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{Caixa, FormaLupa, TamanhoPx};
use crate::error::{Result, SicroError};

/// Folga (px) contra o ruído de ponto flutuante: forma que só encosta no bloco não o cobre.
const FOLGA: f64 = 1e-6;

enum Geo {
    /// Centro e semieixos em px.
    Elipse { cx: f64, cy: f64, rx: f64, ry: f64 },
    /// Polígonos fechados em px; `par_impar` = regra par-ímpar (senão, não zero).
    Poligonos { p: Vec<Vec<(f64, f64)>>, par_impar: bool },
}

struct Regiao {
    geo: Geo,
    /// Borda suave (px).
    b: f64,
    inverter: bool,
    /// Ocultação invertida: fora da forma tudo coberto, gradiente para dentro.
    para_dentro: bool,
    /// Envolvente da forma (px): x0, y0, x1, y1.
    env: (f64, f64, f64, f64),
}

fn invalido(detalhe: &str) -> SicroError {
    SicroError::Validation(format!("{}: {detalhe}", codigo::ITEM_INVALIDO))
}

fn regiao(p: &Primitiva, w: f64, h: f64) -> Result<Option<Regiao>> {
    let Primitiva::Regiao { forma, borda_suave, inverter, modo, .. } = p else { return Ok(None) };
    let Some(env) = envolvente_forma(forma, w, h) else { return Err(invalido("forma da regiao")) };
    // número não finito daria máscara vazia calada (nada oculto): recusa
    let finita = match forma.as_ref() {
        Primitiva::Elipse { cx, cy, rx, ry, .. } => [cx, cy, rx, ry].iter().all(|v| v.is_finite()),
        Primitiva::Caminho { d, .. } => poligonos(d, w, h).iter().flatten().all(|p| p.0.is_finite() && p.1.is_finite()),
        _ => false,
    };
    if !finita || ![env.0, env.1, env.2, env.3].iter().all(|v| v.is_finite()) {
        return Err(invalido("regiao não finita"));
    }
    let geo = match forma.as_ref() {
        Primitiva::Elipse { cx, cy, rx, ry, .. } => {
            let (cx, cy, rx, ry) = (cx * w, cy * h, rx.abs() * w, ry.abs() * h);
            if rx > 0.0 && ry > 0.0 {
                Geo::Elipse { cx, cy, rx, ry }
            } else {
                // degenerada: o segmento que sobrou
                Geo::Poligonos { p: vec![vec![(cx - rx, cy - ry), (cx + rx, cy + ry)]], par_impar: false }
            }
        }
        Primitiva::Caminho { d, regra, .. } => Geo::Poligonos { p: poligonos(d, w, h), par_impar: regra.is_some() },
        _ => return Err(invalido("forma da regiao")),
    };
    let lado = (env.2 - env.0).min(env.3 - env.1);
    let b = if borda_suave.is_finite() { borda_suave.max(0.0) * lado } else { 0.0 };
    let para_dentro = *inverter && *modo != ModoRegiao::Escurecer;
    Ok(Some(Regiao { geo, b, inverter: *inverter, para_dentro, env }))
}

impl Regiao {
    /// O quadrado [x0, x0 + l] × [y0, y0 + l] toca a forma (interseção com área)?
    fn toca(&self, x0: f64, y0: f64, l: f64) -> bool {
        let (a0, a1, b0, b1) = (x0 + FOLGA, x0 + l - FOLGA, y0 + FOLGA, y0 + l - FOLGA);
        match &self.geo {
            Geo::Elipse { cx, cy, rx, ry } => {
                // no espaço da elipse unitária o quadrado vira retângulo: ponto dele mais perto do centro
                let perto = |a: f64, b: f64| if a > 0.0 { a } else if b < 0.0 { b } else { 0.0 };
                let qx = perto((a0 - cx) / rx, (a1 - cx) / rx);
                let qy = perto((b0 - cy) / ry, (b1 - cy) / ry);
                qx * qx + qy * qy < 1.0
            }
            Geo::Poligonos { p, par_impar } => {
                dentro(p, *par_impar, x0 + l * 0.5, y0 + l * 0.5)
                    || arestas(p).any(|(a, b)| segmento_toca(a, b, a0, b0, a1, b1))
            }
        }
    }

    /// O quadrado [x0, x0 + l] × [y0, y0 + l] está inteiro dentro da forma?
    fn contem(&self, x0: f64, y0: f64, l: f64) -> bool {
        match &self.geo {
            Geo::Elipse { cx, cy, rx, ry } => [(x0, y0), (x0 + l, y0), (x0, y0 + l), (x0 + l, y0 + l)]
                .iter()
                .all(|(x, y)| ((x - cx) / rx) * ((x - cx) / rx) + ((y - cy) / ry) * ((y - cy) / ry) <= 1.0),
            Geo::Poligonos { p, par_impar } => {
                dentro(p, *par_impar, x0 + l * 0.5, y0 + l * 0.5)
                    && !arestas(p).any(|(a, b)| segmento_toca(a, b, x0 + FOLGA, y0 + FOLGA, x0 + l - FOLGA, y0 + l - FOLGA))
            }
        }
    }

    /// Distância (px) de um ponto de dentro até a borda da forma.
    fn profundidade(&self, x: f64, y: f64) -> f64 {
        match &self.geo {
            Geo::Elipse { cx, cy, rx, ry } => {
                let (px, py) = (x - cx, y - cy);
                if rx == ry {
                    (rx - (px * px + py * py).sqrt()).max(0.0)
                } else {
                    dist_elipse(px, py, *rx, *ry)
                }
            }
            Geo::Poligonos { p, .. } => arestas(p).map(|(a, b)| dist2_segmento((x, y), a, b)).fold(f64::INFINITY, f64::min).sqrt(),
        }
    }

    /// Valor (0 a 255) da ocultação invertida no pixel (x, y): 255 onde o bloco não está inteiro dentro da forma;
    /// dentro, o gradiente cai de 255 na borda a 0 em B px de profundidade.
    fn valor_para_dentro(&self, x: u32, y: u32, bloco_dentro: bool) -> u8 {
        if !bloco_dentro {
            return 255;
        }
        if self.b.is_nan() || self.b <= 0.0 {
            return 0;
        }
        let d = self.profundidade(x as f64 + 0.5, y as f64 + 0.5);
        if d.is_nan() {
            return 255;
        }
        if d >= self.b {
            return 0;
        }
        (255.0 * (1.0 - curva::suave(d / self.b)) + 0.5).floor() as u8
    }

    /// Distância (px) do ponto à forma; 0 dentro.
    fn distancia(&self, x: f64, y: f64) -> f64 {
        match &self.geo {
            Geo::Elipse { cx, cy, rx, ry } => {
                let (px, py) = (x - cx, y - cy);
                if (px / rx) * (px / rx) + (py / ry) * (py / ry) <= 1.0 {
                    0.0
                } else {
                    dist_elipse(px, py, *rx, *ry)
                }
            }
            Geo::Poligonos { p, par_impar } => {
                if dentro(p, *par_impar, x, y) {
                    return 0.0;
                }
                arestas(p).map(|(a, b)| dist2_segmento((x, y), a, b)).fold(f64::INFINITY, f64::min).sqrt()
            }
        }
    }

    /// Valor de 0 a 255 (antes de inverter) do pixel (x, y) inteiro do espaço.
    fn valor(&self, x: u32, y: u32, bloco: bool) -> u8 {
        if bloco {
            return 255;
        }
        if self.b.is_nan() || self.b <= 0.0 {
            return 0;
        }
        let d = self.distancia(x as f64 + 0.5, y as f64 + 0.5);
        if d >= self.b {
            return 0;
        }
        (255.0 * (1.0 - curva::suave(d / self.b)) + 0.5).floor() as u8
    }
}

fn arestas(p: &[Vec<(f64, f64)>]) -> impl Iterator<Item = ((f64, f64), (f64, f64))> + '_ {
    p.iter().flat_map(|v| (0..v.len()).map(move |i| (v[i], v[(i + 1) % v.len()])))
}

/// Ponto dentro dos polígonos (não zero ou par-ímpar).
fn dentro(p: &[Vec<(f64, f64)>], par_impar: bool, x: f64, y: f64) -> bool {
    let mut voltas = 0i32;
    let mut cruzou = false;
    for (a, b) in arestas(p) {
        if (a.1 <= y) != (b.1 <= y) {
            let xc = a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1);
            if x < xc {
                cruzou = !cruzou;
                voltas += if b.1 > a.1 { 1 } else { -1 };
            }
        }
    }
    if par_impar {
        cruzou
    } else {
        voltas != 0
    }
}

/// Segmento a–b corta o retângulo [x0, x1] × [y0, y1] (Liang–Barsky)?
fn segmento_toca(a: (f64, f64), b: (f64, f64), x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, a.0 - x0), (dx, x1 - a.0), (-dy, a.1 - y0), (dy, y1 - a.1)] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return false;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return false;
                }
                t1 = t1.min(r);
            }
        }
    }
    t0 <= t1
}

fn dist2_segmento(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (ex, ey) = (a.0 + t * dx - p.0, a.1 + t * dy - p.1);
    ex * ex + ey * ey
}

/// Distância de (px, py), relativo ao centro e fora da elipse, à elipse de semieixos rx, ry: ponto mais perto
/// por iteração fixa sobre o parâmetro normalizado (sem trigonometria; 4 passos bastam para frações de px).
fn dist_elipse(px: f64, py: f64, rx: f64, ry: f64) -> f64 {
    let (px, py) = (px.abs(), py.abs());
    if rx == ry {
        return ((px * px + py * py).sqrt() - rx).max(0.0);
    }
    let (a, b) = (rx, ry);
    let (mut tx, mut ty) = (std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2);
    for _ in 0..4 {
        let (x, y) = (a * tx, b * ty);
        let ex = (a * a - b * b) * tx * tx * tx / a;
        let ey = (b * b - a * a) * ty * ty * ty / b;
        let (rx_, ry_) = (x - ex, y - ey);
        let (qx, qy) = (px - ex, py - ey);
        let r = (rx_ * rx_ + ry_ * ry_).sqrt();
        let q = (qx * qx + qy * qy).sqrt();
        if q.is_nan() || q <= 0.0 {
            break;
        }
        tx = ((qx * r / q + ex) / a).clamp(0.0, 1.0);
        ty = ((qy * r / q + ey) / b).clamp(0.0, 1.0);
        let t = (tx * tx + ty * ty).sqrt();
        if t.is_nan() || t <= 0.0 {
            break;
        }
        tx /= t;
        ty /= t;
    }
    let (dx, dy) = (px - a * tx, py - b * ty);
    (dx * dx + dy * dy).sqrt()
}

/// Máscara das primitivas `regiao` num cinza do tamanho de `caixa_px` ([x, y, w, h], px de `espaco`).
/// `espaco` = fonte exibida (máscara por quadro lido) ou canvas (por slot). `inverter` já aplicado.
/// Outras primitivas são ignoradas.
pub fn mascara(regioes: &[Primitiva], espaco: TamanhoPx, caixa_px: [u32; 4]) -> Result<Bitmap> {
    let [x0, y0, w, h] = caixa_px;
    if w == 0 || h == 0 {
        return Err(invalido("caixa_px vazia"));
    }
    let (ew, eh) = (espaco.w as f64, espaco.h as f64);
    let mut px = vec![0u8; w as usize * h as usize];
    let mut primeira = true;
    for p in regioes {
        let Some(r) = regiao(p, ew, eh)? else { continue };
        // fora desta caixa (px) o valor é 0 (antes de inverter)
        let m = r.b + 2.0;
        let limite = |v: f64, lo: u32, hi: u32| -> u32 {
            if v.is_nan() || v <= lo as f64 {
                lo
            } else if v >= hi as f64 {
                hi
            } else {
                v as u32
            }
        };
        let (ax, bx) = (limite((r.env.0 - m).floor(), x0, x0 + w), limite((r.env.2 + m).ceil(), x0, x0 + w));
        let (ay, by) = (limite((r.env.1 - m).floor(), y0, y0 + h), limite((r.env.3 + m).ceil(), y0, y0 + h));
        let fundo = if r.inverter { 255u8 } else { 0 };
        let mut v = vec![fundo; w as usize * h as usize];
        // blocos 2×2 de posição par que tocam a forma
        let bx0 = ax & !1;
        let nbx = ((bx + 1 - bx0) / 2) as usize;
        let mut linha_de_blocos = vec![false; nbx];
        let mut bloco_y = u32::MAX;
        for y in ay..by {
            if y & !1 != bloco_y {
                bloco_y = y & !1;
                for (k, t) in linha_de_blocos.iter_mut().enumerate() {
                    let xb = (bx0 + 2 * k as u32) as f64;
                    *t = if r.para_dentro { r.contem(xb, bloco_y as f64, 2.0) } else { r.toca(xb, bloco_y as f64, 2.0) };
                }
            }
            let linha = (y - y0) as usize * w as usize;
            for x in ax..bx {
                let bloco = linha_de_blocos[((x & !1) - bx0) as usize / 2];
                v[linha + (x - x0) as usize] = if r.para_dentro {
                    r.valor_para_dentro(x, y, bloco)
                } else {
                    let val = r.valor(x, y, bloco);
                    if r.inverter { 255 - val } else { val }
                };
            }
        }
        if primeira {
            px = v;
            primeira = false;
        } else {
            for (a, b) in px.iter_mut().zip(v) {
                *a = (*a).max(b);
            }
        }
    }
    Ok(Bitmap { w, h, canais: 1, px })
}

/// União, em px pares de `espaco`, das caixas (forma + margem + borda suave) das regiões de todos os quadros: [x, y, w, h].
/// Região invertida = o espaço inteiro. Inclui os blocos 2×2 de borda e 1 px de folga.
pub fn uniao_px(quadros: &[Vec<Primitiva>], espaco: TamanhoPx) -> Option<[u32; 4]> {
    let (w, h) = (espaco.w as f64, espaco.h as f64);
    let mut u: Option<(f64, f64, f64, f64)> = None;
    for p in quadros.iter().flatten() {
        let Primitiva::Regiao { forma, borda_suave, inverter, .. } = p else { continue };
        if *inverter {
            return Some(geometria::caixa_px_par(Caixa { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, espaco));
        }
        let inteiro = geometria::caixa_px_par(Caixa { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }, espaco);
        // forma sem limite conhecido: o espaço inteiro (a `mascara` recusa; pular deixaria sem máscara, calado)
        let Some((a, b, c, d)) = envolvente_forma(forma, w, h) else { return Some(inteiro) };
        let borda = if borda_suave.is_finite() { borda_suave.max(0.0) } else { 0.0 };
        let m = borda * (c - a).min(d - b) + 3.0;
        let (a, b, c, d) = (a - m, b - m, c + m, d + m);
        if ![a, b, c, d].iter().all(|v| v.is_finite()) {
            return Some(inteiro);
        }
        u = Some(match u {
            None => (a, b, c, d),
            Some((e, f, g, k)) => (e.min(a), f.min(b), g.max(c), k.max(d)),
        });
    }
    u.map(|(a, b, c, d)| geometria::caixa_px_par(Caixa { x: a / w, y: b / h, w: (c - a) / w, h: (d - b) / h }, espaco))
}

/// Forma PNG da lupa (círculo ou retângulo) para o `alphamerge`: cinza `lado`×`lado` px.
pub fn forma_lupa(forma: FormaLupa, lado: u32) -> Result<Bitmap> {
    if lado == 0 {
        return Err(invalido("lado"));
    }
    let px = match forma {
        FormaLupa::Retangulo => vec![255u8; lado as usize * lado as usize],
        FormaLupa::Circulo => {
            let l = lado as f32;
            let mut m = tiny_skia::Mask::new(lado, lado).ok_or_else(|| invalido("lado"))?;
            let oval = tiny_skia::Rect::from_xywh(0.0, 0.0, l, l)
                .and_then(tiny_skia::PathBuilder::from_oval)
                .ok_or_else(|| invalido("lado"))?;
            m.fill_path(&oval, tiny_skia::FillRule::Winding, true, tiny_skia::Transform::identity());
            m.data().to_vec()
        }
    };
    Ok(Bitmap { w: lado, h: lado, canais: 1, px })
}

// CONTRATO: acréscimo (ponto de extensão da silhueta).
/// Une `outra` (cinza, posta em `onde` = [x, y] do mesmo espaço) a `m` (cinza em `caixa_px`), pelo maior valor.
/// Ponto de extensão da Fase 5: a silhueta dilatada entra aqui sobre a máscara da elipse inscrita (§5.5, §6.6).
pub fn unir(m: &mut Bitmap, caixa_px: [u32; 4], outra: &Bitmap, onde: [u32; 2]) -> Result<()> {
    if m.canais != 1 || outra.canais != 1 {
        return Err(invalido("canais"));
    }
    if m.px.len() != m.w as usize * m.h as usize || outra.px.len() != outra.w as usize * outra.h as usize {
        return Err(invalido("tamanho"));
    }
    for y in 0..outra.h {
        let gy = onde[1] as i64 + y as i64 - caixa_px[1] as i64;
        if gy < 0 || gy >= m.h as i64 {
            continue;
        }
        for x in 0..outra.w {
            let gx = onde[0] as i64 + x as i64 - caixa_px[0] as i64;
            if gx < 0 || gx >= m.w as i64 {
                continue;
            }
            let i = gy as usize * m.w as usize + gx as usize;
            m.px[i] = m.px[i].max(outra.px[y as usize * outra.w as usize + x as usize]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::camada::formas::{EspacoRegiao, ModoRegiao, Segmento};

    const ESPACO: TamanhoPx = TamanhoPx { w: 640, h: 360 };

    fn regiao(forma: Primitiva, borda: f64, inverter: bool) -> Primitiva {
        regiao_modo(ModoRegiao::Solido, forma, borda, inverter)
    }

    fn regiao_modo(modo: ModoRegiao, forma: Primitiva, borda: f64, inverter: bool) -> Primitiva {
        Primitiva::Regiao {
            modo,
            forma: Box::new(forma),
            intensidade: 1.0,
            cor: Some("#000000ff".into()),
            borda_suave: borda,
            inverter,
            espaco: EspacoRegiao::Fonte,
        }
    }

    /// Retângulo em px do espaço.
    fn retangulo(x: f64, y: f64, w: f64, h: f64) -> Primitiva {
        let (ew, eh) = (ESPACO.w as f64, ESPACO.h as f64);
        Primitiva::Caminho {
            d: vec![
                Segmento::M(x / ew, y / eh),
                Segmento::L((x + w) / ew, y / eh),
                Segmento::L((x + w) / ew, (y + h) / eh),
                Segmento::L(x / ew, (y + h) / eh),
                Segmento::Z,
            ],
            traco: None,
            preenche: None,
            regra: None,
        }
    }

    fn valor(m: &Bitmap, caixa: [u32; 4], x: u32, y: u32) -> u8 {
        m.px[((y - caixa[1]) * m.w + (x - caixa[0])) as usize]
    }

    /// Fração (0..1) do pixel coberta pela forma, por 16×16 amostras: > 0 = o pixel toca a forma.
    fn toca_amostrado(dentro: impl Fn(f64, f64) -> bool, x: u32, y: u32) -> bool {
        (0..16).any(|i| (0..16).any(|j| dentro(x as f64 + (i as f64 + 0.5) / 16.0, y as f64 + (j as f64 + 0.5) / 16.0)))
    }

    #[test]
    fn borda_suave_cresce_para_fora_no_retangulo() {
        // caixa 200×120 em (221,5; 121,5) (fora da grade de propósito); borda 0,25 do lado menor = 30 px
        let (x, y, w, h) = (221.5, 121.5, 200.0, 120.0);
        let quadros = vec![vec![regiao(retangulo(x, y, w, h), 0.25, false)]];
        let caixa = uniao_px(&quadros, ESPACO).unwrap();
        assert!(caixa[0] as f64 <= x - 30.0 && (caixa[0] + caixa[2]) as f64 >= x + w + 30.0, "{caixa:?}");
        let m = mascara(&quadros[0], ESPACO, caixa).unwrap();
        let mut dentro = 0;
        let mut anterior = 255u8;
        for py in caixa[1]..caixa[1] + caixa[3] {
            for px in caixa[0]..caixa[0] + caixa[2] {
                let v = valor(&m, caixa, px, py);
                let toca = (px as f64) < x + w && (px + 1) as f64 > x && (py as f64) < y + h && (py + 1) as f64 > y;
                if toca {
                    assert_eq!(v, 255, "pixel ({px}, {py}) da caixa sem cobertura total");
                    dentro += 1;
                }
                // além da borda (distância ≥ 30 px + 1 do bloco): nada
                let dx = (x - (px as f64 + 0.5)).max((px as f64 + 0.5) - (x + w)).max(0.0);
                let dy = (y - (py as f64 + 0.5)).max((py as f64 + 0.5) - (y + h)).max(0.0);
                if (dx * dx + dy * dy).sqrt() >= 30.0 {
                    assert_eq!(v, 0, "({px}, {py}) além da borda");
                }
            }
        }
        assert_eq!(dentro, 201 * 121, "pixels tocados pela caixa fora da grade");
        // gradiente: decresce para fora ao longo de uma linha
        for px in (x + w) as u32 + 2..(x + w) as u32 + 32 {
            let v = valor(&m, caixa, px, 180);
            assert!(v <= anterior, "não decresce em {px}");
            anterior = v;
        }
        assert!(valor(&m, caixa, (x + w) as u32 + 2, 180) > 200 && valor(&m, caixa, (x + w) as u32 + 25, 180) < 30);
    }

    #[test]
    fn bloco_2x2_inteiro_na_borda() {
        // borda em x = 101,3 e y = 51: bloco [100, 102) inteiro coberto (croma 4:2:0 por bloco)
        let r = vec![regiao(retangulo(101.3, 51.0, 20.0, 20.0), 0.0, false)];
        let caixa = [96, 48, 32, 28];
        let m = mascara(&r, ESPACO, caixa).unwrap();
        assert_eq!(valor(&m, caixa, 100, 52), 255);
        assert_eq!(valor(&m, caixa, 101, 52), 255);
        assert_eq!(valor(&m, caixa, 99, 52), 0);
        // fim em 121,3: bloco [120, 122) coberto, [122, 124) não
        assert_eq!(valor(&m, caixa, 121, 60), 255);
        assert_eq!(valor(&m, caixa, 122, 60), 0);
        // borda que só encosta (y = 51 → fim 71): bloco [70, 72) coberto; [72, 74) não
        assert_eq!(valor(&m, caixa, 110, 71), 255);
        assert_eq!(valor(&m, caixa, 110, 72), 0);
        // pares inteiros: nenhum pixel a mais
        let r = vec![regiao(retangulo(100.0, 50.0, 20.0, 20.0), 0.0, false)];
        let m = mascara(&r, ESPACO, caixa).unwrap();
        assert_eq!(m.px.iter().filter(|v| **v == 255).count(), 400);
        assert_eq!(m.px.iter().filter(|v| **v != 0 && **v != 255).count(), 0);
    }

    #[test]
    fn elipse_e_poligono_cobertos_pixel_a_pixel() {
        let (ew, eh) = (ESPACO.w as f64, ESPACO.h as f64);
        let (cx, cy, rx, ry) = (300.3, 170.7, 61.9, 33.2);
        let elipse = Primitiva::Elipse { cx: cx / ew, cy: cy / eh, rx: rx / ew, ry: ry / eh, traco: None, preenche: None };
        let pts = [(250.5, 120.25), (380.0, 140.0), (330.0, 230.75), (270.0, 200.0)];
        let poligono = Primitiva::Caminho {
            d: pts
                .iter()
                .enumerate()
                .map(|(i, p)| if i == 0 { Segmento::M(p.0 / ew, p.1 / eh) } else { Segmento::L(p.0 / ew, p.1 / eh) })
                .chain([Segmento::Z])
                .collect(),
            traco: None,
            preenche: None,
            regra: None,
        };
        let dentro_elipse = |x: f64, y: f64| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) < 1.0;
        let dentro_poligono = |x: f64, y: f64| dentro(&[pts.to_vec()], false, x, y);
        for (forma, dentro_f) in [(elipse, &dentro_elipse as &dyn Fn(f64, f64) -> bool), (poligono, &dentro_poligono)] {
            for borda in [0.0, 0.1, 0.5] {
                let q = vec![vec![regiao(forma.clone(), borda, false)]];
                let caixa = uniao_px(&q, ESPACO).unwrap();
                let m = mascara(&q[0], ESPACO, caixa).unwrap();
                let mut tocados = 0;
                for py in caixa[1]..caixa[1] + caixa[3] {
                    for px in caixa[0]..caixa[0] + caixa[2] {
                        if toca_amostrado(dentro_f, px, py) {
                            assert_eq!(valor(&m, caixa, px, py), 255, "({px}, {py}) borda {borda}");
                            tocados += 1;
                        }
                    }
                }
                assert!(tocados > 1000);
                // a borda do recorte fica em 0: o gradiente não é cortado
                for px in caixa[0]..caixa[0] + caixa[2] {
                    assert_eq!(valor(&m, caixa, px, caixa[1]), 0);
                    assert_eq!(valor(&m, caixa, px, caixa[1] + caixa[3] - 1), 0);
                }
            }
        }
    }

    #[test]
    fn inverter_troca_na_origem_e_uniao_pelo_maior() {
        let a = regiao(retangulo(100.0, 100.0, 40.0, 40.0), 0.0, false);
        let b = regiao(retangulo(200.0, 100.0, 40.0, 40.0), 0.0, false);
        let caixa = [0, 0, 640, 360];
        let m = mascara(&[a.clone(), b], ESPACO, caixa).unwrap();
        assert_eq!(m.px.iter().filter(|v| **v == 255).count(), 3200);
        // holofote (escurecer): complemento exato, o alvo fica intacto
        let inv = regiao_modo(ModoRegiao::Escurecer, retangulo(100.0, 100.0, 40.0, 40.0), 0.25, true);
        let mi = mascara(std::slice::from_ref(&inv), ESPACO, caixa).unwrap();
        let mn = mascara(&[regiao_modo(ModoRegiao::Escurecer, retangulo(100.0, 100.0, 40.0, 40.0), 0.25, false)], ESPACO, caixa).unwrap();
        assert!(mi.px.iter().zip(&mn.px).all(|(a, b)| *a == 255 - *b));
        assert_eq!(valor(&mi, caixa, 120, 120), 0);
        assert_eq!(uniao_px(&[vec![inv]], ESPACO), Some([0, 0, 640, 360]));
        assert_eq!(uniao_px(&[vec![]], ESPACO), None);
    }

    #[test]
    fn ocultar_invertido_cobre_tudo_fora_da_forma() {
        let (ew, eh) = (ESPACO.w as f64, ESPACO.h as f64);
        let (cx, cy, rx, ry) = (300.3, 170.7, 61.9, 33.2);
        let elipse = Primitiva::Elipse { cx: cx / ew, cy: cy / eh, rx: rx / ew, ry: ry / eh, traco: None, preenche: None };
        let pts = [(250.5, 120.25), (380.0, 140.0), (330.0, 230.75), (270.0, 200.0)];
        let poligono = Primitiva::Caminho {
            d: pts
                .iter()
                .enumerate()
                .map(|(i, p)| if i == 0 { Segmento::M(p.0 / ew, p.1 / eh) } else { Segmento::L(p.0 / ew, p.1 / eh) })
                .chain([Segmento::Z])
                .collect(),
            traco: None,
            preenche: None,
            regra: None,
        };
        let (x, y, w, h) = (221.5, 121.5, 200.0, 120.0);
        let fora_ret = move |a: f64, b: f64| !(a > x && a < x + w && b > y && b < y + h);
        let fora_elipse = move |a: f64, b: f64| ((a - cx) / rx).powi(2) + ((b - cy) / ry).powi(2) > 1.0;
        let fora_poligono = move |a: f64, b: f64| !dentro(&[pts.to_vec()], false, a, b);
        let caixa = [0, 0, 640, 360];
        type Fora<'a> = &'a dyn Fn(f64, f64) -> bool;
        let formas: [(Primitiva, Fora); 3] = [(retangulo(x, y, w, h), &fora_ret), (elipse, &fora_elipse), (poligono, &fora_poligono)];
        for (forma, fora) in formas {
            for modo in [ModoRegiao::Solido, ModoRegiao::Desfoque, ModoRegiao::Mosaico] {
                for borda in [0.0, 0.25, 0.5] {
                    let m = mascara(&[regiao_modo(modo, forma.clone(), borda, true)], ESPACO, caixa).unwrap();
                    let (mut cobertos, mut livres) = (0u32, 0u32);
                    for py in 0..360u32 {
                        for px in 0..640u32 {
                            let v = valor(&m, caixa, px, py);
                            // pixel com qualquer pedaço fora da forma: todo coberto
                            if toca_amostrado(fora, px, py) {
                                assert_eq!(v, 255, "({px}, {py}) {modo:?} borda {borda}: fora da forma e descoberto");
                                cobertos += 1;
                            }
                            livres += (v == 0) as u32;
                        }
                    }
                    assert!(cobertos > 200_000);
                    // o miolo aparece (com borda 0,5 o gradiente chega ao centro e pode não sobrar pixel em 0)
                    assert!(borda >= 0.5 || livres > 100, "{modo:?} borda {borda}: {livres}");
                    if borda == 0.0 {
                        assert!(m.px.iter().all(|v| *v == 0 || *v == 255));
                    } else {
                        let sem = mascara(&[regiao_modo(modo, forma.clone(), 0.0, true)], ESPACO, caixa).unwrap();
                        assert!(livres < sem.px.iter().filter(|v| **v == 0).count() as u32);
                        // nenhum pixel mais descoberto do que sem borda
                        assert!(m.px.iter().zip(&sem.px).all(|(a, b)| a >= b));
                        assert!(m.px.iter().any(|v| *v > 0 && *v < 255));
                    }
                }
            }
        }
        // retângulo: o gradiente cai para dentro ao longo de uma linha e zera em B = 0,25 × 120 = 30 px
        let m = mascara(&[regiao(retangulo(x, y, w, h), 0.25, true)], ESPACO, caixa).unwrap();
        let mut anterior = 255u8;
        for px in 222..262u32 {
            let v = valor(&m, caixa, px, 180);
            assert!(v <= anterior, "não decresce em {px}");
            anterior = v;
        }
        assert!(valor(&m, caixa, 224, 180) > 200 && valor(&m, caixa, 246, 180) < 30 && valor(&m, caixa, 253, 180) == 0);
        assert_eq!(valor(&m, caixa, 221, 180), 255);
    }

    #[test]
    fn regiao_nao_finita_e_recusada() {
        let nan = Primitiva::Elipse { cx: f64::NAN, cy: 0.5, rx: 0.1, ry: 0.1, traco: None, preenche: None };
        let inf = Primitiva::Caminho {
            d: vec![Segmento::M(0.1, 0.1), Segmento::L(f64::INFINITY, 0.1), Segmento::L(0.2, f64::NAN), Segmento::Z],
            traco: None,
            preenche: None,
            regra: None,
        };
        for forma in [nan, inf] {
            let r = regiao(forma, 0.1, false);
            let e = mascara(std::slice::from_ref(&r), ESPACO, [0, 0, 64, 36]).unwrap_err().to_string();
            assert!(e.contains("item_invalido"), "{e}");
            // a união não pula a região: pede o espaço inteiro (e a máscara recusa)
            assert_eq!(uniao_px(&[vec![r]], ESPACO), Some([0, 0, 640, 360]));
        }
        let mut m = Bitmap { w: 4, h: 4, canais: 1, px: vec![0; 16] };
        assert!(unir(&mut m, [0, 0, 4, 4], &Bitmap { w: 2, h: 2, canais: 1, px: vec![200; 3] }, [0, 0]).is_err());
    }

    #[test]
    fn forma_da_lupa() {
        let c = forma_lupa(FormaLupa::Circulo, 64).unwrap();
        assert_eq!((c.w, c.h, c.canais), (64, 64, 1));
        assert_eq!(c.px[0], 0);
        assert_eq!(c.px[32 * 64 + 32], 255);
        assert!(forma_lupa(FormaLupa::Retangulo, 8).unwrap().px.iter().all(|v| *v == 255));
        let mut m = Bitmap { w: 4, h: 4, canais: 1, px: vec![0; 16] };
        unir(&mut m, [10, 10, 4, 4], &Bitmap { w: 2, h: 2, canais: 1, px: vec![200; 4] }, [13, 13]).unwrap();
        assert_eq!(m.px[15], 200);
        assert_eq!(m.px.iter().filter(|v| **v != 0).count(), 1);
    }
}
