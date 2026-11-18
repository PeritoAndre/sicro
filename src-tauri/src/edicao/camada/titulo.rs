//! Títulos como primitivas (§5.5): fundo (cor, imagem ou nada) e um sprite por bloco, em 0..1 do canvas.
//! Espelho no front: `modelo/titulos.ts`, pelas fixtures `primitivas-titulo.json` e `titulos-modelos.json`.
//! Donos: P0.5t (primitivas), P1.7 (modelos).
//!
//! `modelo` é só rótulo: todo bloco traz o próprio `lugar` e é posto como o `EfTexto` (canto arredondado ao pixel).
//! Fundo de cor = retângulo do canvas inteiro; de imagem = a imagem cobrindo o canvas sem distorcer, centrada, com o
//! que sobra cortado. Contas como em `formas` (o espelho repete na mesma ordem): em px do canvas, só `+ − × ÷`,
//! `floor` e `max`, e a saída passa por `+ 0`.
//!
//! Os 5 modelos do §8.6 são arranjos dos blocos (lugar, largura, tamanho, peso, alinhamento, cor, caixa) e do fundo,
//! gravados nos blocos quando o perito escolhe o modelo. O texto sai sempre do sprite único de `texto.rs`: por isso
//! a prévia e o render usam o mesmo arquivo (mesmo SHA-256). Sem modelo Typst por título (decisão do orquestrador).

use super::formas::{Primitiva, QuadroCamada, Segmento};
use super::texto::{FonteSprites, PedidoSprite};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{
    Alinhamento, AlinhamentoV, BlocoTexto, Caixa, CaixaTexto, EstiloTexto, Fonte, Fundo, Lugar, ModeloTitulo, PapelTexto, TamanhoPx, Titulo,
};
use crate::error::{Result, SicroError};

/// −0 → 0.
fn z(v: f64) -> f64 {
    v + 0.0
}

fn fator_h(h: Alinhamento) -> f64 {
    match h {
        Alinhamento::Esq => 0.0,
        Alinhamento::Centro => 0.5,
        Alinhamento::Dir => 1.0,
    }
}

fn fator_v(v: AlinhamentoV) -> f64 {
    match v {
        AlinhamentoV::Topo => 0.0,
        AlinhamentoV::Meio => 0.5,
        AlinhamentoV::Base => 1.0,
    }
}

/// Arredonda ao pixel (meio sobe), igual a `Math.floor(v + 0.5)`.
fn ao_pixel(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// Canto de bloco mais longe que isto do canvas (px, 2^30): recusado. A cópia do sprite estoura o `i32` do tiny-skia
/// (pânico) e nada visível fica tão longe.
const MAX_LUGAR_PX: f64 = 1_073_741_824.0;

fn longe(v: f64) -> bool {
    !v.is_finite() || v.abs() > MAX_LUGAR_PX
}

/// Caixa em px do canvas `cw`×`ch` → 0..1.
fn caixa(x: f64, y: f64, w: f64, h: f64, cw: f64, ch: f64) -> Caixa {
    Caixa { x: z(x / cw), y: z(y / ch), w: z(w / cw), h: z(h / ch) }
}

/// Primitivas do título no quadro: fundo (cor ou imagem) e um sprite por bloco, em 0..1 do canvas.
/// Bloco de conteúdo vazio não desenha. Imagem de fundo que `sprites` não conhece: erro `midia_ausente`.
/// Bloco com o canto a mais de `MAX_LUGAR_PX` do canvas (ou não finito): erro `item_invalido`.
pub fn primitivas(titulo: &Titulo, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    let (cw, ch) = (quadro.canvas.w as f64, quadro.canvas.h as f64);
    let mut v = Vec::with_capacity(titulo.blocos.len() + 1);
    match &titulo.fundo {
        Fundo::Transparente => {}
        Fundo::Cor { cor } => v.push(Primitiva::Caminho {
            d: vec![Segmento::M(0.0, 0.0), Segmento::L(1.0, 0.0), Segmento::L(1.0, 1.0), Segmento::L(0.0, 1.0), Segmento::Z],
            traco: None,
            preenche: Some(cor.clone()),
            regra: None,
        }),
        Fundo::Imagem { midia } => {
            let tam = sprites
                .imagem(midia)
                .filter(|s| s.w > 0 && s.h > 0)
                .ok_or_else(|| SicroError::Validation(format!("{}: {midia}", codigo::MIDIA_AUSENTE)))?;
            let (iw, ih) = (tam.w as f64, tam.h as f64);
            // a maior das duas escalas cobre o canvas; a mesma nos dois eixos não distorce
            let e = (cw / iw).max(ch / ih);
            let (sw, sh) = (iw * e, ih * e);
            v.push(Primitiva::Sprite {
                src: format!("midia:{midia}"),
                caixa: caixa((cw - sw) * 0.5, (ch - sh) * 0.5, sw, sh, cw, ch),
                opacidade: 1.0,
            });
        }
    }
    for (i, b) in titulo.blocos.iter().enumerate() {
        if b.conteudo.is_empty() {
            continue;
        }
        let s = sprites.sprite(&pedido(titulo, i, quadro.canvas))?;
        let (sw, sh) = (s.w as f64, s.h as f64);
        let x0 = ao_pixel(b.lugar.x * cw - sw * fator_h(b.lugar.h));
        let y0 = ao_pixel(b.lugar.y * ch - sh * fator_v(b.lugar.v));
        if longe(x0) || longe(y0) {
            return Err(SicroError::Validation(format!("{}: lugar do bloco {}", codigo::ITEM_INVALIDO, b.id)));
        }
        v.push(Primitiva::Sprite { src: s.relativo, caixa: caixa(x0, y0, sw, sh, cw, ch), opacidade: 1.0 });
    }
    Ok(v)
}

/// Pedido de sprite de um bloco do título para esse canvas: o bloco sozinho, com `largura_max` convertida de
/// fração da largura (bloco) para fração da ALTURA (pedido). Índice fora da lista: pedido sem bloco e de largura 0,
/// que o rasterizador de texto recusa com `item_invalido`.
pub fn pedido(titulo: &Titulo, bloco: usize, canvas: TamanhoPx) -> PedidoSprite {
    let (cw, ch) = (canvas.w as f64, canvas.h as f64);
    match titulo.blocos.get(bloco) {
        Some(b) => PedidoSprite { blocos: vec![b.clone()], largura_max: b.largura_max * cw / ch, altura_canvas: canvas.h, fundo: true },
        None => PedidoSprite { blocos: Vec::new(), largura_max: 0.0, altura_canvas: canvas.h, fundo: true },
    }
}

// ---------------------------------------------------------------- modelos (P1.7)

// CONTRATO: acréscimo (P1.7): `AREA_SEGURA`, `FUNDO_CHEIO`, `Arranjo`, `arranjo`, `escala_do_canvas`,
// `sobre_o_video`, `aplicar_modelo`, `bloco_no_modelo`, `titulo_no_modelo`, `na_area_segura`. Espelho em
// `modelo/titulos.ts` (fixture `titulos-modelos.json`).

/// Área segura do texto, em fração de cada lado do canvas (a da validação, contratos-fase0 (i) 4).
pub const AREA_SEGURA: f64 = 0.02;

/// Fundo que um modelo de cartão põe quando o título não tem nenhum.
pub const FUNDO_CHEIO: &str = "#000000";

const BRANCO: &str = "#ffffff";
const CINZA: &str = "#c8c8c8";
const FAIXA: &str = "#0000008c";

/// Como um papel fica num modelo: ponto de apoio, largura (fração da largura do canvas), tamanho (fração da altura),
/// peso, alinhamento das linhas, cor e caixa de fundo.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Arranjo {
    pub lugar: Lugar,
    pub largura_max: f64,
    pub tamanho: f64,
    pub negrito: bool,
    pub alinhamento: Alinhamento,
    pub cor: &'static str,
    pub caixa: Option<CaixaTexto>,
}

/// Uma linha da tabela de `arranjo`; `faixa` = margem da caixa (faixa escura), quando há.
#[allow(clippy::too_many_arguments)]
fn arr(x: f64, y: f64, h: Alinhamento, v: AlinhamentoV, largura_max: f64, tamanho: f64, negrito: bool, cor: &'static str, faixa: Option<f64>) -> Arranjo {
    Arranjo {
        lugar: Lugar { x, y, h, v },
        largura_max,
        tamanho,
        negrito,
        alinhamento: h,
        cor,
        caixa: faixa.map(|margem| CaixaTexto { cor: FAIXA.into(), margem, raio: 0.0, faixa: false }),
    }
}

/// Arranjo do papel no modelo. `livre` não arranja: bloco novo nele vai como na cartela.
pub fn arranjo(modelo: ModeloTitulo, papel: PapelTexto) -> Arranjo {
    use Alinhamento::{Centro, Dir, Esq};
    use AlinhamentoV::{Base, Topo};
    use ModeloTitulo as M;
    use PapelTexto as P;
    match (modelo, papel) {
        // cartão centrado (as posições de `inspetor/novos.ts`, as da Fase 0)
        (M::Cartela | M::Livre, P::Titulo) => arr(0.5, 0.44, Centro, Base, 0.86, 0.078, true, BRANCO, None),
        (M::Cartela | M::Livre, P::Subtitulo) => arr(0.5, 0.5, Centro, Topo, 0.86, 0.052, false, BRANCO, None),
        (M::Cartela | M::Livre, P::Corpo) => arr(0.5, 0.62, Centro, Topo, 0.86, 0.042, false, BRANCO, None),
        (M::Cartela | M::Livre, P::Rodape) => arr(0.5, 0.93, Centro, Base, 0.86, 0.033, false, BRANCO, None),
        // cabeçalho à esquerda
        (M::Abertura, P::Titulo) => arr(0.08, 0.34, Esq, Base, 0.84, 0.078, true, BRANCO, None),
        (M::Abertura, P::Subtitulo) => arr(0.08, 0.38, Esq, Topo, 0.84, 0.046, false, BRANCO, None),
        (M::Abertura, P::Corpo) => arr(0.08, 0.52, Esq, Topo, 0.84, 0.036, false, BRANCO, None),
        (M::Abertura, P::Rodape) => arr(0.08, 0.93, Esq, Base, 0.84, 0.03, false, CINZA, None),
        // faixas no alto, sobre o vídeo
        (M::Evento, P::Titulo) => arr(0.5, 0.05, Centro, Topo, 0.9, 0.052, true, BRANCO, Some(0.012)),
        (M::Evento, P::Subtitulo) => arr(0.5, 0.15, Centro, Topo, 0.9, 0.036, false, BRANCO, Some(0.012)),
        (M::Evento, P::Corpo) => arr(0.5, 0.225, Centro, Topo, 0.9, 0.033, false, BRANCO, Some(0.012)),
        (M::Evento, P::Rodape) => arr(0.5, 0.95, Centro, Base, 0.9, 0.028, false, BRANCO, Some(0.012)),
        // faixas à esquerda no terço de baixo; rodapé no canto de cima
        (M::TercoInferior, P::Titulo) => arr(0.06, 0.76, Esq, Base, 0.7, 0.052, true, BRANCO, Some(0.013)),
        (M::TercoInferior, P::Subtitulo) => arr(0.06, 0.775, Esq, Topo, 0.7, 0.033, false, BRANCO, Some(0.013)),
        (M::TercoInferior, P::Corpo) => arr(0.06, 0.855, Esq, Topo, 0.7, 0.028, false, BRANCO, Some(0.013)),
        (M::TercoInferior, P::Rodape) => arr(0.94, 0.06, Dir, Topo, 0.4, 0.024, false, BRANCO, Some(0.01)),
        // cartão centrado, mais alto
        (M::Encerramento, P::Titulo) => arr(0.5, 0.42, Centro, Base, 0.86, 0.078, true, BRANCO, None),
        (M::Encerramento, P::Subtitulo) => arr(0.5, 0.46, Centro, Topo, 0.86, 0.042, false, BRANCO, None),
        (M::Encerramento, P::Corpo) => arr(0.5, 0.58, Centro, Topo, 0.86, 0.036, false, BRANCO, None),
        (M::Encerramento, P::Rodape) => arr(0.5, 0.94, Centro, Base, 0.86, 0.028, false, CINZA, None),
    }
}

/// Escala do texto do modelo no canvas: 1 de 4:3 para cima; mais estreito (vertical), a largura manda, porque o
/// tamanho é fração da altura e o texto não caberia na área segura.
pub fn escala_do_canvas(canvas: TamanhoPx) -> f64 {
    let k = (canvas.w as f64 * 3.0) / (canvas.h as f64 * 4.0);
    if k.is_finite() && k > 0.0 {
        k.min(1.0)
    } else {
        1.0
    }
}

/// O arranjo com tamanho e margem da faixa na escala `k`.
fn na_escala(mut a: Arranjo, k: f64) -> Arranjo {
    a.tamanho *= k;
    if let Some(c) = a.caixa.as_mut() {
        c.margem *= k;
    }
    a
}

/// Modelo de faixa sobre o vídeo (preso a um clipe, sem fundo).
pub fn sobre_o_video(modelo: ModeloTitulo) -> bool {
    matches!(modelo, ModeloTitulo::Evento | ModeloTitulo::TercoInferior)
}

fn arranjar(b: &mut BlocoTexto, a: &Arranjo) {
    b.lugar = a.lugar;
    b.largura_max = a.largura_max;
    b.estilo.tamanho = a.tamanho;
    b.estilo.negrito = a.negrito;
    b.estilo.alinhamento = a.alinhamento;
    b.estilo.cor = a.cor.to_string();
    b.estilo.caixa = a.caixa.clone();
    b.estilo.contorno = None;
}

/// O título no modelo, para o canvas da saída: cada bloco pelo arranjo do papel (conteúdo, id, ordem, fonte e itálico
/// ficam). Fundo: faixa presa a um clipe (`sobreposto`) fica sem fundo; o resto mantém o fundo de cor ou imagem, e o
/// que não tinha ganha `FUNDO_CHEIO`. `livre` só troca o rótulo.
pub fn aplicar_modelo(t: &Titulo, modelo: ModeloTitulo, sobreposto: bool, canvas: TamanhoPx) -> Titulo {
    let mut n = t.clone();
    n.modelo = modelo;
    if modelo == ModeloTitulo::Livre {
        return n;
    }
    n.fundo = match &t.fundo {
        _ if sobreposto && sobre_o_video(modelo) => Fundo::Transparente,
        Fundo::Transparente => Fundo::Cor { cor: FUNDO_CHEIO.into() },
        f => f.clone(),
    };
    let k = escala_do_canvas(canvas);
    for b in &mut n.blocos {
        arranjar(b, &na_escala(arranjo(modelo, b.papel), k));
    }
    n
}

