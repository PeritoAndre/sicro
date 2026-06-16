//! Evidence Registry models (MVP 5).
//!
//! The registry is a *consolidated read* over the existing tables — we do
//! NOT migrate everyone into a single table. Instead, the aggregator (in
//! `crate::registry`) walks each module's repository and projects rows
//! into `EvidenceRegistryItem` so the frontend gets one uniform list.
//!
//! Two read paths cross the Tauri boundary:
//!   - `EvidenceRegistryItem` — one entry per piece of evidence /
//!     derivative / artefact owned by the workspace.
//!   - `WorkspaceIntegrityReport` — outcome of a verification pass.
//!
//! These models intentionally keep snake_case for serde so the
//! TypeScript mirror reads straight off the wire.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Top-level discriminator for the evidence registry.
///
/// The string form is stable — it shows up in the integrity report and
/// can be filtered on by the UI. New kinds may be added in future MVPs,
/// but old ones must not be renamed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Photo,
    Croqui,
    CroquiExport,
    Video,
    VideoFrame,
    StoryboardFrame,
    Laudo,
    LaudoExport,
    ImportedPackage,
    // MVP 7 — Editor de Imagem Pericial
    ImageAnalysis,
    ImageExport,
    // Módulo Áudio
    Audio,
    // Documentoscopia — documento importado (.pdf/imagem)
    Document,
    Other,
}

impl EvidenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EvidenceKind::Photo => "photo",
            EvidenceKind::Croqui => "croqui",
            EvidenceKind::CroquiExport => "croqui_export",
            EvidenceKind::Video => "video",
            EvidenceKind::VideoFrame => "video_frame",
            EvidenceKind::StoryboardFrame => "storyboard_frame",
            EvidenceKind::Laudo => "laudo",
            EvidenceKind::LaudoExport => "laudo_export",
            EvidenceKind::ImportedPackage => "imported_package",
            EvidenceKind::ImageAnalysis => "image_analysis",
            EvidenceKind::ImageExport => "image_export",
            EvidenceKind::Audio => "audio",
            EvidenceKind::Document => "document",
            EvidenceKind::Other => "other",
        }
    }
}

/// Outcome of the lightweight integrity check for a single item.
///
/// The deep check (hash verification) overlays additional states
/// (`HashMismatch`, `MissingSidecar`). Items that don't carry a
/// `relative_path` at all surface as `Unknown` so the UI can show them
/// without a misleading green/red marker.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityStatus {
    Ok,
    MissingFile,
    HashMismatch,
    MissingSidecar,
    BrokenLink,
    UnsafePath,
    Unknown,
}

impl IntegrityStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            IntegrityStatus::Ok => "ok",
            IntegrityStatus::MissingFile => "missing_file",
            IntegrityStatus::HashMismatch => "hash_mismatch",
            IntegrityStatus::MissingSidecar => "missing_sidecar",
            IntegrityStatus::BrokenLink => "broken_link",
            IntegrityStatus::UnsafePath => "unsafe_path",
            IntegrityStatus::Unknown => "unknown",
        }
    }

    pub fn is_problem(&self) -> bool {
        !matches!(self, IntegrityStatus::Ok | IntegrityStatus::Unknown)
    }
}

/// One row of the consolidated registry. Each piece of evidence in a
/// workspace projects into exactly one of these.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRegistryItem {
    /// Stable, but synthesised — composed as "<kind>:<source-id>". Not
    /// the underlying repo row id by itself, to avoid collisions across
    /// kinds.
    pub id: String,
    pub occurrence_id: Uuid,
    pub kind: EvidenceKind,
    /// Free-form subtype (e.g. `image/png`, `image/jpeg`, `mp4`, etc.).
    pub subtype: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    /// Originating module name (`importer`, `croqui`, `video`, `laudo`,
    /// `dossie`, …) — purely for display / filtering.
    pub source_module: String,
    /// Original identifier on the mobile side (for items that came from
    /// the importer) or another stable upstream id when available.
    pub original_id: Option<String>,
    pub relative_path: Option<String>,
    pub sidecar_relative_path: Option<String>,
    pub hash_sha256: Option<String>,
    pub size_bytes: Option<u64>,
    pub mime_type: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub status: Option<String>,
    pub integrity_status: IntegrityStatus,
    pub integrity_detail: Option<String>,
    pub linked_laudos_count: u32,
    /// JSON object preserved verbatim for the UI ("Ver metadados…").
    pub metadata_json: String,
}

/// Counts surfaced on the "Resumo" tab. Numbers are kept conservative
/// (u32) — workspaces with 4-billion-something items are out of scope.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistrySummary {
    pub photos: u32,
    pub croquis: u32,
    pub croqui_exports: u32,
    pub videos: u32,
    pub video_frames: u32,
    pub storyboard_frames: u32,
    pub laudos: u32,
    pub laudo_exports: u32,
    pub imported_packages: u32,
    // MVP 7
    pub image_analyses: u32,
    pub image_exports: u32,
    pub total_items: u32,
    pub items_with_relative_path: u32,
    pub linked_in_laudos: u32,
    pub files_ok: u32,
    pub files_missing: u32,
    pub unsafe_paths: u32,
    pub broken_links: u32,
    pub hash_mismatches: u32,
    /// Aggregate health: `ok` | `warning` | `critical`.
    pub overall_status: String,
}

impl RegistrySummary {
    pub fn aggregate_status(&self) -> &'static str {
        if self.unsafe_paths > 0 || self.hash_mismatches > 0 {
            "critical"
        } else if self.files_missing > 0 || self.broken_links > 0 {
            "warning"
        } else {
            "ok"
        }
    }
}

/// A broken reference inside a `.sicrodoc`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokenLaudoLink {
    pub laudo_id: Uuid,
    pub laudo_title: String,
    /// `figure` | `storyboardItem` | `evidenceTable` | …
    pub node_type: String,
    pub relative_path: Option<String>,
    pub status: IntegrityStatus,
    pub detail: Option<String>,
}

/// Top-level integrity report — used both for the "Integridade" tab
/// and the persisted HTML report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceIntegrityReport {
    pub occurrence_id: Uuid,
    pub workspace_path: String,
    pub generated_at: DateTime<Utc>,
    pub app_version: String,
    pub summary: RegistrySummary,
    pub items: Vec<EvidenceRegistryItem>,
    pub broken_laudo_links: Vec<BrokenLaudoLink>,
    pub warnings: Vec<String>,
    /// `false` when the perito asked only for the lightweight check.
    pub deep_check_executed: bool,
}

/// Saved report descriptor returned by `generate_workspace_integrity_report`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReportArtifact {
    /// Workspace-relative path (e.g.
    /// `reports/workspace_integrity_20260525_143012.html`).
    pub relative_path: String,
    pub generated_at: DateTime<Utc>,
    pub overall_status: String,
    pub item_count: u32,
}

/// Options for `verify_workspace_integrity`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VerifyOptions {
    /// When `true` the verifier recomputes SHA-256 for every item that
    /// has a stored hash and is otherwise OK. Heavy; reserved for the
    /// "Verificação profunda" button.
    #[serde(default)]
    pub deep: bool,
}
