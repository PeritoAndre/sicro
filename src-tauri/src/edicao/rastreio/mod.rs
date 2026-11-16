//! Rastreio (§6). Fase 0: a avaliação por âncoras e os tipos dos arquivos (que `motor::LeitorRastreios` devolve).
//! Fase 2 (abertura): o trait `Rastreador`, a corrida e os tipos que P2.2 (núcleo), P2.3 (motores neurais) e P2.5
//! (Pessoas) usam. Dono: P2.2 (`contratos-fase2.md` (a)); `vittrack.rs`, `edgetam.rs`: P2.3.

use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use super::modelo::{Caixa, Chave, TamanhoPx};
use crate::error::Result;

pub mod arquivo;
pub mod avaliar;
pub mod corrida;
pub mod edgetam;
pub mod fundir;
pub mod laco;
pub mod mancha;
pub mod silhueta;
pub mod vittrack;

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
    /// Proveniência de cada corrida: `Corrida` serializada (contratos-fase2 (b) B4; `arquivo::corridas` lê tipado).
    pub corridas: Vec<serde_json::Value>,
}

/// Conteúdo de `silhuetas/<rid>/<sha256>.sil`: cabeçalho e uma máscara binária por quadro.
/// O formato do arquivo e do RLE é o de `silhueta.rs` (P2.2).
#[derive(Debug, Clone, PartialEq)]
pub struct ArquivoSilhuetas {
    pub esquema: u32,
    pub pixel: TamanhoPx,
    pub n: u32,
    pub pts: Vec<i64>,
    pub modelo: String,
    /// Uma por quadro, RLE por linha na resolução da fonte exibida (`silhueta::rle`).
    pub mascaras: Vec<Vec<u8>>,
}

// ---------------------------------------------------------------- rastreadores (§6.2)

/// Quadro RGB 8 bits, linhas sem folga (o pipe `rawvideo rgb24`), no espaço do rastreio (`pixel` da corrida).
#[derive(Debug, Clone, Copy)]
pub struct Imagem<'a> {
    pub rgb: &'a [u8],
    pub w: u32,
    pub h: u32,
}

/// Caixa em pixels do quadro lido; (x, y) = canto superior esquerdo.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CaixaPx {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl CaixaPx {
    /// De 0..1 para px de `pixel`.
    pub fn de_caixa(c: &Caixa, pixel: TamanhoPx) -> Self {
        let (w, h) = (pixel.w as f64, pixel.h as f64);
        Self { x: c.x * w, y: c.y * h, w: c.w * w, h: c.h * h }
    }

    /// De px de `pixel` para 0..1.
    pub fn caixa(&self, pixel: TamanhoPx) -> Caixa {
        let (w, h) = (pixel.w as f64, pixel.h as f64);
        Caixa { x: self.x / w, y: self.y / h, w: self.w / w, h: self.h / h }
    }
}

/// Máscara binária do alvo num quadro: `w`×`h`, um byte por pixel (0 ou 1), linha a linha. Vive só durante o
/// passo: a corrida guarda o RLE (`silhueta::rle`), nunca a máscara crua.
#[derive(Debug, Clone, PartialEq)]
pub struct Mascara {
    pub w: u32,
    pub h: u32,
    pub bits: Vec<u8>,
}

/// Onde procurar no quadro: a última caixa aceita, ou a caixa ampliada da reprocura com `janela = false` (sem a
/// janela de Hann). O laço é quem decide; motor por presença (EdgeTAM) ignora.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Busca {
    pub caixa: CaixaPx,
    pub janela: bool,
}

/// Saída de um passo do rastreador: caixa (None = o motor diz que o alvo não está), nota 0..1 (confiança ou
/// presença) e a silhueta quando o motor dá (EdgeTAM).
#[derive(Debug, Clone, PartialEq)]
pub struct Passo {
    pub caixa: Option<CaixaPx>,
    pub conf: f64,
    pub silhueta: Option<Mascara>,
}

/// Como o laço decide (§6.2, `p3a/referencia/laco_sicro.py`): por confiança com reprocura em duas passadas
/// (VitTrack), por presença (EdgeTAM), ou por confiança sem reprocura (mancha).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Laco {
    Reprocura,
    Presenca,
    Simples,
}

/// Limiares gravados em cada corrida (§6.1): `ok` ≥ aceita sem aviso; `perdido` < não aceita; entre os dois,
/// incerto; `paciencia` = quadros seguidos sem o alvo antes de parar (já na taxa da mídia).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Limiares {
    pub ok: f64,
    pub perdido: f64,
    pub paciencia: u32,
}

/// Um motor de rastreio de alvo único. Não decide nada: o laço (`laco.rs`) aceita, avisa, procura ou para.
pub trait Rastreador: Send {
    /// Quadro marcado pelo perito; devolve o passo dele (o EdgeTAM já dá a silhueta do quadro marcado).
    fn iniciar(&mut self, quadro: &Imagem, caixa: CaixaPx) -> Result<Passo>;
    /// Quadro seguinte na ordem da corrida (para a frente ou para trás).
    fn seguir(&mut self, quadro: &Imagem, busca: &Busca) -> Result<Passo>;
    fn laco(&self) -> Laco;
    /// Limiares do motor na taxa `fps` da mídia (a paciência é em segundos no motor e em quadros na corrida).
    fn limiares(&self, fps: f64) -> Limiares;
    /// Motor, modelo (arquivos e SHA-256), runtime e provedor: o que a corrida grava.
    fn corrida(&self) -> MotorCorrida;
}