/// Bloco novo do papel no modelo do título, para o canvas da saída. Do primeiro bloco vêm a fonte, o itálico, a
/// escala do "Tamanho" e a cor, quando o perito trocou a do modelo.
pub fn bloco_no_modelo(t: &Titulo, papel: PapelTexto, id: &str, conteudo: &str, canvas: TamanhoPx) -> BlocoTexto {
    let k = escala_do_canvas(canvas);
    let a = na_escala(arranjo(t.modelo, papel), k);
    let mut estilo = EstiloTexto {
        fonte: Fonte::Sans,
        negrito: a.negrito,
        italico: false,
        tamanho: a.tamanho,
        cor: a.cor.to_string(),
        alinhamento: a.alinhamento,
        caixa: a.caixa.clone(),
        contorno: None,
        sombra: None,
    };
    if let Some(r) = t.blocos.first() {
        let ar = na_escala(arranjo(t.modelo, r.papel), k);
        estilo.fonte = r.estilo.fonte;
        estilo.italico = r.estilo.italico;
        let k = r.estilo.tamanho / ar.tamanho;
        if k.is_finite() && k > 0.0 {
            estilo.tamanho = a.tamanho * k;
        }
        if r.estilo.cor != ar.cor {
            estilo.cor = r.estilo.cor.clone();
        }
    }
    BlocoTexto { id: id.to_string(), papel, conteudo: conteudo.to_string(), estilo, lugar: a.lugar, largura_max: a.largura_max }
}

/// Título novo do modelo: o fundo do modelo e um bloco de título vazio (que não desenha até ter texto).
pub fn titulo_no_modelo(modelo: ModeloTitulo, sobreposto: bool, id_bloco: &str, canvas: TamanhoPx) -> Titulo {
    let vazio = Titulo { modelo, fundo: Fundo::Transparente, blocos: Vec::new() };
    let b = bloco_no_modelo(&vazio, PapelTexto::Titulo, id_bloco, "", canvas);
    aplicar_modelo(&Titulo { blocos: vec![b], ..vazio }, modelo, sobreposto, canvas)
}

/// Caixa (0..1 do canvas) dentro da área segura, com folga de 1e-9.
pub fn na_area_segura(c: &Caixa) -> bool {
    const EPS: f64 = 1e-9;
    let (a, z) = (AREA_SEGURA - EPS, 1.0 - AREA_SEGURA + EPS);
    c.x >= a && c.y >= a && c.x + c.w <= z && c.y + c.h <= z
}

#[cfg(test)]
mod testes {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::edicao::camada::texto::{self, Atlas, Sprite, SpritesEmCache};
    use crate::edicao::camada::{self, Bitmap};
    use crate::edicao::cor;
    use crate::edicao::curva::fixtures;
    use crate::edicao::ffmpeg::args;
    use crate::edicao::ffmpeg::exec::{self, Comando};
    use crate::edicao::ffmpeg::sonda::Capacidades;
    use crate::edicao::geometria::Afim;
    use crate::edicao::importar_v1::{self, FontesV1};
    use crate::edicao::modelo::{ClipePrincipal, EstiloTexto, Faixa, Matriz, MidiaRef, ModeloTitulo, Tolerante};
    use crate::edicao::motor::plano::{Desenho, ImagemCamada, ItemCamada, PedidoCamada, QuadroItem, TipoCamada};
    use crate::edicao::testes::{aceite, executor, fontes, medir};

    const MODELOS: [&str; 6] = ["cartela", "abertura", "evento", "terco_inferior", "encerramento", "livre"];

    /// Fonte de sprites falsa e determinística; guarda pedidos e respostas (vão para a fixture: o espelho TS repete).
    struct Falso {
        imagens: BTreeMap<String, TamanhoPx>,
        sprites: RefCell<Vec<Value>>,
    }

    impl Falso {
        fn novo(imagens: &BTreeMap<String, TamanhoPx>) -> Self {
            Falso { imagens: imagens.clone(), sprites: RefCell::new(Vec::new()) }
        }
    }

    impl FonteSprites for Falso {
        fn sprite(&self, p: &PedidoSprite) -> Result<Sprite> {
            let conteudo: String = p.blocos.iter().map(|b| b.conteudo.as_str()).collect();
            let tam = p.blocos.first().map(|b| b.estilo.tamanho).unwrap_or(0.05) * p.altura_canvas as f64;
            let linhas = conteudo.split('\n').count() as u32;
            let mais_longa = conteudo.split('\n').map(|l| l.chars().count()).max().unwrap_or(0);
            let w = ((mais_longa as f64 * tam * 0.55).round() as u32).min((p.largura_max * p.altura_canvas as f64) as u32).max(1);
            let h = ((tam * 1.25).round() as u32 * linhas).max(1);
            let s = Sprite { relativo: format!("sprites/falso-{w}x{h}.png"), w, h };
            self.sprites.borrow_mut().push(json!({ "pedido": p, "sprite": s }));
            Ok(s)
        }
        fn atlas(&self, _: &EstiloTexto, _: u32) -> Result<Atlas> {
            Err(SicroError::Validation("titulo nao usa atlas".into()))
        }
        fn imagem(&self, midia: &str) -> Option<TamanhoPx> {
            self.imagens.get(midia).copied()
        }
    }

    fn quadro(canvas: TamanhoPx) -> QuadroCamada {
        QuadroCamada { q: 0, t: 0.0, pts: None, a: Afim::IDENTIDADE, canvas, rastreios: BTreeMap::new(), indice: None, programa_s: 0.0 }
    }

    fn estilo(cor: &str, tamanho: f64, negrito: bool, caixa: Option<&str>, contorno: Option<&str>) -> Value {
        json!({ "fonte": "sans", "negrito": negrito, "italico": false, "tamanho": tamanho, "cor": cor, "alinhamento": "centro",
                "caixa": caixa.map(|c| json!({ "cor": c, "margem": 0.012, "raio": 0.004 })),
                "contorno": contorno.map(|c| json!({ "cor": c, "largura": 0.002 })) })
    }

    fn bloco(id: &str, papel: &str, conteudo: &str, e: Value, lugar: (f64, f64, &str, &str), largura: f64) -> Value {
        json!({ "id": id, "papel": papel, "conteudo": conteudo, "estilo": e,
                "lugar": { "x": lugar.0, "y": lugar.1, "h": lugar.2, "v": lugar.3 }, "largura_max": largura })
    }

    fn titulo(modelo: &str, fundo: Value, blocos: Vec<Value>) -> Titulo {
        serde_json::from_value(json!({ "modelo": modelo, "fundo": fundo, "blocos": blocos })).expect("título")
    }

    fn cor_fundo(c: &str) -> Value {
        json!({ "tipo": "cor", "cor": c })
    }

    fn transparente() -> Value {
        json!({ "tipo": "transparente" })
    }

    fn fundo_imagem(m: &str) -> Value {
        json!({ "tipo": "imagem", "midia": m })
    }

    const HD: TamanhoPx = TamanhoPx { w: 1920, h: 1080 };

    /// Três blocos em cantos e alinhamentos diferentes.
    fn tres_blocos() -> Vec<Value> {
        vec![
            bloco("b1", "titulo", "Laudo 1234/2026", estilo("#ffffff", 0.078, true, None, None), (0.5, 0.42, "centro", "base"), 0.9),
            bloco("b2", "subtitulo", "Câmera 2 — saída", estilo("#e0e0e0", 0.052, false, Some("#0000008c"), None), (0.5, 0.45, "centro", "topo"), 0.8),
            bloco("b3", "rodape", "Instituto de Criminalística", estilo("#c0c0c0", 0.033, false, None, Some("#000000")), (0.95, 0.94, "dir", "base"), 0.5),
        ]
    }

    /// Casos da fixture: (nome, título, canvas, imagens conhecidas).
    fn casos() -> Vec<(String, Titulo, TamanhoPx, BTreeMap<String, TamanhoPx>)> {
        let sem = BTreeMap::new();
        let img = |pares: &[(&str, u32, u32)]| pares.iter().map(|(k, w, h)| (k.to_string(), TamanhoPx { w: *w, h: *h })).collect::<BTreeMap<_, _>>();
        let corpo = |c: &str| bloco("c1", "corpo", c, estilo("#fafafa", 0.033, false, None, None), (0.5, 0.5, "centro", "meio"), 0.9);
        let longe_em = |x: f64, y: f64| bloco("g", "corpo", "x", estilo("#ffffff", 0.05, false, None, None), (x, y, "esq", "topo"), 0.5);
        const K1: TamanhoPx = TamanhoPx { w: 1024, h: 576 };
        let mut v = vec![(
            "cartela da v1".to_string(),
            titulo("cartela", cor_fundo("#101010"), vec![corpo("Fim do trecho")]),
            HD,
            sem.clone(),
        )];
        for m in MODELOS {
            v.push((format!("modelo {m}"), titulo(m, cor_fundo("#000000c0"), tres_blocos()), HD, sem.clone()));
        }
        v.extend([
            ("transparente tres blocos 720p".into(), titulo("terco_inferior", transparente(), tres_blocos()), TamanhoPx { w: 1280, h: 720 }, sem.clone()),
            ("transparente sem blocos".into(), titulo("livre", transparente(), vec![]), HD, sem.clone()),
            ("cor sem blocos".into(), titulo("cartela", cor_fundo("#000000"), vec![]), HD, sem.clone()),
            (
                "bloco vazio no meio".into(),
                titulo("abertura", cor_fundo("#000000"), vec![corpo("Antes"), bloco("v", "corpo", "", estilo("#ffffff", 0.05, false, Some("#ff0000"), None), (0.5, 0.6, "centro", "meio"), 0.9), corpo("Depois")]),
                HD,
                sem.clone(),
            ),
            ("imagem 4:3 em 16:9".into(), titulo("abertura", fundo_imagem("fundo"), tres_blocos()), HD, img(&[("fundo", 400, 300)])),
            ("imagem 21:9 em 16:9".into(), titulo("abertura", fundo_imagem("fundo"), vec![]), HD, img(&[("fundo", 2520, 1080)])),
            ("imagem na proporcao do canvas".into(), titulo("encerramento", fundo_imagem("fundo"), vec![corpo("Fim")]), HD, img(&[("fundo", 3840, 2160)])),
            ("imagem em canvas vertical".into(), titulo("evento", fundo_imagem("fundo"), vec![corpo("Evento 1")]), TamanhoPx { w: 1080, h: 1920 }, img(&[("fundo", 400, 300)])),
            ("imagem ausente".into(), titulo("abertura", fundo_imagem("outra"), tres_blocos()), HD, img(&[("fundo", 400, 300)])),
            ("imagem de tamanho zero".into(), titulo("abertura", fundo_imagem("fundo"), tres_blocos()), HD, img(&[("fundo", 0, 300)])),
            (
                "bloco fora do canvas".into(),
                titulo("livre", transparente(), vec![
                    bloco("f1", "corpo", "Fora à direita", estilo("#ffffff", 0.05, false, None, None), (1.2, 0.5, "esq", "meio"), 0.5),
                    bloco("f2", "corpo", "Meio fora no alto", estilo("#ffffff", 0.05, false, None, None), (0.1, -0.02, "esq", "meio"), 0.5),
                    bloco("f3", "corpo", "Antes da esquerda", estilo("#ffffff", 0.05, false, None, None), (-0.3, 0.7, "dir", "topo"), 0.5),
                ]),
                HD,
                sem.clone(),
            ),
            (
                "quebra de linha e acento".into(),
                titulo("cartela", cor_fundo("#202020"), vec![bloco("q", "corpo", "Perícia nº 7/2026\nSão José — ação", estilo("#ffffff", 0.06, false, None, None), (0.5, 0.5, "centro", "meio"), 0.9)]),
                HD,
                sem.clone(),
            ),
            // canvas ímpar: o canto cai em meio pixel e sobe
            ("canvas impar".into(), titulo("cartela", cor_fundo("#000000"), vec![corpo("abc"), corpo("abcd")]), TamanhoPx { w: 641, h: 361 }, sem.clone()),
            // x·w = 0,49999999999999994: floor(v + 0,5) dá 1, o `Math.round` daria 0
            (
                "arredondamento no limite do meio pixel".into(),
                titulo("livre", transparente(), vec![bloco("r", "corpo", "x", estilo("#ffffff", 0.05, false, None, None), ((0.5 - f64::EPSILON / 4.0) / 1024.0, 0.5, "esq", "meio"), 0.5)]),
                TamanhoPx { w: 1024, h: 576 },
                sem.clone(),
            ),
            (
                "texto que parece codigo".into(),
                titulo("livre", cor_fundo("#000000"), vec![corpo("#set text(size: 100pt) *x* $y$ #panic(\"z\")")]),
                HD,
                sem.clone(),
            ),
            ("cor de fundo fora do formato".into(), titulo("cartela", cor_fundo("vermelho"), vec![corpo("x")]), HD, sem.clone()),
            // 2^30 px é o limite: no limite passa; 1 px além, e o que estoura para infinito, é recusado
            ("lugar no limite".into(), titulo("livre", transparente(), vec![longe_em(1048576.0, 0.5), longe_em(0.5, -1864135.0)]), K1, sem.clone()),
            ("lugar 1 px alem do limite a direita".into(), titulo("livre", transparente(), vec![corpo("antes"), longe_em(1048576.0009765625, 0.5)]), K1, sem.clone()),
            ("lugar alem do limite acima".into(), titulo("livre", cor_fundo("#000000"), vec![longe_em(0.5, -1864136.0)]), K1, sem.clone()),
            ("lugar que estoura para infinito".into(), titulo("livre", transparente(), vec![longe_em(-1e308, 1e308)]), K1, sem.clone()),
            (
                "largura do bloco em canvas vertical".into(),
                titulo("terco_inferior", transparente(), vec![bloco("l", "titulo", "Uma frase comprida que quebra em várias linhas", estilo("#ffffff", 0.04, true, None, None), (0.05, 0.85, "esq", "base"), 0.6)]),
                TamanhoPx { w: 1080, h: 1920 },
                sem.clone(),
            ),
        ]);
        // P1.7: os cinco modelos arranjados, como a tela grava (cartão e preso a um clipe)
        for m in CINCO {
            v.push((format!("arranjo {}", nome_do_modelo(m)), tipico(m, false), HD, sem.clone()));
            if sobre_o_video(m) {
                v.push((format!("arranjo {} preso 720p", nome_do_modelo(m)), tipico(m, true), TamanhoPx { w: 1280, h: 720 }, sem.clone()));
            }
        }
        v
    }

