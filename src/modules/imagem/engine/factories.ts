/**
 * Annotation factories — MVP 7.
 *
 * Cada função produz um `SicroAnnotation` com defaults sãos. As cores
 * técnicas escolhidas (vermelho/amarelo/preto) são as típicas de
 * marcação pericial top-down.
 */

import type {
  SicroAnnotation,
  SicroAnnotationKind,
  SicroImagePoint,
} from "./schema";

const ANNOTATIONS_LAYER = "layer_annotations";

function uid(prefix: string): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `${prefix}_${crypto.randomUUID()}`;
  }
  return `${prefix}_${Date.now().toString(36)}_${Math.random()
    .toString(36)
    .slice(2, 8)}`;
}

function nowIso(): string {
  return new Date().toISOString();
}

function base(kind: SicroAnnotationKind, x: number, y: number): SicroAnnotation {
  return {
    id: uid(kind),
    layer_id: ANNOTATIONS_LAYER,
    kind,
    x,
    y,
    stroke: "#ef4444",
    stroke_width: 2,
    opacity: 1,
    visible: true,
    locked: false,
    created_at: nowIso(),
  };
}

export function makeArrow(x1: number, y1: number, x2: number, y2: number): SicroAnnotation {
  return { ...base("arrow", x1, y1), x2, y2 };
}
export function makeLine(x1: number, y1: number, x2: number, y2: number): SicroAnnotation {
  return { ...base("line", x1, y1), x2, y2 };
}
export function makeRect(x: number, y: number, w: number, h: number): SicroAnnotation {
  return { ...base("rect", x, y), width: w, height: h, fill: "rgba(239,68,68,0.0)" };
}
export function makeEllipse(x: number, y: number, w: number, h: number): SicroAnnotation {
  return { ...base("ellipse", x, y), width: w, height: h };
}
export function makeText(x: number, y: number, text: string): SicroAnnotation {
  return {
    ...base("text", x, y),
    text,
    stroke_width: 1,
    stroke: "#facc15",
    fill: "#facc15",
  };
}
export function makeNumberedMarker(
  x: number,
  y: number,
  number: number,
): SicroAnnotation {
  return {
    ...base("numbered_marker", x, y),
    number,
    text: String(number),
    fill: "#dc2626",
    stroke: "#ffffff",
  };
}
export function makePoint(x: number, y: number): SicroAnnotation {
  return { ...base("point", x, y), fill: "#22c55e" };
}
export function makeMeasurement(
  x1: number,
  y1: number,
  x2: number,
  y2: number,
): SicroAnnotation {
  return {
    ...base("measurement", x1, y1),
    x2,
    y2,
    stroke: "#0ea5e9",
    stroke_width: 2,
  };
}
export function makeRedaction(
  x: number,
  y: number,
  w: number,
  h: number,
): SicroAnnotation {
  return {
    ...base("redaction", x, y),
    width: w,
    height: h,
    fill: "#000000",
    stroke: "#000000",
    opacity: 1,
  };
}

// ---------------------------------------------------------------------------
// G12.14 — Novos kinds: polygon, angle, freehand

/**
 * Polígono fechado (área + perímetro). Precisa ≥3 pontos.
 * O ponto de ancoragem `(x, y)` é o primeiro vértice.
 */
export function makePolygon(points: SicroImagePoint[]): SicroAnnotation {
  const first = points[0] ?? { x: 0, y: 0 };
  return {
    ...base("polygon", first.x, first.y),
    points,
    stroke: "#fb923c",
    fill: "rgba(251, 146, 60, 0.10)",
    stroke_width: 2,
  };
}

/**
 * Medida de ângulo formada por 3 pontos. O vértice é o do meio
 * (`points[1]`); os outros dois definem os raios.
 */
export function makeAngle(
  p1: SicroImagePoint,
  vertex: SicroImagePoint,
  p2: SicroImagePoint,
): SicroAnnotation {
  return {
    ...base("angle", vertex.x, vertex.y),
    points: [p1, vertex, p2],
    stroke: "#a855f7",
    stroke_width: 2,
  };
}

/**
 * Desenho à mão livre — amostra de pontos do mouse drag.
 */
export function makeFreehand(points: SicroImagePoint[]): SicroAnnotation {
  const first = points[0] ?? { x: 0, y: 0 };
  return {
    ...base("freehand", first.x, first.y),
    points,
    stroke: "#facc15",
    stroke_width: 2,
  };
}

