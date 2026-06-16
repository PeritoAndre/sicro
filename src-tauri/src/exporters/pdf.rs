//! PDF export via Microsoft Edge headless.
//!
//! Strategy: write the HTML to a temp file inside the workspace's `cache/`
//! folder, invoke `msedge.exe --headless=new --print-to-pdf=<out> file://<in>`,
//! wait for the subprocess to finish, and surface the resulting PDF.
//!
//! Why subprocess and not a Rust HTML→PDF crate?
//!   - Crates like `wkhtmltopdf`, `weasyprint` and `headless_chrome` all add
//!     either a heavy native dependency or wrap a Chromium handshake we'd
//!     have to maintain. Spawning Edge keeps the binary footprint of the
//!     SICRO app unchanged and uses the Chromium that already ships with
//!     Windows 11.
//!   - The `--print-to-pdf` flag has been part of Chromium since 2018 and is
//!     stable across Edge versions.
//!
//! Failure modes the caller must surface to the UI:
//!   - Edge executable not found  → "instale o Microsoft Edge" message.
//!   - Subprocess exit code != 0  → "Edge falhou ao gerar o PDF (exit X)".
//!   - Output file missing after Edge exited                       (idem).
//!   - Subprocess timeout                                           (idem).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use base64::Engine;

use crate::error::{Result, SicroError};

const EDGE_KNOWN_PATHS: &[&str] = &[
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
];

const CHROME_KNOWN_PATHS: &[&str] = &[
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
];

/// Maximum wall-time we let Edge run before considering it stuck.
const PRINT_TIMEOUT: Duration = Duration::from_secs(45);

/// Render the given HTML string to a PDF file at `output_path`.
///
/// `cache_dir` is used to stash the temporary input HTML and is expected to
/// exist (workspace creation guarantees `<workspace>/cache/`).
///
/// `footer_template`: quando `Some`, gera o PDF via DevTools Protocol
/// (`Page.printToPDF` com `footerTemplate`) para imprimir "Folha X de Y" na
/// margem inferior de TODA página — único jeito de numeração no PDF (o CLI
/// `--print-to-pdf` ignora margens @page do CSS). Se o caminho CDP falhar por
/// QUALQUER motivo (Edge antigo, porta, timeout, PDF vazio), cai
/// automaticamente no CLI robusto — o PDF nunca deixa de ser gerado (só fica
/// sem o rodapé de página).
pub fn render_html_to_pdf(
    html: &str,
    cache_dir: &Path,
    output_path: &Path,
    footer_template: Option<&str>,
) -> Result<()> {
    if let Some(footer) = footer_template {
        match render_via_cdp(html, cache_dir, output_path, footer) {
            Ok(()) if output_path.is_file() => {
                if std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0) > 0 {
                    return Ok(());
                }
            }
            Ok(()) => {}
            Err(e) => {
                eprintln!("[pdf] rodapé via CDP falhou ({e}); usando CLI sem rodapé.");
            }
        }
    }
    render_via_cli(html, cache_dir, output_path)
}

fn render_via_cli(html: &str, cache_dir: &Path, output_path: &Path) -> Result<()> {
    let browser = locate_browser()?;

    // 1. Write the HTML to a temporary file. We *don't* try to share the same
    //    name across invocations because parallel exports in the future will
    //    collide. Filename uses nanos for a quick unique suffix.
    let now_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let input_path = cache_dir.join(format!("export_{now_nanos}.html"));
    std::fs::write(&input_path, html).map_err(|e| {
        SicroError::Filesystem(format!(
            "could not write temp HTML at {}: {}",
            input_path.display(),
            e
        ))
    })?;

    // 2. Build the input file URL. Edge requires file:/// on Windows, with
    //    backslashes converted to forward slashes.
    let input_url = path_to_file_url(&input_path)?;

    // 3. Spawn Edge.
    let started_at = Instant::now();
    let mut child = Command::new(&browser)
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-pdf-header-footer")
        .arg("--virtual-time-budget=5000")
        .arg(format!("--print-to-pdf={}", output_path.display()))
        .arg(&input_url)
        .spawn()
        .map_err(|e| {
            SicroError::Filesystem(format!(
                "could not start Edge at {}: {}",
                browser.display(),
                e
            ))
        })?;

    // 4. Wait with a manual timeout — `std::process::Child` doesn't expose
    //    `wait_timeout` without an extra crate, so we poll.
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = std::fs::remove_file(&input_path);
                if !status.success() {
                    return Err(SicroError::Workspace(format!(
                        "Edge exited with status {}",
                        status.code().unwrap_or(-1)
                    )));
                }
                break;
            }
            Ok(None) => {
                if started_at.elapsed() > PRINT_TIMEOUT {
                    let _ = child.kill();
                    let _ = std::fs::remove_file(&input_path);
                    return Err(SicroError::Workspace(
                        "Edge headless timed out while generating the PDF".to_string(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&input_path);
                return Err(SicroError::Workspace(format!(
                    "failed to wait for Edge subprocess: {e}"
                )));
            }
        }
    }

    // 5. Sanity-check: the PDF should exist and not be empty.
    if !output_path.is_file() {
        return Err(SicroError::Workspace(format!(
            "Edge finished but no PDF was created at {}",
            output_path.display()
        )));
    }
    let metadata = std::fs::metadata(output_path)?;
    if metadata.len() == 0 {
        return Err(SicroError::Workspace(
            "Edge produced an empty PDF file".to_string(),
        ));
    }

    Ok(())
}

