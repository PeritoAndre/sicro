//! Geometria única do desenho (§5.5, §8.3): efeito + quadro → `Primitiva`s, e o desenho delas.
//! Espelho no front: `palco/desenho/primitivas.ts`, pelas fixtures `primitivas-*.json`. Dono: P0.5.
//!
//! Contas (o espelho repete na mesma ordem para dar os mesmos bits): só `+ − × ÷`, `floor` e `abs` de f64, e o
//! `cos_sen_graus` daqui; nada de `Math.sin`, `hypot` nem raiz. A conta corre em px do canvas (x·w, y·h) e volta a
//! 0..1 (÷ w, ÷ h) na saída, que passa por `+ 0` (−0 vira 0). Regiões (`ocultar`, `holofote`) ficam em 0..1 direto.
//!
//! Regras do pintor (as mesmas em `pintar.ts`): preenchimento antes do traço; traço de pontas e junções redondas,
//! largura = `largura` × altura do canvas; `contorno` = traço por baixo, na cor dele, com `CONTORNO` × a largura;
//! `sprite` esticado na `caixa`, com `opacidade`; `atlas`: para cada caractere de `texto`, a célula da mesma posição
//! em `glifos`, com altura `altura` × altura do canvas, a caixa do texto posta pelo `lugar` com o canto arredondado
//! ao pixel (`floor(v + 0,5)`); `regiao` e `lupa` não são pintadas (máscara em `mascara.rs`, pixels no ffmpeg).

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::de::{self, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tiny_skia::{
    FillRule, FilterQuality, IntSize, LineCap, LineJoin, Paint, PathBuilder, Pattern, Pixmap, PixmapPaint, Rect,
    SpreadMode, Stroke, Transform,
};

use super::texto::{FonteSprites, PedidoSprite};
use super::Bitmap;
use crate::edicao::cor;
use crate::edicao::geometria::{self, Afim};
use crate::edicao::mensagens::codigo;
use crate::edicao::modelo::{
    AlinhamentoV, Alinhamento, AlvoRegiao, BlocoTexto, Caixa, EfCarimbo, EfDestaque, Efeito, FormaDestaque, FormaLupa,
    FormaRegiao, FormatoCarimbo, Lado, Lugar, ModoOcultar, MostraCarimbo, PapelTexto, Ponto, RetratoRelogio, TamanhoPx,
    Traco,
};
use crate::edicao::motor::plano::ImagemCamada;
use crate::error::{Result, SicroError};

// CONTRATO: acréscimo (regra do pintor, igual em `pintar.ts`).
/// Largura do contorno do traço em relação à do traço.
pub const CONTORNO: f64 = 1.5;
/// Cor do contorno do traço (`Traco.contorno` ligado).
pub const COR_CONTORNO: &str = "#000000bf";
// CONTRATO: acréscimo (código de erro; falta no catálogo de `mensagens.rs`).
/// Efeito que a Fase 0 não desenha (lupa: P1.2; régua e velocidade: P1.3).
pub const NAO_SUPORTADO: &str = "nao_suportado";
/// √2/2 (0,7071067811865476, o `Math.SQRT1_2`): círculo e elipse sobre caixa passam pelos cantos dela.
const RAIZ2_2: f64 = std::f64::consts::FRAC_1_SQRT_2;
const COS30: f64 = 0.8660254037844386;
const SEN30: f64 = 0.5;
/// Asa da seta: fração do comprimento.
const ASA: f64 = 0.3;
/// Braço do colchete: fração do lado menor.
const BRACO: f64 = 0.25;
/// Largura máxima do rótulo do destaque: fração da largura do canvas.
const ROTULO_LARGURA: f64 = 0.5;

/// Traço de uma primitiva: `largura` em fração da altura do canvas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Traco2 {
    pub cor: String,
    pub largura: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contorno: Option<String>,
}

/// Segmento de caminho, em 0..1 do canvas. No fio: `["M", x, y]`, `["L", x, y]`, `["C", x1, y1, x2, y2, x, y]`, `["Z"]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segmento {
    M(f64, f64),
    L(f64, f64),
    C(f64, f64, f64, f64, f64, f64),
    Z,
}

impl Serialize for Segmento {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let (letra, nums): (&str, Vec<f64>) = match *self {
            Segmento::M(x, y) => ("M", vec![x, y]),
            Segmento::L(x, y) => ("L", vec![x, y]),
            Segmento::C(a, b, c, d, x, y) => ("C", vec![a, b, c, d, x, y]),
            Segmento::Z => ("Z", vec![]),
        };
        let mut seq = s.serialize_seq(Some(1 + nums.len()))?;
        seq.serialize_element(letra)?;
        for n in nums {
            seq.serialize_element(&n)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for Segmento {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Segmento;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("[\"M\"|\"L\", x, y], [\"C\", 6 números] ou [\"Z\"]")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Segmento, A::Error> {
                let letra: String = seq.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let mut n = Vec::with_capacity(6);
                while let Some(v) = seq.next_element::<f64>()? {
                    n.push(v);
                }
                match (letra.as_str(), n.as_slice()) {
                    ("M", [x, y]) => Ok(Segmento::M(*x, *y)),
                    ("L", [x, y]) => Ok(Segmento::L(*x, *y)),
                    ("C", [a, b, c, d, x, y]) => Ok(Segmento::C(*a, *b, *c, *d, *x, *y)),
                    ("Z", []) => Ok(Segmento::Z),
                    _ => Err(de::Error::custom("segmento de caminho inválido")),
                }
            }
        }
        d.deserialize_seq(V)
    }
}

/// Regra de preenchimento do caminho; ausente = não zero.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegraPreenche {
    Evenodd,
}

/// O que acontece dentro da região (`escurecer` = holofote).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModoRegiao {
    Desfoque,
    Mosaico,
    Solido,
    Escurecer,
}

/// Onde a região é medida: fonte exibida (por quadro lido) ou canvas (por slot).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EspacoRegiao {
    Fonte,
    Canvas,
}

/// O que se desenha, em 0..1 do canvas. O pintor (front) e este módulo implementam TODOS os `k` uma vez.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "k", rename_all = "snake_case")]
pub enum Primitiva {
    Caminho {
        d: Vec<Segmento>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        traco: Option<Traco2>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preenche: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        regra: Option<RegraPreenche>,
    },
    Elipse {
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        traco: Option<Traco2>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preenche: Option<String>,
    },
    /// `src` = caminho relativo ao cache da Edição do caso (sprite de texto) ou `midia:<MidiaRef.id>` (imagem do projeto).
    Sprite { src: String, caixa: Caixa, opacidade: f64 },
    /// Carimbo: `texto` montado com as células do atlas; `altura` em fração da altura do canvas.
    Atlas { src: String, celula: TamanhoPx, glifos: String, texto: String, lugar: Lugar, altura: f64 },
    /// `forma` = caminho ou elipse.
    Regiao {
        modo: ModoRegiao,
        forma: Box<Primitiva>,
        intensidade: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cor: Option<String>,
        borda_suave: f64,
        inverter: bool,
        espaco: EspacoRegiao,
    },
    /// `origem` em 0..1 da fonte, `destino` em 0..1 do canvas.
    Lupa { origem: Caixa, destino: Caixa, forma: FormaLupa },
}

/// Estado de um quadro para quem desenha (espelho de `QuadroPalco`): slot `q`, tempo `t` (s, base do efeito),
/// `pts` do quadro de fonte, `a` = A(τ), `canvas` em px e a caixa de cada rastreio já avaliado (0..1).
/// Em `MascaraFonte`, `canvas` é o tamanho exibido da fonte (o espaço da máscara) e `rastreios` vêm nesse espaço.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuadroCamada {
    pub q: u64,
    pub t: f64,
    pub pts: Option<i64>,
    pub a: Afim,
    pub canvas: TamanhoPx,
    /// Rastreio sem caixa neste quadro não aparece.
    pub rastreios: BTreeMap<String, Caixa>,
    // CONTRATO: acréscimo (carimbo "Quadro"); no TS, `QuadroPalco.indice`.
    /// Índice do quadro de fonte na tabela (o número que a tela mostra); None sem fonte (título, imagem).
    #[serde(default)]
    pub indice: Option<u64>,
    // CONTRATO: acréscimo (carimbo "Vídeo"); no TS, `QuadroPalco.programa_s`.
    /// Instante do slot no programa (s): `q·den/num` da saída.
    #[serde(default)]
    pub programa_s: f64,
}

/// −0 → 0.
fn z(v: f64) -> f64 {
    v + 0.0
}

// CONTRATO: acréscimo (o espelho TS repete; fixture `primitivas-seno.json`).
/// (cos, sen) de `graus` no sentido anti-horário, só com `+ − × ÷` e `floor`: redução a 0..45° e série de Taylor
/// (erro abaixo de 1 ulp). Igual em toda máquina e no espelho TS, ao contrário de `sin`/`cos` da biblioteca.
pub fn cos_sen_graus(graus: f64) -> (f64, f64) {
    if !graus.is_finite() {
        return (1.0, 0.0);
    }
    let mut g = graus - 360.0 * (graus / 360.0).floor();
    if !(0.0..360.0).contains(&g) {
        g = 0.0;
    }
    let (quadrante, r) = if g < 90.0 {
        (0, g)
    } else if g < 180.0 {
        (1, g - 90.0)
    } else if g < 270.0 {
        (2, g - 180.0)
    } else {
        (3, g - 270.0)
    };
    let (c, s) = if r <= 45.0 {
        serie(r)
    } else {
        let (c, s) = serie(90.0 - r);
        (s, c)
    };
    let (c, s) = match quadrante {
        0 => (c, s),
        1 => (-s, c),
        2 => (-c, -s),
        _ => (s, -c),
    };
    (z(c), z(s))
}

/// (cos, sen) de 0..45°, Horner.
fn serie(g: f64) -> (f64, f64) {
    let x = g * (std::f64::consts::PI / 180.0);
    let x2 = x * x;
    let s = x
        * (1.0
            + x2 * (-1.0 / 6.0
                + x2 * (1.0 / 120.0
                    + x2 * (-1.0 / 5040.0
                        + x2 * (1.0 / 362880.0
                            + x2 * (-1.0 / 39916800.0
                                + x2 * (1.0 / 6227020800.0
                                    + x2 * (-1.0 / 1307674368000.0 + x2 * (1.0 / 355687428096000.0)))))))));
    let c = 1.0
        + x2 * (-0.5
            + x2 * (1.0 / 24.0
                + x2 * (-1.0 / 720.0
                    + x2 * (1.0 / 40320.0
                        + x2 * (-1.0 / 3628800.0
                            + x2 * (1.0 / 479001600.0
                                + x2 * (-1.0 / 87178291200.0 + x2 * (1.0 / 20922789888000.0))))))));
    (c, s)
}

/// Canvas em px, para as contas.
#[derive(Clone, Copy)]
struct Tela {
    w: f64,
    h: f64,
}