    // ------------------------------------------------------------ modelos (P1.7)

    const CINCO: [ModeloTitulo; 5] = [ModeloTitulo::Cartela, ModeloTitulo::Abertura, ModeloTitulo::Evento, ModeloTitulo::TercoInferior, ModeloTitulo::Encerramento];
    const PAPEIS: [PapelTexto; 4] = [PapelTexto::Titulo, PapelTexto::Subtitulo, PapelTexto::Corpo, PapelTexto::Rodape];

    fn nome_do_modelo(m: ModeloTitulo) -> String {
        serde_json::to_value(m).unwrap().as_str().unwrap().to_string()
    }

    /// Textos de um título de verdade, por papel.
    const TEXTOS: [&str; 4] = ["Laudo nº 1234/2026", "Câmera 2 — Rua das Flores", "Gravação de 14/03/2026, 12:03:15", "Instituto de Criminalística"];
    /// Os mesmos, compridos: o título quebra em duas linhas, o corpo tem duas.
    const LONGOS: [&str; 4] = [
        "Laudo pericial nº 1234/2026 — análise de imagens",
        "Câmera 2 — Rua das Flores, esquina com a Avenida Central",
        "Gravação de 14/03/2026, 12:03:15\nVeículo prata entra no cruzamento",
        "Instituto de Criminalística — Seção de Perícias em Audiovisual",
    ];

    /// Título como a tela o monta: o modelo escolhido, depois os quatro papéis digitados em ordem.
    fn com_textos(m: ModeloTitulo, sobreposto: bool, textos: &[&str; 4], canvas: TamanhoPx) -> Titulo {
        let mut t = titulo_no_modelo(m, sobreposto, "b0", canvas);
        t.blocos[0].conteudo = textos[0].into();
        for (i, papel) in PAPEIS.iter().enumerate().skip(1) {
            let b = bloco_no_modelo(&t, *papel, &format!("b{i}"), textos[i], canvas);
            t.blocos.push(b);
        }
        t
    }

    fn tipico(m: ModeloTitulo, sobreposto: bool) -> Titulo {
        com_textos(m, sobreposto, &TEXTOS, HD)
    }

    /// Título de partida com estilo do perito (serifa, itálico, amarelo, tamanho dobrado) e os quatro papéis fora de ordem.
    fn do_perito(fundo: Value) -> Titulo {
        let e = |tam: f64| estilo("#ffd84a", tam, false, Some("#00000080"), Some("#000000"));
        let mut t = titulo("livre", fundo, vec![
            bloco("r", "rodape", "Instituto", e(0.066), (0.1, 0.1, "esq", "topo"), 0.3),
            bloco("t", "titulo", "Laudo", e(0.156), (0.2, 0.2, "dir", "meio"), 0.4),
            bloco("c", "corpo", "Linha 1\nLinha 2", e(0.084), (0.3, 0.3, "centro", "base"), 0.5),
            bloco("s", "subtitulo", "Câmera", e(0.104), (0.4, 0.4, "esq", "topo"), 0.6),
        ]);
        for b in &mut t.blocos {
            b.estilo.fonte = Fonte::Serifa;
            b.estilo.italico = true;
        }
        t
    }

    #[test]
    fn fixture_titulos_modelos() {
        let mut lista = Vec::new();
        let todos = [CINCO.as_slice(), &[ModeloTitulo::Livre]].concat();
        for m in &todos {
            for p in PAPEIS {
                let a = arranjo(*m, p);
                lista.push(json!({ "nome": format!("arranjo {} {}", nome_do_modelo(*m), serde_json::to_value(p).unwrap().as_str().unwrap()),
                    "entrada": { "conta": "arranjo", "modelo": m, "papel": p }, "saida": a }));
            }
        }
        let fundos = [("cor", cor_fundo("#1b2a41")), ("transparente", transparente()), ("imagem", fundo_imagem("fundo"))];
        // 16:9, 4:3 (escala 1), vertical e um quase quadrado (escala < 1) e o canvas sem altura (escala 1)
        let canvases = [HD, TamanhoPx { w: 640, h: 480 }, TamanhoPx { w: 1080, h: 1920 }, TamanhoPx { w: 1001, h: 1080 }, TamanhoPx { w: 0, h: 0 }];
        for c in canvases {
            lista.push(json!({ "nome": format!("escala {}x{}", c.w, c.h), "entrada": { "conta": "escala", "canvas": c }, "saida": escala_do_canvas(c) }));
        }
        for m in &todos {
            for sob in [false, true] {
                for (nf, f) in &fundos {
                    let t = do_perito(f.clone());
                    lista.push(json!({ "nome": format!("aplicar {} fundo {nf} preso {sob}", nome_do_modelo(*m)),
                        "entrada": { "conta": "aplicar", "titulo": t, "modelo": m, "sobreposto": sob, "canvas": HD }, "saida": aplicar_modelo(&t, *m, sob, HD) }));
                }
                for c in &canvases[1..] {
                    let t = do_perito(cor_fundo("#1b2a41"));
                    lista.push(json!({ "nome": format!("aplicar {} {}x{} preso {sob}", nome_do_modelo(*m), c.w, c.h),
                        "entrada": { "conta": "aplicar", "titulo": t, "modelo": m, "sobreposto": sob, "canvas": c }, "saida": aplicar_modelo(&t, *m, sob, *c) }));
                }
                for c in [HD, canvases[2]] {
                    lista.push(json!({ "nome": format!("novo {} {}x{} preso {sob}", nome_do_modelo(*m), c.w, c.h),
                        "entrada": { "conta": "novo", "modelo": m, "sobreposto": sob, "id": "n1", "canvas": c }, "saida": titulo_no_modelo(*m, sob, "n1", c) }));
                }
            }
        }
        // livre copia o bloco como está, −0 incluído
        let mut neg = do_perito(transparente());
        neg.blocos[0].lugar.x = -0.0;
        lista.push(json!({ "nome": "aplicar livre com -0", "entrada": { "conta": "aplicar", "titulo": neg, "modelo": ModeloTitulo::Livre, "sobreposto": false, "canvas": HD },
            "saida": aplicar_modelo(&neg, ModeloTitulo::Livre, false, HD) }));
        // bloco novo: sem bloco, com o primeiro no padrão do modelo, e com o primeiro trocado pelo perito
        for m in &todos {
            let mut bases = vec![("vazio", Titulo { modelo: *m, fundo: Fundo::Transparente, blocos: vec![] }), ("padrao", titulo_no_modelo(*m, false, "p", HD))];
            let mut trocado = aplicar_modelo(&do_perito(transparente()), *m, false, HD);
            trocado.modelo = *m;
            for b in &mut trocado.blocos {
                b.estilo.tamanho *= 0.5;
                b.estilo.cor = "#00ff00".into();
            }
            bases.push(("trocado", trocado));
            // tamanho zero no primeiro: a escala não vale
            let mut zero = titulo_no_modelo(*m, false, "z", HD);
            zero.blocos[0].estilo.tamanho = 0.0;
            bases.push(("tamanho zero", zero));
            // no vertical, o padrão é o do vertical (escala igual: o tamanho fica)
            bases.push(("padrao vertical", titulo_no_modelo(*m, false, "v", canvases[2])));
            for (nb, t) in &bases {
                for p in PAPEIS {
                    let c = if *nb == "padrao vertical" { canvases[2] } else { HD };
                    lista.push(json!({ "nome": format!("bloco {} {nb} {}", nome_do_modelo(*m), serde_json::to_value(p).unwrap().as_str().unwrap()),
                        "entrada": { "conta": "bloco", "titulo": t, "papel": p, "id": "novo", "conteudo": "Texto", "canvas": c }, "saida": bloco_no_modelo(t, p, "novo", "Texto", c) }));
                }
            }
        }
        // área segura: no limite, um fio além, e o que não é finito
        let a = AREA_SEGURA;
        for (n, c) in [
            ("dentro", [0.1, 0.1, 0.5, 0.5]),
            ("no limite", [a, a, 1.0 - 2.0 * a, 1.0 - 2.0 * a]),
            ("à esquerda", [a - 1e-6, 0.5, 0.1, 0.1]),
            ("em cima", [0.5, a - 1e-6, 0.1, 0.1]),
            ("à direita", [0.9, 0.5, 0.08 + 1e-6, 0.1]),
            ("embaixo", [0.5, 0.9, 0.1, 0.08 + 1e-6]),
            ("nan", [f64::NAN, 0.5, 0.1, 0.1]),
        ] {
            let c = Caixa { x: c[0], y: c[1], w: c[2], h: c[3] };
            let entrada = if c.x.is_nan() { json!({ "conta": "area", "caixa": { "x": "nan", "y": c.y, "w": c.w, "h": c.h } }) } else { json!({ "conta": "area", "caixa": c }) };
            lista.push(json!({ "nome": format!("área segura {n}"), "entrada": entrada, "saida": na_area_segura(&c) }));
        }
        fixtures::conferir("titulos-modelos.json", Value::Array(lista));
    }

    #[test]
    fn cinco_modelos_arranjo() {
        // cada modelo é um arranjo diferente; aplicar duas vezes = uma; conteúdo, id, ordem, fonte e itálico ficam
        let base = do_perito(cor_fundo("#1b2a41"));
        let arranjados: Vec<Titulo> = CINCO.iter().map(|m| aplicar_modelo(&base, *m, false, HD)).collect();
        for (i, a) in arranjados.iter().enumerate() {
            assert_eq!(a.modelo, CINCO[i]);
            assert_eq!(aplicar_modelo(a, CINCO[i], false, HD), *a, "{:?} idempotente", CINCO[i]);
            for (b, o) in a.blocos.iter().zip(&base.blocos) {
                assert_eq!((&b.id, b.papel, &b.conteudo, b.estilo.fonte, b.estilo.italico), (&o.id, o.papel, &o.conteudo, o.estilo.fonte, o.estilo.italico));
                let r = arranjo(CINCO[i], b.papel);
                assert_eq!((b.lugar, b.largura_max, b.estilo.tamanho, b.estilo.negrito, b.estilo.alinhamento), (r.lugar, r.largura_max, r.tamanho, r.negrito, r.alinhamento));
                assert_eq!((b.estilo.cor.as_str(), &b.estilo.caixa, &b.estilo.contorno), (r.cor, &r.caixa, &None));
            }
            for b in &arranjados[..i] {
                assert_ne!(a.blocos, b.blocos, "{:?} × {:?}", a.modelo, b.modelo);
            }
            // em todo modelo o título é o maior e o rodapé o menor
            let tam: Vec<f64> = PAPEIS.iter().map(|p| arranjo(CINCO[i], *p).tamanho).collect();
            assert!(tam.windows(2).all(|w| w[0] > w[1]), "{:?}: {tam:?}", CINCO[i]);
        }
        // fundo: cartão mantém cor e imagem; faixa presa a um clipe fica sem fundo; sem fundo ganha o cheio
        for m in CINCO {
            for (f, sob) in [(cor_fundo("#1b2a41"), false), (fundo_imagem("x"), false), (transparente(), false), (cor_fundo("#1b2a41"), true), (transparente(), true)] {
                let t = aplicar_modelo(&do_perito(f.clone()), m, sob, HD);
                let esperado: Fundo = if sob && sobre_o_video(m) {
                    Fundo::Transparente
                } else if f == transparente() {
                    Fundo::Cor { cor: FUNDO_CHEIO.into() }
                } else {
                    serde_json::from_value(f).unwrap()
                };
                assert_eq!(t.fundo, esperado, "{m:?} preso {sob}");
            }
        }
        // livre não mexe em nada além do rótulo
        let l = aplicar_modelo(&base, ModeloTitulo::Livre, true, HD);
        assert_eq!((l.modelo, &l.fundo, &l.blocos), (ModeloTitulo::Livre, &base.fundo, &base.blocos));
        // bloco novo: escala e cor do primeiro bloco quando o perito trocou
        let mut t = titulo_no_modelo(ModeloTitulo::Abertura, false, "a", HD);
        t.blocos[0].estilo.tamanho = 0.039;
        t.blocos[0].estilo.cor = "#ff0000".into();
        t.blocos[0].estilo.fonte = Fonte::Mono;
        let r = bloco_no_modelo(&t, PapelTexto::Rodape, "r", "x", HD);
        assert_eq!((r.estilo.tamanho, r.estilo.cor.as_str(), r.estilo.fonte), (0.03 * 0.5, "#ff0000", Fonte::Mono));
        assert_eq!(r.lugar, arranjo(ModeloTitulo::Abertura, PapelTexto::Rodape).lugar);
        // sem troca, a cor é a do modelo (rodapé cinza na abertura)
        let r = bloco_no_modelo(&titulo_no_modelo(ModeloTitulo::Abertura, false, "a", HD), PapelTexto::Rodape, "r", "x", HD);
        assert_eq!(r.estilo.cor, CINZA);
        // vertical: tamanho e margem da faixa na escala da largura; o que o perito fez no tamanho continua valendo
        let v = TamanhoPx { w: 1080, h: 1920 };
        let k = escala_do_canvas(v);
        assert_eq!(k, (1080.0 * 3.0) / (1920.0 * 4.0));
        let t = titulo_no_modelo(ModeloTitulo::TercoInferior, true, "a", v);
        assert_eq!((t.blocos[0].estilo.tamanho, t.blocos[0].estilo.caixa.as_ref().unwrap().margem), (0.052 * k, 0.013 * k));
        assert_eq!(bloco_no_modelo(&t, PapelTexto::Subtitulo, "s", "x", v).estilo.tamanho, 0.033 * k * ((0.052 * k) / (0.052 * k)));
        assert_eq!(escala_do_canvas(TamanhoPx { w: 640, h: 480 }), 1.0);
        assert_eq!(escala_do_canvas(TamanhoPx { w: 0, h: 0 }), 1.0);
    }

    /// Código do erro de validação (antes do ":").
    fn codigo_do_erro(e: &SicroError) -> String {
        match e {
            SicroError::Validation(m) => m.split(':').next().unwrap_or_default().trim().to_string(),
            outro => panic!("erro inesperado: {outro}"),
        }
    }

