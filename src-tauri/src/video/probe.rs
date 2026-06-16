//! `ffprobe` wrapper — Spike F.
//!
//! We shell out to the system `ffprobe` (no Rust bindings) and parse its
//! JSON output. This mirrors the Python lab decision to treat ffprobe as
//! the technical source of truth, distinct from the visual player.
//!
//! Detection: the binary is looked up in PATH. If not found, callers get
//! a clear `SicroError::Validation`. The Spike F report documents the
//! limitation; bundling ffprobe is a future step.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::error::{Result, SicroError};

/// Parsed subset of ffprobe's JSON that the rest of the backend cares
/// about. The original payload is preserved in `raw_json` so the UI can
/// surface anything we don't model yet.
#[derive(Debug, Clone)]
pub struct ParsedProbe {
    pub raw_json: String,
    pub duration_s: Option<f64>,
    pub bitrate: Option<i64>,
    pub video_codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub pixel_format: Option<String>,
    pub fps_declared: Option<f64>,
    pub avg_frame_rate: Option<String>,
    pub r_frame_rate: Option<String>,
    pub time_base: Option<String>,
    pub frame_count: Option<i64>,
    /// Soft warnings (VFR likely, frame_count missing, etc.). NOT errors.
    pub warnings: Vec<String>,
}

/// Look up an `ffprobe` binary that the orchestrator can call. Tries the
/// user's PATH first; in the future this could fall back to a bundled
/// build. Returns the resolved path.
pub fn detect_ffprobe() -> Result<PathBuf> {
    which("ffprobe")
}

