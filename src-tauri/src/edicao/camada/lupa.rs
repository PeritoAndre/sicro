//! Lupa no rasterizador (§5.4, §5.5): a primitiva `lupa` (origem na fonte, destino no canvas, forma) mais a moldura
//! e a ligação, desenhadas na camada de anotação. Os pixels ampliados são do ffmpeg (`motor/efeitos/lupa.rs`), que
//! lê a geometria daqui: uma regra só para a prévia e o render. Espelho no front: `palco/desenho/lupa.ts`, pela
//! fixture `primitivas-lupa.json`. Dono: P1.2.
//!
//! Contas como em `formas.rs` (o espelho repete na mesma ordem): só `+ − × ÷`, `floor`, `abs` e comparações
//! explícitas (nada de `min`/`max`, que diferem no −0). A conta corre em px do canvas e volta a 0..1 com `+ 0`.
//! Ampliação = em relação ao tamanho da janela no canvas, por `A(τ)` do quadro (o que se vê no vídeo).

use tiny_skia::{FillRule, Mask, PathBuilder, Rect, Transform};

use super::formas::{Primitiva, QuadroCamada, Segmento, Traco2, COR_CONTORNO};
use super::texto::FonteSprites;
use super::Bitmap;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{Caixa, EfLupa, FormaLupa, OndeLupa, Ponto, TamanhoPx, Traco};
use crate::error::{Result, SicroError};

// CONTRATO: acréscimo (o editor e a validação leem a mesma faixa).
/// Ampliação aceita (§2.1).
pub const FATOR_MIN: f64 = 1.5;
pub const FATOR_MAX: f64 = 8.0;

fn z(v: f64) -> f64 {
    v + 0.0
}

fn invalido(campo: &str) -> SicroError {
    SicroError::Validation(format!("{}: {campo}", codigo::ITEM_INVALIDO))
}

/// Par mais perto (meio sobe).
fn par(v: f64) -> f64 {
    2.0 * (v * 0.5 + 0.5).floor()
}

/// Maior par ≤ v.
fn par_abaixo(v: f64) -> f64 {
    2.0 * (v * 0.5).floor()
}

/// `v` em [lo, hi]; hi < lo vale lo.
fn limitar(v: f64, lo: f64, hi: f64) -> f64 {
    let v = if v > hi { hi } else { v };
    if v < lo {
        lo
    } else {
        v
    }
}

fn traco2(t: &Traco) -> Traco2 {
    Traco2 { cor: t.cor.clone(), largura: t.espessura, contorno: t.contorno.then(|| COR_CONTORNO.to_string()) }
}

// CONTRATO: acréscimo (a validação pergunta a mesma regra: `motor::efeitos::lupa::campo_invalido`).
/// Campo que a conta não aceita (`janela` fora de (0, 1], `fator` fora de 1,5..8, `onde` não finito); None = ok.
pub fn campo_invalido(e: &EfLupa) -> Option<&'static str> {
    if !(e.janela.is_finite() && e.janela > 0.0 && e.janela <= 1.0) {
        return Some("janela");
    }
    if !(e.fator.is_finite() && e.fator >= FATOR_MIN && e.fator <= FATOR_MAX) {
        return Some("fator");
    }
    let finitos = match e.onde {
        OndeLupa::Fixa { x, y } => x.is_finite() && y.is_finite(),
        OndeLupa::Segue { dx, dy } => dx.is_finite() && dy.is_finite(),
    };
    (!finitos).then_some("onde")
}

/// Forma (elipse inscrita ou retângulo) na caixa [x0, y0, x1, y1] (px do canvas).
fn forma(f: FormaLupa, b: (f64, f64, f64, f64), traco: Option<Traco2>, wc: f64, hc: f64) -> Primitiva {
    let (x0, y0, x1, y1) = b;
    match f {
        FormaLupa::Circulo => {
            let (rx, ry) = ((x1 - x0) * 0.5, (y1 - y0) * 0.5);
            Primitiva::Elipse { cx: z((x0 + rx) / wc), cy: z((y0 + ry) / hc), rx: z(rx / wc), ry: z(ry / hc), traco, preenche: None }
        }
        FormaLupa::Retangulo => Primitiva::Caminho {
            d: vec![
                Segmento::M(z(x0 / wc), z(y0 / hc)),
                Segmento::L(z(x1 / wc), z(y0 / hc)),
                Segmento::L(z(x1 / wc), z(y1 / hc)),
                Segmento::L(z(x0 / wc), z(y1 / hc)),
                Segmento::Z,
            ],
            traco,
            preenche: None,
            regra: None,
        },
    }
}

