//! Abrir um arquivo com o app padrão do SO (Windows `cmd /C start`, macOS `open`,
//! Linux `xdg-open`) e revelar no gerenciador de arquivos.

use std::path::Path;

use crate::error::{Result, SicroError};

#[cfg(target_os = "windows")]
pub(crate) fn open_with_os(path: &Path) -> Result<()> {
    crate::tools::command("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao abrir: {e}")))?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn open_with_os(path: &Path) -> Result<()> {
    crate::tools::command("open")
        .arg(path)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao abrir: {e}")))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn open_with_os(path: &Path) -> Result<()> {
    crate::tools::command("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao abrir: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Revelar no gerenciador de arquivos

/// Abre o gerenciador de arquivos na pasta de `absolute_path`, selecionando-o onde dá.
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
    crate::tools::command("explorer")
        .args(["/select,", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    crate::tools::command("open")
        .args(["-R", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_with_os(path: &std::path::Path) -> Result<()> {
    // Respeita o gerenciador de pastas padrão do usuário (xdg-mime inode/directory).
    let default = linux::default_dir_handler();
    let entry = default.as_deref().and_then(linux::desktop_entry);
    let done = match (&default, &entry) {
        // De terminal (yazi, ranger…): recebe o arquivo e abre a pasta com ele marcado.
        (Some(id), Some(e)) if e.terminal => linux::launch_with(id, path),
        // Gráfico que também atende o FileManager1 (Nautilus, Dolphin…): seleciona o arquivo.
        (Some(_), Some(e)) if linux::file_manager1_program().as_deref() == Some(e.program.as_str()) => {
            linux::show_items(path)
        }
        (None, _) => linux::show_items(path),
        _ => false,
    };
    if done {
        return Ok(());
    }
    let dir = path.parent().unwrap_or(path);
    crate::tools::command("xdg-open")
        .arg(dir)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
mod linux {
    use std::path::{Path, PathBuf};

    pub struct DesktopEntry {
        pub program: String,
        pub terminal: bool,
    }

    pub fn default_dir_handler() -> Option<String> {
        let out = crate::tools::command("xdg-mime")
            .args(["query", "default", "inode/directory"])
            .output()
            .ok()?;
        let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!id.is_empty()).then_some(id)
    }

    fn data_dirs() -> Vec<PathBuf> {
        let mut v = Vec::new();
        match std::env::var_os("XDG_DATA_HOME").filter(|h| !h.is_empty()) {
            Some(h) => v.push(PathBuf::from(h)),
            None => {
                if let Some(home) = std::env::var_os("HOME") {
                    v.push(PathBuf::from(home).join(".local/share"));
                }
            }
        }
        let dirs = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        v.extend(dirs.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
        v
    }

    /// Programa do primeiro `Exec=` (só o nome) e `Terminal=` de um .desktop ou .service.
    fn read_entry(path: &Path) -> Option<DesktopEntry> {
        let text = std::fs::read_to_string(path).ok()?;
        let (mut program, mut terminal, mut main) = (None, false, false);
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                main = line == "[Desktop Entry]" || line == "[D-BUS Service]";
            } else if !main {
                continue;
            } else if let Some(v) = line.strip_prefix("Exec=") {
                if program.is_none() {
                    program = v
                        .split_whitespace()
                        .next()
                        .and_then(|p| Path::new(p.trim_matches('"')).file_name())
                        .map(|f| f.to_string_lossy().into_owned());
                }
            } else if let Some(v) = line.strip_prefix("Terminal=") {
                terminal = v == "true";
            }
        }
        Some(DesktopEntry { program: program?, terminal })
    }

    pub fn desktop_entry(id: &str) -> Option<DesktopEntry> {
        data_dirs().iter().find_map(|d| read_entry(&d.join("applications").join(id)))
    }

    pub fn file_manager1_program() -> Option<String> {
        data_dirs()
            .iter()
            .find_map(|d| read_entry(&d.join("dbus-1/services/org.freedesktop.FileManager1.service")))
            .map(|e| e.program)
    }

    /// Abre o .desktop com o arquivo; o GLib cuida do terminal (xdg-terminal-exec).
    pub fn launch_with(id: &str, path: &Path) -> bool {
        crate::tools::command("gtk-launch")
            .arg(id)
            .arg(path)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    pub fn show_items(path: &Path) -> bool {
        let uri = format!("file://{}", super::percent_encode_path(&path.to_string_lossy()));
        crate::tools::command("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.freedesktop.FileManager1",
                "--object-path",
                "/org/freedesktop/FileManager1",
                "--method",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("['{uri}']"),
                "",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn percent_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
