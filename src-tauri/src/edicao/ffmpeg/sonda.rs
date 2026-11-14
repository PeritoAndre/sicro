//! Sonda de capacidades e escolha do binário (§5.6). Dono: P0.4.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// De onde veio o binário: `SICRO_FFMPEG`, o embutido ou o PATH.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Origem {
    Configurado,
    Embutido,
    Path,
}

/// Como o binário recebe o grafo em arquivo: `-/filter_complex` ou `-filter_complex_script`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GrafoArquivo {
    BarraFilterComplex,
    FilterComplexScript,
}

/// Codificador de GPU aprovado: `degrau` = nome do degrau de parâmetros que passou; `args` = os dele.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Gpu {
    pub nome: String,
    pub driver: String,
    pub codificador: String,
    pub degrau: String,
    pub args: Vec<String>,
    pub nvdec: bool,
    pub scale_cuda: bool,
}

/// Por que um codificador de GPU foi reprovado (classificado pelo stderr).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClasseReprovacao {
    SemGpu,
    SemDriver,
    DriverAntigo,
    LimiteDeSessoes,
    RecursoNaoSuportado,
    FfmpegSemCodificador,
    /// Código 0 e vídeo destruído (conferência da saída, PSNR < 30 dB).
    SaidaInvalida,
    Outra,
}

/// Codificador reprovado e por quê. Os três últimos só em `driver_antigo`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reprovado {
    pub codificador: String,
    pub classe: ClasseReprovacao,
    pub dica: String,
    pub driver_minimo: Option<String>,
    pub api_exigida: Option<String>,
    pub api_do_driver: Option<String>,
}

/// O que o binário escolhido sabe fazer. `faltam` = filtros obrigatórios ausentes;
/// `gpu` None até a sonda de GPU acabar (ou sem codificador aprovado).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Capacidades {
    pub binario: PathBuf,
    /// Da mesma pasta do binário.
    pub ffprobe: PathBuf,
    pub origem: Origem,
    pub versao: String,
    pub configuracao: String,
    pub sha256: String,
    pub filtros: BTreeSet<String>,
    pub codificadores: BTreeSet<String>,
    pub decodificadores: BTreeSet<String>,
    pub hwaccels: BTreeSet<String>,
    pub muxers: BTreeSet<String>,
    pub grafo_em_arquivo: GrafoArquivo,
    pub faltam: Vec<String>,
    pub gpu: Option<Gpu>,
    pub reprovados: Vec<Reprovado>,
}

/// Resumo para a tela (`edicao_capacidades`). `codificador` = o que a exportação "auto" usaria agora;
/// `sondando` = sonda de GPU em andamento.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResumoCapacidades {
    pub versao: String,
    pub origem: Origem,
    pub sha256: String,
    pub faltam: Vec<String>,
    pub opcionais_ausentes: Vec<String>,
    pub codificador: String,
    pub gpu: Option<Gpu>,
    pub reprovados: Vec<Reprovado>,
    pub sondando: bool,
}

/// Pasta do cache das sondas (`app_cache_dir()/edicao`). Chamada uma vez na partida; sem ela não há cache em disco.
pub fn configurar(cache_edicao: &Path) {
    todo!()
}

/// Capacidades do binário escolhido: memória → `sonda-<sha256>.json` → sonda. Erro se `SICRO_FFMPEG` tem `faltam`.
pub fn capacidades() -> Result<Arc<Capacidades>> {
    todo!()
}

/// Sonda um binário específico, sem cache (testes nos dois ffmpeg).
pub fn sondar(binario: &Path) -> Result<Capacidades> {
    todo!()
}

/// Refaz a sonda de GPU ("Testar de novo", ou depois de falha de render por GPU) e devolve o novo estado.
pub fn testar_de_novo() -> Result<Arc<Capacidades>> {
    todo!()
}

/// Codificação real de 8 quadros no tamanho e na taxa do projeto com o codificador de GPU aprovado; false = usar CPU.
pub fn gpu_serve(cap: &Capacidades, largura: u32, altura: u32, fps: crate::edicao::modelo::Racional) -> bool {
    todo!()
}

/// Resumo para a tela.
pub fn resumo(cap: &Capacidades) -> ResumoCapacidades {
    todo!()
}