/// Reta da ligação entre a janela `o` e a lupa `d` (px): do meio do lado de uma ao meio do lado da outra que se
/// encaram (o meio do lado está no contorno do círculo e do retângulo). None = uma toca a outra.
fn linha(o: (f64, f64, f64, f64), d: (f64, f64, f64, f64)) -> Option<(f64, f64, f64, f64)> {
    let (a, b) = (d.0 - o.2, o.0 - d.2);
    let gx = if a > b { a } else { b };
    let (a, b) = (d.1 - o.3, o.1 - d.3);
    let gy = if a > b { a } else { b };
    if gx <= 0.0 && gy <= 0.0 {
        return None;
    }
    let (omx, omy) = ((o.0 + o.2) * 0.5, (o.1 + o.3) * 0.5);
    let (dmx, dmy) = ((d.0 + d.2) * 0.5, (d.1 + d.3) * 0.5);
    Some(if gx >= gy {
        if d.0 >= o.2 {
            (o.2, omy, d.0, dmy)
        } else {
            (o.0, omy, d.2, dmy)
        }
    } else if d.1 >= o.3 {
        (omx, o.3, dmx, d.1)
    } else {
        (omx, o.1, dmx, d.3)
    })
}

/// Primitivas da lupa no quadro: `Primitiva::Lupa` + moldura (caminho ou elipse) + ligação (contorno da janela no
/// canvas e a reta até a lupa). O rastreio do alvo vem em `quadro.rastreios[e.rastreio]`; sem ele, vazio.
/// Janela: quadrado de lado `janela` × altura da origem, centrado no alvo e contido na origem (encosta na borda).
/// Lupa: lado par = `fator` × lado da janela no canvas, posição par, contida no canvas. Se não couber, a lupa fica
/// do tamanho do canvas e a janela encolhe: a ampliação mostrada é sempre a do `fator` (é a que vai ao relatório).
pub fn primitivas(e: &EfLupa, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    let _ = sprites;
    if let Some(c) = campo_invalido(e) {
        return Err(invalido(c));
    }
    let Some(r) = quadro.rastreios.get(&e.rastreio) else { return Ok(Vec::new()) };
    let (cx, cy) = (r.x + r.w * 0.5, r.y + r.h * 0.5);
    let [a, b, c, d, _, _] = quadro.a.0;
    let (wc, hc) = (quadro.canvas.w as f64, quadro.canvas.h as f64);
    // largura e altura inteiras da origem levadas ao canvas (px): A é giro de 90° com escala igual nos dois eixos
    let sw = a.abs() * wc + b.abs() * hc;
    let sh = c.abs() * wc + d.abs() * hc;
    if !(sw > 0.0 && sh > 0.0 && sw.is_finite() && sh.is_finite() && cx.is_finite() && cy.is_finite()) {
        return Ok(Vec::new());
    }
    // janela (fração da origem); larga demais para a origem, vira a altura que cabe
    let (j, ow) = if e.janela * sh >= sw { (sw / sh, 1.0) } else { (e.janela, e.janela * sh / sw) };
    // lupa (px do canvas) que não cabe no canvas: fica do tamanho dele e a JANELA encolhe; a ampliação é a pedida
    let menor = if wc < hc { wc } else { hc };
    let teto = par_abaixo(menor);
    let cheio = par(e.fator * (j * sh));
    let (j, ow) = if cheio > teto {
        let jj = teto / (e.fator * sh);
        (jj, jj * sh / sw)
    } else {
        (j, ow)
    };
    let oh = j;
    let ox = limitar(cx - ow * 0.5, 0.0, 1.0 - ow);
    let oy = limitar(cy - oh * 0.5, 0.0, 1.0 - oh);
    let origem = Caixa { x: z(ox), y: z(oy), w: z(ow), h: z(oh) };
    let lado = limitar(cheio, 2.0, teto);
    let (px, py) = match e.onde {
        OndeLupa::Fixa { x, y } => (x * wc, y * hc),
        OndeLupa::Segue { dx, dy } => {
            let p = quadro.a.ponto(Ponto { x: cx, y: cy });
            ((p.x + dx) * wc, (p.y + dy) * hc)
        }
    };
    let x0 = limitar(par(px - lado * 0.5), 0.0, par_abaixo(wc - lado));
    let y0 = limitar(par(py - lado * 0.5), 0.0, par_abaixo(hc - lado));
    let destino = Caixa { x: z(x0 / wc), y: z(y0 / hc), w: z(lado / wc), h: z(lado / hc) };
    let dc = (x0, y0, x0 + lado, y0 + lado);
    let traco = Some(traco2(&e.moldura));
    let mut v = vec![Primitiva::Lupa { origem, destino, forma: e.forma }, forma(e.forma, dc, traco.clone(), wc, hc)];
    if e.ligacao {
        let ob = quadro.a.caixa(origem);
        let oc = (ob.x * wc, ob.y * hc, (ob.x + ob.w) * wc, (ob.y + ob.h) * hc);
        v.push(forma(e.forma, oc, traco.clone(), wc, hc));
        if let Some((x1, y1, x2, y2)) = linha(oc, dc) {
            v.push(Primitiva::Caminho {
                d: vec![Segmento::M(z(x1 / wc), z(y1 / hc)), Segmento::L(z(x2 / wc), z(y2 / hc))],
                traco,
                preenche: None,
                regra: None,
            });
        }
    }
    Ok(v)
}

