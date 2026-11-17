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
  /** Separação de locutores: há pacote para este sistema? */
  diar_available: boolean;
  diar_ok: boolean;
  diar_version: string;
  diar_approx_mb: number;
  /** Pessoas no vídeo (onnxruntime + YOLOX): pacote para este sistema? instalado? */
  pessoas_available: boolean;
  pessoas_ok: boolean;
  pessoas_version: string;
  pessoas_approx_mb: number;
  /** Super-resolução (Real-ESRGAN): pacote para este sistema? instalado? */
  superres_available: boolean;
  superres_ok: boolean;
  superres_version: string;
  superres_approx_mb: number;
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

// ---- Fase 2 (contratos-fase2.md; `ia/gpu.rs` e `commands/ia_commands.rs`, P2.1) ----
/** Onde a sessão rodou. */
export type Provedor = "cpu" | "cuda";
/** `nvidia-smi --query-gpu=name,driver_version,compute_cap,memory.total`. */
export interface Placa { nome: string; driver: string; compute_cap: string; memoria_mb: number }
/** Aceleração por GPU na tela; `requisito` e `motivo` = códigos do catálogo (`gpu_*`). */
export interface EstadoGpuIa {
  disponivel: boolean;
  placa: Placa | null;
  requisito: string | null;
  instalada: boolean;
  pronta: boolean;
  motivo: string | null;
  mb: number;
}
/** Estado da IA para o Gerenciador (`ia_estado`). `reabrir`: vale depois de reabrir o SICRO. */
export interface EstadoIa {
  ligada: boolean;
  gpu: EstadoGpuIa;
  rastreador: boolean;
  rastreador_mb: number;
  /** Item "Ocultação" (P4.5). */
  redacao: boolean;
  redacao_mb: number;
  carregada: Provedor | null;
  reabrir: boolean;
}
