//! Gerenciador de IA (Fase 2.1) — baixa o whisper.cpp + modelos de transcrição
//! SOB DEMANDA, de fontes OFICIAIS, com progresso e hash, e auto-configura os
//! caminhos em `AppSettings`.
//!
//! Princípio §13: nada automático. Catálogo curado (GitHub `ggml-org/whisper.cpp`
//! e Hugging Face `ggerganov/whisper.cpp`); o perito escolhe e instala; a
//! verificação de atualização é opt-in e só INFORMA (não instala). A versão do
//! motor continua registrada (reprodutibilidade).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::settings_commands::{get_app_settings, save_app_settings};
use crate::error::{Result, SicroError};

// ---------------------------------------------------------------------------
// Catálogo curado (fontes oficiais)

const WHISPER_VERSION: &str = "v1.8.5";

#[derive(Debug, Clone, Serialize)]
pub struct CatalogItem {
    pub id: &'static str,
    /// "build" (motor whisper.cpp) | "model" (modelo ggml).
    pub kind: &'static str,
    pub label: &'static str,
    pub url: &'static str,
    pub filename: &'static str,
    pub approx_mb: u32,
    pub is_zip: bool,
    pub version: &'static str,
    pub gpu: bool,
    pub lang: &'static str,
    pub note: &'static str,
}

const CATALOG: &[CatalogItem] = &[
    CatalogItem {
        id: "whisper-cuda",
        kind: "build",
        label: "Motor whisper.cpp — GPU NVIDIA (cuBLAS 12.4)",
        url: "https://github.com/ggml-org/whisper.cpp/releases/download/v1.8.5/whisper-cublas-12.4.0-bin-x64.zip",
        filename: "whisper-cublas.zip",
        approx_mb: 439,
        is_zip: true,
        version: WHISPER_VERSION,
        gpu: true,
        lang: "",
        note: "Requer GPU NVIDIA + driver atual. Degravação muito mais rápida.",
    },
    CatalogItem {
        id: "whisper-cpu",
        kind: "build",
        label: "Motor whisper.cpp — CPU (BLAS)",
        url: "https://github.com/ggml-org/whisper.cpp/releases/download/v1.8.5/whisper-blas-bin-x64.zip",
        filename: "whisper-blas.zip",
        approx_mb: 16,
        is_zip: true,
        version: WHISPER_VERSION,
        gpu: false,
        lang: "",
        note: "Funciona em qualquer máquina; mais lento (usa o processador).",
    },
    CatalogItem {
        id: "model-large-v3-turbo",
        kind: "model",
        label: "Modelo large-v3-turbo (melhor)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
        filename: "ggml-large-v3-turbo.bin",
        approx_mb: 1550,
        is_zip: false,
        version: "",
        gpu: false,
        lang: "multilíngue",
        note: "Melhor precisão + velocidade. Ideal com GPU.",
    },
    CatalogItem {
        id: "model-large-v3-turbo-q5",
        kind: "model",
        label: "Modelo large-v3-turbo (q5 — leve)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        filename: "ggml-large-v3-turbo-q5_0.bin",
        approx_mb: 574,
        is_zip: false,
        version: "",
        gpu: false,
        lang: "multilíngue",
        note: "Quase a mesma qualidade do turbo, ~1/3 do tamanho.",
    },
    CatalogItem {
        id: "model-small",
        kind: "model",
        label: "Modelo small (rápido)",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        filename: "ggml-small.bin",
        approx_mb: 488,
        is_zip: false,
        version: "",
        gpu: false,
        lang: "multilíngue",
        note: "Leve; precisão menor. Bom para testar o fluxo.",
    },
    CatalogItem {
        id: "vad-silero",
        kind: "vad",
        label: "VAD Silero — anti-alucinação",
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
        filename: "ggml-silero-v5.1.2.bin",
        approx_mb: 3,
        is_zip: false,
        version: "",
        gpu: false,
        lang: "",
        note: "Faz o whisper transcrever só onde há fala — evita texto inventado em ruído/silêncio.",
    },
];

fn catalog_find(id: &str) -> Result<&'static CatalogItem> {
    CATALOG
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| SicroError::Validation(format!("item de IA desconhecido: {id}")))
}

// ---------------------------------------------------------------------------
// Tipos de retorno

