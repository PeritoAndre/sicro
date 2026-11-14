//! `girar` e `enquadramento` (§5.4): fixo por `crop` + `scale` + `pad`; animado por `perspective`. Dono: P0.6a.
//! O retângulo de origem sai de `A(τ)` (`geometria::a_de`): é `A⁻¹` do canvas, limitado à região e à imagem.

use crate::edicao::cor;
use crate::edicao::geometria::{self, Afim};
use crate::edicao::modelo::{Animavel, Caixa, EfEnquadramento, Efeito, Interpolacao, Ponto, TamanhoPx, Tolerante};
use crate::edicao::motor::expr;
use crate::edicao::motor::grafo::{filtro, scale};
use crate::edicao::motor::plano::Canvas;
use crate::error::{Result, SicroError};

/// Desvio aceito (px do canvas) no retângulo de destino entre os slots de um enquadramento animado.
const DESTINO_FIXO_PX: f64 = 0.5;
/// Erro aceito (px do canvas) na tabela da região animada.
const TABELA_PX: f64 = 0.1;

fn enquadramento(efeitos: &[Tolerante<Efeito>]) -> Option<&EfEnquadramento> {
    efeitos.iter().find_map(|e| match e {
        Tolerante::Conhecido(Efeito::Enquadramento(q)) if q.base.ligado => Some(q),
        _ => None,
    })
}

/// O enquadramento do clipe é animado (usa `perspective`)? Chaves todas iguais valem como região fixa: dão o
/// mesmo quadro do caso fixo (decisão (f) 5). Vale para o clipe inteiro, não para a janela: o quadro final passa
/// pelo mesmo filtro da exportação.
pub fn animado(efeitos: &[Tolerante<Efeito>]) -> bool {
    match enquadramento(efeitos).map(|q| &q.regiao) {
        Some(Animavel::Chaves(k)) => k.windows(2).any(|p| p[0].v != p[1].v),
        _ => false,
    }
}

/// Giro composto dos `girar` ligados: a afim, os filtros (transpose → hflip → vflip de cada um) e se troca os lados.
fn giro(efeitos: &[Tolerante<Efeito>]) -> (Afim, Vec<String>, bool) {
    let (mut g, mut f, mut quartos) = (Afim::IDENTIDADE, Vec::new(), 0u32);
    for e in efeitos {
        let Tolerante::Conhecido(Efeito::Girar(gi)) = e else { continue };
        if !gi.base.ligado {
            continue;
        }
        g = g.seguida_de(&geometria::afim_giro(gi.graus as i32, gi.espelhar_h, gi.espelhar_v));
        let q = ((gi.graus as u32 % 360 + 45) / 90) % 4;
        match q {
            1 => f.push("transpose=dir=clock".to_string()),
            2 => f.extend(["hflip".to_string(), "vflip".to_string()]),
            3 => f.push("transpose=dir=cclock".to_string()),
            _ => {}
        }
        if gi.espelhar_h {
            f.push("hflip".to_string());
        }
        if gi.espelhar_v {
            f.push("vflip".to_string());
        }
        quartos += q;
    }
    (g, f, quartos % 2 == 1)
}

/// O que aparece: retângulo da origem já girada (px dela) e onde cai no canvas (px).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Enquadre {
    pub origem: [f64; 4],
    pub destino: [f64; 4],
}

/// Só a escala e o deslocamento de `A(τ)`: origem girada (0..1) → canvas (0..1), `[kx, ky, ex, ey]`.
fn escala(efeitos: &[Tolerante<Efeito>], tau: f64, origem: TamanhoPx, canvas: TamanhoPx) -> Option<[f64; 4]> {
    let (g, _, _) = giro(efeitos);
    let a = geometria::a_de(efeitos, tau, origem, canvas);
    let [kx, _, _, ky, ex, ey] = g.inversa()?.seguida_de(&a).0;
    (kx > 0.0 && ky > 0.0 && kx.is_finite() && ky.is_finite() && ex.is_finite() && ey.is_finite()).then_some([kx, ky, ex, ey])
}

