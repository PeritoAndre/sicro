/**
 * Mirror of `src-tauri/src/models/video.rs` (Spike F — Video Engine).
 * Field names stay snake_case so serde defaults line up with the wire.
 */

export type VideoEventCategory =
  | "colisao"
  | "frenagem"
  | "impacto"
  | "reacao"
  | "semaforo"
  | "mudanca_faixa"
  | "outro";

export interface VideoMedia {
  id: string;
  occurrence_id: string;
  original_path: string | null;
  /** Workspace-relative path (videos/originais/<filename>). */
  relative_path: string;
  filename: string;
  sha256: string;
  size_bytes: number;
  duration_s: number | null;
  codec: string | null;
  width: number | null;
  height: number | null;
  pixel_format: string | null;
  fps_declared: number | null;
  /** "30000/1001" — kept as the string ffprobe returned. */
  avg_frame_rate: string | null;
  r_frame_rate: string | null;
  time_base: string | null;
  frame_count: number | null;
  bitrate: number | null;
  /** ffprobe -of json verbatim. */
  raw_probe_json: string;
  /** JSON array of strings — technical warnings. */
  warnings_json: string;
  created_at: string;
  updated_at: string;
}

export interface VideoEvent {
  id: string;
  occurrence_id: string;
  media_hash: string;
  timestamp_s: number;
  timestamp_label: string;
  frame_observed: number | null;
  pts: number | null;
  time_base: string | null;
  category: string;
  title: string;
  description: string;
  reviewed: boolean;
  source: string;
  created_at: string;
  updated_at: string;
}

export interface VideoExport {
  id: string;
  occurrence_id: string;
  media_hash: string;
  event_id: string | null;
  type: string;
  requested_timestamp_s: number;
  actual_timestamp_s: number | null;
  delta_s: number | null;
  output_path: string;
  filename: string;
  sidecar_json_path: string | null;
  details_json: string;
  created_at: string;
}

export interface VideoStoryboardFrame {
  id: string;
  occurrence_id: string;
  media_hash: string;
  event_id: string | null;
  export_id: string | null;
  title: string;
  caption: string;
  notes: string;
  requested_timestamp_s: number;
  actual_timestamp_s: number | null;
  delta_s: number | null;
  observed_frame_index: number | null;
  estimated_total_frames: number | null;
  frame_index_is_estimated: boolean;
  pts: number | null;
  time_base: string | null;
  output_path: string;
  sidecar_json_path: string | null;
  reviewed: boolean;
  created_at: string;
  updated_at: string;
}

export interface VideoOperationLog {
  id: number;
  occurrence_id: string;
  media_hash: string | null;
  action: string;
  details_json: string;
  created_at: string;
}

export interface VideoBundle {
  media: VideoMedia;
  events: VideoEvent[];
  exports: VideoExport[];
  storyboard: VideoStoryboardFrame[];
}

export interface RegisterVideoInput {
  source_path: string;
}

export interface CreateVideoEventInput {
  media_hash: string;
  timestamp_s: number;
  category: VideoEventCategory | string;
  title: string;
  description?: string;
}

export interface UpdateVideoEventInput {
  title?: string;
  description?: string;
  category?: VideoEventCategory | string;
  timestamp_s?: number;
  reviewed?: boolean;
}

export interface CollectFrameInput {
  media_hash: string;
  timestamp_s: number;
  event_id?: string | null;
  title?: string;
  caption?: string;
  notes?: string;
}

export interface CollectFrameResult {
  export: VideoExport;
  storyboard_frame: VideoStoryboardFrame;
  warnings: string[];
}

export interface UpdateStoryboardFrameInput {
  title?: string;
  caption?: string;
  notes?: string;
  reviewed?: boolean;
}
