//! Laudo domain model.
//!
//! Spike B treats the document body as an opaque JSON blob (the TipTap document
//! lives entirely on the front-end). The Rust side is responsible only for:
//!   - persisting one row per laudo in the workspace's SQLite;
//!   - writing/reading the corresponding `.sicrodoc` file (UTF-8 JSON).
//!
//! Field names use snake_case to match the TypeScript wire format in
//! `src/types/laudo.ts`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaudoStatus {
    Rascunho,
    Revisado,
    Exportado,
    Assinado,
    Arquivado,
}

impl LaudoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rascunho => "rascunho",
            Self::Revisado => "revisado",
            Self::Exportado => "exportado",
            Self::Assinado => "assinado",
            Self::Arquivado => "arquivado",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "rascunho" => Some(Self::Rascunho),
            "revisado" => Some(Self::Revisado),
            "exportado" => Some(Self::Exportado),
            "assinado" => Some(Self::Assinado),
            "arquivado" => Some(Self::Arquivado),
            _ => None,
        }
    }
}

impl Default for LaudoStatus {
    fn default() -> Self {
        Self::Rascunho
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Laudo {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub title: String,
    pub template_id: String,
    pub relative_path: String,
    pub status: LaudoStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_export_pdf: Option<DateTime<Utc>>,
    pub last_export_docx: Option<DateTime<Utc>>,
    /// H — Tipo de assinatura digital encontrada no `.sicrodoc` (se
    /// existir). Populado pelo `list_laudos` em best-effort: lê
    /// `finalization.signature.type` quando consegue, senão fica `None`.
    /// Valores típicos: `"gov_br"`, `"A1"`, `"A3"`, `"mock"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_type: Option<String>,
}

/// Input for `create_laudo`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct NewLaudoInput {
    pub title: String,
    #[serde(default = "default_template")]
    pub template_id: String,
}

fn default_template() -> String {
    "documento_em_branco".to_string()
}

/// Returned by read/save commands. `doc` is the full `.sicrodoc` payload
/// (schema_version + content + metadata) — the schema is enforced on the
/// front-end by the Document Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaudoDoc {
    pub laudo: Laudo,
    pub doc: serde_json::Value,
    /// Assinatura de versão: a versão do SICRO que SALVOU o arquivo, SE ela for
    /// mais nova que a deste app (lida de `doc.sicro_app_version`). `None` =
    /// igual/mais antiga ou sem carimbo → abre normal. Não persiste; é só sinal
    /// de UI para avisar "salvo em versão mais nova; pode não abrir certo".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opened_with_newer_version: Option<String>,
}
