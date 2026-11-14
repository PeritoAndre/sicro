//! Espaços (§3.2), transformação origem → canvas `A(τ)`, caixas e enquadre.
//! Espelho no front: `modelo/geometria.ts`. Dono: P0.3a.
//!
//! Espaços, todos em 0..1 (`x` para a direita, `y` para baixo):
//! - fonte exibida = original: quadro com o giro do contêiner e a proporção do pixel aplicados (`exibido`);
//! - origem do clipe: o original depois de `estabilizar` e `lente` (mesmo tamanho; nome em `espaco_de_origem`);
//! - canvas: a saída. `A(τ)` leva a origem ao canvas: giro e espelho do `girar`, região do `enquadramento`
//!   em τ e ajuste (`conter` / `cobrir`), sem arredondar para pixel.
//!
//! Determinístico como `curva.rs`: só `+ − × ÷`, na ordem escrita; giro só em múltiplos de 90°.
//! Toda saída de `Afim` passa por `+ 0` (−0 vira 0: o `toEqual` do vitest separa os dois).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::curva;
use super::modelo::{Ajuste, Animavel, Caixa, Efeito, Ponto, Racional, TamanhoPx, Tolerante};

/// −0 → 0; o resto não muda.
fn z(v: f64) -> f64 {
    v + 0.0
}

/// Afim em 0..1, na ordem do canvas 2D (`setTransform(a, b, c, d, e, f)`):
/// x' = a·x + c·y + e; y' = b·x + d·y + f. É o `QuadroPalco.A` do front.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Afim(pub [f64; 6]);

