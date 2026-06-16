/**
 * `.sicroimage` serializer — coerce arbitrary JSON into a `SicroImageDoc`
 * with safe defaults. Mirror do padrão usado em `croqui/engine/serializer.ts`
 * e `laudo/document-engine/serializer.ts`.
 */

import type { BackendAdjustments } from "@domain/image_analysis";
import {
  CURRENT_SCHEMA_VERSION,
  type ProcessingOp,
  type SicroImageCanvas,
  type SicroImageDoc,
  type SicroImageLayer,
  type SicroImagePoint,
  type SicroImageSelection,
  type SicroImageSource,
  type SicroAnnotation,
  type SicroImageScale,
} from "./schema";

const DEFAULT_CANVAS: SicroImageCanvas = {
  zoom: 1,
  pan_x: 0,
  pan_y: 0,
  rotation: 0,
  background_color: "#1f2933",
};

const DEFAULT_ADJUSTMENTS: BackendAdjustments = {
  brightness: 0,
  contrast: 0,
  gamma: 1,
  saturation: 0,
  grayscale: false,
  invert: false,
  hue: 0,
  channel_r: true,
  channel_g: true,
  channel_b: true,
};

const DEFAULT_LAYERS: SicroImageLayer[] = [
  {
    id: "layer_base",
    name: "Imagem base",
    kind: "image_base",
    visible: true,
    locked: true,
    opacity: 1,
  },
  {
    id: "layer_annotations",
    name: "Anotações",
    kind: "annotations",
    visible: true,
    locked: false,
    opacity: 1,
  },
];

export function coerceSicroImage(raw: unknown): SicroImageDoc {
  if (!raw || typeof raw !== "object") {
    throw new Error("invalid .sicroimage: not an object");
  }
  const o = raw as Record<string, unknown>;
  const id = stringField(o, "image_analysis_id");
  const occ = stringField(o, "occurrence_id");
  if (!id || !occ) {
    throw new Error(
      "invalid .sicroimage: missing image_analysis_id / occurrence_id",
    );
  }

  const source = coerceSource(o.source);
  const canvas = coerceCanvas(o.canvas);
  const view_adjustments = coerceAdjustments(o.view_adjustments);
  const coercedLayers = Array.isArray(o.layers)
    ? (o.layers as unknown[])
        .map(coerceLayer)
        .filter((l): l is SicroImageLayer => l !== null)
    : [];
  const layers = coercedLayers.length > 0 ? coercedLayers : DEFAULT_LAYERS;
  const annotations = Array.isArray(o.annotations)
    ? (o.annotations as SicroAnnotation[])
    : [];
  const measurements = Array.isArray(o.measurements)
    ? (o.measurements as SicroAnnotation[])
    : [];
  const scale = coerceScale(o.scale);
  const selection = coerceSelection(o.selection);
  const exports = Array.isArray(o.exports) ? o.exports : [];
  // G12.10 — processing_stack agora é tipado. Pré-G12 docs vinham com
  // unknown[]; mantemos compat coercando shape mínima.
  const processing_stack = Array.isArray(o.processing_stack)
    ? (o.processing_stack as unknown[])
        .filter((op): op is Record<string, unknown> =>
          typeof op === "object" && op !== null,
        )
        .map((op) => coerceProcessingOp(op))
    : [];

  return {
    schema_version:
      stringField(o, "schema_version") ?? CURRENT_SCHEMA_VERSION,
    image_analysis_id: id,
    occurrence_id: occ,
    title: stringField(o, "title") ?? "Análise sem título",
    source,
    canvas,
    view_adjustments,
    processing_stack,
    layers,
    annotations,
    measurements,
    scale,
    selection,
    exports,
    created_at:
      stringField(o, "created_at") ?? new Date().toISOString(),
    updated_at:
      stringField(o, "updated_at") ?? new Date().toISOString(),
  };
}

export function serializeSicroImage(doc: SicroImageDoc): SicroImageDoc {
  return {
    ...doc,
    schema_version: CURRENT_SCHEMA_VERSION,
    updated_at: new Date().toISOString(),
  };
}