/// Enquadre em `tau`: `A⁻¹` do canvas, cortado pela região (as barras do `conter` são fundo, nunca a imagem
/// vizinha) e pela imagem.
pub(crate) fn enquadre(efeitos: &[Tolerante<Efeito>], tau: f64, origem: TamanhoPx, canvas: TamanhoPx) -> Enquadre {
    let (_, _, troca) = giro(efeitos);
    let (wo, ho) = if troca { (origem.h as f64, origem.w as f64) } else { (origem.w as f64, origem.h as f64) };
    let (w, h) = (canvas.w as f64, canvas.h as f64);
    let inteiro = Enquadre { origem: [0.0, 0.0, wo, ho], destino: [0.0, 0.0, w, h] };
    let Some([kx, ky, ex, ey]) = escala(efeitos, tau, origem, canvas) else { return inteiro };
    let r = geometria::regiao_em(efeitos, tau);
    let r = if r.w > 0.0 && r.h > 0.0 && [r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) { r } else { Caixa { x: 0.0, y: 0.0, w: 1.0, h: 1.0 } };
    let lim = |a0: f64, a1: f64, e: f64, kk: f64| {
        let de = a0.max(-e / kk).max(0.0);
        let ate = a1.min((1.0 - e) / kk).min(1.0);
        (de, ate.max(de))
    };
    let (x0, x1) = lim(r.x, r.x + r.w, ex, kx);
    let (y0, y1) = lim(r.y, r.y + r.h, ey, ky);
    let c0 = Ponto { x: kx * x0 + ex, y: ky * y0 + ey };
    let c1 = Ponto { x: kx * x1 + ex, y: ky * y1 + ey };
    Enquadre { origem: [x0 * wo, y0 * ho, (x1 - x0) * wo, (y1 - y0) * ho], destino: [c0.x * w, c0.y * h, (c1.x - c0.x) * w, (c1.y - c0.y) * h] }
}

/// Retângulo da origem girada (px dela) que `A(τ)` leva exatamente ao destino `d` (px do canvas): o vídeo
/// enquadrado cai onde o desenho, que passa por `A(τ)`, cai.
fn origem_do_destino(efeitos: &[Tolerante<Efeito>], tau: f64, origem: TamanhoPx, canvas: TamanhoPx, d: [u32; 4]) -> Option<[f64; 4]> {
    let (_, _, troca) = giro(efeitos);
    let (wo, ho) = if troca { (origem.h as f64, origem.w as f64) } else { (origem.w as f64, origem.h as f64) };
    let [kx, ky, ex, ey] = escala(efeitos, tau, origem, canvas)?;
    let (w, h) = (canvas.w as f64, canvas.h as f64);
    let x0 = (d[0] as f64 / w - ex) / kx * wo;
    let x1 = ((d[0] + d[2]) as f64 / w - ex) / kx * wo;
    let y0 = (d[1] as f64 / h - ey) / ky * ho;
    let y1 = ((d[1] + d[3]) as f64 / h - ey) / ky * ho;
    Some([x0, y0, x1 - x0, y1 - y0])
}

fn par(v: f64) -> u32 {
    ((v / 2.0).round().max(1.0) * 2.0) as u32
}

fn par_abaixo(v: f64) -> u32 {
    ((v / 2.0).floor().max(0.0) * 2.0) as u32
}

fn par_acima(v: f64) -> u32 {
    ((v / 2.0).ceil().max(1.0) * 2.0) as u32
}

/// Destino em px pares POR DENTRO de [x0, x1) × [y0, y1) (px do canvas): [x, y, w, h].
fn destino_dentro(x0: f64, y0: f64, x1: f64, y1: f64, canvas: TamanhoPx) -> [u32; 4] {
    // folga para o erro de arredondamento não comer 2 px de um lado que já é par
    let acima = |v: f64| ((v / 2.0 - 1e-6).ceil().max(0.0) * 2.0) as u32;
    let abaixo = |v: f64| ((v / 2.0 + 1e-6).floor().max(0.0) * 2.0) as u32;
    let (a, b) = (acima(x0), abaixo(x1).min(canvas.w / 2 * 2));
    let (c, d) = (acima(y0), abaixo(y1).min(canvas.h / 2 * 2));
    let (w, h) = (b.saturating_sub(a).max(2), d.saturating_sub(c).max(2));
    [a.min(canvas.w.saturating_sub(w)), c.min(canvas.h.saturating_sub(h)), w, h]
}

