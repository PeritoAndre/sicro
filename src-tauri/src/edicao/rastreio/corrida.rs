//! Corrida pedida pela tela (§6.2, §6.3): uma por vez no processo; quadros por pipe `rawvideo rgb24` (decodificação
//! em CPU) do intermediário de origem quando o espaço não é "original", senão da fonte; busca pela regra do §5.3;
//! pts de cada quadro pelo `showinfo` lido em paralelo, casado por ordem; para trás, blocos de 100 quadros.
//! Eventos `edicao:rastreio`. Dono: P2.2. Abertura: `pedir` recusa com `nao_suportado: rastreio`.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{recusa, EscolhaMotor, Imagem, LeitorQuadros, Parou, Rastreador, Sentido, SentidoPedido};
use crate::commands::settings_commands::AiSettings;
use crate::edicao::ffmpeg::sonda::Capacidades;
use crate::edicao::modelo::{Caixa, Chave, Denso, Silhuetas, TamanhoPx};
use crate::edicao::motor::FonteInfo;
use crate::error::Result;

/// Nome do evento (§6.3).
pub const EVENTO_RASTREIO: &str = "edicao:rastreio";
/// Lado maior da miniatura do evento (px).
pub const MINIATURA_MAX: u32 = 320;
/// Eventos de andamento por segundo, no máximo.
pub const AVANCOS_POR_S: u32 = 5;

/// O que `edicao_rastreio_seguir` recebe (§6.3). `ate_s` em tempo de fonte (None = até a borda da mídia).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PedidoSeguir {
    pub rastreio_id: String,
    pub clipe_id: String,
    pub ancora: Chave<Caixa>,
    pub sentido: SentidoPedido,
    pub ate_s: Option<f64>,
    pub motor: EscolhaMotor,
}

/// Andamento de uma corrida. `caixa` em 0..1 (None = sem o alvo nesse quadro); `estado` como no denso;
/// `miniatura` = JPEG de até 320 px do quadro com a caixa, em `data:image/jpeg;base64,…`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AvancoRastreio {
    pub tarefa_id: String,
    pub rastreio_id: String,
    pub sentido: Sentido,
    pub pts: i64,
    pub caixa: Option<Caixa>,
    pub conf: f64,
    pub estado: u8,
    pub feitos: u32,
    pub miniatura: Option<String>,
}

/// Fim de uma corrida pedida (as duas, em `ambos`): o front aplica `denso` e `silhuetas` com `alterar()`.
/// `denso` None = nada novo (cancelada antes do primeiro quadro seguido).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FimRastreio {
    pub tarefa_id: String,
    pub rastreio_id: String,
    pub parou: Vec<ParouSentido>,
    pub denso: Option<Denso>,
    pub silhuetas: Option<Silhuetas>,
}

/// Onde parou cada sentido (`Parou` com o sentido).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParouSentido {
    pub sentido: Sentido,
    pub motivo: super::Motivo,
    pub pts: Option<i64>,
}

impl ParouSentido {
    pub fn de(sentido: Sentido, p: Parou) -> Self {
        Self { sentido, motivo: p.motivo, pts: p.pts }
    }
}

/// Corrida que falhou: `mensagem` = `"<código>: <frase>"` do catálogo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FalhaRastreio {
    pub tarefa_id: String,
    pub rastreio_id: String,
    pub mensagem: String,
}

/// Carga do evento `edicao:rastreio`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum EventoRastreio {
    Avanco(AvancoRastreio),
    Fim(FimRastreio),
    Falhou(FalhaRastreio),
}

/// Quem recebe os eventos (o comando injeta o `emit` do Tauri; os testes, um coletor).
pub type Emissor = Box<dyn Fn(EventoRastreio) + Send + Sync>;

/// Começa a corrida em segundo plano e devolve o `tarefa_id`. Outra corrida em andamento = `rastreio_em_curso`.
/// Não grava nada no projeto: o arquivo denso novo (e as silhuetas) vão para a pasta do projeto e o ponteiro sai no
/// evento final. `projeto` é o documento da tela (D2).
pub fn pedir(ws: &Path, cache: &Path, ai: &AiSettings, projeto: &Value, pedido: &PedidoSeguir, emitir: Emissor) -> Result<String> {
    let _ = (ws, cache, ai, projeto, pedido, emitir);
    Err(recusa("rastreio"))
}

/// Cancela a corrida (menos de 0,5 s até o fim dela); tarefa que já acabou = `tarefa_ausente`.
pub fn cancelar(tarefa_id: &str) -> Result<()> {
    let _ = tarefa_id;
    Err(recusa("rastreio"))
}

/// O motor da corrida pela regra do `auto` (§6.2). Pedido explícito sem o item = `modelo_ausente: <motor>`;
/// interruptor desligado = `ia_desligada` (a mancha segue sem os modelos).
pub fn escolher(ai: &AiSettings, pedido: EscolhaMotor) -> Result<Box<dyn Rastreador>> {
    let _ = (ai, pedido);
    Err(recusa("rastreio"))
}

/// Leitor por pipe (§6.2). `pixel` = tamanho entregue (o exibido da fonte; o do intermediário no espaço de origem).
pub struct LeitorPipe {
    _privado: (),
}

impl LeitorPipe {
    /// Quadros de `fonte` a partir do quadro de `de_pts`, no `sentido`, até `ate_pts` (incluso; None = borda).
    pub fn abrir(cap: &Capacidades, fonte: &FonteInfo, de_pts: i64, ate_pts: Option<i64>, sentido: Sentido, pixel: TamanhoPx) -> Result<Self> {
        let _ = (cap, fonte, de_pts, ate_pts, sentido, pixel);
        Err(recusa("rastreio"))
    }
}

impl LeitorQuadros for LeitorPipe {
    fn proximo(&mut self) -> Result<Option<(i64, Imagem<'_>)>> {
        todo!("P2.2: atrás da recusa de `LeitorPipe::abrir`")
    }
}

/// O cancelamento visível da corrida em andamento (teste de "menos de 0,5 s").
pub fn em_curso() -> Option<(String, std::sync::Arc<AtomicBool>)> {
    None
}
