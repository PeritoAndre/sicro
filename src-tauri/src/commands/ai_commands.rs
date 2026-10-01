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
    /// Separação de locutores: pacote disponível para este sistema?
    pub diar_available: bool,
    pub diar_ok: bool,
    pub diar_version: String,
    pub diar_approx_mb: u32,
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
/// `base`/`grand_total` somam vários arquivos numa barra só (0 = só este).
fn download_with_progress(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    id: &str,
    base: u64,
    grand_total: u64,
) -> Result<String> {
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
            let _ = app.emit("ai-download-progress", progress(id, base + received, total, grand_total));
            last = received;
        }
    }
    file.flush().ok();
    let _ = app.emit("ai-download-progress", progress(id, base + received, total, grand_total));
    Ok(format!("{:x}", hasher.finalize()))
}

fn progress(id: &str, received: u64, total: u64, grand_total: u64) -> ProgressPayload {
    ProgressPayload {
        id: id.to_string(),
        received,
        total: if grand_total > 0 { grand_total } else { total },
    }
}

/// Extrai um .tar.bz2 em `dest` (`unpack_in` recusa caminhos que saiam dele).
fn extract_tar_bz2(path: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)
        .map_err(|e| SicroError::Filesystem(format!("criar {}: {e}", dest.display())))?;
    let file = std::fs::File::open(path)
        .map_err(|e| SicroError::Filesystem(format!("abrir pacote: {e}")))?;
    let mut archive = tar::Archive::new(bzip2::read::BzDecoder::new(std::io::BufReader::new(file)));
    let entries = archive
        .entries()
        .map_err(|e| SicroError::Validation(format!("pacote inválido: {e}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| SicroError::Validation(format!("pacote inválido: {e}")))?;
        entry
            .unpack_in(dest)
            .map_err(|e| SicroError::Filesystem(format!("extrair: {e}")))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Separação de locutores (sherpa-onnx) — programa + 2 modelos, de fontes
// oficiais (GitHub k2-fsa/sherpa-onnx), com tamanho e SHA-256 FIXOS: se o que
// chegar não conferir, nada é instalado. Roda local e offline depois.

const SHERPA_VERSION: &str = "v1.13.8";

struct DiarFile {
    url: &'static str,
    filename: &'static str,
    bytes: u64,
    sha256: &'static str,
}

/// Programa para este sistema (Windows: CRT estático, sem runtime do VC++).
fn diar_engine() -> Option<DiarFile> {
    if cfg!(target_os = "windows") {
        Some(DiarFile {
            url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.8/sherpa-onnx-v1.13.8-win-x64-shared-MT-Release-no-tts.tar.bz2",
            filename: "sherpa-onnx.tar.bz2",
            bytes: 23_271_851,
            sha256: "4b0a94f7b5c606b1b64a19a831c2127559e4b3d34e195465ebc7be73d9ed4783",
        })
    } else if cfg!(target_os = "linux") {
        Some(DiarFile {
            url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.8/sherpa-onnx-v1.13.8-linux-x64-shared-no-tts.tar.bz2",
            filename: "sherpa-onnx.tar.bz2",
            bytes: 24_802_494,
            sha256: "d0f96c8b65c6cd0974fada22737e337de81bc8cd2abbec2e39caf358b1eec5fc",
        })
    } else {
        None
    }
}

/// Segmentação (pyannote 3.0, licença MIT): onde há fala e troca de voz.
const DIAR_SEGMENTATION: DiarFile = DiarFile {
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2",
    filename: "segmentacao.tar.bz2",
    bytes: 6_958_444,
    sha256: "24615ee884c897d9d2ba09bb4d30da6bb1b15e685065962db5b02e76e4996488",
};

/// Assinatura de voz (WeSpeaker ResNet34, VoxCeleb — vozes de vários idiomas).
const DIAR_EMBEDDING: DiarFile = DiarFile {
    url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_resnet34_LM.onnx",
    filename: "wespeaker_en_voxceleb_resnet34_LM.onnx",
    bytes: 26_530_550,
    sha256: "e9848563da86f263117134dfd7ad63c92355b37de492b55e325400c9d9c39012",
};

const DIAR_EXE: &str = "sherpa-onnx-offline-speaker-diarization";

fn diar_total_bytes() -> u64 {
    diar_engine().map_or(0, |e| e.bytes) + DIAR_SEGMENTATION.bytes + DIAR_EMBEDDING.bytes
}

/// Baixa e instala o separador de locutores (uma barra para os 3 arquivos).
#[tauri::command]
pub async fn install_diarization(app: AppHandle) -> Result<AiStatus> {
    let engine = diar_engine().ok_or_else(|| {
        SicroError::Validation("separação de locutores não disponível neste sistema".into())
    })?;
    let dir = ai_base_dir(&app)?.join("diarizacao");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| SicroError::Filesystem(format!("criar pasta IA: {e}")))?;

    let app2 = app.clone();
    let dir2 = dir.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(PathBuf, PathBuf, PathBuf)> {
        let total = diar_total_bytes();
        let mut base = 0u64;
        let mut fetch = |f: &DiarFile| -> Result<PathBuf> {
            let tmp = dir2.join(format!("{}.part", f.filename));
            let sha = download_with_progress(&app2, f.url, &tmp, "diarizacao", base, total)?;
            base += f.bytes;
            if sha != f.sha256 {
                let _ = std::fs::remove_file(&tmp);
                return Err(SicroError::Validation(format!(
                    "o arquivo baixado ({}) não confere com o hash esperado — download corrompido ou alterado; nada foi instalado",
                    f.filename
                )));
            }
            let fin = dir2.join(f.filename);
            std::fs::rename(&tmp, &fin)
                .map_err(|e| SicroError::Filesystem(format!("finalizar download: {e}")))?;
            Ok(fin)
        };
        let eng = fetch(&engine)?;
        let seg = fetch(&DIAR_SEGMENTATION)?;
        let emb = fetch(&DIAR_EMBEDDING)?;

        extract_tar_bz2(&eng, &dir2.join("programa"))?;
        let _ = std::fs::remove_file(&eng);
        extract_tar_bz2(&seg, &dir2)?;
        let _ = std::fs::remove_file(&seg);

        let exe = find_executable(&dir2.join("programa"), DIAR_EXE).ok_or_else(|| {
            SicroError::Validation(format!("{DIAR_EXE} não encontrado no pacote baixado"))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755));
        }
        let seg_model = dir2.join("sherpa-onnx-pyannote-segmentation-3-0").join("model.onnx");
        if !seg_model.is_file() {
            return Err(SicroError::Validation("modelo de segmentação ausente no pacote".into()));
        }
        Ok((exe, seg_model, emb))
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa de download: {e}")))?;

    let (exe, seg_model, emb) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let mut s = get_app_settings(app.clone()).await?;
    s.ai.diar_bin_path = exe.to_string_lossy().to_string();
    s.ai.diar_segmentation_path = seg_model.to_string_lossy().to_string();
    s.ai.diar_embedding_path = emb.to_string_lossy().to_string();
    s.ai.diar_version = SHERPA_VERSION.to_string();
    save_app_settings(app.clone(), s).await?;
    tracing::info!("separação de locutores instalada: sherpa-onnx {SHERPA_VERSION}");
    get_ai_status(app).await
}

/// Remove o separador de locutores e limpa a configuração.
#[tauri::command]
pub async fn remove_diarization(app: AppHandle) -> Result<AiStatus> {
    let _ = std::fs::remove_dir_all(ai_base_dir(&app)?.join("diarizacao"));
    let mut s = get_app_settings(app.clone()).await?;
    s.ai.diar_bin_path = String::new();
    s.ai.diar_segmentation_path = String::new();
    s.ai.diar_embedding_path = String::new();
    s.ai.diar_version = String::new();
    save_app_settings(app.clone(), s).await?;
    get_ai_status(app).await
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
    let diar_ok = [&s.ai.diar_bin_path, &s.ai.diar_segmentation_path, &s.ai.diar_embedding_path]
        .iter()
        .all(|p| !p.is_empty() && Path::new(p.as_str()).is_file());
    Ok(AiStatus {
        whisper_bin_path,
        whisper_ok,
        whisper_version: s.ai.whisper_version,
        model_path,
        model_ok,
        installed_models,
        diar_available: diar_engine().is_some(),
        diar_ok,
        diar_version: s.ai.diar_version,
        diar_approx_mb: (diar_total_bytes() / 1_000_000) as u32,
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
        download_with_progress(&app2, &url, &tmp2, &id, 0, 0)
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
        download_with_progress(&app2, &url, &tmp2, &id, 0, 0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrai_tar_bz2_numa_pasta_nova() {
        let dir = std::env::temp_dir().join(format!("sicro-tarbz2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pkg = dir.join("p.tar.bz2");
        {
            let enc = bzip2::write::BzEncoder::new(std::fs::File::create(&pkg).unwrap(), bzip2::Compression::fast());
            let mut b = tar::Builder::new(enc);
            let data = b"ola";
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o755);
            h.set_cksum();
            b.append_data(&mut h, "pacote/bin/programa", &data[..]).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        // Destino ainda não existe (como "programa/" na instalação).
        extract_tar_bz2(&pkg, &dir.join("nova").join("programa")).unwrap();
        let out = dir.join("nova/programa/pacote/bin/programa");
        assert_eq!(std::fs::read(&out).unwrap(), b"ola");
        assert!(find_executable(&dir.join("nova"), "programa").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
