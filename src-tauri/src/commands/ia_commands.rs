//! Comandos da IA (§6.3, §6.5): estado para o Gerenciador, aceleração por GPU (instalar, remover, autoteste em
//! processo filho) e o item "Rastreador" (EdgeTAM; arquivos e SHA-256 em `edicao::rastreio::edgetam::ARQUIVOS`).
//! O interruptor "Usar modelos de IA" grava `AiSettings.modelos_desligados` pelas Configurações. Progresso dos
//! downloads pelo evento `ai-download-progress` (`id` = "gpu" | "rastreador"). Dono: P2.1.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::settings_commands::{get_app_settings, save_app_settings, AiSettings};
use crate::edicao::mensagens::{self, codigo};
use crate::edicao::rastreio::edgetam;
use crate::error::{Result, SicroError};
use crate::ia::gpu::{self, PacoteGpu, Placa};
use crate::ia::pacote::{self, Arquivo, Fonte};
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

#[derive(Clone, Serialize)]
struct Progresso {
    id: &'static str,
    received: u64,
    total: u64,
}

/// Sem botão de cancelar na tela: a bandeira existe para o download parar limpo se um dia houver.
static CANCELAR: AtomicBool = AtomicBool::new(false);

fn pasta_ai(app: &AppHandle) -> Result<PathBuf> {
    Ok(app.path().app_local_data_dir().map_err(|e| SicroError::Filesystem(format!("data dir: {e}")))?.join("ai"))
}

fn erro(e: SicroError) -> SicroError {
    mensagens::erro_de_comando(e)
}

fn codigo_erro(c: &str) -> SicroError {
    erro(SicroError::Validation(c.to_string()))
}

/// Os 4 grafos do EdgeTAM com o tamanho certo em `ai.rastreio_dir` (o SHA-256 é conferido ao abrir a sessão).
pub(crate) fn rastreador_instalado(ai: &AiSettings) -> bool {
    !ai.rastreio_dir.is_empty()
        && edgetam::ARQUIVOS.iter().all(|a| std::fs::metadata(Path::new(&ai.rastreio_dir).join(a.nome)).is_ok_and(|m| m.is_file() && m.len() == a.bytes))
}

fn rastreador_mb() -> u32 {
    (edgetam::ARQUIVOS.iter().map(|a| a.bytes).sum::<u64>() / 1_000_000) as u32
}

/// Os arquivos do item "Rastreador" pela URL fixa (`URL_BASE` + nome).
pub(crate) fn arquivos_rastreador() -> Vec<Arquivo> {
    edgetam::ARQUIVOS.iter().map(|a| Arquivo { nome: a.nome.into(), url: format!("{}{}", edgetam::URL_BASE, a.nome), bytes: a.bytes, sha256: a.sha256.into() }).collect()
}

/// O estado para a tela, com a placa dada (None = sem placa NVIDIA).
pub(crate) fn estado_de(ai: &AiSettings, placa: Option<Placa>) -> EstadoIa {
    estado_com(ai, placa, crate::ia::sessao::provedor_carregado())
}

/// `estado_de` com o onnxruntime já carregado neste processo dado (None = nenhum).
fn estado_com(ai: &AiSettings, placa: Option<Placa>, carregada: Option<Provedor>) -> EstadoIa {
    let dir = (!ai.gpu_dir.is_empty()).then(|| PathBuf::from(&ai.gpu_dir));
    let instalada = dir.as_deref().and_then(PacoteGpu::da_pasta).is_some();
    let requisito = match &placa {
        None => Some(codigo::GPU_SEM_PLACA.to_string()),
        Some(p) => gpu::requisito(p).map(str::to_string),
    };
    let pronta = instalada && gpu::pronta_com_placa(ai, placa.as_ref()).is_some();
    let motivo = if instalada && !pronta {
        dir.as_deref().and_then(gpu::estado).and_then(|e| e.autoteste).and_then(|a| a.motivo).or_else(|| requisito.clone())
    } else {
        None
    };
    EstadoIa {
        ligada: crate::ia::ligada(ai),
        gpu: EstadoGpuIa { disponivel: gpu::arquivos().is_some(), placa, requisito, instalada, pronta, motivo, mb: gpu::mb() },
        rastreador: rastreador_instalado(ai),
        rastreador_mb: rastreador_mb(),
        carregada,
        // a lib de GPU só entra num processo que ainda não carregou a de CPU
        reabrir: pronta && carregada == Some(Provedor::Cpu),
    }
}

