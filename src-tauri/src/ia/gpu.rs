//! Aceleração por GPU (NVIDIA): item do Gerenciador em `app_local_data_dir()/ai/gpu/` e autoteste em processo
//! filho (§6.5; `gpu-inferencia.md` §7). "Pronta" só depois do autoteste, refeito quando o driver muda. Dono: P2.1.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};

use super::sessao::Provedor;
use crate::commands::settings_commands::AiSettings;
use crate::error::{Result, SicroError};

/// Argumento do binário que roda o autoteste e imprime `Autoteste` em JSON (uma linha) no stdout.
pub const ARG_AUTOTESTE: &str = "--autoteste-gpu";
/// Subpastas de `ai/gpu/` (`gpu-inferencia.md` §7.2): `onnxruntime/lib/`, `cuda/`, `cudnn/` e o `estado.json`.
pub const PASTA_ONNXRUNTIME: &str = "onnxruntime";
pub const PASTA_CUDA: &str = "cuda";
pub const PASTA_CUDNN: &str = "cudnn";
pub const ESTADO: &str = "estado.json";
/// Requisitos (§6.5): `compute_cap` ≥ 7.5 e driver ≥ 580.
pub const COMPUTE_CAP_MIN: (u32, u32) = (7, 5);
pub const DRIVER_MIN: u32 = 580;
/// Conv 3×3 + ReLU de 1,1 kB (`p2/scripts/gerar_autoteste_onnx.py`): exercita CUDA e cuDNN sem modelo baixado.
pub const MODELO_AUTOTESTE: &[u8] = include_bytes!("autoteste_conv.onnx");
pub const FORMA_AUTOTESTE: [usize; 4] = [1, 3, 64, 64];
/// Saída da GPU igual à da CPU até aqui (autoteste).
pub const TOLERANCIA_AUTOTESTE: f32 = 1e-3;

/// Bibliotecas de um pacote de GPU: a `libonnxruntime` de GPU e as pastas pré-carregadas (CUDA, depois cuDNN).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacoteGpu {
    pub lib: PathBuf,
    pub cuda: PathBuf,
    pub cudnn: PathBuf,
}

impl PacoteGpu {
    /// O pacote no leiaute de `ai/gpu/`; None = falta a lib ou uma das pastas.
    pub fn da_pasta(gpu_dir: &Path) -> Option<Self> {
        let _ = gpu_dir;
        None
    }
}

/// Placa pelo `nvidia-smi --query-gpu=name,driver_version,compute_cap,memory.total`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placa {
    pub nome: String,
    pub driver: String,
    pub compute_cap: String,
    pub memoria_mb: u64,
}

/// Resultado do autoteste (no filho, impresso em JSON). `motivo` = código do catálogo (`gpu_*`); `detalhe` = texto
/// do onnxruntime (vai para o log, nunca para a tela).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Autoteste {
    pub pronta: bool,
    pub provedor: Provedor,
    pub motivo: Option<String>,
    pub detalhe: Option<String>,
    pub ms_sessao: f64,
    pub ms_primeira: f64,
    pub ms_segunda: f64,
    pub nos_gpu: u32,
    pub nos_total: u32,
    pub dif_max: Option<f32>,
}

/// `ai/gpu/estado.json`: o que foi instalado e o último autoteste (com a placa e o driver de então).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstadoGpu {
    /// "cuda13"
    pub variante: String,
    /// "1.31.1"
    pub onnxruntime: String,
    pub placa: Option<Placa>,
    pub autoteste: Option<Autoteste>,
    pub testado_em: Option<String>,
}

fn recusa() -> SicroError {
    SicroError::Validation(format!("{}: gpu", crate::edicao::mensagens::codigo::NAO_SUPORTADO))
}

/// Placa NVIDIA desta máquina; None = sem `nvidia-smi` ou sem placa.
pub fn placa() -> Option<Placa> {
    None
}

/// Código do catálogo quando a placa não atende (`gpu_placa_antiga`, `gpu_driver_antigo`); None = atende.
pub fn requisito(p: &Placa) -> Option<&'static str> {
    let _ = p;
    None
}

/// Lê `estado.json`; None = ausente ou ilegível.
pub fn estado(gpu_dir: &Path) -> Option<EstadoGpu> {
    let _ = gpu_dir;
    None
}

/// Pacote pronto para `criar_sessao`: instalado, autoteste aprovado e o driver de agora igual ao do autoteste.
pub fn pronta(ai: &AiSettings) -> Option<PacoteGpu> {
    let _ = ai;
    None
}

/// Baixa e extrai o item (URLs, bytes e SHA-256 fixos por sistema; `gpu-inferencia.md` §7.1) em `gpu_dir`.
/// `progresso(recebidos, total)`. Não roda o autoteste.
pub fn instalar(gpu_dir: &Path, cancelar: &AtomicBool, progresso: &mut dyn FnMut(u64, u64)) -> Result<()> {
    let _ = (gpu_dir, cancelar, progresso);
    Err(recusa())
}

pub fn remover(gpu_dir: &Path) -> Result<()> {
    let _ = gpu_dir;
    Err(recusa())
}

/// O autoteste neste processo (é o corpo do filho): CPU × GPU com o modelo embutido.
pub fn autoteste_aqui(pacote: &PacoteGpu) -> Autoteste {
    let _ = pacote;
    Autoteste {
        pronta: false,
        provedor: Provedor::Cpu,
        motivo: Some(crate::edicao::mensagens::codigo::GPU_FALHOU.into()),
        detalhe: None,
        ms_sessao: 0.0,
        ms_primeira: 0.0,
        ms_segunda: 0.0,
        nos_gpu: 0,
        nos_total: 0,
        dif_max: None,
    }
}

/// Roda `exe <args…> --autoteste-gpu <gpu_dir>` e lê o JSON. Em produção `exe` = `current_exe()`; nos testes, o
/// binário de teste com o filtro do teste filho (`ia::testes`). Falha de driver que derruba o filho = não pronta.
pub fn autoteste_em_processo(exe: &Path, args: &[String], gpu_dir: &Path) -> Result<Autoteste> {
    let _ = (exe, args, gpu_dir);
    Err(recusa())
}

/// Entrada do `main.rs`: Some(código de saída) quando os argumentos pedem o autoteste (imprime o JSON); None = não.
pub fn principal_do_autoteste(args: &[String]) -> Option<i32> {
    let _ = args;
    None
}

/// Erro do onnxruntime/CUDA → código do catálogo (`gpu_sem_bibliotecas`, `gpu_sem_cudnn`, `gpu_sem_placa`,
/// `gpu_driver_antigo`, `gpu_sem_memoria`, `gpu_nao_instalada`, `gpu_falhou`); molde em `explicar_queda`.
pub fn motivo(erro: &str) -> &'static str {
    let _ = erro;
    crate::edicao::mensagens::codigo::GPU_FALHOU
}
