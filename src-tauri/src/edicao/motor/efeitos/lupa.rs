//! Lupa (§5.3 `split=<1 + lupas>`, §5.4): cópia da base ANTES do enquadramento (pixels da origem do clipe) →
//! `crop` com x,y em árvore de `t` (pares) → `scale` (interpolação do efeito) → `alphamerge` com a forma → `overlay`
//! no canvas. Moldura e ligação vão na camada de anotação (`camada/lupa.rs`). Dono: P1.2.
//!
//! A geometria de cada slot é a da primitiva `lupa` (`camada::lupa::primitivas`, a mesma da prévia). O recorte
//! leva uma margem em volta da janela e a ampliação é posta no canvas pelo `overlay` em px pares: o alvo cai no
//! centro da lupa a até 1 px, qualquer que seja o fator (recortar em px pares da fonte erraria `fator` px).
//! Tudo acontece dentro da caixa `U` (união das lupas da janela): recorte do canvas → ampliação por cima → forma
//! (máscara por slot, px do canvas) → de volta. Lado da lupa que muda com o zoom animado: `scale` por quadro.
//! A cópia vem antes do `girar` (que faz parte de `A`): o recorte é girado e espelhado como o vídeo antes da escala.

use std::collections::BTreeMap;

use crate::edicao::camada::formas::{Primitiva, QuadroCamada};
use crate::edicao::camada::texto::{Atlas, FonteSprites, PedidoSprite, Sprite};
use crate::edicao::camada::{self};
use crate::edicao::geometria::Afim;
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{Caixa, EfLupa, Efeito, EstiloTexto, Interpolacao, OndeLupa, TamanhoPx};
use crate::edicao::motor::clipe::Montador;
use crate::edicao::motor::efeitos::{anotacao, base_ativa};
use crate::edicao::motor::expr;
use crate::edicao::motor::grafo::{filtro, scale};
use crate::edicao::motor::plano::{Desenho, ItemCamada, QuadroItem, TipoCamada};
use crate::error::{Result, SicroError};

// CONTRATO: acréscimo.
/// Menor lado da janela na fonte (px): abaixo disso a ampliação não mostra nada e o recorte com margem ficaria
/// muitas vezes maior que a lupa.
pub const JANELA_MIN_PX: f64 = 8.0;
/// Margem do recorte em volta da janela (px da fonte, par): cobre o arredondamento da posição da ampliação.
const MARGEM: i64 = 4;
/// Teto do lado da ampliação antes de cortar na forma (px).
const LADO_MAX: i64 = 16_384;

/// Detalhe do `nao_suportado` da lupa. Uma regra só: `validar` e `clipe::cadeia` perguntam aqui (contratos (h) 1).
/// None = renderiza.
pub fn recusa(e: &EfLupa) -> Option<&'static str> {
    let _ = e;
    None
}

fn invalido(campo: &str) -> SicroError {
    SicroError::Validation(format!("{}: {campo}", codigo::ITEM_INVALIDO))
}

/// A lupa não usa texto nem imagem.
struct SemSprites;

impl FonteSprites for SemSprites {
    fn sprite(&self, _: &PedidoSprite) -> Result<Sprite> {
        Err(invalido("sprite"))
    }
    fn atlas(&self, _: &EstiloTexto, _: u32) -> Result<Atlas> {
        Err(invalido("atlas"))
    }
    fn imagem(&self, _: &str) -> Option<TamanhoPx> {
        None
    }
}

/// Par mais perto (meio sobe).
fn par(v: f64) -> i64 {
    2 * (v * 0.5 + 0.5).floor() as i64
}

/// Maior par ≤ v.
fn par_abaixo(v: f64) -> i64 {
    2 * (v * 0.5).floor() as i64
}

/// Valor de um parâmetro por slot no grafo: inteiro quando não muda; senão árvore sobre `t` (§5.11) com meio px
/// a mais, porque `crop` e `overlay` truncam (e levam ao par) o que a reta devolve a até 0,25 px do inteiro.
fn por_slot(v: &[i64], fps: crate::edicao::modelo::Racional) -> String {
    if v.windows(2).all(|w| w[0] == w[1]) {
        return v.first().copied().unwrap_or(0).to_string();
    }
    let f: Vec<f64> = v.iter().map(|&x| x as f64 + 0.5).collect();
    expr::tabela(&f, fps)
}

/// Lado da `scale` por slot: inteiro, ou árvore exata (tolerância 0) com 0,25 a mais, para dar o mesmo inteiro
/// truncando ou arredondando.
fn lado_por_slot(v: &[i64], fps: crate::edicao::modelo::Racional) -> Option<String> {
    if v.windows(2).all(|w| w[0] == w[1]) {
        return None;
    }
    let f: Vec<f64> = v.iter().map(|&x| x as f64 + 0.25).collect();
    Some(expr::tabela_em(&f, fps, 0.0, "t"))
}

fn flags(i: Interpolacao) -> &'static str {
    match i {
        Interpolacao::Vizinho => "neighbor",
        Interpolacao::Bilinear => "bilinear",
        Interpolacao::Bicubica => "bicubic",
    }
}

/// Giro e espelhos do `girar` do clipe, lidos de `A`: a cópia é tirada antes dele e o recorte tem de sair na
/// orientação do vídeo (sem isso a lupa mostrava a imagem deitada ou não espelhada ao lado do vídeo girado).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Giro {
    /// `transpose=dir=clock` antes dos espelhos.
    troca: bool,
    hflip: bool,
    vflip: bool,
}

impl Giro {
    fn de(a: &Afim) -> Self {
        let [a, b, c, d, _, _] = a.0;
        if a.abs() >= b.abs() {
            Giro { troca: false, hflip: a < 0.0, vflip: d < 0.0 }
        } else {
            // x' = c·y, y' = b·x; o giro horário dá b > 0 e c < 0
            Giro { troca: true, hflip: c > 0.0, vflip: b < 0.0 }
        }
    }

    fn filtros(self) -> Vec<String> {
        let mut f = Vec::new();
        if self.troca {
            f.push("transpose=dir=clock".to_string());
        }
        if self.hflip {
            f.push("hflip".to_string());
        }
        if self.vflip {
            f.push("vflip".to_string());
        }
        f
    }

    /// Lados do recorte e um ponto dele (px), depois do giro.
    fn levar(self, (w, h): (i64, i64), (x, y): (f64, f64)) -> ((i64, i64), (f64, f64)) {
        let ((w, h), (mut x, mut y)) = if self.troca { ((h, w), (h as f64 - y, x)) } else { ((w, h), (x, y)) };
        if self.hflip {
            x = w as f64 - x;
        }
        if self.vflip {
            y = h as f64 - y;
        }
        ((w, h), (x, y))
    }
}

/// Um slot com lupa, em px: recorte (x1, y1) na fonte, lados da ampliação já girada (sw, sh) e posição dela no
/// canvas (px, py); destino [x, y, lado] da lupa no canvas.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot {
    x1: i64,
    y1: i64,
    sw: i64,
    sh: i64,
    px: i64,
    py: i64,
    destino: [i64; 3],
}

/// Lado da janela na fonte (px): o que a primitiva dá, `janela` × altura, ou a largura inteira se não couber.
fn lado_da_janela(e: &EfLupa, origem: TamanhoPx) -> f64 {
    let (wo, ho) = (origem.w as f64, origem.h as f64);
    if e.janela * ho >= wo {
        wo
    } else {
        e.janela * ho
    }
}

// CONTRATO: acréscimo. A regra do `item_invalido` da lupa, para a validação acusar antes (como o `recusa`).
/// Campo da lupa que o motor recusa com `item_invalido` numa origem de `origem` px (a da fonte exibida, da imagem
/// ou o canvas do título): os de `camada::lupa::campo_invalido` e a janela abaixo de `JANELA_MIN_PX`. None = ok.
pub fn campo_invalido(e: &EfLupa, origem: TamanhoPx) -> Option<&'static str> {
    camada::lupa::campo_invalido(e).or_else(|| (!(lado_da_janela(e, origem) >= JANELA_MIN_PX)).then_some("janela"))
}

/// Maior janela (px da fonte) que a lupa mostra no clipe: a pedida, ou a que enche o canvas na ampliação pedida
/// (`camada::lupa::primitivas` encolhe a janela da lupa que não cabe) com a imagem inteira enquadrada, que é a
/// menor escala do clipe. Não depende dos slots: igual em toda janela do clipe.
fn janela_maxima(e: &EfLupa, origem: TamanhoPx, canvas: TamanhoPx, giro: Giro) -> f64 {
    let l = lado_da_janela(e, origem);
    let (wo, ho) = if giro.troca { (origem.h, origem.w) } else { (origem.w, origem.h) };
    let s = (canvas.w as f64 / wo as f64).min(canvas.h as f64 / ho as f64);
    let cabe = par_abaixo(canvas.w.min(canvas.h) as f64) as f64 / (e.fator * s);
    if cabe.is_finite() && cabe < l {
        cabe
    } else {
        l
    }
}

/// Recorte com margem (px da fonte, pares) para uma janela de lado `l` px.
fn recorte_de(l: f64, origem: TamanhoPx) -> (i64, i64) {
    let lado = 2 * (l * 0.5).ceil() as i64 + 2 * MARGEM + 2;
    let (lw, lh) = (par_abaixo(origem.w as f64), par_abaixo(origem.h as f64));
    (lado.min(lw).max(2), lado.min(lh).max(2))
}

/// Recorte da lupa no clipe (o da `janela_maxima`): igual em toda janela de slots do clipe.
fn recorte(e: &EfLupa, origem: TamanhoPx, canvas: TamanhoPx, giro: Giro) -> Result<(i64, i64)> {
    if let Some(c) = campo_invalido(e, origem) {
        return Err(invalido(c));
    }
    Ok(recorte_de(janela_maxima(e, origem, canvas, giro), origem))
}

/// Geometria de um slot a partir da primitiva (origem 0..1 da fonte, destino 0..1 do canvas).
fn slot(o: Caixa, d: Caixa, origem: TamanhoPx, canvas: TamanhoPx, (cw, ch): (i64, i64), giro: Giro) -> Result<Slot> {
    let (wo, ho) = (origem.w as f64, origem.h as f64);
    let (wx0, wy0, l) = (o.x * wo, o.y * ho, o.h * ho);
    let (dx, dy, dd) = ((d.x * canvas.w as f64).round(), (d.y * canvas.h as f64).round(), (d.w * canvas.w as f64).round());
    if !(l > 0.0 && dd > 0.0 && [wx0, wy0, dx, dy].iter().all(|v| v.is_finite())) {
        return Err(invalido("lupa"));
    }
    // px da lupa por px da fonte
    let k = dd / l;
    let x1 = (par_abaixo(wx0) - MARGEM).clamp(0, (par_abaixo(wo) - cw).max(0));
    let y1 = (par_abaixo(wy0) - MARGEM).clamp(0, (par_abaixo(ho) - ch).max(0));
    // o recorte e o centro da janela nele, na orientação do vídeo
    let ((gw, gh), (gx, gy)) = giro.levar((cw, ch), (wx0 + l * 0.5 - x1 as f64, wy0 + l * 0.5 - y1 as f64));
    let (sw, sh) = (par(gw as f64 * k).max(2), par(gh as f64 * k).max(2));
    if sw > LADO_MAX || sh > LADO_MAX {
        return Err(invalido("fator"));
    }
    // a escala que a `scale` faz de fato; o centro da janela cai no centro da lupa
    let (kx, ky) = (sw as f64 / gw as f64, sh as f64 / gh as f64);
    let px = par(dx + dd * 0.5 - gx * kx);
    let py = par(dy + dd * 0.5 - gy * ky);
    Ok(Slot { x1, y1, sw, sh, px, py, destino: [dx as i64, dy as i64, dd as i64] })
}