async fn estado_atual(app: &AppHandle) -> Result<EstadoIa> {
    let ai = get_app_settings(app.clone()).await?.ai;
    tauri::async_runtime::spawn_blocking(move || estado_de(&ai, gpu::placa())).await.map_err(|e| SicroError::Validation(format!("estado da IA: {e}")))
}

#[tauri::command]
pub async fn ia_estado(app: AppHandle) -> Result<EstadoIa> {
    estado_atual(&app).await.map_err(erro)
}

/// Baixa e extrai `ai/gpu/` e roda o autoteste no filho (`sicro --autoteste-gpu`).
#[tauri::command]
pub async fn ia_gpu_instalar(app: AppHandle) -> Result<EstadoIa> {
    gpu::arquivos().ok_or_else(|| codigo_erro(&format!("{}: gpu", codigo::NAO_SUPORTADO)))?;
    let placa = gpu::placa().ok_or_else(|| codigo_erro(codigo::GPU_SEM_PLACA))?;
    if let Some(c) = gpu::requisito(&placa) {
        return Err(codigo_erro(c));
    }
    let dir = pasta_ai(&app)?.join("gpu");
    let app2 = app.clone();
    let dir2 = dir.clone();
    CANCELAR.store(false, std::sync::atomic::Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let mut ultimo = 0u64;
        gpu::instalar(&dir2, &CANCELAR, &mut |r, t| {
            if r - ultimo >= 4_000_000 || r == t {
                ultimo = r;
                let _ = app2.emit("ai-download-progress", Progresso { id: "gpu", received: r, total: t });
            }
        })?;
        gpu::testar_e_gravar(&dir2, Some(placa)).map(|_| ())
    })
    .await
    .map_err(|e| SicroError::Validation(format!("instalação: {e}")))?
    .map_err(erro)?;
    let mut s = get_app_settings(app.clone()).await?;
    s.ai.gpu_dir = dir.to_string_lossy().into_owned();
    save_app_settings(app.clone(), s).await?;
    tracing::info!(target: "ia", "aceleração por GPU instalada em {}", dir.display());
    estado_atual(&app).await.map_err(erro)
}

#[tauri::command]
pub async fn ia_gpu_remover(app: AppHandle) -> Result<EstadoIa> {
    let mut s = get_app_settings(app.clone()).await?;
    let dir = if s.ai.gpu_dir.is_empty() { pasta_ai(&app)?.join("gpu") } else { PathBuf::from(&s.ai.gpu_dir) };
    gpu::remover(&dir).map_err(erro)?;
    s.ai.gpu_dir = String::new();
    save_app_settings(app.clone(), s).await?;
    estado_atual(&app).await.map_err(erro)
}

/// "Testar de novo": autoteste no processo filho; grava o resultado em `ai/gpu/estado.json`.
#[tauri::command]
pub async fn ia_gpu_autoteste(app: AppHandle) -> Result<EstadoIa> {
    let ai = get_app_settings(app.clone()).await?.ai;
    let dir = PathBuf::from(&ai.gpu_dir);
    if ai.gpu_dir.is_empty() || PacoteGpu::da_pasta(&dir).is_none() {
        return Err(codigo_erro(codigo::GPU_NAO_INSTALADA));
    }
    tauri::async_runtime::spawn_blocking(move || gpu::testar_e_gravar(&dir, gpu::placa()))
        .await
        .map_err(|e| SicroError::Validation(format!("autoteste: {e}")))?
        .map_err(erro)?;
    estado_atual(&app).await.map_err(erro)
}

