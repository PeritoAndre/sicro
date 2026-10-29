//! Gerenciador de IA: baixa whisper.cpp, modelos e o separador de locutores de
//! fontes oficiais, sob demanda, com progresso e hash, e configura os caminhos em
//! `AppSettings`. Nada automático: o perito escolhe e instala; atualização só informa.

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
    /// Pessoas no vídeo (onnxruntime + YOLOX): pacote para este sistema? instalado?
    pub pessoas_available: bool,
    pub pessoas_ok: bool,
    pub pessoas_version: String,
    pub pessoas_approx_mb: u32,
    /// Super-resolução (Real-ESRGAN): pacote para este sistema? instalado?
    pub superres_available: bool,
    pub superres_ok: bool,
    pub superres_version: String,
    pub superres_approx_mb: u32,
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
// Separação de locutores (sherpa-onnx): programa + 2 modelos de fontes oficiais,
// com tamanho e SHA-256 FIXOS — se não conferir, nada é instalado.

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
// Pessoas no vídeo: onnxruntime (MIT, Microsoft) + YOLOX-S (Apache-2.0, Megvii).
// Tamanho e SHA-256 FIXOS, como na separação de locutores.

const ORT_VERSION: &str = "1.31.1";

fn ort_package() -> Option<DiarFile> {
    if cfg!(target_os = "windows") {
        Some(DiarFile {
            url: "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-win-x64-1.31.1.zip",
            filename: "onnxruntime.zip",
            bytes: 82_929_944,
            sha256: "5869e26bb25314375f2d5ef1877e3ea06b1543d6c710e6e4a1ec2f7177ab4b2c",
        })
    } else if cfg!(target_os = "linux") {
        Some(DiarFile {
            url: "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-linux-x64-1.31.1.tgz",
            filename: "onnxruntime.tgz",
            bytes: 11_413_655,
            sha256: "c849e36a1ef3e22d4ff217b9d1ce4a3cf5b9467b975610913ae959ad5b16bdc9",
        })
    } else {
        None
    }
}

const YOLOX_S: DiarFile = DiarFile {
    url: "https://github.com/Megvii-BaseDetection/YOLOX/releases/download/0.1.1rc0/yolox_s.onnx",
    filename: "yolox_s.onnx",
    bytes: 35_858_002,
    sha256: "c5c2d13e59ae883e6af3b45daea64af4833a4951c92d116ec270d9ddbe998063",
};

fn pessoas_total_bytes() -> u64 {
    ort_package().map_or(0, |e| e.bytes) + YOLOX_S.bytes
}

/// Biblioteca e modelo existem no disco?
pub fn pessoas_ready(ai: &crate::commands::settings_commands::AiSettings) -> bool {
    [&ai.ort_lib_path, &ai.pessoas_model_path]
        .iter()
        .all(|p| !p.is_empty() && Path::new(p.as_str()).is_file())
}

