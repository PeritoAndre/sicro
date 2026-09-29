//! Miniatura de um vídeo para a lista do módulo (qualidade de vida, não prova).
//!
//! Um JPEG pequeno extraído com seek RÁPIDO (antes do `-i`): para reconhecer a
//! câmera de olho, snap no keyframe é irrelevante. Vai para o cache do app,
//! nomeado pelo SHA-256 do vídeo — nunca para dentro do `.sicro`.

use std::path::Path;
use std::process::Command;

use crate::error::{Result, SicroError};
use crate::video::frame_export::detect_ffmpeg;

/// Largura da miniatura em px (altura proporcional, par).
pub const THUMB_WIDTH: u32 = 320;

pub fn make_thumbnail(video: &Path, out_jpg: &Path, timestamp_s: f64) -> Result<()> {
    if let Some(parent) = out_jpg.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SicroError::Filesystem(format!("cannot create {}: {e}", parent.display()))
        })?;
    }
    let ffmpeg = detect_ffmpeg()?;
    // Grava num temporário e renomeia: uma miniatura pela metade nunca fica no cache.
    let tmp = out_jpg.with_extension("tmp.jpg");
    let run = |ts: f64| {
        Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-ss"])
            .arg(format!("{:.3}", ts.max(0.0)))
            .arg("-i")
            .arg(video)
            .args(["-frames:v", "1", "-vf"])
            .arg(format!("scale={THUMB_WIDTH}:-2"))
            .args(["-q:v", "5", "-y"])
            .arg(&tmp)
            .output()
    };
    let mut out = run(timestamp_s)
        .map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
    // Instante além do fim (duração errada no container) → tenta o início.
    if !out.status.success() || !tmp.is_file() {
        out = run(0.0).map_err(|e| SicroError::Workspace(format!("could not spawn ffmpeg: {e}")))?;
    }
    if !out.status.success() || !tmp.is_file() {
        let _ = std::fs::remove_file(&tmp);
        return Err(SicroError::Workspace(format!(
            "ffmpeg thumbnail failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    std::fs::rename(&tmp, out_jpg)
        .map_err(|e| SicroError::Filesystem(format!("cannot move thumbnail: {e}")))?;
    Ok(())
}