/// Item "Rastreador" (EdgeTAM vídeo, 65,2 MB) em `ai/rastreio/`; grava `AiSettings.rastreio_dir`.
#[tauri::command]
pub async fn ia_rastreador_instalar(app: AppHandle) -> Result<EstadoIa> {
    let dir = pasta_ai(&app)?.join("rastreio");
    let app2 = app.clone();
    let dir2 = dir.clone();
    CANCELAR.store(false, std::sync::atomic::Ordering::Relaxed);
    tauri::async_runtime::spawn_blocking(move || {
        let mut ultimo = 0u64;
        pacote::instalar_arquivos(&arquivos_rastreador(), &Fonte::Rede, &dir2, &CANCELAR, &mut |r, t| {
            if r - ultimo >= 1_500_000 || r == t {
                ultimo = r;
                let _ = app2.emit("ai-download-progress", Progresso { id: "rastreador", received: r, total: t });
            }
        })
    })
    .await
    .map_err(|e| SicroError::Validation(format!("instalação: {e}")))?
    .map_err(erro)?;
    let mut s = get_app_settings(app.clone()).await?;
    s.ai.rastreio_dir = dir.to_string_lossy().into_owned();
    s.ai.rastreio_version = format!("EdgeTAM vídeo {}", &edgetam::COMMIT[..8]);
    save_app_settings(app.clone(), s).await?;
    estado_atual(&app).await.map_err(erro)
}