/// Destino em px pares dentro do canvas: [x, y, w, h].
fn destino_px(d: [f64; 4], canvas: TamanhoPx) -> [u32; 4] {
    let w = par(d[2]).min(canvas.w / 2 * 2).max(2);
    let h = par(d[3]).min(canvas.h / 2 * 2).max(2);
    let x = par_abaixo(d[0] + 1.0).min(canvas.w.saturating_sub(w));
    let y = par_abaixo(d[1] + 1.0).min(canvas.h.saturating_sub(h));
    [x, y, w, h]
}

fn flags(i: Interpolacao) -> &'static str {
    match i {
        Interpolacao::Vizinho => "neighbor",
        Interpolacao::Bilinear => "bilinear",
        Interpolacao::Bicubica => "bicubic",
    }
}

fn pad(d: [u32; 4], canvas: &Canvas) -> Result<Option<String>> {
    if d[2] == canvas.w && d[3] == canvas.h {
        return Ok(None);
    }
    let c = cor::ler(&canvas.cor_fundo)?;
    Ok(Some(format!("pad={}:{}:{}:{}:color=0x{:02x}{:02x}{:02x}", canvas.w, canvas.h, d[0], d[1], c.r, c.g, c.b)))
}

/// Filtros (separados por vírgula) que levam a origem (`origem` px) ao canvas W×H; vazio = nada a fazer.
/// `tempos[i]` = tempo (s, base do clipe) do slot local `primeiro + i` do clipe: no trecho é o tempo de fonte do
/// quadro mostrado. A janela começa em `tempos[antes]`; os slots a mais (os `antes` da frente e os do fim, até
/// fechar um bloco de `expr::BLOCO_DA_TABELA`) só alinham a tabela da região animada, para a janela dar a conta
/// do clipe inteiro (`primeiro` múltiplo do bloco).
// CONTRATO: `tempos`, `primeiro` e `antes` no lugar de `n_q`. A região anda no tempo do clipe (de fonte, no
// trecho), que o número de slots não dá.
pub fn filtros(efeitos: &[Tolerante<Efeito>], origem: TamanhoPx, canvas: &Canvas, tempos: &[f64], primeiro: usize, antes: usize) -> Result<String> {
    let alvo = TamanhoPx { w: canvas.w, h: canvas.h };
    let (_, mut f, troca) = giro(efeitos);
    let (wo, ho) = if troca { (origem.h, origem.w) } else { (origem.w, origem.h) };
    let interp = enquadramento(efeitos).map(|q| q.interpolacao).unwrap_or(Interpolacao::Bicubica);
    let t0 = tempos.get(antes).or(tempos.first()).copied().unwrap_or(0.0);
    if !animado(efeitos) {
        let e = enquadre(efeitos, t0, origem, alvo);
        let d = destino_px(e.destino, alvo);
        let [sx, sy, sw, sh] = e.origem;
        // janela folgada em px pares → escala → recorte fino no destino (receitas-video §8: 0,8 px contra 7 do crop + scale)
        let (lx, ly) = (par_abaixo(sx), par_abaixo(sy));
        let (lx1, ly1) = (par_acima(sx + sw).min(wo / 2 * 2).max(lx + 2), par_acima(sy + sh).min(ho / 2 * 2).max(ly + 2));
        let (lw, lh) = (lx1 - lx, ly1 - ly);
        if lw != wo || lh != ho {
            f.push(format!("crop={lw}:{lh}:{lx}:{ly}:exact=1"));
        }
        let (fx, fy) = (e.destino[2] / sw.max(1e-9), e.destino[3] / sh.max(1e-9));
        let (tw, th) = (par(lw as f64 * fx).max(d[2]), par(lh as f64 * fy).max(d[3]));
        if tw != lw || th != lh {
            f.push(scale(tw, th, flags(interp)));
        }
        if tw != d[2] || th != d[3] {
            let cx = par_abaixo((sx - lx as f64) * tw as f64 / lw as f64 + 1.0).min(tw - d[2]);
            let cy = par_abaixo((sy - ly as f64) * th as f64 / lh as f64 + 1.0).min(th - d[3]);
            f.push(format!("crop={}:{}:{cx}:{cy}:exact=1", d[2], d[3]));
        }
        f.extend(pad(d, canvas)?);
        return Ok(f.join(","));
    }
    // animado: a região vira o quadro inteiro pelo `perspective`; o destino tem de ser o mesmo em todos os slots.
    // Destino e tolerância saem das chaves (do clipe inteiro), nunca da janela: o quadro final dá a conta da exportação.
    let chaves = tempos_das_chaves(efeitos, t0);
    let referencia = enquadre(efeitos, chaves[0], origem, alvo);
    let quadros: Vec<Enquadre> = tempos.iter().map(|&t| enquadre(efeitos, t, origem, alvo)).collect();
    if !mesmo_destino(&quadros, &referencia) {
        let id = enquadramento(efeitos).map(|q| q.base.id.as_str()).unwrap_or("");
        return Err(SicroError::Validation(format!("nao_suportado: enquadramento {id}: destino variável")));
    }
    let r = referencia.destino;
    let d = destino_dentro(r[0], r[1], r[0] + r[2], r[1] + r[3], alvo);
    // origem de cada slot = `A⁻¹` do destino em px pares (arredondar só o destino deslocava o vídeo até 1,4 px do desenho)
    let quadros: Vec<Enquadre> =
        tempos.iter().zip(quadros).map(|(&t, q)| Enquadre { origem: origem_do_destino(efeitos, t, origem, alvo, d).unwrap_or(q.origem), destino: q.destino }).collect();
    // tolerância em px da origem que dá `TABELA_PX` no canvas, no maior zoom (o tamanho da região é monótono entre chaves)
    let zoom = chaves
        .iter()
        .map(|&t| enquadre(efeitos, t, origem, alvo))
        .map(|q| (q.destino[2] / q.origem[2].max(1e-9)).max(q.destino[3] / q.origem[3].max(1e-9)))
        .fold(1.0f64, f64::max);
    let tol = TABELA_PX / zoom;
    let var = format!("(on-1)*{}/{}", canvas.fps.den, canvas.fps.num);
    // src(i) = x0 + (x1 − x0)·i/W; o centro do pixel i de saída cai em origem.x + (i + 0,5)·passo
    let col = |de: usize, lado: usize, total: f64| -> (Vec<f64>, Vec<f64>) {
        let a: Vec<f64> = quadros.iter().map(|q| q.origem[de] + 0.5 * q.origem[lado] / total - 0.5).collect();
        let b: Vec<f64> = quadros.iter().zip(&a).map(|(q, a0)| a0 + q.origem[lado]).collect();
        (a, b)
    };
    let (xa, xb) = col(0, 2, wo as f64);
    let (ya, yb) = col(1, 3, ho as f64);
    let t = |v: &[f64]| expr::tabela_alinhada(v, primeiro, antes, canvas.fps, tol, &var);
    let (xa, xb, ya, yb) = (t(&xa), t(&xb), t(&ya), t(&yb));
    // em 4:4:4: o `perspective` trata cada plano por si e, em 4:2:0, o croma anda até 0,75 px com o zoom
    f.push("scale=w=iw:h=ih:flags=bicubic+accurate_rnd+bitexact".to_string());
    f.push("format=yuv444p".to_string());
    f.push(filtro(
        "perspective",
        None,
        &[
            ("x0", xa.clone()), ("y0", ya.clone()), ("x1", xb.clone()), ("y1", ya), ("x2", xa), ("y2", yb.clone()), ("x3", xb), ("y3", yb),
            ("sense", "source".into()), ("eval", "frame".into()),
            ("interpolation", if interp == Interpolacao::Bicubica { "cubic" } else { "linear" }.into()),
        ],
    ));
    f.push(scale(d[2], d[3], flags(interp)));
    f.push("format=yuv420p".to_string());
    f.extend(pad(d, canvas)?);
    Ok(f.join(","))
}

