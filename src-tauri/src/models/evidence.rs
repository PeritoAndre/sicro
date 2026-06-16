//! Evidence link model (MVP 4 — Evidência → Laudo).
//!
//! `EvidenceLink` rows are written when the perito clicks "Inserir" on the
//! Laudo Inspector's "Evidências" tab. They are the audit trail; the
//! source-of-truth attributes still live on the `.sicrodoc` node.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Discriminator for what kind of evidence was inserted. Matches the
/// `source_kind` column in `evidence_links`.
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
    /// Always `"laudo"` for now; reserved for future surfaces.
    pub target_type: String,
    /// UUID of the laudo (when target_type == "laudo").
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

/// Wire payload for `record_evidence_link`. Most fields are optional —
/// the frontend supplies whatever the kind requires.
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

/// Returned by `read_evidence_asset` so the frontend / renderer can embed
/// the bytes (data URI for HTML/PDF, binary for DOCX).
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceAsset {
    pub relative_path: String,
    pub mime_type: String,
    /// Base64-encoded bytes (no `data:` prefix).
    pub base64: String,
    pub size_bytes: u64,
}
