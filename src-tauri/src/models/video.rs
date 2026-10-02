//! Modelos do módulo Vídeo. Espelhados em `src/types/video.ts` — mudar nos dois.

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
    /// `videos/originais/<filename>`, relativo ao workspace.
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
    /// Fração "30000/1001" preservada como string (fidelidade técnica).
    pub avg_frame_rate: Option<String>,
    pub r_frame_rate: Option<String>,
    pub time_base: Option<String>,
    pub frame_count: Option<i64>,
    pub bitrate: Option<i64>,
    pub raw_probe_json: String,
    pub warnings_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Trecho exportado: SHA-256 do vídeo de origem (None = vídeo original).
    #[serde(default)]
    pub derived_from_hash: Option<String>,
    /// Como o trecho foi feito (JSON): entrada/saída, modo, comando ffmpeg.
    #[serde(default)]
    pub derivation_json: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterVideoInput {
    /// Caminho absoluto (diálogo do SO).
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
// Exportar trecho (migration 019)

#[derive(Debug, Clone, Deserialize)]
pub struct ExportClipInput {
    pub media_hash: String,
    pub start_s: f64,
    pub end_s: f64,
    /// "copy" (sem recompressão, padrão) | "reencode".
    #[serde(default = "default_clip_mode")]
    pub mode: String,
    #[serde(default = "default_true")]
    pub include_audio: bool,
}

fn default_clip_mode() -> String {
    "copy".into()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportClipResult {
    /// O trecho registrado como vídeo do caso.
    pub media: VideoMedia,
    /// Onde o trecho começa de fato no vídeo de origem (copy: quadro-chave).
    pub actual_start_s: f64,
    pub actual_end_s: f64,
    /// Já existia um trecho idêntico (mesmo hash) — devolvido em vez de duplicar.
    pub already_existed: bool,
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// VideoClockCalibration — relógio da câmera (migration 018)

/// "Aos `media_time_s` do vídeo, o relógio da câmera marca `clock_seconds`."
/// Horário da câmera em qualquer instante t = t − media_time_s + clock_seconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoClockCalibration {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_hash: String,
    pub media_time_s: f64,
    /// Segundos desde 00:00:00 (pode ter fração).
    pub clock_seconds: f64,
    /// AAAA-MM-DD, quando a câmera mostra a data.
    pub clock_date: Option<String>,
    /// Exatamente o que o perito digitou.
    pub clock_label: String,
    pub note: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetVideoClockInput {
    pub media_hash: String,
    pub media_time_s: f64,
    pub clock_seconds: f64,
    #[serde(default)]
    pub clock_date: Option<String>,
    pub clock_label: String,
    #[serde(default)]
    pub note: String,
}

// ---------------------------------------------------------------------------
// VideoExport (frame PNG extraído)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoExport {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_hash: String,
    pub event_id: Option<Uuid>,
    /// 'frame_png' — reservado para outros tipos de export.
    pub r#type: String,
    pub requested_timestamp_s: f64,
    pub actual_timestamp_s: Option<f64>,
    pub delta_s: Option<f64>,
    /// `videos/storyboards/frames/<filename>.png`, relativo ao workspace.
    pub output_path: String,
    pub filename: String,
    pub sidecar_json_path: Option<String>,
    /// Contexto completo da extração (delta, versão do ffmpeg…).
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

/// Export + frame do storyboard já vinculados: a UI atualiza os dois painéis
/// numa ida só.
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
    /// true = índice estimado por FPS; false = posição ancorada em PTS pelo FFmpeg.
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
// Pacote devolvido por `open_video_media`: o módulo inteiro numa ida só.

#[derive(Debug, Clone, Serialize)]
pub struct VideoBundle {
    pub media: VideoMedia,
    pub events: Vec<VideoEvent>,
    pub exports: Vec<VideoExport>,
    pub storyboard: Vec<VideoStoryboardFrame>,
}
