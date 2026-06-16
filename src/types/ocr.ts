/**
 * Espelho TS do gerenciador de OCR (Rust: `commands/ocr_commands.rs`).
 * Motor: RapidOCR/PaddleOCR (PP-OCRv5, ONNX Runtime embutido no app, sem
 * Python). O perito baixa sob demanda o "pacote latino" (detecção +
 * reconhecimento latino + dicionário), que cobre PT/ES/EN/FR/IT…
 */

export interface OcrPackItem {
  id: string;
  label: string;
  approx_mb: number;
  note: string;
}

export interface OcrCatalog {
  /** O motor (ONNX Runtime) é embutido — sempre disponível. */
  engine_ready: boolean;
  engine_label: string;
  items: OcrPackItem[];
}

export interface InstalledOcrModel {
  filename: string;
  size_bytes: number;
}

export interface OcrStatus {
  engine_ready: boolean;
  engine_label: string;
  /** Os 3 modelos do pacote latino estão presentes? */
  models_ready: boolean;
  models_dir: string;
  installed: InstalledOcrModel[];
}

/** Payload do evento Tauri "ocr-download-progress". */
export interface OcrProgress {
  id: string;
  received: number;
  total: number;
}

/**
 * Resultado de "Verificar atualizações" do pacote de modelos (oar-ocr).
 * Apenas informativo: o motor de inferência é embutido no app; atualizar o
 * pacote é uma ação opt-in do perito.
 */
export interface OcrUpdateInfo {
  current: string;
  latest: string;
  update_available: boolean;
}
