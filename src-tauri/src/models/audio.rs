//! Modelos do módulo Áudio (Camada 1).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Uma mídia de áudio registrada numa ocorrência. O `relative_path` aponta
/// para o WAV de análise (derivado determinístico); o original importado fica
/// preservado em `original_relative_path` (quando `kind == "importado"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMedia {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    /// "importado" | "extraido".
    pub kind: String,
    pub original_path: Option<String>,
    pub original_relative_path: Option<String>,
    pub relative_path: String,
    pub filename: String,
    /// Hash do WAV de análise (chave de dedupe na ocorrência).
    pub sha256: String,
    pub original_sha256: Option<String>,
    pub source_video_sha256: Option<String>,
    pub size_bytes: u64,
    pub duration_s: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub codec: Option<String>,
    pub bitrate: Option<i64>,
    pub raw_probe_json: String,
    pub warnings_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Marcador temporal posto pelo perito no player de áudio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioMarker {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    /// Hash do WAV de análise ao qual o marcador pertence.
    pub audio_sha256: String,
    pub t_seconds: f64,
    pub label: String,
    pub created_at: DateTime<Utc>,
}

/// Registro de um realce (auxílio de escuta) aplicado a um áudio. Não-destrutivo:
/// a saída é uma nova mídia; aqui fica o vínculo origem→saída + cadeia FFmpeg.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioEnhancement {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub source_audio_sha256: String,
    pub output_audio_sha256: String,
    pub filters_json: String,
    pub created_at: DateTime<Utc>,
}

/// Um segmento de degravação MANUAL (trabalho do perito): trecho temporal com
/// rótulo de locutor e texto. O tool não transcreve nem interpreta — só guarda.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioTranscriptSegment {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub audio_sha256: String,
    pub idx: i64,
    pub t_start: f64,
    pub t_end: Option<f64>,
    pub speaker: String,
    pub text: String,
    /// Rascunho da IA (draft, confidence, words) — `None` se escrito pelo perito.
    pub ai: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// Um turno de fala da separação de locutores (locutor 1..N).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiarTurn {
    pub t_start: f64,
    pub t_end: f64,
    pub speaker: u32,
}

/// Separação de locutores de um áudio (uma por áudio; rodar de novo substitui).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDiarization {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub audio_sha256: String,
    pub turns: Vec<DiarTurn>,
    /// Nomes dados pelo perito (índice = locutor − 1; vazio = "Locutor N").
    pub names: Vec<String>,
    pub params: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Entrada de segmento vinda do front (sem id/custódia — gerados ao salvar).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptSegmentInput {
    pub idx: i64,
    pub t_start: f64,
    pub t_end: Option<f64>,
    #[serde(default)]
    pub speaker: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub ai: Option<serde_json::Value>,
}