/// Run `ffprobe -show_format -show_streams -of json` against `path` and
/// return the parsed subset. The video file MUST already exist locally
/// (we don't reach the network).
pub fn probe_media(path: &Path) -> Result<ParsedProbe> {
    if !path.is_file() {
        return Err(SicroError::Filesystem(format!(
            "video not found: {}",
            path.display()
        )));
    }
    let bin = detect_ffprobe()?;

    let output = Command::new(&bin)
        .args([
            "-v",
            "error",
            "-show_format",
            "-show_streams",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|e| {
            SicroError::Workspace(format!(
                "could not spawn ffprobe at {}: {}",
                bin.display(),
                e
            ))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SicroError::Workspace(format!(
            "ffprobe failed (exit {:?}): {}",
            output.status.code(),
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    parse_probe_output(&stdout)
}

fn parse_probe_output(raw: &str) -> Result<ParsedProbe> {
    let value: Value = serde_json::from_str(raw).map_err(|e| {
        SicroError::Validation(format!("ffprobe output is not valid JSON: {e}"))
    })?;

    let format = value.get("format").cloned().unwrap_or(Value::Null);
    let streams = value
        .get("streams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // Pick the first video stream (most files have exactly one).
    let video_stream = streams
        .iter()
        .find(|s| s.get("codec_type").and_then(Value::as_str) == Some("video"))
        .cloned();

    let duration_s = first_float(&[
        format.get("duration"),
        video_stream.as_ref().and_then(|s| s.get("duration")),
    ]);
    let bitrate = first_int(&[
        format.get("bit_rate"),
        video_stream.as_ref().and_then(|s| s.get("bit_rate")),
    ]);

    let video_codec = video_stream
        .as_ref()
        .and_then(|s| s.get("codec_name"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let width = video_stream
        .as_ref()
        .and_then(|s| s.get("width"))
        .and_then(Value::as_i64)
        .map(|n| n as u32);
    let height = video_stream
        .as_ref()
        .and_then(|s| s.get("height"))
        .and_then(Value::as_i64)
        .map(|n| n as u32);
    let pixel_format = video_stream
        .as_ref()
        .and_then(|s| s.get("pix_fmt"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let avg_frame_rate = video_stream
        .as_ref()
        .and_then(|s| s.get("avg_frame_rate"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let r_frame_rate = video_stream
        .as_ref()
        .and_then(|s| s.get("r_frame_rate"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let time_base = video_stream
        .as_ref()
        .and_then(|s| s.get("time_base"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let fps_declared = avg_frame_rate
        .as_deref()
        .and_then(parse_fraction)
        .or_else(|| r_frame_rate.as_deref().and_then(parse_fraction));

    let frame_count = video_stream
        .as_ref()
        .and_then(|s| s.get("nb_frames"))
        .and_then(|v| {
            // ffprobe sometimes emits nb_frames as a numeric string.
            v.as_i64().or_else(|| {
                v.as_str().and_then(|s| s.parse::<i64>().ok())
            })
        });

    let mut warnings = Vec::new();
    if video_stream.is_none() {
        warnings.push("no video stream found".to_string());
    }
    if frame_count.is_none() {
        warnings.push(
            "frame_count ausente — índice de frame derivado será sempre estimado".to_string(),
        );
    }
    if let (Some(avg), Some(r)) = (avg_frame_rate.as_deref(), r_frame_rate.as_deref()) {
        // Mobile-shot footage commonly reports a VFR-friendly avg ≠ r.
        if avg != r {
            warnings.push(format!(
                "FPS possivelmente variável (avg_frame_rate={avg} ≠ r_frame_rate={r}) — VFR provável; trate índice de frame como estimativa"
            ));
        }
    }
    if fps_declared.is_none() {
        warnings.push("FPS não pôde ser determinado pelo ffprobe".to_string());
    }
    if duration_s.is_none() {
        warnings.push("duração não pôde ser determinada pelo ffprobe".to_string());
    }

    Ok(ParsedProbe {
        raw_json: raw.to_string(),
        duration_s,
        bitrate,
        video_codec,
        width,
        height,
        pixel_format,
        fps_declared,
        avg_frame_rate,
        r_frame_rate,
        time_base,
        frame_count,
        warnings,
    })
}

fn first_float(candidates: &[Option<&Value>]) -> Option<f64> {
    for c in candidates {
        if let Some(v) = c {
            if let Some(n) = v.as_f64() {
                return Some(n);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.parse::<f64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

fn first_int(candidates: &[Option<&Value>]) -> Option<i64> {
    for c in candidates {
        if let Some(v) = c {
            if let Some(n) = v.as_i64() {
                return Some(n);
            }
            if let Some(s) = v.as_str() {
                if let Ok(n) = s.parse::<i64>() {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Parse "30000/1001" or "30" into a float; returns None for "0/0" and
/// other degenerate inputs.
pub fn parse_fraction(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some((num, den)) = s.split_once('/') {
        let n: f64 = num.trim().parse().ok()?;
        let d: f64 = den.trim().parse().ok()?;
        if d == 0.0 {
            return None;
        }
        let v = n / d;
        return if v.is_finite() && v > 0.0 { Some(v) } else { None };
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0)
}

/// Cross-platform `which` — tries the literal name and then with platform
/// extensions. Returns `Ok(path)` if any candidate is executable.
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
        "binary '{name}' not found in PATH. Install FFmpeg (with ffprobe) and ensure both ffprobe and ffmpeg are on the PATH."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fraction_handles_typical_inputs() {
        assert!((parse_fraction("30000/1001").unwrap() - 29.97).abs() < 0.01);
        assert_eq!(parse_fraction("30").unwrap(), 30.0);
        assert_eq!(parse_fraction("0/0"), None);
        assert_eq!(parse_fraction("garbage"), None);
        assert_eq!(parse_fraction(""), None);
    }

    #[test]
    fn parse_probe_warns_when_avg_differs_from_r() {
        // Minimal probe blob — VFR-looking footage.
        let raw = r#"{
          "format": { "duration": "12.345", "bit_rate": "1000000" },
          "streams": [{
            "codec_type": "video",
            "codec_name": "h264",
            "width": 1920,
            "height": 1080,
            "pix_fmt": "yuv420p",
            "avg_frame_rate": "29970000/1001000",
            "r_frame_rate": "30/1",
            "time_base": "1/15360",
            "nb_frames": "370"
          }]
        }"#;
        let parsed = parse_probe_output(raw).unwrap();
        assert_eq!(parsed.width, Some(1920));
        assert_eq!(parsed.height, Some(1080));
        assert_eq!(parsed.frame_count, Some(370));
        assert!(parsed
            .warnings
            .iter()
            .any(|w| w.contains("VFR provável")));
    }

    #[test]
    fn parse_probe_warns_when_no_video_stream() {
        let raw = r#"{ "format": {}, "streams": [] }"#;
        let parsed = parse_probe_output(raw).unwrap();
        assert!(parsed
            .warnings
            .iter()
            .any(|w| w == "no video stream found"));
    }
}
