/**
 * Mirror of `src-tauri/src/models/image_analysis.rs` (MVP 7).
 * Field names em snake_case por compatibilidade com serde.
 */

export type ImageSourceKind =
  | "photo"
  | "video_frame"
  | "evidence"
  | "local_import";

export interface ImageAnalysis {
  id: string;
  occurrence_id: string;
  title: string;
  source_kind: ImageSourceKind;
  source_id: string | null;
  original_relative_path: string;
  original_hash_sha256: string | null;
  analysis_relative_path: string;
  last_export_relative_path: string | null;
  status: string;
  metadata_json: string;
  created_at: string;
  updated_at: string;
}

export interface ImageExport {
  id: string;
  occurrence_id: string;
  image_analysis_id: string;
  output_relative_path: string;
  sidecar_relative_path: string | null;
  hash_sha256: string | null;
  width: number | null;
  height: number | null;
  format: string;
  created_at: string;
  operation_summary_json: string;
}

export interface ImageOperationLog {
  id: number;
  occurrence_id: string;
  image_analysis_id: string;
  action: string;
  details_json: string;
  created_at: string;
}

export interface CreateImageAnalysisInput {
  title: string;
  source_kind: ImageSourceKind;
  source_id?: string | null;
  original_relative_path: string;
  original_hash_sha256?: string | null;
}

export interface ImportLocalImageInput {
  source_path: string;
  title?: string;
}

export interface BackendAdjustments {
  brightness: number;
  contrast: number;
  gamma: number;
  saturation: number;
  grayscale: boolean;
  invert: boolean;
  /** W14.2 — Matiz (rotação de cor, graus). 0 = neutro. Matriz hueRotate
   * do SVG/CSS (preservando luminância) — replicada no backend. */
  hue?: number;
  /** W14.2 — visibilidade de canal (GIMP-style). false zera o canal de
   * saída. Default true. */
  channel_r?: boolean;
  channel_g?: boolean;
  channel_b?: boolean;
}

export type BackendOperation =
  // Geometric (MVP 7)
  | { kind: "rotate_90_cw" }
  | { kind: "rotate_90_ccw" }
  | { kind: "rotate_180" }
  | { kind: "flip_horizontal" }
  | { kind: "flip_vertical" }
  | { kind: "crop"; x: number; y: number; width: number; height: number }
  | { kind: "resize"; width: number; height: number }
  // G12.1 — Edge detection
  | { kind: "edge_sobel"; strength?: number }
  | { kind: "edge_laplacian"; strength?: number }
  | { kind: "edge_canny"; low_threshold?: number; high_threshold?: number }
  // G12.2 — Blur / denoise
  | { kind: "blur_gaussian"; sigma: number }
  | { kind: "blur_median"; radius: number }
  | { kind: "blur_bilateral"; sigma_space: number; sigma_color: number }
  // G12.3 — Enhancement
  | { kind: "clahe"; tile_size?: number; clip_limit?: number }
  | { kind: "histogram_equalize" }
  | { kind: "auto_levels"; percentile_low?: number; percentile_high?: number }
  | { kind: "white_balance_gray_world" }
  // G12.4 — Morphology
  | { kind: "dilate"; radius?: number }
  | { kind: "erode"; radius?: number }
  | { kind: "open"; radius?: number }
  | { kind: "close"; radius?: number }
  // G12.6 — Perspective
  | {
      kind: "perspective";
      src: [[number, number], [number, number], [number, number], [number, number]];
      dst: [[number, number], [number, number], [number, number], [number, number]];
      output_width: number;
      output_height: number;
    }
  // G12 — Misc
  | { kind: "unsharp_mask"; sigma: number; amount?: number }
  | { kind: "threshold"; value: number }
  | {
      kind: "pixelize";
      x: number;
      y: number;
      width: number;
      height: number;
      block_size: number;
    };

export interface ExportImageInput {
  apply_backend_adjustments?: boolean;
  composed_png_base64?: string | null;
  adjustments?: BackendAdjustments | null;
  operations?: BackendOperation[];
  format?: "png" | "jpg";
  operation_summary_json?: string | null;
}

export interface SaveImageAnalysisInput {
  doc: unknown;
  metadata_json?: string;
  title?: string;
}

export interface ImageAnalysisPayload {
  analysis: ImageAnalysis;
  doc: unknown;
}

export interface ImageMetadata {
  width: number;
  height: number;
  mime_type: string | null;
  format_label: string | null;
  size_bytes: number;
  hash_sha256: string | null;
  exif_json: string | null;
  /** G12.8 — Conjunto completo de hashes para chain of custody. */
  hash_set?: ImageHashSet | null;
}

/** G12.8 — Múltiplos hashes (chain of custody pericial). */
export interface ImageHashSet {
  md5: string;
  sha1: string;
  sha256: string;
  sha3_256: string;
}

/** G12.9 — Histograma com 256 bins por canal + estatísticas. */
export interface ImageHistogram {
  red: number[];
  green: number[];
  blue: number[];
  luminance: number[];
  stats: HistogramStats;
}

export interface HistogramStats {
  mean_r: number;
  mean_g: number;
  mean_b: number;
  mean_lum: number;
  stddev_r: number;
  stddev_g: number;
  stddev_b: number;
  stddev_lum: number;
  min_lum: number;
  max_lum: number;
  total_pixels: number;
}

/** G12 — Preview de uma operação (sem persistir). */
export interface ApplyOperationPreviewInput {
  image_base64: string;
  operation: BackendOperation;
}

export interface ApplyOperationPreviewResult {
  image_base64: string;
  width: number;
  height: number;
}

/** G12 — Aplica pilha de ops na imagem original. */
export interface ApplyOperationStackInput {
  relative_path: string;
  adjustments?: BackendAdjustments | null;
  operations?: BackendOperation[];
  as_jpeg?: boolean;
}

export interface ApplyOperationStackResult {
  image_base64: string;
  mime: string;
  width: number;
  height: number;
}

/** G12.21 — Artifact do relatório HTML gerado. */
export interface ImageAnalysisReportArtifact {
  html: string;
  output_relative_path: string;
}

export interface ImageAssetBytes {
  relative_path: string;
  mime_type: string;
  base64: string;
  size_bytes: number;
}
