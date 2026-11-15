//! Fábrica única de sessão (§6.5; protótipo `p2/proto/src/sessao_ia.rs`). Um onnxruntime por processo: a lib de
//! GPU quando `gpu::pronta`, senão a de CPU (Pessoas ou super-resolução). CUDA com `error_on_failure`,
//! `use_tf32 = 0`, busca heurística e aquecimento; qualquer erro refaz em CPU com o motivo. Dono: P2.1.
// Abertura da Fase 2: corpos recusam; `video/ort_env.rs`, `pessoas/yolox.rs` e `realce/superres.rs` passam a
// chamar daqui quando o P2.1 entregar.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::gpu::PacoteGpu;
use crate::commands::settings_commands::AiSettings;
use crate::error::{Result, SicroError};

/// Onde a sessão rodou. No fio e nos registros (`Corrida.provedor`, `ProvIa.provedor`): `cpu` | `cuda`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provedor {
    Cpu,
    Cuda,
}

impl Provedor {
    /// Texto gravado nas corridas e no `tecnico.json`.
    pub fn fio(self) -> &'static str {
        match self {
            Provedor::Cpu => "cpu",
            Provedor::Cuda => "cuda",
        }
    }
}

/// `Auto`: GPU se pronta, senão CPU com o motivo em `SessaoIa::queda`. `Cpu`: nunca tenta GPU (VitTrack).
/// `Gpu`: erro se a GPU não servir (autoteste; os grafos seguintes do EdgeTAM depois que o primeiro ficou na GPU).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preferencia {
    Auto,
    Cpu,
    Gpu,
}

#[derive(Debug, Clone)]
pub struct Opcoes {
    pub preferencia: Preferencia,
    pub threads_cpu: usize,
    /// Forma NCHW f32 da primeira entrada para a inferência de aquecimento na GPU (a falta do cuDNN e da memória de
    /// vídeo só aparece nela); None = sem aquecimento (grafo de várias entradas: aquecer o primeiro).
    pub aquecer: Option<[usize; 4]>,
    /// Teto da arena CUDA em MiB; None = padrão do provedor.
    pub vram_max_mb: Option<usize>,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self {
            preferencia: Preferencia::Auto,
            threads_cpu: std::thread::available_parallelism().map_or(4, |n| n.get()).min(8),
            aquecer: None,
            vram_max_mb: None,
        }
    }
}

impl Opcoes {
    /// "Só CPU" (VitTrack, §6.5).
    pub fn so_cpu() -> Self {
        Self { preferencia: Preferencia::Cpu, ..Self::default() }
    }
}

pub struct SessaoIa {
    pub session: ort::session::Session,
    pub provedor: Provedor,
    /// Código do catálogo (`gpu_*`) de por que não ficou na GPU, quando `Auto` caiu para CPU.
    pub queda: Option<String>,
}

/// De onde vem o grafo.
#[derive(Debug, Clone, Copy)]
pub enum Modelo<'a> {
    Arquivo(&'a Path),
    /// Grafo embutido no binário (VitTrack, modelo do autoteste).
    Bytes(&'a [u8]),
}

fn recusa() -> SicroError {
    SicroError::Validation(format!("{}: ia", crate::edicao::mensagens::codigo::NAO_SUPORTADO))
}

/// Biblioteca e pacote de GPU que `criar_sessao` usa: a lib de GPU quando `gpu::pronta(ai)`, senão a de CPU
/// (`ort_lib_path`, depois `sr_ort_lib_path`). None = nenhum onnxruntime instalado (`modelo_ausente: onnxruntime`).
pub fn biblioteca(ai: &AiSettings) -> Option<(PathBuf, Option<PacoteGpu>)> {
    let _ = ai;
    None
}

/// A função única (§6.5). Interruptor desligado: `Validation("ia_desligada")`.
pub fn criar_sessao(ai: &AiSettings, modelo: &Path, op: &Opcoes) -> Result<SessaoIa> {
    let _ = (ai, modelo, op);
    Err(recusa())
}

/// Mesma regra, com o grafo em memória.
pub fn criar_sessao_de_bytes(ai: &AiSettings, modelo: &[u8], op: &Opcoes) -> Result<SessaoIa> {
    let _ = (ai, modelo, op);
    Err(recusa())
}

/// Com a biblioteca explícita: autoteste no processo filho e testes (`ia::testes::ambiente`). Não olha o interruptor.
pub fn criar_sessao_com(lib: &Path, pacote: Option<&PacoteGpu>, modelo: Modelo<'_>, op: &Opcoes) -> Result<SessaoIa> {
    let _ = (lib, pacote, modelo, op);
    Err(recusa())
}

/// Ambiente único do processo (o `ort` só admite um; telemetria desligada). `video/ort_env.rs` vira ponte para cá.
pub fn ambiente(lib: &Path) -> Result<&'static ort::environment::Environment> {
    let _ = lib;
    Err(recusa())
}

/// Biblioteca já carregada neste processo (None = nenhuma). Outra pedida depois = reabrir o SICRO (`reabrir`).
pub fn carregada() -> Option<PathBuf> {
    None
}

/// "onnxruntime 1.31.1": versão da lib carregada, para `Corrida.runtime` e `ProvIa.runtime`.
pub fn versao_runtime() -> String {
    String::new()
}