#[derive(Debug, Clone, Serialize)]
pub struct AiCatalog {
    pub gpu_detected: bool,
    pub items: Vec<CatalogItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledModel {
    pub filename: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiStatus {
    pub whisper_bin_path: String,
    pub whisper_ok: bool,
    pub whisper_version: String,
    pub model_path: String,
    pub model_ok: bool,
    pub installed_models: Vec<InstalledModel>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiUpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    id: String,
    received: u64,
    total: u64,
}

// ---------------------------------------------------------------------------
// Helpers

fn ai_base_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| SicroError::Filesystem(format!("data dir: {e}")))?
        .join("ai"))
}

/// Procura recursivamente um executável cujo nome contenha `needle`.
fn find_executable(dir: &Path, needle: &str) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_file() {
            if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                let low = name.to_lowercase();
                if low.contains(needle) && (low.ends_with(".exe") || !low.contains('.')) {
                    return Some(p);
                }
            }
        } else if p.is_dir() {
            subdirs.push(p);
        }
    }
    for d in subdirs {
        if let Some(f) = find_executable(&d, needle) {
            return Some(f);
        }
    }
    None
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| SicroError::Filesystem(format!("abrir zip: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SicroError::Validation(format!("zip inválido: {e}")))?;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| SicroError::Validation(format!("entrada de zip: {e}")))?;
        let rel = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).ok();
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut f = std::fs::File::create(&out)
                .map_err(|e| SicroError::Filesystem(format!("escrever {}: {e}", out.display())))?;
            std::io::copy(&mut entry, &mut f)
                .map_err(|e| SicroError::Filesystem(format!("extrair: {e}")))?;
        }
    }
    Ok(())
}

/// Agente HTTP com TLS nativo (SChannel/Secure Transport) — sem ring/OpenSSL.
fn http_agent() -> Result<ureq::Agent> {
    let connector = native_tls::TlsConnector::new()
        .map_err(|e| SicroError::Validation(format!("TLS indisponível: {e}")))?;
    Ok(ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .tls_connector(std::sync::Arc::new(connector))
        .build())
}

/// Download em streaming, com SHA-256 e progresso (`ai-download-progress`).
fn download_with_progress(app: &AppHandle, url: &str, dest: &Path, id: &str) -> Result<String> {
    let agent = http_agent()?;
    let resp = agent
        .get(url)
        .call()
        .map_err(|e| SicroError::Validation(format!("falha ao baixar ({url}): {e}")))?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest)
        .map_err(|e| SicroError::Filesystem(format!("criar arquivo: {e}")))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 65536];
    let mut received: u64 = 0;
    let mut last: u64 = 0;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| SicroError::Filesystem(format!("erro de leitura: {e}")))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| SicroError::Filesystem(format!("erro de escrita: {e}")))?;
        hasher.update(&buf[..n]);
        received += n as u64;
        if received - last >= 1_500_000 {
            let _ = app.emit(
                "ai-download-progress",
                ProgressPayload { id: id.to_string(), received, total },
            );
            last = received;
        }
    }
    file.flush().ok();
    let _ = app.emit(
        "ai-download-progress",
        ProgressPayload { id: id.to_string(), received, total },
    );
    Ok(format!("{:x}", hasher.finalize()))
}

// ---------------------------------------------------------------------------
// Comandos

/// Catálogo curado + se há GPU NVIDIA detectada (sugere o build cuBLAS).
#[tauri::command]
pub async fn get_ai_catalog() -> Result<AiCatalog> {
    let gpu_detected = which::which("nvidia-smi").is_ok();
    Ok(AiCatalog {
        gpu_detected,
        items: CATALOG.to_vec(),
    })
}

/// O que está instalado/configurado (caminhos + modelos presentes na pasta).
#[tauri::command]
pub async fn get_ai_status(app: AppHandle) -> Result<AiStatus> {
    let s = get_app_settings(app.clone()).await?;
    let whisper_bin_path = s.ai.whisper_bin_path.clone();
    let model_path = s.ai.model_path.clone();
    let whisper_ok = !whisper_bin_path.is_empty() && Path::new(&whisper_bin_path).is_file();
    let model_ok = !model_path.is_empty() && Path::new(&model_path).is_file();

    let mut installed_models = Vec::new();
    if let Ok(base) = ai_base_dir(&app) {
        if let Ok(rd) = std::fs::read_dir(base.join("models")) {
            for entry in rd.flatten() {
                let p = entry.path();
                if p.is_file() {
                    installed_models.push(InstalledModel {
                        filename: p
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string(),
                        size_bytes: std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
                    });
                }
            }
        }
    }
    Ok(AiStatus {
        whisper_bin_path,
        whisper_ok,
        whisper_version: s.ai.whisper_version,
        model_path,
        model_ok,
        installed_models,
    })
}