/// Máscara da forma (cinza, branco = mostra a ampliação) das `Primitiva::Lupa` de um quadro, em `caixa_px` do
/// `canvas`: é a camada `MascaraCanvas` que `motor/efeitos/lupa.rs` pede ao montador para o `alphamerge` (o
/// rasterizador chama aqui quando o quadro traz lupa). O destino cai em px inteiros, onde o motor põe a ampliação;
/// a borda do círculo é suavizada (a moldura passa por cima).
// CONTRATO: acréscimo da revisão da abertura (sem ele a máscara de uma lupa saía vazia: o rasterizador só via `Regiao`).
pub fn mascara(lupas: &[Primitiva], canvas: TamanhoPx, caixa_px: [u32; 4]) -> Result<Bitmap> {
    let [x0, y0, w, h] = caixa_px;
    let mut m = Mask::new(w, h).ok_or_else(|| invalido("caixa_px"))?;
    let (wc, hc) = (canvas.w as f64, canvas.h as f64);
    for p in lupas {
        let Primitiva::Lupa { destino: d, forma, .. } = p else { continue };
        let (x, y, dw, dh) = (d.x * wc - x0 as f64, d.y * hc - y0 as f64, d.w * wc, d.h * hc);
        if ![x, y, dw, dh].iter().all(|v| v.is_finite()) {
            return Err(invalido("destino"));
        }
        let Some(r) = Rect::from_xywh(x.round() as f32, y.round() as f32, dw.round() as f32, dh.round() as f32) else { continue };
        let caminho = match forma {
            FormaLupa::Circulo => PathBuilder::from_oval(r),
            FormaLupa::Retangulo => Some(PathBuilder::from_rect(r)),
        };
        if let Some(c) = caminho {
            m.fill_path(&c, FillRule::Winding, true, Transform::identity());
        }
    }
    Ok(Bitmap { w, h, canais: 1, px: m.data().to_vec() })
}

#[cfg(test)]
mod testes {
    use serde_json::{json, Value};

    use super::*;
    use crate::edicao::camada::formas::testes::{base, com, fixture, quadro, CAIXA, CANVAS};
    use crate::edicao::geometria::{a_de, Afim};
    use crate::edicao::modelo::{Efeito, Tolerante};

    fn lupa(extra: Value) -> Value {
        com(
            com(base("lupa", "l1"), json!({ "rastreio": "r1", "janela": 0.2, "fator": 2.0, "forma": "circulo", "interpolacao": "bilinear",
                "onde": { "modo": "fixa", "x": 0.8, "y": 0.2 }, "moldura": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "ligacao": true })),
            extra,
        )
    }

    fn a_da_origem(w: u32, h: u32, canvas: TamanhoPx, girar: Option<i32>) -> Afim {
        let efeitos: Vec<Tolerante<Efeito>> = girar
            .map(|g| {
                serde_json::from_value(json!({ "tipo": "girar", "id": "g", "ligado": true, "nome": "", "ativo": null,
                    "graus": g, "espelhar_h": false, "espelhar_v": false }))
                .unwrap()
            })
            .map(Tolerante::Conhecido)
            .into_iter()
            .collect();
        a_de(&efeitos, 0.0, TamanhoPx { w, h }, canvas)
    }

