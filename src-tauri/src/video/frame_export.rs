//! `ffmpeg` frame export — Spike F.
//!
//! Implements the lab's decision (`SICRO_VIDEO_LAB_RELATORIO.md` §6):
//! a frame is NOT a screenshot of the player. It's extracted from the
//! source file by ffmpeg with the request timestamp, written as PNG,
//! and accompanied by a sidecar JSON describing the technical context.
//!
//! Command used (fast + ACCURATE seek):
//!     ffmpeg -hide_banner -nostdin
//!            -ss <ts - REWIND>   ← coarse fast seek (keyframe snap, cheap)
//!            -copyts             ← keep original timestamps
//!            -i <video>
//!            -ss <ts>            ← accurate seek to the EXACT instant
//!            -frames:v 1
//!            -update 1
//!            -y <out.png>
//!
//! Why not a plain `-ss` BEFORE `-i` (fast seek alone)? It snaps to the
//! nearest preceding keyframe, so several DISTINCT requested instants that
//! fall inside the same GOP collapse onto ONE decoded frame. In a forensic
//! workflow that is corrosive: the perito marks points on what are actually
//! identical images, and — because every collapsed frame then reports the
//! keyframe's timestamp — the speed regression sees Δt = 0 and aborts with
//! "amplitude temporal zero". We therefore use the canonical fast+accurate
//! recipe: a coarse input seek lands on the keyframe just before the target
//! (so ffmpeg only decodes a short span, even deep into a long video), and
//! `-copyts` + an ABSOLUTE output `-ss` make ffmpeg decode-and-discard until
//! it reaches the exact frame. Distinct requests ⇒ distinct frames.

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::error::{Result, SicroError};

use super::probe::detect_ffprobe;

/// How far BEFORE the target instant the coarse fast-seek lands. It only has
/// to be enough that the keyframe ffmpeg snaps to sits before the target, so
/// the accurate output seek has frames to decode-and-discard up to it. This
/// is purely a performance knob (smaller ⇒ less decoding); correctness comes
/// from `-copyts` + the absolute output `-ss`, not from this margin.
const SEEK_REWIND_S: f64 = 2.0;

pub struct ExtractFrameOptions<'a> {
    pub video_path: &'a Path,
    pub timestamp_s: f64,
    pub out_png: &'a Path,
    /// Optional sidecar JSON path (next to the PNG). If None, sidecar is
    /// not written. The orchestrator decides — we just do disk + ffmpeg.
    pub sidecar_json: Option<&'a Path>,
    /// Echoed verbatim into the sidecar (media_hash, event_id, etc.) so
    /// the caller can attach domain context without us having to model it
    /// at this layer.
    pub sidecar_extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractedFrame {
    pub out_png: PathBuf,
    pub sidecar_json_path: Option<PathBuf>,
    pub requested_timestamp_s: f64,
    pub actual_timestamp_s: Option<f64>,
    pub delta_s: Option<f64>,
    pub size_bytes: u64,
    /// Captures the ffmpeg version line (first line of `ffmpeg -version`).
    pub ffmpeg_version: Option<String>,
    pub extracted_at: DateTime<Utc>,
}

/// Detect the `ffmpeg` binary in PATH. Same strategy as ffprobe (Spike F).
pub fn detect_ffmpeg() -> Result<PathBuf> {
    which("ffmpeg")
}

/// Extract a single frame from the video at the given timestamp.
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

    // Fast + ACCURATE seek (see module docs): a coarse input seek snaps to the
    // keyframe just before the target (cheap), `-copyts` keeps the original
    // timestamps, and an ABSOLUTE output seek nails the exact frame — so two
    // instants in the same GOP no longer collapse onto one keyframe.
    let coarse = format_seconds((opts.timestamp_s - SEEK_REWIND_S).max(0.0));
    let target = format_seconds(opts.timestamp_s);
    let status = Command::new(&ffmpeg)
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

    // Try to read back the precise timestamp of the produced PNG by
    // re-running ffprobe on the SOURCE near that timestamp. ffmpeg with
    // fast `-ss` may snap to a keyframe; we surface the delta honestly.
    let actual = probe_actual_timestamp(opts.video_path, opts.timestamp_s).ok();
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

/// Report the TRUE presentation time of the frame our accurate seek lands on.
///
/// Critically, this reads decoded FRAMES, not packets. A packet probe (like
/// any keyframe-snapping seek) returns the GOP's keyframe time, so every
/// instant inside that GOP reports the SAME timestamp — the exact failure that
/// made distinct collected frames share one `actual_timestamp_s` and the speed
/// regression abort with Δt = 0. Instead we decode frames in a small window
/// (the coarse seek may still snap its START to a keyframe, but we then scan
/// forward) and pick the first whose presentation time reaches the request —
/// mirroring the accurate output-seek used by `extract_frame`.
fn probe_actual_timestamp(video: &Path, ts_s: f64) -> Result<f64> {
    let ffprobe = detect_ffprobe()?;
    let ts = ts_s.max(0.0);
    let start = (ts - SEEK_REWIND_S).max(0.0);
    // Window must comfortably reach past the target so the frame at/after it
    // is decoded and listed.
    let window = SEEK_REWIND_S + 4.0;
    let output = Command::new(&ffprobe)
        .args([
            "-v",
            "error",
            "-read_intervals",
            &format!("{start}%+{window}"),
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
    // First frame whose time reaches the request (where accurate output-seek
    // lands); fall back to the closest frame just before the window's end.
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
    let output = Command::new(ffmpeg).arg("-version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::to_string)
}

/// Format seconds as `HH:MM:SS.mmm` — ffmpeg accepts both decimal seconds
/// and HH:MM:SS notation; we use the latter for readability in process
/// listings during debugging.
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

/// Estimate the frame index from `timestamp_s` and a declared FPS.
/// Returns `None` if either input is missing/zero. The result is ALWAYS
/// estimated — the orchestrator must mark the row accordingly.
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
