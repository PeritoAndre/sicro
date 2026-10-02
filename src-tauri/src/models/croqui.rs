//! Croqui: como o laudo, o corpo é JSON opaco (schema do Croqui Engine, no
//! front); o Rust persiste a linha e o blob. Espelhado em `src/types/croqui.ts`.

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
    /// "viario" (.sicrocroqui) | "corporal" (.sicrocorpo). Migration 017;
    /// croquis antigos caem em "viario".
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

/// Linha + envelope `.sicrocroqui` completo; schema imposto no front.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CroquiDoc {
    pub croqui: Croqui,
    pub doc: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportCroquiPngInput {
    /// PNG em base64, sem prefixo `data:`.
    pub png_base64: String,
}