/// Aplica uma lupa sobre `atual` (já no canvas, depois do holofote). `copia` = rótulo da cópia da base tirada antes do
/// enquadramento (px de `origem`); `quadros` = um por slot da janela, com o rastreio do alvo avaliado em
/// `rastreios[e.rastreio]` e `A(τ)` em `a`. Pede ao montador a camada da forma. Devolve o rótulo novo.
/// Só é chamada quando `recusa(e)` é None.
pub fn aplicar(m: &mut dyn Montador, atual: &str, copia: &str, e: &EfLupa, quadros: &[QuadroCamada], origem: TamanhoPx) -> Result<String> {
    let canvas = m.canvas().clone();
    let tela = TamanhoPx { w: canvas.w, h: canvas.h };
    // o `girar` vale no clipe inteiro: o de um slot é o de todos
    let giro = quadros.first().map(|q| Giro::de(&q.a)).unwrap_or_default();
    let (mut cw, mut ch) = recorte(e, origem, tela, giro)?;
    let mut geometria: Vec<Option<(Caixa, Caixa)>> = Vec::with_capacity(quadros.len());
    let mut mascara = Vec::with_capacity(quadros.len());
    for q in quadros {
        let mut q = q.clone();
        if !base_ativa(&e.base, q.t) {
            q.rastreios.remove(&e.rastreio);
        }
        let g = camada::lupa::primitivas(e, &q, &SemSprites)?.into_iter().find_map(|p| match p {
            Primitiva::Lupa { origem: o, destino: d, .. } => Some((o, d)),
            _ => None,
        });
        geometria.push(g);
        // a máscara só depende do destino: na fixa, um alvo fixo deixa os quadros iguais (um PNG só)
        if g.is_some() && matches!(e.onde, OndeLupa::Fixa { .. }) {
            q.rastreios = BTreeMap::from([(e.rastreio.clone(), Caixa { x: 0.5, y: 0.5, w: 0.0, h: 0.0 })]);
        }
        mascara.push(QuadroItem { quadro: q, transformacao: None });
    }
    // enquadramento com região maior que a imagem (escala abaixo da mínima): a janela de um slot pode passar do
    // recorte; aí vale o da janela pedida (só nesse caso a janela de slots pode diferir da exportação em subpixel)
    let maior = geometria.iter().flatten().map(|(o, _)| o.h * origem.h as f64).fold(0.0, f64::max);
    if maior > janela_maxima(e, origem, tela, giro) + 0.5 {
        (cw, ch) = recorte_de(lado_da_janela(e, origem), origem);
    }
    let montar = |recorte: (i64, i64)| -> Result<Vec<Option<Slot>>> { geometria.iter().map(|g| g.map(|(o, d)| slot(o, d, origem, tela, recorte, giro)).transpose()).collect() };
    let slots = match montar((cw, ch)) {
        Ok(s) => s,
        // zoom forte com a lupa ligada: a janela mostrada encolhe e o recorte do clipe, ampliado, passa do teto;
        // vale o da maior janela mostrada NESTA janela de slots (mesma ressalva de cima), em vez de falhar
        Err(e) => {
            let menor = recorte_de(maior, origem);
            if menor.0 >= cw && menor.1 >= ch {
                return Err(e);
            }
            (cw, ch) = menor;
            montar(menor)?
        }
    };
    let Some(primeiro) = slots.iter().flatten().next().copied() else {
        // nenhum slot com lupa nesta janela: a cópia não vai a lugar nenhum
        m.grafo().cadeia(&[copia], &["nullsink".to_string()], &[]);
        return Ok(atual.to_string());
    };
    // união dos destinos (pares, dentro do canvas)
    let (mut ux0, mut uy0, mut ux1, mut uy1) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
    for s in slots.iter().flatten() {
        let [x, y, l] = s.destino;
        (ux0, uy0, ux1, uy1) = (ux0.min(x), uy0.min(y), ux1.max(x + l), uy1.max(y + l));
    }
    let u = [ux0, uy0, ux1 - ux0, uy1 - uy0].map(|v| v.max(0) as u32);
    // slot sem lupa repete o vizinho (a máscara apaga): árvores curtas
    let mut cheio: Vec<Slot> = Vec::with_capacity(slots.len());
    let mut ultimo = primeiro;
    for s in &slots {
        ultimo = s.unwrap_or(ultimo);
        cheio.push(ultimo);
    }
    let fps = canvas.fps;
    let col = |f: fn(&Slot) -> i64| cheio.iter().map(f).collect::<Vec<i64>>();
    let mut efeito = e.clone();
    efeito.ligacao = false;
    let item = ItemCamada { desenho: Desenho::Efeito { efeito: Efeito::Lupa(efeito) }, de: 0, quadros: mascara };
    let n = quadros.len() as u32;
    let ent = m.camada('l', TipoCamada::MascaraCanvas, u, n, vec![item], Vec::new());
    let g = m.grafo();
    let mk = anotacao::entrada_por_slot(g, &ent, fps, n, true);
    // ampliação: recorte da cópia → giro do vídeo → com alfa (o `overlay` pede; antes da `scale`, que pode mudar de lado) → escala
    let fl = flags(e.interpolacao);
    let (sws, shs) = (col(|s| s.sw), col(|s| s.sh));
    let escala = match (lado_por_slot(&sws, fps), lado_por_slot(&shs, fps)) {
        (None, None) => scale(sws[0] as u32, shs[0] as u32, fl),
        (w, h) => filtro(
            "scale",
            None,
            &[
                ("w", w.unwrap_or_else(|| sws[0].to_string())),
                ("h", h.unwrap_or_else(|| shs[0].to_string())),
                ("eval", "frame".into()),
                ("flags", format!("{fl}+accurate_rnd+bitexact")),
            ],
        ),
    };
    let lz = g.rotulo("l");
    let corte = filtro("crop", None, &[("w", cw.to_string()), ("h", ch.to_string()), ("x", por_slot(&col(|s| s.x1), fps)), ("y", por_slot(&col(|s| s.y1), fps))]);
    let mut ampliar = vec![corte];
    ampliar.extend(giro.filtros());
    ampliar.extend(["format=yuva420p".to_string(), escala]);
    g.cadeia(&[copia], &ampliar, &[&lz]);
    let (a, b, fu, lu, la, saida) = (g.rotulo("l"), g.rotulo("l"), g.rotulo("l"), g.rotulo("l"), g.rotulo("l"), g.rotulo("l"));
    g.cadeia(&[atual], &["split".to_string()], &[&a, &b]);
    g.cadeia(&[&b], &[format!("crop={}:{}:{}:{}:exact=1", u[2], u[3], u[0], u[1])], &[&fu]);
    let (xs, ys): (Vec<i64>, Vec<i64>) = cheio.iter().map(|s| (s.px - ux0, s.py - uy0)).unzip();
    let sobre = filtro("overlay", None, &[("x", por_slot(&xs, fps)), ("y", por_slot(&ys, fps)), ("format", "yuv420".into()), ("eof_action", "repeat".into())]);
    g.cadeia(&[&fu, &lz], &[sobre], &[&lu]);
    g.cadeia(&[&lu, &mk], &["alphamerge".to_string()], &[&la]);
    let volta = filtro("overlay", None, &[("x", u[0].to_string()), ("y", u[1].to_string()), ("format", "yuv420".into()), ("eof_action", "repeat".into())]);
    g.cadeia(&[&a, &la], &[volta], &[&saida]);
    Ok(saida)
}

#[cfg(test)]
mod testes {
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::*;
    use crate::edicao::camada::formas;
    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::geometria::a_de;
    use crate::edicao::modelo::{ClipePrincipal, Tolerante};
    use crate::edicao::motor::plano::Plano;
    use crate::edicao::motor::FonteInfo;
    use crate::edicao::quadros::{apoio, TabelaQuadros};
    use crate::edicao::testes::aceite::{alvo_final, ancora, efeito, midia, projeto, rastreio, trecho, Mundo};
    use crate::edicao::testes::executor::{self, Opcoes};
    use crate::edicao::testes::fontes;
    use crate::edicao::testes::medir::{self, QuadroRgb};

    // ------------------------------------------------------------ sem ffmpeg

    fn lupa(id: &str, campos: Value) -> Value {
        let mut c = json!({ "rastreio": "r1", "janela": 0.1, "fator": 4.0, "forma": "circulo", "interpolacao": "bilinear",
            "onde": { "modo": "fixa", "x": 0.75, "y": 0.3 }, "moldura": { "cor": "#00ff00", "espessura": 0.004, "contorno": false }, "ligacao": false });
        for (k, v) in campos.as_object().unwrap() {
            c[k] = v.clone();
        }
        efeito("lupa", id, c)
    }