// ===========================================================================
// Caminho CDP (DevTools Protocol) — para imprimir "Folha X de Y" no rodapé.
//
// O CLI `--print-to-pdf` ignora margens @page do CSS e não aceita rodapé.
// A ÚNICA forma de numeração de página no PDF é `Page.printToPDF` com
// `footerTemplate` (placeholders `pageNumber`/`totalPages` que o Chromium
// substitui). Dirigimos o Edge headless via WebSocket local.

type CdpSocket =
    tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>;

fn render_via_cdp(
    html: &str,
    cache_dir: &Path,
    output_path: &Path,
    footer_template: &str,
) -> Result<()> {
    let browser = locate_browser()?;
    let now_nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let input_path = cache_dir.join(format!("export_cdp_{now_nanos}.html"));
    std::fs::write(&input_path, html)
        .map_err(|e| SicroError::Filesystem(format!("could not write temp HTML: {e}")))?;
    let input_url = path_to_file_url(&input_path)?;
    let user_data_dir = cache_dir.join(format!("cdp_profile_{now_nanos}"));
    let _ = std::fs::create_dir_all(&user_data_dir);

    let port = pick_free_port()?;
    let mut child = Command::new(&browser)
        .arg("--headless=new")
        .arg("--disable-gpu")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg(format!("--remote-debugging-port={port}"))
        .arg("--remote-allow-origins=*")
        .arg(format!("--user-data-dir={}", user_data_dir.display()))
        .arg("about:blank")
        .spawn()
        .map_err(|e| SicroError::Workspace(format!("could not start Edge (CDP): {e}")))?;

    let outcome = cdp_print(port, &input_url, footer_template, output_path);

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&input_path);
    let _ = std::fs::remove_dir_all(&user_data_dir);
    outcome
}

