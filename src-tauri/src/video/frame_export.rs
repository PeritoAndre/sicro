//! Coleta de quadro: extraído do arquivo pelo ffmpeg (não é screenshot do
//! player), em PNG + sidecar JSON. Busca rápida + exata:
//!   `-ss <alvo − 2 s> -copyts -i <vídeo> -ss <alvo> -frames:v 1`
//! Só o `-ss` de entrada cai no quadro-chave: instantes distintos do mesmo GOP
//! viravam um só quadro (Δt = 0 na regressão). O `-ss` de entrada conta do
//! `start_time` do arquivo e o alvo é absoluto — a busca rápida desconta isso.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::error::{Result, SicroError};

use super::probe::detect_ffprobe;

/// Quanto antes do alvo a busca rápida mira. Só desempenho (menos decodificação);
/// a exatidão vem do `-copyts` + `-ss` de saída.
const SEEK_REWIND_S: f64 = 2.0;

pub struct ExtractFrameOptions<'a> {
    pub video_path: &'a Path,
    pub timestamp_s: f64,
    pub out_png: &'a Path,
    /// Sidecar JSON ao lado do PNG; `None` não grava.
    pub sidecar_json: Option<&'a Path>,
    /// Copiado como está para o sidecar (media_hash, event_id…).
    pub sidecar_extra: serde_json::Value,
    /// `format.start_time` do arquivo: a busca rápida de entrada desconta isto.
    pub start_time_s: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractedFrame {
    pub out_png: PathBuf,
    pub sidecar_json_path: Option<PathBuf>,
    pub requested_timestamp_s: f64,
    pub actual_timestamp_s: Option<f64>,
    pub delta_s: Option<f64>,
    pub size_bytes: u64,
    /// 1ª linha de `ffmpeg -version`.
    pub ffmpeg_version: Option<String>,
    pub extracted_at: DateTime<Utc>,
}

/// ffmpeg empacotado com o SICRO ou o do PATH.
pub fn detect_ffmpeg() -> Result<PathBuf> {
    if let Some(p) = crate::tools::bundled_ffmpeg_tool("ffmpeg") {
        return Ok(p);
    }
    which("ffmpeg")
}

/// Coleta um quadro no instante pedido (PNG + sidecar opcional).
pub fn extract_frame(opts: ExtractFrameOptions<'_>) -> Result<ExtractedFrame> {
    if !opts.video_path.is_file() {
        return Err(SicroError::Filesystem(format!(
            "video not found: {}",
            opts.video_path.display()
        )));
    }
    if let Some(parent) = opts.out_png.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            SicroError::Filesystem(format!(
                "cannot create frame export dir {}: {}",
                parent.display(),
                e
            ))
        })?;
    }
    let ffmpeg = detect_ffmpeg()?;

    // O quadro é o que o player mostra (último com pts <= t). Mira 1/4 de quadro
    // antes dele: a busca exata de saída fica com o 1º quadro >= alvo — ele, não
    // o anterior nem o seguinte. Sem tempos de pacote, usa o pedido cru.
    let (target_abs, shown_pts) = match crate::video::clip::plan_clip(
        opts.video_path,
        opts.timestamp_s,
        opts.timestamp_s,
    ) {
        Ok(p) => ((p.first_frame - p.frame_dur * 0.25).max(0.0), Some(p.first_frame)),
        Err(_) => (opts.timestamp_s, None),
    };

    // Busca rápida de entrada (descontando start_time) + exata de saída (absoluta).
    let coarse = format_seconds(
        (target_abs - SEEK_REWIND_S - opts.start_time_s.max(0.0)).max(0.0),
    );
    let target = format_seconds(target_abs);
    let status = crate::tools::command(&ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-ss",
            &coarse,
            "-copyts",
            "-i",
        ])
        .arg(opts.video_path)
        .args(["-ss", &target, "-frames:v", "1", "-update", "1", "-y"])
        .arg(opts.out_png)
        .output()
        .map_err(|e| {
            SicroError::Workspace(format!(
                "could not spawn ffmpeg at {}: {}",
                ffmpeg.display(),
                e
            ))
        })?;
    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(SicroError::Workspace(format!(
            "ffmpeg frame extraction failed (exit {:?}): {}",
            status.status.code(),
            stderr.trim()
        )));
    }
    if !opts.out_png.is_file() {
        return Err(SicroError::Workspace(
            "ffmpeg returned success but did not write the PNG".to_string(),
        ));
    }

    let size_bytes = std::fs::metadata(opts.out_png)
        .map(|m| m.len())
        .unwrap_or(0);

    // Sem plano por pacotes, lê o tempo real do quadro decodificado.
    let actual = shown_pts.or_else(|| probe_actual_timestamp(opts.video_path, opts.timestamp_s).ok());
    let delta = actual.map(|a| a - opts.timestamp_s);

    let ffmpeg_version = detect_ffmpeg_version(&ffmpeg);
    let extracted = ExtractedFrame {
        out_png: opts.out_png.to_path_buf(),
        sidecar_json_path: opts.sidecar_json.map(Path::to_path_buf),
        requested_timestamp_s: opts.timestamp_s,
        actual_timestamp_s: actual,
        delta_s: delta,
        size_bytes,
        ffmpeg_version,
        extracted_at: Utc::now(),
    };

    if let Some(sidecar) = opts.sidecar_json {
        write_sidecar(sidecar, &extracted, &opts.sidecar_extra)?;
    }

    Ok(extracted)
}