    fn rodar_caso(t: &Titulo, canvas: TamanhoPx, imagens: &BTreeMap<String, TamanhoPx>) -> (Value, Value) {
        let f = Falso::novo(imagens);
        let q = quadro(canvas);
        let mut saida = match primitivas(t, &q, &f) {
            Ok(p) => json!({ "primitivas": p }),
            Err(e) => json!({ "erro": codigo_do_erro(&e) }),
        };
        // um pedido por bloco e um fora da lista
        saida["pedidos"] = json!((0..=t.blocos.len()).map(|i| pedido(t, i, canvas)).collect::<Vec<_>>());
        (json!({ "titulo": t, "quadro": q, "imagens": imagens, "sprites": f.sprites.take() }), saida)
    }

    #[test]
    fn fixture_primitivas_titulo() {
        let lista: Vec<Value> = casos()
            .into_iter()
            .map(|(nome, t, canvas, imagens)| {
                let (entrada, saida) = rodar_caso(&t, canvas, &imagens);
                json!({ "nome": nome, "entrada": entrada, "saida": saida })
            })
            .collect();
        fixtures::conferir("primitivas-titulo.json", Value::Array(lista));
    }

    fn do_caso(nome: &str) -> Vec<Primitiva> {
        let (_, t, canvas, imagens) = casos().into_iter().find(|c| c.0 == nome).unwrap();
        primitivas(&t, &quadro(canvas), &Falso::novo(&imagens)).unwrap()
    }

    fn caixa_px(p: &Primitiva, canvas: TamanhoPx) -> [f64; 4] {
        let Primitiva::Sprite { caixa: c, .. } = p else { panic!("{p:?}") };
        [c.x * canvas.w as f64, c.y * canvas.h as f64, c.w * canvas.w as f64, c.h * canvas.h as f64]
    }

    #[test]
    fn seis_modelos_sem_panico_e_o_lugar_do_bloco_vale() {
        let base = do_caso("modelo cartela");
        assert_eq!(base.len(), 4);
        for m in MODELOS {
            assert_eq!(do_caso(&format!("modelo {m}")), base, "{m}");
        }
        // o modelo é lido do texto
        for m in MODELOS {
            let t = titulo(m, transparente(), vec![]);
            assert_eq!(serde_json::to_value(t.modelo).unwrap(), json!(m));
        }
        // fundo de cor: o canvas inteiro, por baixo dos blocos; blocos na ordem
        let Primitiva::Caminho { d, preenche, traco: None, regra: None } = &base[0] else { panic!() };
        assert_eq!(preenche.as_deref(), Some("#000000c0"));
        assert_eq!(d, &[Segmento::M(0.0, 0.0), Segmento::L(1.0, 0.0), Segmento::L(1.0, 1.0), Segmento::L(0.0, 1.0), Segmento::Z]);
        // b1 centro/base em (0,5; 0,42); b3 dir/base em (0,95; 0,94): o ponto do lugar a meio px
        let [x, y, w, h] = caixa_px(&base[1], HD);
        assert!((x + w * 0.5 - 960.0).abs() <= 0.5 && (y + h - 0.42 * 1080.0).abs() <= 0.5, "{x} {y} {w} {h}");
        let [x, y, w, h] = caixa_px(&base[3], HD);
        assert!((x + w - 0.95 * 1920.0).abs() <= 0.5 && (y + h - 0.94 * 1080.0).abs() <= 0.5);
        // canto em px inteiro
        for p in &base[1..] {
            let [x, y, ..] = caixa_px(p, HD);
            assert!((x.round() - x).abs() < 1e-9 && (y.round() - y).abs() < 1e-9, "{x} {y}");
        }
    }

    #[test]
    fn fundo_de_imagem_cobre_sem_distorcer_e_vazios_nao_desenham() {
        for (nome, canvas, img) in [
            ("imagem 4:3 em 16:9", HD, (400.0, 300.0)),
            ("imagem 21:9 em 16:9", HD, (2520.0, 1080.0)),
            ("imagem na proporcao do canvas", HD, (3840.0, 2160.0)),
            ("imagem em canvas vertical", TamanhoPx { w: 1080, h: 1920 }, (400.0, 300.0)),
        ] {
            let p = do_caso(nome);
            let Primitiva::Sprite { src, opacidade, .. } = &p[0] else { panic!() };
            assert_eq!((src.as_str(), *opacidade), ("midia:fundo", 1.0));
            let [x, y, w, h] = caixa_px(&p[0], canvas);
            let (cw, ch) = (canvas.w as f64, canvas.h as f64);
            // mesma escala nos dois eixos, cobre o canvas e fica centrada
            assert!((w / img.0 - h / img.1).abs() < 1e-9, "{nome}: {w}×{h}");
            assert!(x <= 1e-9 && y <= 1e-9 && x + w >= cw - 1e-9 && y + h >= ch - 1e-9, "{nome}");
            assert!((x + w * 0.5 - cw * 0.5).abs() < 1e-9 && (y + h * 0.5 - ch * 0.5).abs() < 1e-9, "{nome}");
            assert!((w - cw).abs() < 1e-9 || (h - ch).abs() < 1e-9, "{nome}: um lado exato");
        }
        let f = Falso::novo(&BTreeMap::new());
        let erro = |t: &Titulo| codigo_do_erro(&primitivas(t, &quadro(HD), &f).unwrap_err());
        assert_eq!(erro(&titulo("abertura", fundo_imagem("nada"), vec![])), codigo::MIDIA_AUSENTE);
        // vazios
        assert!(do_caso("transparente sem blocos").is_empty());
        assert_eq!(do_caso("cor sem blocos").len(), 1);
        let p = do_caso("bloco vazio no meio");
        assert_eq!(p.len(), 3, "o bloco vazio não desenha nem a caixa de fundo");
    }

    #[test]
    fn pedido_em_fracao_da_altura() {
        let t = titulo("cartela", transparente(), tres_blocos());
        let p = pedido(&t, 1, HD);
        assert_eq!(p.blocos, vec![t.blocos[1].clone()]);
        assert_eq!((p.largura_max, p.altura_canvas, p.fundo), (0.8 * 1920.0 / 1080.0, 1080, true));
        let v = pedido(&t, 1, TamanhoPx { w: 1080, h: 1920 });
        assert_eq!(v.largura_max, 0.8 * 1080.0 / 1920.0);
        // fora da lista: sem bloco, largura 0
        for i in [3, usize::MAX] {
            assert_eq!(pedido(&t, i, HD), PedidoSprite { blocos: vec![], largura_max: 0.0, altura_canvas: 1080, fundo: true });
        }
        let vazio = titulo("livre", transparente(), vec![]);
        assert_eq!(pedido(&vazio, 0, HD).blocos.len(), 0);
        // canvas sem altura: não derruba
        let _ = pedido(&t, 0, TamanhoPx { w: 0, h: 0 });
        // com o rasterizador de texto de verdade: a largura do sprite respeita a do bloco
        let cache = fontes::pasta_temporaria("p05t-pedido");
        let longo = "o veículo entra no cruzamento e reduz antes da faixa de pedestres ".repeat(3);
        for canvas in [HD, TamanhoPx { w: 1080, h: 1920 }] {
            let t = titulo("livre", transparente(), vec![bloco("l", "corpo", &longo, estilo("#ffffff", 0.04, false, None, None), (0.5, 0.5, "centro", "meio"), 0.6)]);
            let s = texto::sprite(&cache, &pedido(&t, 0, canvas)).unwrap();
            assert!(s.w as f64 <= 0.6 * canvas.w as f64 + 1.0 && s.w as f64 > 0.5 * canvas.w as f64, "{canvas:?}: {}", s.w);
        }
        let erro = texto::sprite(&cache, &pedido(&t, 7, HD)).unwrap_err();
        assert_eq!(codigo_do_erro(&erro), codigo::ITEM_INVALIDO);
        let _ = std::fs::remove_dir_all(cache);
    }

    // ------------------------------------------------------------ rasterizado de verdade

    fn quadros(canvas: TamanhoPx, n: u32) -> Vec<QuadroItem> {
        (0..n)
            .map(|q| {
                let mut k = quadro(canvas);
                k.q = q as u64;
                k.t = q as f64 / 30.0;
                k.programa_s = k.t;
                QuadroItem { quadro: k, transformacao: None }
            })
            .collect()
    }

    /// Camada de anotação do canvas inteiro com o título em `n` quadros.
    fn pedido_camada(t: &Titulo, canvas: TamanhoPx, n: u32, imagens: Vec<ImagemCamada>) -> PedidoCamada {
        let item = ItemCamada { desenho: Desenho::Titulo { titulo: t.clone() }, de: 0, quadros: quadros(canvas, n) };
        let mut p = PedidoCamada {
            id: "c000_titulo".into(),
            tipo: TipoCamada::Anotacao,
            caixa_px: [0, 0, canvas.w, canvas.h],
            n,
            hash: String::new(),
            itens: vec![item],
            imagens,
        };
        p.hash = camada::hash_do_pedido(&p);
        p
    }

    fn png_da_camada(c: &camada::CamadaPronta) -> image::RgbaImage {
        image::open(c.pasta.join(&c.arquivos[0].arquivo)).expect("png da camada").to_rgba8()
    }

