/**
 * Mirror of `src-tauri/src/models/dossie.rs` (MVP 3 — Dossiê Operacional).
 * Field names stay snake_case so serde's default render matches the wire.
 *
 * Each row has a `raw_json` string preserving the verbatim mobile payload
 * for forward-compat. The structured columns are what the UI uses.
 */

import type { Import } from "./import";
import type { Occurrence } from "./occurrence";

export interface ChecklistItem {
  id: string;
  occurrence_id: string;
  import_id: string;
  original_id: string | null;
  category: string | null;
  question: string;
  required: boolean;
  /** `sim` | `nao` | `nao_se_aplica` | `nao_verificado` */
  answer: string;
  note: string | null;
  default_note: string | null;
  /** `base` | `adicionado` */
  origin: string;
  sort_order: number;
  raw_json: string;
  created_at: string;
}

export interface ChecklistSummary {
  total: number;
  answered: number;
  not_verified: number;
  not_applicable: number;
  required_total: number;
  required_pending: number;
}

export type EntityType = "vehicle" | "victim";

export interface Entity {
  id: string;
  occurrence_id: string;
  import_id: string;
  original_id: string | null;
  type: EntityType;
  identifier: string | null;
  label: string | null;
  summary: string | null;
  /** JSON-encoded array of strings. */
  photo_ids_json: string;
  raw_json: string;
  sort_order: number;
  created_at: string;
}

export interface Trace {
  id: string;
  occurrence_id: string;
  import_id: string;
  original_id: string | null;
  identifier: string | null;
  type: string | null;
  description: string | null;
  location_description: string | null;
  length: number | null;
  width: number | null;
  unit: string | null;
  direction: string | null;
  note: string | null;
  photo_ids_json: string;
  sketch_element_ids_json: string;
  raw_json: string;
  sort_order: number;
  created_at: string;
}

export interface Measurement {
  id: string;
  occurrence_id: string;
  import_id: string;
  original_id: string | null;
  label: string | null;
  point_a: string | null;
  point_b: string | null;
  value: number | null;
  unit: string | null;
  method: string | null;
  note: string | null;
  photo_ids_json: string;
  sketch_element_ids_json: string;
  raw_json: string;
  sort_order: number;
  created_at: string;
}

export interface FieldNote {
  id: string;
  occurrence_id: string;
  import_id: string;
  original_id: string | null;
  text: string | null;
  category: string | null;
  priority: string | null;
  note_created_at: string | null;
  note_updated_at: string | null;
  raw_json: string;
  sort_order: number;
  created_at: string;
}

export interface TimelineEvent {
  id: string;
  occurrence_id: string;
  import_id: string;
  original_id: string | null;
  type: string | null;
  title: string | null;
  description: string | null;
  occurred_at: string | null;
  raw_json: string;
  sort_order: number;
  created_at: string;
}

export interface OccurrenceStats {
  id: string;
  occurrence_id: string;
  import_id: string;
  duration_seconds: number | null;
  photos_count: number | null;
  victims_count: number | null;
  vehicles_count: number | null;
  traces_count: number | null;
  measurements_count: number | null;
  notes_count: number | null;
  checklist_items_count: number | null;
  answered_checklist_items_count: number | null;
  not_applicable_items_count: number | null;
  best_gps_accuracy_m: number | null;
  gps_readings_count: number | null;
  raw_json: string;
  created_at: string;
}

export interface DossieCounts {
  photos: number;
  vehicles: number;
  victims: number;
  traces: number;
  measurements: number;
  notes: number;
  timeline: number;
  checklist: ChecklistSummary;
}

export interface DossieSummary {
  occurrence: Occurrence;
  latest_import: Import | null;
  stats: OccurrenceStats | null;
  counts: DossieCounts;
}

export interface RehydrateOutcome {
  rehydrated: boolean;
  from_package_path: string | null;
  checklist_loaded: number;
  entities_loaded: number;
  traces_loaded: number;
  measurements_loaded: number;
  notes_loaded: number;
  timeline_loaded: number;
  stats_loaded: boolean;
  warnings: string[];
}
