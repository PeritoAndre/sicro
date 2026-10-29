//! Média (ou mediana) de recortes alinhados de vários quadros: some o ruído de cada quadro
//! e deixa o que se repete (placa, rosto). Quadros que não casam com a referência são descartados.

use serde::Serialize;

use super::align::{self, Gray, Warp};

/// Recorte de um quadro: a região de interesse mais uma margem, em RGB.
#[derive(Debug, Clone)]
pub struct Recorte {
    pub rgb: Vec<u8>,
    pub w: usize,
    pub h: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct Opcoes {
    /// Região de interesse dentro do recorte: x, y, largura, altura.
    pub roi: (usize, usize, usize, usize),
    /// Índice do quadro de referência na lista de recortes.
    pub referencia: usize,
    /// Alinha por homografia (gira / muda de escala); senão só translação.
    pub homografia: bool,
    pub mediana: bool,
    /// Deslocamento máximo procurado (px).
    pub busca: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct InfoQuadro {
    pub indice: usize,
    pub dx: f64,
    pub dy: f64,
    pub ncc: f32,
    pub aceito: bool,
}

#[derive(Debug, Clone)]
pub struct Combinado {
    /// ROI combinada, RGB.
    pub rgb: Vec<u8>,
    /// ROI crua do quadro de referência, RGB.
    pub rgb_ref: Vec<u8>,
    pub w: usize,
    pub h: usize,
    pub usados: usize,
    pub quadros: Vec<InfoQuadro>,
}

fn amostra_rgb(r: &Recorte, x: f64, y: f64) -> [f32; 3] {
    let x = x.clamp(0.0, (r.w - 1) as f64) as f32;
    let y = y.clamp(0.0, (r.h - 1) as f64) as f32;
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(r.w - 1), (y0 + 1).min(r.h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut o = [0.0f32; 3];
    for (c, oc) in o.iter_mut().enumerate() {
        let g = |xx: usize, yy: usize| r.rgb[(yy * r.w + xx) * 3 + c] as f32;
        let a = g(x0, y0) * (1.0 - fx) + g(x1, y0) * fx;
        let b = g(x0, y1) * (1.0 - fx) + g(x1, y1) * fx;
        *oc = a * (1.0 - fy) + b * fy;
    }
    o
}

/// ROI do recorte `r` levada ao sistema da referência por `warp`.
fn levar_a_roi(r: &Recorte, warp: &Warp, roi: (usize, usize, usize, usize)) -> Vec<u8> {
    let (rx, ry, rw, rh) = roi;
    let mut out = Vec::with_capacity(rw * rh * 3);
    for y in ry..ry + rh {
        for x in rx..rx + rw {
            let (u, v) = warp.aplica(x as f64, y as f64);
            let p = amostra_rgb(r, u, v);
            out.extend(p.iter().map(|c| c.round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

fn gray_de(rgb: &[u8]) -> Vec<f32> {
    rgb.chunks_exact(3).map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32).collect()
}

/// Alinha todos os recortes à referência e combina. `progresso(feitos, total)` é chamado
/// a cada quadro alinhado (de qualquer fio).
pub fn combinar(
    recortes: &[Recorte],
    op: Opcoes,
    cancelar: &std::sync::atomic::AtomicBool,
    progresso: &(dyn Fn(usize, usize) + Sync),
) -> Result<Combinado, String> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let n = recortes.len();
    if n == 0 || op.referencia >= n {
        return Err("sem quadros para combinar".into());
    }
    let (rx, ry, rw, rh) = op.roi;
    let r0 = &recortes[op.referencia];
    if rx + rw > r0.w || ry + rh > r0.h || rw < 8 || rh < 8 {
        return Err("região de interesse inválida".into());
    }
    if recortes.iter().any(|r| r.w != r0.w || r.h != r0.h) {
        return Err("os recortes têm tamanhos diferentes".into());
    }
    let grays: Vec<Gray> = recortes.iter().map(|r| Gray::de_rgb(&r.rgb, r.w, r.h).borrar(1.0)).collect();
    let refg = &grays[op.referencia];
    // a fase é calculada num borrão maior (o ruído atrapalha mais do que a textura fina ajuda)
    let grays2: Vec<Gray> = grays.iter().map(|g| g.borrar(1.5)).collect();
    let refg2 = &grays2[op.referencia];

    let feitos = AtomicUsize::new(0);
    let mut alinhados: Vec<Option<(Warp, Vec<u8>, f32, f32)>> = vec![None; n];
    let fios = std::thread::available_parallelism().map_or(4, |v| v.get()).clamp(1, 8);
    let blocos: Vec<Vec<usize>> = (0..fios).map(|f| (f..n).step_by(fios).collect()).collect();
    let partes: Vec<Vec<(usize, Warp, Vec<u8>, f32, f32)>> = std::thread::scope(|s| {
        let hs: Vec<_> = blocos
            .iter()
            .map(|idx| {
                let (feitos, grays, grays2, refg2) = (&feitos, &grays, &grays2, refg2);
                s.spawn(move || {
                    let mut res = Vec::new();
                    for &i in idx {
                        if cancelar.load(Ordering::Relaxed) {
                            break;
                        }
                        let (warp, pico) = if i == op.referencia {
                            (Warp::identidade(), 1.0)
                        } else {
                            let (dx, dy, pico) = align::correlacao_de_fase(&refg2, &grays2[i], op.busca);
                            let ini = Warp::translacao(dx as f64, dy as f64);
                            let (w1, _) = align::refinar(refg, &grays[i], op.roi, ini, false, 25);
                            let w2 = if op.homografia { align::refinar(refg, &grays[i], op.roi, w1, true, 40).0 } else { w1 };
                            (w2, pico)
                        };
                        let roi = levar_a_roi(&recortes[i], &warp, op.roi);
                        res.push((i, warp, roi, pico, 0.0));
                        let f = feitos.fetch_add(1, Ordering::Relaxed) + 1;
                        progresso(f, n);
                    }
                    res
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap_or_default()).collect()
    });
    if cancelar.load(Ordering::Relaxed) {
        return Err("cancelado".into());
    }
    for (i, w, roi, pico, z) in partes.into_iter().flatten() {
        alinhados[i] = Some((w, roi, pico, z));
    }

    // Média provisória de todos → nota de cada quadro contra ela (menos ruído que contra um quadro só).
    let npx = rw * rh * 3;
    let mut soma = vec![0.0f32; npx];
    for a in alinhados.iter().flatten() {
        for (s, v) in soma.iter_mut().zip(&a.1) {
            *s += *v as f32;
        }
    }
    let cont = alinhados.iter().flatten().count() as f32;
    let prov: Vec<f32> = soma.iter().map(|s| s / cont).collect();
    let prov_g: Vec<f32> = prov.chunks_exact(3).map(|p| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]).collect();
    let notas: Vec<f32> = alinhados
        .iter()
        .map(|a| a.as_ref().map_or(-1.0, |a| align::ncc(&gray_de(&a.1), &prov_g)))
        .collect();
    let mut ordenadas: Vec<f32> = notas.iter().copied().filter(|v| *v >= -0.5).collect();
    ordenadas.sort_by(|a, b| a.total_cmp(b));
    let med = ordenadas[ordenadas.len() / 2];
    let mut desv: Vec<f32> = ordenadas.iter().map(|v| (v - med).abs()).collect();
    desv.sort_by(|a, b| a.total_cmp(b));
    let sigma = 1.4826 * desv[desv.len() / 2];
    let limite = (med - (3.0 * sigma).max(0.08)).max(0.15);

    let mut aceitos: Vec<usize> = Vec::new();
    let mut quadros = Vec::with_capacity(n);
    for (i, a) in alinhados.iter().enumerate() {
        let Some((w, _, _, _)) = a else { continue };
        let ok = i == op.referencia || notas[i] >= limite;
        if ok {
            aceitos.push(i);
        }
        quadros.push(InfoQuadro { indice: i, dx: w.p[4], dy: w.p[5], ncc: notas[i], aceito: ok });
    }

    let mut rgb = vec![0u8; npx];
    if op.mediana {
        let mut col = vec![0u8; aceitos.len()];
        for k in 0..npx {
            for (c, &i) in aceitos.iter().enumerate() {
                col[c] = alinhados[i].as_ref().map_or(0, |a| a.1[k]);
            }
            col.sort_unstable();
            let m = col.len() / 2;
            rgb[k] = if col.len() % 2 == 1 { col[m] } else { ((col[m - 1] as u16 + col[m] as u16 + 1) / 2) as u8 };
        }
    } else {
        let mut acc = vec![0.0f32; npx];
        for &i in &aceitos {
            if let Some(a) = &alinhados[i] {
                for (s, v) in acc.iter_mut().zip(&a.1) {
                    *s += *v as f32;
                }
            }
        }
        for (o, s) in rgb.iter_mut().zip(&acc) {
            *o = (s / aceitos.len() as f32).round().clamp(0.0, 255.0) as u8;
        }
    }
    let rgb_ref = alinhados[op.referencia].as_ref().map(|a| a.1.clone()).unwrap_or_default();
    Ok(Combinado { rgb, rgb_ref, w: rw, h: rh, usados: aceitos.len(), quadros })
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// Cena contínua (textura de senoides + "letras"), amostrável em qualquer ponto: o deslocamento é exato.
    fn cena(x: f64, y: f64) -> f64 {
        let t = 128.0
            + 40.0 * (x * 0.21).sin() * (y * 0.17).cos()
            + 30.0 * (x * 0.07 + y * 0.11).sin()
            + 20.0 * (x * 0.43 - y * 0.31).cos();
        // um bloco claro com bordas nítidas, como um caractere
        let bloco = if (30.0..44.0).contains(&x) && (28.0..52.0).contains(&y) { 60.0 } else { 0.0 };
        t + bloco
    }

    struct Rng(u64);
    impl Rng {
        fn unif(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
        fn gauss(&mut self, s: f64) -> f64 {
            let (a, b) = (self.unif().max(1e-12), self.unif());
            s * (-2.0 * a.ln()).sqrt() * (2.0 * std::f64::consts::PI * b).cos()
        }
    }

    fn recorte(w: usize, h: usize, dx: f64, dy: f64, ruido: f64, rng: &mut Rng) -> Recorte {
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let v = cena(x as f64 - dx, y as f64 - dy);
                for c in 0..3 {
                    let tom = v + (c as f64 - 1.0) * 10.0 + rng.gauss(ruido);
                    rgb.push(tom.round().clamp(0.0, 255.0) as u8);
                }
            }
        }
        Recorte { rgb, w, h }
    }

    fn desvio(a: &[u8], b: &[f64]) -> f64 {
        let s: f64 = a.iter().zip(b).map(|(x, y)| (*x as f64 - y).powi(2)).sum();
        (s / a.len() as f64).sqrt()
    }

    #[test]
    fn correlacao_de_fase_acha_deslocamento_subpixel() {
        let mut rng = Rng(7);
        let a = Gray::de_rgb(&recorte(96, 80, 0.0, 0.0, 0.0, &mut rng).rgb, 96, 80);
        let b = Gray::de_rgb(&recorte(96, 80, 3.4, -2.6, 0.0, &mut rng).rgb, 96, 80);
        let (dx, dy, pico) = align::correlacao_de_fase(&a, &b, 12);
        assert!((dx - 3.4).abs() < 0.75 && (dy + 2.6).abs() < 0.75, "dx {dx} dy {dy}");
        assert!(pico > 0.1);
    }

    #[test]
    fn alinha_com_erro_pequeno_e_a_media_reduz_o_ruido() {
        let mut rng = Rng(12345);
        let (w, h) = (96usize, 80usize);
        let desloc = [(0.0, 0.0), (1.3, -0.8), (-2.2, 1.7), (3.1, 2.4), (-0.6, -2.9), (2.0, -1.5), (-1.4, 0.5), (0.9, 2.2)];
        let sigma = 25.0;
        let rec: Vec<Recorte> = (0..16)
            .map(|i| {
                let (dx, dy) = desloc[i % desloc.len()];
                recorte(w, h, dx + (i / 8) as f64 * 0.5, dy, sigma, &mut rng)
            })
            .collect();
        let roi = (16usize, 16usize, 64usize, 48usize);
        let cancel = AtomicBool::new(false);
        let r = combinar(&rec, Opcoes { roi, referencia: 0, homografia: false, mediana: false, busca: 10 }, &cancel, &|_, _| {}).unwrap();
        assert_eq!(r.usados, 16);
        for q in &r.quadros {
            let (dx, dy) = desloc[q.indice % desloc.len()];
            let dx = dx + (q.indice / 8) as f64 * 0.5;
            assert!((q.dx - dx).abs() < 0.45 && (q.dy - dy).abs() < 0.45, "quadro {} ({:.2},{:.2}) esperado ({dx},{dy})", q.indice, q.dx, q.dy);
        }
        // referência exata da ROI (sem ruído) para medir o erro
        let verdade: Vec<f64> = (16..64)
            .flat_map(|y| (16..80).flat_map(move |x| (0..3).map(move |c| cena(x as f64, y as f64) + (c as f64 - 1.0) * 10.0)))
            .collect();
        let ruido_cru = desvio(&r.rgb_ref, &verdade);
        let ruido_media = desvio(&r.rgb, &verdade);
        // 16 quadros: o ideal é 4x menos; o alinhamento e o borrão da interpolação comem um pouco
        assert!(ruido_media < ruido_cru / 2.5, "cru {ruido_cru:.2} média {ruido_media:.2}");
    }

    #[test]
    fn descarta_quadro_que_nao_casa() {
        let mut rng = Rng(99);
        let (w, h) = (96usize, 80usize);
        let mut rec: Vec<Recorte> = (0..9).map(|i| recorte(w, h, i as f64 * 0.4 - 1.6, 0.0, 10.0, &mut rng)).collect();
        // quadro de outra cena (ruído branco)
        let estranho: Vec<u8> = (0..w * h * 3).map(|_| (rng.unif() * 255.0) as u8).collect();
        rec[8] = Recorte { rgb: estranho, w, h };
        let cancel = AtomicBool::new(false);
        let r = combinar(&rec, Opcoes { roi: (16, 16, 64, 48), referencia: 4, homografia: false, mediana: true, busca: 8 }, &cancel, &|_, _| {}).unwrap();
        assert!(!r.quadros[8].aceito, "{:?}", r.quadros[8]);
        assert_eq!(r.usados, 8);
    }

    #[test]
    fn homografia_acompanha_pequena_escala() {
        let mut rng = Rng(5);
        let (w, h) = (110usize, 90usize);
        let base = recorte(w, h, 0.0, 0.0, 0.0, &mut rng);
        // quadro 2: escala de 3% em torno do centro
        let mut rgb = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
                let v = cena(cx + (x as f64 - cx) / 1.03, cy + (y as f64 - cy) / 1.03);
                for c in 0..3 {
                    rgb.push((v + (c as f64 - 1.0) * 10.0).round().clamp(0.0, 255.0) as u8);
                }
            }
        }
        let b = Recorte { rgb, w, h };
        let g0 = Gray::de_rgb(&base.rgb, w, h).borrar(1.0);
        let g1 = Gray::de_rgb(&b.rgb, w, h).borrar(1.0);
        let roi = (20, 20, 70, 50);
        let (_, e_t) = align::refinar(&g0, &g1, roi, Warp::identidade(), false, 25);
        let (wp, e_h) = align::refinar(&g0, &g1, roi, Warp::identidade(), true, 60);
        assert!(e_h < e_t * 0.5, "translação {e_t} homografia {e_h}");
        // o centro da ROI deve cair perto de si mesmo e a escala ~1.03
        assert!((wp.p[0] - 0.03).abs() < 0.01 || (wp.p[3] - 0.03).abs() < 0.01, "{:?}", wp.p);
    }
}
