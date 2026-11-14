//! Rastreio (§6.1). Fase 0: só a avaliação por âncoras. Os tipos de arquivo ficam aqui
//! porque `motor::LeitorRastreios` os devolve; quem os grava nasce na Fase 2. Dono: P0.3a.

use serde::{Deserialize, Serialize};

use super::modelo::TamanhoPx;

pub mod avaliar;

/// Estados da coluna `estado` do denso.
pub const ESTADO_OK: u8 = 0;
pub const ESTADO_INCERTO: u8 = 1;
pub const ESTADO_AUSENTE: u8 = 2;

/// Valores da coluna `tipo` do denso.
pub const TIPO_MEDIDO: u8 = 0;
pub const TIPO_INTERPOLADO: u8 = 1;
pub const TIPO_ESTENDIDO: u8 = 2;

/// Conteúdo de `rastreios/<rid>/<sha256>.json`: uma linha por quadro da fonte, em colunas.
/// `pts` na base da tabela do decodificador; `x`, `y`, `w`, `h` em 0..1; `corrida` = índice em `corridas`.
/// `rastreio` = id com que o arquivo nasceu: a cópia do projeto leva o arquivo byte a byte (mesmo SHA-256),
/// então o leitor NÃO compara esse campo com o id do rastreio que aponta para ele.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArquivoDenso {
    pub esquema: u32,
    pub rastreio: String,
    pub midia_sha256: String,
    pub espaco: String,
    pub pixel: TamanhoPx,
    pub pts: Vec<i64>,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub w: Vec<f64>,
    pub h: Vec<f64>,
    pub conf: Vec<f64>,
    pub estado: Vec<u8>,
    pub tipo: Vec<u8>,
    pub corrida: Vec<u32>,
    /// Proveniência de cada corrida (motor, modelo, limiares); o formato fecha na Fase 2.
    pub corridas: Vec<serde_json::Value>,
}

/// Conteúdo de `silhuetas/<rid>/<sha256>.sil`: cabeçalho e uma máscara binária por quadro.
// CONTRATO: o formato do RLE de `mascaras` fecha na Fase 2 (`rastreio/silhueta.rs`).
#[derive(Debug, Clone, PartialEq)]
pub struct ArquivoSilhuetas {
    pub esquema: u32,
    pub pixel: TamanhoPx,
    pub n: u32,
    pub pts: Vec<i64>,
    pub modelo: String,
    /// Uma por quadro, RLE por linha na resolução da fonte exibida.
    pub mascaras: Vec<Vec<u8>>,
}
