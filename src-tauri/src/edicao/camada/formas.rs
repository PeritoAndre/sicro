//! Geometria única do desenho (§5.5, §8.3): efeito + quadro → `Primitiva`s, e o desenho delas.
//! Espelho no front: `palco/desenho/primitivas.ts`, pelas fixtures `primitivas-*.json`. Dono: P0.5.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::texto::FonteSprites;
use super::Bitmap;
use crate::edicao::geometria::Afim;
use crate::edicao::modelo::{Caixa, Efeito, FormaLupa, Lugar, TamanhoPx};
use crate::edicao::motor::plano::ImagemCamada;
use crate::error::Result;

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuadroCamada {
    pub q: u64,
    pub t: f64,
    pub pts: Option<i64>,
    pub a: Afim,
    pub canvas: TamanhoPx,
    /// Rastreio sem caixa neste quadro não aparece.
    pub rastreios: BTreeMap<String, Caixa>,
}

/// Primitivas de um efeito num quadro (vazio = nada a desenhar). Efeito de pixel puro (ajustes, lente…) devolve vazio.
pub fn primitivas(efeito: &Efeito, quadro: &QuadroCamada, sprites: &dyn FonteSprites) -> Result<Vec<Primitiva>> {
    todo!()
}

/// Caixa envolvente (0..1 do canvas) do que as primitivas desenham, com traço e borda suave; None = nada.
pub fn envolvente(primitivas: &[Primitiva], canvas: TamanhoPx) -> Option<Caixa> {
    todo!()
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
    todo!()
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
}
