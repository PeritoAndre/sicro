//! Modelos do importador de `.sicroapp` (`imports`, `media_assets`,
//! `evidence_items` e o `ImportReport`, gravado em `imports/<id>/import_report.json`).
//! Espelhados em `src/types/import.ts` — mudar nos dois. Nunca renomear campo já gravado.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// imports

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    /// Tudo presente, hashes conferidos.
    Imported,
    /// Avisos não fatais (mídia ausente, JSON opcional faltando…).
    ImportedWithWarnings,
    /// Abortou antes de persistir a ocorrência; a linha pode existir para auditoria.
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
    /// JSON array de strings, repassado ao front.
    pub warnings_json: String,
    pub errors_json: String,
    /// `manifest.json` íntegro, para auditoria.
    pub raw_manifest_json: String,
    pub imported_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// media_assets

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaAssetType {
    Photo,
    // Por enquanto só fotos vêm do .sicroapp.
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
    /// Item de `fotos.json` íntegro — a UI mostra campos ainda não modelados.
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
// Relatório do import (gravado em disco + devolvido à UI)

/// Resumo de um import. Defaults seguros: o painel renderiza mesmo se o
/// importador abortou no meio.
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

    // Resumo da ocorrência (o que foi digitado no mobile).
    pub tipo_pericia: Option<String>,
    pub natureza: Option<String>,
    pub resultado: Option<String>,
    pub bo: Option<String>,
    pub protocolo: Option<String>,
    pub municipio: Option<String>,
    pub bairro: Option<String>,
    pub logradouro: Option<String>,

    // Declarado vs. importado: a UI sinaliza import parcial.
    pub photos_declared: u32,
    pub photos_imported: u32,
    pub photos_missing: u32,

    // Verificação de hashes.
    pub hashes_present: bool,
    pub hashes_verified_ok: u32,
    pub hashes_mismatched: Vec<HashMismatch>,
    pub files_missing_from_hashes: Vec<String>,

    // Arquivos do ZIP (relativos à raiz).
    pub jsons_read: Vec<String>,
    pub jsons_missing: Vec<String>,
    pub files_ignored: Vec<String>,

    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub status: Option<ImportStatus>,

    /// Eco de `manifest.json -> contagens`.
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
// Input do front

/// Payload do front ao escolher um .sicroapp.
#[derive(Debug, Clone, Deserialize)]
pub struct ImportSicroappInput {
    /// Caminho absoluto do .sicroapp.
    pub package_path: String,
    /// Pasta-mãe do workspace; `None` = Documentos do SO.
    #[serde(default)]
    pub parent_directory: Option<String>,
}

/// Devolvido ao front quando o import dá certo.
#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub import: Import,
    pub occurrence: crate::models::Occurrence,
    pub workspace_path: String,
    pub report: ImportReport,
}
