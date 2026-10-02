//! Modelos da Documentoscopia (migration 016). Bounding boxes normalizados (0..1)
//! em relação à página. O arquivo original nunca é alterado: `relative_path` é a
//! cópia preservada no workspace, com `sha256` próprio (cadeia de custódia).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Documento (PDF ou imagem) importado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentCaseFile {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub title: String,
    pub original_filename: String,
    pub relative_path: String,
    /// "pdf" | "image".
    pub file_type: String,
    pub extension: String,
    /// cnh|rg|crlv|contrato|recibo|declaracao|oficio|boletim|laudo|processo|outro
    pub doc_type: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub page_count: i64,
    pub has_text_layer: bool,
    /// importado|ocr_pendente|ocr_concluido|revisado
    pub status: String,
    pub metadata_json: String,
    pub notes: String,
    pub imported_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Página renderizada (imagem = 1; PDF = N).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentPage {
    pub id: Uuid,
    pub document_id: Uuid,
    pub page_number: i64,
    pub width: i64,
    pub height: i64,
    pub rotation: i64,
    pub dpi: Option<i64>,
    pub rendered_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Execução de OCR/extração de texto (qualquer motor, inclusive a camada
/// textual do PDF ou o mock).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrRun {
    pub id: Uuid,
    pub document_id: Uuid,
    pub page_number: Option<i64>,
    pub engine: String,
    pub engine_version: String,
    pub language: String,
    pub mode: String,
    pub status: String,
    pub avg_confidence: Option<f64>,
    pub block_count: i64,
    pub parameters_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrTextBlock {
    pub id: Uuid,
    pub ocr_run_id: Uuid,
    pub document_id: Uuid,
    pub page_number: i64,
    pub text: String,
    pub confidence: Option<f64>,
    pub bbox_x: f64,
    pub bbox_y: f64,
    pub bbox_w: f64,
    pub bbox_h: f64,
    pub block_type: String,
    pub reading_order: i64,
    pub corrected_text: Option<String>,
    pub reviewed: bool,
    pub created_at: DateTime<Utc>,
}

/// Campo extraído (heurística/OCR/manual), com revisão humana.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedField {
    pub id: Uuid,
    pub document_id: Uuid,
    pub page_number: Option<i64>,
    pub field_name: String,
    pub field_value: String,
    pub confidence: Option<f64>,
    pub source: String,
    pub bbox_x: Option<f64>,
    pub bbox_y: Option<f64>,
    pub bbox_w: Option<f64>,
    pub bbox_h: Option<f64>,
    pub reviewed: bool,
    pub corrected_value: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Região marcada sobre o documento (detectada ou manual).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRegion {
    pub id: Uuid,
    pub document_id: Uuid,
    pub page_number: i64,
    pub region_type: String,
    pub bbox_x: f64,
    pub bbox_y: f64,
    pub bbox_w: f64,
    pub bbox_h: f64,
    pub label: String,
    pub confidence: Option<f64>,
    pub notes: String,
    pub created_at: DateTime<Utc>,
}

/// Análise assistida (ELA, ruído, integridade…).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentAnalysis {
    pub id: Uuid,
    pub document_id: Uuid,
    pub analysis_type: String,
    pub status: String,
    pub parameters_json: String,
    pub result_json: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

/// Comparação entre documento questionado e padrão.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonSession {
    pub id: Uuid,
    pub occurrence_id: Uuid,
    pub questioned_document_id: Uuid,
    pub reference_document_id: Uuid,
    pub comparison_type: String,
    pub results_json: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentLog {
    pub id: Uuid,
    pub document_id: Option<Uuid>,
    pub occurrence_id: Uuid,
    pub action: String,
    pub parameters_json: String,
    pub result: String,
    pub source_hash: Option<String>,
    pub output_hash: Option<String>,
    pub actor: Option<String>,
    pub created_at: DateTime<Utc>,
}
