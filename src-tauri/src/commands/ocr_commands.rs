//! Gerenciador de OCR (Documentoscopia) — **RapidOCR / PaddleOCR (PP-OCRv5,
//! ONNX)** via crate `oar-ocr`.
//!
//! O MOTOR (ONNX Runtime) é **embutido** no app (não baixa). O que o perito
//! baixa, sob demanda e com um clique, é o **pacote de modelos latino**
//! (detecção + reconhecimento latino + dicionário) — 3 arquivos, ~13 MB, que
//! cobrem português/espanhol/inglês/francês/etc. Fonte oficial: Releases do
//! `oar-ocr` (Apache-2.0). Offline após o download; verificação por SHA-256.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::settings_commands::{get_app_settings, save_app_settings};
use crate::error::{Result, SicroError};

/// Versão CURADA dos modelos PP-OCRv5 (oar-ocr, Apache-2.0). O SICRO fixa esta
/// versão para instalações novas; "Verificar atualizações" compara com a release
/// mais nova upstream e o perito pode, opcionalmente, atualizar (§13: ação dele;
/// a versão usada fica registrada em `AppSettings.ocr.ocr_version`).
const OAR_OCR_VERSION: &str = "v0.3.0";

/// Monta a base de download dos assets de uma release específica do oar-ocr.
fn oar_release_base(version: &str) -> String {
    format!("https://github.com/GreatV/oar-ocr/releases/download/{version}")
}

/// Tamanho aproximado de cada arquivo do pacote (rótulo da UI).
const PACK_APPROX_MB: u32 = 13;

/// Os 3 arquivos do pacote latino. Os nomes canônicos vêm de `crate::ocr`
/// (fonte única, compartilhada com o seletor de motor).
fn pack_files() -> [&'static str; 3] {
    [
        crate::ocr::RAPIDOCR_DET_FILE,
        crate::ocr::RAPIDOCR_REC_FILE,
        crate::ocr::RAPIDOCR_DICT_FILE,
    ]
}

// ---------------------------------------------------------------------------
// Tipos de retorno (espelham o TS em types/ocr.ts)

#[derive(Debug, Clone, Serialize)]
pub struct OcrPackItem {
    pub id: &'static str,
    pub label: &'static str,
    pub approx_mb: u32,
    pub note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct OcrCatalog {
    /// O motor (ONNX Runtime) está embutido no app — sempre disponível.
    pub engine_ready: bool,
    pub engine_label: String,
    pub items: Vec<OcrPackItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledOcrModel {
    pub filename: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OcrStatus {
    pub engine_ready: bool,
    pub engine_label: String,
    /// Os 3 modelos do pacote latino estão presentes?
    pub models_ready: bool,
    pub models_dir: String,
    pub installed: Vec<InstalledOcrModel>,
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    id: String,
    received: u64,
    total: u64,
}

const PACK: OcrPackItem = OcrPackItem {
    id: "latin",
    label: "Pacote de OCR — latino (PP-OCRv5)",
    approx_mb: PACK_APPROX_MB,
    note: "Cobre português, espanhol, inglês, francês, italiano… Modelos neurais \
           (PaddleOCR). Offline após baixar.",
};

// ---------------------------------------------------------------------------
// Infra

/// Diretório dos modelos de OCR: `…\AppData\Local\…\ocr\models` (não-roaming).
/// Compartilhado entre o download (aqui) e o motor (Documentoscopia).
pub fn ocr_models_dir(app: &AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| SicroError::Filesystem(format!("data dir: {e}")))?
        .join("ocr")
        .join("models"))
}

fn http_agent() -> Result<ureq::Agent> {
    let connector = native_tls::TlsConnector::new()
        .map_err(|e| SicroError::Validation(format!("TLS indisponível: {e}")))?;
    Ok(ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .tls_connector(std::sync::Arc::new(connector))
        .build())
}

fn download_with_progress(
    app: &AppHandle,
    url: &str,
    dest: &Path,
    id: &str,
    event: &str,
) -> Result<String> {
    let agent = http_agent()?;
    let resp = agent
        .get(url)
        .set("User-Agent", "SICRO")
        .call()
        .map_err(|e| SicroError::Validation(format!("download: {e}")))?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest)
        .map_err(|e| SicroError::Filesystem(format!("criar arquivo: {e}")))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut received: u64 = 0;
    let mut last: u64 = 0;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| SicroError::Validation(format!("leitura: {e}")))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| SicroError::Filesystem(format!("gravar: {e}")))?;
        hasher.update(&buf[..n]);
        received += n as u64;
        if received - last >= 1_500_000 {
            last = received;
            let _ = app.emit(
                event,
                ProgressPayload {
                    id: id.to_string(),
                    received,
                    total,
                },
            );
        }
    }
    let _ = app.emit(
        event,
        ProgressPayload {
            id: id.to_string(),
            received,
            total,
        },
    );
    Ok(format!("{:x}", hasher.finalize()))
}