/// Por que a corrida parou (§6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motivo {
    Fim,
    Perdido,
    SaiuDoQuadro,
    Limite,
    Ancora,
    Cancelado,
}

/// Motor que rodou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorRastreio {
    Edgetam,
    Vittrack,
    Mancha,
}

/// Motor pedido: `auto` = edgetam com GPU pronta e item instalado, senão vittrack (com onnxruntime), senão mancha.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscolhaMotor {
    Auto,
    Edgetam,
    Vittrack,
    Mancha,
}

/// Sentido de uma corrida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sentido {
    Frente,
    Tras,
}

/// Sentido pedido pela tela: `ambos` = duas corridas a partir da mesma âncora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SentidoPedido {
    Frente,
    Tras,
    Ambos,
}

/// Arquivo de modelo: nome e SHA-256.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArquivoModelo {
    pub nome: String,
    pub sha256: String,
}

/// Modelo identificado por todos os arquivos (§6.6), com a origem, o commit e o desvio conhecido.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeloCorrida {
    pub nome: String,
    pub origem: String,
    pub commit: String,
    pub arquivos: Vec<ArquivoModelo>,
    pub desvio: String,
}

/// A parte da corrida que o motor dá. `runtime` = "onnxruntime 1.31.1" (vazio na mancha); `provedor` = `cpu` | `cuda`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MotorCorrida {
    pub motor: MotorRastreio,
    pub modelo: ModeloCorrida,
    pub runtime: String,
    pub provedor: String,
}

/// Uma corrida, como fica em `ArquivoDenso.corridas[]` (§6.1). `de_pts`/`ate_pts` = faixa percorrida (na ordem da
/// mídia); `feito_em` em RFC 3339.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Corrida {
    #[serde(flatten)]
    pub motor: MotorCorrida,
    pub sentido: Sentido,
    pub ancora_pts: i64,
    pub de_pts: i64,
    pub ate_pts: i64,
    pub limiares: Limiares,
    pub parou: Motivo,
    pub feito_em: String,
}

/// O que uma corrida precisa. `pixel` = tamanho do quadro lido (espaço do rastreio); `ancora` com pts;
/// `ate_pts` = limite pedido (None = até a borda da mídia); `max_quadros` = teto (`Motivo::Limite`).
#[derive(Debug, Clone, PartialEq)]
pub struct PedidoCorrida {
    pub rastreio: String,
    pub midia_sha256: String,
    pub espaco: String,
    pub pixel: TamanhoPx,
    pub ancora: Chave<Caixa>,
    pub sentido: Sentido,
    pub ate_pts: Option<i64>,
    pub max_quadros: u32,
    pub fps: f64,
}

/// Teto de quadros de uma corrida (também limita a memória das silhuetas): 30 min a 30 qps.
pub const MAX_QUADROS: u32 = 54_000;

/// Quadros na ordem da corrida (para trás: decrescente), começando no da âncora, com o pts de cada um.
pub trait LeitorQuadros {
    /// None = acabou a faixa.
    fn proximo(&mut self) -> Result<Option<(i64, Imagem<'_>)>>;
}

/// Andamento, a cada quadro (quem emite o evento limita a 5 por segundo). `caixa` em 0..1; `estado` como no denso.
pub struct Avanco<'a> {
    pub pts: i64,
    pub caixa: Option<Caixa>,
    pub conf: f64,
    pub estado: u8,
    pub feitos: u32,
    pub quadro: &'a Imagem<'a>,
}

/// Uma linha da corrida. `silhueta` = RLE (`silhueta::rle`) quando o motor dá.
#[derive(Debug, Clone, PartialEq)]
pub struct AmostraCorrida {
    pub pts: i64,
    pub caixa: Option<Caixa>,
    pub conf: f64,
    pub estado: u8,
    pub silhueta: Option<Vec<u8>>,
}

/// Onde e por que parou; `pts` = primeiro quadro perdido (onde o perito corrige e retoma) ou o último lido.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Parou {
    pub motivo: Motivo,
    pub pts: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResultadoCorrida {
    /// Em ordem de corrida, a primeira é a da âncora; a cauda de perdidos já sai cortada.
    pub amostras: Vec<AmostraCorrida>,
    pub parou: Parou,
    pub corrida: Corrida,
}

/// Uma corrida inteira: lê, segue, decide pelo laço do motor. Não grava nada (quem grava é `arquivo.rs`).
pub fn correr(
    p: &PedidoCorrida,
    leitor: &mut dyn LeitorQuadros,
    r: &mut dyn Rastreador,
    cancelar: &AtomicBool,
    avanco: &mut dyn FnMut(Avanco),
) -> Result<ResultadoCorrida> {
    laco::correr(p, leitor, r, cancelar, avanco)
}
