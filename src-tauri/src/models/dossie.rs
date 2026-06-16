//! Dossiê Operacional models (MVP 3).
//!
//! Each struct mirrors one of the structured tables introduced in
//! `migrations/005_dossie.sql`. Field names stay snake_case so serde's
//! default rendering matches the TypeScript wire format.
//!
//! Convention for "raw payload" preservation: every row stores a
//! `raw_json` string containing the original mobile JSON object verbatim.
//! The structured columns above are what the Desktop UI *uses*; the raw
//! payload is what the Desktop *never loses*.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// checklist

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub import_id: Uuid,
    pub original_id: Option<String>,
    pub category: Option<String>,
    pub question: String,
    pub required: bool,
    /// `sim` | `nao` | `nao_se_aplica` | `nao_verificado`
    pub answer: String,
    pub note: Option<String>,
    pub default_note: Option<String>,
    /// `base` | `adicionado`
    pub origin: String,
    pub sort_order: i32,
    pub raw_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ChecklistSummary {
    pub total: u32,
    pub answered: u32,
    pub not_verified: u32,
    pub not_applicable: u32,
    pub required_total: u32,
    pub required_pending: u32,
}

// ---------------------------------------------------------------------------
// entities (vehicle / victim — polymorphic)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub import_id: Uuid,
    pub original_id: Option<String>,
    /// `vehicle` | `victim`
    pub r#type: String,
    pub identifier: Option<String>,
    pub label: Option<String>,
    pub summary: Option<String>,
    /// JSON array of original photo IDs (strings).
    pub photo_ids_json: String,
    pub raw_json: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// traces (vestígios)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub import_id: Uuid,
    pub original_id: Option<String>,
    pub identifier: Option<String>,
    pub r#type: Option<String>,
    pub description: Option<String>,
    pub location_description: Option<String>,
    pub length: Option<f64>,
    pub width: Option<f64>,
    pub unit: Option<String>,
    pub direction: Option<String>,
    pub note: Option<String>,
    pub photo_ids_json: String,
    pub sketch_element_ids_json: String,
    pub raw_json: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// measurements

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Measurement {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub import_id: Uuid,
    pub original_id: Option<String>,
    pub label: Option<String>,
    pub point_a: Option<String>,
    pub point_b: Option<String>,
    pub value: Option<f64>,
    pub unit: Option<String>,
    pub method: Option<String>,
    pub note: Option<String>,
    pub photo_ids_json: String,
    pub sketch_element_ids_json: String,
    pub raw_json: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// field_notes

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldNote {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub import_id: Uuid,
    pub original_id: Option<String>,
    pub text: Option<String>,
    pub category: Option<String>,
    pub priority: Option<String>,
    pub note_created_at: Option<DateTime<Utc>>,
    pub note_updated_at: Option<DateTime<Utc>>,
    pub raw_json: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// timeline_events

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub import_id: Uuid,
    pub original_id: Option<String>,
    pub r#type: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub raw_json: String,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// occurrence_stats

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OccurrenceStats {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub import_id: Uuid,
    pub duration_seconds: Option<i64>,
    pub photos_count: Option<i64>,
    pub victims_count: Option<i64>,
    pub vehicles_count: Option<i64>,
    pub traces_count: Option<i64>,
    pub measurements_count: Option<i64>,
    pub notes_count: Option<i64>,
    pub checklist_items_count: Option<i64>,
    pub answered_checklist_items_count: Option<i64>,
    pub not_applicable_items_count: Option<i64>,
    pub best_gps_accuracy_m: Option<f64>,
    pub gps_readings_count: Option<i64>,
    pub raw_json: String,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Aggregated dossier summary returned by `get_dossie_summary`.

#[derive(Debug, Clone, Serialize)]
pub struct DossieSummary {
    pub occurrence: super::Occurrence,
    pub latest_import: Option<super::Import>,
    pub stats: Option<OccurrenceStats>,
    pub counts: DossieCounts,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DossieCounts {
    pub photos: u32,
    pub vehicles: u32,
    pub victims: u32,
    pub traces: u32,
    pub measurements: u32,
    pub notes: u32,
    pub timeline: u32,
    pub checklist: ChecklistSummary,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RehydrateOutcome {
    pub rehydrated: bool,
    pub from_package_path: Option<String>,
    pub checklist_loaded: u32,
    pub entities_loaded: u32,
    pub traces_loaded: u32,
    pub measurements_loaded: u32,
    pub notes_loaded: u32,
    pub timeline_loaded: u32,
    pub stats_loaded: bool,
    pub warnings: Vec<String>,
}