/// Tempo de apresentação real do quadro em que a busca exata cai: lê QUADROS
/// decodificados (pacotes devolveriam o quadro-chave do GOP para todo instante
/// nele) e pega o 1º com tempo >= pedido.
fn probe_actual_timestamp(video: &Path, ts_s: f64) -> Result<f64> {
    let ffprobe = detect_ffprobe()?;
    let ts = ts_s.max(0.0);
    let start = (ts - SEEK_REWIND_S).max(0.0);
    // Fim absoluto: `%+n` contaria de onde a busca caiu (o quadro-chave) e a
    // janela podia acabar antes do alvo.
    let end = ts + 4.0;
    let output = crate::tools::command(&ffprobe)
        .args([
            "-v",
            "error",
            "-read_intervals",
            &format!("{start}%{end}"),
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=best_effort_timestamp_time",
            "-of",
            "default=nokey=1:noprint_wrappers=1",
        ])
        .arg(video)
        .output()
        .map_err(|e| {
            SicroError::Workspace(format!(
                "could not spawn ffprobe at {}: {}",
                ffprobe.display(),
                e
            ))
        })?;
    if !output.status.success() {
        return Err(SicroError::Workspace(
            "ffprobe failed when reading actual timestamp".to_string(),
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut times: Vec<f64> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != "N/A")
        .filter_map(|l| l.parse::<f64>().ok())
        .filter(|t| t.is_finite())
        .collect();
    if times.is_empty() {
        return Err(SicroError::Workspace(
            "ffprobe returned no frames".to_string(),
        ));
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    const EPS: f64 = 1e-6;
    let chosen = times
        .iter()
        .copied()
        .find(|&t| t >= ts - EPS)
        .or_else(|| times.last().copied())
        .unwrap_or(ts);
    Ok(chosen)
}

fn write_sidecar(
    path: &Path,
    frame: &ExtractedFrame,
    extra: &serde_json::Value,
) -> Result<()> {
    let mut blob = serde_json::json!({
        "schema_version": "0.1",
        "kind": "video_frame_export",
        "requested_timestamp_s": frame.requested_timestamp_s,
        "actual_timestamp_s": frame.actual_timestamp_s,
        "delta_s": frame.delta_s,
        "output_path": frame.out_png.to_string_lossy(),
        "size_bytes": frame.size_bytes,
        "ffmpeg_version": frame.ffmpeg_version,
        "extracted_at": frame.extracted_at.to_rfc3339(),
        "software": "SICRO Desktop 2.0 / Spike F"
    });
    if let Value::Object(map) = extra {
        if let Some(obj) = blob.as_object_mut() {
            for (k, v) in map.iter() {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    let bytes = serde_json::to_vec_pretty(&blob)?;
    crate::filesystem::atomic_write_bytes(path, &bytes)?;
    Ok(())
}

fn detect_ffmpeg_version(ffmpeg: &Path) -> Option<String> {
    let output = crate::tools::command(ffmpeg).arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::to_string)
}

/// `HH:MM:SS.mmm` (o ffmpeg aceita; mais legível na lista de processos).
fn format_seconds(s: f64) -> String {
    let s = s.max(0.0);
    let total_ms = (s * 1000.0).round() as i64;
    let ms = total_ms % 1000;
    let total_s = total_ms / 1000;
    let sec = total_s % 60;
    let min = (total_s / 60) % 60;
    let hr = total_s / 3600;
    format!("{hr:02}:{min:02}:{sec:02}.{ms:03}")
}

/// Quadro estimado = round(t × fps); `None` sem fps válido. É sempre estimativa.
pub fn estimate_frame_index(timestamp_s: f64, fps_declared: Option<f64>) -> Option<i64> {
    let fps = fps_declared?;
    if fps <= 0.0 || !fps.is_finite() {
        return None;
    }
    Some((timestamp_s * fps).round() as i64)
}

fn which(name: &str) -> Result<PathBuf> {
    let path_env = std::env::var_os("PATH").ok_or_else(|| {
        SicroError::Filesystem("PATH environment variable is empty".to_string())
    })?;
    let mut candidates: Vec<String> = vec![name.to_string()];
    if cfg!(windows) {
        candidates.push(format!("{name}.exe"));
        candidates.push(format!("{name}.cmd"));
    }
    for dir in std::env::split_paths(&path_env) {
        for cand in &candidates {
            let full = dir.join(cand);
            if full.is_file() {
                return Ok(full);
            }
        }
    }
    Err(SicroError::Validation(format!(
        "binary '{name}' not found in PATH. Install FFmpeg and ensure ffmpeg + ffprobe are on the PATH."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_seconds_pads_correctly() {
        assert_eq!(format_seconds(0.0), "00:00:00.000");
        assert_eq!(format_seconds(0.5), "00:00:00.500");
        assert_eq!(format_seconds(65.25), "00:01:05.250");
        assert_eq!(format_seconds(3661.001), "01:01:01.001");
    }

    #[test]
    fn estimate_frame_index_handles_missing_fps() {
        assert_eq!(estimate_frame_index(1.0, None), None);
        assert_eq!(estimate_frame_index(1.0, Some(0.0)), None);
        assert_eq!(estimate_frame_index(1.0, Some(f64::NAN)), None);
        assert_eq!(estimate_frame_index(1.0, Some(30.0)), Some(30));
        assert_eq!(estimate_frame_index(2.5, Some(30.0)), Some(75));
    }
}