/// Tempos das chaves da região animada (`t0` quando não há chaves).
fn tempos_das_chaves(efeitos: &[Tolerante<Efeito>], t0: f64) -> Vec<f64> {
    match enquadramento(efeitos).map(|q| &q.regiao) {
        Some(Animavel::Chaves(k)) if !k.is_empty() => k.iter().map(|c| c.t).collect(),
        _ => vec![t0],
    }
}

/// Todos os enquadres caem no destino da referência (até `DESTINO_FIXO_PX`)?
fn mesmo_destino(quadros: &[Enquadre], referencia: &Enquadre) -> bool {
    quadros.iter().all(|q| q.destino.iter().zip(referencia.destino).all(|(a, b)| (a - b).abs() <= DESTINO_FIXO_PX))
}

/// Enquadramento animado cujo destino muda entre os instantes dados (s, base do clipe): `conter` com a proporção
/// da região variando, ou região que sai da imagem. É a regra que `filtros` recusa com `nao_suportado`; a validação
/// a aplica a todos os slots do clipe, no canvas da saída.
pub(crate) fn destino_variavel(efeitos: &[Tolerante<Efeito>], origem: TamanhoPx, canvas: TamanhoPx, tempos: &[f64]) -> bool {
    if !animado(efeitos) || tempos.is_empty() {
        return false;
    }
    let referencia = enquadre(efeitos, tempos_das_chaves(efeitos, tempos[0])[0], origem, canvas);
    !tempos.iter().all(|&t| mesmo_destino(&[enquadre(efeitos, t, origem, canvas)], &referencia))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::edicao::modelo::Racional;
    use serde_json::json;

    fn efeitos(v: serde_json::Value) -> Vec<Tolerante<Efeito>> {
        serde_json::from_value(v).unwrap()
    }
    fn canvas(w: u32, h: u32) -> Canvas {
        Canvas { w, h, fps: Racional { num: 30, den: 1 }, cor_fundo: "#102030".into() }
    }
    fn enq(regiao: serde_json::Value, ajuste: &str) -> serde_json::Value {
        json!({ "tipo": "enquadramento", "id": "e1", "ligado": true, "nome": "", "ativo": null, "regiao": regiao, "ajuste": ajuste, "interpolacao": "bicubica" })
    }
    fn girar(graus: u32, h: bool, v: bool) -> serde_json::Value {
        json!({ "tipo": "girar", "id": "g1", "ligado": true, "nome": "", "ativo": null, "graus": graus, "espelhar_h": h, "espelhar_v": v })
    }
    const T: TamanhoPx = TamanhoPx { w: 320, h: 180 };

    #[test]
    fn sem_efeito_so_escala_e_barras() {
        // mesmo tamanho: nada
        assert_eq!(filtros(&[], T, &canvas(320, 180), &[0.0], 0, 0).unwrap(), "");
        assert_eq!(filtros(&[], T, &canvas(640, 360), &[0.0], 0, 0).unwrap(), "scale=640:360:flags=bicubic+accurate_rnd+bitexact");
        // 4:3 num canvas 16:9: barras dos lados, na cor do fundo
        let f = filtros(&[], TamanhoPx { w: 640, h: 480 }, &canvas(1280, 720), &[0.0], 0, 0).unwrap();
        assert_eq!(f, "scale=960:720:flags=bicubic+accurate_rnd+bitexact,pad=1280:720:160:0:color=0x102030");
        // vertical: barras maiores
        let f = filtros(&[], TamanhoPx { w: 180, h: 320 }, &canvas(320, 180), &[0.0], 0, 0).unwrap();
        assert_eq!(f, "scale=102:180:flags=bicubic+accurate_rnd+bitexact,pad=320:180:110:0:color=0x102030");
    }

    #[test]
    fn giro_emite_transpose_hflip_vflip_e_troca_os_lados() {
        let e = efeitos(json!([girar(90, true, true)]));
        let f = filtros(&e, T, &canvas(180, 320), &[0.0], 0, 0).unwrap();
        assert_eq!(f, "transpose=dir=clock,hflip,vflip");
        let e = efeitos(json!([girar(180, false, false), girar(270, false, false)]));
        let f = filtros(&e, T, &canvas(180, 320), &[0.0], 0, 0).unwrap();
        assert_eq!(f, "hflip,vflip,transpose=dir=cclock");
        assert!(!animado(&e));
    }

    #[test]
    fn regiao_fixa_recorta_escala_e_poe_barras_so_no_conter() {
        // região 2:1 no meio, canvas 16:9
        let r = json!({ "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 });
        let f = filtros(&efeitos(json!([enq(r.clone(), "conter")])), T, &canvas(320, 180), &[0.0], 0, 0).unwrap();
        assert_eq!(f, "crop=160:92:80:44:exact=1,scale=320:184:flags=bicubic+accurate_rnd+bitexact,crop=320:180:0:2:exact=1");
        // região mais larga que o canvas: conter põe barras em cima e embaixo; cobrir corta os lados
        let larga = json!({ "x": 0.0, "y": 0.25, "w": 1.0, "h": 0.25 });
        let f = filtros(&efeitos(json!([enq(larga.clone(), "conter")])), T, &canvas(320, 180), &[0.0], 0, 0).unwrap();
        assert_eq!(f, "crop=320:46:0:44:exact=1,pad=320:180:0:68:color=0x102030");
        let f = filtros(&efeitos(json!([enq(larga, "cobrir")])), T, &canvas(320, 180), &[0.0], 0, 0).unwrap();
        assert!(f.starts_with("crop=") && f.contains("scale=") && !f.contains("pad="), "{f}");
        let e = enquadre(&efeitos(json!([enq(json!({ "x": 0.0, "y": 0.25, "w": 1.0, "h": 0.25 }), "cobrir")])), 0.0, T, TamanhoPx { w: 320, h: 180 });
        assert_eq!(e.destino, [0.0, 0.0, 320.0, 180.0]);
        assert!((e.origem[2] - 80.0).abs() < 1e-9 && (e.origem[3] - 45.0).abs() < 1e-9, "{e:?}");
    }

    #[test]
    fn animado_usa_perspective_com_arvore_sobre_on() {
        let chaves = json!([
            { "t": 0.0, "v": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 } },
            { "t": 1.0, "v": { "x": 0.5, "y": 0.5, "w": 0.25, "h": 0.25 } }
        ]);
        let e = efeitos(json!([enq(chaves, "conter")]));
        assert!(animado(&e));
        let tempos: Vec<f64> = (0..45).map(|q| q as f64 / 30.0).collect();
        let f = filtros(&e, T, &canvas(640, 360), &tempos, 0, 0).unwrap();
        assert!(f.starts_with("scale=w=iw:h=ih:flags=bicubic+accurate_rnd+bitexact,format=yuv444p,perspective=x0="), "{f}");
        assert!(f.contains("(on-1)*1/30") && f.contains("sense=source:eval=frame:interpolation=cubic"));
        assert!(f.ends_with(",scale=640:360:flags=bicubic+accurate_rnd+bitexact,format=yuv420p"));
        // proporção que muda no conter: o destino anda e a Fase 0 recusa com o id
        let muda = json!([
            { "t": 0.0, "v": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 } },
            { "t": 1.0, "v": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 0.5 } }
        ]);
        let erro = filtros(&efeitos(json!([enq(muda.clone(), "conter")])), T, &canvas(640, 360), &tempos, 0, 0).unwrap_err().to_string();
        assert!(erro.contains("nao_suportado") && erro.contains("e1"), "{erro}");
        // no cobrir o destino é sempre o canvas
        assert!(filtros(&efeitos(json!([enq(muda, "cobrir")])), T, &canvas(640, 360), &tempos, 0, 0).is_ok());
    }
}
