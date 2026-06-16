//! Gerenciador de dependência LibreOffice (Configurações › Dependências).
//!
//! O LibreOffice é OPCIONAL: serve para gerar um PDF com diagramação "estilo
//! Word" (numeração de página no lugar, dentro de tabela, cabeçalho que repete)
//! a partir do `.docx` que o SICRO já produz. É um programa do sistema (~360MB),
//! não um arquivo de modelo — então, no mesmo molde de IA/OCR, este módulo:
//!   - detecta se o `soffice` está instalado (caminhos conhecidos + PATH);
//!   - baixa o instalador OFICIAL (.msi) COM progresso para um cache TEMPORÁRIO
//!     (`app_cache_dir`, FORA do workspace → não entra no backup) e o ABRE;
//!   - o perito conclui a instalação e clica "Verificar" para detectar.
//!
//! §13: offline-after-download, opt-in, nada instalado silenciosamente.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{Result, SicroError};

/// Versão oficial baixada por "Baixar/Atualizar". Atualizável aqui quando sair
/// uma nova stable (a URL segue o padrão do TDF).
const LIBREOFFICE_VERSION: &str = "25.8.7";
const LIBREOFFICE_MSI_URL: &str = "https://download.documentfoundation.org/libreoffice/stable/25.8.7/win/x86_64/LibreOffice_25.8.7_Win_x86-64.msi";
const LIBREOFFICE_APPROX_MB: u32 = 349;
/// Página oficial (fallback manual se o download direto falhar).
const LIBREOFFICE_SITE: &str = "https://pt-br.libreoffice.org/baixe-ja/libreoffice-novo/";

const KNOWN_SOFFICE_PATHS: &[&str] = &[
    r"C:\Program Files\LibreOffice\program\soffice.exe",
    r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
];

#[derive(Debug, Clone, Serialize)]
pub struct LibreOfficeStatus {
    pub installed: bool,
    pub soffice_path: String,
    /// Linha de versão reportada pelo soffice (pode ficar vazia em algumas
    /// instalações Windows; nesse caso a UI mostra "versão desconhecida").
    pub version: String,
    /// Versão que o botão "Baixar/Atualizar" instalaria.
    pub download_version: String,
    pub approx_mb: u32,
    /// URL direta do instalador (.msi).
    pub download_url: String,
    /// Página oficial (fallback manual).
    pub site_url: String,
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    id: String,
    received: u64,
    total: u64,
}

/// Localiza o `soffice` (caminhos conhecidos primeiro, depois PATH).
pub fn find_soffice() -> Option<PathBuf> {
    for p in KNOWN_SOFFICE_PATHS {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    which::which("soffice")
        .ok()
        .or_else(|| which::which("soffice.exe").ok())
}

/// Tenta obter a versão. No Windows o `soffice.com` (wrapper de console, ao lado
/// do .exe) imprime em stdout; preferimos ele para `--version`.
fn query_version(soffice: &Path) -> String {
    let console = soffice.with_file_name("soffice.com");
    let bin = if console.is_file() { console } else { soffice.to_path_buf() };
    if let Ok(o) = Command::new(&bin).arg("--version").output() {
        let s = String::from_utf8_lossy(&o.stdout);
        let line = s.lines().next().unwrap_or("").trim();
        if !line.is_empty() {
            return line.to_string();
        }
    }
    String::new()
}

/// Agente HTTP com TLS nativo (mesmo padrão do ai_commands — sem ring/OpenSSL).
fn http_agent() -> Result<ureq::Agent> {
    let connector = native_tls::TlsConnector::new()
        .map_err(|e| SicroError::Validation(format!("TLS indisponível: {e}")))?;
    Ok(ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(30))
        .tls_connector(std::sync::Arc::new(connector))
        .build())
}