impl Afim {
    /// Não muda nada.
    pub const IDENTIDADE: Afim = Afim([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// Ponto transformado (0..1 da origem → 0..1 do canvas). `(a·x + c·y) + e`.
    pub fn ponto(&self, p: Ponto) -> Ponto {
        let [a, b, c, d, e, f] = self.0;
        Ponto { x: z(a * p.x + c * p.y + e), y: z(b * p.x + d * p.y + f) }
    }

    /// Caixa envolvente da caixa transformada: canto (x, y) transformado mais a parte negativa
    /// das duas arestas; lado = soma dos módulos das arestas.
    pub fn caixa(&self, cx: Caixa) -> Caixa {
        let [a, b, c, d, e, f] = self.0;
        let neg = |v: f64| if v < 0.0 { v } else { 0.0 };
        let (px, py) = (a * cx.x + c * cx.y + e, b * cx.x + d * cx.y + f);
        let (wx, wy) = (a * cx.w, b * cx.w);
        let (hx, hy) = (c * cx.h, d * cx.h);
        Caixa { x: z(px + neg(wx) + neg(hx)), y: z(py + neg(wy) + neg(hy)), w: z(wx.abs() + hx.abs()), h: z(wy.abs() + hy.abs()) }
    }

    /// Composição: aplica `self` e depois `depois`.
    pub fn seguida_de(&self, depois: &Afim) -> Afim {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = depois.0;
        Afim([
            z(a2 * a + c2 * b),
            z(b2 * a + d2 * b),
            z(a2 * c + c2 * d),
            z(b2 * c + d2 * d),
            z(a2 * e + c2 * f + e2),
            z(b2 * e + d2 * f + f2),
        ])
    }

    /// Inversa (canvas → origem); None se degenerada. É a do toque na tela.
    pub fn inversa(&self) -> Option<Afim> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let (ai, bi, ci, di) = (d / det, -b / det, -c / det, a / det);
        let r = Afim([z(ai), z(bi), z(ci), z(di), z(-(ai * e + ci * f)), z(-(bi * e + di * f))]);
        r.0.iter().all(|v| v.is_finite()).then_some(r)
    }
}

/// Graus (qualquer sinal) → quartos de volta no sentido horário, 0..3 (arredonda ao múltiplo de 90 mais perto).
fn quartos(graus: i32) -> u8 {
    (((graus.rem_euclid(360) + 45) / 90) % 4) as u8
}

/// Giro horário de `graus` (múltiplo de 90) e, depois dele, os espelhos, em 0..1.
/// Mesma ordem dos filtros: `transpose` → `hflip` → `vflip`. Serve ao `girar` e ao giro do contêiner
/// (o `rotation` do ffprobe é anti-horário: passar com o sinal trocado; a etiqueta `rotate` já é horária).
pub fn afim_giro(graus: i32, espelhar_h: bool, espelhar_v: bool) -> Afim {
    let g = match quartos(graus) {
        0 => Afim::IDENTIDADE,
        // (x, y) → (1 − y, x)
        1 => Afim([0.0, 1.0, -1.0, 0.0, 1.0, 0.0]),
        2 => Afim([-1.0, 0.0, 0.0, -1.0, 1.0, 1.0]),
        // (x, y) → (y, 1 − x)
        _ => Afim([0.0, -1.0, 1.0, 0.0, 0.0, 1.0]),
    };
    let g = if espelhar_h { g.seguida_de(&Afim([-1.0, 0.0, 0.0, 1.0, 1.0, 0.0])) } else { g };
    if espelhar_v {
        g.seguida_de(&Afim([1.0, 0.0, 0.0, -1.0, 0.0, 1.0]))
    } else {
        g
    }
}

/// Tamanho exibido (px) de um quadro codificado com proporção de pixel `sar` e giro do contêiner `giro`
/// (graus no sentido horário): largura × sar arredondada, troca nos giros de 90° e 270°, lados pares
/// (mínimo 2). Mesma regra de `video/pessoas/decode.rs::display_size`.
pub fn exibido(codificado: TamanhoPx, sar: Racional, giro: i32) -> TamanhoPx {
    let (num, den) = if sar.num > 0 && sar.den > 0 { (sar.num as i128, sar.den as i128) } else { (1, 1) };
    let w = ((2 * codificado.w as i128 * num + den) / (2 * den)).min(u32::MAX as i128) as u32;
    let (w, h) = if quartos(giro) % 2 == 1 { (codificado.h, w) } else { (w, codificado.h) };
    let par = |v: u32| (v / 2 * 2).max(2);
    TamanhoPx { w: par(w), h: par(h) }
}

/// Giro e espelhos dos `girar` ligados, compostos na ordem, e o total de quartos de volta.
fn giro_dos_efeitos(efeitos: &[Tolerante<Efeito>]) -> (Afim, u8) {
    let mut g = Afim::IDENTIDADE;
    let mut q = 0u8;
    for e in efeitos {
        if let Tolerante::Conhecido(Efeito::Girar(gi)) = e {
            if gi.base.ligado {
                g = g.seguida_de(&afim_giro(gi.graus as i32, gi.espelhar_h, gi.espelhar_v));
                q = (q + quartos(gi.graus as i32)) % 4;
            }
        }
    }
    (g, q)
}

/// Primeiro `enquadramento` ligado (o enquadre vale no clipe inteiro: `ativo` não se aplica).
fn enquadramento(efeitos: &[Tolerante<Efeito>]) -> Option<&super::modelo::EfEnquadramento> {
    efeitos.iter().find_map(|e| match e {
        Tolerante::Conhecido(Efeito::Enquadramento(q)) if q.base.ligado => Some(q),
        _ => None,
    })
}

const INTEIRA: Caixa = Caixa { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

/// `A(τ)`: giro e espelho do `girar` + região do `enquadramento` em `tau` (s, base do clipe) + ajuste.
/// `origem` = tamanho exibido do espaço de origem (px); `canvas` = saída (px).
/// Sem `enquadramento`: imagem inteira com `conter`. A região é centrada e escalada por igual nos dois eixos:
/// `conter` mostra a região toda, `cobrir` enche o canvas. Região sem lado positivo = imagem inteira.
pub fn a_de(efeitos: &[Tolerante<Efeito>], tau: f64, origem: TamanhoPx, canvas: TamanhoPx) -> Afim {
    let (g, q) = giro_dos_efeitos(efeitos);
    let (wo, ho) = if q % 2 == 1 { (origem.h, origem.w) } else { (origem.w, origem.h) };
    let ajuste = enquadramento(efeitos).map(|q| q.ajuste).unwrap_or(Ajuste::Conter);
    let r = regiao_em(efeitos, tau);
    let r = if r.w > 0.0 && r.h > 0.0 && r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite() { r } else { INTEIRA };
    // p ≤ q: a região é mais alta (ou igual) que o canvas
    let p = (r.w * wo as f64) * canvas.h as f64;
    let qq = (r.h * ho as f64) * canvas.w as f64;
    if !(p > 0.0 && qq > 0.0 && p.is_finite() && qq.is_finite()) {
        return g;
    }
    let (sx, sy) = match ajuste {
        Ajuste::Conter if p <= qq => (p / qq, 1.0),
        Ajuste::Conter => (1.0, qq / p),
        Ajuste::Cobrir if p >= qq => (p / qq, 1.0),
        Ajuste::Cobrir => (1.0, qq / p),
    };
    let (kx, ky) = (sx / r.w, sy / r.h);
    let ex = (1.0 - sx) * 0.5 - r.x * kx;
    let ey = (1.0 - sy) * 0.5 - r.y * ky;
    g.seguida_de(&Afim([kx, 0.0, 0.0, ky, ex, ey]))
}

/// Região do enquadramento em `tau` (0..1 da origem já girada); sem o efeito, a imagem inteira.
/// Lista de chaves vazia (`"regiao": []` no arquivo) também vale a imagem inteira: aqui não há pânico.
pub fn regiao_em(efeitos: &[Tolerante<Efeito>], tau: f64) -> Caixa {
    match enquadramento(efeitos).map(|q| &q.regiao) {
        Some(Animavel::Chaves(ks)) if ks.is_empty() => INTEIRA,
        Some(r) => curva::avaliar(r, tau),
        None => INTEIRA,
    }
}

/// Caixa da origem (0..1) levada ao canvas (0..1). Espelho de `caixaNoCanvas`.
pub fn caixa_no_canvas(a: &Afim, c: Caixa) -> Caixa {
    a.caixa(c)
}

/// Identificador do espaço de origem do clipe: "original" ou "origem:<sha256>" dos efeitos geométricos (§3.2).
/// Entram `estabilizar` e `lente` ligados, na ordem. Texto do hash, uma linha por item (`\n` no fim de cada):
/// `sicro.edicao.origem/1`, o SHA-256 da mídia, e por efeito
/// `estabilizar <de_pts> <ate_pts> <suavizar> <zoom_pct> <trf_sha256 | -> <borda>` ou `lente <k1> <k2> <cx> <cy>`;
/// cada f64 como os 16 dígitos hexadecimais dos seus bits (zero sem sinal).
pub fn espaco_de_origem(midia_sha256: &str, efeitos: &[Tolerante<Efeito>]) -> String {
    let bits = |v: f64| format!("{:016x}", if v == 0.0 { 0u64 } else { v.to_bits() });
    let mut linhas = Vec::new();
    for e in efeitos {
        match e {
            Tolerante::Conhecido(Efeito::Estabilizar(s)) if s.base.ligado => {
                let borda = match s.borda {
                    super::modelo::BordaEstab::Preta => "preta",
                    super::modelo::BordaEstab::Ampliar => "ampliar",
                };
                linhas.push(format!(
                    "estabilizar {} {} {} {} {} {}",
                    s.analise.de_pts,
                    s.analise.ate_pts,
                    s.suavizar,
                    bits(s.zoom_pct),
                    s.trf_sha256.as_deref().unwrap_or("-"),
                    borda
                ));
            }
            Tolerante::Conhecido(Efeito::Lente(l)) if l.base.ligado => {
                linhas.push(format!("lente {} {} {} {}", bits(l.k1), bits(l.k2), bits(l.cx), bits(l.cy)));
            }
            _ => {}
        }
    }
    if linhas.is_empty() {
        return "original".into();
    }
    let mut texto = format!("sicro.edicao.origem/1\n{midia_sha256}\n");
    for l in linhas {
        texto.push_str(&l);
        texto.push('\n');
    }
    let h = Sha256::digest(texto.as_bytes());
    let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
    format!("origem:{hex}")
}

/// Pixel que contém a coordenada `v` (px, já multiplicada pelo lado), arredondado para baixo;
/// a menos de 1e-6 de um inteiro vale o inteiro (0,1 × 1920 = 192,00000000000003).
fn chao(v: f64) -> f64 {
    let r = v.round();
    if (v - r).abs() <= 1e-6 {
        r
    } else {
        v.floor()
    }
}

fn teto(v: f64) -> f64 {
    let r = v.round();
    if (v - r).abs() <= 1e-6 {
        r
    } else {
        v.ceil()
    }
}

/// Faixa par [a, a + n) de `0..total` que cobre [ini, fim) (0..1); n ≥ 2 quando cabe.
fn faixa_par(ini: f64, fim: f64, total: u32) -> (u32, u32) {
    let lim = total & !1;
    let para_px = |v: f64| -> u32 {
        if !(v > 0.0) {
            0
        } else if v >= lim as f64 {
            lim
        } else {
            v as u32
        }
    };
    let (ini, fim) = if fim < ini { (fim, ini) } else { (ini, fim) };
    let mut a = para_px(chao(ini * total as f64)) & !1;
    let mut b = (para_px(teto(fim * total as f64)) + 1) & !1;
    b = b.min(lim);
    if b < a + 2 {
        b = (a + 2).min(lim);
        a = b.saturating_sub(2);
    }
    (a, b - a)
}

/// Caixa 0..1 em pixels pares dentro de `tamanho`: [x, y, w, h], que cobre a caixa inteira.
/// Posição e lados pares (recorte, máscara e camada em yuv420p); lado mínimo 2; limitada ao maior
/// tamanho par contido em `tamanho`. Caixa fora da imagem vira 2 px colados na borda.
pub fn caixa_px_par(c: Caixa, tamanho: TamanhoPx) -> [u32; 4] {
    let (x, w) = faixa_par(c.x, c.x + c.w, tamanho.w);
    let (y, h) = faixa_par(c.y, c.y + c.h, tamanho.h);
    [x, y, w, h]
}

/// Caixa em pixels → 0..1 de `tamanho`.
pub fn caixa_de_px(px: [u32; 4], tamanho: TamanhoPx) -> Caixa {
    let (tw, th) = (tamanho.w as f64, tamanho.h as f64);
    Caixa { x: px[0] as f64 / tw, y: px[1] as f64 / th, w: px[2] as f64 / tw, h: px[3] as f64 / th }
}

/// Caixa com a margem aplicada (fração do lado), limitada a 0..1.
/// Margem negativa encolhe até o centro; ponto (lado 0) não muda.
pub fn com_margem(c: Caixa, margem: f64) -> Caixa {
    let lado = |ini: f64, tam: f64| -> (f64, f64) {
        let d = tam * margem;
        let (mut a, mut b) = (ini - d, (ini + tam) + d);
        if b < a {
            let meio = ini + tam * 0.5;
            a = meio;
            b = meio;
        }
        let a = a.max(0.0).min(1.0);
        let b = b.max(0.0).min(1.0);
        (a, b - a)
    };
    let (x, w) = lado(c.x, c.w);
    let (y, h) = lado(c.y, c.h);
    Caixa { x, y, w, h }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::curva::fixtures;
    use crate::edicao::modelo::{BordaEstab, Chave, EfBase, EfEnquadramento, EfEstabilizar, EfGirar, EfLente, FaixaAnalise, Interp, Interpolacao};
    use serde_json::{json, Value};

    fn base(id: &str) -> EfBase {
        EfBase { id: id.into(), ligado: true, nome: String::new(), ativo: None }
    }
    fn girar(graus: u16, h: bool, v: bool) -> Tolerante<Efeito> {
        Tolerante::Conhecido(Efeito::Girar(EfGirar { base: base("g"), graus, espelhar_h: h, espelhar_v: v }))
    }
    fn enq(regiao: Animavel<Caixa>, ajuste: Ajuste) -> Tolerante<Efeito> {
        Tolerante::Conhecido(Efeito::Enquadramento(EfEnquadramento { base: base("q"), regiao, ajuste, interpolacao: Interpolacao::Bilinear }))
    }
    fn cx(x: f64, y: f64, w: f64, h: f64) -> Caixa {
        Caixa { x, y, w, h }
    }
    fn tam(w: u32, h: u32) -> TamanhoPx {
        TamanhoPx { w, h }
    }
    fn perto(a: Ponto, b: Ponto, tol: f64) -> bool {
        (a.x - b.x).abs() <= tol && (a.y - b.y).abs() <= tol
    }
    fn grade() -> Vec<Ponto> {
        let mut v = Vec::new();
        for i in 0..=10 {
            for j in 0..=10 {
                v.push(Ponto { x: i as f64 / 10.0 + 0.0123 * j as f64, y: j as f64 / 10.0 - 0.0071 * i as f64 });
            }
        }
        v
    }
    /// ponto → canvas → ponto a menos de 1e-9.
    fn ida_e_volta(a: &Afim) {
        let inv = a.inversa().expect("inversível");
        for p in grade() {
            let volta = inv.ponto(a.ponto(p));
            assert!(perto(volta, p, 1e-9), "{p:?} voltou {volta:?}");
        }
    }

    const PIXEL_10_11: TamanhoPx = TamanhoPx { w: 654, h: 480 };

    #[test]
    fn a_16x9_4x3_vertical_pixel_10_11_giro_90() {
        let c720 = tam(1280, 720);
        // 16:9 → 16:9: identidade
        let a = a_de(&[], 0.0, tam(1920, 1080), c720);
        assert_eq!(a, Afim::IDENTIDADE);
        ida_e_volta(&a);
        // 4:3 → 16:9, conter: faixas laterais de 1/8
        let a = a_de(&[], 0.0, tam(640, 480), c720);
        assert_eq!(a, Afim([0.75, 0.0, 0.0, 1.0, 0.125, 0.0]));
        ida_e_volta(&a);
        // vertical 1080×1920: largura 0,31640625 do canvas, centrada
        let a = a_de(&[], 0.0, tam(1080, 1920), c720);
        assert_eq!(a.ponto(Ponto { x: 0.0, y: 0.0 }), Ponto { x: 0.341796875, y: 0.0 });
        assert_eq!(a.ponto(Ponto { x: 1.0, y: 1.0 }), Ponto { x: 0.658203125, y: 1.0 });
        ida_e_volta(&a);
        // pixel 10:11 (720×480 → 654×480)
        assert_eq!(exibido(tam(720, 480), Racional { num: 10, den: 11 }, 0), PIXEL_10_11);
        let a = a_de(&[], 0.0, PIXEL_10_11, c720);
        assert_eq!(a.0[0], 654.0 * 720.0 / (480.0 * 1280.0));
        assert_eq!(a.0[0], 0.76640625);
        ida_e_volta(&a);
        // giro de 90°: o canto superior esquerdo da origem vai para o superior direito da imagem
        let a = a_de(&[girar(90, false, false)], 0.0, tam(1920, 1080), c720);
        assert_eq!(a.ponto(Ponto { x: 0.0, y: 0.0 }), Ponto { x: 0.658203125, y: 0.0 });
        assert_eq!(a.ponto(Ponto { x: 0.0, y: 1.0 }), Ponto { x: 0.341796875, y: 0.0 });
        assert_eq!(a.ponto(Ponto { x: 1.0, y: 0.0 }), Ponto { x: 0.658203125, y: 1.0 });
        ida_e_volta(&a);
    }

    /// Três casos com pixel não quadrado e giro, refeitos à parte com frações (px da imagem girada,
    /// escala única, centragem): os esperados abaixo são as frações exatas.
    #[test]
    fn a_com_sar_e_giro_a_mao() {
        let ve = |a: &Afim, p: (f64, f64), e: (f64, f64)| {
            let r = a.ponto(Ponto { x: p.0, y: p.1 });
            assert!(perto(r, Ponto { x: e.0, y: e.1 }, 1e-12), "{p:?}: {r:?} × {e:?}");
        };
        // 1. NTSC 720×480 10:11 (654×480) com `girar` 90 em 1280×720, conter: a imagem vira 480×654
        let o = exibido(tam(720, 480), Racional { num: 10, den: 11 }, 0);
        let a = a_de(&[girar(90, false, false)], 0.0, o, tam(1280, 720));
        ve(&a, (0.0, 0.0), (77.0 / 109.0, 0.0));
        ve(&a, (1.0, 1.0), (32.0 / 109.0, 1.0));
        ve(&a, (0.25, 0.5), (0.5, 0.25));
        ida_e_volta(&a);
        // 2. PAL 720×576 16:15 (768×576), `girar` 270 + espelho horizontal, região fixa, cobrir, em 1920×1080
        let o = exibido(tam(720, 576), Racional { num: 16, den: 15 }, 0);
        assert_eq!(o, tam(768, 576));
        let ef = [girar(270, true, false), enq(Animavel::Fixo(cx(0.25, 0.125, 0.5, 0.5)), Ajuste::Cobrir)];
        let a = a_de(&ef, 0.0, o, tam(1920, 1080));
        ve(&a, (0.0, 0.0), (1.5, 187.0 / 54.0));
        ve(&a, (1.0, 1.0), (-0.5, -23.0 / 18.0));
        ve(&a, (0.25, 0.5), (0.5, 41.0 / 18.0));
        ve(&a, (0.5, 0.5), (0.5, 59.0 / 54.0));
        ida_e_volta(&a);
        // 3. 1440×1080 4:3 com giro de 90 no contêiner (1080×1920), `girar` 180 + espelho vertical,
        //    zoom suave em τ = 3 (f = 0,25 → 0,15625), conter, em 1280×720
        let o = exibido(tam(1440, 1080), Racional { num: 4, den: 3 }, 90);
        assert_eq!(o, tam(1080, 1920));
        let zoom = Animavel::Chaves(vec![
            Chave { t: 2.0, pts: None, v: cx(0.0, 0.0, 1.0, 1.0), interp: Interp::Suave },
            Chave { t: 6.0, pts: None, v: cx(0.5, 0.25, 0.25, 0.5), interp: Interp::Linear },
        ]);
        let ef = [girar(180, false, true), enq(zoom, Ajuste::Conter)];
        assert_eq!(regiao_em(&ef, 3.0), cx(5.0 / 64.0, 5.0 / 128.0, 113.0 / 128.0, 59.0 / 64.0));
        let a = a_de(&ef, 3.0, o, tam(1280, 720));
        ve(&a, (0.0, 0.0), (40171.0 / 60416.0, -5.0 / 118.0));
        ve(&a, (1.0, 1.0), (19435.0 / 60416.0, 123.0 / 118.0));
        ve(&a, (0.25, 0.5), (593.0 / 1024.0, 0.5));
        ida_e_volta(&a);
    }

    #[test]
    fn giros_e_espelhos() {
        let quad = tam(1000, 1000);
        let p = Ponto { x: 0.25, y: 0.125 };
        for (graus, h, v, esperado) in [
            (0, false, false, (0.25, 0.125)),
            (90, false, false, (0.875, 0.25)),
            (180, false, false, (0.75, 0.875)),
            (270, false, false, (0.125, 0.75)),
            (0, true, false, (0.75, 0.125)),
            (0, false, true, (0.25, 0.875)),
            // gira e depois espelha na imagem girada
            (90, true, false, (0.125, 0.25)),
        ] {
            let a = a_de(&[girar(graus, h, v)], 0.0, quad, quad);
            assert_eq!(a.ponto(p), Ponto { x: esperado.0, y: esperado.1 }, "{graus} {h} {v}");
            ida_e_volta(&a);
        }
        // dois giros de 90 = 180; giro desligado não conta
        let a = a_de(&[girar(90, false, false), girar(90, false, false)], 0.0, quad, quad);
        assert_eq!(a, a_de(&[girar(180, false, false)], 0.0, quad, quad));
        let mut g = girar(90, false, false);
        if let Tolerante::Conhecido(Efeito::Girar(x)) = &mut g {
            x.base.ligado = false;
        }
        assert_eq!(a_de(&[g, Tolerante::Desconhecido(json!({ "tipo": "novo" }))], 0.0, quad, quad), Afim::IDENTIDADE);
        // giro do contêiner com sinal: −90 = 270
        assert_eq!(afim_giro(-90, false, false), afim_giro(270, false, false));
        assert_eq!(afim_giro(450, false, false), afim_giro(90, false, false));
    }

    #[test]
    fn exibido_sar_e_giro() {
        let r1 = Racional { num: 1, den: 1 };
        assert_eq!(exibido(tam(1920, 1080), r1, 0), tam(1920, 1080));
        assert_eq!(exibido(tam(1920, 1080), r1, 90), tam(1080, 1920));
        assert_eq!(exibido(tam(1920, 1080), r1, -90), tam(1080, 1920));
        assert_eq!(exibido(tam(1920, 1080), r1, 180), tam(1920, 1080));
        assert_eq!(exibido(tam(720, 480), Racional { num: 10, den: 11 }, 90), tam(480, 654));
        assert_eq!(exibido(tam(720, 576), Racional { num: 64, den: 45 }, 0), tam(1024, 576));
        assert_eq!(exibido(tam(1440, 1080), Racional { num: 4, den: 3 }, 0), tam(1920, 1080));
        // sar ausente (0:1) = quadrado; lado ímpar desce para par
        assert_eq!(exibido(tam(641, 361), Racional { num: 0, den: 1 }, 0), tam(640, 360));
    }

    #[test]
    fn conter_e_cobrir_com_regiao() {
        let o = tam(1920, 1080);
        let c = tam(1280, 720);
        // região quadrada no meio: conter = faixas; cobrir = corta em cima e embaixo
        let r = cx(0.25, 0.0, 0.5625, 1.0);
        let a = a_de(&[enq(Animavel::Fixo(r), Ajuste::Conter)], 0.0, o, c);
        let k = a.caixa(r);
        assert!((k.w - 0.5625).abs() < 1e-15 && (k.h - 1.0).abs() < 1e-15 && (k.x - 0.21875).abs() < 1e-15 && k.y.abs() < 1e-15, "{k:?}");
        ida_e_volta(&a);
        let a = a_de(&[enq(Animavel::Fixo(r), Ajuste::Cobrir)], 0.0, o, c);
        let k = a.caixa(r);
        assert!((k.w - 1.0).abs() < 1e-15 && (k.x).abs() < 1e-15 && (k.h - 1.0 / 0.5625).abs() < 1e-12, "{k:?}");
        ida_e_volta(&a);
        // região com a proporção do canvas: conter = cobrir = a região enche o canvas
        let r = cx(0.1, 0.2, 0.5, 0.5);
        for aj in [Ajuste::Conter, Ajuste::Cobrir] {
            let a = a_de(&[enq(Animavel::Fixo(r), aj)], 0.0, o, c);
            assert_eq!(a, Afim([2.0, 0.0, 0.0, 2.0, -0.2, -0.4]));
        }
        // região degenerada = imagem inteira
        let a = a_de(&[enq(Animavel::Fixo(cx(0.2, 0.2, 0.0, 0.3)), Ajuste::Conter)], 0.0, o, c);
        assert_eq!(a, Afim::IDENTIDADE);
        // lista de chaves vazia (arquivo editado à mão): imagem inteira, sem pânico
        let vazia = [enq(Animavel::Chaves(vec![]), Ajuste::Cobrir)];
        assert_eq!(regiao_em(&vazia, 1.0), INTEIRA);
        assert_eq!(a_de(&vazia, 1.0, o, c), Afim::IDENTIDADE);
    }

    #[test]
    fn zoom_animado() {
        let o = tam(1920, 1080);
        let c = tam(1280, 720);
        let regiao = Animavel::Chaves(vec![
            Chave { t: 10.0, pts: Some(153600), v: cx(0.0, 0.0, 1.0, 1.0), interp: Interp::Suave },
            Chave { t: 12.0, pts: Some(184320), v: cx(0.5, 0.5, 0.25, 0.25), interp: Interp::Linear },
        ]);
        let ef = [enq(regiao, Ajuste::Conter)];
        assert_eq!(a_de(&ef, 9.0, o, c), Afim::IDENTIDADE);
        // meio do caminho (suave em 0,5 = 0,5): região (0,25; 0,25; 0,625; 0,625)
        let a = a_de(&ef, 11.0, o, c);
        assert_eq!(regiao_em(&ef, 11.0), cx(0.25, 0.25, 0.625, 0.625));
        assert!(perto(a.ponto(Ponto { x: 0.25, y: 0.25 }), Ponto { x: 0.0, y: 0.0 }, 1e-15));
        assert!(perto(a.ponto(Ponto { x: 0.875, y: 0.875 }), Ponto { x: 1.0, y: 1.0 }, 1e-15));
        ida_e_volta(&a);
        let a = a_de(&ef, 20.0, o, c);
        assert_eq!(a, Afim([4.0, 0.0, 0.0, 4.0, -2.0, -2.0]));
        ida_e_volta(&a);
        // com giro de 90: a região é da origem girada
        let ef2 = [girar(90, false, false), ef[0].clone()];
        let a = a_de(&ef2, 11.5, o, c);
        ida_e_volta(&a);
    }

    #[test]
    fn caixa_no_canvas_girada() {
        let a = a_de(&[girar(90, false, false)], 0.0, tam(1000, 1000), tam(1000, 1000));
        let k = caixa_no_canvas(&a, cx(0.0, 0.0, 0.5, 0.25));
        assert_eq!(k, cx(0.75, 0.0, 0.25, 0.5));
        let a = a_de(&[girar(270, true, true)], 0.0, tam(1000, 1000), tam(1000, 1000));
        let k = caixa_no_canvas(&a, cx(0.1, 0.2, 0.3, 0.4));
        // 270 + os dois espelhos = 90
        let a90 = a_de(&[girar(90, false, false)], 0.0, tam(1000, 1000), tam(1000, 1000));
        let k90 = caixa_no_canvas(&a90, cx(0.1, 0.2, 0.3, 0.4));
        assert!((k.x - k90.x).abs() < 1e-15 && (k.y - k90.y).abs() < 1e-15 && (k.w - k90.w).abs() < 1e-15 && (k.h - k90.h).abs() < 1e-15);
    }

    #[test]
    fn sem_zero_negativo() {
        let neg = |v: f64| v == 0.0 && v.is_sign_negative();
        for g in [0, 90, 180, 270] {
            for (h, v) in [(false, false), (true, false), (false, true), (true, true)] {
                let a = a_de(&[girar(g as u16, h, v)], 0.0, tam(1920, 1080), tam(1280, 720));
                let inv = a.inversa().unwrap();
                assert!(!a.0.iter().chain(inv.0.iter()).any(|x| neg(*x)), "{g} {h} {v}: {a:?} {inv:?}");
                let p = inv.ponto(a.ponto(Ponto { x: 0.0, y: 0.0 }));
                let k = a.caixa(cx(0.0, 0.0, 0.0, 0.0));
                assert!(![p.x, p.y, k.x, k.y, k.w, k.h].iter().any(|x| neg(*x)));
            }
        }
    }

    #[test]
    fn inversa_degenerada() {
        assert!(Afim([0.0, 0.0, 0.0, 0.0, 1.0, 1.0]).inversa().is_none());
        assert!(Afim([1.0, 2.0, 2.0, 4.0, 0.0, 0.0]).inversa().is_none());
        assert_eq!(Afim::IDENTIDADE.inversa(), Some(Afim::IDENTIDADE));
    }

    #[test]
    fn px_pares_cobrem_a_caixa() {
        let t = tam(1920, 1080);
        assert_eq!(caixa_px_par(cx(0.0, 0.0, 1.0, 1.0), t), [0, 0, 1920, 1080]);
        // 0,1 × 1920 = 192,00000000000003: não vira 194
        assert_eq!(caixa_px_par(cx(0.1, 0.1, 0.2, 0.2), t), [192, 108, 384, 216]);
        // bordas ímpares abrem para fora
        let px = caixa_px_par(cx(101.0 / 1920.0, 51.0 / 1080.0, 37.5 / 1920.0, 10.2 / 1080.0), t);
        assert_eq!(px, [100, 50, 40, 12]);
        // ponto ganha 2 px; fora da imagem cola na borda
        assert_eq!(caixa_px_par(cx(0.5, 0.5, 0.0, 0.0), t), [960, 540, 2, 2]);
        assert_eq!(caixa_px_par(cx(1.2, -0.5, 0.1, 0.1), t), [1918, 0, 2, 2]);
        // tamanho ímpar: limitado ao par contido
        assert_eq!(caixa_px_par(cx(0.0, 0.0, 1.0, 1.0), tam(655, 481)), [0, 0, 654, 480]);
        // sorteio: par, dentro, cobrindo
        let mut s: u64 = 99;
        for _ in 0..2000 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
            let u = |k: u32| ((s >> k) & 0xffff) as f64 / 65535.0;
            let c = cx(u(0) * 0.9, u(16) * 0.9, u(32) * 0.1, u(48) * 0.1);
            let [x, y, w, h] = caixa_px_par(c, t);
            assert!(x % 2 == 0 && y % 2 == 0 && w % 2 == 0 && h % 2 == 0 && w >= 2 && h >= 2);
            assert!(x + w <= 1920 && y + h <= 1080);
            assert!(x as f64 <= c.x * 1920.0 + 1e-6 && (x + w) as f64 >= (c.x + c.w) * 1920.0 - 1e-6);
            assert!(y as f64 <= c.y * 1080.0 + 1e-6 && (y + h) as f64 >= (c.y + c.h) * 1080.0 - 1e-6);
        }
        assert_eq!(caixa_de_px([192, 108, 384, 216], t), cx(0.1, 0.1, 0.2, 0.2));
    }

    #[test]
    fn margem() {
        assert_eq!(com_margem(cx(0.25, 0.25, 0.5, 0.25), 0.25), cx(0.125, 0.1875, 0.75, 0.375));
        // limitada a 0..1
        assert_eq!(com_margem(cx(0.0, 0.5, 0.5, 0.5), 0.5), cx(0.0, 0.25, 0.75, 0.75));
        // negativa encolhe até o centro
        assert_eq!(com_margem(cx(0.25, 0.25, 0.5, 0.5), -0.25), cx(0.375, 0.375, 0.25, 0.25));
        assert_eq!(com_margem(cx(0.25, 0.25, 0.5, 0.5), -2.0), cx(0.5, 0.5, 0.0, 0.0));
        // ponto não muda
        assert_eq!(com_margem(cx(0.3, 0.6, 0.0, 0.0), 0.4), cx(0.3, 0.6, 0.0, 0.0));
    }

    fn estab(zoom: f64, trf: Option<&str>) -> Tolerante<Efeito> {
        Tolerante::Conhecido(Efeito::Estabilizar(EfEstabilizar {
            base: base("e"),
            suavizar: 15,
            borda: BordaEstab::Preta,
            analise: FaixaAnalise { de_s: 1.0, ate_s: 9.0, de_pts: 15360, ate_pts: 138240 },
            zoom_pct: zoom,
            trf_sha256: trf.map(String::from),
        }))
    }
    fn lente(k1: f64) -> Tolerante<Efeito> {
        Tolerante::Conhecido(Efeito::Lente(EfLente { base: base("l"), k1, k2: 0.01, cx: 0.5, cy: 0.5, perfil: "gopro".into() }))
    }

    #[test]
    fn espaco_de_origem_pelos_efeitos_geometricos() {
        let sha = "ab".repeat(32);
        assert_eq!(espaco_de_origem(&sha, &[]), "original");
        // tom e enquadre não mudam o espaço
        assert_eq!(espaco_de_origem(&sha, &[girar(90, true, false), enq(Animavel::Fixo(INTEIRA), Ajuste::Cobrir)]), "original");
        let e = espaco_de_origem(&sha, &[estab(0.0, Some("cd"))]);
        assert!(e.starts_with("origem:") && e.len() == 7 + 64 && e[7..].chars().all(|c| c.is_ascii_hexdigit()));
        // o mesmo texto dá o mesmo nome; cada parâmetro muda o nome
        assert_eq!(e, espaco_de_origem(&sha, &[girar(90, false, false), estab(0.0, Some("cd"))]));
        assert_eq!(e, espaco_de_origem(&sha, &[estab(-0.0, Some("cd"))]));
        for outro in [
            espaco_de_origem(&"ac".repeat(32), &[estab(0.0, Some("cd"))]),
            espaco_de_origem(&sha, &[estab(5.0, Some("cd"))]),
            espaco_de_origem(&sha, &[estab(0.0, None)]),
            espaco_de_origem(&sha, &[estab(0.0, Some("cd")), lente(-0.2)]),
            espaco_de_origem(&sha, &[lente(-0.2), estab(0.0, Some("cd"))]),
        ] {
            assert_ne!(e, outro);
        }
        // desligado não conta
        let mut d = estab(0.0, Some("cd"));
        if let Tolerante::Conhecido(Efeito::Estabilizar(x)) = &mut d {
            x.base.ligado = false;
        }
        assert_eq!(espaco_de_origem(&sha, &[d]), "original");
        // texto canônico fixo: o nome não muda entre versões
        let texto = format!("sicro.edicao.origem/1\n{sha}\nlente {:016x} {:016x} {:016x} {:016x}\n", (-0.2f64).to_bits(), 0.01f64.to_bits(), 0.5f64.to_bits(), 0.5f64.to_bits());
        let h: String = Sha256::digest(texto.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(espaco_de_origem(&sha, &[lente(-0.2)]), format!("origem:{h}"));
    }

    /// `geometria-a.json`: `A(τ)`, a inversa, a região e pontos e caixas levados ao canvas, em 16:9, 4:3,
    /// vertical, pixel 10:11, giros de 90/180/270 com espelhos, zoom animado (as três interpolações) e
    /// conter/cobrir.
    #[test]
    fn fixture_a() {
        let c720 = tam(1280, 720);
        let zoom = Animavel::Chaves(vec![
            Chave { t: 1.0, pts: Some(15360), v: cx(0.0, 0.0, 1.0, 1.0), interp: Interp::Suave },
            Chave { t: 3.0, pts: Some(46080), v: cx(0.3, 0.2, 0.4, 0.4), interp: Interp::Linear },
            Chave { t: 4.5, pts: Some(69120), v: cx(0.1, 0.5, 0.3, 0.3), interp: Interp::Degrau },
            Chave { t: 5.0, pts: Some(76800), v: cx(0.6, 0.55, 0.4, 0.45), interp: Interp::Linear },
        ]);
        // o `Value` é o que vai para a fixture como `efeitos` (None = o que o serde grava)
        let mut crus: Vec<(String, Value, f64, TamanhoPx, TamanhoPx)> = Vec::new();
        let mut entradas: Vec<(String, Vec<Tolerante<Efeito>>, f64, TamanhoPx, TamanhoPx)> = vec![
            ("16:9 em 16:9".into(), vec![], 0.0, tam(1920, 1080), tam(1920, 1080)),
            ("16:9 em 1280x720".into(), vec![], 0.0, tam(1920, 1080), c720),
            ("4:3 conter".into(), vec![], 0.0, tam(640, 480), c720),
            ("4:3 cobrir".into(), vec![enq(Animavel::Fixo(INTEIRA), Ajuste::Cobrir)], 0.0, tam(640, 480), c720),
            ("vertical".into(), vec![], 0.0, tam(1080, 1920), c720),
            ("vertical cobrir".into(), vec![enq(Animavel::Fixo(INTEIRA), Ajuste::Cobrir)], 0.0, tam(1080, 1920), c720),
            ("pixel 10:11 (720x480 -> 654x480)".into(), vec![], 0.0, PIXEL_10_11, c720),
            ("pixel 10:11 em 640x480".into(), vec![], 0.0, PIXEL_10_11, tam(640, 480)),
            ("conteiner girado 90 (1920x1080 -> 1080x1920)".into(), vec![], 0.0, exibido(tam(1920, 1080), Racional { num: 1, den: 1 }, 90), c720),
            ("regiao fixa conter".into(), vec![enq(Animavel::Fixo(cx(0.25, 0.1, 0.3, 0.8)), Ajuste::Conter)], 0.0, tam(1920, 1080), c720),
            ("regiao fixa cobrir".into(), vec![enq(Animavel::Fixo(cx(0.25, 0.1, 0.3, 0.8)), Ajuste::Cobrir)], 0.0, tam(1920, 1080), c720),
        ];
        for g in [90u16, 180, 270] {
            for (h, v) in [(false, false), (true, false), (false, true), (true, true)] {
                entradas.push((format!("girar {g} h={h} v={v}"), vec![girar(g, h, v)], 0.0, tam(1920, 1080), c720));
            }
        }
        for t in [0.0, 1.0, 1.7, 2.0, 2.999, 3.0, 3.6, 4.5, 4.9, 5.0, 6.0] {
            entradas.push((format!("zoom animado t={t}"), vec![enq(zoom.clone(), Ajuste::Conter)], t, tam(1920, 1080), c720));
        }
        for t in [2.0, 4.0] {
            entradas.push((format!("girar 90 + zoom animado t={t}"), vec![girar(90, false, false), enq(zoom.clone(), Ajuste::Cobrir)], t, tam(1920, 1080), c720));
            entradas.push((format!("zoom animado pixel 10:11 t={t}"), vec![enq(zoom.clone(), Ajuste::Conter)], t, PIXEL_10_11, c720));
        }
        // pixel não quadrado com giro; PAL 16:15; contêiner girado com pixel 4:3
        let ntsc = exibido(tam(720, 480), Racional { num: 10, den: 11 }, 0);
        let pal = exibido(tam(720, 576), Racional { num: 16, den: 15 }, 0);
        let celular = exibido(tam(1440, 1080), Racional { num: 4, den: 3 }, 90);
        entradas.push(("pixel 10:11 + girar 90".into(), vec![girar(90, false, false)], 0.0, ntsc, c720));
        entradas.push((
            "PAL 16:15 + girar 270 h + regiao cobrir em 1920x1080".into(),
            vec![girar(270, true, false), enq(Animavel::Fixo(cx(0.25, 0.125, 0.5, 0.5)), Ajuste::Cobrir)],
            0.0,
            pal,
            tam(1920, 1080),
        ));
        for t in [3.0, 5.5] {
            entradas.push((format!("conteiner 90 pixel 4:3 + girar 180 v + zoom t={t}"), vec![girar(180, false, true), enq(zoom.clone(), Ajuste::Conter)], t, celular, c720));
        }
        // o que a lista de efeitos pode trazer: dois giros, item desligado, item desconhecido, dois enquadramentos
        let desligado = |mut e: Tolerante<Efeito>| {
            match &mut e {
                Tolerante::Conhecido(Efeito::Girar(g)) => g.base.ligado = false,
                Tolerante::Conhecido(Efeito::Enquadramento(q)) => q.base.ligado = false,
                _ => {}
            }
            e
        };
        let desconhecido = Tolerante::Desconhecido(json!({ "tipo": "efeito_do_futuro", "id": "x", "graus": 90 }));
        let r1 = cx(0.25, 0.1, 0.3, 0.8);
        let r2 = cx(0.5, 0.5, 0.5, 0.5);
        entradas.push(("dois girar (90 h + 90)".into(), vec![girar(90, true, false), girar(90, false, false)], 0.0, tam(1920, 1080), c720));
        entradas.push(("girar desligado + desconhecido".into(), vec![desligado(girar(90, false, false)), desconhecido.clone()], 0.0, tam(1920, 1080), c720));
        entradas.push((
            "enquadramento desligado + ligado".into(),
            vec![desligado(enq(Animavel::Fixo(r1), Ajuste::Cobrir)), desconhecido, enq(Animavel::Fixo(r2), Ajuste::Conter)],
            0.0,
            tam(1920, 1080),
            c720,
        ));
        entradas.push(("dois enquadramentos ligados (vale o primeiro)".into(), vec![enq(Animavel::Fixo(r1), Ajuste::Cobrir), enq(Animavel::Fixo(r2), Ajuste::Conter)], 0.0, tam(1920, 1080), c720));
        entradas.push(("enquadramento antes do girar".into(), vec![enq(Animavel::Fixo(r1), Ajuste::Conter), girar(90, false, false)], 0.0, tam(1920, 1080), c720));
        // região fora de 0..1 (não é limitada) e região sem tamanho (vale a imagem inteira; `regiao` sai como está)
        entradas.push(("regiao fora de 0..1".into(), vec![enq(Animavel::Fixo(cx(-0.25, 0.5, 1.5, 0.75)), Ajuste::Conter)], 0.0, tam(1920, 1080), c720));
        entradas.push(("regiao sem largura".into(), vec![enq(Animavel::Fixo(cx(0.2, 0.2, 0.0, 0.3)), Ajuste::Cobrir)], 0.0, tam(640, 480), c720));
        entradas.push(("regiao com altura negativa".into(), vec![enq(Animavel::Fixo(cx(0.2, 0.2, 0.3, -0.3)), Ajuste::Conter)], 0.0, tam(640, 480), c720));
        let pontos = [Ponto { x: 0.0, y: 0.0 }, Ponto { x: 1.0, y: 1.0 }, Ponto { x: 0.5, y: 0.5 }, Ponto { x: 0.3, y: 0.7 }, Ponto { x: 0.91, y: 0.13 }];
        let caixas = [cx(0.0, 0.0, 1.0, 1.0), cx(0.1, 0.2, 0.3, 0.4), cx(0.45, 0.6, 0.0, 0.0), cx(0.62, 0.05, 0.25, 0.1)];
        // como o front grava: chave sem `interp` (= linear) e sem `pts`; e a lista de chaves vazia
        let sem_interp = json!([{ "tipo": "enquadramento", "id": "q", "ligado": true, "nome": "", "ativo": null, "ajuste": "conter", "interpolacao": "bicubica",
            "regiao": [{ "t": 1, "v": { "x": 0, "y": 0, "w": 1, "h": 1 } }, { "t": 3, "v": { "x": 0.3, "y": 0.2, "w": 0.4, "h": 0.4 }, "interp": "degrau" }, { "t": 4, "v": { "x": 0.5, "y": 0.5, "w": 0.5, "h": 0.5 } }] }]);
        for t in [2.5, 3.5] {
            crus.push((format!("chaves sem interp nem pts t={t}"), sem_interp.clone(), t, tam(1920, 1080), c720));
        }
        let sem_chaves = json!([{ "tipo": "enquadramento", "id": "q", "ligado": true, "nome": "", "ativo": null, "ajuste": "cobrir", "interpolacao": "bilinear", "regiao": [] }]);
        crus.push(("regiao sem chaves (imagem inteira)".into(), sem_chaves, 0.0, tam(640, 480), c720));
        let todas = entradas.into_iter().map(|(n, e, t, o, c)| (n, json!(e), t, o, c)).chain(crus);
        let casos: Vec<Value> = todas
            .map(|(nome, efeitos_json, tau, origem, canvas)| {
                let efeitos: Vec<Tolerante<Efeito>> = serde_json::from_value(efeitos_json.clone()).expect("efeitos legíveis");
                assert!(efeitos.iter().all(|e| matches!(e, Tolerante::Conhecido(_)) || e == &Tolerante::Desconhecido(json!({ "tipo": "efeito_do_futuro", "id": "x", "graus": 90 }))), "{nome}: efeito caiu em Desconhecido");
                let a = a_de(&efeitos, tau, origem, canvas);
                let inv = a.inversa().expect("inversível");
                for p in &pontos {
                    assert!(perto(inv.ponto(a.ponto(*p)), *p, 1e-9), "{nome}");
                }
                json!({
                    "nome": nome,
                    "entrada": { "efeitos": efeitos_json, "tau": tau, "origem": origem, "canvas": canvas, "pontos": pontos, "caixas": caixas },
                    "saida": {
                        "a": a, "inversa": inv, "regiao": regiao_em(&efeitos, tau),
                        "pontos": pontos.iter().map(|p| a.ponto(*p)).collect::<Vec<_>>(),
                        "caixas": caixas.iter().map(|c| caixa_no_canvas(&a, *c)).collect::<Vec<_>>(),
                    }
                })
            })
            .collect();
        fixtures::conferir("geometria-a.json", Value::Array(casos));
    }
}