    fn casos() -> Vec<(String, Value, QuadroCamada)> {
        let id = Afim::IDENTIDADE;
        let q = quadro(CANVAS, id, &[("r1", CAIXA)]);
        let a43 = a_da_origem(1440, 1080, CANVAS, None);
        let a4k = a_da_origem(3840, 2160, CANVAS, None);
        let girado = a_da_origem(1280, 720, CANVAS, Some(90));
        let vertical = a_da_origem(1080, 1920, CANVAS, None);
        let segue = |dx: f64, dy: f64| json!({ "onde": { "modo": "segue", "dx": dx, "dy": dy } });
        vec![
            ("lupa".into(), lupa(json!({})), q.clone()),
            ("lupa retangulo sem contorno".into(), lupa(json!({ "forma": "retangulo", "moldura": { "cor": "#ffcc00", "espessura": 0.006, "contorno": false } })), q.clone()),
            ("lupa sem ligacao".into(), lupa(json!({ "ligacao": false })), q.clone()),
            ("lupa segue a direita".into(), lupa(segue(0.25, -0.05)), q.clone()),
            ("lupa segue embaixo retangulo".into(), lupa(com(segue(0.0, 0.4), json!({ "forma": "retangulo" }))), q.clone()),
            ("lupa segue acima".into(), lupa(segue(0.02, -0.29)), quadro(CANVAS, id, &[("r1", [0.45, 0.7, 0.04, 0.06])])),
            ("lupa segue a esquerda".into(), lupa(segue(-0.3, 0.0)), quadro(CANVAS, id, &[("r1", [0.7, 0.45, 0.04, 0.06])])),
            ("lupa sobre a janela sem reta".into(), lupa(json!({ "onde": { "modo": "fixa", "x": 0.45, "y": 0.4 } })), q.clone()),
            ("lupa 4:3 em 16:9".into(), lupa(json!({})), quadro(CANVAS, a43, &[("r1", CAIXA)])),
            ("lupa 4K em 1080p".into(), lupa(json!({ "fator": 3.0, "janela": 0.1 })), quadro(CANVAS, a4k, &[("r1", CAIXA)])),
            ("lupa girada 90".into(), lupa(segue(0.2, 0.0)), quadro(CANVAS, girado, &[("r1", CAIXA)])),
            ("lupa origem vertical janela inteira".into(), lupa(json!({ "janela": 1.0, "fator": 1.5 })), quadro(CANVAS, vertical, &[("r1", CAIXA)])),
            ("lupa ponto".into(), lupa(json!({})), quadro(CANVAS, id, &[("r1", [0.3, 0.6, 0.0, 0.0])])),
            ("lupa alvo na borda".into(), lupa(segue(-0.1, 0.1)), quadro(CANVAS, id, &[("r1", [0.98, -0.01, 0.04, 0.04])])),
            ("lupa maior que o canvas".into(), lupa(json!({ "janela": 0.9, "fator": 8.0 })), q.clone()),
            ("lupa que nao cabe girada 90".into(), lupa(com(segue(0.1, 0.0), json!({ "janela": 0.6, "fator": 6.0, "forma": "retangulo" }))), quadro(CANVAS, girado, &[("r1", CAIXA)])),
            ("lupa que nao cabe 4:3 na borda".into(), lupa(json!({ "janela": 0.7, "fator": 3.0 })), quadro(CANVAS, a43, &[("r1", [0.9, 0.05, 0.04, 0.04])])),
            ("lupa canvas vertical".into(), lupa(json!({ "onde": { "modo": "fixa", "x": 0.5, "y": 0.85 } })), quadro(TamanhoPx { w: 1080, h: 1920 }, id, &[("r1", CAIXA)])),
            ("lupa sem rastreio no quadro".into(), lupa(json!({})), quadro(CANVAS, id, &[("outro", CAIXA)])),
            ("lupa janela zero".into(), lupa(json!({ "janela": 0.0 })), q.clone()),
            ("lupa janela acima de 1".into(), lupa(json!({ "janela": 1.5 })), q.clone()),
            ("lupa fator abaixo".into(), lupa(json!({ "fator": 1.0 })), q.clone()),
            ("lupa fator acima".into(), lupa(json!({ "fator": 9.0 })), q.clone()),
        ]
    }