// ---------------------------------------------------------------------------

function coerceSource(raw: unknown): SicroImageSource {
  if (!raw || typeof raw !== "object") {
    return {
      kind: "local_import",
      source_id: null,
      original_relative_path: "",
      original_hash_sha256: null,
      mime_type: null,
      width: 0,
      height: 0,
      size_bytes: 0,
    };
  }
  const o = raw as Record<string, unknown>;
  return {
    kind: (stringField(o, "kind") as never) ?? "local_import",
    source_id: stringField(o, "source_id"),
    original_relative_path: stringField(o, "original_relative_path") ?? "",
    original_hash_sha256: stringField(o, "original_hash_sha256"),
    mime_type: stringField(o, "mime_type"),
    width: numberField(o, "width") ?? 0,
    height: numberField(o, "height") ?? 0,
    size_bytes: numberField(o, "size_bytes") ?? 0,
  };
}

function coerceCanvas(raw: unknown): SicroImageCanvas {
  if (!raw || typeof raw !== "object") return { ...DEFAULT_CANVAS };
  const o = raw as Record<string, unknown>;
  return {
    zoom: numberField(o, "zoom") ?? DEFAULT_CANVAS.zoom,
    pan_x: numberField(o, "pan_x") ?? 0,
    pan_y: numberField(o, "pan_y") ?? 0,
    rotation: numberField(o, "rotation") ?? 0,
    background_color:
      stringField(o, "background_color") ?? DEFAULT_CANVAS.background_color,
  };
}

function coerceAdjustments(raw: unknown): BackendAdjustments {
  if (!raw || typeof raw !== "object") return { ...DEFAULT_ADJUSTMENTS };
  const o = raw as Record<string, unknown>;
  return {
    brightness: numberField(o, "brightness") ?? 0,
    contrast: numberField(o, "contrast") ?? 0,
    gamma: numberField(o, "gamma") ?? 1,
    saturation: numberField(o, "saturation") ?? 0,
    grayscale: boolField(o, "grayscale"),
    invert: boolField(o, "invert"),
    // W14.2 — Matiz + canais. Canais default TRUE (visíveis) quando ausentes,
    // para docs antigos não "sumirem" com nenhum canal.
    hue: numberField(o, "hue") ?? 0,
    channel_r: boolFieldDefault(o, "channel_r", true),
    channel_g: boolFieldDefault(o, "channel_g", true),
    channel_b: boolFieldDefault(o, "channel_b", true),
  };
}

function coerceScale(raw: unknown): SicroImageScale | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const px = numberField(o, "px_per_unit");
  if (!px || px <= 0) return null;
  const unit = stringField(o, "unit") ?? "m";
  return {
    px_per_unit: px,
    unit: (unit === "cm" || unit === "mm" ? unit : "m") as SicroImageScale["unit"],
    calibrated_by: Array.isArray(o.calibrated_by)
      ? (o.calibrated_by as never)
      : [],
    calibration_real_distance: numberField(o, "calibration_real_distance") ?? 0,
    created_at: stringField(o, "created_at") ?? new Date().toISOString(),
  };
}

/** W20 — coerce a SicroImageSelection; devolve null se inválido/ausente.
 * rect/ellipse exigem bbox positivo; polygon exige 3+ pontos. */
function coerceSelection(raw: unknown): SicroImageSelection | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const kind = stringField(o, "kind");
  if (kind !== "rect" && kind !== "ellipse" && kind !== "polygon") return null;
  const base = {
    id:
      (typeof o.id === "string" && o.id) ||
      `sel-${Math.random().toString(36).slice(2, 10)}`,
    inverted: boolField(o, "inverted"),
    source_tool: stringField(o, "source_tool") ?? undefined,
    created_at: stringField(o, "created_at") ?? new Date().toISOString(),
  };
  if (kind === "polygon") {
    const points = coercePoints(o.points);
    if (points.length < 3) return null;
    return { ...base, kind, points };
  }
  const x = numberField(o, "x");
  const y = numberField(o, "y");
  const width = numberField(o, "width");
  const height = numberField(o, "height");
  if (x === null || y === null || width === null || height === null) return null;
  if (width <= 0 || height <= 0) return null;
  return { ...base, kind, x, y, width, height };
}

