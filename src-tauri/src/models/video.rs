//! Video models (Spike F — Video Engine).
//!
//! Mirrors the Python lab models (`SICRO_VIDEO_LAB_RELATORIO.md` §3)
//! adapted to the SICRO Desktop wire format. Field names use snake_case;
//! the frontend types in `src/types/video.ts` mirror them 1:1.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// VideoMedia

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMedia {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    /// Caminho original no disco do usuário (informativo).
    pub original_path: Option<String>,
    /// Caminho relativo dentro do workspace: `videos/originais/<filename>`.
    pub relative_path: String,
    pub filename: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub duration_s: Option<f64>,
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub pixel_format: Option<String>,
    pub fps_declared: Option<f64>,
    /// String "30000/1001" — preservamos fidelidade técnica.
    pub avg_frame_rate: Option<String>,
    pub r_frame_rate: Option<String>,
    pub time_base: Option<String>,
    pub frame_count: Option<i64>,
    pub bitrate: Option<i64>,
    pub raw_probe_json: String,
    pub warnings_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterVideoInput {
    /// Absolute path on the user's disk (returned by the OS file dialog).
    pub source_path: String,
}

// ---------------------------------------------------------------------------
// VideoEvent

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoEvent {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_hash: String,
    pub timestamp_s: f64,
    pub timestamp_label: String,
    pub frame_observed: Option<i64>,
    pub pts: Option<i64>,
    pub time_base: Option<String>,
    pub category: String,
    pub title: String,
    pub description: String,
    pub reviewed: bool,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateVideoEventInput {
    pub media_hash: String,
    pub timestamp_s: f64,
    pub category: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateVideoEventInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub timestamp_s: Option<f64>,
    pub reviewed: Option<bool>,
}

// ---------------------------------------------------------------------------
// VideoExport (PNG frame extraction)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoExport {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_hash: String,
    pub event_id: Option<Uuid>,
    /// 'frame_png' — reserved discriminator for future export kinds.
    pub r#type: String,
    pub requested_timestamp_s: f64,
    pub actual_timestamp_s: Option<f64>,
    pub delta_s: Option<f64>,
    /// Workspace-relative path: `videos/storyboards/frames/<filename>.png`.
    pub output_path: String,
    pub filename: String,
    pub sidecar_json_path: Option<String>,
    /// Full extraction context (timestamp delta, ffmpeg version, etc.).
    pub details_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CollectFrameInput {
    pub media_hash: String,
    pub timestamp_s: f64,
    pub event_id: Option<Uuid>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// Result of `collect_video_frame`: the export row + the storyboard row
/// already linked, so the frontend can update both panels in one go.
#[derive(Debug, Clone, Serialize)]
pub struct CollectFrameResult {
    pub export: VideoExport,
    pub storyboard_frame: VideoStoryboardFrame,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// VideoStoryboardFrame

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoStoryboardFrame {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_hash: String,
    pub event_id: Option<Uuid>,
    pub export_id: Option<Uuid>,
    pub title: String,
    pub caption: String,
    pub notes: String,
    pub requested_timestamp_s: f64,
    pub actual_timestamp_s: Option<f64>,
    pub delta_s: Option<f64>,
    pub observed_frame_index: Option<i64>,
    pub estimated_total_frames: Option<i64>,
    /// True when the frame index is an estimate (FPS-based), false when
    /// FFmpeg returned a confident PTS-anchored position.
    pub frame_index_is_estimated: bool,
    pub pts: Option<i64>,
    pub time_base: Option<String>,
    pub output_path: String,
    pub sidecar_json_path: Option<String>,
    pub reviewed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateStoryboardFrameInput {
    pub title: Option<String>,
    pub caption: Option<String>,
    pub notes: Option<String>,
    pub reviewed: Option<bool>,
}

// ---------------------------------------------------------------------------
// VideoOperationLog

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoOperationLog {
    pub id: i64,
    pub occurrence_id: Uuid,
    pub media_hash: Option<String>,
    pub action: String,
    pub details_json: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Aggregated bundle returned by `open_video_media` so the UI can hydrate
// the whole module in a single round-trip.

#[derive(Debug, Clone, Serialize)]
pub struct VideoBundle {
    pub media: VideoMedia,
    pub events: Vec<VideoEvent>,
    pub exports: Vec<VideoExport>,
    pub storyboard: Vec<VideoStoryboardFrame>,
}
