/**
 * Espelho TS do gerenciador de IA (Rust: `commands/ai_commands.rs`).
 * Snake_case porque o backend serializa os campos como estão.
 */

export interface CatalogItem {
  id: string;
  /** "build" (motor whisper.cpp) | "model" (modelo ggml). */
  kind: "build" | "model";
  label: string;
  url: string;
  filename: string;
  approx_mb: number;
  is_zip: boolean;
  version: string;
  gpu: boolean;
  lang: string;
  note: string;
}

export interface AiCatalog {
  gpu_detected: boolean;
  items: CatalogItem[];
}

export interface InstalledModel {
  filename: string;
  size_bytes: number;
}

export interface AiStatus {
  whisper_bin_path: string;
  whisper_ok: boolean;
  whisper_version: string;
  model_path: string;
  model_ok: boolean;
  installed_models: InstalledModel[];
}

export interface AiUpdateInfo {
  current: string;
  latest: string;
  update_available: boolean;
}

/** Payload do evento Tauri "ai-download-progress". */
export interface AiProgress {
  id: string;
  received: number;
  total: number;
}
