//! Comandos da IA (§6.3, §6.5): estado para o Gerenciador, aceleração por GPU (instalar, remover, autoteste em
//! processo filho) e o item "Rastreador" (EdgeTAM; arquivos e SHA-256 em `edicao::rastreio::edgetam::ARQUIVOS`).
//! O interruptor "Usar modelos de IA" grava `AiSettings.modelos_desligados` pelas Configurações. Progresso dos
//! downloads pelo evento `ai-download-progress` (`id` = "gpu" | "rastreador"). Dono: P2.1.
//! Abertura: os corpos recusam com `nao_suportado: ia` até o P2.1 entregar.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::edicao::mensagens::{self, codigo};
use crate::error::{Result, SicroError};
use crate::ia::gpu::Placa;
use crate::ia::Provedor;

/// Aceleração por GPU na tela (`gpu-inferencia.md` §7.3). Códigos do catálogo em `requisito` e `motivo`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstadoGpuIa {
    /// Há pacote para este sistema (Linux e Windows x64).
    pub disponivel: bool,
    pub placa: Option<Placa>,
    /// Placa que não atende: `gpu_sem_placa`, `gpu_placa_antiga`, `gpu_driver_antigo`; None = atende.
    pub requisito: Option<String>,
    pub instalada: bool,
    pub pronta: bool,
    /// Por que o último autoteste reprovou (`gpu_*`).
    pub motivo: Option<String>,
    pub mb: u32,
}

/// Estado da IA para o Gerenciador.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EstadoIa {
    /// Interruptor "Usar modelos de IA".
    pub ligada: bool,
    pub gpu: EstadoGpuIa,
    /// Rastreador (EdgeTAM) instalado e o tamanho do download.
    pub rastreador: bool,
    pub rastreador_mb: u32,
    /// onnxruntime carregado nesta sessão (None = nenhum ainda).
    pub carregada: Option<Provedor>,
    /// Instalação feita com outro onnxruntime já carregado: vale ao reabrir o SICRO (`reabrir`).
    pub reabrir: bool,
}

fn recusa() -> SicroError {
    mensagens::erro_de_comando(SicroError::Validation(format!("{}: ia", codigo::NAO_SUPORTADO)))
}

#[tauri::command]
pub async fn ia_estado(app: AppHandle) -> Result<EstadoIa> {
    let _ = app;
    Err(recusa())
}

/// Baixa e extrai `ai/gpu/` e roda o autoteste no filho (`sicro --autoteste-gpu`).
#[tauri::command]
pub async fn ia_gpu_instalar(app: AppHandle) -> Result<EstadoIa> {
    let _ = app;
    Err(recusa())
}

#[tauri::command]
pub async fn ia_gpu_remover(app: AppHandle) -> Result<EstadoIa> {
    let _ = app;
    Err(recusa())
}

/// "Testar de novo": autoteste no processo filho; grava o resultado em `ai/gpu/estado.json`.
#[tauri::command]
pub async fn ia_gpu_autoteste(app: AppHandle) -> Result<EstadoIa> {
    let _ = app;
    Err(recusa())
}

/// Item "Rastreador" (EdgeTAM vídeo, 65,2 MB) em `ai/rastreio/`; grava `AiSettings.rastreio_dir`.
#[tauri::command]
pub async fn ia_rastreador_instalar(app: AppHandle) -> Result<EstadoIa> {
    let _ = app;
    Err(recusa())
}

#[tauri::command]
pub async fn ia_rastreador_remover(app: AppHandle) -> Result<EstadoIa> {
    let _ = app;
    Err(recusa())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn comandos_registraveis() {
        let _h: Box<dyn Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync> = Box::new(tauri::generate_handler![
            ia_estado,
            ia_gpu_instalar,
            ia_gpu_remover,
            ia_gpu_autoteste,
            ia_rastreador_instalar,
            ia_rastreador_remover,
        ]);
    }
}