#[tauri::command]
pub async fn ia_rastreador_remover(app: AppHandle) -> Result<EstadoIa> {
    let mut s = get_app_settings(app.clone()).await?;
    let dir = if s.ai.rastreio_dir.is_empty() { pasta_ai(&app)?.join("rastreio") } else { PathBuf::from(&s.ai.rastreio_dir) };
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| codigo_erro(&format!("{}: {e}", codigo::REABRIR)))?;
    }
    s.ai.rastreio_dir = String::new();
    s.ai.rastreio_version = String::new();
    save_app_settings(app.clone(), s).await?;
    estado_atual(&app).await.map_err(erro)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::ia::testes as t;

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

    fn placa(cc: &str, driver: &str) -> Placa {
        Placa { nome: "NVIDIA X".into(), driver: driver.into(), compute_cap: cc.into(), memoria_mb: 8192 }
    }

    /// O que a tela recebe: interruptor, requisito da placa, item não instalado, tamanhos fixos, e o motivo do
    /// último autoteste quando a pasta está lá e não ficou pronta.
    #[test]
    fn estado_para_a_tela() {
        let ai = AiSettings { modelos_desligados: true, ..Default::default() };
        let e = estado_de(&ai, None);
        assert!(!e.ligada && !e.gpu.instalada && !e.gpu.pronta && !e.rastreador && !e.reabrir);
        assert_eq!(e.gpu.requisito.as_deref(), Some(codigo::GPU_SEM_PLACA));
        assert_eq!(e.gpu.mb, gpu::mb());
        assert!(e.gpu.mb >= 1_200 && e.rastreador_mb == 65);
        assert_eq!(estado_de(&AiSettings::default(), Some(placa("6.1", "610.1"))).gpu.requisito.as_deref(), Some(codigo::GPU_PLACA_ANTIGA));
        assert_eq!(estado_de(&AiSettings::default(), Some(placa("8.9", "570.1"))).gpu.requisito.as_deref(), Some(codigo::GPU_DRIVER_ANTIGO));
        assert_eq!(estado_de(&AiSettings::default(), Some(placa("8.9", "610.1"))).gpu.requisito, None);
        assert!(estado_de(&AiSettings::default(), None).ligada);

        // pasta instalada, autoteste reprovado: o motivo vem dele
        let d = std::env::temp_dir().join(format!("p21-estado-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for sub in ["onnxruntime/lib", "cuda", "cudnn"] {
            std::fs::create_dir_all(d.join(sub)).unwrap();
        }
        let lib = if cfg!(windows) { "onnxruntime.dll" } else { "libonnxruntime.so.1.31.1" };
        std::fs::write(d.join("onnxruntime/lib").join(lib), b"x").unwrap();
        let reprovado = gpu::Autoteste { pronta: false, provedor: Provedor::Cpu, motivo: Some(codigo::GPU_SEM_CUDNN.into()), detalhe: Some("texto".into()), ms_sessao: 0.0, ms_primeira: 0.0, ms_segunda: 0.0, nos_gpu: 0, nos_total: 0, dif_max: None };
        let p = placa("8.9", "610.57.04");
        gpu::gravar_estado(&d, &gpu::EstadoGpu { variante: "cuda13".into(), onnxruntime: "1.31.1".into(), placa: Some(p.clone()), autoteste: Some(reprovado), testado_em: None }).unwrap();
        let ai = AiSettings { gpu_dir: d.to_string_lossy().into_owned(), ..Default::default() };
        let e = estado_de(&ai, Some(p.clone()));
        assert!(e.gpu.instalada && !e.gpu.pronta);
        assert_eq!(e.gpu.motivo.as_deref(), Some(codigo::GPU_SEM_CUDNN));
        assert!(!estado_com(&ai, Some(p.clone()), Some(Provedor::Cpu)).reabrir);

        // aprovada com a placa de agora: pronta; "Reabrir o SICRO" só com o onnxruntime de CPU já carregado
        let aprovado = gpu::Autoteste { pronta: true, provedor: Provedor::Cuda, motivo: None, detalhe: None, ms_sessao: 1.0, ms_primeira: 1.0, ms_segunda: 1.0, nos_gpu: 4, nos_total: 4, dif_max: Some(0.0) };
        gpu::gravar_estado(&d, &gpu::EstadoGpu { variante: "cuda13".into(), onnxruntime: "1.31.1".into(), placa: Some(p.clone()), autoteste: Some(aprovado), testado_em: None }).unwrap();
        let reabrir = |c: Option<Provedor>| {
            let e = estado_com(&ai, Some(p.clone()), c);
            assert!(e.gpu.instalada && e.gpu.pronta && e.gpu.motivo.is_none(), "{e:?}");
            (e.carregada, e.reabrir)
        };
        assert_eq!(reabrir(Some(Provedor::Cpu)), (Some(Provedor::Cpu), true));
        assert_eq!(reabrir(Some(Provedor::Cuda)), (Some(Provedor::Cuda), false));
        assert_eq!(reabrir(None), (None, false));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Item "Rastreador": os 4 grafos pela URL fixa do contrato, instalados a partir dos modelos do dossiê (sem rede)
    /// com SHA-256 e tamanho conferidos; arquivo de tamanho errado não conta como instalado.
    #[test]
    fn rastreador_instala_dos_modelos_do_dossie() {
        let a = arquivos_rastreador();
        assert_eq!(a.len(), 4);
        assert!(a.iter().all(|x| x.url.starts_with("https://huggingface.co/jax-image-tools/edgetam-video-onnx/resolve/8ca3d3e4") && x.url.ends_with(&x.nome)));
        let Some(m) = t::ou_dispensa(t::modelos(), "rastreador_instala_dos_modelos_do_dossie", "a pasta de modelos") else { return };
        let origem = m.join("rastreio/edgetam_video");
        let d = std::env::temp_dir().join(format!("p21-rastreador-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let dir = d.join("rastreio");
        let nao = AtomicBool::new(false);
        pacote::instalar_arquivos(&a, &Fonte::Pastas(vec![origem]), &dir, &nao, &mut |_, _| {}).unwrap();
        let ai = AiSettings { rastreio_dir: dir.to_string_lossy().into_owned(), ..Default::default() };
        assert!(rastreador_instalado(&ai));
        for x in edgetam::ARQUIVOS {
            assert!(t::conferir_sha256(&dir.join(x.nome), x.sha256), "{}", x.nome);
        }
        std::fs::write(dir.join(edgetam::ARQUIVOS[0].nome), b"truncado").unwrap();
        assert!(!rastreador_instalado(&ai));
        assert!(estado_de(&AiSettings::default(), None).rastreador_mb == rastreador_mb());
        let _ = std::fs::remove_dir_all(&d);
    }
}
