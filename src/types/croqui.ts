/**
 * Espelha `src-tauri/src/models/croqui.rs`. O envelope `doc` é do Croqui Engine
 * do front (`modules/croqui/engine/schema.ts`); o Rust trata como opaco.
 */

export type CroquiStatus = "draft" | "ready" | "archived";

/** viário (.sicrocroqui) | corporal (.sicrocorpo) | planta (.sicroplanta). */
export type CroquiKind = "viario" | "corporal" | "planta";

export interface Croqui {
  id: string;
  occurrence_id: string;
  title: string;
  relative_path: string;
  status: CroquiStatus;
  schema_version: string;
  last_export_relative_path: string | null;
  /** Ausente em croquis antigos → "viario". */
  kind: CroquiKind;
  created_at: string;
  updated_at: string;
}

export interface CroquiDocPayload {
  croqui: Croqui;
  /** JSON opaco; o Croqui Engine valida a forma. */
  doc: unknown;
}

export interface NewCroquiInput {
  title: string;
  /** Ausente → "viario". */
  kind?: CroquiKind;
}

export interface ExportCroquiPngInput {
  /** PNG em base64, sem o prefixo `data:`. */
  png_base64: string;
}

// ----- Importação de drone -----

export interface CropRectInput {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface DroneImportInput {
  source_absolute_path: string;
  /** 0..=1; 0 desliga a correção de lente. */
  intensity: number;
  /** Aplicado DEPOIS da correção de lente. */
  crop: CropRectInput;
  /** Rastreabilidade gravada no sidecar. */
  croqui_id?: string;
  occurrence_id?: string;
}

export interface DroneImportResult {
  output_relative_path: string;
  sidecar_relative_path: string;
  output_width: number;
  output_height: number;
  output_hash_sha256: string;
}
