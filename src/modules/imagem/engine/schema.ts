/**
 * `.sicroimage` schema — Editor de Imagem Pericial (MVP 7).
 *
 * O `.sicroimage` é a fonte da verdade da sessão de análise. PNG/JPG
 * derivados são exportações.
 *
 * Compatibilidade: aditiva. Campos novos no futuro entram com `?`
 * (opcionais) e default no `coerceSicroImage`.
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
 * W20 — Seleção de região (estilo Photoshop). Define um ROI usado para
 * mascarar operações (filtros/desenho só dentro — fase S2), inverter, e
 * copiar/colar como nova camada (S3). Geometria em px da imagem original
 * (mesma convenção das anotações).
 * - `rect`/`ellipse`: bounding box (x, y, width, height).
 * - `polygon`: contorno fechado (3+ pontos) — gerado pelo laço, poligonal
 *   ou magnética.
 * `inverted = true` → a seleção efetiva é o COMPLEMENTO da geometria.
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
  // W20 (S3) — camada de pixels: recorte de uma seleção, movível.
  | "pixels";

/** W20 (S3) — origem dos pixels de uma camada recortada. */
export type PixelLayerSource = "original" | "processed";

export interface SicroImageLayer {
  id: string;
  name: string;
  kind: SicroImageLayerKind;
  visible: boolean;
  locked: boolean;
  opacity: number;
  // ----- W20 (S3) — campos da camada de pixels (só quando kind="pixels") -----
  /** Deslocamento da camada em px da imagem (canto sup-esq). */
  offset_x?: number;
  offset_y?: number;
  /** Dimensões do recorte em px. */
  width?: number;
  height?: number;
  /** W20 (S3) — rotação da camada em graus (pivô = canto sup-esq, igual ao
   *  Konva e ao composite do export). 0 = sem rotação. */
  rotation?: number;
  /** Caminho relativo (workspace) do PNG recortado — o `.sicroimage` só
   *  referencia (igual originais/derivados; nada de bitmap embutido). */
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
  // G12.14 — Anotações novas no schema 0.2
  | "polygon"
  | "angle"
  | "freehand";

export interface SicroAnnotation {
  id: string;
  layer_id: string;
  kind: SicroAnnotationKind;
  /** For shapes: top-left or center; for measurement: p1; for text: anchor. */
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
  /**
   * G12 — Lista de pontos para polygon/freehand/angle. Coordenadas
   * absolutas em px da imagem original.
   * - polygon: 3+ pontos formando contorno fechado (área + perímetro).
   * - angle: exatamente 3 pontos (vértice é o do meio).
   * - freehand: N pontos amostrados do mouse drag.
   */
  points?: SicroImagePoint[];
  /**
   * G12 — Pré-computado pelo frontend quando há `scale` calibrada.
   * Usado para evitar recálculo a cada render.
   */
  measured_value?: {
    /** "distance_m" / "area_m2" / "angle_deg" / "perimeter_m" */
    kind: string;
    value: number;
    /** unidade (m, m², °) — display only */
    unit: string;
  };
}

/**
 * G12.10 — Pipeline de processamento NÃO destrutivo.
 *
 * Cada `ProcessingOp` representa uma operação na pilha:
 *   - `enabled = false` mantém na história mas não aplica.
 *   - reordenável via drag.
 *   - parâmetros editáveis a qualquer momento.
 *
 * O backend é chamado com o array desabilitados-filtrados quando
 * o usuário pede preview ou export.
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
  // W12 (paridade GIMP) — tonais / canais / forense / genéricas
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

/** W20 (S2) — escopo de aplicação de uma operação. */
export type ProcessingOpScope = "image" | "selection";

export interface ProcessingOp {
  id: string;
  kind: ProcessingOpKind;
  enabled: boolean;
  /** Parâmetros específicos da operação (sigma, threshold, radius, etc.). */
  params: Record<string, unknown>;
  /** Comentário do perito sobre por que aplicou (audit). */
  notes?: string;
  /**
   * W20 (S2) — escopo: "image" (default) aplica na imagem inteira;
   * "selection" confina o efeito à região de `mask` (estilo Photoshop).
   */
  scope?: ProcessingOpScope;
  /**
   * W20 (S2) — geometria da seleção CONGELADA no momento em que a operação
   * foi adicionada com escopo "selection" (coords em px da imagem). Manter a
   * máscara por-operação preserva a reprodutibilidade: o filtro continua
   * confinado à mesma região mesmo se o perito deselecionar/alterar a seleção
   * depois. Ausente/null quando scope = "image".
   */
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
  /** G12.10 — pilha de operações não-destrutivas (filtros forenses). */
  processing_stack: ProcessingOp[];
  layers: SicroImageLayer[];
  annotations: SicroAnnotation[];
  measurements: SicroAnnotation[];
  scale: SicroImageScale | null;
  /** W20 — seleção de região ativa (ROI estilo Photoshop). Aditivo, v0.3. */
  selection?: SicroImageSelection | null;
  exports: unknown[]; // populated by backend on read; UI keeps last hint
  created_at: string;
  updated_at: string;
}
