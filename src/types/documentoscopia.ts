/**
 * Espelho TS dos modelos do módulo Documentoscopia (Rust:
 * `models/documentoscopia.rs`). Coordenadas de bbox são NORMALIZADAS (0..1)
 * em relação à página. O arquivo original nunca é alterado — `relative_path`
 * aponta para a cópia preservada no workspace, com `sha256` próprio.
 */

export interface DocumentCaseFile {
  id: string;
  occurrence_id: string;
  title: string;
  original_filename: string;
  relative_path: string;
  /** "pdf" | "image" */
  file_type: string;
  extension: string;
  doc_type: string;
  sha256: string;
  size_bytes: number;
  page_count: number;
  has_text_layer: boolean;
  /** importado | ocr_pendente | ocr_concluido | revisado */
  status: string;
  metadata_json: string;
  notes: string;
  imported_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface DocumentPage {
  id: string;
  document_id: string;
  page_number: number;
  width: number;
  height: number;
  rotation: number;
  dpi: number | null;
  rendered_path: string | null;
  thumbnail_path: string | null;
  created_at: string;
}

export interface OcrRun {
  id: string;
  document_id: string;
  page_number: number | null;
  engine: string;
  engine_version: string;
  language: string;
  mode: string;
  status: string;
  avg_confidence: number | null;
  block_count: number;
  parameters_json: string;
  created_at: string;
}

export interface OcrTextBlock {
  id: string;
  ocr_run_id: string;
  document_id: string;
  page_number: number;
  text: string;
  confidence: number | null;
  bbox_x: number;
  bbox_y: number;
  bbox_w: number;
  bbox_h: number;
  block_type: string;
  reading_order: number;
  corrected_text: string | null;
  reviewed: boolean;
  created_at: string;
}

export interface DetectedField {
  id: string;
  document_id: string;
  page_number: number | null;
  field_name: string;
  field_value: string;
  confidence: number | null;
  source: string;
  bbox_x: number | null;
  bbox_y: number | null;
  bbox_w: number | null;
  bbox_h: number | null;
  reviewed: boolean;
  corrected_value: string | null;
  created_at: string;
}

export interface DocumentRegion {
  id: string;
  document_id: string;
  page_number: number;
  region_type: string;
  bbox_x: number;
  bbox_y: number;
  bbox_w: number;
  bbox_h: number;
  label: string;
  confidence: number | null;
  notes: string;
  created_at: string;
}

export interface DocumentAnalysis {
  id: string;
  document_id: string;
  analysis_type: string;
  status: string;
  parameters_json: string;
  result_json: string;
  summary: string;
  created_at: string;
}

export interface ComparisonSession {
  id: string;
  occurrence_id: string;
  questioned_document_id: string;
  reference_document_id: string;
  comparison_type: string;
  results_json: string;
  summary: string;
  created_at: string;
}

export interface DocumentLog {
  id: string;
  document_id: string | null;
  occurrence_id: string;
  action: string;
  parameters_json: string;
  result: string;
  source_hash: string | null;
  output_hash: string | null;
  actor: string | null;
  created_at: string;
}

export interface OcrRunResult {
  run: OcrRun;
  blocks: OcrTextBlock[];
}

// --- Inputs (snake_case — batem com os structs serde do backend) ---

export interface TextBlockInput {
  page_number: number;
  text: string;
  confidence: number | null;
  bbox_x: number;
  bbox_y: number;
  bbox_w: number;
  bbox_h: number;
  block_type: string;
  reading_order: number;
}

export interface OcrRunInput {
  page_number: number | null;
  engine: string;
  engine_version: string;
  language: string;
  mode: string;
  parameters_json: string;
  blocks: TextBlockInput[];
}

export interface FieldInput {
  page_number: number | null;
  field_name: string;
  field_value: string;
  confidence: number | null;
  source: string;
  bbox_x: number | null;
  bbox_y: number | null;
  bbox_w: number | null;
  bbox_h: number | null;
}

export interface RegionInput {
  page_number: number;
  region_type: string;
  bbox_x: number;
  bbox_y: number;
  bbox_w: number;
  bbox_h: number;
  label: string;
  confidence: number | null;
  notes: string;
}

/** Tipos de documento oferecidos na UI (rótulo + valor persistido). */
export const DOC_TYPES: { value: string; label: string }[] = [
  { value: "cnh", label: "CNH" },
  { value: "rg", label: "RG" },
  { value: "crlv", label: "CRLV" },
  { value: "contrato", label: "Contrato" },
  { value: "recibo", label: "Recibo" },
  { value: "declaracao", label: "Declaração" },
  { value: "oficio", label: "Ofício" },
  { value: "boletim", label: "Boletim" },
  { value: "laudo", label: "Laudo externo" },
  { value: "processo", label: "Processo" },
  { value: "outro", label: "Outro" },
];

export function docTypeLabel(value: string): string {
  return DOC_TYPES.find((t) => t.value === value)?.label ?? "Outro";
}

/** Rótulo + cor (token CSS) de um status de documento. */
export function docStatusInfo(status: string): { label: string; tone: string } {
  switch (status) {
    case "importado":
      return { label: "Importado", tone: "info" };
    case "ocr_pendente":
      return { label: "OCR pendente", tone: "warn" };
    case "ocr_concluido":
      return { label: "OCR concluído", tone: "accent" };
    case "revisado":
      return { label: "Revisado", tone: "success" };
    default:
      return { label: status, tone: "muted" };
  }
}
