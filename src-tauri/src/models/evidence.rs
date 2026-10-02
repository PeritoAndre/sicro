//! Vínculo evidência → laudo. As linhas são trilha de auditoria; os atributos
//! de verdade vivem no nó do `.sicrodoc`. Espelhado em `src/types/evidence.ts`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Casa com a coluna `source_kind` de `evidence_links`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSourceKind {
    Photo,
    Croqui,
    VideoFrame,
    VideoStoryboard,
    OccurrenceField,
    ChecklistTable,
    TracesTable,
    MeasurementsTable,
    FieldNote,
}

impl EvidenceSourceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Photo => "photo",
            Self::Croqui => "croqui",
            Self::VideoFrame => "video_frame",
            Self::VideoStoryboard => "video_storyboard",
            Self::OccurrenceField => "occurrence_field",
            Self::ChecklistTable => "checklist_table",
            Self::TracesTable => "traces_table",
            Self::MeasurementsTable => "measurements_table",
            Self::FieldNote => "field_note",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceLink {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    /// Sempre "laudo" por enquanto.
    pub target_type: String,
    /// UUID do laudo.
    pub target_id: String,
    pub relation_type: String,
    pub source_kind: EvidenceSourceKind,

    pub media_asset_id: Option<Uuid>,
    pub croqui_id: Option<Uuid>,
    pub video_media_hash: Option<String>,
    pub video_event_id: Option<Uuid>,
    pub video_storyboard_frame_id: Option<Uuid>,
    pub field_note_id: Option<Uuid>,

    pub relative_path: Option<String>,
    pub source_hash: Option<String>,
    pub metadata_json: String,

    pub created_at: DateTime<Utc>,
}

/// Payload de `record_evidence_link`; o front manda o que o kind exige.
#[derive(Debug, Clone, Deserialize)]
pub struct RecordEvidenceLinkInput {
    pub target_type: String,
    pub target_id: String,
    pub source_kind: EvidenceSourceKind,
    #[serde(default = "default_relation")]
    pub relation_type: String,
    #[serde(default)]
    pub media_asset_id: Option<Uuid>,
    #[serde(default)]
    pub croqui_id: Option<Uuid>,
    #[serde(default)]
    pub video_media_hash: Option<String>,
    #[serde(default)]
    pub video_event_id: Option<Uuid>,
    #[serde(default)]
    pub video_storyboard_frame_id: Option<Uuid>,
    #[serde(default)]
    pub field_note_id: Option<Uuid>,
    #[serde(default)]
    pub relative_path: Option<String>,
    #[serde(default)]
    pub source_hash: Option<String>,
    #[serde(default = "default_metadata")]
    pub metadata_json: String,
}

fn default_relation() -> String {
    "inserted_in_laudo".to_string()
}
fn default_metadata() -> String {
    "{}".to_string()
}

/// Bytes para o front/renderer embutir (data URI no HTML/PDF, binário no DOCX).
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceAsset {
    pub relative_path: String,
    pub mime_type: String,
    /// Sem prefixo `data:`.
    pub base64: String,
    pub size_bytes: u64,
}