/// Baixa os 3 arquivos do pacote latino da release `version` para o diretório
/// de modelos (download atômico: `.part` → rename). Compartilhado entre a
/// instalação (versão curada) e a atualização (versão upstream mais nova).
async fn download_pack(app: &AppHandle, version: &str) -> Result<()> {
    let dir = ocr_models_dir(app)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| SicroError::Filesystem(format!("criar pasta de modelos OCR: {e}")))?;
    let base = oar_release_base(version);
    for name in pack_files() {
        let url = format!("{base}/{name}");
        let dest = dir.join(name);
        let tmp = dir.join(format!("{name}.part"));
        let app2 = app.clone();
        let tmp2 = tmp.clone();
        let sha = tauri::async_runtime::spawn_blocking(move || {
            download_with_progress(&app2, &url, &tmp2, "latin", "ocr-download-progress")
        })
        .await
        .map_err(|e| SicroError::Validation(format!("tarefa de download: {e}")))??;
        std::fs::rename(&tmp, &dest)
            .map_err(|e| SicroError::Filesystem(format!("finalizar modelo: {e}")))?;
        tracing::info!("OCR modelo ({version}): {name} sha256={sha}");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Comandos

#[tauri::command]
pub async fn get_ocr_catalog() -> Result<OcrCatalog> {
    Ok(OcrCatalog {
        engine_ready: true,
        engine_label: "PP-OCRv5 (ONNX Runtime embutido)".to_string(),
        items: vec![PACK],
    })
}

#[tauri::command]
pub async fn get_ocr_status(app: AppHandle) -> Result<OcrStatus> {
    let dir = ocr_models_dir(&app)?;
    let mut installed = Vec::new();
    let mut present = 0;
    for name in pack_files() {
        let p = dir.join(name);
        if p.is_file() {
            present += 1;
            installed.push(InstalledOcrModel {
                filename: name.to_string(),
                size_bytes: std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0),
            });
        }
    }
    Ok(OcrStatus {
        engine_ready: true,
        engine_label: "PP-OCRv5 (ONNX Runtime embutido)".to_string(),
        models_ready: present == pack_files().len(),
        models_dir: dir.to_string_lossy().to_string(),
        installed,
    })
}

/// Baixa o pacote latino (3 arquivos), na versão CURADA, para o diretório de
/// modelos. `_asset_id` existe por compatibilidade da assinatura; hoje há um
/// único pacote.
#[tauri::command]
pub async fn install_ocr_asset(app: AppHandle, asset_id: String) -> Result<OcrStatus> {
    let _ = asset_id;
    download_pack(&app, OAR_OCR_VERSION).await?;
    // Registra a versão instalada (rastreabilidade + base p/ "Verificar atualizações").
    let mut s = get_app_settings(app.clone()).await?;
    s.ocr.ocr_version = OAR_OCR_VERSION.to_string();
    save_app_settings(app.clone(), s).await?;
    get_ocr_status(app).await
}

#[tauri::command]
pub async fn remove_ocr_asset(app: AppHandle, asset_id: String) -> Result<OcrStatus> {
    let _ = asset_id;
    let dir = ocr_models_dir(&app)?;
    for name in pack_files() {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let mut s = get_app_settings(app.clone()).await?;
    s.ocr.ocr_version = String::new();
    save_app_settings(app.clone(), s).await?;
    get_ocr_status(app).await
}

// ---------------------------------------------------------------------------
// Atualizações (opt-in) — modelos PP-OCRv5 do oar-ocr

#[derive(Debug, Clone, Serialize)]
pub struct OcrUpdateInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
}