    /// Fixture do espelho (`palco/desenho/lupa.ts`): `SICRO_GRAVAR_FIXTURES=1` regrava.
    #[test]
    fn fixture_primitivas_lupa() {
        fixture("primitivas-lupa.json", casos());
    }

    struct Nada;
    impl FonteSprites for Nada {
        fn sprite(&self, _: &crate::edicao::camada::texto::PedidoSprite) -> Result<crate::edicao::camada::texto::Sprite> {
            Err(invalido("sprite"))
        }
        fn atlas(&self, _: &crate::edicao::modelo::EstiloTexto, _: u32) -> Result<crate::edicao::camada::texto::Atlas> {
            Err(invalido("atlas"))
        }
        fn imagem(&self, _: &str) -> Option<TamanhoPx> {
            None
        }
    }

    fn rodar(nome: &str) -> Result<Vec<Primitiva>> {
        let (_, e, q) = casos().into_iter().find(|c| c.0 == nome).unwrap();
        let Efeito::Lupa(l) = serde_json::from_value(e).unwrap() else { panic!("lupa") };
        primitivas(&l, &q, &Nada)
    }

    fn geometria(p: &[Primitiva]) -> (Caixa, Caixa) {
        match p[0] {
            Primitiva::Lupa { origem, destino, .. } => (origem, destino),
            _ => panic!("primeira não é a lupa: {p:?}"),
        }
    }

    #[test]
    fn janela_centrada_no_alvo_lupa_par_e_do_tamanho_do_fator() {
        // alvo (0,45; 0,4), janela 0,2 da altura (216 px em 1080) → lupa 432 px centrada em (0,8; 0,2)
        let (o, d) = geometria(&rodar("lupa").unwrap());
        assert!((o.x + o.w / 2.0 - 0.45).abs() < 1e-12 && (o.y + o.h / 2.0 - 0.4).abs() < 1e-12, "{o:?}");
        assert!((o.h - 0.2).abs() < 1e-12 && (o.w * 1920.0 - 216.0).abs() < 1e-9);
        let px = [d.x * 1920.0, d.y * 1080.0, d.w * 1920.0, d.h * 1080.0];
        assert_eq!(px.map(|v| v.round()), [1320.0, 0.0, 432.0, 432.0]);
        assert!(px.iter().all(|v| (v - v.round()).abs() < 1e-9 && v.round() as i64 % 2 == 0), "{px:?}");
        // 4K em 1080p: a janela de 0,1 tem 216 px da fonte e 108 do canvas; ×3 → 324 px
        let (_, d) = geometria(&rodar("lupa 4K em 1080p").unwrap());
        assert_eq!((d.w * 1920.0).round(), 324.0);
    }

    /// Lupa que não cabe no canvas: fica do tamanho dele e a janela encolhe em volta do alvo; a ampliação continua
    /// a do `fator` (antes a janela ficava e a ampliação caía calada: 8× pedidos davam 1,1×).
    #[test]
    fn lupa_que_nao_cabe_encolhe_a_janela_e_mantem_a_ampliacao() {
        // janela 0,9 × 8 em 1080 de altura: a lupa teria 7776 px
        let (o, d) = geometria(&rodar("lupa maior que o canvas").unwrap());
        assert_eq!(((d.w * 1920.0).round(), (d.h * 1080.0).round()), (1080.0, 1080.0));
        assert!((d.h * 1080.0 / (o.h * 1080.0) - 8.0).abs() < 1e-9 && (d.w * 1920.0 / (o.w * 1920.0) - 8.0).abs() < 1e-9, "{o:?} {d:?}");
        assert!((o.x + o.w / 2.0 - 0.45).abs() < 1e-12 && (o.y + o.h / 2.0 - 0.4).abs() < 1e-12, "{o:?}");
        // a que cabe não muda: 4K em 1080p, janela 0,1 × 3
        let (o, d) = geometria(&rodar("lupa 4K em 1080p").unwrap());
        assert!((o.h - 0.1).abs() < 1e-12 && (d.h * 1080.0 / (o.h * 1080.0) - 3.0).abs() < 1e-9, "{o:?} {d:?}");
        // em todo caso da fixture a ampliação é a pedida (a menos do arredondamento do lado ao par)
        for (nome, e, q) in casos() {
            let Ok(Efeito::Lupa(l)) = serde_json::from_value(e) else { panic!("{nome}") };
            let Ok(p) = primitivas(&l, &q, &Nada) else { continue };
            if p.is_empty() {
                continue;
            }
            let (o, d) = geometria(&p);
            let [_, _, c, dd, _, _] = q.a.0;
            let janela_px = o.h * (c.abs() * q.canvas.w as f64 + dd.abs() * q.canvas.h as f64);
            let lado = d.h * q.canvas.h as f64;
            assert!((lado - l.fator * janela_px).abs() <= 1.0 + 1e-9, "{nome}: lupa {lado} px, janela {janela_px} px, fator {}", l.fator);
        }
    }