function coercePoints(raw: unknown): SicroImagePoint[] {
  if (!Array.isArray(raw)) return [];
  const out: SicroImagePoint[] = [];
  for (const p of raw) {
    if (p && typeof p === "object") {
      const po = p as Record<string, unknown>;
      const x = numberField(po, "x");
      const y = numberField(po, "y");
      if (x !== null && y !== null) out.push({ x, y });
    }
  }
  return out;
}

function stringField(o: Record<string, unknown>, key: string): string | null {
  const v = o[key];
  return typeof v === "string" && v.length > 0 ? v : null;
}
function numberField(o: Record<string, unknown>, key: string): number | null {
  const v = o[key];
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}
function boolField(o: Record<string, unknown>, key: string): boolean {
  const v = o[key];
  return typeof v === "boolean" ? v : false;
}
function boolFieldDefault(
  o: Record<string, unknown>,
  key: string,
  fallback: boolean,
): boolean {
  const v = o[key];
  return typeof v === "boolean" ? v : fallback;
}

/**
 * Coerce uma camada com defaults seguros. W20 (S3): camadas de pixels
 * (`kind="pixels"`) carregam offset/dims/caminho do bitmap; uma camada de
 * pixels sem bitmap ou sem dimensão é inválida e descartada (defensivo).
 */
function coerceLayer(raw: unknown): SicroImageLayer | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const id = typeof o.id === "string" && o.id ? o.id : null;
  if (!id) return null;
  const kind = (stringField(o, "kind") ??
    "annotations") as SicroImageLayer["kind"];
  const num = (v: unknown): number | undefined =>
    typeof v === "number" && Number.isFinite(v) ? v : undefined;
  const layer: SicroImageLayer = {
    id,
    name: stringField(o, "name") ?? "Camada",
    kind,
    visible: o.visible !== false,
    locked: o.locked === true,
    opacity:
      typeof o.opacity === "number"
        ? Math.max(0, Math.min(1, o.opacity))
        : 1,
  };
  if (kind === "pixels") {
    layer.offset_x = num(o.offset_x) ?? 0;
    layer.offset_y = num(o.offset_y) ?? 0;
    layer.width = num(o.width) ?? 0;
    layer.height = num(o.height) ?? 0;
    layer.rotation = num(o.rotation) ?? 0;
    layer.bitmap_relative_path =
      stringField(o, "bitmap_relative_path") ?? undefined;
    layer.pixel_source =
      o.pixel_source === "processed" ? "processed" : "original";
    layer.hash_sha256 = stringField(o, "hash_sha256") ?? undefined;
    layer.created_at = stringField(o, "created_at") ?? undefined;
    if (!layer.bitmap_relative_path || !layer.width || !layer.height) {
      return null;
    }
  }
  return layer;
}

/** G12.10 — coerce a ProcessingOp with safe defaults. */
function coerceProcessingOp(o: Record<string, unknown>): ProcessingOp {
  return {
    id:
      (typeof o.id === "string" && o.id) ||
      `op-${Math.random().toString(36).slice(2, 10)}`,
    kind: (stringField(o, "kind") as ProcessingOp["kind"]) ?? "blur_gaussian",
    enabled: typeof o.enabled === "boolean" ? o.enabled : true,
    params:
      typeof o.params === "object" && o.params !== null
        ? (o.params as Record<string, unknown>)
        : {},
    notes: stringField(o, "notes") ?? undefined,
    // W20 (S2) — escopo + máscara congelada (defensivo: docs antigos não têm).
    scope: o.scope === "selection" ? "selection" : "image",
    mask: o.scope === "selection" ? coerceSelection(o.mask) : null,
    created_at: stringField(o, "created_at") ?? new Date().toISOString(),
  };
}
