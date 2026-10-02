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
    let dir = path.parent().unwrap_or(path);
    crate::tools::command("xdg-open")
        .arg(dir)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao revelar: {e}")))?;
    Ok(())
}