/// Baixa e instala um item do catálogo (com progresso/hash) e auto-configura
/// os caminhos em AppSettings. Devolve o status atualizado.
#[tauri::command]
pub async fn install_ai_asset(app: AppHandle, asset_id: String) -> Result<AiStatus> {
    let item = catalog_find(&asset_id)?;
    let base = ai_base_dir(&app)?;
    let sub = if item.kind == "build" { "bin" } else { "models" };
    let dest_dir = base.join(sub);
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| SicroError::Filesystem(format!("criar pasta IA: {e}")))?;

    let tmp = dest_dir.join(format!("{}.part", item.filename));
    let url = item.url.to_string();
    let id = asset_id.clone();
    let app2 = app.clone();
    let tmp2 = tmp.clone();
    let sha = tauri::async_runtime::spawn_blocking(move || {
        download_with_progress(&app2, &url, &tmp2, &id)
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa de download: {e}")))??;

    // Finaliza: extrai (build .zip) ou move (modelo .bin).
    let mut bin_path = String::new();
    let mut model_path = String::new();
    if item.is_zip {
        let extract_dir = dest_dir.join(item.id);
        let _ = std::fs::remove_dir_all(&extract_dir);
        std::fs::create_dir_all(&extract_dir).ok();
        extract_zip(&tmp, &extract_dir)?;
        let _ = std::fs::remove_file(&tmp);
        let exe = find_executable(&extract_dir, "whisper-cli").ok_or_else(|| {
            SicroError::Validation("whisper-cli não encontrado no pacote baixado".into())
        })?;
        bin_path = exe.to_string_lossy().to_string();
    } else {
        let final_path = dest_dir.join(item.filename);
        let _ = std::fs::remove_file(&final_path);
        std::fs::rename(&tmp, &final_path)
            .map_err(|e| SicroError::Filesystem(format!("finalizar modelo: {e}")))?;
        model_path = final_path.to_string_lossy().to_string();
    }

    // Auto-configura + registra o sha256 do que foi baixado (rastreabilidade).
    let mut s = get_app_settings(app.clone()).await?;
    match item.kind {
        "build" => {
            s.ai.whisper_bin_path = bin_path;
            s.ai.whisper_version = item.version.to_string();
        }
        "vad" => s.ai.vad_model_path = model_path,
        _ => s.ai.model_path = model_path,
    }
    save_app_settings(app.clone(), s).await?;
    tracing::info!("IA instalada: {asset_id} sha256={sha}");

    get_ai_status(app).await
}

/// Remove um item instalado e limpa a configuração correspondente.
#[tauri::command]
pub async fn remove_ai_asset(app: AppHandle, asset_id: String) -> Result<AiStatus> {
    let item = catalog_find(&asset_id)?;
    let base = ai_base_dir(&app)?;
    let mut s = get_app_settings(app.clone()).await?;
    match item.kind {
        "build" => {
            let _ = std::fs::remove_dir_all(base.join("bin").join(item.id));
            s.ai.whisper_bin_path = String::new();
            s.ai.whisper_version = String::new();
        }
        "vad" => {
            let _ = std::fs::remove_file(base.join("models").join(item.filename));
            if s.ai.vad_model_path.ends_with(item.filename) {
                s.ai.vad_model_path = String::new();
            }
        }
        _ => {
            let _ = std::fs::remove_file(base.join("models").join(item.filename));
            if s.ai.model_path.ends_with(item.filename) {
                s.ai.model_path = String::new();
            }
        }
    }
    save_app_settings(app.clone(), s).await?;
    get_ai_status(app).await
}

/// Busca a tag da última release do whisper.cpp no GitHub (vazio se falhar).
fn fetch_latest_tag() -> Result<String> {
    let agent = http_agent()?;
    let resp = agent
        .get("https://api.github.com/repos/ggml-org/whisper.cpp/releases/latest")
        .set("User-Agent", "SICRO")
        .call()
        .map_err(|e| SicroError::Validation(format!("consulta de atualização: {e}")))?;
    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| SicroError::Validation(format!("resposta inválida: {e}")))?;
    Ok(v.get("tag_name")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string())
}

