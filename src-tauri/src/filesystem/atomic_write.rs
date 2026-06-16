//! Atomic file write — write to a sibling temp file, fsync, then rename.
//!
//! Why this matters: if the app dies mid-write, the original file stays
//! intact. `manifest.json` corruption would brick a workspace, so every
//! write goes through this helper.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::error::{Result, SicroError};

pub fn atomic_write_bytes(target: &Path, bytes: &[u8]) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| SicroError::Filesystem(format!("path has no parent: {}", target.display())))?;

    // Cada passo de I/O é embrulhado com CONTEXTO (qual operação + qual
    // caminho). Sem isso, uma falha vira um "i/o error: ... (os error 2)" seco,
    // impossível de diagnosticar — exatamente o que queremos evitar no único
    // caminho de escrita crítico do app.
    fs::create_dir_all(parent).map_err(|e| {
        SicroError::Filesystem(format!(
            "não consegui criar a pasta {}: {}",
            parent.display(),
            e
        ))
    })?;

    // Use a `.tmp` sibling so we stay on the same volume (rename is atomic on
    // the same filesystem on every platform we care about).
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

    // Rename atômico com resiliência a pastas SINCRONIZADAS (OneDrive/Dropbox):
    // o serviço de sync pode segurar ou mover o `.tmp` entre o sync e o rename,
    // fazendo o rename falhar (sharing violation; ou a origem some → "os error 2").
    // Estratégia: (1) no Windows o rename não sobrescreve, então removemos o
    // destino e tentamos de novo; (2) repetimos algumas vezes (o sync solta o
    // handle); (3) se ainda falhar, gravamos DIRETO no destino — perde a
    // atomicidade nesse caso raro, mas garante que o arquivo exista (melhor que
    // deixar um laudo "fantasma" sem arquivo, que aparece na lista e não abre).
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

    // Fallback: escrita direta no destino (último recurso).
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