    /// Todos os arquivos (caminho relativo → bytes) de uma pasta, recursivo.
    fn arquivos(raiz: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut v = BTreeMap::new();
        let mut pilha = vec![raiz.to_path_buf()];
        while let Some(d) = pilha.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    pilha.push(p);
                } else {
                    v.insert(p.strip_prefix(raiz).unwrap().to_path_buf(), std::fs::read(&p).unwrap());
                }
            }
        }
        v
    }

    /// Imagem `w`×`h` azul com um quadrado vermelho no meio (lado = um quarto do menor lado), gravada em `pasta`.
    fn imagem_quadrado(pasta: &Path, midia: &str, w: u32, h: u32) -> (ImagemCamada, f64) {
        let lado = w.min(h) / 4;
        let (x0, y0) = ((w - lado) / 2, (h - lado) / 2);
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let dentro = (x0..x0 + lado).contains(&x) && (y0..y0 + lado).contains(&y);
                px.extend_from_slice(if dentro { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
            }
        }
        let png = Bitmap { w, h, canais: 4, px }.png().unwrap();
        let caminho = pasta.join(format!("{midia}.png"));
        std::fs::write(&caminho, &png).unwrap();
        let sha256 = Sha256::digest(&png).iter().map(|b| format!("{b:02x}")).collect();
        (ImagemCamada { midia: midia.into(), sha256, caminho, tamanho: TamanhoPx { w, h } }, lado as f64)
    }

    struct SemBanco;
    impl FontesV1 for SemBanco {
        fn video(&self, _: &str) -> Option<MidiaRef> {
            None
        }
        fn narracao(&self, _: &str) -> Option<MidiaRef> {
            None
        }
        fn pts(&self, _: &str, _: f64) -> Option<i64> {
            None
        }
        fn tamanho(&self, _: &str) -> Option<(u32, u32)> {
            None
        }
    }

    /// Centro (px, centro do pixel = +0,5) ponderado por `peso` em cada pixel.
    fn centro_ponderado(img: &image::RgbaImage, peso: impl Fn(&image::Rgba<u8>) -> f64) -> (f64, f64) {
        let (mut sx, mut sy, mut s) = (0.0, 0.0, 0.0);
        for (x, y, p) in img.enumerate_pixels() {
            let k = peso(p);
            sx += k * (x as f64 + 0.5);
            sy += k * (y as f64 + 0.5);
            s += k;
        }
        assert!(s > 0.0, "nada desenhado");
        (sx / s, sy / s)
    }

    #[test]
    fn cartela_da_v1_rasteriza_com_o_texto_no_centro() {
        let nao = AtomicBool::new(false);
        // toda cartela de toda fixture `v1-*.json`
        let mut v1s: Vec<PathBuf> = std::fs::read_dir(fixtures::caminho(""))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("v1-") && n.ends_with(".json")))
            .collect();
        v1s.sort();
        let (mut com_texto, mut sem_texto) = (Vec::new(), Vec::new());
        for caminho in &v1s {
            let arquivo = caminho.file_name().unwrap().to_string_lossy().into_owned();
            let v1: Value = serde_json::from_str(&std::fs::read_to_string(caminho).unwrap()).unwrap();
            let p = importar_v1::converter(&v1, &SemBanco, "2026-11-14T12:00:00.000Z", "teste").unwrap_or_else(|e| panic!("{arquivo}: {e}"));
            let canvas = TamanhoPx { w: p.saida.largura, h: p.saida.altura };
            for c in &p.principal {
                let Tolerante::Conhecido(ClipePrincipal::Titulo(k)) = c else { continue };
                let id = format!("{arquivo} {}", k.base.id);
                assert_eq!(k.titulo.modelo, ModeloTitulo::Cartela, "{id}");
                let Fundo::Cor { cor: fundo } = &k.titulo.fundo else { panic!("{id}") };
                let fundo = cor::ler(fundo).unwrap();
                let cache = fontes::pasta_temporaria("p05t-cartela");
                let c = camada::rasterizar(&pedido_camada(&k.titulo, canvas, 3, Vec::new()), &cache, &nao).unwrap();
                assert!(c.fixa && c.arquivos.len() == 1, "{id}");
                let png = png_da_camada(&c);
                assert_eq!(png.dimensions(), (canvas.w, canvas.h), "{id}");
                // cartela sem texto (a v1 guardava só espaços): o fundo, opaco, no canvas inteiro
                if k.titulo.blocos.is_empty() {
                    assert!(png.pixels().all(|p| p.0 == [fundo.r, fundo.g, fundo.b, 255]), "{id}: só o fundo");
                    sem_texto.push(id);
                    let _ = std::fs::remove_dir_all(cache);
                    continue;
                }
                assert_eq!(k.titulo.blocos.len(), 1, "{id}");
                let letra = cor::ler(&k.titulo.blocos[0].estilo.cor).unwrap();
                // o sprite que o título pediu, lido do cache
                let s = texto::sprite(&cache, &pedido(&k.titulo, 0, canvas)).unwrap();
                let sprite = image::open(cache.join(&s.relativo)).unwrap().to_rgba8();
                // tinta na camada: quanto o pixel andou do fundo para a cor do texto (o canal de maior contraste)
                let canal = (0..3).max_by_key(|&i| [letra.r, letra.g, letra.b][i].abs_diff([fundo.r, fundo.g, fundo.b][i])).unwrap();
                let (f, l) = ([fundo.r, fundo.g, fundo.b][canal] as f64, [letra.r, letra.g, letra.b][canal] as f64);
                let tinta = |p: &image::Rgba<u8>| ((p[canal] as f64 - f) / (l - f)).clamp(0.0, 1.0);
                let (lx, ly) = centro_ponderado(&png, tinta);
                let (sx, sy) = centro_ponderado(&sprite, |p| p[3] as f64 / 255.0);
                // onde o sprite caiu na camada (medido): a caixa do texto centrada no canvas a até 1 px
                let (cx, cy) = (lx - sx + s.w as f64 * 0.5, ly - sy + s.h as f64 * 0.5);
                let (mx, my) = (canvas.w as f64 * 0.5, canvas.h as f64 * 0.5);
                eprintln!("{id}: caixa do texto com centro em ({cx:.3}, {cy:.3}), tinta em ({lx:.3}, {ly:.3}); canvas ({mx}, {my})");
                assert!((cx - mx).abs() <= 1.0 && (cy - my).abs() <= 1.0, "{id}: centro ({cx}, {cy}) × ({mx}, {my})");
                // e a tinta não foi esticada nem cortada: a mesma soma do sprite
                let soma_camada: f64 = png.pixels().map(tinta).sum();
                let soma_sprite: f64 = sprite.pixels().map(|p| p[3] as f64 / 255.0).sum();
                assert!((soma_camada - soma_sprite).abs() <= soma_sprite * 0.02 + 2.0, "{id}: {soma_camada} × {soma_sprite}");
                // fora da caixa do texto, só o fundo, opaco
                let (x0, y0) = ((cx - s.w as f64 * 0.5).round() as u32, (cy - s.h as f64 * 0.5).round() as u32);
                let fora = png
                    .enumerate_pixels()
                    .filter(|(x, y, _)| !((x0..x0 + s.w).contains(x) && (y0..y0 + s.h).contains(y)))
                    .filter(|(_, _, p)| p.0 != [fundo.r, fundo.g, fundo.b, 255])
                    .count();
                assert_eq!(fora, 0, "{id}: pixel fora da caixa do texto diferente do fundo");
                com_texto.push(id);
                let _ = std::fs::remove_dir_all(cache);
            }
        }
        // as três cartelas das fixtures de hoje; fixture nova com cartela entra sozinha
        for id in ["v1-completo.json f2", "v1-velocidade.json a2"] {
            assert!(com_texto.iter().any(|c| c == id), "{id} em {com_texto:?}");
        }
        assert!(sem_texto.iter().any(|c| c == "v1-completo.json f6"), "{sem_texto:?}");
    }

    #[test]
    fn fundo_de_imagem_sem_distorcao_na_camada() {
        let nao = AtomicBool::new(false);
        let pasta = fontes::pasta_temporaria("p05t-imagem");
        for (iw, ih, canvas) in [
            (160, 120, TamanhoPx { w: 640, h: 360 }),
            (120, 160, TamanhoPx { w: 640, h: 360 }),
            (160, 120, TamanhoPx { w: 360, h: 640 }),
            // muito reduzida: passa pela redução por blocos do rasterizador
            (1600, 1200, TamanhoPx { w: 640, h: 360 }),
            (640, 360, TamanhoPx { w: 640, h: 360 }),
        ] {
            let (img, lado) = imagem_quadrado(&pasta, &format!("q{iw}x{ih}"), iw, ih);
            let t = titulo("abertura", fundo_imagem(&img.midia), vec![]);
            let c = camada::rasterizar(&pedido_camada(&t, canvas, 1, vec![img]), &pasta, &nao).unwrap();
            let png = png_da_camada(&c);
            // cobre o canvas inteiro
            assert!(png.pixels().all(|p| p[3] == 255), "{iw}×{ih} em {canvas:?}: pixel sem fundo");
            // o quadrado continua quadrado, no tamanho da escala e no meio
            let vermelho: Vec<(u32, u32)> = png.enumerate_pixels().filter(|(_, _, p)| p[0] > 128 && p[2] < 128).map(|(x, y, _)| (x, y)).collect();
            let (x0, x1) = (vermelho.iter().map(|p| p.0).min().unwrap(), vermelho.iter().map(|p| p.0).max().unwrap());
            let (y0, y1) = (vermelho.iter().map(|p| p.1).min().unwrap(), vermelho.iter().map(|p| p.1).max().unwrap());
            let (w, h) = ((x1 - x0 + 1) as f64, (y1 - y0 + 1) as f64);
            let e = (canvas.w as f64 / iw as f64).max(canvas.h as f64 / ih as f64);
            eprintln!("{iw}×{ih} em {}×{}: quadrado {w}×{h} (esperado {:.2})", canvas.w, canvas.h, lado * e);
            assert!((w - h).abs() <= 1.0, "{iw}×{ih} em {canvas:?}: distorcido {w}×{h}");
            assert!((w - lado * e).abs() <= 1.5 && (h - lado * e).abs() <= 1.5, "{iw}×{ih}: lado {w}×{h} × {}", lado * e);
            let (cx, cy) = ((x0 + x1 + 1) as f64 * 0.5, (y0 + y1 + 1) as f64 * 0.5);
            assert!((cx - canvas.w as f64 * 0.5).abs() <= 1.0 && (cy - canvas.h as f64 * 0.5).abs() <= 1.0, "{cx} {cy}");
        }
        let _ = std::fs::remove_dir_all(pasta);
    }

    #[test]
    fn vazios_texto_de_20000_e_bloco_fora_do_canvas_nao_derrubam() {
        let nao = AtomicBool::new(false);
        let canvas = TamanhoPx { w: 640, h: 360 };
        let cache = fontes::pasta_temporaria("p05t-vazios");
        let sprites = SpritesEmCache { cache: &cache, imagens: &[] };
        let e = || estilo("#ffffff", 0.05, false, Some("#00000080"), Some("#000000"));
        let centro = (0.5, 0.5, "centro", "meio");
        // lista vazia, bloco vazio, só espaço: nada derruba; com cor, só o fundo
        for (t, n_prims) in [
            (titulo("livre", transparente(), vec![]), 0),
            (titulo("cartela", cor_fundo("#123456"), vec![]), 1),
            (titulo("abertura", transparente(), vec![bloco("v", "corpo", "", e(), centro, 0.9)]), 0),
            (titulo("abertura", cor_fundo("#000000"), vec![bloco("v", "corpo", "", e(), centro, 0.9), bloco("s", "corpo", "  \n ", e(), centro, 0.9)]), 2),
        ] {
            let p = primitivas(&t, &quadro(canvas), &sprites).unwrap();
            assert_eq!(p.len(), n_prims, "{t:?}");
            let c = camada::rasterizar(&pedido_camada(&t, canvas, 2, Vec::new()), &cache, &nao).unwrap();
            assert!(c.fixa);
        }
        // o pedido do bloco vazio também serve ao rasterizador de texto
        let t = titulo("abertura", transparente(), vec![bloco("v", "corpo", "", e(), centro, 0.9)]);
        texto::sprite(&cache, &pedido(&t, 0, canvas)).unwrap();
        // 20 000 caracteres: sprite de verdade, maior que o canvas (cortado na camada)
        let limite = "palavra ".repeat(2500);
        assert_eq!(limite.chars().count(), 20_000);
        let t = titulo("livre", transparente(), vec![bloco("l", "corpo", &limite, estilo("#ffffff", 0.02, false, None, None), centro, 0.9)]);
        let inicio = std::time::Instant::now();
        let p = primitivas(&t, &quadro(canvas), &sprites).unwrap();
        eprintln!("20 000 caracteres: {:?}, {:?}", inicio.elapsed(), caixa_px(&p[0], canvas));
        let [_, y, _, h] = caixa_px(&p[0], canvas);
        assert!(y < 0.0 && h > canvas.h as f64, "o texto longo passa do canvas");
        let c = camada::rasterizar(&pedido_camada(&t, canvas, 1, Vec::new()), &cache, &nao).unwrap();
        assert!(png_da_camada(&c).pixels().any(|p| p[3] == 255));
        // um a mais: recusado com código, sem pânico
        let demais = format!("{limite}a");
        let t = titulo("livre", transparente(), vec![bloco("l", "corpo", &demais, estilo("#ffffff", 0.02, false, None, None), centro, 0.9)]);
        assert_eq!(codigo_do_erro(&primitivas(&t, &quadro(canvas), &sprites).unwrap_err()), codigo::ITEM_INVALIDO);
        let e = camada::rasterizar(&pedido_camada(&t, canvas, 1, Vec::new()), &cache, &nao).unwrap_err();
        assert_eq!(codigo_do_erro(&e), codigo::ITEM_INVALIDO);
        // bloco todo fora e bloco cortado pela borda, num título de 1 quadro
        let t = titulo("livre", transparente(), vec![
            bloco("f", "corpo", "Fora", estilo("#ff0000", 0.08, true, None, None), (1.2, 0.5, "esq", "meio"), 0.5),
            bloco("m", "corpo", "Meio fora", estilo("#00ff00", 0.08, true, None, None), (0.0, 0.0, "centro", "meio"), 0.5),
        ]);
        let c = camada::rasterizar(&pedido_camada(&t, canvas, 1, Vec::new()), &cache, &nao).unwrap();
        assert!(c.fixa && c.lista.is_none() && c.arquivos.len() == 1);
        let png = png_da_camada(&c);
        assert!(png.pixels().all(|p| p[3] == 0 || p[0] == 0), "nada do bloco de fora aparece");
        let verde = png.enumerate_pixels().filter(|(_, _, p)| p[3] == 255 && p[1] == 255).map(|(x, y, _)| (x, y)).collect::<Vec<_>>();
        assert!(!verde.is_empty() && verde.iter().all(|&(x, y)| x < 160 && y < 40), "o bloco cortado aparece só no canto");
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn bloco_longe_demais_e_recusado_sem_panico() {
        let nao = AtomicBool::new(false);
        let canvas = TamanhoPx { w: 640, h: 360 };
        let cache = fontes::pasta_temporaria("p05t-longe");
        let em = |x: f64, y: f64| titulo("livre", transparente(), vec![bloco("f", "corpo", "Fora", estilo("#ff0000", 0.08, true, None, None), (x, y, "esq", "base"), 0.5)]);
        // recorte da camada inteiro e deslocado: a cópia do sprite desconta o canto do recorte
        let recortes = [[0, 0, 640, 360], [100, 100, 200, 200]];
        let rasterizar = |t: &Titulo, caixa_px: [u32; 4]| {
            let mut p = pedido_camada(t, canvas, 1, Vec::new());
            p.caixa_px = caixa_px;
            p.hash = camada::hash_do_pedido(&p);
            camada::rasterizar(&p, &cache, &nao)
        };
        let (lx, ly) = (MAX_LUGAR_PX / canvas.w as f64, MAX_LUGAR_PX / canvas.h as f64);
        let longe = [(4e6, 0.5), (-3.5e6, 0.5), (0.5, 1e7), (0.5, -1e7), (1e300, 0.5), (-1e300, 0.5), (0.5, -1e300), (f64::MAX, f64::MAX), (lx + 1.0, 0.5), (-lx - 1.0, 0.5), (0.5, ly + 1.0)];
        for (x, y) in longe {
            for caixa_px in recortes {
                assert_eq!(codigo_do_erro(&rasterizar(&em(x, y), caixa_px).unwrap_err()), codigo::ITEM_INVALIDO, "({x}, {y})");
            }
        }
        // no limite, e longe mas dentro dele: desenha (nada aparece)
        for (x, y) in [(lx, 0.5), (-lx, 0.5), (0.5, ly), (1e5, -1e5), (-1e5, 1e5)] {
            for caixa_px in recortes {
                let c = rasterizar(&em(x, y), caixa_px).unwrap();
                assert!(png_da_camada(&c).pixels().all(|p| p[3] == 0), "({x}, {y})");
            }
        }
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn mesmos_bytes_em_duas_execucoes() {
        let nao = AtomicBool::new(false);
        let canvas = TamanhoPx { w: 640, h: 360 };
        let img = fontes::pasta_temporaria("p05t-bytes-img");
        let (imagem, _) = imagem_quadrado(&img, "fundo", 200, 120);
        let t = titulo("abertura", fundo_imagem("fundo"), vec![
            bloco("a", "titulo", "Perícia nº 7/2026\nSão José — ação", estilo("#ffcc00", 0.07, true, Some("#0000008c"), Some("#000000")), (0.5, 0.45, "centro", "base"), 0.9),
            bloco("b", "rodape", "Câmera 2", estilo("#ffffff", 0.04, false, None, None), (0.97, 0.95, "dir", "base"), 0.4),
        ]);
        let rodar = |cache: &Path| {
            let c = camada::rasterizar(&pedido_camada(&t, canvas, 2, vec![imagem.clone()]), cache, &nao).unwrap();
            (c.arquivos.clone(), arquivos(cache))
        };
        let (c1, c2) = (fontes::pasta_temporaria("p05t-bytes-a"), fontes::pasta_temporaria("p05t-bytes-b"));
        let (a1, f1) = rodar(&c1);
        typst::comemo::evict(0);
        let (a2, f2) = rodar(&c2);
        assert_eq!(a1, a2);
        assert_eq!(f1, f2);
        assert!(f1.keys().any(|k| k.starts_with("sprites")) && f1.keys().any(|k| k.starts_with("camadas")));
        for p in [c1, c2, img] {
            let _ = std::fs::remove_dir_all(p);
        }
    }

    // ------------------------------------------------------------ com ffmpeg

    fn ffmpeg(cap: &Capacidades, pasta: &Path, nome: &str, entradas: &[String], grafo: &str) -> Vec<u8> {
        args::gravar_texto(pasta, &format!("{nome}.txt"), grafo).unwrap();
        let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-v", "error"].map(String::from).into();
        a.extend_from_slice(entradas);
        a.extend(args::grafo_em_arquivo(cap, &format!("{nome}.txt")));
        a.extend(["-map", "[v]", "-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"].map(String::from));
        let cmd = Comando {
            nome: nome.into(),
            programa: cap.binario.clone(),
            args: a,
            pasta: pasta.to_path_buf(),
            ambiente: Vec::new(),
            sem_ambiente: Vec::new(),
            stderr: pasta.join(format!("{nome}.stderr")),
        };
        let (t, out) = exec::rodar_com_saida(&cmd, &AtomicBool::new(false)).expect("ffmpeg roda");
        assert_eq!(t.codigo, Some(0), "{nome} ({}): {:?}", cap.versao, exec::ultimas_linhas(&cmd.stderr, 8));
        out
    }

    #[test]
    fn ffmpeg_titulo_transparente_sobre_video() {
        let Some(cap) = fontes::cap() else { return };
        eprintln!("ffmpeg {} ({})", cap.versao, cap.binario.display());
        let (w, h) = (640u32, 360u32);
        let canvas = TamanhoPx { w, h };
        let cores = [("#ff0000", [255u8, 0, 0]), ("#00ff00", [0, 255, 0]), ("#0000ff", [0, 0, 255])];
        let t = titulo("terco_inferior", transparente(), vec![
            bloco("b1", "titulo", "Câmera 2", estilo(cores[0].0, 0.1, true, None, None), (0.05, 0.6, "esq", "base"), 0.9),
            bloco("b2", "subtitulo", "Saída 12h", estilo(cores[1].0, 0.16, true, Some("#000000a0"), None), (0.05, 0.62, "esq", "topo"), 0.9),
            bloco("b3", "rodape", "Laudo nº 123", estilo(cores[2].0, 0.1, true, None, None), (0.95, 0.3, "dir", "base"), 0.9),
        ]);
        let n = 6u32;
        let pasta = fontes::pasta_temporaria("p05t-video");
        let cache = pasta.join("cachê do caso");
        let c = camada::rasterizar(&pedido_camada(&t, canvas, n, Vec::new()), &cache, &AtomicBool::new(false)).unwrap();
        assert!(c.fixa, "título que não muda: um PNG só");
        let png = c.pasta.join(&c.arquivos[0].arquivo);
        let camada = image::open(&png).unwrap().to_rgba8();
        let caixas: Vec<[f64; 4]> = primitivas(&t, &quadro(canvas), &SpritesEmCache { cache: &cache, imagens: &[] })
            .unwrap()
            .iter()
            .map(|p| caixa_px(p, canvas))
            .collect();
        assert_eq!(caixas.len(), 3);
        let video: Vec<String> = ["-f", "lavfi", "-i", &format!("testsrc2=s={w}x{h}:r=30:d=1")].map(String::from).into();
        let base = format!("[0:v]trim=end_frame={n},format=yuv420p,setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv");
        let sem = ffmpeg(&cap, &pasta, "sem", &video, &format!("{base}[v]"));
        let mut entradas = video.clone();
        entradas.extend(["-loop", "1", "-framerate", "30", "-t", "1", "-i"].map(String::from));
        entradas.push(png.to_string_lossy().into_owned());
        let g = format!("{base}[b];[1:v]trim=end_frame={n},{}[an];[b][an]overlay=0:0:format=yuv420:eof_action=pass[v]", cor::filtro_camada());
        let com = ffmpeg(&cap, &pasta, "com", &entradas, &g);
        let (ny, nc) = ((w * h) as usize, (w * h / 4) as usize);
        let tam = ny + 2 * nc;
        assert_eq!((sem.len(), com.len()), (tam * n as usize, tam * n as usize));
        let alfa = |x: u32, y: u32| camada.get_pixel(x, y)[3];
        let dentro = |x: u32, y: u32| caixas.iter().any(|c| (x as f64) >= c[0] && (x as f64) < c[0] + c[2] && (y as f64) >= c[1] && (y as f64) < c[1] + c[3]);
        let bloco_croma = |cx: u32, cy: u32| [(2 * cx, 2 * cy), (2 * cx + 1, 2 * cy), (2 * cx, 2 * cy + 1), (2 * cx + 1, 2 * cy + 1)];
        let (mut fora, mut mudou_fora, mut mudou_dentro) = (0u64, 0u64, 0u64);
        let mut pior = [0.0f64; 3];
        let mut medidos = [0u64; 3];
        for k in 0..n as usize {
            let (s, m) = (&sem[k * tam..(k + 1) * tam], &com[k * tam..(k + 1) * tam]);
            // luma: alfa 0 ou fora das caixas dos blocos = o pixel do vídeo, sem erro
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) as usize;
                    if alfa(x, y) == 0 || !dentro(x, y) {
                        fora += 1;
                        mudou_fora += (s[i] != m[i]) as u64;
                    } else {
                        mudou_dentro += (s[i] != m[i]) as u64;
                    }
                }
            }
            for cy in 0..h / 2 {
                for cx in 0..w / 2 {
                    let i = (cy * w / 2 + cx) as usize;
                    let px = bloco_croma(cx, cy);
                    let (u, v) = (ny + i, ny + nc + i);
                    if px.iter().all(|&(x, y)| alfa(x, y) == 0 || !dentro(x, y)) {
                        fora += 2;
                        mudou_fora += (s[u] != m[u]) as u64 + (s[v] != m[v]) as u64;
                    }
                    // cor do texto: bloco 2×2 opaco na cor de um bloco, e o alcance do filtro de croma da camada
                    // (3,5 px além do bloco) todo nessa cor; perto da caixa de fundo o 4:2:0 mistura as duas
                    for (j, (_, rgb)) in cores.iter().enumerate() {
                        let alcance = ((2 * cx).saturating_sub(3)..(2 * cx + 5).min(w))
                            .flat_map(|x| ((2 * cy).saturating_sub(3)..(2 * cy + 5).min(h)).map(move |y| (x, y)));
                        let cheio = px.iter().all(|&(x, y)| camada.get_pixel(x, y)[3] == 255);
                        if cheio && alcance.into_iter().all(|(x, y)| camada.get_pixel(x, y).0[..3] == rgb[..]) {
                            for &(x, y) in &px {
                                let c = cor::rgb((m[(y * w + x) as usize], m[u], m[v]), Matriz::Bt709, Faixa::Tv);
                                for (p, (a, b)) in pior.iter_mut().zip([c.r, c.g, c.b].iter().zip(rgb)) {
                                    *p = p.max(a.abs_diff(*b) as f64);
                                }
                                medidos[j] += 1;
                            }
                        }
                    }
                }
            }
        }
        eprintln!("{}: {fora} amostras fora dos blocos, {mudou_fora} mudaram; {mudou_dentro} mudaram dentro; erro de cor {pior:?} em {medidos:?} px", cap.versao);
        assert_eq!(mudou_fora, 0, "{} ({}): o vídeo mudou fora dos blocos", cap.versao, cap.binario.display());
        assert!(fora as f64 > 0.8 * (tam * n as usize) as f64, "quase tudo é fora dos blocos");
        assert!(mudou_dentro > 1000, "o título não apareceu");
        assert!(medidos.iter().all(|&m| m >= 100), "poucos pixels de cor cheia: {medidos:?}");
        assert!(pior.iter().all(|&e| e <= 2.0), "{} ({}): erro de cor {pior:?}", cap.versao, cap.binario.display());
        let _ = std::fs::remove_dir_all(pasta);
    }

    // ------------------------------------------------------------ modelos rasterizados (P1.7)

    /// Camada do título rasterizada de verdade e a caixa (px) de cada bloco com texto, na ordem dos blocos.
    fn rasterizado(t: &Titulo, canvas: TamanhoPx, cache: &Path) -> (image::RgbaImage, Vec<[f64; 4]>) {
        let c = camada::rasterizar(&pedido_camada(t, canvas, 1, Vec::new()), cache, &AtomicBool::new(false)).unwrap();
        (png_da_camada(&c), caixas_dos_blocos(t, canvas, cache))
    }

    fn caixas_dos_blocos(t: &Titulo, canvas: TamanhoPx, cache: &Path) -> Vec<[f64; 4]> {
        let prims = primitivas(t, &quadro(canvas), &SpritesEmCache { cache, imagens: &[] }).unwrap();
        prims.iter().filter(|p| matches!(p, Primitiva::Sprite { src, .. } if !src.starts_with("midia:"))).map(|p| caixa_px(p, canvas)).collect()
    }

    fn em(c: &[f64; 4], x: u32, y: u32) -> bool {
        let (fx, fy) = (x as f64, y as f64);
        fx >= c[0] && fx < c[0] + c[2] && fy >= c[1] && fy < c[1] + c[3]
    }

    fn sobrepoem(a: &[f64; 4], b: &[f64; 4]) -> bool {
        a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3]
    }

    /// Caixa [x0, y0, x1, y1] (px; x1 e y1 inclusivos) dos pixels de `img` dentro de `c` que passam em `f`.
    fn caixa_de(img: &image::RgbaImage, c: &[f64; 4], f: impl Fn(&image::Rgba<u8>) -> bool) -> Option<[u32; 4]> {
        let mut r: Option<[u32; 4]> = None;
        for (x, y, p) in img.enumerate_pixels() {
            if em(c, x, y) && f(p) {
                let a = r.get_or_insert([x, y, x, y]);
                *a = [a[0].min(x), a[1].min(y), a[2].max(x), a[3].max(y)];
            }
        }
        r
    }

    fn rgb_de(cor: &str) -> [u8; 3] {
        let c = cor::ler(cor).unwrap();
        [c.r, c.g, c.b]
    }

    /// Os cinco modelos com texto de verdade (Typst) em 1920×1080: blocos sem sobreposição e na área segura; fundo
    /// do cartão opaco e o da faixa presa vazio; a cor de cada papel nos pixels; o alinhamento e a altura de cada
    /// modelo medidos na tinta; os cinco diferentes. Depois, pelo motor e pelo ffmpeg, cada modelo no vídeo.
    #[test]
    fn cinco_modelos() {
        let cache = fontes::pasta_temporaria("p17-cinco");
        let mut camadas: Vec<image::RgbaImage> = Vec::new();
        for m in CINCO {
            let preso = sobre_o_video(m);
            let t = tipico(m, preso);
            let (png, caixas) = rasterizado(&t, HD, &cache);
            let (cw, ch) = (HD.w as f64, HD.h as f64);
            assert_eq!(caixas.len(), 4, "{m:?}");
            for (i, a) in caixas.iter().enumerate() {
                assert!(na_area_segura(&Caixa { x: a[0] / cw, y: a[1] / ch, w: a[2] / cw, h: a[3] / ch }), "{m:?} bloco {i} fora da área segura: {a:?}");
                for b in &caixas[..i] {
                    assert!(!sobrepoem(a, b), "{m:?}: blocos sobrepostos {a:?} × {b:?}");
                }
            }
            // fundo: fora dos blocos, o fundo do cartão (opaco) ou nada (faixa presa)
            let fundo = if preso { [0, 0, 0, 0] } else { [0, 0, 0, 255] };
            let fora = png.enumerate_pixels().filter(|(x, y, p)| !caixas.iter().any(|c| em(c, *x, *y)) && p.0 != fundo).count();
            assert_eq!(fora, 0, "{m:?}: pixel fora dos blocos diferente do fundo");
            let mut alturas = Vec::new();
            for (i, (b, c)) in t.blocos.iter().zip(&caixas).enumerate() {
                let a = arranjo(m, b.papel);
                let cor = rgb_de(a.cor);
                // tinta = pixel opaco da cor do papel (o miolo das letras sai exato em alfa reto)
                let n_cor = png.enumerate_pixels().filter(|(x, y, p)| em(c, *x, *y) && p[3] == 255 && p.0[..3] == cor).count();
                assert!(n_cor > 200, "{m:?} bloco {i}: {n_cor} px da cor {}", a.cor);
                // nenhum pixel opaco de cor que não seja a do papel misturada ao fundo (cinza de borda incluído)
                let estranho = png.enumerate_pixels().filter(|(x, y, p)| em(c, *x, *y) && p[3] == 255 && !(p[0] == p[1] && p[1] == p[2])).count();
                assert_eq!(estranho, 0, "{m:?} bloco {i}: pixel com matiz");
                let tinta = caixa_de(&png, c, |p| p[3] == 255 && p[0] > 100).unwrap();
                // faixa: a caixa inteira coberta pela faixa (borda da caixa = cor da faixa, alfa reto)
                if let Some(f) = &a.caixa {
                    let cf = cor::ler(&f.cor).unwrap();
                    for (x, y) in [(c[0] + 1.0, c[1] + 1.0), (c[0] + c[2] - 2.0, c[1] + c[3] - 2.0)] {
                        assert_eq!(png.get_pixel(x.round() as u32, y.round() as u32).0, [cf.r, cf.g, cf.b, cf.a], "{m:?} bloco {i}: faixa");
                    }
                }
                // alinhamento medido: centrado pelo meio da tinta, à esquerda e à direita pela borda da caixa
                let (x0, x1) = (tinta[0] as f64, tinta[2] as f64 + 1.0);
                let lx = a.lugar.x * cw;
                // a margem da faixa fica entre a borda da caixa e a letra
                let folga = a.caixa.as_ref().map_or(0.0, |f| f.margem * ch) + 0.012 * cw;
                match a.lugar.h {
                    Alinhamento::Centro => assert!(((x0 + x1) * 0.5 - lx).abs() <= 0.006 * cw, "{m:?} bloco {i}: meio da tinta {} × {lx}", (x0 + x1) * 0.5),
                    Alinhamento::Esq => {
                        assert!((c[0] - lx).abs() <= 0.5, "{m:?} bloco {i}: caixa em {} × {lx}", c[0]);
                        assert!(x0 >= lx && x0 - lx <= folga, "{m:?} bloco {i}: tinta começa em {x0} × {lx}");
                    }
                    Alinhamento::Dir => {
                        assert!((c[0] + c[2] - lx).abs() <= 0.5, "{m:?} bloco {i}: caixa termina em {} × {lx}", c[0] + c[2]);
                        assert!(x1 <= lx && lx - x1 <= folga, "{m:?} bloco {i}: tinta termina em {x1} × {lx}");
                    }
                }
                // vertical: o ponto de apoio na borda da caixa (topo ou base), a tinta dentro dela
                let ly = a.lugar.y * ch;
                let borda = if a.lugar.v == AlinhamentoV::Topo { c[1] } else { c[1] + c[3] };
                assert!((borda - ly).abs() <= 0.5, "{m:?} bloco {i}: {borda} × {ly}");
                alturas.push(c[3]);
            }
            // tamanho: título maior, rodapé menor (uma linha cada)
            assert!(alturas.windows(2).all(|w| w[0] > w[1]), "{m:?}: alturas {alturas:?}");
            for (j, o) in camadas.iter().enumerate() {
                assert!(o != &png, "{m:?} igual ao modelo {:?}", CINCO[j]);
            }
            camadas.push(png);
        }
        // assinatura de cada modelo, na tinta
        let tinta_do = |m: ModeloTitulo, i: usize| {
            let t = tipico(m, sobre_o_video(m));
            let (png, caixas) = rasterizado(&t, HD, &cache);
            caixa_de(&png, &caixas[i], |p| p[3] == 255 && p[0] > 100).unwrap()
        };
        // evento: título no décimo de cima; terço inferior: título, subtítulo e corpo entre 60 % e 95 % da altura
        assert!(tinta_do(ModeloTitulo::Evento, 0)[3] < 1080 / 7);
        for i in 0..3 {
            let k = tinta_do(ModeloTitulo::TercoInferior, i);
            assert!(k[1] > 1080 * 6 / 10 && k[3] < 1080 * 95 / 100 && k[2] < 1920 * 77 / 100, "terço inferior bloco {i}: {k:?}");
        }
        // abertura: a mesma margem esquerda nos quatro blocos (até 1 % da largura de diferença: o lado das letras)
        let esq: Vec<u32> = (0..4).map(|i| tinta_do(ModeloTitulo::Abertura, i)[0]).collect();
        assert!(esq.iter().max().unwrap() - esq.iter().min().unwrap() <= 19, "abertura: {esq:?}");

        // ---- no vídeo: os cinco como cartões da principal, pelo compilador e pelo ffmpeg
        let Some(cap) = fontes::cap() else {
            let _ = std::fs::remove_dir_all(cache);
            return;
        };
        let canvas = TamanhoPx { w: 640, h: 360 };
        let principal: Vec<Value> = CINCO
            .iter()
            .enumerate()
            .map(|(k, m)| json!({ "tipo": "titulo", "id": format!("t{k}"), "rotulo": "", "duracao_s": 0.1, "titulo": tipico(*m, false) }))
            .collect();
        let p = aceite::projeto(canvas.w, canvas.h, (30, 1), vec![], principal, json!({}));
        let quadros = renderizar(&cap, &p, &aceite::Mundo::novo(&cap), "p17-cinco-video");
        assert_eq!(quadros.len(), 15);
        for (k, m) in CINCO.iter().enumerate() {
            let (esperado, _) = rasterizado(&tipico(*m, false), canvas, &cache);
            let q = &quadros[3 * k + 1];
            let (mut soma, mut longe) = (0u64, 0u64);
            for (x, y, e) in esperado.enumerate_pixels() {
                let i = ((y * canvas.w + x) * 3) as usize;
                let d = (0..3).map(|c| q.px[i + c].abs_diff(e[c])).max().unwrap();
                soma += d as u64;
                longe += u64::from(d > 24);
            }
            let n = (canvas.w * canvas.h) as f64;
            eprintln!("{m:?} no vídeo ({}): erro médio {:.3}, {longe} px acima de 24", cap.versao, soma as f64 / n);
            // medido: erro médio 0,002 e nenhum pixel acima de 24 nos dois binários
            assert!(soma as f64 / n <= 0.25 && (longe as f64) <= 0.0005 * n, "{m:?} ({}): erro médio {}, {longe} px longe", cap.versao, soma as f64 / n);
        }
        let _ = std::fs::remove_dir_all(cache);
    }

    /// Área segura (2 %): em 16:9 (1080p e 720p), 4:3 e vertical, com o texto de sempre e com o comprido (duas linhas),
    /// todo bloco dos cinco modelos cabe nela, e com o de sempre nenhum encosta no outro; na camada de 1280×720, nenhum
    /// pixel de texto fora dela.
    #[test]
    fn area_segura() {
        let cache = fontes::pasta_temporaria("p17-area");
        let mut falhas = Vec::new();
        for m in CINCO {
            for canvas in [HD, TamanhoPx { w: 1280, h: 720 }, TamanhoPx { w: 640, h: 480 }, TamanhoPx { w: 1080, h: 1920 }] {
                for (nt, textos) in [("típico", &TEXTOS), ("longo", &LONGOS)] {
                    let t = com_textos(m, sobre_o_video(m), textos, canvas);
                    let caixas = caixas_dos_blocos(&t, canvas, &cache);
                    let (cw, ch) = (canvas.w as f64, canvas.h as f64);
                    for (i, a) in caixas.iter().enumerate() {
                        if !na_area_segura(&Caixa { x: a[0] / cw, y: a[1] / ch, w: a[2] / cw, h: a[3] / ch }) {
                            falhas.push(format!("{m:?} {}×{} {nt}: bloco {i} fora {a:?}", canvas.w, canvas.h));
                        }
                        // o comprido pode encostar no vizinho (cada papel tem um ponto de apoio fixo): a prévia mostra
                        for b in caixas[..i].iter().filter(|_| nt == "típico") {
                            if sobrepoem(a, b) {
                                falhas.push(format!("{m:?} {}×{} {nt}: blocos sobrepostos {a:?} × {b:?}", canvas.w, canvas.h));
                            }
                        }
                    }
                }
            }
            // pixel: a camada sem fundo, texto longo
            let canvas = TamanhoPx { w: 1280, h: 720 };
            let t = Titulo { fundo: Fundo::Transparente, ..com_textos(m, sobre_o_video(m), &LONGOS, canvas) };
            let (png, _) = rasterizado(&t, canvas, &cache);
            let (a0, a1) = ((AREA_SEGURA * 1280.0) as u32, (AREA_SEGURA * 720.0) as u32);
            let fora = png.enumerate_pixels().filter(|(x, y, p)| p[3] > 0 && (*x < a0 || *y < a1 || *x >= 1280 - a0 || *y >= 720 - a1)).count();
            assert_eq!(fora, 0, "{m:?}: pixel fora da área segura");
            assert!(png.pixels().filter(|p| p[3] > 0).count() > 5000, "{m:?}: nada desenhado");
        }
        assert!(falhas.is_empty(), "{}", falhas.join("\n"));
        // a validação (o aviso da tela) também não acusa os cinco, como cartão e como faixa presa
        let cap = crate::edicao::quadros::apoio::cap_falsa();
        let fontes = std::collections::HashMap::new();
        let ctx = crate::edicao::motor::Contexto {
            fontes: &fontes,
            cap: &cap,
            rastreios: &crate::edicao::quadros::apoio::SemRastreios,
            pasta_projeto: Path::new("/nao/existe/projeto"),
            midias: &[],
        };
        for canvas in [HD, TamanhoPx { w: 1280, h: 720 }, TamanhoPx { w: 640, h: 480 }, TamanhoPx { w: 1080, h: 1920 }] {
            let (mut principal, mut presos) = (Vec::new(), Vec::new());
            for (k, m) in CINCO.iter().enumerate() {
                principal.push(json!({ "tipo": "titulo", "id": format!("t{k}"), "rotulo": "", "duracao_s": 1.0, "titulo": com_textos(*m, false, &TEXTOS, canvas) }));
                presos.push(sob_titulo(&format!("s{k}"), json!({ "ref": "clipe", "clipe": format!("t{k}"), "desloc": 0.0 }), None, &com_textos(*m, true, &TEXTOS, canvas)));
            }
            let p = aceite::projeto(canvas.w, canvas.h, (30, 1), vec![], principal, json!({ "sobreposicoes": [{ "id": "s", "nome": "", "oculta": false, "clipes": presos }] }));
            let a = crate::edicao::validar::analisar(&p, &ctx, None);
            let ruins: Vec<String> = a.problemas.iter().filter(|x| [codigo::TEXTO_FORA, codigo::ITEM_INVALIDO].contains(&x.codigo)).map(|x| format!("{} {:?}", x.codigo, x.clipe)).collect();
            assert!(ruins.is_empty(), "{}×{}: {ruins:?}", canvas.w, canvas.h);
        }
        // e as contas acusam o bloco que sai dela
        let mut t = tipico(ModeloTitulo::TercoInferior, true);
        t.blocos[0].lugar.x = 0.01;
        let c = caixas_dos_blocos(&t, HD, &cache)[0];
        assert!(!na_area_segura(&Caixa { x: c[0] / 1920.0, y: c[1] / 1080.0, w: c[2] / 1920.0, h: c[3] / 1080.0 }));
        let p = aceite::projeto(1920, 1080, (30, 1), vec![], vec![json!({ "tipo": "titulo", "id": "t", "rotulo": "", "duracao_s": 1.0, "titulo": t })], json!({}));
        assert!(crate::edicao::validar::analisar(&p, &ctx, None).problemas.iter().any(|x| x.codigo == codigo::TEXTO_FORA));
        let _ = std::fs::remove_dir_all(cache);
    }

    fn sha256_hex(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|v| format!("{v:02x}")).collect()
    }

    /// O pedido como a prévia o manda (fio JSON do front, `edicao_sprite_texto`) dá o MESMO arquivo, com o mesmo
    /// SHA-256, que o render pede ao rasterizar o plano do motor; e o sprite entra na camada sem reamostrar.
    #[test]
    fn sprite_da_previa_igual_ao_do_render() {
        let canvas = TamanhoPx { w: 1280, h: 720 };
        // os cinco na principal (os de faixa com fundo transparente) e as faixas presas ao clipe
        let mut principal = Vec::new();
        let mut presos = Vec::new();
        let mut titulos = Vec::new();
        for (k, m) in CINCO.iter().enumerate() {
            let t = Titulo { fundo: Fundo::Transparente, ..tipico(*m, true) };
            principal.push(json!({ "tipo": "titulo", "id": format!("t{k}"), "rotulo": "", "duracao_s": 0.2, "titulo": t }));
            titulos.push(t);
            if sobre_o_video(*m) {
                let mut s = tipico(*m, true);
                s.blocos.iter_mut().for_each(|b| b.conteudo = format!("{} (preso)", b.conteudo));
                presos.push(json!({ "tipo": "titulo", "id": format!("s{k}"), "rotulo": "", "pos": { "ref": "clipe", "clipe": format!("t{k}"), "desloc": 0.0 },
                    "duracao_s": null, "transformacao": { "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [], "titulo": s }));
                titulos.push(s);
            }
        }
        let p = aceite::projeto(canvas.w, canvas.h, (30, 1), vec![], principal, json!({ "sobreposicoes": [{ "id": "s", "nome": "", "oculta": false, "clipes": presos }] }));
        let cap = std::sync::Arc::new(crate::edicao::quadros::apoio::cap_falsa());
        let plano = aceite::Mundo::novo(&cap).compilar(&p, &aceite::alvo_final("cpu")).unwrap();
        let render = fontes::pasta_temporaria("p17-render");
        let prontas = camada::rasterizar_todos(&plano.camadas, &render, &AtomicBool::new(false), &mut |_| {}).unwrap();
        let do_render: BTreeMap<String, String> = arquivos(&render.join("sprites")).into_iter().map(|(k, b)| (format!("sprites/{}", k.display()), sha256_hex(&b))).collect();
        // prévia: um pedido por bloco, pelo fio
        let previa = fontes::pasta_temporaria("p17-previa");
        let mut da_previa = BTreeMap::new();
        for t in &titulos {
            for i in 0..t.blocos.len() {
                let fio = serde_json::to_string(&pedido(t, i, canvas)).unwrap();
                let s = texto::sprite(&previa, &serde_json::from_str::<PedidoSprite>(&fio).unwrap()).unwrap();
                da_previa.insert(s.relativo.clone(), sha256_hex(&std::fs::read(previa.join(&s.relativo)).unwrap()));
            }
        }
        // blocos de sprite igual (o título da cartela e o do encerramento: só o lugar muda) dão o mesmo arquivo
        assert!(da_previa.len() >= 3 * titulos.len(), "{} sprites", da_previa.len());
        assert_eq!(da_previa, do_render, "prévia × render");
        // na camada de cada cartão (sem transformação): os pixels do sprite, alfa exato
        let mut conferidas = 0;
        for (pedido_c, pronta) in plano.camadas.iter().zip(&prontas) {
            let Some(ItemCamada { desenho: Desenho::Titulo { titulo: t }, quadros: q, .. }) = pedido_c.itens.first() else { continue };
            if q[0].transformacao.is_some() {
                continue;
            }
            let png = png_da_camada(pronta);
            let [ox, oy, ..] = pedido_c.caixa_px;
            for (i, c) in caixas_dos_blocos(t, canvas, &previa).iter().enumerate() {
                let s = texto::sprite(&previa, &pedido(t, i, canvas)).unwrap();
                let sprite = image::open(previa.join(&s.relativo)).unwrap().to_rgba8();
                for (x, y, e) in sprite.enumerate_pixels() {
                    // a caixa vem em 0..1: o canto volta a px com resto de arredondamento
                    let a = png.get_pixel(c[0].round() as u32 + x - ox, c[1].round() as u32 + y - oy);
                    assert_eq!(a[3], e[3], "{:?} bloco {i} ({x}, {y})", t.modelo);
                    let tol = if e[3] == 255 || e[3] == 0 { 0 } else { 255u8.div_ceil(e[3]) };
                    assert!(e[3] == 0 || (0..3).all(|k| a[k].abs_diff(e[k]) <= tol), "{:?} bloco {i} ({x}, {y}): {a:?} × {e:?}", t.modelo);
                }
            }
            conferidas += 1;
        }
        assert_eq!(conferidas, CINCO.len(), "uma camada por cartão");
        for d in [render, previa] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    // ------------------------------------------------------------ presos e marca fixa, no vídeo (P1.7)

    /// Compila (alvo final), roda sem perdas e devolve os quadros, um por slot.
    fn renderizar(cap: &std::sync::Arc<Capacidades>, p: &crate::edicao::modelo::ProjetoEdicao, mundo: &aceite::Mundo, nome: &str) -> Vec<medir::QuadroRgb> {
        let plano = mundo.compilar(p, &aceite::alvo_final("cpu")).unwrap_or_else(|e| panic!("{nome}: {e}"));
        let pasta = fontes::pasta_temporaria(nome);
        let op = executor::Opcoes { sem_perdas: true, ..executor::Opcoes::default() };
        let r = executor::rodar(cap, &plano, &pasta, &op).unwrap_or_else(|e| panic!("{nome} ({}): {e}", cap.versao));
        let mut q = Vec::new();
        for b in &r.blocos {
            q.extend(medir::quadros(cap, b).unwrap_or_else(|e| panic!("{nome}: {e}")));
        }
        assert_eq!(q.len() as u64, plano.total_q, "{nome}: quadros × slots");
        let _ = std::fs::remove_dir_all(pasta);
        q
    }

    fn px3(q: &medir::QuadroRgb, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * q.w + x) * 3) as usize;
        [q.px[i], q.px[i + 1], q.px[i + 2]]
    }

    /// Quantos pixels da região passam em `f`. A medida é pela cor dominante: o RGB lido do vídeo cru sem etiqueta
    /// sai por BT.601 (o #c00000 volta como (175, 0, 2)).
    fn quantos(q: &medir::QuadroRgb, r: [u32; 4], f: fn([u8; 3]) -> bool) -> u32 {
        let mut n = 0;
        for y in r[1]..r[3] {
            for x in r[0]..r[2] {
                n += u32::from(f(px3(q, x, y)));
            }
        }
        n
    }

    /// Caixa [x, y, w, h] (px) dos pixels que passam em `f`.
    fn caixa_q(q: &medir::QuadroRgb, f: fn([u8; 3]) -> bool) -> Option<[u32; 4]> {
        let mut r: Option<[u32; 4]> = None;
        for y in 0..q.h {
            for x in 0..q.w {
                if f(px3(q, x, y)) {
                    let a = r.get_or_insert([x, y, x, y]);
                    *a = [a[0].min(x), a[1].min(y), a[2].max(x), a[3].max(y)];
                }
            }
        }
        r.map(|a| [a[0], a[1], a[2] - a[0] + 1, a[3] - a[1] + 1])
    }

    fn amarelo(p: [u8; 3]) -> bool {
        p[0] > 120 && p[1] > 120 && p[2] < 60
    }
    fn ciano(p: [u8; 3]) -> bool {
        p[1] > 120 && p[2] > 120 && p[0] < 60
    }
    fn verde(p: [u8; 3]) -> bool {
        p[1] > 120 && p[0] < 60 && p[2] < 60
    }
    fn magenta(p: [u8; 3]) -> bool {
        p[0] > 120 && p[2] > 120 && p[1] < 60
    }

    const AMARELO: &str = "#c0c000";
    const CIANO: &str = "#00c0c0";

    fn cartao(id: &str, cor: &str, duracao_s: f64, transicao: Value) -> Value {
        json!({ "tipo": "titulo", "id": id, "rotulo": "", "duracao_s": duracao_s, "transicao_entrada": transicao,
                "titulo": { "modelo": "cartela", "fundo": { "tipo": "cor", "cor": cor }, "blocos": [] } })
    }

    fn sob_titulo(id: &str, pos: Value, duracao_s: Option<f64>, t: &Titulo) -> Value {
        json!({ "tipo": "titulo", "id": id, "rotulo": "", "pos": pos, "duracao_s": duracao_s,
                "transformacao": { "x": 0.5, "y": 0.5, "escala": 1.0, "rotacao": 0.0, "opacidade": 1.0 }, "entrada_s": 0.0, "saida_s": 0.0, "efeitos": [], "titulo": t })
    }

    /// De que clipe é o quadro, pelo canto de cima à esquerda: cartão amarelo, cartão ciano ou o trecho.
    fn de_quem(q: &medir::QuadroRgb) -> char {
        let r = [2, 2, 14, 14];
        if quantos(q, r, amarelo) == 144 {
            'A'
        } else if quantos(q, r, ciano) == 144 {
            'C'
        } else {
            'B'
        }
    }

    /// Terço inferior "até o fim do clipe" preso ao trecho B: a faixa aparece EXATAMENTE nos quadros do trecho, com
    /// B no meio, depois do clipe reordenado para o começo e depois de aparado (outros quadros da mídia, outra duração).
    #[test]
    fn terco_inferior_segue_o_clipe_reordenado_e_aparado() {
        let Some(cap) = fontes::cap() else { return };
        let m = crate::edicao::quadros::apoio::medida(&cap, fontes::Fonte::Mp4);
        let tab = &m.info.quadros;
        // a faixa do terço em verde opaco, para medir
        let cor_faixa = "#00e000";
        let mut terco = titulo_no_modelo(ModeloTitulo::TercoInferior, true, "b0", TamanhoPx { w: 320, h: 180 });
        terco.blocos[0].conteudo = "Câmera 2".into();
        terco.blocos[0].estilo.caixa.as_mut().unwrap().cor = cor_faixa.into();
        assert_eq!(terco.fundo, Fundo::Transparente);
        let cache = fontes::pasta_temporaria("p17-terco-caixa");
        let esperada = caixas_dos_blocos(&terco, TamanhoPx { w: 320, h: 180 }, &cache)[0];
        let sob = json!({ "sobreposicoes": [{ "id": "s1", "nome": "", "oculta": false,
            "clipes": [sob_titulo("ti", json!({ "ref": "clipe", "clipe": "B", "desloc": 0.0 }), None, &terco)] }] });
        let a = cartao("A", AMARELO, 0.5, Value::Null);
        let c = cartao("C", CIANO, 0.5, Value::Null);
        let mundo = aceite::Mundo::novo(&cap).com("m1", &m.info);
        // terço de baixo, metade esquerda
        let regiao = [0, 90, 220, 175];
        for (nome, principal, n_b) in [
            ("no meio", vec![a.clone(), aceite::trecho("B", "m1", tab, 60, 90, 1.0, json!([])), c.clone()], 30),
            ("reordenado", vec![aceite::trecho("B", "m1", tab, 60, 90, 1.0, json!([])), a.clone(), c.clone()], 30),
            ("aparado", vec![a.clone(), aceite::trecho("B", "m1", tab, 66, 80, 1.0, json!([])), c.clone()], 14),
            ("aparado e reordenado", vec![a.clone(), c.clone(), aceite::trecho("B", "m1", tab, 100, 145, 1.0, json!([]))], 45),
        ] {
            let p = aceite::projeto(320, 180, (30, 1), vec![aceite::midia("m1", "video", json!([]))], principal, sob.clone());
            let q = renderizar(&cap, &p, &mundo, &format!("p17-terco-{}", nome.replace(' ', "-")));
            let quem: String = q.iter().map(de_quem).collect();
            let faixa: Vec<bool> = q.iter().map(|x| quantos(x, regiao, verde) > 150).collect();
            eprintln!("{nome} ({}): {quem}", cap.versao);
            assert_eq!(quem.matches('B').count(), n_b, "{nome}: quadros do trecho");
            for (k, (c, f)) in quem.chars().zip(&faixa).enumerate() {
                assert_eq!(*f, c == 'B', "{nome} ({}): quadro {k} do clipe {c} com faixa {f}", cap.versao);
            }
            // no lugar do modelo: a faixa no vídeo é a caixa do bloco, a 1 px (borda do croma 4:2:0), e na área segura
            for (k, x) in q.iter().enumerate().filter(|(k, _)| faixa[*k]) {
                let v = caixa_q(x, verde).unwrap();
                let bordas = [v[0] as f64, v[1] as f64, (v[0] + v[2]) as f64, (v[1] + v[3]) as f64];
                let alvo = [esperada[0], esperada[1], esperada[0] + esperada[2], esperada[1] + esperada[3]];
                assert!(bordas.iter().zip(&alvo).all(|(a, b)| (a - b).abs() <= 1.0), "{nome} ({}): quadro {k}: faixa em {v:?} × {esperada:?}", cap.versao);
                assert!(na_area_segura(&Caixa { x: v[0] as f64 / 320.0, y: v[1] as f64 / 180.0, w: v[2] as f64 / 320.0, h: v[3] as f64 / 180.0 }), "{nome}: quadro {k}: {v:?}");
            }
        }
        let _ = std::fs::remove_dir_all(cache);
    }

    /// O mesmo terço preso ao trecho B em 29,97 a 0,5×, em 25 a 2× e deslocado 0,4 s: a faixa só nos quadros do
    /// trecho (a partir do deslocamento); sem o terço, nenhum quadro com faixa (o medidor não acende no vídeo).
    #[test]
    fn terco_inferior_em_2997_25_e_deslocado() {
        let Some(cap) = fontes::cap() else { return };
        let m = crate::edicao::quadros::apoio::medida(&cap, fontes::Fonte::Mp4);
        let tab = &m.info.quadros;
        let mut terco = titulo_no_modelo(ModeloTitulo::TercoInferior, true, "b0", TamanhoPx { w: 320, h: 180 });
        terco.blocos[0].conteudo = "Câmera 2".into();
        terco.blocos[0].estilo.caixa.as_mut().unwrap().cor = "#00e000".into();
        let (a, c) = (cartao("A", AMARELO, 0.5, Value::Null), cartao("C", CIANO, 0.5, Value::Null));
        let mundo = aceite::Mundo::novo(&cap).com("m1", &m.info);
        let regiao = [0, 90, 220, 175];
        // (nome, fps, velocidade, deslocamento, quadros de B, com o terço)
        for (nome, fps, vel, desloc, n_b, com) in [
            ("sem o terco", (30, 1), 1.0, 0.0, 30, false),
            ("2997 a meia", (30000, 1001), 0.5, 0.0, 60, true),
            ("25 a 2x", (25, 1), 2.0, 0.0, 13, true),
            ("deslocado", (30, 1), 1.0, 0.4, 30, true),
        ] {
            let sob = if com {
                json!({ "sobreposicoes": [{ "id": "s1", "nome": "", "oculta": false,
                    "clipes": [sob_titulo("ti", json!({ "ref": "clipe", "clipe": "B", "desloc": desloc }), None, &terco)] }] })
            } else {
                json!({})
            };
            let principal = vec![a.clone(), aceite::trecho("B", "m1", tab, 60, 90, vel, json!([])), c.clone()];
            let p = aceite::projeto(320, 180, fps, vec![aceite::midia("m1", "video", json!([]))], principal, sob);
            let q = renderizar(&cap, &p, &mundo, &format!("p17-terco2-{}", nome.replace(' ', "-")));
            let quem: String = q.iter().map(de_quem).collect();
            eprintln!("{nome} ({}): {quem}", cap.versao);
            assert_eq!(quem.matches('B').count(), n_b, "{nome}: quadros do trecho");
            let b0 = quem.find('B').unwrap();
            // início do preso = início da âncora + round(desloc·fps)
            let de = b0 + (desloc * fps.0 as f64 / fps.1 as f64).round() as usize;
            for (k, (cl, x)) in quem.chars().zip(&q).enumerate() {
                let faixa = quantos(x, regiao, verde) > 150;
                assert_eq!(faixa, com && cl == 'B' && k >= de, "{nome} ({}): quadro {k} do clipe {cl}", cap.versao);
            }
        }
    }

    /// Marca fixa (título preso ao programa, sem duração): a mesma, nos mesmos pixels, em todo quadro do programa, por
    /// cima de cartões, trecho, transições e fecho; começando em 1 s, só dali ao último quadro.
    #[test]
    fn marca_fixa_no_programa_inteiro() {
        let Some(cap) = fontes::cap() else { return };
        let m = crate::edicao::quadros::apoio::medida(&cap, fontes::Fonte::Mp4);
        let tab = &m.info.quadros;
        let cor_marca = "#e000e0";
        let mut marca = titulo_no_modelo(ModeloTitulo::Livre, true, "b0", TamanhoPx { w: 320, h: 180 });
        let b = &mut marca.blocos[0];
        b.conteudo = "SICRO".into();
        b.lugar = Lugar { x: 0.97, y: 0.04, h: Alinhamento::Dir, v: AlinhamentoV::Topo };
        b.estilo.tamanho = 0.08;
        b.estilo.caixa = Some(CaixaTexto { cor: cor_marca.into(), margem: 0.02, raio: 0.0, faixa: false });
        let escurecer = json!({ "tipo": "escurecer", "duracao_s": 0.2, "audio": "acompanhar" });
        let principal = vec![
            cartao("A", AMARELO, 0.5, Value::Null),
            aceite::trecho("B", "m1", tab, 60, 90, 1.0, json!([])),
            cartao("C", CIANO, 0.5, escurecer.clone()),
        ];
        let mundo = aceite::Mundo::novo(&cap).com("m1", &m.info);
        let canto = [200, 0, 320, 60];
        for (inicio, de_q) in [(0.0, 0usize), (1.0, 30)] {
            let extra = json!({ "fecho": escurecer, "sobreposicoes": [{ "id": "s1", "nome": "", "oculta": false,
                "clipes": [sob_titulo("mf", json!({ "ref": "programa", "t": inicio }), None, &marca)] }] });
            let p = aceite::projeto(320, 180, (30, 1), vec![aceite::midia("m1", "video", json!([]))], principal.clone(), extra);
            let q = renderizar(&cap, &p, &mundo, &format!("p17-marca-{de_q}"));
            assert_eq!(q.len(), 15 + 30 + 15);
            let presente: Vec<bool> = q.iter().map(|x| quantos(x, canto, magenta) > 150).collect();
            let esperado: Vec<bool> = (0..q.len()).map(|k| k >= de_q).collect();
            assert_eq!(presente, esperado, "início {inicio} ({})", cap.versao);
            // a mesma em todo quadro: caixa da cor no mesmo lugar (a borda do croma 4:2:0 mistura o fundo: 1 px) e o
            // miolo da faixa, com o texto, pixel a pixel (a 6 px da borda: a leitura em RGB interpola o croma vizinho)
            let caixa_em = |x: &medir::QuadroRgb| caixa_q(x, magenta).unwrap();
            let ref_q = &q[de_q];
            let cx = caixa_em(ref_q);
            assert!(cx[2] > 30 && cx[3] > 15, "faixa da marca {cx:?}");
            for (k, x) in q.iter().enumerate().skip(de_q) {
                let c = caixa_em(x);
                let bordas = |c: [u32; 4]| [c[0], c[1], c[0] + c[2], c[1] + c[3]];
                assert!(bordas(c).iter().zip(&bordas(cx)).all(|(a, b)| a.abs_diff(*b) <= 1), "quadro {k}: marca em {c:?} × {cx:?}");
                for yy in cx[1] + 6..cx[1] + cx[3] - 6 {
                    for xx in cx[0] + 6..cx[0] + cx[2] - 6 {
                        assert_eq!(px3(x, xx, yy), px3(ref_q, xx, yy), "quadro {k} ({xx}, {yy}): miolo da marca mudou");
                    }
                }
            }
        }
    }
}
