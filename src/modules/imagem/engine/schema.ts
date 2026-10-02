/**
 * Schema do `.sicroimage`, fonte da verdade da sessão de análise (PNG/JPG
 * derivados são exportações). Compatibilidade aditiva: campo novo entra com
 * `?` e default no `coerceSicroImage`.
 */

import type {
  BackendAdjustments,
  ImageSourceKind,
} from "@domain/image_analysis";

export const CURRENT_SCHEMA_VERSION = "0.3";

export interface SicroImagePoint {
  x: number;
  y: number;
}

export interface SicroImageCanvas {
  zoom: number;
  pan_x: number;
  pan_y: number;
  rotation: number;
  background_color: string;
}

export interface SicroImageSource {
  kind: ImageSourceKind;
  source_id: string | null;
  original_relative_path: string;
  original_hash_sha256: string | null;
  mime_type: string | null;
  width: number;
  height: number;
  size_bytes: number;
}

export interface SicroImageScale {
  px_per_unit: number;
  unit: "m" | "cm" | "mm";
  calibrated_by: SicroImagePoint[];
  calibration_real_distance: number;
  created_at: string;
}

/**
 * Seleção de região (ROI) em px da imagem original: `rect`/`ellipse` usam o
 * bounding box, `polygon` um contorno fechado (3+ pontos).
 * `inverted = true`: a seleção efetiva é o complemento da geometria.
 */
export type SicroSelectionKind = "rect" | "ellipse" | "polygon";

export interface SicroImageSelection {
  id: string;
  kind: SicroSelectionKind;
  /** rect/ellipse — bounding box em px da imagem. */
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  /** polygon — contorno fechado em px da imagem. */
  points?: SicroImagePoint[];
  /** Ferramenta que originou (informativo): laço, poligonal, magnética… */
  source_tool?: string;
  inverted: boolean;
  created_at: string;
}

export type SicroImageLayerKind =
  | "image_base"
  | "annotations"
  | "measurements"
  | "redactions"
  | "adjustments"
  // recorte de uma seleção, movível
  | "pixels";

/** Origem dos pixels de uma camada recortada. */
export type PixelLayerSource = "original" | "processed";

export interface SicroImageLayer {
  id: string;
  name: string;
  kind: SicroImageLayerKind;
  visible: boolean;
  locked: boolean;
  opacity: number;
  // ----- Campos da camada de pixels (só quando kind="pixels") -----
  /** Deslocamento da camada em px da imagem (canto sup-esq). */
  offset_x?: number;
  offset_y?: number;
  /** Dimensões do recorte em px. */
  width?: number;
  height?: number;
  /** Graus, pivô no canto sup-esq (igual ao Konva e ao composite do export). */
  rotation?: number;
  /** Caminho relativo (workspace) do PNG recortado: o `.sicroimage` só referencia, nunca embute bitmap. */
  bitmap_relative_path?: string;
  /** Origem do recorte: evidência fiel × resultado com filtros (custódia). */
  pixel_source?: PixelLayerSource;
  /** Hash do PNG recortado (integridade). */
  hash_sha256?: string;
  created_at?: string;
}

export type SicroAnnotationKind =
  | "arrow"
  | "line"
  | "rect"
  | "ellipse"
  | "text"
  | "numbered_marker"
  | "point"
  | "measurement"
  | "redaction"
  | "polygon"
  | "angle"
  | "freehand";

export interface SicroAnnotation {
  id: string;
  layer_id: string;
  kind: SicroAnnotationKind;
  /** Formas: canto sup-esq ou centro; measurement: p1; text: âncora. */
  x: number;
  y: number;
  /** rect/ellipse */
  width?: number;
  height?: number;
  /** arrow/line/measurement */
  x2?: number;
  y2?: number;
  /** text / numbered_marker */
  text?: string;
  /** numbered_marker */
  number?: number;
  rotation?: number;
  stroke?: string;
  fill?: string;
  stroke_width?: number;
  opacity?: number;
  label?: string;
  notes?: string;
  visible?: boolean;
  locked?: boolean;
  created_at: string;
  /** polygon (3+ pontos), angle (exatamente 3, vértice no meio), freehand (N). Px da imagem original. */
  points?: SicroImagePoint[];
  /** Pré-computado pelo frontend quando há `scale` calibrada, para não recalcular a cada render. */
  measured_value?: {
    /** "distance_m" / "area_m2" / "angle_deg" / "perimeter_m" */
    kind: string;
    value: number;
    /** unidade (m, m², °), só para exibição */
    unit: string;
  };
}

/**
 * Pipeline de processamento não destrutivo: `enabled = false` fica na
 * história sem aplicar; o backend recebe só as habilitadas no preview/export.
 */
export type ProcessingOpKind =
  | "edge_sobel"
  | "edge_laplacian"
  | "edge_canny"
  | "blur_gaussian"
  | "blur_median"
  | "blur_bilateral"
  | "clahe"
  | "subtract_background"
  | "histogram_equalize"
  | "auto_levels"
  | "white_balance_gray_world"
  | "dilate"
  | "erode"
  | "open"
  | "close"
  | "unsharp_mask"
  | "threshold"
  | "pixelize"
  | "perspective"
  // Geométricas
  | "rotate_90_cw"
  | "rotate_90_ccw"
  | "rotate_180"
  | "flip_horizontal"
  | "flip_vertical"
  | "crop"
  | "resize"
  // Tonais / canais / forense / genéricas
  | "levels"
  | "curves"
  | "posterize"
  | "extract_channel"
  | "false_color"
  | "ela"
  | "difference_of_gaussians"
  | "luminance_gradient"
  | "decorrelation_stretch"
  | "rotate_arbitrary"
  | "convolve";

/** Escopo de aplicação de uma operação. */
export type ProcessingOpScope = "image" | "selection";

export interface ProcessingOp {
  id: string;
  kind: ProcessingOpKind;
  enabled: boolean;
  /** Parâmetros específicos da operação (sigma, threshold, radius, etc.). */
  params: Record<string, unknown>;
  /** Comentário do perito sobre por que aplicou (audit). */
  notes?: string;
  /** "image" (default) aplica na imagem inteira; "selection" confina à região de `mask`. */
  scope?: ProcessingOpScope;
  /** Seleção congelada quando a op foi adicionada com escopo "selection".
   *  Guardar por operação preserva a reprodutibilidade mesmo se a seleção mudar depois.
   *  Null quando scope = "image". */
  mask?: SicroImageSelection | null;
  created_at: string;
}

export interface SicroImageDoc {
  schema_version: string;
  image_analysis_id: string;
  occurrence_id: string;
  title: string;
  source: SicroImageSource;
  canvas: SicroImageCanvas;
  view_adjustments: BackendAdjustments;
  /** Pilha de operações não-destrutivas (filtros forenses). */
  processing_stack: ProcessingOp[];
  layers: SicroImageLayer[];
  annotations: SicroAnnotation[];
  measurements: SicroAnnotation[];
  scale: SicroImageScale | null;
  /** Seleção de região ativa (ROI). */
  selection?: SicroImageSelection | null;
  exports: unknown[]; // preenchido pelo backend na leitura
  created_at: string;
  updated_at: string;
}
