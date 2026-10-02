//! Modelos do Registro de Evidências: projeção consolidada das tabelas de cada
//! módulo (nada é migrado para uma tabela única). Espelhados em
//! `src/types/evidence_registry.ts` — mudar nos dois.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A forma string é estável (vai para o relatório e para filtros da UI):
/// nunca renomear um valor antigo.
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
    ImageAnalysis,
    ImageExport,
    Audio,
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

/// Veredito de integridade de um item. Sem `relative_path` o item fica
/// `Unknown`, para a UI não mostrar verde/vermelho enganoso.
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

/// Uma linha do registro consolidado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRegistryItem {
    /// Sintético: "<kind>:<id-da-origem>", para não colidir entre kinds.
    pub id: String,
    pub occurrence_id: Uuid,
    pub kind: EvidenceKind,
    /// Ex.: `image/png`, `mp4`.
    pub subtype: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    /// Módulo de origem (`importer`, `croqui`, `video`…) — só exibição/filtro.
    pub source_module: String,
    /// Id no mobile (itens importados) ou outro id estável de origem.
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
    /// JSON preservado como veio ("Ver metadados…").
    pub metadata_json: String,
}

/// Contadores da aba "Resumo".
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
    /// `ok` | `warning` | `critical`.
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

/// Referência quebrada dentro de um `.sicrodoc`.
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

/// Relatório de integridade — aba "Integridade" e HTML persistido.
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
    /// `false` quando o perito pediu só a verificação leve.
    pub deep_check_executed: bool,
}

/// Relatório salvo, devolvido por `generate_workspace_integrity_report`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityReportArtifact {
    /// Relativo ao workspace (`reports/workspace_integrity_<TS>.html`).
    pub relative_path: String,
    pub generated_at: DateTime<Utc>,
    pub overall_status: String,
    pub item_count: u32,
}

/// Opções de `verify_workspace_integrity`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VerifyOptions {
    /// Recalcula SHA-256 de todo item com hash guardado. Pesado — botão
    /// "Verificação profunda".
    #[serde(default)]
    pub deep: bool,
}
