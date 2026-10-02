//! Laudo: o corpo do documento é JSON opaco (TipTap, no front); o Rust só
//! persiste a linha e o arquivo `.sicrodoc`.

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
    /// `finalization.signature.type` lido do `.sicrodoc` pelo `list_laudos`
    /// (best-effort). Ex.: "gov_br", "A1", "A3".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_type: Option<String>,
}

/// Entrada de `create_laudo`.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct NewLaudoInput {
    pub title: String,
    #[serde(default = "default_template")]
    pub template_id: String,
}

fn default_template() -> String {
    "documento_em_branco".to_string()
}

/// `doc` é o `.sicrodoc` completo; o schema é imposto no front pelo Document Engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaudoDoc {
    pub laudo: Laudo,
    pub doc: serde_json::Value,
    /// Versão do SICRO que salvou o arquivo, se mais nova que esta
    /// (`doc.sicro_app_version`); `None` = abre normal. Só aviso de UI, não persiste.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opened_with_newer_version: Option<String>,
}