/// OPT-IN: consulta a última release do whisper.cpp e compara com a versão
/// INSTALADA. Apenas INFORMA — a atualização é uma ação à parte do perito.
#[tauri::command]
pub async fn check_ai_updates(app: AppHandle) -> Result<AiUpdateInfo> {
    let s = get_app_settings(app).await?;
    let current = if s.ai.whisper_version.is_empty() {
        WHISPER_VERSION.to_string()
    } else {
        s.ai.whisper_version
    };
    let latest = tauri::async_runtime::spawn_blocking(fetch_latest_tag)
        .await
        .map_err(|e| SicroError::Validation(format!("tarefa de atualização: {e}")))??;
    Ok(AiUpdateInfo {
        update_available: !latest.is_empty() && latest != current,
        current,
        latest,
    })
}

/// OPT-IN: atualiza o motor whisper.cpp para a última release upstream,
/// reinstalando o MESMO build (GPU/CPU) com a versão trocada na URL do asset.
/// Registra a nova versão e o sha256 (rastreabilidade). §13: ação do perito —
/// nada é trocado automaticamente; a versão usada fica registrada.
#[tauri::command]
pub async fn update_whisper_engine(app: AppHandle) -> Result<AiStatus> {
    let s = get_app_settings(app.clone()).await?;
    let bin = s.ai.whisper_bin_path.clone();
    if bin.is_empty() {
        return Err(SicroError::Validation(
            "nenhum motor instalado para atualizar".into(),
        ));
    }
    let item = CATALOG
        .iter()
        .find(|i| i.kind == "build" && bin.contains(i.id))
        .ok_or_else(|| {
            SicroError::Validation("motor instalado não reconhecido no catálogo".into())
        })?;

    let latest = tauri::async_runtime::spawn_blocking(fetch_latest_tag)
        .await
        .map_err(|e| SicroError::Validation(format!("tarefa de atualização: {e}")))??;
    if latest.is_empty() {
        return Err(SicroError::Validation(
            "não foi possível obter a versão mais recente".into(),
        ));
    }
    let current = if s.ai.whisper_version.is_empty() {
        WHISPER_VERSION
    } else {
        s.ai.whisper_version.as_str()
    };
    if latest.as_str() == current {
        return Err(SicroError::Validation(format!(
            "o motor já está na versão mais recente ({latest})"
        )));
    }

    // Mesmo asset, versão trocada no caminho do release.
    let from = format!("/{}/", item.version);
    let to = format!("/{latest}/");
    let url = item.url.replace(from.as_str(), to.as_str());

    let base = ai_base_dir(&app)?;
    let dest_dir = base.join("bin");
    std::fs::create_dir_all(&dest_dir)
        .map_err(|e| SicroError::Filesystem(format!("criar pasta IA: {e}")))?;
    let tmp = dest_dir.join(format!("{}.part", item.filename));
    let id = item.id.to_string();
    let app2 = app.clone();
    let tmp2 = tmp.clone();
    let sha = tauri::async_runtime::spawn_blocking(move || {
        download_with_progress(&app2, &url, &tmp2, &id)
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa de download: {e}")))??;

    let extract_dir = dest_dir.join(item.id);
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir).ok();
    extract_zip(&tmp, &extract_dir)?;
    let _ = std::fs::remove_file(&tmp);
    let exe = find_executable(&extract_dir, "whisper-cli").ok_or_else(|| {
        SicroError::Validation(
            "whisper-cli não encontrado no pacote atualizado (o nome do arquivo pode ter mudado upstream)"
                .into(),
        )
    })?;

    let mut s = get_app_settings(app.clone()).await?;
    s.ai.whisper_bin_path = exe.to_string_lossy().to_string();
    s.ai.whisper_version = latest.clone();
    save_app_settings(app.clone(), s).await?;
    tracing::info!("motor IA atualizado para {latest} sha256={sha}");
    get_ai_status(app).await
}