fn download_with_progress(app: &AppHandle, url: &str, dest: &Path, id: &str) -> Result<()> {
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
        received += n as u64;
        if received - last >= 2_000_000 {
            let _ = app.emit(
                "libreoffice-download-progress",
                ProgressPayload { id: id.to_string(), received, total },
            );
            last = received;
        }
    }
    file.flush().ok();
    let _ = app.emit(
        "libreoffice-download-progress",
        ProgressPayload { id: id.to_string(), received, total },
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Comandos

/// Status do LibreOffice (instalado? versão? + metadados de download).
#[tauri::command]
pub async fn get_libreoffice_status() -> Result<LibreOfficeStatus> {
    let found = find_soffice();
    let (installed, soffice_path, version) = match found {
        Some(p) => {
            let v = query_version(&p);
            (true, p.to_string_lossy().to_string(), v)
        }
        None => (false, String::new(), String::new()),
    };
    Ok(LibreOfficeStatus {
        installed,
        soffice_path,
        version,
        download_version: LIBREOFFICE_VERSION.to_string(),
        approx_mb: LIBREOFFICE_APPROX_MB,
        download_url: LIBREOFFICE_MSI_URL.to_string(),
        site_url: LIBREOFFICE_SITE.to_string(),
    })
}

/// Baixa o instalador oficial (.msi) com progresso para um cache TEMPORÁRIO
/// (fora do workspace, logo NÃO entra no backup) e abre-o. O perito conclui a
/// instalação manualmente; depois usa "Verificar" para detectar.
#[tauri::command]
pub async fn download_libreoffice_installer(app: AppHandle) -> Result<()> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| SicroError::Filesystem(format!("cache dir: {e}")))?
        .join("libreoffice");
    std::fs::create_dir_all(&cache)
        .map_err(|e| SicroError::Filesystem(format!("criar cache: {e}")))?;
    let dest = cache.join(format!("LibreOffice_{LIBREOFFICE_VERSION}_Win_x86-64.msi"));

    let app2 = app.clone();
    let dest2 = dest.clone();
    tauri::async_runtime::spawn_blocking(move || {
        download_with_progress(&app2, LIBREOFFICE_MSI_URL, &dest2, "libreoffice")
    })
    .await
    .map_err(|e| SicroError::Validation(format!("tarefa de download: {e}")))??;

    // Abre o instalador (NÃO silencioso): o perito clica pra concluir.
    Command::new("msiexec")
        .arg("/i")
        .arg(&dest)
        .spawn()
        .map_err(|e| {
            SicroError::Workspace(format!(
                "instalador baixado em {}, mas não foi possível abri-lo: {e}",
                dest.display()
            ))
        })?;
    Ok(())
}

/// Converte um `.docx` em PDF via LibreOffice headless (diagramação "estilo
/// Word": numeração no lugar, em tabela, cabeçalho que repete). O PDF herda a
/// fidelidade do `.docx`. Bloqueante — use em `spawn_blocking`/comando async.
///
/// Usa `soffice.com` (versão de console, que BLOQUEIA até terminar no Windows)
/// quando existe, e um perfil de usuário isolado em `temp` para não conflitar
/// com um LibreOffice que o perito já tenha aberto.
pub fn convert_docx_to_pdf(docx: &Path, output_pdf: &Path, pdf_a: bool) -> Result<()> {
    let soffice = find_soffice().ok_or_else(|| {
        SicroError::Workspace(
            "LibreOffice não encontrado. Instale-o em Configurações › Dependências."
                .to_string(),
        )
    })?;
    // soffice.com bloqueia até concluir; soffice.exe pode retornar antes.
    let console = soffice.with_file_name("soffice.com");
    let bin = if console.is_file() { console } else { soffice };

    let outdir = output_pdf
        .parent()
        .ok_or_else(|| SicroError::Filesystem("PDF de saída sem diretório".to_string()))?;
    std::fs::create_dir_all(outdir).ok();

    // Perfil isolado → evita conflito com instância já aberta do LibreOffice.
    let profile = std::env::temp_dir().join("sicro_lo_profile");
    let profile_url = format!("file:///{}", profile.to_string_lossy().replace('\\', "/"));

    // PDF/A-2b (ISO 19005-2) para arquivamento: SelectPdfVersion=2. As opções
    // de filtro vão em JSON após o segundo `:` (suportado no LO 7.x+). Como
    // usamos Command::arg (sem shell), as aspas são literais — sem escaping.
    let convert_to = if pdf_a {
        r#"pdf:writer_pdf_Export:{"SelectPdfVersion":{"type":"long","value":"2"}}"#
    } else {
        "pdf:writer_pdf_Export"
    };

    let status = Command::new(&bin)
        .arg("--headless")
        .arg("--norestore")
        .arg("--invisible")
        .arg("--nolockcheck")
        .arg(format!("-env:UserInstallation={profile_url}"))
        .arg("--convert-to")
        .arg(convert_to)
        .arg("--outdir")
        .arg(outdir)
        .arg(docx)
        .status()
        .map_err(|e| SicroError::Workspace(format!("falha ao executar o LibreOffice: {e}")))?;
    if !status.success() {
        return Err(SicroError::Workspace(format!(
            "LibreOffice falhou na conversão (código {})",
            status.code().unwrap_or(-1)
        )));
    }

    // O soffice nomeia a saída como `<nome-do-docx>.pdf` no outdir; renomeia
    // para o caminho final esperado pelo export.
    let stem = docx
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("laudo");
    let produced = outdir.join(format!("{stem}.pdf"));
    if produced != *output_pdf {
        if output_pdf.exists() {
            let _ = std::fs::remove_file(output_pdf);
        }
        if std::fs::rename(&produced, output_pdf).is_err() {
            std::fs::copy(&produced, output_pdf)
                .map_err(|e| SicroError::Filesystem(format!("mover PDF: {e}")))?;
            let _ = std::fs::remove_file(&produced);
        }
    }
    if !output_pdf.is_file() || std::fs::metadata(output_pdf).map(|m| m.len()).unwrap_or(0) == 0 {
        return Err(SicroError::Workspace(
            "LibreOffice terminou, mas o PDF não foi gerado.".to_string(),
        ));
    }
    Ok(())
}