    #[test]
    fn segue_o_alvo_no_canvas_e_fica_dentro_dele() {
        // centro do alvo (0,45; 0,4) + (0,25; −0,05) = (0,7; 0,35) do canvas
        let (_, d) = geometria(&rodar("lupa segue a direita").unwrap());
        let (cx, cy) = ((d.x + d.w / 2.0) * 1920.0, (d.y + d.h / 2.0) * 1080.0);
        assert!((cx - 0.7 * 1920.0).abs() <= 1.0 && (cy - 0.35 * 1080.0).abs() <= 1.0, "{cx} {cy}");
        for nome in ["lupa alvo na borda", "lupa maior que o canvas", "lupa canvas vertical"] {
            let (o, d) = geometria(&rodar(nome).unwrap());
            assert!(o.x >= 0.0 && o.y >= 0.0 && o.x + o.w <= 1.0 + 1e-12 && o.y + o.h <= 1.0 + 1e-12, "{nome}: {o:?}");
            assert!(d.x >= 0.0 && d.y >= 0.0 && d.x + d.w <= 1.0 + 1e-12 && d.y + d.h <= 1.0 + 1e-12, "{nome}: {d:?}");
        }
        // girada 90: o alvo vai ao canvas por A (x da origem vira y do canvas)
        let (_, d) = geometria(&rodar("lupa girada 90").unwrap());
        let q = casos().into_iter().find(|c| c.0 == "lupa girada 90").unwrap().2;
        let p = q.a.ponto(Ponto { x: 0.45, y: 0.4 });
        assert!(((d.x + d.w / 2.0) - (p.x + 0.2)).abs() * 1920.0 <= 1.0 && ((d.y + d.h / 2.0) - p.y).abs() * 1080.0 <= 1.0);
    }