fn cdp_print(
    port: u16,
    input_url: &str,
    footer_template: &str,
    output_path: &Path,
) -> Result<()> {
    use serde_json::json;
    use tungstenite::stream::MaybeTlsStream;

    let ws_url = wait_for_page_ws(port)?;
    let (mut socket, _resp) = tungstenite::connect(ws_url.as_str())
        .map_err(|e| SicroError::Workspace(format!("CDP websocket connect failed: {e}")))?;
    if let MaybeTlsStream::Plain(s) = socket.get_mut() {
        let _ = s.set_read_timeout(Some(Duration::from_secs(20)));
    }

    cdp_send(&mut socket, 1, "Page.enable", json!({}))?;
    cdp_await_response(&mut socket, 1, Instant::now() + Duration::from_secs(10))?;

    cdp_send(&mut socket, 2, "Page.navigate", json!({ "url": input_url }))?;
    cdp_await_response(&mut socket, 2, Instant::now() + Duration::from_secs(15))?;

    // Assets são inlinados (data URIs) e o arquivo é local — sem rede. Um
    // intervalo curto cobre layout/fontes sem a complexidade de assinar
    // `Page.loadEventFired` (que poderia ser consumido fora de ordem).
    std::thread::sleep(Duration::from_millis(1200));

    // Numeração padrão do laudo fica no CABEÇALHO (topo da margem), não no
    // rodapé. `footer_template` carrega o "Folha X de Y" — vai no headerTemplate.
    let params = json!({
        "displayHeaderFooter": true,
        "headerTemplate": footer_template,
        "footerTemplate": "<span></span>",
        "printBackground": true,
        "preferCSSPageSize": true,
    });
    cdp_send(&mut socket, 3, "Page.printToPDF", params)?;
    let resp = cdp_await_response(&mut socket, 3, Instant::now() + Duration::from_secs(40))?;
    let data_b64 = resp
        .get("result")
        .and_then(|r| r.get("data"))
        .and_then(|d| d.as_str())
        .ok_or_else(|| SicroError::Workspace("CDP printToPDF: sem data".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .map_err(|e| SicroError::Workspace(format!("CDP base64 decode: {e}")))?;
    std::fs::write(output_path, bytes)
        .map_err(|e| SicroError::Filesystem(format!("could not write PDF: {e}")))?;
    let _ = socket.close(None);
    Ok(())
}

fn cdp_send(
    sock: &mut CdpSocket,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> Result<()> {
    let msg = serde_json::json!({ "id": id, "method": method, "params": params }).to_string();
    sock.send(tungstenite::Message::Text(msg.into()))
        .map_err(|e| SicroError::Workspace(format!("CDP send {method}: {e}")))
}

/// Lê mensagens até achar a RESPOSTA com `id` (eventos são ignorados).
fn cdp_await_response(
    sock: &mut CdpSocket,
    id: u64,
    deadline: Instant,
) -> Result<serde_json::Value> {
    loop {
        if Instant::now() > deadline {
            return Err(SicroError::Workspace("CDP: timeout aguardando resposta".into()));
        }
        let msg = match sock.read() {
            Ok(m) => m,
            Err(tungstenite::Error::Io(ref e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => return Err(SicroError::Workspace(format!("CDP read: {e}"))),
        };
        if let tungstenite::Message::Text(t) = msg {
            let v: serde_json::Value =
                serde_json::from_str(t.as_str()).unwrap_or(serde_json::Value::Null);
            if v.get("id").and_then(|x| x.as_u64()) == Some(id) {
                if let Some(err) = v.get("error") {
                    return Err(SicroError::Workspace(format!("CDP error: {err}")));
                }
                return Ok(v);
            }
        }
    }
}

fn pick_free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| SicroError::Workspace(format!("CDP: sem porta livre: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| SicroError::Workspace(format!("CDP: {e}")))?
        .port();
    drop(listener);
    Ok(port)
}

/// Pergunta ao Edge (HTTP `/json/list`) o `webSocketDebuggerUrl` de um alvo
/// "page", aguardando o navegador subir.
fn wait_for_page_ws(port: u16) -> Result<String> {
    let url = format!("http://127.0.0.1:{port}/json/list");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if Instant::now() > deadline {
            return Err(SicroError::Workspace("CDP: alvo não ficou pronto".into()));
        }
        if let Ok(resp) = ureq::get(&url).call() {
            if let Ok(text) = resp.into_string() {
                if let Ok(serde_json::Value::Array(list)) =
                    serde_json::from_str::<serde_json::Value>(&text)
                {
                    for t in &list {
                        if t.get("type").and_then(|x| x.as_str()) == Some("page") {
                            if let Some(ws) =
                                t.get("webSocketDebuggerUrl").and_then(|x| x.as_str())
                            {
                                return Ok(ws.to_string());
                            }
                        }
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn locate_browser() -> Result<PathBuf> {
    for path in EDGE_KNOWN_PATHS.iter().chain(CHROME_KNOWN_PATHS.iter()) {
        let p = PathBuf::from(path);
        if p.is_file() {
            return Ok(p);
        }
    }
    // Last resort: try to find chrome.exe in PATH (works for portable Chrome installs).
    if let Ok(output) = Command::new("where").arg("chrome.exe").output() {
        if output.status.success() {
            if let Ok(first_line) = String::from_utf8(output.stdout) {
                if let Some(first) = first_line.lines().next() {
                    let p = PathBuf::from(first.trim());
                    if p.is_file() {
                        return Ok(p);
                    }
                }
            }
        }
    }

    Err(SicroError::Workspace(
        "Microsoft Edge não encontrado. Instale o Edge ou configure manualmente o caminho de um navegador baseado em Chromium.".to_string(),
    ))
}

fn path_to_file_url(path: &Path) -> Result<String> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let s = canonical
        .to_str()
        .ok_or_else(|| SicroError::Filesystem(format!("non-UTF8 path: {}", path.display())))?;
    // Strip Windows long-path prefix `\\?\` that `canonicalize` adds.
    let stripped = s.strip_prefix(r"\\?\").unwrap_or(s);
    Ok(format!("file:///{}", stripped.replace('\\', "/")))
}