/// Extrai um .tar.gz em `dest` (`unpack_in` recusa caminhos que saiam dele).
/// Só entram as entradas de `lib/` (cabeçalhos e cmake do pacote não servem a ninguém).
fn extract_tar_gz_lib(path: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)
        .map_err(|e| SicroError::Filesystem(format!("criar {}: {e}", dest.display())))?;
    let file = std::fs::File::open(path)
        .map_err(|e| SicroError::Filesystem(format!("abrir pacote: {e}")))?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(std::io::BufReader::new(file)));
    let entries = archive
        .entries()
        .map_err(|e| SicroError::Validation(format!("pacote inválido: {e}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| SicroError::Validation(format!("pacote inválido: {e}")))?;
        let na_lib = entry
            .path()
            .map(|p| p.components().any(|c| c.as_os_str() == "lib"))
            .unwrap_or(false);
        if !na_lib || !entry.header().entry_type().is_file() {
            continue;
        }
        entry
            .unpack_in(dest)
            .map_err(|e| SicroError::Filesystem(format!("extrair: {e}")))?;
    }
    Ok(())
}

/// A biblioteca do onnxruntime dentro da pasta extraída (arquivo de verdade, não o atalho).
fn find_ort_lib(dir: &Path) -> Option<PathBuf> {
    let lib = dir.join("lib");
    let rd = std::fs::read_dir(&lib).ok()?;
    let mut best: Option<PathBuf> = None;
    for e in rd.flatten() {
        let p = e.path();
        let name = p.file_name()?.to_str()?.to_lowercase();
        let meta = std::fs::symlink_metadata(&p).ok()?;
        let e_lib = name == "onnxruntime.dll"
            || (name.starts_with("libonnxruntime.so.") && name.matches('.').count() >= 3);
        if e_lib && meta.is_file() {
            best = Some(p);
        }
    }
    best
}

/// Baixa e instala "Pessoas no vídeo" (uma barra para os dois arquivos).
#[tauri::command]
pub async fn install_pessoas(app: AppHandle) -> Result<AiStatus> {
    let pkg = ort_package().ok_or_else(|| {
        SicroError::Validation("pessoas no vídeo não disponível neste sistema".into())
    })?;
    let dir = ai_base_dir(&app)?.join("pessoas");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| SicroError::Filesystem(format!("criar pasta IA: {e}")))?;

    let app2 = app.clone();
    let dir2 = dir.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(PathBuf, PathBuf)> {
        let total = pessoas_total_bytes();
        let mut base = 0u64;
        let mut fetch = |f: &DiarFile| -> Result<PathBuf> {
            let tmp = dir2.join(format!("{}.part", f.filename));
            let sha = download_with_progress(&app2, f.url, &tmp, "pessoas", base, total)?;
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
        let ort_pkg = fetch(&pkg)?;
        let model = fetch(&YOLOX_S)?;
        let out = dir2.join("onnxruntime");
        if pkg.filename.ends_with(".zip") {
            std::fs::create_dir_all(&out).ok();
            extract_zip(&ort_pkg, &out)?;
        } else {
            extract_tar_gz_lib(&ort_pkg, &out)?;
        }
        let _ = std::fs::remove_file(&ort_pkg);
        // o pacote traz uma pasta com o nome da versão
        let raiz = std::fs::read_dir(&out)
            .ok()
            .and_then(|mut r| r.find_map(|e| e.ok().map(|e| e.path()).filter(|p| p.is_dir())))
            .unwrap_or(out.clone());
        let lib = find_ort_lib(&raiz)
            .ok_or_else(|| SicroError::Validation("biblioteca do onnxruntime não encontrada no pacote".into()))?;
        Ok((lib, model))
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa de download: {e}")))?;

    let (lib, model) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let mut s = get_app_settings(app.clone()).await?;
    s.ai.ort_lib_path = lib.to_string_lossy().to_string();
    s.ai.pessoas_model_path = model.to_string_lossy().to_string();
    s.ai.pessoas_version = format!("onnxruntime {ORT_VERSION} + YOLOX-S");
    save_app_settings(app.clone(), s).await?;
    tracing::info!("pessoas no vídeo instalado: onnxruntime {ORT_VERSION}");
    get_ai_status(app).await
}

/// Remove "Pessoas no vídeo" e limpa a configuração (as trilhas dos casos ficam).
#[tauri::command]
pub async fn remove_pessoas(app: AppHandle) -> Result<AiStatus> {
    let _ = std::fs::remove_dir_all(ai_base_dir(&app)?.join("pessoas"));
    let mut s = get_app_settings(app.clone()).await?;
    s.ai.ort_lib_path = String::new();
    s.ai.pessoas_model_path = String::new();
    s.ai.pessoas_version = String::new();
    save_app_settings(app.clone(), s).await?;
    get_ai_status(app).await
}

// ---------------------------------------------------------------------------
// Super-resolução (5.3): Real-ESRGAN general-x4v3 (BSD-3, Xintao Wang) em ONNX, pelo mesmo
// onnxruntime das Pessoas (reaproveitado quando já está instalado). Só CPU.

const REALESRGAN_X4V3: DiarFile = DiarFile {
    url: "https://huggingface.co/OwlMaster/AllFilesRope/resolve/d783e61585b3d83a85c91ca8a3b299e8ade94d72/realesr-general-x4v3.onnx",
    filename: "realesr-general-x4v3.onnx",
    bytes: 4_871_181,
    sha256: "09b757accd747d7e423c1d352b3e8f23e77cc5742d04bae958d4eb8082b76fa4",
};

/// Biblioteca do onnxruntime que a super-resolução usa: a própria, senão a das Pessoas.
pub fn superres_lib(ai: &crate::commands::settings_commands::AiSettings) -> Option<PathBuf> {
    [&ai.sr_ort_lib_path, &ai.ort_lib_path]
        .iter()
        .map(|p| PathBuf::from(p.as_str()))
        .find(|p| !p.as_os_str().is_empty() && p.is_file())
}

pub fn superres_ready(ai: &crate::commands::settings_commands::AiSettings) -> bool {
    superres_lib(ai).is_some()
        && !ai.sr_model_path.is_empty()
        && Path::new(ai.sr_model_path.as_str()).is_file()
}

fn superres_total_bytes(precisa_ort: bool) -> u64 {
    REALESRGAN_X4V3.bytes + if precisa_ort { ort_package().map_or(0, |e| e.bytes) } else { 0 }
}

#[tauri::command]
pub async fn install_superres(app: AppHandle) -> Result<AiStatus> {
    let pkg = ort_package().ok_or_else(|| {
        SicroError::Validation("super-resolução não disponível neste sistema".into())
    })?;
    let atual = get_app_settings(app.clone()).await?;
    let ja_tem_ort = atual.ai.ort_lib_path.as_str() != "" && Path::new(atual.ai.ort_lib_path.as_str()).is_file();
    let dir = ai_base_dir(&app)?.join("superres");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| SicroError::Filesystem(format!("criar pasta IA: {e}")))?;

    let app2 = app.clone();
    let dir2 = dir.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(Option<PathBuf>, PathBuf)> {
        let total = superres_total_bytes(!ja_tem_ort);
        let mut base = 0u64;
        let mut fetch = |f: &DiarFile| -> Result<PathBuf> {
            let tmp = dir2.join(format!("{}.part", f.filename));
            let sha = download_with_progress(&app2, f.url, &tmp, "superres", base, total)?;
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
        let lib = if ja_tem_ort {
            None
        } else {
            let ort_pkg = fetch(&pkg)?;
            let out = dir2.join("onnxruntime");
            if pkg.filename.ends_with(".zip") {
                std::fs::create_dir_all(&out).ok();
                extract_zip(&ort_pkg, &out)?;
            } else {
                extract_tar_gz_lib(&ort_pkg, &out)?;
            }
            let _ = std::fs::remove_file(&ort_pkg);
            let raiz = std::fs::read_dir(&out)
                .ok()
                .and_then(|mut r| r.find_map(|e| e.ok().map(|e| e.path()).filter(|p| p.is_dir())))
                .unwrap_or(out.clone());
            Some(find_ort_lib(&raiz).ok_or_else(|| {
                SicroError::Validation("biblioteca do onnxruntime não encontrada no pacote".into())
            })?)
        };
        let model = fetch(&REALESRGAN_X4V3)?;
        Ok((lib, model))
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa de download: {e}")))?;

    let (lib, model) = match result {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    let mut s = get_app_settings(app.clone()).await?;
    s.ai.sr_ort_lib_path = lib.map(|l| l.to_string_lossy().to_string()).unwrap_or_default();
    s.ai.sr_model_path = model.to_string_lossy().to_string();
    s.ai.sr_version = format!("Real-ESRGAN general-x4v3 + onnxruntime {ORT_VERSION}");
    save_app_settings(app.clone(), s).await?;
    tracing::info!("super-resolução instalada");
    get_ai_status(app).await
}

#[tauri::command]
pub async fn remove_superres(app: AppHandle) -> Result<AiStatus> {
    let _ = std::fs::remove_dir_all(ai_base_dir(&app)?.join("superres"));
    let mut s = get_app_settings(app.clone()).await?;
    s.ai.sr_ort_lib_path = String::new();
    s.ai.sr_model_path = String::new();
    s.ai.sr_version = String::new();
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
    let pessoas_ok = pessoas_ready(&s.ai);
    let sr_version = s.ai.sr_version.clone();
    let (sr_ok, sr_mb) = (superres_ready(&s.ai), superres_total_bytes(s.ai.ort_lib_path.is_empty()) / 1_000_000);
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
        pessoas_available: ort_package().is_some(),
        pessoas_ok,
        pessoas_version: s.ai.pessoas_version,
        pessoas_approx_mb: (pessoas_total_bytes() / 1_000_000) as u32,
        superres_available: ort_package().is_some(),
        superres_ok: sr_ok,
        superres_version: sr_version,
        superres_approx_mb: sr_mb as u32,
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

/// OPT-IN: reinstala o MESMO build (GPU/CPU) com a versão trocada na URL do
/// release. Ação do perito — nada é trocado automaticamente; a versão fica registrada.
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

    #[test]
    fn extrai_so_a_lib_do_tgz_e_acha_a_biblioteca() {
        let dir = std::env::temp_dir().join(format!("sicro-tgz-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pkg = dir.join("p.tgz");
        {
            let enc = flate2::write::GzEncoder::new(std::fs::File::create(&pkg).unwrap(), flate2::Compression::fast());
            let mut b = tar::Builder::new(enc);
            for (nome, dados) in [
                ("onnxruntime-x/include/cabecalho.h", &b"h"[..]),
                ("onnxruntime-x/lib/libonnxruntime.so.1.31.1", &b"lib"[..]),
                ("onnxruntime-x/lib/libonnxruntime_providers_shared.so", &b"p"[..]),
            ] {
                let mut h = tar::Header::new_gnu();
                h.set_size(dados.len() as u64);
                h.set_mode(0o644);
                h.set_cksum();
                b.append_data(&mut h, nome, dados).unwrap();
            }
            b.into_inner().unwrap().finish().unwrap();
        }
        extract_tar_gz_lib(&pkg, &dir.join("out")).unwrap();
        assert!(!dir.join("out/onnxruntime-x/include").exists());
        let lib = find_ort_lib(&dir.join("out/onnxruntime-x")).unwrap();
        assert!(lib.ends_with("libonnxruntime.so.1.31.1"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
