//! Croqui domain model (Spike E — Croqui Engine).
//!
//! Same shape as Laudo (Spike B): Rust treats the document body as opaque
//! JSON. The Croqui Engine on the frontend owns the `.sicrocroqui` schema.
//! Rust persists one row per croqui + the JSON blob on disk.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CroquiStatus {
    Draft,
    Ready,
    Archived,
}

impl CroquiStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Ready => "ready",
            Self::Archived => "archived",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "ready" => Some(Self::Ready),
            "archived" => Some(Self::Archived),
            _ => None,
        }
    }
}

impl Default for CroquiStatus {
    fn default() -> Self {
        Self::Draft
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Croqui {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub title: String,
    pub relative_path: String,
    pub status: CroquiStatus,
    pub schema_version: String,
    pub last_export_relative_path: Option<String>,
    /// "viario" (.sicrocroqui) | "corporal" (.sicrocorpo). Migration 017 —
    /// default "viario" pros croquis existentes.
    #[serde(default = "default_kind")]
    pub kind: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn default_kind() -> String {
    "viario".to_string()
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct NewCroquiInput {
    pub title: String,
    /// "viario" | "corporal". Ausente/desconhecido → "viario".
    #[serde(default)]
    pub kind: Option<String>,
}

/// Returned by read/save commands — Croqui row + the full `.sicrocroqui`
/// JSON envelope. Schema is enforced on the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CroquiDoc {
    pub croqui: Croqui,
    pub doc: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportCroquiPngInput {
    /// base64-encoded PNG bytes (no `data:` prefix expected).
    pub png_base64: String,
}
