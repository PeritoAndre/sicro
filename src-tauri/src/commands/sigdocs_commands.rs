//! Tauri commands para a integração SIGDOC (sistema de fluxo de
//! documentos do Estado do Amapá).
//!
//! Três modalidades:
//!
//! **Onda 1 — Janela secundária** (`open_sigdocs_window`):
//!   `WebviewWindow` separada do SO ao lado da principal. OS controla
//!   layout; user move/redimensiona à vontade.
//!
//! **Onda 3 — Cover mode** (`open_sigdocs_cover` + `update_…_bounds`):
//!   Webview borderless posicionado EXATAMENTE sobre a área de
//!   conteúdo da window principal — dá a impressão de que o portal
//!   abriu "no lugar" do laudo. O frontend mede a área (entre topbar
//!   e statusbar, à direita da rail) e envia bounds em CSS px
//!   relativos à window principal. Quando a window é redimensionada,
//!   o frontend re-envia bounds (ResizeObserver no host).
//!
//! **Onda 2 — Split sincronizado** (DEPRECATED em 2026-05-27 quando o
//! usuário preferiu o cover mode pra demonstração à direção).
//!
//! Também expõe `reveal_path_in_explorer` — abre o Windows Explorer
//! na pasta de um arquivo, selecionando-o. Útil porque SIGDOC bloqueia
//! Ctrl+V no upload — o perito arrasta direto do Explorer pro portal.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow,
    WindowEvent,
};

use crate::error::{Result, SicroError};
use crate::workspace::manifest::{DEFAULT_SIGDOCS_URL, Manifest};

const SIGDOCS_SECONDARY_LABEL: &str = "sigdocs-secondary";
const SIGDOCS_COVER_LABEL: &str = "sigdocs-cover";
const MAIN_WINDOW_LABEL: &str = "main";