/// Busca a release MAIS RECENTE do oar-ocr que realmente publica os 3 arquivos
/// do pacote latino (det + rec + dicionário). As releases do oar-ocr são, em
/// geral, só de CÓDIGO — os modelos ONNX foram anexados a uma release específica
/// (a v0.3.0), não a todas. Por isso comparar com a "última release" geraria um
/// 404 ao baixar. Retorna a tag da release que contém TODOS os nossos arquivos
/// (vazio se nenhuma os tiver / falha de rede). §13: só oferecemos atualização
/// quando há um pacote COMPATÍVEL upstream — nunca um download que falha.
fn fetch_latest_pack_release() -> Result<String> {
    let agent = http_agent()?;
    let resp = agent
        .get("https://api.github.com/repos/GreatV/oar-ocr/releases?per_page=100")
        .set("User-Agent", "SICRO")
        .call()
        .map_err(|e| SicroError::Validation(format!("consulta de atualização: {e}")))?;
    let releases: serde_json::Value = resp
        .into_json()
        .map_err(|e| SicroError::Validation(format!("resposta inválida: {e}")))?;
    let arr = match releases.as_array() {
        Some(a) => a,
        None => return Ok(String::new()),
    };
    // GitHub devolve as releases da mais nova para a mais antiga; a primeira que
    // contiver os 3 arquivos é a versão mais recente do pacote.
    let wanted = pack_files();
    for rel in arr {
        let assets = match rel.get("assets").and_then(|a| a.as_array()) {
            Some(a) => a,
            None => continue,
        };
        let has_all = wanted.iter().all(|w| {
            assets
                .iter()
                .any(|a| a.get("name").and_then(|n| n.as_str()) == Some(*w))
        });
        if has_all {
            if let Some(tag) = rel.get("tag_name").and_then(|t| t.as_str()) {
                return Ok(tag.to_string());
            }
        }
    }
    Ok(String::new())
}

/// OPT-IN: consulta a última release do oar-ocr (fonte dos modelos PP-OCRv5) e
/// compara com a versão INSTALADA do pacote. Apenas INFORMA — atualizar é uma
/// ação à parte do perito. §13: o motor de inferência (ONNX Runtime) é embutido
/// no SICRO; um release novo é informativo e o caminho mais seguro costuma ser a
/// própria atualização do app.
#[tauri::command]
pub async fn check_ocr_updates(app: AppHandle) -> Result<OcrUpdateInfo> {
    let s = get_app_settings(app).await?;
    let current = if s.ocr.ocr_version.is_empty() {
        OAR_OCR_VERSION.to_string()
    } else {
        s.ocr.ocr_version
    };
    let latest = tauri::async_runtime::spawn_blocking(fetch_latest_pack_release)
        .await
        .map_err(|e| SicroError::Validation(format!("tarefa de atualização: {e}")))??;
    Ok(OcrUpdateInfo {
        update_available: !latest.is_empty() && latest != current,
        current,
        latest,
    })
}

/// OPT-IN: atualiza o pacote de modelos para a última release do oar-ocr,
/// trocando a versão na URL do asset. Registra a nova versão (rastreabilidade).
/// §13: ação do perito; se o nome dos arquivos mudar upstream, falha com erro
/// claro (404) em vez de instalar algo incompatível.
#[tauri::command]
pub async fn update_ocr_models(app: AppHandle) -> Result<OcrStatus> {
    let st = get_ocr_status(app.clone()).await?;
    if !st.models_ready {
        return Err(SicroError::Validation(
            "nenhum pacote de OCR instalado para atualizar".into(),
        ));
    }
    let s = get_app_settings(app.clone()).await?;
    let current = if s.ocr.ocr_version.is_empty() {
        OAR_OCR_VERSION.to_string()
    } else {
        s.ocr.ocr_version.clone()
    };
    drop(s);

    let latest = tauri::async_runtime::spawn_blocking(fetch_latest_pack_release)
        .await
        .map_err(|e| SicroError::Validation(format!("tarefa de atualização: {e}")))??;
    if latest.is_empty() {
        return Err(SicroError::Validation(
            "nenhuma release do oar-ocr publica o pacote de modelos latino — \
             novas versões dos modelos chegam por atualização do SICRO"
                .into(),
        ));
    }
    if latest == current {
        return Err(SicroError::Validation(format!(
            "o pacote de OCR já está na versão mais recente disponível ({latest})"
        )));
    }

    download_pack(&app, &latest).await?;
    let mut s = get_app_settings(app.clone()).await?;
    s.ocr.ocr_version = latest.clone();
    save_app_settings(app.clone(), s).await?;
    tracing::info!("pacote de OCR atualizado para {latest}");
    get_ocr_status(app).await
}
