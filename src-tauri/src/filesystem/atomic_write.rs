//! Escrita atômica: temp ao lado, fsync, rename. Se o app cair no meio, o
//! original fica intacto — um `manifest.json` corrompido inutilizaria o workspace.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::error::{Result, SicroError};

pub fn atomic_write_bytes(target: &Path, bytes: &[u8]) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| SicroError::Filesystem(format!("path has no parent: {}", target.display())))?;

    // Cada passo de I/O leva contexto (operação + caminho); sem isso o erro
    // vira um "os error 2" seco.
    fs::create_dir_all(parent).map_err(|e| {
        SicroError::Filesystem(format!(
            "não consegui criar a pasta {}: {}",
            parent.display(),
            e
        ))
    })?;

    // Temp na mesma pasta = mesmo volume; rename só é atômico no mesmo filesystem.
    let tmp_name = format!(
        "{}.tmp",
        target
            .file_name()
            .ok_or_else(|| SicroError::Filesystem(format!(
                "path has no filename: {}",
                target.display()
            )))?
            .to_string_lossy()
    );
    let tmp_path = parent.join(tmp_name);

    {
        let mut f = File::create(&tmp_path).map_err(|e| {
            SicroError::Filesystem(format!(
                "não consegui criar o arquivo temporário {}: {}",
                tmp_path.display(),
                e
            ))
        })?;
        f.write_all(bytes).map_err(|e| {
            SicroError::Filesystem(format!(
                "não consegui escrever em {}: {}",
                tmp_path.display(),
                e
            ))
        })?;
        f.sync_all().map_err(|e| {
            SicroError::Filesystem(format!(
                "não consegui sincronizar {}: {}",
                tmp_path.display(),
                e
            ))
        })?;
    }

    // Pastas sincronizadas (OneDrive/Dropbox) podem segurar o `.tmp` e fazer o
    // rename falhar. No Windows rename não sobrescreve: remove o destino e tenta
    // de novo (4x); se ainda falhar, grava direto — perde atomicidade, mas o arquivo existe.
    let mut last_err: Option<std::io::Error> = None;
    for attempt in 0..4u32 {
        match fs::rename(&tmp_path, target) {
            Ok(()) => return Ok(()),
            Err(e) => {
                if target.exists() {
                    let _ = fs::remove_file(target);
                }
                last_err = Some(e);
                if attempt < 3 {
                    std::thread::sleep(std::time::Duration::from_millis(60));
                }
            }
        }
    }

    fs::write(target, bytes).map_err(|e| {
        SicroError::Filesystem(format!(
            "não consegui gravar {} (rename falhou: {}): {}",
            target.display(),
            last_err
                .as_ref()
                .map(|le| le.to_string())
                .unwrap_or_else(|| "?".to_string()),
            e
        ))
    })?;
    let _ = fs::remove_file(&tmp_path);
    Ok(())
}
