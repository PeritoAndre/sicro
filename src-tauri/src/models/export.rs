//! Export artefact model.
//!
//! An `Export` row records ONE artefact produced by the Export Engine:
//! HTML intermediate, PDF or DOCX. The file lives in the workspace under
//! `exports/<kind>/`. Field names match the TypeScript side
//! (`src/types/export.ts`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExportKind {
    Html,
    Pdf,
    Docx,
}

impl ExportKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
            Self::Docx => "docx",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "html" => Some(Self::Html),
            "pdf" => Some(Self::Pdf),
            "docx" => Some(Self::Docx),
            _ => None,
        }
    }

    pub fn subdir(&self) -> &'static str {
        match self {
            Self::Html => "exports/html",
            Self::Pdf => "exports/pdf",
            Self::Docx => "exports/docx",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Pdf => "pdf",
            Self::Docx => "docx",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Export {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub laudo_id: Uuid,
    pub kind: ExportKind,
    pub relative_path: String,
    pub file_size: i64,
    pub created_at: DateTime<Utc>,
}
