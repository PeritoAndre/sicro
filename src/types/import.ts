/**
 * Mirror of Rust structs in src-tauri/src/models/import.rs (Spike D —
 * .sicroapp importer). Keep field names in snake_case so serde defaults
 * line up — the wire format flows straight to/from Tauri.
 */

import type { Occurrence } from "./occurrence";

export type ImportStatus =
  | "imported"
  | "imported_with_warnings"
  | "failed";

export interface Import {
  id: string;
  package_relative_path: string;
  original_filename: string | null;
  package_sha256: string;
  format: string;
  schema_version: string;
  app_name: string | null;
  app_version: string | null;
  mobile_occurrence_id: string | null;
  status: ImportStatus;
  /** Serialised JSON array of strings. Use JSON.parse if needed. */
  warnings_json: string;
  errors_json: string;
  raw_manifest_json: string;
  imported_at: string;
}

export type MediaAssetType = "photo";

export interface MediaAsset {
  id: string;
  import_id: string;
  occurrence_id: string;
  original_id: string | null;
  type: MediaAssetType;
  relative_path: string;
  original_package_path: string | null;
  original_filename: string | null;
  mime_type: string | null;
  size_bytes: number;
  sha256: string | null;
  captured_at: string | null;
  imported_at: string;
  category: string | null;
  caption: string | null;
  raw_json: string;
}

export interface EvidenceItem {
  id: string;
  occurrence_id: string;
  media_asset_id: string | null;
  type: string;
  title: string | null;
  description: string | null;
  source_module: string | null;
  captured_at: string | null;
  metadata_json: string;
  created_at: string;
}

export interface HashMismatch {
  path: string;
  expected: string;
  actual: string;
}

export interface ImportReport {
  import_id: string | null;
  occurrence_id: string | null;
  workspace_path: string | null;

  package_original_filename: string | null;
  package_sha256: string | null;
  package_size_bytes: number;

  format: string | null;
  schema_version: string | null;
  app_name: string | null;
  app_version: string | null;
  mobile_occurrence_id: string | null;
  generated_at: string | null;
  exported_at: string | null;

  tipo_pericia: string | null;
  natureza: string | null;
  resultado: string | null;
  bo: string | null;
  protocolo: string | null;
  municipio: string | null;
  bairro: string | null;
  logradouro: string | null;

  photos_declared: number;
  photos_imported: number;
  photos_missing: number;

  hashes_present: boolean;
  hashes_verified_ok: number;
  hashes_mismatched: HashMismatch[];
  files_missing_from_hashes: string[];

  jsons_read: string[];
  jsons_missing: string[];
  files_ignored: string[];

  warnings: string[];
  errors: string[];
  status: ImportStatus | null;

  manifest_counts: Record<string, unknown> | null;

  imported_at: string | null;
}

export interface ImportSicroappInput {
  /** Absolute path to the .sicroapp the user picked. */
  package_path: string;
  /** Optional parent directory for the new workspace. */
  parent_directory?: string | null;
}

/** Combined result of a successful import. */
export interface ImportResult {
  import: Import;
  occurrence: Occurrence;
  workspace_path: string;
  report: ImportReport;
}