    /// Projeto de um trecho da mídia m1 com o rastreio r1 (`caixas[j]` no quadro j) e os efeitos.
    fn projeto_com(tab: &TabelaQuadros, (w, h): (u32, u32), n: usize, caixas: &dyn Fn(usize) -> [f64; 4], efeitos: Value) -> crate::edicao::modelo::ProjetoEdicao {
        let ancoras = (0..n).map(|j| ancora(tab.tempo(j), Some(tab.pts[j]), caixas(j))).collect();
        projeto(w, h, (30, 1), vec![midia("m1", "video", json!([]))], vec![trecho("c1", "m1", tab, 0, n, 1.0, efeitos)], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras)] }))
    }

    fn mundo_falso(n: usize, exibido: (u32, u32)) -> (Mundo, FonteInfo) {
        let info = apoio::fonte(apoio::tabela((1, 30), (0..n as i64).collect()), exibido);
        (Mundo::novo(&Arc::new(apoio::cap_falsa())).com("m1", &info), info)
    }

    #[test]
    fn grafo_da_lupa_e_mascara_unica_na_fixa() {
        let (mundo, info) = mundo_falso(30, (1280, 720));
        let mover = |j: usize| [0.3 + j as f64 * 0.003, 0.6, 0.01, 0.02];
        let p = projeto_com(&info.quadros, (1280, 720), 30, &mover, json!([lupa("L1", json!({}))]));
        let plano = mundo.compilar(&p, &alvo_final("cpu")).expect("compila");
        let g = &plano.blocos[0].grafo;
        for trecho in ["format=yuva420p", "alphamerge", "crop=w=82:h=82:x=", "scale=328:328:flags=bilinear+accurate_rnd+bitexact"] {
            assert!(g.contains(trecho), "{trecho} fora do grafo:\n{g}");
        }
        assert!(!g.contains("nullsink"));
        // uma máscara, do tamanho da lupa (0,1 × 720 × 4 = 288 px, centrada em (960, 216)); quadros iguais
        let mk: Vec<_> = plano.camadas.iter().filter(|c| c.tipo == TipoCamada::MascaraCanvas).collect();
        assert_eq!(mk.len(), 1);
        assert_eq!(mk[0].caixa_px, [816, 72, 288, 288]);
        let item = &mk[0].itens[0];
        let Desenho::Efeito { efeito } = &item.desenho else { panic!() };
        let prims: Vec<_> = item.quadros.iter().map(|q| formas::primitivas(efeito, &q.quadro, &SemSprites).unwrap()).collect();
        assert!(prims.iter().all(|p| p == &prims[0]) && !prims[0].is_empty());
        // a anotação leva a moldura
        assert!(plano.camadas.iter().any(|c| c.tipo == TipoCamada::Anotacao));
        // acompanhando: uma máscara por posição
        let p = projeto_com(&info.quadros, (1280, 720), 30, &mover, json!([lupa("L1", json!({ "onde": { "modo": "segue", "dx": 0.2, "dy": -0.2 } }))]));
        let plano = mundo.compilar(&p, &alvo_final("cpu")).expect("compila");
        let mk = plano.camadas.iter().find(|c| c.tipo == TipoCamada::MascaraCanvas).unwrap();
        assert!(mk.caixa_px[2] > 288, "{:?}", mk.caixa_px);
    }

    #[test]
    fn sem_slot_ativo_a_copia_acaba_no_nullsink() {
        let (mundo, info) = mundo_falso(30, (1280, 720));
        let p = projeto_com(&info.quadros, (1280, 720), 30, &|_| [0.4, 0.4, 0.02, 0.02], json!([lupa("L1", json!({ "ativo": { "de": 100.0, "ate": null } }))]));
        let plano = mundo.compilar(&p, &alvo_final("cpu")).expect("compila");
        assert!(plano.blocos[0].grafo.contains("nullsink"));
        assert!(!plano.camadas.iter().any(|c| c.tipo == TipoCamada::MascaraCanvas));
    }

    /// O recorte é o da janela que a lupa mostra no clipe (a pedida, ou a que enche o canvas na ampliação pedida);
    /// com a região do enquadramento maior que a imagem a janela mostrada passa disso e vale o recorte da pedida.
    #[test]
    fn recorte_da_janela_mostrada() {
        let (mundo, info) = mundo_falso(4, (1280, 720));
        let fixo = |_: usize| [0.5, 0.5, 0.0, 0.0];
        let grafo = |efeitos: Value| mundo.compilar(&projeto_com(&info.quadros, (1280, 720), 4, &fixo, efeitos), &alvo_final("cpu")).expect("compila").blocos[0].grafo.clone();
        // 0,5 × 720 = 360 px pedidos; a 8× cabem 90 px
        let l = lupa("L1", json!({ "janela": 0.5, "fator": 8.0 }));
        let g = grafo(json!([l.clone()]));
        assert!(g.contains("crop=w=100:h=100:x=") && g.contains("scale=800:800:"), "{g}");
        // região 2× maior que a imagem: o vídeo fica a 0,5× e a lupa mostra 180 px
        let enq = efeito("enquadramento", "enq", json!({ "regiao": { "x": -0.5, "y": -0.5, "w": 2.0, "h": 2.0 }, "ajuste": "conter", "interpolacao": "bilinear" }));
        let g = grafo(json!([enq, l]));
        assert!(g.contains("crop=w=370:h=370:x=") && g.contains("scale=1480:1480:"), "{g}");
    }

    /// Zoom forte com a lupa ligada (4K, região 1/14, janela 0,15 × 4): com o recorte do clipe (334 px) a ampliação
    /// teria 18 704 px e a exportação falhava com `item_invalido: fator`; vale o recorte da janela mostrada (38,6 px).
    #[test]
    fn zoom_forte_usa_o_recorte_da_janela_mostrada_em_vez_de_falhar() {
        let (mundo, info) = mundo_falso(4, (3840, 2160));
        let fixo = |_: usize| [0.5, 0.5, 0.0, 0.0];
        let (z, m) = (1.0 / 14.0, (1.0 - 1.0 / 14.0) / 2.0);
        let enq = efeito("enquadramento", "enq", json!({ "regiao": { "x": m, "y": m, "w": z, "h": z }, "ajuste": "conter", "interpolacao": "bilinear" }));
        let l = lupa("L1", json!({ "janela": 0.15, "fator": 4.0 }));
        let plano = mundo.compilar(&projeto_com(&info.quadros, (3840, 2160), 4, &fixo, json!([enq, l])), &alvo_final("cpu")).expect("compila");
        let g = &plano.blocos[0].grafo;
        // 2160 / (4 × 14) = 38,57 px → recorte 50; × 56 = 2800 px
        assert!(g.contains("crop=w=50:h=50:x=") && g.contains("scale=2800:2800:"), "{g}");
        // a lupa ocupa a altura do canvas, com a janela no centro dela a até 1 px
        let Efeito::Lupa(e) = serde_json::from_value(lupa("L1", json!({ "janela": 0.15, "fator": 4.0 }))).unwrap() else { unreachable!() };
        let (o, c) = (TamanhoPx { w: 3840, h: 2160 }, TamanhoPx { w: 3840, h: 2160 });
        let a = Afim([14.0, 0.0, 0.0, 14.0, -6.5, -6.5]);
        let q = QuadroCamada { q: 0, t: 0.0, pts: None, a, canvas: c, rastreios: BTreeMap::from([("r1".to_string(), Caixa { x: 0.5, y: 0.5, w: 0.0, h: 0.0 })]), indice: None, programa_s: 0.0 };
        let Primitiva::Lupa { origem: oj, destino: d, .. } = camada::lupa::primitivas(&e, &q, &SemSprites).unwrap()[0] else { panic!() };
        assert!(slot(oj, d, o, c, recorte(&e, o, c, Giro::default()).unwrap(), Giro::default()).is_err(), "o recorte do clipe ainda passa do teto");
        let s = slot(oj, d, o, c, (50, 50), Giro::default()).unwrap();
        // lupa de 2160 px encostada na direita (centro pedido em 2880): [1680, 0]
        assert_eq!((s.sw, s.sh, s.destino), (2800, 2800, [1680, 0, 2160]));
        let centro = s.px as f64 + (0.5 * 3840.0 - s.x1 as f64) * 56.0;
        assert!((centro - 2760.0).abs() <= 1.0 && s.px <= 1680 && s.px + s.sw >= 3840, "{s:?}");
    }

    #[test]
    fn campo_invalido_e_a_regra_do_motor() {
        let l = |c: Value| match serde_json::from_value(lupa("L1", c)).unwrap() {
            Efeito::Lupa(l) => l,
            _ => unreachable!(),
        };
        let o = TamanhoPx { w: 1280, h: 720 };
        assert_eq!(campo_invalido(&l(json!({})), o), None);
        for (c, campo) in [(json!({ "janela": 0.0 }), "janela"), (json!({ "janela": 1.01 }), "janela"), (json!({ "janela": 0.011 }), "janela"), (json!({ "fator": 1.4 }), "fator"), (json!({ "fator": 8.5 }), "fator")] {
            assert_eq!(campo_invalido(&l(c.clone()), o), Some(campo), "{c}");
        }
        // 8 px da fonte bastam; numa origem vertical a janela inteira é a largura
        assert_eq!(campo_invalido(&l(json!({ "janela": 8.0 / 720.0 })), o), None);
        assert_eq!(campo_invalido(&l(json!({ "janela": 1.0 })), TamanhoPx { w: 6, h: 720 }), Some("janela"));
    }

    #[test]
    fn janela_abaixo_de_8_px_e_item_invalido() {
        let (mundo, info) = mundo_falso(30, (1280, 720));
        // 0,01 × 720 = 7,2 px
        let p = projeto_com(&info.quadros, (1280, 720), 30, &|_| [0.4, 0.4, 0.02, 0.02], json!([lupa("L1", json!({ "janela": 0.01 }))]));
        let e = mundo.compilar(&p, &alvo_final("cpu")).unwrap_err().to_string();
        assert!(e.contains("item_invalido") && e.contains("janela"), "{e}");
    }

    fn girar(graus: i32, espelhar_h: bool, espelhar_v: bool) -> Value {
        efeito("girar", "g", json!({ "graus": graus, "espelhar_h": espelhar_h, "espelhar_v": espelhar_v }))
    }

    /// As 4 voltas e os espelhos que mudam a orientação (8 casos).
    const GIROS: [(i32, bool, bool); 8] =
        [(0, false, false), (90, false, false), (180, false, false), (270, false, false), (0, true, false), (0, false, true), (90, true, false), (270, true, false)];

    /// O alvo (centro do rastreio) cai no centro da lupa a até 1 px em qualquer fator, escala e giro, e a ampliação
    /// cobre a lupa inteira. Recortar em px pares da fonte, sem a compensação pelo `overlay`, erra até `fator` px.
    #[test]
    fn alvo_cai_no_centro_da_lupa_a_ate_1_px() {
        let mut pior = 0.0f64;
        for (origem, canvas) in [((1280, 720), (1280, 720)), ((3840, 2160), (1920, 1080)), ((1280, 720), (1920, 1080)), ((640, 480), (1920, 1080)), ((1920, 1080), (854, 480))] {
            let (o, c) = (TamanhoPx { w: origem.0, h: origem.1 }, TamanhoPx { w: canvas.0, h: canvas.1 });
            for (graus, eh, ev) in GIROS {
                let efeitos: Vec<Tolerante<Efeito>> = vec![serde_json::from_value(girar(graus, eh, ev)).unwrap()];
                let a = a_de(&efeitos, 0.0, o, c);
                let giro = Giro::de(&a);
                for fator in [1.5, 2.0, 3.0, 4.0, 6.5, 8.0] {
                    let e: EfLupa = match serde_json::from_value(lupa("L1", json!({ "fator": fator, "janela": 0.12 }))).unwrap() {
                        Efeito::Lupa(l) => l,
                        _ => unreachable!(),
                    };
                    let (cw, ch) = recorte(&e, o, c, giro).unwrap();
                    for i in 0..40 {
                        // centros fracionários, longe da borda
                        let (cx, cy) = (0.3 + i as f64 * 0.0091 + 0.37 / origem.0 as f64, 0.35 + i as f64 * 0.0067 + 0.71 / origem.1 as f64);
                        let q = QuadroCamada { q: 0, t: 0.0, pts: None, a, canvas: c, rastreios: BTreeMap::from([("r1".to_string(), Caixa { x: cx, y: cy, w: 0.0, h: 0.0 })]), indice: None, programa_s: 0.0 };
                        let p = camada::lupa::primitivas(&e, &q, &SemSprites).unwrap();
                        let Primitiva::Lupa { origem: oj, destino: d, .. } = p[0] else { panic!() };
                        let s = slot(oj, d, o, c, (cw, ch), giro).unwrap();
                        // onde o alvo (px da fonte, levado à orientação do vídeo) cai no canvas, e o centro da lupa
                        let ((gw, gh), (gx, gy)) = giro.levar((cw, ch), (cx * o.w as f64 - s.x1 as f64, cy * o.h as f64 - s.y1 as f64));
                        let (kx, ky) = (s.sw as f64 / gw as f64, s.sh as f64 / gh as f64);
                        let (ax, ay) = (s.px as f64 + gx * kx, s.py as f64 + gy * ky);
                        let [dx, dy, dd] = s.destino;
                        let (mx, my) = (dx as f64 + dd as f64 / 2.0, dy as f64 + dd as f64 / 2.0);
                        pior = pior.max((ax - mx).abs()).max((ay - my).abs());
                        assert!(s.px <= dx && s.py <= dy && s.px + s.sw >= dx + dd && s.py + s.sh >= dy + dd, "{origem:?} {canvas:?} {graus} {fator}: {s:?}");
                        assert!(s.px % 2 == 0 && s.py % 2 == 0 && s.x1 % 2 == 0 && s.y1 % 2 == 0);
                    }
                }
            }
        }
        assert!(pior <= 1.0 + 1e-9, "pior {pior} px");
    }

    /// O giro lido de `A` é o do `girar`: os filtros são os do enquadramento (`transpose` → `hflip` → `vflip`)
    /// reduzidos a uma volta horária e aos espelhos que sobram.
    #[test]
    fn giro_lido_de_a() {
        let o = TamanhoPx { w: 1280, h: 720 };
        let de = |graus: i32, eh: bool, ev: bool| {
            let efeitos: Vec<Tolerante<Efeito>> = vec![serde_json::from_value(girar(graus, eh, ev)).unwrap()];
            Giro::de(&a_de(&efeitos, 0.0, o, o)).filtros().join(",")
        };
        assert_eq!(de(0, false, false), "");
        assert_eq!(de(90, false, false), "transpose=dir=clock");
        assert_eq!(de(180, false, false), "hflip,vflip");
        assert_eq!(de(270, false, false), "transpose=dir=clock,hflip,vflip");
        assert_eq!(de(0, true, false), "hflip");
        assert_eq!(de(0, false, true), "vflip");
        assert_eq!(de(90, true, false), "transpose=dir=clock,hflip");
        assert_eq!(de(90, false, true), "transpose=dir=clock,vflip");
        // um ponto do recorte 10×6: giro horário leva (x, y) a (6 − y, x)
        let g = Giro { troca: true, hflip: false, vflip: false };
        assert_eq!(g.levar((10, 6), (1.0, 2.0)), ((6, 10), (4.0, 1.0)));
        assert_eq!(Giro { troca: false, hflip: true, vflip: true }.levar((10, 6), (1.0, 2.0)), ((10, 6), (9.0, 4.0)));
    }

    // ------------------------------------------------------------ com ffmpeg

    /// Fonte sem perdas (FFV1, 30 qps) com os quadros que `desenhar(j, y, u, v)` pinta (yuv420p, cinza 128 antes).
    fn fonte(cap: &Capacidades, pasta: &Path, nome: &str, (w, h): (usize, usize), n: u32, desenhar: &dyn Fn(u32, &mut [u8], &mut [u8], &mut [u8])) -> PathBuf {
        let cru = pasta.join(format!("{nome}.yuv"));
        {
            let mut f = std::io::BufWriter::new(std::fs::File::create(&cru).unwrap());
            let (mut y, mut u, mut v) = (vec![0u8; w * h], vec![0u8; w * h / 4], vec![0u8; w * h / 4]);
            for j in 0..n {
                y.fill(128);
                u.fill(128);
                v.fill(128);
                desenhar(j, &mut y, &mut u, &mut v);
                for p in [&y, &u, &v] {
                    f.write_all(p).unwrap();
                }
            }
        }
        let saida = format!("{nome}.mkv");
        let a: Vec<String> = ["-hide_banner", "-nostdin", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-s", &format!("{w}x{h}"), "-r", "30", "-i", &format!("{nome}.yuv"), "-c:v", "ffv1", "-g", "1", &saida]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let cmd = Comando { nome: "fonte".into(), programa: cap.binario.clone(), args: a, pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join(format!("{nome}.stderr")) };
        let fim = exec::rodar(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
        let _ = std::fs::remove_file(&cru);
        pasta.join(saida)
    }

    fn info_de(cap: &Capacidades, video: &Path) -> FonteInfo {
        let tab = TabelaQuadros::do_decodificador(cap, video, &AtomicBool::new(false), &mut |_| {}).expect("tabela");
        crate::edicao::quadros::fonte_info(cap, video, &"0".repeat(64), Arc::new(tab), None).expect("fonte_info")
    }

    /// Quadrado de lado `l` px no plano y (e croma) com canto (x, y) par.
    fn quadrado(y: &mut [u8], u: &mut [u8], v: &mut [u8], w: usize, (x0, y0, l): (usize, usize, usize), cor: [u8; 3]) {
        for r in y0..y0 + l {
            y[r * w + x0..r * w + x0 + l].fill(cor[0]);
        }
        for r in y0 / 2..(y0 + l) / 2 {
            u[r * w / 2 + x0 / 2..r * w / 2 + (x0 + l) / 2].fill(cor[1]);
            v[r * w / 2 + x0 / 2..r * w / 2 + (x0 + l) / 2].fill(cor[2]);
        }
    }

    /// Vermelho em YUV BT.709 faixa limitada.
    const VERMELHO: [u8; 3] = [63, 102, 240];
    /// Folga do fator medido pela massa de cor (o croma 4:2:0 come a borda do alvo): 2 %.
    const FOLGA_FATOR: f64 = 0.02;

    struct Render {
        rgb: Vec<QuadroRgb>,
        luma: Vec<Vec<u8>>,
        plano: Plano,
    }

    fn renderizar(cap: &Arc<Capacidades>, nome: &str, info: &FonteInfo, p: &crate::edicao::modelo::ProjetoEdicao) -> Render {
        renderizar_alvo(cap, nome, info, p, &alvo_final("cpu"))
    }

    fn renderizar_alvo(cap: &Arc<Capacidades>, nome: &str, info: &FonteInfo, p: &crate::edicao::modelo::ProjetoEdicao, alvo: &crate::edicao::motor::Alvo) -> Render {
        let plano = Mundo::novo(cap).com("m1", info).compilar(p, alvo).unwrap_or_else(|e| panic!("{nome}: {e}"));
        let pasta = fontes::pasta_temporaria(&format!("lupa-{nome}"));
        let r = executor::rodar(cap, &plano, &pasta, &Opcoes { sem_perdas: true, ..Opcoes::default() }).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        let (mut rgb, mut luma) = (Vec::new(), Vec::new());
        let (w, h) = (p.saida.largura as usize, p.saida.altura as usize);
        for b in &r.blocos {
            rgb.extend(medir::quadros(cap, b).unwrap());
            luma.extend(lumas(cap, b, w * h));
        }
        assert_eq!(rgb.len() as u64, plano.total_q, "{nome}: quadros × slots");
        // variável vazia não grava nada (gravava na pasta corrente)
        if let Some(d) = std::env::var("SICRO_GUARDAR_QUADROS").ok().filter(|d| !d.is_empty()) {
            for (i, q) in rgb.iter().enumerate() {
                let _ = image::save_buffer(Path::new(&d).join(format!("lupa-{nome}-{i}.png")), &q.px, q.w, q.h, image::ExtendedColorType::Rgb8);
            }
        }
        Render { rgb, luma, plano }
    }

    /// Luma exata de cada quadro (`extractplanes`).
    fn lumas(cap: &Capacidades, video: &Path, tamanho: usize) -> Vec<Vec<u8>> {
        let pasta = video.parent().unwrap();
        let a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-i", &video.to_string_lossy(), "-map", "0:v:0", "-fps_mode", "passthrough", "-vf", "extractplanes=y", "-f", "rawvideo", "-pix_fmt", "gray", "-"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let cmd = Comando { nome: "luma".into(), programa: cap.binario.clone(), args: a, pasta: pasta.to_path_buf(), ambiente: Vec::new(), sem_ambiente: Vec::new(), stderr: pasta.join("luma.stderr") };
        let (fim, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).unwrap();
        exec::exigir_sucesso(&cmd, &fim).unwrap();
        assert_eq!(out.len() % tamanho, 0);
        out.chunks_exact(tamanho).map(|c| c.to_vec()).collect()
    }

    fn verde(p: &[u8]) -> bool {
        p[1] > 170 && p[0] < 110 && p[2] < 110
    }

    /// Caixa [x0, y0, x1, y1] (px, inclusiva) da moldura verde; None = não há.
    fn moldura(q: &QuadroRgb) -> Option<[u32; 4]> {
        let (mut b, mut n) = ([u32::MAX, u32::MAX, 0, 0], 0);
        for (i, p) in q.px.chunks_exact(3).enumerate() {
            if verde(p) {
                let (x, y) = (i as u32 % q.w, i as u32 / q.w);
                b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                n += 1;
            }
        }
        (n > 50).then_some(b)
    }

    /// Massa de vermelho (quanto R passa de G e B) e o centro dela, dentro de [x0, y0, x1, y1).
    fn vermelho_em(q: &QuadroRgb, r: [u32; 4]) -> (f64, f64, f64) {
        let (mut m, mut sx, mut sy) = (0.0, 0.0, 0.0);
        for y in r[1]..r[3].min(q.h) {
            for x in r[0]..r[2].min(q.w) {
                let i = ((y * q.w + x) * 3) as usize;
                let e = (q.px[i] as f64 - (q.px[i + 1].max(q.px[i + 2])) as f64).max(0.0);
                (m, sx, sy) = (m + e, sx + e * (x as f64 + 0.5), sy + e * (y as f64 + 0.5));
            }
        }
        (m, sx / m, sy / m)
    }

    fn so_lupa(c: &Value) -> Value {
        json!([c])
    }

    /// Aceite §11: alvo centrado a até 2 px, fixa e acompanhando. O alvo (10 px, centro em px ímpar) anda em
    /// diagonal; ampliado 4×, recortar em px pares da fonte o tiraria do centro em 4 px. A lupa fixa liga no
    /// 5º quadro (`ativo`): antes, nem moldura nem ampliação.
    #[test]
    fn alvo_centrado_a_ate_2_px_fixa_e_acompanhando() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-alvo");
        let (w, h, n) = (1280usize, 720usize, 12u32);
        let canto = |j: u32| (400 + 6 * j as usize, 470 + 2 * j as usize);
        let video = fonte(&cap, &pasta, "alvo", (w, h), n, &|j, y, u, v| quadrado(y, u, v, w, (canto(j).0, canto(j).1, 10), VERMELHO));
        let info = info_de(&cap, &video);
        let tab = &info.quadros;
        let caixa = |j: usize| {
            let (x, y) = canto(j as u32);
            [x as f64 / w as f64, y as f64 / h as f64, 10.0 / w as f64, 10.0 / h as f64]
        };
        let casos = [
            ("fixa", lupa("L1", json!({ "ativo": { "de": tab.tempo(4), "ate": null } }))),
            ("acompanhando", lupa("L1", json!({ "forma": "retangulo", "onde": { "modo": "segue", "dx": 0.25, "dy": -0.15 } }))),
        ];
        let mut pior = (0.0f64, 0.0f64);
        for (nome, l) in casos {
            let p = projeto_com(tab, (w as u32, h as u32), n as usize, &caixa, so_lupa(&l));
            let r = renderizar(&cap, nome, &info, &p);
            for (j, q) in r.rgb.iter().enumerate() {
                let (x, y) = canto(j as u32);
                let alvo = (x as f64 + 5.0, y as f64 + 5.0);
                let esperado = if nome == "fixa" { (960.0, 216.0) } else { (alvo.0 + 320.0, alvo.1 - 108.0) };
                if nome == "fixa" && j < 4 {
                    assert!(moldura(q).is_none(), "{nome} quadro {j}: lupa antes do `ativo`");
                    let (m, _, _) = vermelho_em(q, [800, 60, 1120, 380]);
                    assert!(m < 1.0, "{nome} quadro {j}: ampliação antes do `ativo` ({m})");
                    continue;
                }
                let b = moldura(q).unwrap_or_else(|| panic!("{nome} quadro {j}: sem moldura"));
                let centro = ((b[0] + b[2] + 1) as f64 / 2.0, (b[1] + b[3] + 1) as f64 / 2.0);
                assert!((centro.0 - esperado.0).abs() <= 2.0 && (centro.1 - esperado.1).abs() <= 2.0, "{nome} quadro {j}: lupa em {centro:?}, esperada em {esperado:?}");
                let (m, cx, cy) = vermelho_em(q, [b[0] + 8, b[1] + 8, b[2] - 7, b[3] - 7]);
                assert!(m > 255.0 * 1000.0, "{nome} quadro {j}: alvo ampliado não achado ({m})");
                assert!((cx - centro.0).abs() <= 2.0 && (cy - centro.1).abs() <= 2.0, "{nome} quadro {j} ({}): alvo em ({cx:.2}, {cy:.2}), centro da lupa {centro:?}", cap.versao);
                // fator medido nos dois modos: massa do alvo na lupa × no vídeo
                let (mv, _, _) = vermelho_em(q, [x as u32 - 20, y as u32 - 20, x as u32 + 30, y as u32 + 30]);
                let fator = (m / mv).sqrt();
                assert!((fator - 4.0).abs() <= FOLGA_FATOR * 4.0, "{nome} quadro {j} ({}): fator medido {fator:.3}", cap.versao);
                pior = (pior.0.max((cx - centro.0).abs()).max((cy - centro.1).abs()), pior.1.max((centro.0 - esperado.0).abs()).max((centro.1 - esperado.1).abs()));
            }
        }
        eprintln!("lupa ({}): alvo a até {:.2} px do centro; lupa a até {:.1} px do lugar", cap.versao, pior.0, pior.1);
    }

    /// A janela (prévia, bloco) dá os mesmos pixels da exportação inteira: a lupa que acompanha, com recorte,
    /// posição e máscara diferentes em cada slot.
    #[test]
    fn janela_da_os_mesmos_pixels_da_exportacao() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-janela");
        let (w, h, n) = (640usize, 360usize, 10u32);
        let canto = |j: u32| (200 + 6 * j as usize, 230 + 2 * j as usize);
        let video = fonte(&cap, &pasta, "janela", (w, h), n, &|j, y, u, v| quadrado(y, u, v, w, (canto(j).0, canto(j).1, 10), VERMELHO));
        let info = info_de(&cap, &video);
        let caixa = |j: usize| {
            let (x, y) = canto(j as u32);
            [x as f64 / w as f64, y as f64 / h as f64, 10.0 / w as f64, 10.0 / h as f64]
        };
        let l = lupa("L1", json!({ "janela": 0.12, "fator": 3.0, "onde": { "modo": "segue", "dx": 0.3, "dy": -0.25 } }));
        let p = projeto_com(&info.quadros, (w as u32, h as u32), n as usize, &caixa, so_lupa(&l));
        let tudo = renderizar(&cap, "janela-tudo", &info, &p);
        let parte = renderizar_alvo(&cap, "janela-parte", &info, &p, &crate::edicao::motor::Alvo::Previa { altura: h as u32, de_q: 4, ate_q: 9 });
        assert_eq!(parte.luma.len(), 5);
        for (i, y) in parte.luma.iter().enumerate() {
            let dif = y.iter().zip(&tudo.luma[4 + i]).filter(|(a, b)| a != b).count();
            assert_eq!(dif, 0, "slot {} ({}): {dif} px diferentes da exportação", 4 + i, cap.versao);
            // e a cor (croma)
            // e a cor (croma), fora da faixa da moldura: o croma do traço muda quando ele cai na borda do recorte da
            // camada de anotação (do miolo), e esse recorte é a união dos slots da janela
            let (a, b) = (&parte.rgb[i], &tudo.rgb[4 + i]);
            let m = moldura(a).expect("moldura");
            let (cx, cy, raio) = ((m[0] + m[2] + 1) as f64 / 2.0, (m[1] + m[3] + 1) as f64 / 2.0, (m[2] - m[0] + 1) as f64 / 2.0);
            let cor = a.px.chunks_exact(3).zip(b.px.chunks_exact(3)).enumerate().filter(|(k, (x, y))| {
                let (px, py) = ((*k as u32 % a.w) as f64 + 0.5, (*k as u32 / a.w) as f64 + 0.5);
                x != y && ((px - cx).hypot(py - cy) - raio).abs() > 6.0
            });
            assert_eq!(cor.count(), 0, "slot {} ({}): cor diferente da exportação", 4 + i, cap.versao);
        }
        // a lupa está lá (senão a comparação não prova nada)
        assert!(parte.rgb.iter().all(|q| moldura(q).is_some()));
    }

    /// Aceite §11: fator medido. Fonte 1280×720 num canvas 1920×1080 (1,5×): o alvo de 20 px aparece com 30 px no
    /// vídeo e, na lupa de 3×, com 90 px (a ampliação é em relação ao que se vê). Massa de vermelho = área.
    #[test]
    fn fator_medido() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-fator");
        let (w, h, n) = (1280usize, 720usize, 3u32);
        let video = fonte(&cap, &pasta, "fator", (w, h), n, &|_, y, u, v| quadrado(y, u, v, w, (600, 300, 20), VERMELHO));
        let info = info_de(&cap, &video);
        let l = lupa("L1", json!({ "fator": 3.0, "janela": 0.15, "onde": { "modo": "fixa", "x": 0.8, "y": 0.75 } }));
        let p = projeto_com(&info.quadros, (1920, 1080), n as usize, &|_| [600.0 / 1280.0, 300.0 / 720.0, 20.0 / 1280.0, 20.0 / 720.0], so_lupa(&l));
        let r = renderizar(&cap, "fator", &info, &p);
        for (j, q) in r.rgb.iter().enumerate() {
            let b = moldura(q).expect("moldura");
            let (na_lupa, _, _) = vermelho_em(q, [b[0] + 8, b[1] + 8, b[2] - 7, b[3] - 7]);
            // o alvo no vídeo: (610, 310) × 1,5 = (915, 465)
            let (no_video, cx, cy) = vermelho_em(q, [855, 405, 975, 525]);
            assert!((cx - 915.0).abs() < 1.0 && (cy - 465.0).abs() < 1.0, "{j}: alvo no vídeo em ({cx}, {cy})");
            let fator = (na_lupa / no_video).sqrt();
            assert!((fator - 3.0).abs() <= 0.06, "quadro {j} ({}): fator medido {fator:.3} (lupa {na_lupa:.0}, vídeo {no_video:.0})", cap.versao);
            eprintln!("lupa ({}): fator medido {fator:.4} no quadro {j}", cap.versao);
        }
    }

    /// A ampliação medida é a pedida também quando a lupa não cabe no canvas (janela 0,15 × 8 em 720 de altura
    /// daria 864 px): a lupa ocupa a altura inteira e mostra menos em volta do alvo, a 8×. Sem isso saía 6,7×.
    #[test]
    fn fator_medido_na_lupa_que_nao_cabe() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-teto");
        let (w, h, n) = (1280usize, 720usize, 2u32);
        let video = fonte(&cap, &pasta, "teto", (w, h), n, &|_, y, u, v| quadrado(y, u, v, w, (200, 300, 20), VERMELHO));
        let info = info_de(&cap, &video);
        let l = lupa("L1", json!({ "fator": 8.0, "janela": 0.15, "forma": "retangulo", "onde": { "modo": "fixa", "x": 0.7, "y": 0.5 } }));
        let p = projeto_com(&info.quadros, (1280, 720), n as usize, &|_| [200.0 / 1280.0, 300.0 / 720.0, 20.0 / 1280.0, 20.0 / 720.0], so_lupa(&l));
        let r = renderizar(&cap, "teto", &info, &p);
        // recorte da janela que cabe (90 px + margem), não o da pedida
        assert!(r.plano.blocos[0].grafo.contains("crop=w=100:h=100:x="), "{}", r.plano.blocos[0].grafo);
        for (j, q) in r.rgb.iter().enumerate() {
            let b = moldura(q).expect("moldura");
            // lupa de 720 px centrada em (896, 360): a moldura passa 1 px para fora dela
            assert_eq!(((b[0] + b[2] + 1) / 2, b[1], b[3]), (896, 0, 719), "quadro {j}: a lupa ocupa a altura do canvas: {b:?}");
            assert!((b[2] - b[0] + 1).abs_diff(720) <= 4, "quadro {j}: {b:?}");
            let (na_lupa, cx, cy) = vermelho_em(q, [b[0] + 8, b[1] + 8, b[2] - 7, b[3] - 7]);
            let (no_video, _, _) = vermelho_em(q, [150, 250, 270, 370]);
            let fator = (na_lupa / no_video).sqrt();
            assert!((fator - 8.0).abs() <= 0.16, "quadro {j} ({}): fator medido {fator:.3}", cap.versao);
            assert!((cx - 896.0).abs() <= 2.0 && (cy - 360.0).abs() <= 2.0, "quadro {j}: alvo em ({cx:.2}, {cy:.2})");
            eprintln!("lupa ({}): fator medido {fator:.4} na lupa que não cabe", cap.versao);
        }
    }

    /// Aceite §11: a lupa usa os pixels da fonte. Fonte 4K com listras de 1 px (luma 16 e 235) em volta do alvo,
    /// canvas 1080p: no vídeo reduzido as listras somem; na lupa de 4× (2 px da lupa por px da fonte), com
    /// vizinho, cada coluna da lupa é uma coluna da fonte, de 2 px.
    #[test]
    fn pixels_da_fonte_4k_em_canvas_1080p() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-4k");
        let (w, h, n) = (3840usize, 2160usize, 2u32);
        let video = fonte(&cap, &pasta, "4k", (w, h), n, &|_, y, _, _| {
            for r in 1080 - 64..1080 + 64 {
                for x in 1920 - 64..1920 + 64 {
                    y[r * w + x] = if x % 2 == 0 { 16 } else { 235 };
                }
            }
        });
        let info = info_de(&cap, &video);
        assert_eq!(info.exibido, (3840, 2160));
        let l = lupa("L1", json!({ "fator": 4.0, "janela": 0.025, "forma": "retangulo", "interpolacao": "vizinho", "onde": { "modo": "fixa", "x": 0.8, "y": 0.3 } }));
        let p = projeto_com(&info.quadros, (1920, 1080), n as usize, &|_| [0.5 - 4.0 / 3840.0, 0.5 - 4.0 / 2160.0, 8.0 / 3840.0, 8.0 / 2160.0], so_lupa(&l));
        let r = renderizar(&cap, "4k", &info, &p);
        // 0,025 × 2160 = 54 px da fonte = 27 do canvas; × 4 = 108 px, centrada em (1536, 324)
        let (x0, y0, lado) = (1482usize, 270usize, 108usize);
        for (j, y) in r.luma.iter().enumerate() {
            let b = moldura(&r.rgb[j]).expect("moldura");
            let centro = ((b[0] + b[2] + 1) as f64 / 2.0, (b[1] + b[3] + 1) as f64 / 2.0);
            assert_eq!(centro, ((x0 + lado / 2) as f64, (y0 + lado / 2) as f64), "quadro {j}: lupa fora do lugar: {b:?}");
            let (mut colunas, mut fora) = (0, 0);
            for row in y0 + 6..y0 + lado - 6 {
                let linha = &y[row * 1920..(row + 1) * 1920];
                fora += linha[x0 + 6..x0 + lado - 6].iter().filter(|&&v| v != 16 && v != 235).count();
                // cada coluna da fonte vira 2 da lupa: troca de valor a cada 2 px, sempre
                let trocas: Vec<usize> = (x0 + 7..x0 + lado - 6).filter(|&x| linha[x] != linha[x - 1]).collect();
                colunas += trocas.len();
                assert!(trocas.windows(2).all(|t| t[1] - t[0] == 2), "quadro {j} linha {row}: listras de largura diferente de 2: {trocas:?}");
            }
            assert_eq!(fora, 0, "quadro {j} ({}): valores que não são da fonte na lupa", cap.versao);
            assert!(colunas > 40 * 90, "quadro {j}: listras sumiram na lupa ({colunas})");
            // no vídeo reduzido não há listra: cinza perto de 125
            let meio: Vec<u8> = (520..560).flat_map(|r| y[r * 1920 + 940..r * 1920 + 980].to_vec()).collect();
            let (mn, mx) = (*meio.iter().min().unwrap(), *meio.iter().max().unwrap());
            assert!(mx - mn < 40 && (100..150).contains(&mn), "quadro {j}: o vídeo reduzido ainda tem as listras ({mn}..{mx})");
        }
    }

    /// Aceite §11: com bilinear nenhum valor da lupa sai de [mínimo, máximo] da janela (xadrez 60/180 de 8 px,
    /// ampliado 6×). A bicúbica, que cria valores fora (E15), sai da faixa no mesmo quadro: a medida enxerga.
    #[test]
    fn bilinear_dentro_da_faixa_da_janela() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-bilinear");
        let (w, h, n) = (1280usize, 720usize, 2u32);
        let video = fonte(&cap, &pasta, "xadrez", (w, h), n, &|_, y, _, _| {
            for r in 360 - 64..360 + 64 {
                for x in 640 - 64..640 + 64 {
                    y[r * w + x] = if (x / 8 + r / 8) % 2 == 0 { 60 } else { 180 };
                }
            }
        });
        let info = info_de(&cap, &video);
        let mut faixas = Vec::new();
        for interp in ["bilinear", "bicubica"] {
            let l = lupa("L1", json!({ "fator": 6.0, "janela": 0.08, "interpolacao": interp, "onde": { "modo": "fixa", "x": 0.75, "y": 0.35 } }));
            let p = projeto_com(&info.quadros, (w as u32, h as u32), n as usize, &|_| [0.5 - 3.0 / 1280.0, 0.5 - 3.0 / 720.0, 6.0 / 1280.0, 6.0 / 720.0], so_lupa(&l));
            let r = renderizar(&cap, interp, &info, &p);
            // 0,08 × 720 = 57,6 px × 6 → lupa de 346 px centrada em (960, 252); miolo: o quadrado inscrito no círculo
            let (mut mn, mut mx) = (255u8, 0u8);
            for y in &r.luma {
                for row in 252 - 115..252 + 115 {
                    for &v in &y[row * w + 960 - 115..row * w + 960 + 115] {
                        (mn, mx) = (mn.min(v), mx.max(v));
                    }
                }
            }
            faixas.push((interp, mn, mx));
        }
        eprintln!("lupa ({}): faixa na lupa {faixas:?}", cap.versao);
        assert_eq!((faixas[0].1, faixas[0].2), (60, 180), "bilinear ({}): {faixas:?}", cap.versao);
        assert!(faixas[1].1 < 60 || faixas[1].2 > 180, "a bicúbica devia sair da faixa: {faixas:?}");
    }

    /// A cópia da lupa sai depois do `ocultar` (§5.3): o que a mídia oculta não reaparece ampliado. E a lupa no
    /// quadro congelado amplia o alvo do instante congelado.
    #[test]
    fn nao_mostra_o_ocultado_e_amplia_o_congelado() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-ocultar");
        let (w, h, n) = (1280usize, 720usize, 8u32);
        let canto = |j: u32| (400 + 6 * j as usize, 470 + 2 * j as usize);
        let video = fonte(&cap, &pasta, "oc", (w, h), n, &|j, y, u, v| quadrado(y, u, v, w, (canto(j).0, canto(j).1, 10), VERMELHO));
        let info = info_de(&cap, &video);
        let tab = &info.quadros;
        let caixa = |j: usize| {
            let (x, y) = canto(j as u32);
            [x as f64 / w as f64, y as f64 / h as f64, 10.0 / w as f64, 10.0 / h as f64]
        };
        let ancoras = || (0..n as usize).map(|j| ancora(tab.tempo(j), Some(tab.pts[j]), caixa(j))).collect::<Vec<_>>();
        let l = lupa("L1", json!({ "onde": { "modo": "segue", "dx": 0.25, "dy": -0.15 } }));
        let com = |ocultacoes: Value, clipe: Value| projeto(1280, 720, (30, 1), vec![midia("m1", "video", ocultacoes)], vec![clipe], json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras())] }));
        let um_trecho = || trecho("c1", "m1", tab, 0, n as usize, 1.0, json!([l.clone()]));
        // sem ocultar o alvo aparece ampliado; com o sólido da mídia, nem no vídeo nem na lupa
        let visto = renderizar(&cap, "oc-sem", &info, &com(json!([]), um_trecho()));
        let oculto = renderizar(&cap, "oc-com", &info, &com(json!([crate::edicao::testes::aceite::ocultar("o1", "r1", "solido", json!({}))]), um_trecho()));
        for (j, (a, b)) in visto.rgb.iter().zip(&oculto.rgb).enumerate() {
            assert!(moldura(b).is_some(), "quadro {j}: sem lupa");
            let (antes, depois) = (vermelho_em(a, [0, 0, 1280, 720]).0, vermelho_em(b, [0, 0, 1280, 720]).0);
            assert!(antes > 255.0 * 1500.0, "quadro {j}: alvo não achado sem ocultar ({antes})");
            assert!(depois < 255.0, "quadro {j} ({}): o ocultado aparece ({depois})", cap.versao);
        }
        // congelado no quadro 6: o alvo desse quadro no centro da lupa, em todos os slots
        let congelado = json!({ "tipo": "congelar", "id": "c1", "rotulo": "", "midia": "m1", "instante_s": tab.tempo(6), "instante_pts": tab.pts[6], "duracao_s": 0.2, "efeitos": [l.clone()] });
        let r = renderizar(&cap, "congelado", &info, &com(json!([]), congelado));
        assert_eq!(r.rgb.len(), 6);
        let alvo = (canto(6).0 as f64 + 5.0, canto(6).1 as f64 + 5.0);
        for (j, q) in r.rgb.iter().enumerate() {
            let b = moldura(q).unwrap_or_else(|| panic!("congelado, slot {j}: sem moldura"));
            let centro = ((b[0] + b[2] + 1) as f64 / 2.0, (b[1] + b[3] + 1) as f64 / 2.0);
            assert!((centro.0 - (alvo.0 + 320.0)).abs() <= 2.0 && (centro.1 - (alvo.1 - 108.0)).abs() <= 2.0, "congelado, slot {j}: lupa em {centro:?}");
            let (m, cx, cy) = vermelho_em(q, [b[0] + 8, b[1] + 8, b[2] - 7, b[3] - 7]);
            assert!(m > 255.0 * 1000.0 && (cx - centro.0).abs() <= 2.0 && (cy - centro.1).abs() <= 2.0, "congelado, slot {j} ({}): alvo em ({cx:.2}, {cy:.2}), centro {centro:?}", cap.versao);
        }
    }

    /// Azul em YUV BT.709 faixa limitada.
    const AZUL: [u8; 3] = [32, 240, 118];

    /// Caixa [x0, y0, x1, y1] (inclusiva) dos px que `f` aceita dentro de `r` = [x0, y0, x1, y1).
    fn caixa_de(q: &QuadroRgb, r: [u32; 4], f: &dyn Fn(&[u8]) -> bool) -> Option<[u32; 4]> {
        let (mut b, mut n) = ([u32::MAX, u32::MAX, 0, 0], 0);
        for y in r[1]..r[3].min(q.h) {
            for x in r[0]..r[2].min(q.w) {
                let i = ((y * q.w + x) * 3) as usize;
                if f(&q.px[i..i + 3]) {
                    b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                    n += 1;
                }
            }
        }
        (n > 20).then_some(b)
    }

    /// O `girar` faz parte de `A` e a cópia da lupa é tirada antes dele: a ampliação tem de sair girada e espelhada
    /// como o vídeo. Barra vermelha deitada (48×12) com um quadrado azul à direita: na lupa, a barra fica deitada ou
    /// em pé como no vídeo, com o azul do mesmo lado, 4× maior e com o alvo no centro. O centro do alvo cai em px
    /// pares: a janela fica 2 px fora do meio do recorte nos dois eixos, e errar o giro do centro desloca 8 px.
    #[test]
    fn girada_e_espelhada_como_o_video() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-giro");
        let (w, h, n) = (1280usize, 720usize, 2u32);
        let video = fonte(&cap, &pasta, "giro", (w, h), n, &|_, y, u, v| {
            for (x0, blocos, cor) in [(600usize, 4usize, VERMELHO), (660, 1, AZUL)] {
                for k in 0..blocos {
                    quadrado(y, u, v, w, (x0 + 12 * k, 300, 12), cor);
                }
            }
        });
        let info = info_de(&cap, &video);
        let caixa = |_: usize| [600.0 / 1280.0, 300.0 / 720.0, 72.0 / 1280.0, 12.0 / 720.0];
        let vermelho = |p: &[u8]| p[0] > 150 && p[1] < 90 && p[2] < 90;
        let azul = |p: &[u8]| p[2] > 150 && p[0] < 90 && p[1] < 90;
        let meio = |c: [u32; 4]| ((c[0] + c[2] + 1) as f64 / 2.0, (c[1] + c[3] + 1) as f64 / 2.0);
        let deitada = |c: [u32; 4]| c[2] - c[0] > c[3] - c[1];
        // de que lado do vermelho está o azul: (eixo x?, sinal)
        let lado = |r: [u32; 4], a: [u32; 4]| {
            let (d0, d1) = (meio(a).0 - meio(r).0, meio(a).1 - meio(r).1);
            if d0.abs() > d1.abs() {
                (true, d0 > 0.0)
            } else {
                (false, d1 > 0.0)
            }
        };
        for (graus, eh, ev) in GIROS {
            let nome = format!("giro-{graus}-{eh}-{ev}");
            let l = lupa("L1", json!({ "fator": 4.0, "janela": 0.15, "forma": "retangulo", "onde": { "modo": "fixa", "x": 0.8, "y": 0.3 } }));
            let p = projeto_com(&info.quadros, (1280, 720), n as usize, &caixa, json!([girar(graus, eh, ev), l]));
            let r = renderizar(&cap, &nome, &info, &p);
            for q in &r.rgb {
                let b = moldura(q).unwrap_or_else(|| panic!("{nome}: sem moldura"));
                let dentro = [b[0] + 8, b[1] + 8, b[2] - 7, b[3] - 7];
                // o vídeo fica à esquerda da lupa
                let fora = [0, 0, b[0] - 4, q.h];
                let achar = |onde: [u32; 4], f: &dyn Fn(&[u8]) -> bool, o_que: &str| caixa_de(q, onde, f).unwrap_or_else(|| panic!("{nome}: {o_que} não achado"));
                let (rv, av) = (achar(fora, &vermelho, "vermelho no vídeo"), achar(fora, &azul, "azul no vídeo"));
                let (rl, al) = (achar(dentro, &vermelho, "vermelho na lupa"), achar(dentro, &azul, "azul na lupa"));
                assert_eq!(deitada(rl), deitada(rv), "{nome} ({}): barra na lupa {rl:?}, no vídeo {rv:?}", cap.versao);
                assert_eq!(lado(rl, al), lado(rv, av), "{nome} ({}): azul na lupa {al:?} × vermelho {rl:?}; no vídeo {av:?} × {rv:?}", cap.versao);
                // 4× o que se vê: lado maior da barra
                let maior = |c: [u32; 4]| ((c[2] - c[0]).max(c[3] - c[1]) + 1) as f64;
                let fator = maior(rl) / maior(rv);
                assert!((fator - 4.0).abs() <= 0.5, "{nome}: fator {fator:.2} ({rl:?} × {rv:?})");
                // o centro do alvo (vermelho + azul) no centro da lupa
                let alvo = meio([rl[0].min(al[0]), rl[1].min(al[1]), rl[2].max(al[2]), rl[3].max(al[3])]);
                let centro = meio(b);
                assert!((alvo.0 - centro.0).abs() <= 2.0 && (alvo.1 - centro.1).abs() <= 2.0, "{nome} ({}): alvo em {alvo:?}, centro da lupa {centro:?}", cap.versao);
            }
        }
    }

    /// Zoom animado: o lado da lupa acompanha a escala do vídeo (`scale` por quadro) e o alvo segue no centro.
    #[test]
    fn zoom_animado_muda_o_lado_e_mantem_o_alvo_no_centro() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-zoom");
        let (w, h, n) = (1280usize, 720usize, 10u32);
        let video = fonte(&cap, &pasta, "zoom", (w, h), n, &|_, y, u, v| quadrado(y, u, v, w, (634, 354, 12), VERMELHO));
        let info = info_de(&cap, &video);
        let tab = &info.quadros;
        let regiao = json!([
            { "t": tab.tempo(0), "pts": tab.pts[0], "v": { "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0 }, "interp": "linear" },
            { "t": tab.tempo(9), "pts": tab.pts[9], "v": { "x": 0.25, "y": 0.25, "w": 0.5, "h": 0.5 }, "interp": "linear" }
        ]);
        let enq = efeito("enquadramento", "enq", json!({ "regiao": regiao, "ajuste": "conter", "interpolacao": "bilinear" }));
        let l = lupa("L1", json!({ "fator": 2.0, "janela": 0.1, "onde": { "modo": "fixa", "x": 0.8, "y": 0.3 } }));
        let p = projeto_com(tab, (w as u32, h as u32), n as usize, &|_| [634.0 / 1280.0, 354.0 / 720.0, 12.0 / 1280.0, 12.0 / 720.0], json!([enq, l]));
        let r = renderizar(&cap, "zoom", &info, &p);
        assert!(r.plano.blocos[0].grafo.contains("eval=frame"), "lado fixo num zoom animado");
        let efeitos = match &p.principal[0] {
            Tolerante::Conhecido(ClipePrincipal::Trecho(t)) => t.base.efeitos.clone(),
            _ => unreachable!(),
        };
        let mut lados = Vec::new();
        for (j, q) in r.rgb.iter().enumerate() {
            let a = a_de(&efeitos, tab.tempo(j), TamanhoPx { w: 1280, h: 720 }, TamanhoPx { w: 1280, h: 720 });
            // lado esperado: 2 × 72 px da fonte × escala do vídeo nesse quadro
            let esperado = 2.0 * 72.0 * a.0[3];
            let b = moldura(q).expect("moldura");
            let lado = (b[2] - b[0] + 1) as f64;
            assert!((lado - esperado).abs() <= 6.0, "quadro {j} ({}): lado {lado}, esperado {esperado:.1}", cap.versao);
            let centro = ((b[0] + b[2] + 1) as f64 / 2.0, (b[1] + b[3] + 1) as f64 / 2.0);
            let (_, cx, cy) = vermelho_em(q, [b[0] + 8, b[1] + 8, b[2] - 7, b[3] - 7]);
            assert!((cx - centro.0).abs() <= 2.0 && (cy - centro.1).abs() <= 2.0, "quadro {j}: alvo em ({cx:.2}, {cy:.2}), centro {centro:?}");
            lados.push(lado);
        }
        assert!(lados[n as usize - 1] > lados[0] * 1.8, "{lados:?}");
        eprintln!("lupa ({}): lados da moldura no zoom {lados:?}", cap.versao);
    }

    // ------------------------------------------------------------ revisão: o que os aceites não rodavam

    fn amarelo(p: &[u8]) -> bool {
        p[0] > 170 && p[1] > 170 && p[2] < 110
    }

    /// Massa de azul (quanto B passa de R e G) e o centro dela, dentro de [x0, y0, x1, y1).
    fn azul_em(q: &QuadroRgb, r: [u32; 4]) -> (f64, f64, f64) {
        let (mut m, mut sx, mut sy) = (0.0, 0.0, 0.0);
        for y in r[1]..r[3].min(q.h) {
            for x in r[0]..r[2].min(q.w) {
                let i = ((y * q.w + x) * 3) as usize;
                let e = (q.px[i + 2] as f64 - (q.px[i].max(q.px[i + 1])) as f64).max(0.0);
                (m, sx, sy) = (m + e, sx + e * (x as f64 + 0.5), sy + e * (y as f64 + 0.5));
            }
        }
        (m, sx / m, sy / m)
    }

    /// A janela sem nenhum slot ativo (a cópia vai ao `nullsink`) roda de verdade e dá os quadros do clipe sem lupa.
    #[test]
    fn sem_slot_ativo_renderiza_igual_ao_clipe_sem_lupa() {
        // mede os pixels da cadeia; o selo (camada global) cairia na faixa medida
        let _sem_selos = crate::edicao::camada::selos::desligar_no_teste();
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-inativa");
        let (w, h, n) = (640usize, 360usize, 6u32);
        let video = fonte(&cap, &pasta, "inativa", (w, h), n, &|j, y, u, v| quadrado(y, u, v, w, (200 + 6 * j as usize, 230, 10), VERMELHO));
        let info = info_de(&cap, &video);
        let caixa = |j: usize| [(200 + 6 * j) as f64 / w as f64, 230.0 / h as f64, 10.0 / w as f64, 10.0 / h as f64];
        let l = lupa("L1", json!({ "ativo": { "de": 100.0, "ate": null } }));
        let com = renderizar(&cap, "inativa-com", &info, &projeto_com(&info.quadros, (w as u32, h as u32), n as usize, &caixa, so_lupa(&l)));
        assert!(com.plano.blocos[0].grafo.contains("nullsink"));
        let sem = renderizar(&cap, "inativa-sem", &info, &projeto_com(&info.quadros, (w as u32, h as u32), n as usize, &caixa, json!([])));
        assert_eq!(com.rgb.len(), n as usize);
        for (j, (a, b)) in com.rgb.iter().zip(&sem.rgb).enumerate() {
            assert!(a.px == b.px, "quadro {j} ({}): a lupa inativa mudou o quadro", cap.versao);
        }
    }

    /// Duas lupas no mesmo clipe (duas cópias no `split`, uma sobre a outra): cada uma amplia o alvo dela.
    #[test]
    fn duas_lupas_no_mesmo_clipe() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-duas");
        let (w, h, n) = (1280usize, 720usize, 4u32);
        let ver = |j: u32| (300 + 6 * j as usize, 500usize);
        let azu = |j: u32| (900usize, 560 - 4 * j as usize);
        let video = fonte(&cap, &pasta, "duas", (w, h), n, &|j, y, u, v| {
            quadrado(y, u, v, w, (ver(j).0, ver(j).1, 10), VERMELHO);
            quadrado(y, u, v, w, (azu(j).0, azu(j).1, 10), AZUL);
        });
        let info = info_de(&cap, &video);
        let tab = &info.quadros;
        let ancoras = |c: &dyn Fn(u32) -> (usize, usize)| (0..n).map(|j| ancora(tab.tempo(j as usize), Some(tab.pts[j as usize]), [c(j).0 as f64 / w as f64, c(j).1 as f64 / h as f64, 10.0 / w as f64, 10.0 / h as f64])).collect::<Vec<_>>();
        let l1 = lupa("L1", json!({}));
        let l2 = lupa("L2", json!({ "rastreio": "r2", "forma": "retangulo", "onde": { "modo": "segue", "dx": -0.3, "dy": -0.3 }, "moldura": { "cor": "#ffff00", "espessura": 0.004, "contorno": false } }));
        let p = projeto(
            w as u32,
            h as u32,
            (30, 1),
            vec![midia("m1", "video", json!([]))],
            vec![trecho("c1", "m1", tab, 0, n as usize, 1.0, json!([l1, l2]))],
            json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras(&ver)), rastreio("r2", Some("m1"), "fonte", ancoras(&azu))] }),
        );
        let r = renderizar(&cap, "duas", &info, &p);
        assert_eq!(r.plano.camadas.iter().filter(|c| c.tipo == TipoCamada::MascaraCanvas).count(), 2);
        for (j, q) in r.rgb.iter().enumerate() {
            // L1: moldura verde em (960, 216), o vermelho no centro, 4× maior que no vídeo
            let b = moldura(q).unwrap_or_else(|| panic!("quadro {j}: sem a moldura da primeira"));
            let centro = ((b[0] + b[2] + 1) as f64 / 2.0, (b[1] + b[3] + 1) as f64 / 2.0);
            assert!((centro.0 - 960.0).abs() <= 2.0 && (centro.1 - 216.0).abs() <= 2.0, "quadro {j}: primeira em {centro:?}");
            let (m, cx, cy) = vermelho_em(q, [b[0] + 8, b[1] + 8, b[2] - 7, b[3] - 7]);
            let (x, y) = ver(j as u32);
            let (mv, _, _) = vermelho_em(q, [x as u32 - 20, y as u32 - 20, x as u32 + 30, y as u32 + 30]);
            assert!((cx - centro.0).abs() <= 2.0 && (cy - centro.1).abs() <= 2.0, "quadro {j} ({}): vermelho em ({cx:.2}, {cy:.2}), centro {centro:?}", cap.versao);
            assert!(((m / mv).sqrt() - 4.0).abs() <= 0.08, "quadro {j}: fator da primeira {:.3}", (m / mv).sqrt());
            // L2: moldura amarela ao lado do azul, o azul no centro
            let a = caixa_de(q, [0, 0, q.w, q.h], &amarelo).unwrap_or_else(|| panic!("quadro {j}: sem a moldura da segunda"));
            let centro = ((a[0] + a[2] + 1) as f64 / 2.0, (a[1] + a[3] + 1) as f64 / 2.0);
            let (x, y) = azu(j as u32);
            let esperado = (x as f64 + 5.0 - 0.3 * w as f64, y as f64 + 5.0 - 0.3 * h as f64);
            assert!((centro.0 - esperado.0).abs() <= 2.0 && (centro.1 - esperado.1).abs() <= 2.0, "quadro {j}: segunda em {centro:?}, esperada em {esperado:?}");
            let (m, cx, cy) = azul_em(q, [a[0] + 8, a[1] + 8, a[2] - 7, a[3] - 7]);
            let (mv, _, _) = azul_em(q, [x as u32 - 20, y as u32 - 20, x as u32 + 30, y as u32 + 30]);
            assert!((cx - centro.0).abs() <= 2.0 && (cy - centro.1).abs() <= 2.0, "quadro {j} ({}): azul em ({cx:.2}, {cy:.2}), centro {centro:?}", cap.versao);
            assert!(((m / mv).sqrt() - 4.0).abs() <= 0.08, "quadro {j}: fator da segunda {:.3}", (m / mv).sqrt());
        }
    }

    /// Lupa que acompanha no SEGUNDO clipe, a 29,97 e em meia velocidade: as árvores de `t` do recorte e da posição
    /// valem no tempo local da janela do clipe, e cada quadro repetido leva a lupa do quadro dele.
    #[test]
    fn segundo_clipe_a_2997_em_meia_velocidade_acompanha_o_alvo() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-2997");
        let (w, h, n) = (640usize, 360usize, 12u32);
        let canto = |j: u32| (200 + 6 * j as usize, 230 + 2 * j as usize);
        let video = fonte(&cap, &pasta, "2997", (w, h), n, &|j, y, u, v| quadrado(y, u, v, w, (canto(j).0, canto(j).1, 10), VERMELHO));
        let info = info_de(&cap, &video);
        let tab = &info.quadros;
        let ancoras = (0..n).map(|j| ancora(tab.tempo(j as usize), Some(tab.pts[j as usize]), [canto(j).0 as f64 / w as f64, canto(j).1 as f64 / h as f64, 10.0 / w as f64, 10.0 / h as f64])).collect::<Vec<_>>();
        let l = lupa("L1", json!({ "janela": 0.12, "fator": 3.0, "forma": "retangulo", "onde": { "modo": "segue", "dx": 0.3, "dy": -0.25 } }));
        let p = projeto(
            w as u32,
            h as u32,
            (30000, 1001),
            vec![midia("m1", "video", json!([]))],
            vec![trecho("c1", "m1", tab, 0, 6, 1.0, json!([])), trecho("c2", "m1", tab, 6, 12, 0.5, json!([l]))],
            json!({ "rastreios": [rastreio("r1", Some("m1"), "fonte", ancoras)] }),
        );
        let r = renderizar(&cap, "2997", &info, &p);
        assert!(r.rgb.len() >= 17, "{} slots", r.rgb.len());
        let mut vistos = std::collections::BTreeSet::new();
        for (i, q) in r.rgb.iter().enumerate() {
            if i < 6 {
                assert!(moldura(q).is_none(), "slot {i}: lupa no primeiro clipe");
                continue;
            }
            // o alvo no vídeo diz o quadro de fonte do slot
            let (_, ax, ay) = vermelho_em(q, [190, 220, 330, 290]);
            let j = ((ax - 205.0) / 6.0).round() as u32;
            assert!((6..12).contains(&j) && (ax - (canto(j).0 as f64 + 5.0)).abs() < 0.6 && (ay - (canto(j).1 as f64 + 5.0)).abs() < 0.6, "slot {i}: alvo em ({ax:.2}, {ay:.2})");
            vistos.insert(j);
            let b = moldura(q).unwrap_or_else(|| panic!("slot {i}: sem moldura"));
            let centro = ((b[0] + b[2] + 1) as f64 / 2.0, (b[1] + b[3] + 1) as f64 / 2.0);
            let esperado = (ax + 0.3 * w as f64, ay - 0.25 * h as f64);
            assert!((centro.0 - esperado.0).abs() <= 2.0 && (centro.1 - esperado.1).abs() <= 2.0, "slot {i} (quadro {j}, {}): lupa em {centro:?}, esperada em {esperado:?}", cap.versao);
            let (m, cx, cy) = vermelho_em(q, [b[0] + 8, b[1] + 8, b[2] - 7, b[3] - 7]);
            assert!(m > 255.0 * 500.0 && (cx - centro.0).abs() <= 2.0 && (cy - centro.1).abs() <= 2.0, "slot {i} (quadro {j}, {}): alvo em ({cx:.2}, {cy:.2}), centro {centro:?}", cap.versao);
        }
        assert_eq!(vistos.len(), 6, "{vistos:?}");
    }

    /// Alvo no canto da fonte: a janela encosta na borda e a ampliação cobre a lupa inteira, com o canto da fonte no
    /// canto da lupa (nenhum px do vídeo de baixo, que ali é branco, aparece por dentro).
    #[test]
    fn alvo_no_canto_da_fonte_a_lupa_fica_coberta() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-canto");
        let (w, h, n) = (1280usize, 720usize, 2u32);
        let video = fonte(&cap, &pasta, "canto", (w, h), n, &|_, y, u, v| {
            // branco debaixo da lupa (816..1104 × 72..360)
            for r in 60..380 {
                y[r * w + 800..r * w + 1120].fill(235);
            }
            quadrado(y, u, v, w, (0, 0, 10), VERMELHO);
            quadrado(y, u, v, w, (1270, 710, 10), VERMELHO);
        });
        let info = info_de(&cap, &video);
        let cantos = [[0.0, 0.0, 10.0 / 1280.0, 10.0 / 720.0], [1270.0 / 1280.0, 710.0 / 720.0, 10.0 / 1280.0, 10.0 / 720.0]];
        let l = lupa("L1", json!({ "forma": "retangulo", "interpolacao": "vizinho" }));
        let p = projeto_com(&info.quadros, (w as u32, h as u32), n as usize, &|j| cantos[j], so_lupa(&l));
        let r = renderizar(&cap, "canto", &info, &p);
        let vermelho = |p: &[u8]| p[0] > 150 && p[1] < 90 && p[2] < 90;
        // janela de 72 px encostada no canto: o quadrado de 10 px vira 40 px no canto da lupa
        let esperado = [[816u32, 72, 855, 111], [1064, 320, 1103, 359]];
        for (j, q) in r.rgb.iter().enumerate() {
            let b = moldura(q).expect("moldura");
            assert!(b[0].abs_diff(815) <= 2 && b[1].abs_diff(71) <= 2 && b[2].abs_diff(1104) <= 2 && b[3].abs_diff(360) <= 2, "quadro {j}: moldura {b:?}");
            let claros = (76..356usize).flat_map(|row| r.luma[j][row * w + 820..row * w + 1100].iter().copied().collect::<Vec<u8>>()).filter(|&v| v > 200).count();
            assert_eq!(claros, 0, "quadro {j} ({}): o vídeo de baixo aparece dentro da lupa", cap.versao);
            let v = caixa_de(q, [816, 72, 1104, 360], &vermelho).unwrap_or_else(|| panic!("quadro {j}: canto ampliado não achado"));
            let e = esperado[j];
            assert!((0..4).all(|k| v[k].abs_diff(e[k]) <= 3), "quadro {j} ({}): canto ampliado em {v:?}, esperado {e:?}", cap.versao);
        }
    }

    /// Lupa em clipe de imagem (a origem é a foto, rastreio local) e em título (a origem é o canvas): os quatro
    /// quadrantes da foto em volta do centro da lupa; no título, a lupa sai sobre o fundo.
    #[test]
    fn em_clipe_de_imagem_e_em_titulo() {
        let Some(cap) = fontes::cap() else { return };
        let pasta = fontes::pasta_temporaria("lupa-foto");
        let (iw, ih) = (400u32, 300u32);
        let cores: [[u8; 4]; 4] = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]];
        let mut px = Vec::with_capacity((iw * ih * 4) as usize);
        for y in 0..ih {
            for x in 0..iw {
                px.extend_from_slice(&cores[(usize::from(y >= ih / 2) << 1) | usize::from(x >= iw / 2)]);
            }
        }
        let caminho = pasta.join("foto.png");
        std::fs::write(&caminho, camada::Bitmap { w: iw, h: ih, canais: 4, px }.png().unwrap()).unwrap();
        let mut foto = apoio::fonte(apoio::tabela((1, 1), vec![0]), (iw, ih));
        foto.caminho = caminho;
        foto.pix_fmt = "rgba".into();
        // foto 400×300 em 640×360: 1,2×; janela 0,2 = 60 px da foto = 72 do canvas; × 2 = 144 px em (128, 108)
        let l = |id: &str| {
            lupa(id, json!({ "janela": 0.2, "fator": 2.0, "forma": "retangulo", "interpolacao": "vizinho", "onde": { "modo": "fixa", "x": 0.2, "y": 0.3 },
                "moldura": { "cor": "#ffff00", "espessura": 0.004, "contorno": false } }))
        };
        let p = projeto(
            640,
            360,
            (25, 1),
            vec![midia("m1", "imagem", json!([]))],
            vec![
                json!({ "tipo": "imagem", "id": "c1", "rotulo": "", "midia": "m1", "duracao_s": 0.2, "efeitos": [l("L1")] }),
                json!({ "tipo": "titulo", "id": "c2", "rotulo": "", "duracao_s": 0.2, "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": "#000000" }, "blocos": [] }, "efeitos": [l("L2")] }),
            ],
            json!({ "rastreios": [rastreio("r1", None, "local", vec![ancora(0.0, None, [0.5, 0.5, 0.0, 0.0])])] }),
        );
        let r = renderizar(&cap, "foto", &foto, &p);
        assert_eq!(r.rgb.len(), 10);
        let perto = |q: &QuadroRgb, x: u32, y: u32, cor: [u8; 3]| {
            let i = ((y * q.w + x) * 3) as usize;
            // `medir::quadros` não converte por BT.709: folga de cor larga, o que se mede é o quadrante
            q.px[i..i + 3].iter().zip(&cor).all(|(a, b)| a.abs_diff(*b) <= 40)
        };
        for (i, q) in r.rgb.iter().enumerate() {
            let b = caixa_de(q, [0, 0, q.w, q.h], &amarelo).unwrap_or_else(|| panic!("slot {i}: sem moldura"));
            let centro = ((b[0] + b[2] + 1) / 2, (b[1] + b[3] + 1) / 2);
            assert!(centro.0.abs_diff(128) <= 2 && centro.1.abs_diff(108) <= 2 && (b[2] - b[0]).abs_diff(144) <= 4, "slot {i}: moldura {b:?}");
            if i < 5 {
                for (dx, dy, cor) in [(-1i32, -1i32, [255u8, 0, 0]), (1, -1, [0, 255, 0]), (-1, 1, [0, 0, 255]), (1, 1, [255, 255, 255])] {
                    for d in [8, 40] {
                        let (x, y) = ((128 + dx * d) as u32, (108 + dy * d) as u32);
                        assert!(perto(q, x, y, cor), "slot {i} ({}): ({x}, {y}) não é {cor:?}", cap.versao);
                    }
                }
            } else {
                // título preto: dentro da lupa, preto
                assert!(perto(q, 128, 108, [0, 0, 0]) && perto(q, 90, 150, [0, 0, 0]), "slot {i}: título");
            }
        }
    }
}
