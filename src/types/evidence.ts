/**
 * Mirror of `src-tauri/src/models/evidence.rs` (MVP 4).
 */

export type EvidenceSourceKind =
  | "photo"
  | "croqui"
  | "video_frame"
  | "video_storyboard"
  | "occurrence_field"
  | "checklist_table"
  | "traces_table"
  | "measurements_table"
  | "field_note";

export interface EvidenceLink {
  id: string;
  occurrence_id: string;
  target_type: string;
  target_id: string;
  relation_type: string;
  source_kind: EvidenceSourceKind;
  media_asset_id: string | null;
  croqui_id: string | null;
  video_media_hash: string | null;
  video_event_id: string | null;
  video_storyboard_frame_id: string | null;
  field_note_id: string | null;
  relative_path: string | null;
  source_hash: string | null;
  /** JSON object preserved verbatim. */
  metadata_json: string;
  created_at: string;
}

export interface RecordEvidenceLinkInput {
  target_type: string;
  target_id: string;
  source_kind: EvidenceSourceKind;
  relation_type?: string;
  media_asset_id?: string | null;
  croqui_id?: string | null;
  video_media_hash?: string | null;
  video_event_id?: string | null;
  video_storyboard_frame_id?: string | null;
  field_note_id?: string | null;
  relative_path?: string | null;
  source_hash?: string | null;
  metadata_json?: string;
}

export interface EvidenceAsset {
  relative_path: string;
  mime_type: string;
  /** Base64-encoded bytes (no `data:` prefix). */
  base64: string;
  size_bytes: number;
}
