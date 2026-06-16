/**
 * Mirror of `src-tauri/src/models/croqui.rs` (Spike E — Croqui Engine).
 * Field names stay snake_case so serde defaults line up with the wire.
 *
 * The `doc` JSON envelope is owned by the frontend Croqui Engine
 * (see `src/modules/croqui/engine/schema.ts`). Rust treats it as opaque.
 */

export type CroquiStatus = "draft" | "ready" | "archived";

/** Sub-sistema sob a umbrella Croquis: viário (.sicrocroqui) | corporal
 * (.sicrocorpo) | planta (.sicroplanta, planta baixa/cena). */
export type CroquiKind = "viario" | "corporal" | "planta";

export interface Croqui {
  id: string;
  occurrence_id: string;
  title: string;
  relative_path: string;
  status: CroquiStatus;
  schema_version: string;
  last_export_relative_path: string | null;
  /** Migration 017 — default "viario" pros croquis existentes. */
  kind: CroquiKind;
  created_at: string;
  updated_at: string;
}

export interface CroquiDocPayload {
  croqui: Croqui;
  /** Opaque .sicrocroqui JSON — the frontend Croqui Engine validates the shape. */
  doc: unknown;
}

export interface NewCroquiInput {
  title: string;
  /** Ausente → "viario" (croqui de via). "corporal" cria carta de lesões. */
  kind?: CroquiKind;
}

export interface ExportCroquiPngInput {
  /** Base64-encoded PNG bytes (Konva's toDataURL() output works after stripping the data: prefix). */
  png_base64: string;
}

// MVP 9 Round 4 — Drone import flow ------------------------------------------

export interface CropRectInput {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface DroneImportInput {
  /** Absolute path of the source image (drone export / dossier photo). */
  source_absolute_path: string;
  /** Slider value 0..=1; 0 disables lens correction. */
  intensity: number;
  /** Crop rectangle applied AFTER lens correction. */
  crop: CropRectInput;
  /** Optional traceability fields recorded in the sidecar. */
  croqui_id?: string;
  occurrence_id?: string;
}

export interface DroneImportResult {
  /** Workspace-relative path of the corrected + cropped PNG. */
  output_relative_path: string;
  /** Workspace-relative path of the JSON sidecar. */
  sidecar_relative_path: string;
  output_width: number;
  output_height: number;
  /** SHA-256 of the saved PNG bytes (lowercase hex). */
  output_hash_sha256: string;
}
