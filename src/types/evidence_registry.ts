/**
 * Mirror of `src-tauri/src/models/registry.rs` (MVP 5 — Central de
 * Evidências). Field names stay snake_case so serde defaults line up.
 */

export type EvidenceKind =
  | "photo"
  | "croqui"
  | "croqui_export"
  | "video"
  | "video_frame"
  | "storyboard_frame"
  | "laudo"
  | "laudo_export"
  | "imported_package"
  | "audio"
  | "image_analysis"
  | "image_export"
  | "document"
  | "other";

export type IntegrityStatus =
  | "ok"
  | "missing_file"
  | "hash_mismatch"
  | "missing_sidecar"
  | "broken_link"
  | "unsafe_path"
  | "unknown";

export interface EvidenceRegistryItem {
  /** Synthetic id "<kind>:<repo-uuid>". Stable per workspace. */
  id: string;
  occurrence_id: string;
  kind: EvidenceKind;
  subtype: string | null;
  title: string | null;
  description: string | null;
  source_module: string;
  original_id: string | null;
  relative_path: string | null;
  sidecar_relative_path: string | null;
  hash_sha256: string | null;
  size_bytes: number | null;
  mime_type: string | null;
  created_at: string | null;
  updated_at: string | null;
  status: string | null;
  integrity_status: IntegrityStatus;
  integrity_detail: string | null;
  linked_laudos_count: number;
  metadata_json: string;
}

export interface RegistrySummary {
  photos: number;
  croquis: number;
  croqui_exports: number;
  videos: number;
  video_frames: number;
  storyboard_frames: number;
  laudos: number;
  laudo_exports: number;
  imported_packages: number;
  total_items: number;
  items_with_relative_path: number;
  linked_in_laudos: number;
  files_ok: number;
  files_missing: number;
  unsafe_paths: number;
  broken_links: number;
  hash_mismatches: number;
  /** "ok" | "warning" | "critical" */
  overall_status: string;
}

export interface BrokenLaudoLink {
  laudo_id: string;
  laudo_title: string;
  node_type: string;
  relative_path: string | null;
  status: IntegrityStatus;
  detail: string | null;
}

export interface WorkspaceIntegrityReport {
  occurrence_id: string;
  workspace_path: string;
  generated_at: string;
  app_version: string;
  summary: RegistrySummary;
  items: EvidenceRegistryItem[];
  broken_laudo_links: BrokenLaudoLink[];
  warnings: string[];
  deep_check_executed: boolean;
}

export interface IntegrityReportArtifact {
  relative_path: string;
  generated_at: string;
  overall_status: string;
  item_count: number;
}

export interface VerifyOptions {
  /** When true, recomputes SHA-256 for items that store a hash. */
  deep?: boolean;
}