/// Bounds da área coberta pelo SIGDOC, em CSS px relativos ao
/// webview principal. (0, 0) = top-left do conteúdo do webview.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CoverBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Estado em memória do cover — guarda os últimos bounds para que o
/// listener de Resize/Moved possa reposicionar a window secundária.
#[derive(Debug, Default)]
pub struct SigdocsCoverState {
    pub bounds: Mutex<Option<CoverBounds>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SigdocsConfig {
    pub url: String,
    pub source: String, // "manifest" | "default"
}

/// Resolve a URL do SIGDOC a partir do manifest do workspace.
#[tauri::command]
pub async fn get_sigdocs_url(workspace_path: String) -> Result<SigdocsConfig> {
    let ws = std::path::PathBuf::from(&workspace_path);
    match Manifest::read(&ws) {
        Ok(manifest) => {
            let url = manifest.effective_sigdocs_url();
            let source = if manifest.sigdocs_url.is_some() {
                "manifest"
            } else {
                "default"
            };
            Ok(SigdocsConfig {
                url,
                source: source.to_string(),
            })
        }
        Err(_) => Ok(SigdocsConfig {
            url: DEFAULT_SIGDOCS_URL.to_string(),
            source: "default".to_string(),
        }),
    }
}

// ---------------------------------------------------------------------------
// reveal_path_in_explorer — abre Explorer na pasta de um arquivo
// ---------------------------------------------------------------------------

/// Abre o gerenciador de arquivos do SO na pasta contendo `absolute_path`,
/// idealmente selecionando o arquivo. Usado quando o perito vai arrastar
/// o PDF exportado pra dentro do SIGDOC (que bloqueia Ctrl+V).
#[tauri::command]
pub async fn reveal_path_in_explorer(absolute_path: String) -> Result<()> {
    let p = std::path::PathBuf::from(&absolute_path);
    if !p.exists() {
        return Err(SicroError::Filesystem(format!(
            "arquivo não encontrado em {}",
            p.display()
        )));
    }
    reveal_with_os(&p)
}

#[cfg(target_os = "windows")]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    std::process::Command::new("explorer")
        .args(["/select,", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    std::process::Command::new("open")
        .args(["-R", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    let dir = path.parent().unwrap_or(path);
    std::process::Command::new("xdg-open")
        .arg(dir)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Onda 1 — Janela secundária independente
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn open_sigdocs_window(app: AppHandle, url: Option<String>) -> Result<()> {
    let target = url.unwrap_or_else(|| DEFAULT_SIGDOCS_URL.to_string());
    validate_url(&target)?;
    let parsed = target
        .parse::<tauri::Url>()
        .map_err(|e| SicroError::Validation(format!("URL SIGDOC inválida: {e}")))?;

    if let Some(existing) = app.get_webview_window(SIGDOCS_SECONDARY_LABEL) {
        let _ = existing.set_focus();
        return Ok(());
    }

    let (pos_x, pos_y, w, h) = compute_secondary_position(&app);

    tauri::WebviewWindowBuilder::new(
        &app,
        SIGDOCS_SECONDARY_LABEL,
        WebviewUrl::External(parsed),
    )
    .title("SIGDOC — Estado do Amapá")
    .inner_size(w, h)
    .position(pos_x, pos_y)
    .resizable(true)
    .decorations(true)
    .build()
    .map_err(|e| SicroError::Workspace(format!("falha ao abrir SIGDOC: {e}")))?;

    Ok(())
}

#[tauri::command]
pub async fn close_sigdocs_window(app: AppHandle) -> Result<()> {
    if let Some(win) = app.get_webview_window(SIGDOCS_SECONDARY_LABEL) {
        let _ = win.close();
    }
    Ok(())
}

fn compute_secondary_position(app: &AppHandle) -> (f64, f64, f64, f64) {
    let default = (1200.0, 100.0, 1200.0, 800.0);
    let Some(main) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return default;
    };
    let Ok(outer_pos) = main.outer_position() else {
        return default;
    };
    let Ok(outer_size) = main.outer_size() else {
        return default;
    };
    let scale = main.scale_factor().unwrap_or(1.0);
    let x = (outer_pos.x as f64 + outer_size.width as f64) / scale + 8.0;
    let y = outer_pos.y as f64 / scale;
    (x, y, 1200.0, 800.0)
}

// ---------------------------------------------------------------------------
// Onda 3 — Cover mode (webview borderless cobrindo a área do editor)
// ---------------------------------------------------------------------------

/// Abre o SIGDOC num webview borderless posicionado SOBRE a área de
/// conteúdo da window principal. O frontend é responsável por medir
/// a área disponível (entre topbar e statusbar, à direita da rail)
/// e passar `bounds` em CSS px relativos ao webview principal.
///
/// A janela secundária NÃO tem chrome (decorations: false), não pode
/// ser movida pelo user, e fecha junto com a principal.
#[tauri::command]
pub async fn open_sigdocs_cover(
    app: AppHandle,
    state: tauri::State<'_, SigdocsCoverState>,
    url: Option<String>,
    bounds: CoverBounds,
) -> Result<()> {
    let target = url.unwrap_or_else(|| DEFAULT_SIGDOCS_URL.to_string());
    validate_url(&target)?;
    let parsed = target
        .parse::<tauri::Url>()
        .map_err(|e| SicroError::Validation(format!("URL SIGDOC inválida: {e}")))?;

    let main_window = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| SicroError::Workspace("main window not found".to_string()))?;

    if let Some(existing) = app.get_webview_window(SIGDOCS_COVER_LABEL) {
        // Já tá aberto — só reposiciona.
        *state.bounds.lock().unwrap() = Some(bounds);
        position_cover(&main_window, &existing, bounds)?;
        let _ = existing.set_focus();
        return Ok(());
    }

    let (abs_x, abs_y, w, h) = absolute_bounds(&main_window, bounds)?;

    // K — Se temos credenciais salvas, prepara o script de autofill.
    // O script é injetado em CADA page-load (não só no primeiro) pra
    // funcionar mesmo se o SIGDOC redirecionar pós-login para outra
    // tela com form (sessão expirada, etc).
    let credentials = load_credentials_internal(&app);
    let autofill_script = credentials.as_ref().map(|(email, password)| {
        build_autofill_script(email, password)
    });

    // CONTENÇÃO — `parent(main)` torna o cover uma OWNED WINDOW da
    // principal (no Windows: relação de "owner"). Consequências, todas
    // desejadas:
    //   • o cover fica SEMPRE imediatamente acima da main no z-order,
    //     reforçado pelo SO — cobre a área do editor como se fosse
    //     parte dela (não tem como "afundar" atrás da main → some o
    //     retângulo preto que aparecia quando ele perdia o z-order);
    //   • mas NÃO é `always_on_top`: ao focar outro programa (Explorer,
    //     navegador), a dupla main+cover afunda JUNTA — o cover deixa de
    //     flutuar por cima dos outros apps. Era exatamente o bug do
    //     "popup" relatado;
    //   • minimiza / restaura / fecha junto com a main.
    // Substitui o antigo `always_on_top(true)` + a gambiarra de
    // esconder/mostrar por foco (que deixava a tela preta e quebrava o
    // arrastar do Explorer).
    //
    // `disable_drag_drop_handler()` desliga o handler de drag-drop do
    // Tauri NESTE webview. Sem isso o Tauri intercepta o arquivo solto e
    // a página do SIGDOC nunca o recebe; com isso o WebView2 entrega o
    // PDF nativo (arrastado do Explorer) direto pra área de upload do
    // portal — é o que faz o "arraste o PDF pra cá" funcionar de verdade.
    //
    // `focused(false)` previne que ele roube foco no abrir (o user
    // continua mexendo na main); o autofill do JS injetado funciona
    // mesmo sem foco.
    let mut builder = tauri::WebviewWindowBuilder::new(
        &app,
        SIGDOCS_COVER_LABEL,
        WebviewUrl::External(parsed),
    )
    .title("SIGDOC")
    .inner_size(w, h)
    .position(abs_x, abs_y)
    .resizable(false)
    .decorations(false)
    .parent(&main_window)
    .map_err(|e| SicroError::Workspace(format!("parent cover: {e}")))?
    .disable_drag_drop_handler()
    .skip_taskbar(true)
    .focused(false);

    if let Some(script) = autofill_script.clone() {
        // `initialization_script` roda ANTES de qualquer JS da página,
        // mas o seletor pode falhar (DOM ainda não montado). Por isso
        // o script abaixo agenda um setTimeout/DOMContentLoaded.
        builder = builder.initialization_script(&script);
    }

    let cover = builder
        .build()
        .map_err(|e| {
            SicroError::Workspace(format!("falha ao criar cover SIGDOC: {e}"))
        })?;

    // Fecha o cover quando a window principal fecha.
    let app_for_close = app.clone();
    main_window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { .. } = event {
            if let Some(c) = app_for_close.get_webview_window(SIGDOCS_COVER_LABEL) {
                let _ = c.close();
            }
        }
    });

    *state.bounds.lock().unwrap() = Some(bounds);
    let _ = cover.show();
    Ok(())
}

#[tauri::command]
pub async fn update_sigdocs_cover_bounds(
    app: AppHandle,
    state: tauri::State<'_, SigdocsCoverState>,
    bounds: CoverBounds,
) -> Result<()> {
    *state.bounds.lock().unwrap() = Some(bounds);
    let Some(main_window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return Ok(());
    };
    let Some(cover) = app.get_webview_window(SIGDOCS_COVER_LABEL) else {
        return Ok(());
    };
    position_cover(&main_window, &cover, bounds)?;
    Ok(())
}

#[tauri::command]
pub async fn close_sigdocs_cover(
    app: AppHandle,
    state: tauri::State<'_, SigdocsCoverState>,
) -> Result<()> {
    if let Some(win) = app.get_webview_window(SIGDOCS_COVER_LABEL) {
        let _ = win.close();
    }
    *state.bounds.lock().unwrap() = None;
    Ok(())
}

/// Converte bounds (em CSS px relativos ao webview principal) para
/// posição/tamanho absolutos do SO (também em CSS px — Tauri faz
/// scale internamente).
///
/// Soma:
///   - outer_position da main (top-left da window no monitor).
///   - chrome offset (diferença entre outer_size e inner_size).
/// Retorna (x, y, width, height) em coords lógicas.
fn absolute_bounds(
    main: &WebviewWindow,
    bounds: CoverBounds,
) -> Result<(f64, f64, f64, f64)> {
    let outer_pos = main
        .outer_position()
        .map_err(|e| SicroError::Workspace(format!("outer_position: {e}")))?;
    let outer_size = main
        .outer_size()
        .map_err(|e| SicroError::Workspace(format!("outer_size: {e}")))?;
    let inner_size = main
        .inner_size()
        .map_err(|e| SicroError::Workspace(format!("inner_size: {e}")))?;
    let scale = main.scale_factor().unwrap_or(1.0);

    // Chrome offsets — diferença entre outer e inner, dividida por 2
    // pra horizontal (bordas L+R) e o restante pra vertical (title bar
    // + borda inferior). Em Windows 11 com borda invisível isso geralmente
    // é 0/0, mas no Windows 10 tem ~8px de borda.
    let chrome_h = (outer_size.width as f64 - inner_size.width as f64) / scale;
    let chrome_v = (outer_size.height as f64 - inner_size.height as f64) / scale;
    let border_left = chrome_h / 2.0;
    let title_bar = chrome_v - border_left; // assume border inferior == border lateral

    let abs_x = (outer_pos.x as f64) / scale + border_left + bounds.x;
    let abs_y = (outer_pos.y as f64) / scale + title_bar + bounds.y;
    Ok((abs_x, abs_y, bounds.width, bounds.height))
}

fn position_cover(
    main: &WebviewWindow,
    cover: &WebviewWindow,
    bounds: CoverBounds,
) -> Result<()> {
    let (x, y, w, h) = absolute_bounds(main, bounds)?;
    cover
        .set_position(LogicalPosition::new(x, y))
        .map_err(|e| SicroError::Workspace(format!("set_position cover: {e}")))?;
    cover
        .set_size(LogicalSize::new(w.max(50.0), h.max(50.0)))
        .map_err(|e| SicroError::Workspace(format!("set_size cover: {e}")))?;
    Ok(())
}

/// Hook chamado pelo `lib.rs` no `setup()` da app. Registra listener
/// de Resize/Moved na main window pra reposicionar o cover (owned
/// window não acompanha sozinha o mover/redimensionar do dono) e um
/// listener de minimize/restore pra escondê-lo/mostrá-lo junto.
pub fn install_cover_resize_listener(app: &AppHandle) {
    let Some(main_window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let handle = app.clone();
    main_window.on_window_event(move |event| {
        match event {
            // Reposiciona quando a main muda de tamanho ou posição.
            WindowEvent::Resized(_) | WindowEvent::Moved(_) => {
                let cover_state = match handle.try_state::<SigdocsCoverState>() {
                    Some(s) => s,
                    None => return,
                };
                let bounds = cover_state.bounds.lock().ok().and_then(|g| *g);
                let Some(b) = bounds else { return };
                let Some(main) = handle.get_webview_window(MAIN_WINDOW_LABEL) else {
                    return;
                };
                let Some(cover) = handle.get_webview_window(SIGDOCS_COVER_LABEL)
                else {
                    return;
                };
                let _ = position_cover(&main, &cover, b);
            }
            _ => {}
        }
    });

    // L — Listener separado pra minimize/restore da main. Usa o
    // listener da WebviewWindow porque `Resized` não cobre minimize
    // de forma confiável no Windows.
    let handle2 = app.clone();
    let main_window2 = main_window.clone();
    main_window.on_window_event(move |event| {
        if let WindowEvent::Resized(size) = event {
            // size = (0, 0) costuma indicar minimização no Windows.
            let cover = match handle2.get_webview_window(SIGDOCS_COVER_LABEL) {
                Some(c) => c,
                None => return,
            };
            let is_minimized = main_window2.is_minimized().unwrap_or(false);
            if is_minimized || (size.width == 0 && size.height == 0) {
                let _ = cover.hide();
            } else {
                let _ = cover.show();
            }
        }
    });
}

fn validate_url(target: &str) -> Result<()> {
    let lower = target.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err(SicroError::Validation(format!(
            "URL SIGDOC deve começar com http:// ou https://: {target}"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// K — Credenciais do SIGDOC (Windows Credential Manager via crate keyring)
// ---------------------------------------------------------------------------

/// Service identifier no Windows Credential Manager / macOS Keychain.
/// O `email` é usado como `username` na entrada.
const SIGDOC_KEYRING_SERVICE: &str = "sicro-desktop-sigdoc";

/// Arquivo JSON que guarda o EMAIL atualmente cadastrado (a senha vai
/// pro keyring). Fica em `app_config_dir/sigdoc-email.txt`.
fn email_storage_path(app: &AppHandle) -> Result<std::path::PathBuf> {
    let base = app
        .path()
        .app_config_dir()
        .map_err(|e| SicroError::Filesystem(format!("config dir: {e}")))?;
    std::fs::create_dir_all(&base).map_err(|e| {
        SicroError::Filesystem(format!("cannot create config dir: {e}"))
    })?;
    Ok(base.join("sigdoc-email.txt"))
}

fn read_stored_email(app: &AppHandle) -> Option<String> {
    let path = email_storage_path(app).ok()?;
    std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
}

fn write_stored_email(app: &AppHandle, email: &str) -> Result<()> {
    let path = email_storage_path(app)?;
    std::fs::write(&path, email.trim().as_bytes()).map_err(|e| {
        SicroError::Filesystem(format!("write email: {e}"))
    })?;
    Ok(())
}

fn delete_stored_email(app: &AppHandle) -> Result<()> {
    let path = email_storage_path(app)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| {
            SicroError::Filesystem(format!("remove email file: {e}"))
        })?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct SigdocCredentialsStatus {
    pub email: Option<String>,
    pub has_password: bool,
}

/// Salva email + senha do SIGDOC.
///
/// Email vai pro arquivo `sigdoc-email.txt` (legível). Senha vai pro
/// **Windows Credential Manager** (Win32 API) — armazenamento
/// criptografado per-user, mesmo nível de proteção do Edge/Chrome.
#[tauri::command]
pub async fn save_sigdoc_credentials(
    app: AppHandle,
    email: String,
    password: String,
) -> Result<()> {
    let trimmed_email = email.trim();
    if trimmed_email.is_empty() {
        return Err(SicroError::Validation("email vazio".to_string()));
    }
    if password.is_empty() {
        return Err(SicroError::Validation("senha vazia".to_string()));
    }
    // Se o email mudou em relação ao anterior, remove a entrada antiga
    // do keyring pra não deixar credencial órfã.
    if let Some(old_email) = read_stored_email(&app) {
        if !old_email.is_empty() && old_email != trimmed_email {
            if let Ok(entry) =
                keyring::Entry::new(SIGDOC_KEYRING_SERVICE, &old_email)
            {
                let _ = entry.delete_credential();
            }
        }
    }
    let entry = keyring::Entry::new(SIGDOC_KEYRING_SERVICE, trimmed_email)
        .map_err(|e| {
            SicroError::Workspace(format!("keyring entry create: {e}"))
        })?;
    entry
        .set_password(&password)
        .map_err(|e| SicroError::Workspace(format!("keyring set: {e}")))?;
    write_stored_email(&app, trimmed_email)?;
    Ok(())
}

/// Status das credenciais — retorna o email atual e SE há senha
/// salva no keyring. NUNCA retorna a senha em si (consulta interna
/// é feita via `load_credentials_internal` no fluxo do cover).
#[tauri::command]
pub async fn get_sigdoc_credentials_status(
    app: AppHandle,
) -> Result<SigdocCredentialsStatus> {
    let email = read_stored_email(&app);
    let has_password = match &email {
        Some(e) if !e.is_empty() => {
            match keyring::Entry::new(SIGDOC_KEYRING_SERVICE, e) {
                Ok(entry) => entry.get_password().is_ok(),
                Err(_) => false,
            }
        }
        _ => false,
    };
    Ok(SigdocCredentialsStatus {
        email,
        has_password,
    })
}

#[tauri::command]
pub async fn delete_sigdoc_credentials(app: AppHandle) -> Result<()> {
    if let Some(email) = read_stored_email(&app) {
        if !email.is_empty() {
            if let Ok(entry) =
                keyring::Entry::new(SIGDOC_KEYRING_SERVICE, &email)
            {
                let _ = entry.delete_credential();
            }
        }
    }
    delete_stored_email(&app)?;
    Ok(())
}

/// Lê email + senha do storage. Usado internamente pelo open_sigdocs_cover
/// pra autopreencher. Retorna None se faltar algum.
fn load_credentials_internal(app: &AppHandle) -> Option<(String, String)> {
    let email = read_stored_email(app)?;
    if email.is_empty() {
        return None;
    }
    let entry = keyring::Entry::new(SIGDOC_KEYRING_SERVICE, &email).ok()?;
    let password = entry.get_password().ok()?;
    Some((email, password))
}

/// Escapa string pra ser segura dentro de JS literal entre aspas duplas.
/// Necessário porque email/senha podem ter caracteres como `"`, `\`, etc.
fn js_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
        .replace('\t', "\\t")
}

/// Gera JS que tenta preencher email + senha em qualquer form que
/// pareça login (heurística baseada em seletores comuns).
///
/// IMPORTANTE: NÃO submete o form — só preenche. O perito sempre
/// revisa antes de clicar "Entrar". Tradeoff entre conveniência e
/// segurança institucional.
fn build_autofill_script(email: &str, password: &str) -> String {
    let email_js = js_escape(email);
    let password_js = js_escape(password);
    format!(
        r#"
(function() {{
  const EMAIL = "{email}";
  const PASSWORD = "{password}";

  // Seletores comuns em portais governamentais brasileiros (JSF,
  // Spring MVC, etc): name="email"/"usuario"/"login"/"cpf",
  // type="email" ou "text" com placeholder; senha sempre type="password".
  const EMAIL_SELECTORS = [
    'input[type="email"]',
    'input[name="email" i]',
    'input[name="usuario" i]',
    'input[name="login" i]',
    'input[name="cpf" i]',
    'input[id*="email" i]',
    'input[id*="usuario" i]',
    'input[id*="login" i]',
    'input[id*="cpf" i]',
  ];
  const PWD_SELECTORS = [
    'input[type="password"]',
  ];

  function fillField(selectors, value) {{
    for (const sel of selectors) {{
      const el = document.querySelector(sel);
      if (el && !el.disabled && !el.readOnly) {{
        el.focus();
        el.value = value;
        // Dispara eventos pra frameworks JSF/Angular/React/Vue detectarem.
        el.dispatchEvent(new Event('input', {{bubbles: true}}));
        el.dispatchEvent(new Event('change', {{bubbles: true}}));
        el.blur();
        return true;
      }}
    }}
    return false;
  }}

  function tryFill() {{
    const filledEmail = fillField(EMAIL_SELECTORS, EMAIL);
    const filledPwd = fillField(PWD_SELECTORS, PASSWORD);
    return filledEmail || filledPwd;
  }}

  // Tenta na carga do DOM, no readyState complete, e algumas vezes
  // depois (SPAs JSF carregam o form via JS post-load).
  function scheduleFill() {{
    if (tryFill()) return;
    let attempts = 0;
    const interval = setInterval(() => {{
      attempts++;
      if (tryFill() || attempts >= 10) {{
        clearInterval(interval);
      }}
    }}, 400);
  }}

  if (document.readyState === 'loading') {{
    document.addEventListener('DOMContentLoaded', scheduleFill);
  }} else {{
    scheduleFill();
  }}
}})();
"#,
        email = email_js,
        password = password_js,
    )
}