impl Tela {
    fn de(c: TamanhoPx) -> Self {
        Tela { w: c.w as f64, h: c.h as f64 }
    }
    fn caixa(&self, x: f64, y: f64, w: f64, h: f64) -> Caixa {
        Caixa { x: z(x / self.w), y: z(y / self.h), w: z(w / self.w), h: z(h / self.h) }
    }
    fn m(&self, x: f64, y: f64) -> Segmento {
        Segmento::M(z(x / self.w), z(y / self.h))
    }
    fn l(&self, x: f64, y: f64) -> Segmento {
        Segmento::L(z(x / self.w), z(y / self.h))
    }
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

fn traco2(t: &Traco) -> Traco2 {
    Traco2 { cor: t.cor.clone(), largura: t.espessura, contorno: t.contorno.then(|| COR_CONTORNO.to_string()) }
}

fn invalido(detalhe: &str) -> SicroError {
    SicroError::Validation(format!("{}: {detalhe}", codigo::ITEM_INVALIDO))
}

/// Primitivas de um efeito num quadro (vazio = nada a desenhar). Efeito de pixel puro (ajustes, lente…) devolve vazio;
/// efeito desligado também. `ativo` não é olhado aqui: quem chama só pede os quadros em que o efeito vale.
/// Lupa, régua e velocidade exibida são da Fase 1: erro `nao_suportado`.
pub fn primitivas(efeito: &Efeito, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    if !ligado(efeito) {
        return Ok(Vec::new());
    }
    let t = Tela::de(quadro.canvas);
    match efeito {
        Efeito::Estabilizar(_)
        | Efeito::Lente(_)
        | Efeito::Desentrelacar(_)
        | Efeito::MediaTemporal(_)
        | Efeito::Ajustes(_)
        | Efeito::Nitidez(_)
        | Efeito::Girar(_)
        | Efeito::Enquadramento(_) => Ok(Vec::new()),
        Efeito::Ocultar(o) => {
            let Some(r) = quadro.rastreios.get(&o.alvo.rastreio) else { return Ok(Vec::new()) };
            let modo = match o.modo {
                ModoOcultar::Desfoque => ModoRegiao::Desfoque,
                ModoOcultar::Mosaico => ModoRegiao::Mosaico,
                ModoOcultar::Solido => ModoRegiao::Solido,
            };
            Ok(vec![Primitiva::Regiao {
                modo,
                forma: Box::new(forma_regiao(&o.alvo, *r, None)?),
                intensidade: o.intensidade,
                cor: (modo == ModoRegiao::Solido).then(|| o.cor.clone()),
                borda_suave: o.alvo.borda_suave,
                inverter: o.alvo.inverter,
                espaco: EspacoRegiao::Fonte,
            }])
        }
        Efeito::Holofote(h) => {
            let Some(r) = quadro.rastreios.get(&h.alvo.rastreio) else { return Ok(Vec::new()) };
            // escurece FORA do alvo: a máscara é a do alvo, invertida
            Ok(vec![Primitiva::Regiao {
                modo: ModoRegiao::Escurecer,
                forma: Box::new(forma_regiao(&h.alvo, *r, Some(&quadro.a))?),
                intensidade: h.escurecer,
                cor: None,
                borda_suave: h.alvo.borda_suave,
                inverter: !h.alvo.inverter,
                espaco: EspacoRegiao::Canvas,
            }])
        }
        Efeito::Destaque(d) => destaque(d, quadro, &t, sprites),
        Efeito::Texto(x) => {
            if x.conteudo.is_empty() {
                return Ok(Vec::new());
            }
            let pedido = PedidoSprite {
                blocos: vec![BlocoTexto {
                    id: x.base.id.clone(),
                    papel: PapelTexto::Corpo,
                    conteudo: x.conteudo.clone(),
                    estilo: x.estilo.clone(),
                    lugar: x.lugar,
                    largura_max: x.largura_max,
                }],
                largura_max: x.largura_max * t.w / t.h,
                altura_canvas: quadro.canvas.h,
                fundo: true,
            };
            let s = sprites.sprite(&pedido)?;
            let (sw, sh) = (s.w as f64, s.h as f64);
            let x0 = ao_pixel(x.lugar.x * t.w - sw * fator_h(x.lugar.h));
            let y0 = ao_pixel(x.lugar.y * t.h - sh * fator_v(x.lugar.v));
            Ok(vec![Primitiva::Sprite { src: s.relativo, caixa: t.caixa(x0, y0, sw, sh), opacidade: 1.0 }])
        }
        Efeito::Carimbo(c) => {
            let Some(texto) = texto_carimbo(c, quadro)? else { return Ok(Vec::new()) };
            let atlas = sprites.atlas(&c.estilo, quadro.canvas.h)?;
            let texto = if c.estilo.caixa.is_some() { format!(" {texto} ") } else { texto };
            let altura = z(atlas.celula.h as f64 / t.h);
            Ok(vec![Primitiva::Atlas { src: atlas.relativo, celula: atlas.celula, glifos: atlas.glifos, texto, lugar: c.lugar, altura }])
        }
        Efeito::Imagem(i) => {
            let tam = sprites
                .imagem(&i.midia)
                .filter(|s| s.w > 0 && s.h > 0)
                .ok_or_else(|| SicroError::Validation(format!("{}: {}", codigo::MIDIA_AUSENTE, i.midia)))?;
            let sw = i.largura * t.w;
            let sh = sw * tam.h as f64 / tam.w as f64;
            let x0 = i.lugar.x * t.w - sw * fator_h(i.lugar.h);
            let y0 = i.lugar.y * t.h - sh * fator_v(i.lugar.v);
            Ok(vec![Primitiva::Sprite { src: format!("midia:{}", i.midia), caixa: t.caixa(x0, y0, sw, sh), opacidade: i.opacidade }])
        }
        Efeito::Lupa(_) => Err(SicroError::Validation(format!("{NAO_SUPORTADO}: lupa"))),
        Efeito::Regua(_) => Err(SicroError::Validation(format!("{NAO_SUPORTADO}: regua"))),
        Efeito::VelocidadeExibida(_) => Err(SicroError::Validation(format!("{NAO_SUPORTADO}: velocidade_exibida"))),
    }
}

fn ligado(e: &Efeito) -> bool {
    match e {
        Efeito::Estabilizar(x) => x.base.ligado,
        Efeito::Lente(x) => x.base.ligado,
        Efeito::Desentrelacar(x) => x.base.ligado,
        Efeito::MediaTemporal(x) => x.base.ligado,
        Efeito::Ajustes(x) => x.base.ligado,
        Efeito::Nitidez(x) => x.base.ligado,
        Efeito::Girar(x) => x.base.ligado,
        Efeito::Enquadramento(x) => x.base.ligado,
        Efeito::Ocultar(x) => x.base.ligado,
        Efeito::Holofote(x) => x.base.ligado,
        Efeito::Destaque(x) => x.base.ligado,
        Efeito::Lupa(x) => x.base.ligado,
        Efeito::Regua(x) => x.base.ligado,
        Efeito::VelocidadeExibida(x) => x.base.ligado,
        Efeito::Texto(x) => x.base.ligado,
        Efeito::Carimbo(x) => x.base.ligado,
        Efeito::Imagem(x) => x.base.ligado,
    }
}

/// Forma da região (caixa do rastreio com a margem): retângulo, elipse inscrita ou polígono (vértices em 0..1 da
/// caixa). `a` = levar ao canvas (holofote); None = fica no espaço do rastreio (ocultar, na fonte).
/// Silhueta é da Fase 5 (elipse inscrita ∪ silhueta dilatada, `mascara::unir`): até lá, `nao_suportado`.
fn forma_regiao(alvo: &AlvoRegiao, caixa: Caixa, a: Option<&Afim>) -> Result<Primitiva> {
    let c = geometria::com_margem(caixa, alvo.margem);
    let no_espaco = |c: Caixa| a.map(|a| geometria::caixa_no_canvas(a, c)).unwrap_or(c);
    let b = no_espaco(c);
    Ok(match alvo.forma {
        FormaRegiao::Retangulo => Primitiva::Caminho {
            d: vec![
                Segmento::M(b.x, b.y),
                Segmento::L(z(b.x + b.w), b.y),
                Segmento::L(z(b.x + b.w), z(b.y + b.h)),
                Segmento::L(b.x, z(b.y + b.h)),
                Segmento::Z,
            ],
            traco: None,
            preenche: None,
            regra: None,
        },
        FormaRegiao::Elipse => Primitiva::Elipse {
            cx: z(b.x + b.w * 0.5),
            cy: z(b.y + b.h * 0.5),
            rx: z(b.w * 0.5),
            ry: z(b.h * 0.5),
            traco: None,
            preenche: None,
        },
        FormaRegiao::Poligono => {
            let v = alvo.vertices.as_deref().filter(|v| v.len() >= 3).ok_or_else(|| invalido("vertices"))?;
            let mut d = Vec::with_capacity(v.len() + 1);
            for (i, p) in v.iter().enumerate() {
                let o = Ponto { x: c.x + p.x * c.w, y: c.y + p.y * c.h };
                let p = a.map(|a| a.ponto(o)).unwrap_or(o);
                d.push(if i == 0 { Segmento::M(z(p.x), z(p.y)) } else { Segmento::L(z(p.x), z(p.y)) });
            }
            d.push(Segmento::Z);
            Primitiva::Caminho { d, traco: None, preenche: None, regra: None }
        }
        FormaRegiao::Silhueta => return Err(SicroError::Validation(format!("{NAO_SUPORTADO}: silhueta"))),
    })
}

/// Destaque sobre caixa ou ponto do rastreio, levado ao canvas por A. Sobre ponto o tamanho vem de `tamanho`
/// (fração da altura); sobre caixa, da caixa com a margem. Círculo e elipse sobre caixa passam pelos cantos dela.
fn destaque(d: &EfDestaque, q: &QuadroCamada, t: &Tela, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    let Some(r) = q.rastreios.get(&d.rastreio) else { return Ok(Vec::new()) };
    let ponto = (r.w.is_nan() || r.w <= 0.0) && (r.h.is_nan() || r.h <= 0.0);
    // (x0, y0) canto, (w, h) tamanho, (cx, cy) centro, em px do canvas
    let (x0, y0, w, h, cx, cy) = if ponto {
        let p = q.a.ponto(Ponto { x: r.x, y: r.y });
        let l = d.tamanho * t.h;
        let (cx, cy) = (p.x * t.w, p.y * t.h);
        (cx - l * 0.5, cy - l * 0.5, l, l, cx, cy)
    } else {
        let c = geometria::caixa_no_canvas(&q.a, geometria::com_margem(*r, d.margem));
        let (x0, y0, w, h) = (c.x * t.w, c.y * t.h, c.w * t.w, c.h * t.h);
        (x0, y0, w, h, x0 + w * 0.5, y0 + h * 0.5)
    };
    let traco = Some(traco2(&d.traco));
    // envolvente da forma (px), para o rótulo
    let (forma, (bx0, by0, bx1, by1)) = match d.forma {
        FormaDestaque::Circulo | FormaDestaque::Elipse => {
            let (rx, ry) = match (d.forma, ponto) {
                (_, true) => (w * 0.5, h * 0.5),
                (FormaDestaque::Circulo, false) => {
                    let m = RAIZ2_2 * w.max(h);
                    (m, m)
                }
                _ => (RAIZ2_2 * w, RAIZ2_2 * h),
            };
            let p = Primitiva::Elipse { cx: z(cx / t.w), cy: z(cy / t.h), rx: z(rx / t.w), ry: z(ry / t.h), traco, preenche: None };
            (p, (cx - rx, cy - ry, cx + rx, cy + ry))
        }
        FormaDestaque::Retangulo => {
            let (x1, y1) = (x0 + w, y0 + h);
            let dd = vec![t.m(x0, y0), t.l(x1, y0), t.l(x1, y1), t.l(x0, y1), Segmento::Z];
            (Primitiva::Caminho { d: dd, traco, preenche: None, regra: None }, (x0, y0, x1, y1))
        }
        FormaDestaque::Colchetes => {
            let (x1, y1) = (x0 + w, y0 + h);
            let b = BRACO * w.min(h);
            let dd = vec![
                t.m(x0, y0 + b),
                t.l(x0, y0),
                t.l(x0 + b, y0),
                t.m(x1 - b, y0),
                t.l(x1, y0),
                t.l(x1, y0 + b),
                t.m(x1, y1 - b),
                t.l(x1, y1),
                t.l(x1 - b, y1),
                t.m(x0 + b, y1),
                t.l(x0, y1),
                t.l(x0, y1 - b),
            ];
            (Primitiva::Caminho { d: dd, traco, preenche: None, regra: None }, (x0, y0, x1, y1))
        }
        FormaDestaque::Seta => {
            // u: da ponta para a cauda (y para baixo); a cauda vem de `angulo_graus` (anti-horário a partir da direita)
            let (c, s) = cos_sen_graus(d.angulo_graus);
            let (ux, uy) = (c, -s);
            let (px, py) = if ponto {
                (cx, cy)
            } else {
                // saída do raio do centro pela borda da caixa
                let (hw, hh) = (w * 0.5, h * 0.5);
                let kx = if ux != 0.0 { hw / ux.abs() } else { f64::INFINITY };
                let ky = if uy != 0.0 { hh / uy.abs() } else { f64::INFINITY };
                let k = kx.min(ky);
                (cx + k * ux, cy + k * uy)
            };
            let l = d.tamanho * t.h;
            let (tx, ty) = (px + l * ux, py + l * uy);
            let a = l * ASA;
            let (a1x, a1y) = (px + a * (ux * COS30 - uy * SEN30), py + a * (ux * SEN30 + uy * COS30));
            let (a2x, a2y) = (px + a * (ux * COS30 + uy * SEN30), py + a * (uy * COS30 - ux * SEN30));
            let dd = vec![t.m(tx, ty), t.l(px, py), t.m(a1x, a1y), t.l(px, py), t.l(a2x, a2y)];
            let xs = [tx, px, a1x, a2x];
            let ys = [ty, py, a1y, a2y];
            let mn = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
            let mx = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            (Primitiva::Caminho { d: dd, traco, preenche: None, regra: None }, (mn(&xs), mn(&ys), mx(&xs), mx(&ys)))
        }
    };
    let mut v = vec![forma];
    if let Some(rot) = d.rotulo.as_ref().filter(|r| !r.texto.is_empty()) {
        let pedido = PedidoSprite {
            blocos: vec![BlocoTexto {
                id: d.base.id.clone(),
                papel: PapelTexto::Corpo,
                conteudo: rot.texto.clone(),
                estilo: rot.estilo.clone(),
                lugar: Lugar { x: 0.0, y: 0.0, h: Alinhamento::Esq, v: AlinhamentoV::Topo },
                largura_max: ROTULO_LARGURA,
            }],
            largura_max: ROTULO_LARGURA * t.w / t.h,
            altura_canvas: q.canvas.h,
            fundo: true,
        };
        let s = sprites.sprite(&pedido)?;
        let (sw, sh) = (s.w as f64, s.h as f64);
        let g = d.traco.espessura * t.h;
        let (x, y) = match rot.lado {
            Lado::Cima => ((bx0 + bx1) * 0.5 - sw * 0.5, by0 - g - sh),
            Lado::Baixo => ((bx0 + bx1) * 0.5 - sw * 0.5, by1 + g),
            Lado::Esq => (bx0 - g - sw, (by0 + by1) * 0.5 - sh * 0.5),
            Lado::Dir => (bx1 + g, (by0 + by1) * 0.5 - sh * 0.5),
        };
        v.push(Primitiva::Sprite { src: s.relativo, caixa: t.caixa(ao_pixel(x), ao_pixel(y), sw, sh), opacidade: 1.0 });
    }
    Ok(v)
}

/// Texto do carimbo, com o prefixo fixo. None = nada a mostrar (quadro sem fonte).
fn texto_carimbo(c: &EfCarimbo, q: &QuadroCamada) -> Result<Option<String>> {
    Ok(Some(match c.mostra {
        MostraCarimbo::TempoFonte => format!("Original {}", tempo(q.t, c.formato)),
        MostraCarimbo::TempoPrograma => format!("Vídeo {}", tempo(q.programa_s, c.formato)),
        MostraCarimbo::Quadro => match q.indice {
            Some(j) => format!("Quadro {j}"),
            None => return Ok(None),
        },
        MostraCarimbo::Relogio => {
            let r = c.relogio.as_ref().ok_or_else(|| invalido("relogio"))?;
            format!("Câmera {}", relogio(r, q.t, c.formato)?)
        }
    }))
}

/// Milissegundos inteiros (meio sobe).
fn ms_de(s: f64) -> i64 {
    (s * 1000.0 + 0.5).floor() as i64
}

/// "HH:MM:SS,mmm" (ou "HH:MM:SS") de milissegundos.
fn hms(ms: u64, com_ms: bool) -> String {
    let (h, m, s, r) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    if com_ms {
        format!("{h:02}:{m:02}:{s:02},{r:03}")
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

/// Tempo de mídia ou de programa; `data_hora` não tem data aqui e sai como `hms_ms`.
fn tempo(s: f64, f: FormatoCarimbo) -> String {
    let ms = ms_de(s);
    let sinal = if ms < 0 { "-" } else { "" };
    format!("{sinal}{}", hms(ms.unsigned_abs(), f != FormatoCarimbo::Hms))
}

/// Relógio da câmera: `clock_seconds + (t − media_time_s)`, com virada de dia; `data_hora` = "DD/MM/AAAA HH:MM:SS".
fn relogio(r: &RetratoRelogio, t: f64, f: FormatoCarimbo) -> Result<String> {
    let ms = ms_de(r.clock_seconds + (t - r.media_time_s));
    let (dias, resto) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000) as u64);
    Ok(match (f, r.clock_date.as_deref()) {
        (FormatoCarimbo::HmsMs, _) => hms(resto, true),
        (FormatoCarimbo::Hms, _) | (FormatoCarimbo::DataHora, None) => hms(resto, false),
        (FormatoCarimbo::DataHora, Some(d)) => {
            let (a, m, dd) = ler_data(d).ok_or_else(|| invalido("clock_date"))?;
            let (a, m, dd) = civil_de_dias(dias_de_civil(a, m, dd) + dias);
            format!("{dd:02}/{m:02}/{a:04} {}", hms(resto, false))
        }
    })
}

/// "AAAA-MM-DD" → (ano, mês, dia).
fn ler_data(s: &str) -> Option<(i64, i64, i64)> {
    let mut p = s.split('-');
    let (a, m, d) = (p.next()?.parse().ok()?, p.next()?.parse().ok()?, p.next()?.parse().ok()?);
    (p.next().is_none() && (1..=9999).contains(&a) && (1..=12).contains(&m) && (1..=31).contains(&d)).then_some((a, m, d))
}

/// Dias desde 1970-01-01 (calendário gregoriano proléptico; H. Hinnant).
fn dias_de_civil(a: i64, m: i64, d: i64) -> i64 {
    let a = if m <= 2 { a - 1 } else { a };
    let era = a.div_euclid(400);
    let aoe = a - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = aoe * 365 + aoe / 4 - aoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_de_dias(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let aoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * aoe + aoe / 4 - aoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (aoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

/// Meia largura (px) do que o traço ocupa além da linha, com o contorno.
fn meio_traco(t: &Option<Traco2>, h: f64) -> f64 {
    t.as_ref().map(|t| t.largura * h * 0.5 * if t.contorno.is_some() { CONTORNO } else { 1.0 }).unwrap_or(0.0)
}

/// Caixa do texto do atlas em px do canvas: (x, y, w, h, escala da célula).
pub(crate) fn caixa_atlas(celula: TamanhoPx, texto: &str, lugar: Lugar, altura: f64, t: TamanhoPx) -> (f64, f64, f64, f64, f64) {
    let tt = Tela::de(t);
    let e = if celula.h > 0 { altura * tt.h / celula.h as f64 } else { 0.0 };
    let e = if (e - 1.0).abs() < 1e-9 { 1.0 } else { e };
    let n = texto.chars().count() as f64;
    let (w, h) = (n * celula.w as f64 * e, celula.h as f64 * e);
    (ao_pixel(lugar.x * tt.w - w * fator_h(lugar.h)), ao_pixel(lugar.y * tt.h - h * fator_v(lugar.v)), w, h, e)
}

/// Pontos (px do espaço) de um caminho, com as curvas achatadas em 16 retas; cada sub-caminho é um polígono.
pub(crate) fn poligonos(d: &[Segmento], w: f64, h: f64) -> Vec<Vec<(f64, f64)>> {
    let mut v: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut atual: Vec<(f64, f64)> = Vec::new();
    for s in d {
        match *s {
            Segmento::M(x, y) => {
                if atual.len() > 1 {
                    v.push(std::mem::take(&mut atual));
                }
                atual.clear();
                atual.push((x * w, y * h));
            }
            Segmento::L(x, y) => atual.push((x * w, y * h)),
            Segmento::C(x1, y1, x2, y2, x, y) => {
                let (x0, y0) = atual.last().copied().unwrap_or((x * w, y * h));
                let (x1, y1, x2, y2, x3, y3) = (x1 * w, y1 * h, x2 * w, y2 * h, x * w, y * h);
                for i in 1..=16 {
                    let u = i as f64 / 16.0;
                    let k = 1.0 - u;
                    let (a, b, c, e) = (k * k * k, 3.0 * k * k * u, 3.0 * k * u * u, u * u * u);
                    atual.push((a * x0 + b * x1 + c * x2 + e * x3, a * y0 + b * y1 + c * y2 + e * y3));
                }
            }
            Segmento::Z => {
                if atual.len() > 1 {
                    let primeiro = atual[0];
                    v.push(std::mem::take(&mut atual));
                    atual.push(primeiro);
                }
            }
        }
    }
    if atual.len() > 1 {
        v.push(atual);
    }
    v
}

/// Envolvente (px) de uma forma de região: (x0, y0, x1, y1).
pub(crate) fn envolvente_forma(forma: &Primitiva, w: f64, h: f64) -> Option<(f64, f64, f64, f64)> {
    match forma {
        Primitiva::Elipse { cx, cy, rx, ry, .. } => {
            let (cx, cy, rx, ry) = (cx * w, cy * h, rx.abs() * w, ry.abs() * h);
            Some((cx - rx, cy - ry, cx + rx, cy + ry))
        }
        Primitiva::Caminho { d, .. } => {
            let mut b: Option<(f64, f64, f64, f64)> = None;
            for p in poligonos(d, w, h).iter().flatten() {
                b = Some(match b {
                    None => (p.0, p.1, p.0, p.1),
                    Some((a, c, e, f)) => (a.min(p.0), c.min(p.1), e.max(p.0), f.max(p.1)),
                });
            }
            b
        }
        _ => None,
    }
}

/// Caixa envolvente (0..1 do canvas) do que as primitivas desenham, com traço, contorno, 1 px de suavização e a
/// borda suave das regiões; região invertida = o canvas inteiro. None = nada.
pub fn envolvente(primitivas: &[Primitiva], canvas: TamanhoPx) -> Option<Caixa> {
    let t = Tela::de(canvas);
    let mut u: Option<(f64, f64, f64, f64)> = None;
    for p in primitivas {
        let b = match p {
            Primitiva::Caminho { traco, .. } | Primitiva::Elipse { traco, .. } => {
                envolvente_forma(p, t.w, t.h).map(|(a, b, c, d)| {
                    let m = meio_traco(traco, t.h) + 1.0;
                    (a - m, b - m, c + m, d + m)
                })
            }
            Primitiva::Sprite { caixa, .. } => {
                Some((caixa.x * t.w, caixa.y * t.h, (caixa.x + caixa.w) * t.w, (caixa.y + caixa.h) * t.h))
            }
            Primitiva::Atlas { celula, texto, lugar, altura, .. } => {
                let (x, y, w, h, _) = caixa_atlas(*celula, texto, *lugar, *altura, canvas);
                Some((x, y, x + w, y + h))
            }
            Primitiva::Regiao { forma, borda_suave, inverter, .. } => {
                if *inverter {
                    Some((0.0, 0.0, t.w, t.h))
                } else {
                    envolvente_forma(forma, t.w, t.h).map(|(a, b, c, d)| {
                        let m = borda_suave.max(0.0) * (c - a).min(d - b) + 1.0;
                        (a - m, b - m, c + m, d + m)
                    })
                }
            }
            Primitiva::Lupa { destino, .. } => {
                Some((destino.x * t.w, destino.y * t.h, (destino.x + destino.w) * t.w, (destino.y + destino.h) * t.h))
            }
        };
        if let Some((a, b, c, d)) = b.filter(|b| b.0.is_finite() && b.1.is_finite() && b.2.is_finite() && b.3.is_finite()) {
            u = Some(match u {
                None => (a, b, c, d),
                Some((e, f, g, h)) => (e.min(a), f.min(b), g.max(c), h.max(d)),
            });
        }
    }
    u.map(|(a, b, c, d)| t.caixa(a, b, c - a, d - b))
}

// ---------------------------------------------------------------- desenho

/// Imagens que as primitivas citam (sprites e atlas do cache, `midia:<id>` das imagens do pedido), lidas uma vez.
pub(crate) struct Recursos<'a> {
    cache: &'a Path,
    imagens: &'a [ImagemCamada],
    lidos: Mutex<HashMap<String, Arc<Pixmap>>>,
}

impl<'a> Recursos<'a> {
    pub(crate) fn novo(cache: &'a Path, imagens: &'a [ImagemCamada]) -> Self {
        Recursos { cache, imagens, lidos: Mutex::new(HashMap::new()) }
    }

    pub(crate) fn pixmap(&self, src: &str) -> Result<Arc<Pixmap>> {
        if let Some(p) = self.lidos.lock().unwrap_or_else(|e| e.into_inner()).get(src) {
            return Ok(p.clone());
        }
        let (caminho, esperado) = match src.strip_prefix("midia:") {
            Some(id) => self
                .imagens
                .iter()
                .find(|i| i.midia == id)
                .map(|i| (i.caminho.clone(), Some(i.tamanho)))
                .ok_or_else(|| SicroError::Validation(format!("{}: {id}", codigo::MIDIA_AUSENTE)))?,
            None => {
                let rel = Path::new(src);
                if rel.is_absolute() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
                    return Err(invalido("src"));
                }
                (self.cache.join(rel), None)
            }
        };
        let img = ler_imagem(&caminho)?;
        let (w, h) = img.dimensions();
        // tamanho diferente do que o pedido usou na geometria esticaria a imagem
        if esperado.is_some_and(|t| (t.w, t.h) != (w, h)) {
            return Err(invalido("tamanho da imagem"));
        }
        let p = Arc::new(pixmap_de_reto(w, h, img.into_raw())?);
        self.lidos.lock().unwrap_or_else(|e| e.into_inner()).insert(src.to_string(), p.clone());
        Ok(p)
    }

    /// A imagem para desenhar com escala `s`, e o lado `k` do bloco em que foi reduzida (1 = a original; cada
    /// pixel devolvido vale `k` da original). Abaixo de 0,5× o tiny-skia pula pixels: serrilha e cria listras falsas.
    pub(crate) fn pixmap_para(&self, src: &str, s: f64) -> Result<(Arc<Pixmap>, u32)> {
        let k = bloco_de_reducao(s);
        if k == 1 {
            return Ok((self.pixmap(src)?, 1));
        }
        let chave = format!("{src}\u{0}{k}");
        if let Some(p) = self.lidos.lock().unwrap_or_else(|e| e.into_inner()).get(&chave) {
            return Ok((p.clone(), k));
        }
        let p = Arc::new(reduzir(self.pixmap(src)?.as_ref(), k)?);
        self.lidos.lock().unwrap_or_else(|e| e.into_inner()).insert(chave, p.clone());
        Ok((p, k))
    }
}

/// Lê a imagem pelo conteúdo (não pela extensão) e aplica a orientação do EXIF, como o navegador da prévia:
/// foto de celular sai em pé. `ImagemCamada.tamanho` é o tamanho já orientado.
fn ler_imagem(caminho: &Path) -> Result<image::RgbaImage> {
    use image::ImageDecoder;
    let erro = |e: &dyn fmt::Display| SicroError::Filesystem(format!("{}: {e}", caminho.display()));
    let leitor = image::ImageReader::open(caminho).map_err(|e| erro(&e))?.with_guessed_format().map_err(|e| erro(&e))?;
    let mut dec = leitor.into_decoder().map_err(|e| erro(&e))?;
    // EXIF ilegível: fica como está
    let orientacao = dec.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(dec).map_err(|e| erro(&e))?;
    img.apply_orientation(orientacao);
    Ok(img.to_rgba8())
}

/// Maior bloco da redução prévia (px).
const REDUCAO_MAX: u32 = 64;

/// Lado do bloco da redução prévia para a escala `s`: o maior que ainda deixa a escala restante em 0,5× ou mais.
fn bloco_de_reducao(s: f64) -> u32 {
    if s.is_finite() && s > 0.0 && s < 0.5 {
        ((1.0 / s).floor() as u32).clamp(2, REDUCAO_MAX)
    } else {
        1
    }
}

/// Média de blocos k×k (pré-multiplicado, só inteiros: mesmos bytes em toda máquina). O bloco cortado pela
/// borda tira a média do que tem: borda opaca continua opaca.
fn reduzir(p: &Pixmap, k: u32) -> Result<Pixmap> {
    let (w, h, k) = (p.width() as usize, p.height() as usize, k as usize);
    let (rw, rh) = (w.div_ceil(k), h.div_ceil(k));
    let d = p.data();
    let mut px = vec![0u8; rw * rh * 4];
    for oy in 0..rh {
        let (y0, y1) = (oy * k, ((oy + 1) * k).min(h));
        for ox in 0..rw {
            let (x0, x1) = (ox * k, ((ox + 1) * k).min(w));
            let mut soma = [0u32; 4];
            for y in y0..y1 {
                for q in d[(y * w + x0) * 4..(y * w + x1) * 4].as_chunks::<4>().0 {
                    for c in 0..4 {
                        soma[c] += q[c] as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            let o = (oy * rw + ox) * 4;
            for c in 0..4 {
                px[o + c] = ((soma[c] + n / 2) / n) as u8;
            }
            // a média arredondada não pode passar do alfa (pré-multiplicado)
            let a = px[o + 3];
            for c in 0..3 {
                px[o + c] = px[o + c].min(a);
            }
        }
    }
    IntSize::from_wh(rw as u32, rh as u32).and_then(|s| Pixmap::from_vec(px, s)).ok_or_else(|| invalido("imagem vazia"))
}

/// RGBA de alfa reto → `Pixmap` (pré-multiplicado).
pub(crate) fn pixmap_de_reto(w: u32, h: u32, mut px: Vec<u8>) -> Result<Pixmap> {
    for p in px.as_chunks_mut::<4>().0 {
        let a = p[3] as u32;
        if a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }
    IntSize::from_wh(w, h).and_then(|s| Pixmap::from_vec(px, s)).ok_or_else(|| invalido("imagem vazia"))
}

/// `Pixmap` (pré-multiplicado) → RGBA de alfa reto, o do PNG, com a cor sangrada para os pixels de alfa 0 vizinhos.
pub(crate) fn bitmap_reto(pm: Pixmap) -> Bitmap {
    let (w, h) = (pm.width(), pm.height());
    let mut px = pm.take();
    for p in px.as_chunks_mut::<4>().0 {
        let a = p[3] as u32;
        if a == 0 {
            p[..3].fill(0);
        } else if a < 255 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    sangrar(&mut px, w as usize, h as usize);
    Bitmap { w, h, canais: 4, px }
}

/// Alcance (px) da cor nos pixels de alfa 0 em volta do desenho: o filtro de croma do `scale` (RGBA → yuva420p,
/// bicúbico) lê até 3,5 linhas além do bloco 2×2 e não olha o alfa.
const SANGRIA: u8 = 4;

/// Leva a cor do desenho aos pixels de alfa 0 a até `SANGRIA` px dele (o alfa não muda: nada aparece a mais).
/// Sem isso o preto transparente entra na média do croma e a borda de toda forma chega ao vídeo dessaturada
/// (medido: até 28/255 em V numa borda vermelha). Cada volta lê só as anteriores: não depende da ordem.
fn sangrar(px: &mut [u8], w: usize, h: usize) {
    const VAZIO: u8 = u8::MAX;
    const FORA: u8 = u8::MAX - 1;
    if w == 0 || h == 0 {
        return;
    }
    // volta de cada pixel, com moldura de 1 px (FORA) para os vizinhos não testarem a borda:
    // 0 = desenho (alfa > 0); k = pintado na volta k
    let lw = w + 2;
    let n = lw * (h + 2);
    let mut volta = vec![FORA; n];
    let mut tem = vec![0u8; n];
    for y in 0..h {
        let l = (y + 1) * lw + 1;
        for (x, p) in px[y * w * 4..(y + 1) * w * 4].as_chunks::<4>().0.iter().enumerate() {
            let d = (p[3] > 0) as u8;
            tem[l + x] = d;
            volta[l + x] = if d == 1 { 0 } else { VAZIO };
        }
    }
    // desenho a até 1 px (8 vizinhos): espalha na linha, depois na coluna
    let mut na_linha = vec![0u8; n];
    for i in 1..n - 1 {
        na_linha[i] = tem[i - 1] | tem[i] | tem[i + 1];
    }
    // 1ª frente: vazio que encosta no desenho; cada item = (índice com moldura, índice do pixel)
    let mut frente: Vec<(u32, u32)> = Vec::new();
    for y in 0..h {
        let l = (y + 1) * lw + 1;
        for x in 0..w {
            let i = l + x;
            if volta[i] == VAZIO && (na_linha[i - lw] | na_linha[i] | na_linha[i + lw]) != 0 {
                volta[i] = 1;
                frente.push((i as u32, (y * w + x) as u32));
            }
        }
    }
    let (lwi, wi) = (lw as isize, w as isize);
    let vizinhos: [(isize, isize); 8] =
        [(-lwi - 1, -wi - 1), (-lwi, -wi), (-lwi + 1, -wi + 1), (-1, -1), (1, 1), (lwi - 1, wi - 1), (lwi, wi), (lwi + 1, wi + 1)];
    for k in 1..=SANGRIA {
        let mut proxima = Vec::new();
        for &(i, u) in &frente {
            let (i, u) = (i as isize, u as isize);
            // média dos vizinhos das voltas anteriores; pixel do desenho pesa pelo alfa, pintado pesa 1
            let (mut s, mut peso) = ([0u32; 3], 0u32);
            for (di, du) in vizinhos {
                let v = volta[(i + di) as usize];
                if v < k {
                    let j = (u + du) as usize * 4;
                    let p = if v == 0 { px[j + 3] as u32 } else { 1 };
                    for c in 0..3 {
                        s[c] += p * px[j + c] as u32;
                    }
                    peso += p;
                }
            }
            let o = u as usize * 4;
            for c in 0..3 {
                px[o + c] = ((s[c] + peso / 2) / peso) as u8;
            }
            if k < SANGRIA {
                for (di, du) in vizinhos {
                    let j = (i + di) as usize;
                    if volta[j] == VAZIO {
                        volta[j] = k + 1;
                        proxima.push((j as u32, (u + du) as u32));
                    }
                }
            }
        }
        frente = proxima;
    }
}

pub(crate) fn pixmap_vazio(caixa_px: [u32; 4]) -> Result<Pixmap> {
    Pixmap::new(caixa_px[2], caixa_px[3]).ok_or_else(|| invalido("caixa_px vazia"))
}

fn pintura(cor: &str) -> Result<Paint<'static>> {
    let c = cor::ler(cor)?;
    let mut p = Paint { anti_alias: true, ..Paint::default() };
    p.set_color_rgba8(c.r, c.g, c.b, c.a);
    Ok(p)
}

/// Desenha as primitivas num RGBA do tamanho de `caixa_px` ([x, y, w, h] em px do canvas).
/// `cache` resolve o `src` de sprites e atlas; `imagens`, o `src` "midia:<id>". `regiao` e `lupa` não são desenhadas aqui (são do ffmpeg).
pub fn desenhar(
    primitivas: &[Primitiva],
    canvas: TamanhoPx,
    caixa_px: [u32; 4],
    cache: &std::path::Path,
    imagens: &[ImagemCamada],
) -> Result<Bitmap> {
    let r = Recursos::novo(cache, imagens);
    let mut pm = pixmap_vazio(caixa_px)?;
    pintar(&mut pm, primitivas, canvas, caixa_px, &r, None, 1.0)?;
    Ok(bitmap_reto(pm))
}

/// Pinta em `pm` (a caixa `caixa_px` do canvas). `extra` = transformação do canvas antes do recorte (sobreposto);
/// com `extra` ou opacidade < 1 o grupo é pintado à parte e composto com a opacidade.
pub(crate) fn pintar(
    pm: &mut Pixmap,
    primitivas: &[Primitiva],
    canvas: TamanhoPx,
    caixa_px: [u32; 4],
    r: &Recursos,
    extra: Option<Transform>,
    opacidade: f64,
) -> Result<()> {
    if opacidade < 1.0 {
        let mut grupo = pixmap_vazio(caixa_px)?;
        pintar(&mut grupo, primitivas, canvas, caixa_px, r, extra, 1.0)?;
        let paint = PixmapPaint { opacity: opacidade.clamp(0.0, 1.0) as f32, ..PixmapPaint::default() };
        pm.draw_pixmap(0, 0, grupo.as_ref(), &paint, Transform::identity(), None);
        return Ok(());
    }
    let t = Tela::de(canvas);
    let recorte = Transform::from_translate(-(caixa_px[0] as f32), -(caixa_px[1] as f32));
    let ts = extra.map(|e| e.post_concat(recorte)).unwrap_or(recorte);
    for p in primitivas {
        match p {
            Primitiva::Caminho { d, traco, preenche, regra } => {
                let mut pb = PathBuilder::new();
                for s in d {
                    match *s {
                        Segmento::M(x, y) => pb.move_to((x * t.w) as f32, (y * t.h) as f32),
                        Segmento::L(x, y) => pb.line_to((x * t.w) as f32, (y * t.h) as f32),
                        Segmento::C(a, b, c, e, x, y) => pb.cubic_to(
                            (a * t.w) as f32,
                            (b * t.h) as f32,
                            (c * t.w) as f32,
                            (e * t.h) as f32,
                            (x * t.w) as f32,
                            (y * t.h) as f32,
                        ),
                        Segmento::Z => pb.close(),
                    }
                }
                let Some(path) = pb.finish() else { continue };
                if let Some(c) = preenche {
                    let regra = if regra.is_some() { FillRule::EvenOdd } else { FillRule::Winding };
                    pm.fill_path(&path, &pintura(c)?, regra, ts, None);
                }
                tracar(pm, &path, traco, t.h, ts)?;
            }
            Primitiva::Elipse { cx, cy, rx, ry, traco, preenche } => {
                let (rx, ry) = (rx.abs() * t.w, ry.abs() * t.h);
                let Some(path) = Rect::from_xywh((cx * t.w - rx) as f32, (cy * t.h - ry) as f32, (2.0 * rx) as f32, (2.0 * ry) as f32)
                    .and_then(PathBuilder::from_oval)
                else {
                    continue;
                };
                if let Some(c) = preenche {
                    pm.fill_path(&path, &pintura(c)?, FillRule::Winding, ts, None);
                }
                tracar(pm, &path, traco, t.h, ts)?;
            }
            Primitiva::Sprite { src, caixa, opacidade } => {
                let img = r.pixmap(src)?;
                let (iw, ih) = (img.width() as f64, img.height() as f64);
                let quase = |v: f64| if (v - v.round()).abs() < 1e-6 { v.round() } else { v };
                let (x, y, w, h) = (quase(caixa.x * t.w), quase(caixa.y * t.h), quase(caixa.w * t.w), quase(caixa.h * t.h));
                // muito reduzida: parte da imagem já reduzida por blocos (iw, ih seguem os da original)
                let escala_extra = extra.map(|e| ((e.sx * e.sy - e.kx * e.ky).abs() as f64).sqrt()).unwrap_or(1.0);
                let (img, k) = r.pixmap_para(src, (w / iw).max(h / ih) * escala_extra)?;
                let (iw, ih) = (iw / k as f64, ih / k as f64);
                let opacity = opacidade.clamp(0.0, 1.0) as f32;
                // tamanho igual e posição inteira: cópia sem reamostrar
                let inteiro = extra.is_none() && w == iw && h == ih && x == x.floor() && y == y.floor();
                if inteiro {
                    let paint = PixmapPaint { opacity, quality: FilterQuality::Nearest, ..PixmapPaint::default() };
                    pm.draw_pixmap(x as i32 - caixa_px[0] as i32, y as i32 - caixa_px[1] as i32, tiny_skia::Pixmap::as_ref(&img), &paint, Transform::identity(), None);
                } else if w > 0.0 && h > 0.0 {
                    let paint = PixmapPaint { opacity, quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
                    let local = Transform::from_row((w / iw) as f32, 0.0, 0.0, (h / ih) as f32, x as f32, y as f32);
                    pm.draw_pixmap(0, 0, tiny_skia::Pixmap::as_ref(&img), &paint, local.post_concat(ts), None);
                }
            }
            Primitiva::Atlas { src, celula, glifos, texto, lugar, altura } => {
                let img = r.pixmap(src)?;
                let (x, y, _, h, e) = caixa_atlas(*celula, texto, *lugar, *altura, canvas);
                if e.is_nan() || e <= 0.0 || celula.w == 0 {
                    continue;
                }
                let indice: HashMap<char, usize> = glifos.chars().enumerate().map(|(i, c)| (c, i)).collect();
                let cw = celula.w as f64 * e;
                let qualidade = if e == 1.0 && extra.is_none() { FilterQuality::Nearest } else { FilterQuality::Bilinear };
                for (k, c) in texto.chars().enumerate() {
                    let Some(&i) = indice.get(&c) else { continue };
                    let dx = x + k as f64 * cw;
                    let Some(alvo) = Rect::from_xywh(dx as f32, y as f32, cw as f32, h as f32) else { continue };
                    // célula i do atlas cai em (dx, y)
                    let padrao = Transform::from_row(e as f32, 0.0, 0.0, e as f32, (dx - i as f64 * cw) as f32, y as f32);
                    let paint = Paint {
                        shader: Pattern::new(tiny_skia::Pixmap::as_ref(&img), SpreadMode::Pad, qualidade, 1.0, padrao),
                        anti_alias: false,
                        ..Paint::default()
                    };
                    pm.fill_rect(alvo, &paint, ts, None);
                }
            }
            Primitiva::Regiao { .. } | Primitiva::Lupa { .. } => {}
        }
    }
    Ok(())
}

fn tracar(pm: &mut Pixmap, path: &tiny_skia::Path, traco: &Option<Traco2>, h: f64, ts: Transform) -> Result<()> {
    let Some(t) = traco else { return Ok(()) };
    let largura = t.largura * h;
    if largura.is_nan() || largura <= 0.0 {
        return Ok(());
    }
    let s = |w: f64| Stroke { width: w as f32, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
    if let Some(c) = &t.contorno {
        pm.stroke_path(path, &pintura(c)?, &s(largura * CONTORNO), ts, None);
    }
    pm.stroke_path(path, &pintura(&t.cor)?, &s(largura), ts, None);
    Ok(())
}

/// Transformação (px do canvas) de um objeto `ow`×`oh` px posto pela `Transformacao` avaliada: centro em (x, y),
/// largura `escala` × largura do canvas, girado `rotacao` graus no sentido horário da tela.
pub(crate) fn transformacao(x: f64, y: f64, escala: f64, rotacao: f64, ow: f64, oh: f64, canvas: TamanhoPx) -> Transform {
    let t = Tela::de(canvas);
    let s = if ow > 0.0 { escala * t.w / ow } else { 0.0 };
    // horário com y para baixo = anti-horário da conta
    let (c, sn) = cos_sen_graus(-rotacao);
    let (a, b, cc, d) = (s * c, -s * sn, s * sn, s * c);
    let (hx, hy) = (ow * 0.5, oh * 0.5);
    let (tx, ty) = (x * t.w - (a * hx + cc * hy), y * t.h - (b * hx + d * hy));
    Transform::from_row(a as f32, b as f32, cc as f32, d as f32, tx as f32, ty as f32)
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn primitiva_tag_k() {
        let casos = vec![
            json!({ "k": "caminho", "d": [["M", 0.1, 0.2], ["L", 0.3, 0.2], ["C", 0.3, 0.3, 0.2, 0.4, 0.1, 0.4], ["Z"]],
                    "traco": { "cor": "#ffcc00", "largura": 0.006, "contorno": "#000000" }, "regra": "evenodd" }),
            json!({ "k": "elipse", "cx": 0.5, "cy": 0.5, "rx": 0.1, "ry": 0.05, "preenche": "#ff000080" }),
            json!({ "k": "sprite", "src": "sprites/ab.png", "caixa": { "x": 0.1, "y": 0.8, "w": 0.8, "h": 0.1 }, "opacidade": 1.0 }),
            json!({ "k": "atlas", "src": "atlas/cd.png", "celula": { "w": 20, "h": 36 }, "glifos": "0123456789:,", "texto": "00:00:12,345",
                    "lugar": { "x": 0.984, "y": 0.028, "h": "dir", "v": "topo" }, "altura": 0.033 }),
            json!({ "k": "regiao", "modo": "solido", "forma": { "k": "elipse", "cx": 0.5, "cy": 0.5, "rx": 0.1, "ry": 0.1 },
                    "intensidade": 1.0, "cor": "#000000ff", "borda_suave": 0.0, "inverter": false, "espaco": "fonte" }),
            json!({ "k": "lupa", "origem": { "x": 0.4, "y": 0.4, "w": 0.1, "h": 0.1 }, "destino": { "x": 0.7, "y": 0.1, "w": 0.25, "h": 0.25 }, "forma": "circulo" }),
        ];
        for j in casos {
            let p: Primitiva = serde_json::from_value(j.clone()).expect("lê");
            assert_eq!(serde_json::to_value(&p).unwrap(), j);
        }
        assert!(serde_json::from_value::<Segmento>(json!(["M", 1.0])).is_err());
        assert!(serde_json::from_value::<Segmento>(json!(["Q", 1.0, 2.0])).is_err());
    }

    use std::cell::RefCell;

    use crate::edicao::curva::fixtures;
    use crate::edicao::geometria::a_de;
    use crate::edicao::modelo::{EstiloTexto, Tolerante};
    use crate::edicao::camada::texto::{Atlas, Sprite};
    use serde_json::Value;

    /// Fonte falsa e determinística; guarda os pedidos e as respostas (vão para a fixture: o espelho TS repete).
    struct Falso {
        imagens: BTreeMap<String, TamanhoPx>,
        sprites: RefCell<Vec<Value>>,
        atlas: RefCell<Vec<Value>>,
    }

    impl Falso {
        fn novo() -> Self {
            let mut imagens = BTreeMap::new();
            imagens.insert("logo".to_string(), TamanhoPx { w: 400, h: 300 });
            Falso { imagens, sprites: RefCell::new(Vec::new()), atlas: RefCell::new(Vec::new()) }
        }
    }

    impl FonteSprites for Falso {
        fn sprite(&self, p: &PedidoSprite) -> Result<Sprite> {
            let n: usize = p.blocos.iter().map(|b| b.conteudo.chars().count()).sum();
            let tam = p.blocos.first().map(|b| b.estilo.tamanho).unwrap_or(0.05) * p.altura_canvas as f64;
            let w = ((n as f64 * tam * 0.55).round() as u32).min((p.largura_max * p.altura_canvas as f64) as u32).max(1);
            let h = (tam * 1.25).round() as u32 * p.blocos.len().max(1) as u32;
            let s = Sprite { relativo: format!("sprites/falso-{w}x{h}.png"), w, h };
            self.sprites.borrow_mut().push(json!({ "pedido": p, "sprite": s }));
            Ok(s)
        }
        fn atlas(&self, estilo: &EstiloTexto, altura_canvas: u32) -> Result<Atlas> {
            let tam = estilo.tamanho * altura_canvas as f64;
            let a = Atlas {
                relativo: "atlas/falso.png".into(),
                celula: TamanhoPx { w: (tam * 0.62).ceil() as u32, h: (tam * 1.17).ceil() as u32 },
                glifos: crate::edicao::camada::texto::GLIFOS_CARIMBO.into(),
            };
            self.atlas.borrow_mut().push(json!({ "estilo": estilo, "altura_canvas": altura_canvas, "atlas": a }));
            Ok(a)
        }
        fn imagem(&self, midia: &str) -> Option<TamanhoPx> {
            self.imagens.get(midia).copied()
        }
    }

    const CANVAS: TamanhoPx = TamanhoPx { w: 1920, h: 1080 };

    fn quadro(canvas: TamanhoPx, a: Afim, rastreios: &[(&str, [f64; 4])]) -> QuadroCamada {
        QuadroCamada {
            q: 120,
            t: 3723.4565,
            pts: Some(57_210_000),
            a,
            canvas,
            rastreios: rastreios.iter().map(|(k, c)| (k.to_string(), Caixa { x: c[0], y: c[1], w: c[2], h: c[3] })).collect(),
            indice: Some(312),
            programa_s: 4.004,
        }
    }

    fn a_43() -> Afim {
        a_de(&[], 0.0, TamanhoPx { w: 1440, h: 1080 }, CANVAS)
    }

    fn a_girado() -> Afim {
        let girar: Efeito = serde_json::from_value(json!({ "tipo": "girar", "id": "g", "ligado": true, "nome": "", "ativo": null,
            "graus": 90, "espelhar_h": false, "espelhar_v": false })).unwrap();
        a_de(&[Tolerante::Conhecido(girar)], 0.0, TamanhoPx { w: 1280, h: 720 }, CANVAS)
    }

    fn base(tipo: &str, id: &str) -> Value {
        json!({ "tipo": tipo, "id": id, "ligado": true, "nome": "", "ativo": null })
    }

    fn com(mut v: Value, extra: Value) -> Value {
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    }

    fn estilo(fonte: &str, caixa: bool, contorno: bool) -> Value {
        json!({ "fonte": fonte, "negrito": false, "italico": false, "tamanho": 0.033, "cor": "#ffffff", "alinhamento": "centro",
                "caixa": if caixa { json!({ "cor": "#0000008c", "margem": 0.01, "raio": 0.005 }) } else { Value::Null },
                "contorno": if contorno { json!({ "cor": "#000000", "largura": 0.002 }) } else { Value::Null } })
    }

    fn destaque(forma: &str, extra: Value) -> Value {
        com(
            com(base("destaque", "d1"), json!({ "forma": forma, "rastreio": "r1",
                "traco": { "cor": "#ffcc00", "espessura": 0.006, "contorno": true }, "tamanho": 0.12, "angulo_graus": 45.0,
                "margem": 0.05, "rotulo": null })),
            extra,
        )
    }

    fn alvo(forma: &str, extra: Value) -> Value {
        com(json!({ "rastreio": "r1", "forma": forma, "vertices": null, "margem": 0.1, "borda_suave": 0.0, "inverter": false }), extra)
    }

    fn ocultar(modo: &str, alvo: Value) -> Value {
        com(base("ocultar", "o1"), json!({ "modo": modo, "alvo": alvo, "intensidade": 0.6, "cor": "#000000ff", "conteudo": "placa", "motivo": "" }))
    }

    fn carimbo(mostra: &str, formato: &str, relogio: Value, caixa: bool) -> Value {
        com(base("carimbo", "k1"), json!({ "mostra": mostra, "formato": formato, "relogio": relogio,
            "estilo": estilo("mono", caixa, false), "lugar": { "x": 0.984, "y": 0.028, "h": "dir", "v": "topo" } }))
    }

    const CAIXA: [f64; 4] = [0.4, 0.3, 0.1, 0.2];
    const PONTO: [f64; 4] = [0.5, 0.5, 0.0, 0.0];

    /// Casos da fixture: (nome, efeito, quadro).
    fn casos() -> Vec<(String, Value, QuadroCamada)> {
        let id = Afim::IDENTIDADE;
        let q_caixa = quadro(CANVAS, id, &[("r1", CAIXA)]);
        let q_ponto = quadro(CANVAS, id, &[("r1", PONTO)]);
        let mut v: Vec<(String, Value, QuadroCamada)> = vec![
            ("destaque circulo caixa".into(), destaque("circulo", json!({})), q_caixa.clone()),
            ("destaque circulo ponto".into(), destaque("circulo", json!({})), q_ponto.clone()),
            ("destaque elipse caixa 4:3 em 16:9".into(), destaque("elipse", json!({})), quadro(CANVAS, a_43(), &[("r1", CAIXA)])),
            ("destaque elipse ponto sem contorno".into(), destaque("elipse", json!({ "traco": { "cor": "#ff0000", "espessura": 0.01, "contorno": false } })), q_ponto.clone()),
            ("destaque retangulo caixa margem".into(), destaque("retangulo", json!({ "margem": 0.1 })), q_caixa.clone()),
            ("destaque retangulo ponto".into(), destaque("retangulo", json!({})), q_ponto.clone()),
            ("destaque colchetes caixa".into(), destaque("colchetes", json!({})), q_caixa.clone()),
            ("destaque colchetes ponto girado".into(), destaque("colchetes", json!({})), quadro(CANVAS, a_girado(), &[("r1", [0.2, 0.7, 0.0, 0.0])])),
            ("destaque seta caixa 135".into(), destaque("seta", json!({ "angulo_graus": 135.0 })), q_caixa.clone()),
            ("destaque seta caixa 0 girado".into(), destaque("seta", json!({ "angulo_graus": 0.0 })), quadro(CANVAS, a_girado(), &[("r1", CAIXA)])),
            ("destaque sem rastreio no quadro".into(), destaque("circulo", json!({})), quadro(CANVAS, id, &[("outro", CAIXA)])),
            ("destaque desligado".into(), destaque("circulo", json!({ "ligado": false })), q_caixa.clone()),
        ];
        for g in [45.0, 0.0, 90.0, 180.0, 270.0, -30.0, 400.0, 137.5, 359.9999999] {
            v.push((format!("destaque seta ponto {g}"), destaque("seta", json!({ "angulo_graus": g })), q_ponto.clone()));
        }
        for lado in ["cima", "baixo", "esq", "dir"] {
            let rotulo = json!({ "texto": "Veículo 1", "estilo": estilo("sans", true, false), "lado": lado });
            v.push((format!("destaque rotulo {lado}"), destaque("circulo", json!({ "rotulo": rotulo })), q_caixa.clone()));
        }
        v.push((
            "destaque seta rotulo vazio".into(),
            destaque("seta", json!({ "rotulo": { "texto": "", "estilo": estilo("sans", false, false), "lado": "cima" } })),
            q_ponto.clone(),
        ));
        let poligono = json!([{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.2 }, { "x": 0.6, "y": 1.0 }]);
        v.extend([
            ("ocultar solido retangulo".into(), ocultar("solido", alvo("retangulo", json!({}))), q_caixa.clone()),
            ("ocultar desfoque elipse borda".into(), ocultar("desfoque", alvo("elipse", json!({ "borda_suave": 0.25 }))), q_caixa.clone()),
            ("ocultar mosaico poligono".into(), ocultar("mosaico", alvo("poligono", json!({ "vertices": poligono }))), q_caixa.clone()),
            ("ocultar inverter margem negativa".into(), ocultar("solido", alvo("retangulo", json!({ "inverter": true, "margem": -0.2 }))), q_caixa.clone()),
            ("ocultar fora da imagem".into(), ocultar("solido", alvo("retangulo", json!({}))), quadro(CANVAS, id, &[("r1", [0.95, -0.05, 0.1, 0.1])])),
            ("ocultar poligono sem vertices".into(), ocultar("solido", alvo("poligono", json!({}))), q_caixa.clone()),
            ("ocultar silhueta".into(), ocultar("solido", alvo("silhueta", json!({}))), q_caixa.clone()),
            (
                "holofote elipse girado".into(),
                com(base("holofote", "h1"), json!({ "alvo": alvo("elipse", json!({ "borda_suave": 0.1 })), "escurecer": 0.7, "dessaturar": 0.5 })),
                quadro(CANVAS, a_girado(), &[("r1", CAIXA)]),
            ),
            (
                "holofote poligono 4:3 invertido".into(),
                com(base("holofote", "h1"), json!({ "alvo": alvo("poligono", json!({ "vertices": poligono, "inverter": true })), "escurecer": 0.5, "dessaturar": 0.0 })),
                quadro(CANVAS, a_43(), &[("r1", CAIXA)]),
            ),
        ]);
        let texto = |conteudo: &str, e: Value, lugar: Value| {
            com(base("texto", "t1"), json!({ "conteudo": conteudo, "estilo": e, "lugar": lugar, "largura_max": 0.8 }))
        };
        v.extend([
            (
                "texto dir base com caixa e contorno".into(),
                texto("Câmera 2 — saída", estilo("sans", true, true), json!({ "x": 0.95, "y": 0.9, "h": "dir", "v": "base" })),
                q_caixa.clone(),
            ),
            ("texto centro meio".into(), texto("#import *a* $x$", estilo("serifa", false, false), json!({ "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" })), q_caixa.clone()),
            ("texto vazio".into(), texto("", estilo("sans", false, false), json!({ "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" })), q_caixa.clone()),
            (
                "texto vertical".into(),
                texto("Vertical", estilo("mono", false, false), json!({ "x": 0.1, "y": 0.1, "h": "esq", "v": "topo" })),
                quadro(TamanhoPx { w: 1080, h: 1920 }, id, &[]),
            ),
        ]);
        let mut q_neg = q_caixa.clone();
        q_neg.t = -1.5;
        let mut q_sem = q_caixa.clone();
        q_sem.indice = None;
        let rel = |data: Value| json!({ "media_time_s": 3600.0, "clock_seconds": 86300.0, "clock_date": data });
        v.extend([
            ("carimbo tempo_fonte hms_ms".into(), carimbo("tempo_fonte", "hms_ms", Value::Null, false), q_caixa.clone()),
            ("carimbo tempo_fonte hms".into(), carimbo("tempo_fonte", "hms", Value::Null, false), q_caixa.clone()),
            ("carimbo tempo_fonte data_hora".into(), carimbo("tempo_fonte", "data_hora", Value::Null, false), q_caixa.clone()),
            ("carimbo tempo negativo".into(), carimbo("tempo_fonte", "hms_ms", Value::Null, false), q_neg),
            ("carimbo relogio hms_ms virando o dia".into(), carimbo("relogio", "hms_ms", rel(Value::Null), false), q_caixa.clone()),
            ("carimbo relogio data_hora virando o mes".into(), carimbo("relogio", "data_hora", rel(json!("2028-02-28")), false), q_caixa.clone()),
            ("carimbo relogio data_hora sem data".into(), carimbo("relogio", "data_hora", rel(Value::Null), false), q_caixa.clone()),
            ("carimbo relogio sem retrato".into(), carimbo("relogio", "hms", Value::Null, false), q_caixa.clone()),
            ("carimbo quadro".into(), carimbo("quadro", "hms_ms", Value::Null, false), q_caixa.clone()),
            ("carimbo quadro sem indice".into(), carimbo("quadro", "hms_ms", Value::Null, false), q_sem),
            ("carimbo tempo_programa com caixa".into(), carimbo("tempo_programa", "hms_ms", Value::Null, true), q_caixa.clone()),
            (
                "imagem centro meio".into(),
                com(base("imagem", "i1"), json!({ "midia": "logo", "lugar": { "x": 0.5, "y": 0.5, "h": "centro", "v": "meio" }, "largura": 0.25, "opacidade": 0.8 })),
                q_caixa.clone(),
            ),
            (
                "imagem ausente".into(),
                com(base("imagem", "i1"), json!({ "midia": "nada", "lugar": { "x": 0.0, "y": 0.0, "h": "esq", "v": "topo" }, "largura": 0.25, "opacidade": 1.0 })),
                q_caixa.clone(),
            ),
            ("ajustes".into(), com(base("ajustes", "a1"), json!({ "brilho": 0.1, "contraste": 1.0, "gama": 1.0, "saturacao": 1.0 })), q_caixa.clone()),
            (
                "lupa".into(),
                com(base("lupa", "l1"), json!({ "rastreio": "r1", "janela": 0.2, "fator": 2.0, "forma": "circulo", "interpolacao": "bilinear",
                    "onde": { "modo": "fixa", "x": 0.8, "y": 0.2 }, "moldura": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "ligacao": true })),
                q_caixa.clone(),
            ),
            (
                "lupa desligada".into(),
                com(base("lupa", "l1"), json!({ "ligado": false, "rastreio": "r1", "janela": 0.2, "fator": 2.0, "forma": "circulo", "interpolacao": "bilinear",
                    "onde": { "modo": "fixa", "x": 0.8, "y": 0.2 }, "moldura": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "ligacao": true })),
                q_caixa.clone(),
            ),
            (
                "regua".into(),
                com(base("regua", "g1"), json!({ "calibracao": { "id": "c", "metodo": "plane", "h": [1, 0, 0, 0, 1, 0, 0, 0, 1], "espaco": { "w": 1920, "h": 1080 },
                    "pontos_sha256": "", "residuo_m": 0.01, "regiao": [] }, "ancora": { "x": 0.5, "y": 0.5 }, "direcao_graus": 0.0, "comprimento_m": 1.0,
                    "divisoes": 10, "traco": { "cor": "#ffffff", "espessura": 0.004, "contorno": true }, "rotulo": true })),
                q_caixa.clone(),
            ),
            (
                "velocidade_exibida".into(),
                com(base("velocidade_exibida", "v1"), json!({ "calculo": { "id": "c", "kmh": 42.0, "faixa": [39.0, 45.0], "de_s": 0.0, "ate_s": 1.0, "metodo": "x" },
                    "onde": { "modo": "fixa", "lugar": { "x": 0.5, "y": 0.9, "h": "centro", "v": "base" } }, "estilo": estilo("sans", false, false) })),
                q_caixa.clone(),
            ),
        ]);
        v
    }

    /// Código do erro de validação (antes do ":").
    fn codigo_do_erro(e: &SicroError) -> String {
        match e {
            SicroError::Validation(m) => m.split(':').next().unwrap_or_default().trim().to_string(),
            outro => panic!("erro inesperado: {outro}"),
        }
    }

    fn rodar_caso(efeito: &Value, q: &QuadroCamada) -> Value {
        let e: Efeito = serde_json::from_value(efeito.clone()).expect("efeito lido");
        let f = Falso::novo();
        let saida = match primitivas(&e, q, &f) {
            Ok(p) => json!({ "primitivas": p }),
            Err(e) => json!({ "erro": codigo_do_erro(&e) }),
        };
        json!({
            "entrada": { "efeito": efeito, "quadro": q, "imagens": f.imagens, "sprites": f.sprites.take(), "atlas": f.atlas.take() },
            "saida": saida,
        })
    }

    #[test]
    fn fixture_primitivas_efeitos() {
        let mut lista = Vec::new();
        for (nome, ef, q) in casos() {
            let mut c = rodar_caso(&ef, &q);
            c["nome"] = json!(nome);
            lista.push(json!({ "nome": c["nome"], "entrada": c["entrada"], "saida": c["saida"] }));
        }
        fixtures::conferir("primitivas-efeitos.json", Value::Array(lista));
    }

    #[test]
    fn fixture_primitivas_seno() {
        let angulos = [0.0, 1.0, 29.999, 30.0, 44.999999, 45.0, 45.000001, 60.0, 89.9, 90.0, 120.0, 135.0, 179.0, 180.0, 225.0, 270.0, 315.0,
            359.9999999, 360.0, -0.0, -1e-20, -30.0, -90.0, 400.0, 720.5, 1e9, 137.5];
        let casos: Vec<Value> = angulos
            .iter()
            .map(|&g| {
                let (c, s) = cos_sen_graus(g);
                json!({ "nome": format!("{g:?}"), "entrada": { "graus": g }, "saida": { "cos": c, "sen": s } })
            })
            .collect();
        fixtures::conferir("primitivas-seno.json", Value::Array(casos));
    }

    #[test]
    fn cos_sen_perto_da_biblioteca() {
        let mut pior = 0.0f64;
        for i in -3600..=3600 {
            let g = i as f64 * 0.137;
            let (c, s) = cos_sen_graus(g);
            let r = g.to_radians();
            pior = pior.max((c - r.cos()).abs()).max((s - r.sin()).abs());
        }
        assert!(pior < 1e-15, "pior {pior}");
        assert_eq!(cos_sen_graus(90.0), (0.0, 1.0));
        assert_eq!(cos_sen_graus(180.0), (-1.0, 0.0));
        assert_eq!(cos_sen_graus(-90.0), (0.0, -1.0));
        assert!(cos_sen_graus(90.0).0.is_sign_positive());
        assert_eq!(cos_sen_graus(f64::NAN), (1.0, 0.0));
    }

    #[test]
    fn calendario_ida_e_volta() {
        for z in -800_000..800_000i64 {
            let (a, m, d) = civil_de_dias(z);
            assert_eq!(dias_de_civil(a, m, d), z);
        }
        assert_eq!(civil_de_dias(dias_de_civil(2028, 2, 28) + 1), (2028, 2, 29));
        assert_eq!(civil_de_dias(dias_de_civil(2026, 2, 28) + 1), (2026, 3, 1));
        assert_eq!(dias_de_civil(1970, 1, 1), 0);
    }

    fn primitivas_do_caso(nome: &str) -> Vec<Primitiva> {
        let (_, e, q) = casos().into_iter().find(|c| c.0 == nome).unwrap();
        let e: Efeito = serde_json::from_value(e).unwrap();
        primitivas(&e, &q, &Falso::novo()).unwrap()
    }

    #[test]
    fn carimbo_textos() {
        let texto = |nome: &str| match &primitivas_do_caso(nome)[0] {
            Primitiva::Atlas { texto, .. } => texto.clone(),
            p => panic!("{p:?}"),
        };
        assert_eq!(texto("carimbo tempo_fonte hms_ms"), "Original 01:02:03,457");
        assert_eq!(texto("carimbo tempo_fonte hms"), "Original 01:02:03");
        assert_eq!(texto("carimbo tempo negativo"), "Original -00:00:01,500");
        // 86300 + (3723,4565 − 3600) = 86423,4565 → dia seguinte, 00:00:23,457
        assert_eq!(texto("carimbo relogio hms_ms virando o dia"), "Câmera 00:00:23,457");
        assert_eq!(texto("carimbo relogio data_hora virando o mes"), "Câmera 29/02/2028 00:00:23");
        assert_eq!(texto("carimbo relogio data_hora sem data"), "Câmera 00:00:23");
        assert_eq!(texto("carimbo quadro"), "Quadro 312");
        assert_eq!(texto("carimbo tempo_programa com caixa"), " Vídeo 00:00:04,004 ");
        let glifos = crate::edicao::camada::texto::GLIFOS_CARIMBO;
        for n in ["carimbo tempo_fonte hms_ms", "carimbo relogio data_hora virando o mes", "carimbo tempo_programa com caixa", "carimbo quadro"] {
            assert!(texto(n).chars().all(|c| glifos.contains(c)), "{n}: glifo fora do atlas");
        }
        // tempo absurdo não derruba (o módulo de i64::MIN estourava)
        for t in [f64::NEG_INFINITY, f64::INFINITY, f64::NAN, -1e300, 1e300] {
            assert!(tempo(t, FormatoCarimbo::HmsMs).contains(':'));
            let r = RetratoRelogio { media_time_s: 0.0, clock_seconds: 10.0, clock_date: Some("2026-11-14".into()) };
            assert!(relogio(&r, t, FormatoCarimbo::DataHora).is_ok());
        }
        assert_eq!(tempo(360_000.0, FormatoCarimbo::Hms), "100:00:00");
        // data fora do calendário de 4 dígitos: item inválido, sem estouro
        for d in ["99999999999999999-01-01", "0-01-01", "2026-13-01", "2026-01-32", "2026-01", "14/11/2026", ""] {
            let r = RetratoRelogio { media_time_s: 0.0, clock_seconds: 10.0, clock_date: Some(d.into()) };
            assert!(relogio(&r, 5.0, FormatoCarimbo::DataHora).is_err(), "{d:?}");
        }
    }

    #[test]
    fn seta_aponta_e_circulo_cobre_a_caixa() {
        // seta a 45°: cauda acima e à direita da ponta, comprimento = tamanho × altura
        let Primitiva::Caminho { d, .. } = &primitivas_do_caso("destaque seta ponto 45")[0] else { panic!() };
        let (Segmento::M(tx, ty), Segmento::L(px, py)) = (d[0], d[1]) else { panic!() };
        assert_eq!((px, py), (0.5, 0.5));
        let (dx, dy) = ((tx - px) * 1920.0, (ty - py) * 1080.0);
        assert!(dx > 0.0 && dy < 0.0 && ((dx * dx + dy * dy).sqrt() - 0.12 * 1080.0).abs() < 1e-9);
        // círculo sobre caixa: os quatro cantos (com a margem) ficam dentro
        let Primitiva::Elipse { cx, cy, rx, ry, .. } = primitivas_do_caso("destaque circulo caixa")[0] else { panic!() };
        let c = geometria::com_margem(Caixa { x: 0.4, y: 0.3, w: 0.1, h: 0.2 }, 0.05);
        for (x, y) in [(c.x, c.y), (c.x + c.w, c.y), (c.x, c.y + c.h), (c.x + c.w, c.y + c.h)] {
            let (u, v) = ((x - cx) / rx, (y - cy) / ry);
            assert!(u * u + v * v <= 1.0 + 1e-12);
        }
        assert!((rx * 1920.0 - ry * 1080.0).abs() < 1e-9, "círculo em px");
    }

    #[test]
    fn envolvente_inclui_traco_e_borda() {
        let p = primitivas_do_caso("destaque retangulo caixa margem");
        let e = envolvente(&p, CANVAS).unwrap();
        let c = geometria::com_margem(Caixa { x: 0.4, y: 0.3, w: 0.1, h: 0.2 }, 0.1);
        // meia largura do traço com contorno (1,5×) + 1 px
        let m = 0.006 * 1080.0 * 0.5 * CONTORNO + 1.0;
        assert!((e.x * 1920.0 - (c.x * 1920.0 - m)).abs() < 1e-6 && (e.h * 1080.0 - (c.h * 1080.0 + 2.0 * m)).abs() < 1e-6, "{e:?}");
        let inv = primitivas_do_caso("ocultar inverter margem negativa");
        assert_eq!(envolvente(&inv, CANVAS), Some(Caixa { x: 0.0, y: 0.0, w: 1.0, h: 1.0 }));
        assert_eq!(envolvente(&[], CANVAS), None);
    }

    #[test]
    fn sprite_em_px_inteiro_sai_copiado_sem_reamostrar() {
        let cache = crate::edicao::testes::fontes::pasta_temporaria("p05-copia");
        let mut px = Vec::new();
        for y in 0..17u32 {
            for x in 0..23u32 {
                px.extend_from_slice(&[(x * 11) as u8, (y * 15) as u8, 77, [255, 128, 3][((x + y) % 3) as usize]]);
            }
        }
        let b = Bitmap { w: 23, h: 17, canais: 4, px };
        std::fs::create_dir_all(cache.join("sprites")).unwrap();
        std::fs::write(cache.join("sprites/s.png"), b.png().unwrap()).unwrap();
        // posição que não volta inteira por ÷ e × (245/1920·1920 ≠ 245; 249/1080·1080 ≠ 249)
        let t = Tela::de(CANVAS);
        assert_ne!(245.0 / 1920.0 * 1920.0, 245.0);
        let p = vec![Primitiva::Sprite { src: "sprites/s.png".into(), caixa: t.caixa(245.0, 249.0, 23.0, 17.0), opacidade: 1.0 }];
        let d = desenhar(&p, CANVAS, [238, 238, 40, 40], &cache, &[]).unwrap();
        for y in 0..17u32 {
            for x in 0..23u32 {
                let i = (((y + 11) * 40 + x + 7) * 4) as usize;
                let j = ((y * 23 + x) * 4) as usize;
                let (a, e) = (&d.px[i..i + 4], &b.px[j..j + 4]);
                // alfa exato (o padrão de alfa denuncia meio pixel de deslocamento); a cor passa pelo pré-multiplicado,
                // que quantiza em passos de 255/alfa
                assert_eq!(a[3], e[3], "({x}, {y})");
                let tol = 255u32.div_ceil(e[3] as u32);
                assert!(a.iter().zip(e).all(|(u, v)| (u.abs_diff(*v) as u32) <= if e[3] < 255 { tol } else { 0 }), "({x}, {y}): {a:?} × {e:?}");
            }
        }
        let _ = std::fs::remove_dir_all(cache);
    }

    #[test]
    fn desenho_em_alfa_reto() {
        // traço vermelho puro sem contorno: todo pixel com alfa tem cor (255, 0, 0), nada escurecido
        let p = vec![Primitiva::Elipse {
            cx: 0.5,
            cy: 0.5,
            rx: 0.3,
            ry: 0.3,
            traco: Some(Traco2 { cor: "#ff0000".into(), largura: 0.05, contorno: None }),
            preenche: None,
        }];
        let t = TamanhoPx { w: 200, h: 200 };
        let b = desenhar(&p, t, [20, 20, 160, 160], Path::new("/nao/existe"), &[]).unwrap();
        assert_eq!((b.w, b.h, b.canais), (160, 160, 4));
        let mut parciais = 0;
        for px in b.px.as_chunks::<4>().0 {
            if px[3] > 0 {
                assert_eq!(&px[..3], &[255, 0, 0], "alfa {}", px[3]);
                parciais += (px[3] < 255) as u32;
            } else {
                // alfa 0: sem cor, ou a do desenho vizinho (sangria)
                assert!(px[..3] == [0, 0, 0] || px[..3] == [255, 0, 0], "{px:?}");
            }
        }
        assert!(parciais > 50, "borda suavizada: {parciais}");
        // com contorno: o centro do traço é a cor do traço, por cima do contorno
        let p = vec![Primitiva::Caminho {
            d: vec![Segmento::M(0.1, 0.5), Segmento::L(0.9, 0.5)],
            traco: Some(Traco2 { cor: "#00ff00".into(), largura: 0.05, contorno: Some(COR_CONTORNO.into()) }),
            preenche: None,
            regra: None,
        }];
        let b = desenhar(&p, t, [0, 0, 200, 200], Path::new("/nao/existe"), &[]).unwrap();
        let px = |x: usize, y: usize| &b.px[(y * 200 + x) * 4..(y * 200 + x) * 4 + 4];
        assert_eq!(px(100, 100), &[0, 255, 0, 255]);
        // fora da largura do traço (5 px de meia largura) e dentro da do contorno (7,5): preto translúcido
        let c = px(100, 106);
        assert_eq!((c[0], c[1], c[2]), (0, 0, 0));
        assert!(c[3] > 150 && c[3] < 255, "{c:?}");
    }
    #[test]
    fn cor_sangra_para_o_alfa_zero_sem_mudar_o_que_aparece() {
        // quadrado azul opaco de 10×10 em (20, 20) e um pixel quase transparente, laranja, em (50, 20)
        let (w, h) = (64usize, 48usize);
        let mut pm = Pixmap::new(w as u32, h as u32).unwrap();
        let mut azul = Paint::default();
        azul.set_color_rgba8(0, 0, 255, 255);
        pm.fill_rect(Rect::from_xywh(20.0, 20.0, 10.0, 10.0).unwrap(), &azul, Transform::identity(), None);
        let mut laranja = Paint::default();
        laranja.set_color_rgba8(255, 128, 0, 64);
        pm.fill_rect(Rect::from_xywh(50.0, 20.0, 1.0, 1.0).unwrap(), &laranja, Transform::identity(), None);
        let antes: Vec<u8> = pm.data().to_vec();
        let b = bitmap_reto(pm);
        let px = |x: usize, y: usize| &b.px[(y * w + x) * 4..(y * w + x) * 4 + 4];
        for y in 0..h {
            for x in 0..w {
                let p = px(x, y);
                // o alfa não muda em nenhum pixel
                assert_eq!(p[3], antes[(y * w + x) * 4 + 3], "({x}, {y})");
                let dentro = (20..30).contains(&x) && (20..30).contains(&y);
                let d = |a: usize, lo: usize, hi: usize| lo.saturating_sub(a).max(a.saturating_sub(hi));
                let dist = d(x, 20, 29).max(d(y, 20, 29));
                let dist_l = x.abs_diff(50).max(y.abs_diff(20));
                if dentro {
                    assert_eq!(p, &[0, 0, 255, 255]);
                } else if dist <= SANGRIA as usize {
                    assert_eq!(p, &[0, 0, 255, 0], "({x}, {y}) a {dist} px do quadrado");
                } else if (x, y) == (50, 20) {
                    assert_eq!((p[0], p[2], p[3]), (255, 0, 64));
                    assert!(p[1].abs_diff(128) <= 2, "{p:?}");
                } else if dist_l <= SANGRIA as usize {
                    assert_eq!((p[0], p[2], p[3]), (255, 0, 0), "({x}, {y})");
                } else {
                    assert_eq!(p, &[0, 0, 0, 0], "({x}, {y}) longe do desenho");
                }
            }
        }
        // camada vazia fica vazia
        let vazio = bitmap_reto(Pixmap::new(8, 6).unwrap());
        assert!(vazio.px.iter().all(|v| *v == 0));
    }
}
