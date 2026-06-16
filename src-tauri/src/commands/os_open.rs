//! Pequeno utilitário compartilhado para abrir um arquivo com o aplicativo
//! padrão do sistema operacional.
//!
//! Centraliza a lógica multiplataforma (Windows `cmd /C start`, macOS `open`,
//! Linux `xdg-open`) que antes vivia privada em `registry_commands.rs`. Assim
//! tanto a Central de Evidências (`open_evidence_file`) quanto o módulo de
//! laudos (`open_laudo_external`) abrem `.docx`/`.pdf`/imagens no editor padrão
//! (Word / LibreOffice / visualizador) sem duplicar o comando por plataforma.

use std::path::Path;
use std::process::Command;

use crate::error::{Result, SicroError};

#[cfg(target_os = "windows")]
pub(crate) fn open_with_os(path: &Path) -> Result<()> {
    Command::new("cmd")
        .args(["/C", "start", "", &path.to_string_lossy()])
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao abrir: {e}")))?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn open_with_os(path: &Path) -> Result<()> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao abrir: {e}")))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn open_with_os(path: &Path) -> Result<()> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|e| SicroError::Filesystem(format!("falha ao abrir: {e}")))?;
    Ok(())
}
