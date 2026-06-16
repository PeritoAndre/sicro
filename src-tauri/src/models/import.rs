//! Models for the .sicroapp importer (Spike D).
//!
//! Three persisted entities + one transient report:
//!   - `Import`         — row in `imports` (one per .sicroapp brought in).
//!   - `MediaAsset`     — row in `media_assets` (one per binary extracted).
//!   - `EvidenceItem`   — row in `evidence_items` (one per domain evidence).
//!   - `ImportReport`   — JSON written to `imports/<id>/import_report.json`
//!                        AND returned to the frontend.
//!
//! Wire format is serde-default snake_case so the TypeScript mirrors are
//! drop-in. Never rename a field once it's been written to disk.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// imports

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    /// All required files present, hashes verified, no missing media.
    Imported,
    /// Imported but with one or more non-fatal warnings (missing media,
    /// missing optional JSON, unknown extra files, etc.).
    ImportedWithWarnings,
    /// Aborted before persisting the occurrence. Row may still exist for
    /// audit purposes if the failure happened late.
    Failed,
}

impl ImportStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Imported => "imported",
            Self::ImportedWithWarnings => "imported_with_warnings",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "imported" => Some(Self::Imported),
            "imported_with_warnings" => Some(Self::ImportedWithWarnings),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub id: Uuid,
    pub package_relative_path: String,
    pub original_filename: Option<String>,
    pub package_sha256: String,
    pub format: String,
    pub schema_version: String,
    pub app_name: Option<String>,
    pub app_version: Option<String>,
    pub mobile_occurrence_id: Option<String>,
    pub status: ImportStatus,
    /// JSON array of strings, surfaced as-is to the frontend.
    pub warnings_json: String,
    pub errors_json: String,
    /// Raw `manifest.json` payload, preserved verbatim for future audits.
    pub raw_manifest_json: String,
    pub imported_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// media_assets

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaAssetType {
    Photo,
    // Reserved — Spike D only emits Photo. Future spikes:
    // Video, Audio, Attachment,
}

impl MediaAssetType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Photo => "photo",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: Uuid,
    pub import_id: Uuid,
    pub occurrence_id: Uuid,
    pub original_id: Option<String>,
    pub r#type: MediaAssetType,
    pub relative_path: String,
    pub original_package_path: Option<String>,
    pub original_filename: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: u64,
    pub sha256: Option<String>,
    pub captured_at: Option<DateTime<Utc>>,
    pub imported_at: DateTime<Utc>,
    pub category: Option<String>,
    pub caption: Option<String>,
    /// Verbatim payload from `fotos.json[].*`, so the UI can show categoria
    /// or any future field even before it's modelled here.
    pub raw_json: String,
}

// ---------------------------------------------------------------------------
// evidence_items

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub media_asset_id: Option<Uuid>,
    pub r#type: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub source_module: Option<String>,
    pub captured_at: Option<DateTime<Utc>>,
    pub metadata_json: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Import report (transient — written to disk + returned to UI)

/// Summary of one `.sicroapp` import. Always-defined fields use safe defaults
/// so the frontend can render the panel even if the importer aborted half-way.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImportReport {
    pub import_id: Option<Uuid>,
    pub occurrence_id: Option<Uuid>,
    pub workspace_path: Option<String>,

    pub package_original_filename: Option<String>,
    pub package_sha256: Option<String>,
    pub package_size_bytes: u64,

    pub format: Option<String>,
    pub schema_version: Option<String>,
    pub app_name: Option<String>,
    pub app_version: Option<String>,
    pub mobile_occurrence_id: Option<String>,
    pub generated_at: Option<String>,
    pub exported_at: Option<String>,

    // Occurrence summary (mirrors what the user typed in mobile, helps QA).
    pub tipo_pericia: Option<String>,
    pub natureza: Option<String>,
    pub resultado: Option<String>,
    pub bo: Option<String>,
    pub protocolo: Option<String>,
    pub municipio: Option<String>,
    pub bairro: Option<String>,
    pub logradouro: Option<String>,

    // Counts (declared vs. imported lets the UI flag partial imports).
    pub photos_declared: u32,
    pub photos_imported: u32,
    pub photos_missing: u32,

    // Hash verification.
    pub hashes_present: bool,
    pub hashes_verified_ok: u32,
    pub hashes_mismatched: Vec<HashMismatch>,
    pub files_missing_from_hashes: Vec<String>,

    // Files discovered in the ZIP (relative to root).
    pub jsons_read: Vec<String>,
    pub jsons_missing: Vec<String>,
    pub files_ignored: Vec<String>,

    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub status: Option<ImportStatus>,

    /// Echo of important counts straight from `manifest.json -> contagens`.
    pub manifest_counts: Option<serde_json::Value>,

    pub imported_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashMismatch {
    pub path: String,
    pub expected: String,
    pub actual: String,
}

// ---------------------------------------------------------------------------
// Input from the frontend

/// Payload sent by the front-end when the user picks a .sicroapp.
#[derive(Debug, Clone, Deserialize)]
pub struct ImportSicroappInput {
    /// Absolute path to the chosen .sicroapp file.
    pub package_path: String,
    /// Parent directory for the workspace. `None` means use the OS Documents
    /// folder (same default as Spike A).
    #[serde(default)]
    pub parent_directory: Option<String>,
}

/// Final result returned to the front-end when import succeeds.
/// Combines the Import row, the imported Occurrence row, the workspace
/// path, and the full ImportReport (which the UI panel renders).
#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub import: Import,
    pub occurrence: crate::models::Occurrence,
    pub workspace_path: String,
    pub report: ImportReport,
}