    #[test]
    fn ligacao_moldura_e_erros() {
        assert_eq!(rodar("lupa").unwrap().len(), 4);
        assert_eq!(rodar("lupa sem ligacao").unwrap().len(), 2);
        // lupa sobre a janela: contorno da janela sem a reta
        assert_eq!(rodar("lupa sobre a janela sem reta").unwrap().len(), 3);
        assert!(rodar("lupa sem rastreio no quadro").unwrap().is_empty());
        for (nome, campo) in [("lupa janela zero", "janela"), ("lupa janela acima de 1", "janela"), ("lupa fator abaixo", "fator"), ("lupa fator acima", "fator")] {
            let e = rodar(nome).unwrap_err().to_string();
            assert!(e.contains("item_invalido") && e.contains(campo), "{nome}: {e}");
        }
        // reta: do lado da janela ao lado da lupa que se encaram
        let p = rodar("lupa segue embaixo retangulo").unwrap();
        let Primitiva::Caminho { d, .. } = &p[3] else { panic!("{p:?}") };
        let (Segmento::M(_, y1), Segmento::L(_, y2)) = (d[0], d[1]) else { panic!("{d:?}") };
        let (o, de) = geometria(&p);
        assert!((y1 - (o.y + o.h)).abs() < 1e-9 && (y2 - de.y).abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn mascara_desenha_a_forma_no_destino() {
        let lupa = |forma| Primitiva::Lupa { origem: Caixa { x: 0.0, y: 0.0, w: 0.1, h: 0.1 }, destino: Caixa { x: 100.0 / 1920.0, y: 50.0 / 1080.0, w: 64.0 / 1920.0, h: 64.0 / 1080.0 }, forma };
        // caixa [96, 40, 80, 80]: o destino começa em (4, 10) dela
        let r = mascara(&[lupa(FormaLupa::Retangulo)], CANVAS, [96, 40, 80, 80]).unwrap();
        assert_eq!((r.w, r.h, r.canais), (80, 80, 1));
        for y in 0..80usize {
            for x in 0..80usize {
                let dentro = (4..68).contains(&x) && (10..74).contains(&y);
                assert_eq!(r.px[y * 80 + x], if dentro { 255 } else { 0 }, "({x}, {y})");
            }
        }
        let c = mascara(&[lupa(FormaLupa::Circulo)], CANVAS, [96, 40, 80, 80]).unwrap();
        let v = |x: usize, y: usize| c.px[y * 80 + x];
        assert_eq!((v(36, 42), v(5, 11), v(66, 72), v(0, 0)), (255, 0, 0, 0));
        // área do círculo (π·32² ≈ 3217) e centro no centro do destino (36, 42)
        let (mut area, mut sx, mut sy) = (0.0, 0.0, 0.0);
        for (i, &p) in c.px.iter().enumerate() {
            let a = p as f64 / 255.0;
            (area, sx, sy) = (area + a, sx + a * ((i % 80) as f64 + 0.5), sy + a * ((i / 80) as f64 + 0.5));
        }
        assert!((area / (std::f64::consts::PI * 32.0 * 32.0) - 1.0).abs() < 0.01, "{area}");
        assert!((sx / area - 36.0).abs() < 0.05 && (sy / area - 42.0).abs() < 0.05, "{} {}", sx / area, sy / area);
        // outras primitivas não entram; destino não finito é erro
        let elipse = Primitiva::Elipse { cx: 0.1, cy: 0.1, rx: 0.1, ry: 0.1, traco: None, preenche: Some("#ffffff".into()) };
        assert!(mascara(&[elipse], CANVAS, [0, 0, 8, 8]).unwrap().px.iter().all(|&p| p == 0));
        let mut ruim = lupa(FormaLupa::Circulo);
        if let Primitiva::Lupa { destino, .. } = &mut ruim {
            destino.x = f64::NAN;
        }
        assert!(mascara(&[ruim], CANVAS, [0, 0, 8, 8]).is_err());
    }

    /// A máscara pelo rasterizador de verdade (o gancho do `camada/mod.rs`): o quadro que traz a lupa sai com a
    /// forma no destino; o que não traz, preto.
    #[test]
    fn rasterizador_chama_a_mascara_da_lupa() {
        use crate::edicao::motor::plano::{Desenho, ItemCamada, PedidoCamada, QuadroItem, TipoCamada};
        let Efeito::Lupa(l) = serde_json::from_value(lupa(json!({ "forma": "retangulo" }))).unwrap() else { panic!() };
        let q = quadro(CANVAS, Afim::IDENTIDADE, &[("r1", CAIXA)]);
        let mut sem = q.clone();
        sem.rastreios.clear();
        let (_, d) = geometria(&primitivas(&l, &q, &Nada).unwrap());
        let caixa = [d.x * 1920.0, d.y * 1080.0, d.w * 1920.0, d.h * 1080.0].map(|v| v.round() as u32);
        let item = ItemCamada { desenho: Desenho::Efeito { efeito: Efeito::Lupa(l) }, de: 0, quadros: vec![QuadroItem { quadro: q, transformacao: None }, QuadroItem { quadro: sem, transformacao: None }] };
        let mut pedido = PedidoCamada { id: "m".into(), tipo: TipoCamada::MascaraCanvas, caixa_px: caixa, n: 2, hash: String::new(), itens: vec![item], imagens: Vec::new() };
        pedido.hash = crate::edicao::camada::hash_do_pedido(&pedido);
        let pasta = crate::edicao::testes::fontes::pasta_temporaria("lupa-mascara");
        let pronta = crate::edicao::camada::rasterizar(&pedido, &pasta, &std::sync::atomic::AtomicBool::new(false)).unwrap();
        assert_eq!(pronta.arquivos.len(), 2);
        let ler = |a: &str| image::open(pronta.pasta.join(a)).unwrap().into_luma8();
        let (cheia, vazia) = (ler(&pronta.arquivos[0].arquivo), ler(&pronta.arquivos[1].arquivo));
        assert_eq!((cheia.width(), cheia.height()), (caixa[2], caixa[3]));
        assert!(cheia.pixels().all(|p| p.0[0] == 255) && vazia.pixels().all(|p| p.0[0] == 0));
    }
}
