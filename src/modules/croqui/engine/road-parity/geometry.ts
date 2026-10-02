/**
 * Geometria pura do motor parity (sem Konva/React). Tudo em metros (mundo);
 * `pxPerM` só entra na projeção final para o canvas.
 */

import {
  PARITY_DEFAULT_PX_PER_M,
  PARITY_SIDEWALK_WIDTH_M,
  type SicroRoadObject_parity,
  type SicroRoundaboutObject_parity,
} from "./types";

export interface Vec2World {
  x: number;
  y: number;
}

// ---- Bezier ----

/** Amostra a Bezier cúbica da via em `n + 1` pontos (metros). 32 é leve e suave o bastante. */
export function sampleCubicBezier(
  road: SicroRoadObject_parity,
  n = 32,
): Vec2World[] {
  const out: Vec2World[] = [];
  for (let i = 0; i <= n; i++) {
    const t = i / n;
    const u = 1 - t;
    const b0 = u * u * u;
    const b1 = 3 * u * u * t;
    const b2 = 3 * u * t * t;
    const b3 = t * t * t;
    out.push({
      x: b0 * road.ax + b1 * road.cx1 + b2 * road.cx2 + b3 * road.bx,
      y: b0 * road.ay + b1 * road.cy1 + b2 * road.cy2 + b3 * road.by,
    });
  }
  return out;
}

// ---- Bordas e polígonos da via ----

/** Desloca cada amostra pela perpendicular local, ±`halfWidthM`, gerando as duas bordas. */
export function buildRoadEdges(
  samples: ReadonlyArray<Vec2World>,
  halfWidthM: number,
): { left: Vec2World[]; right: Vec2World[] } {
  const left: Vec2World[] = [];
  const right: Vec2World[] = [];
  const n = samples.length;
  if (n < 2) return { left, right };

  for (let i = 0; i < n; i++) {
    const p = samples[i] as Vec2World;
    let tx = 0;
    let ty = 0;
    if (i === 0) {
      const next = samples[1] as Vec2World;
      tx = next.x - p.x;
      ty = next.y - p.y;
    } else if (i === n - 1) {
      const prev = samples[i - 1] as Vec2World;
      tx = p.x - prev.x;
      ty = p.y - prev.y;
    } else {
      const next = samples[i + 1] as Vec2World;
      const prev = samples[i - 1] as Vec2World;
      tx = next.x - prev.x;
      ty = next.y - prev.y;
    }
    const len = Math.hypot(tx, ty) || 1;
    const nx = (-ty / len) * halfWidthM;
    const ny = (tx / len) * halfWidthM;
    left.push({ x: p.x + nx, y: p.y + ny });
    right.push({ x: p.x - nx, y: p.y - ny });
  }
  return { left, right };
}

/** Polígono fechado da pista: borda esquerda + borda direita invertida. */
export function buildRoadRibbon(
  samples: ReadonlyArray<Vec2World>,
  halfWidthM: number,
): Vec2World[] {
  const { left, right } = buildRoadEdges(samples, halfWidthM);
  return [...left, ...right.slice().reverse()];
}

/** Polígono da calçada: ribbon com `extraM` a mais na meia-largura. */
export function buildRoadSidewalk(
  samples: ReadonlyArray<Vec2World>,
  halfWidthM: number,
  extraM: number = PARITY_SIDEWALK_WIDTH_M,
): Vec2World[] {
  return buildRoadRibbon(samples, halfWidthM + extraM);
}

// ---- Rotatória ----

/** Raios da rotatória já em px de canvas (sem offset — o caller soma offset_x/y). */
interface RoundaboutRingsPx {
  cx_px: number;
  cy_px: number;
  sidewalk_r_px: number;
  outer_r_px: number;
  inner_r_px: number; // pode ser 0 se ilha some
  /** Disco do asfalto em metros, para clipping de marcações. */
  outer_r_m: number;
}

export function buildRoundaboutRings(
  rb: SicroRoundaboutObject_parity,
  pxPerM: number,
): RoundaboutRingsPx {
  const safePx = Math.max(pxPerM, 0.0001);
  const halfLargM = rb.largura_m / 2;
  const inner_r_m = Math.max(0, rb.r_m - halfLargM);
  return {
    cx_px: rb.cx * safePx,
    cy_px: rb.cy * safePx,
    sidewalk_r_px:
      (rb.r_m + halfLargM + PARITY_SIDEWALK_WIDTH_M) * safePx,
    outer_r_px: (rb.r_m + halfLargM) * safePx,
    inner_r_px: inner_r_m * safePx,
    outer_r_m: rb.r_m + halfLargM,
  };
}

/** Disco da rotatória como polígono (metros) — obstáculo para clipping de marcações. */
export function buildRoundaboutDiskPolygon(
  rb: SicroRoundaboutObject_parity,
  segments = 48,
  paddingM = 0,
): Vec2World[] {
  const r = rb.r_m + rb.largura_m / 2 + paddingM;
  const out: Vec2World[] = [];
  for (let i = 0; i < segments; i++) {
    const t = (i / segments) * Math.PI * 2;
    out.push({ x: rb.cx + r * Math.cos(t), y: rb.cy + r * Math.sin(t) });
  }
  return out;
}

// ---- Mundo → canvas ----

/** `pxPerM` efetivo; nulo ou inválido cai no default. */
export function resolvePxPerM(scalePxPerM: number | null | undefined): number {
  if (typeof scalePxPerM !== "number") return PARITY_DEFAULT_PX_PER_M;
  if (!Number.isFinite(scalePxPerM) || scalePxPerM <= 0) {
    return PARITY_DEFAULT_PX_PER_M;
  }
  return scalePxPerM;
}

export function projectWorldPoints(
  worldPts: ReadonlyArray<Vec2World>,
  pxPerM: number,
  offsetX: number,
  offsetY: number,
): Vec2World[] {
  const scale = Math.max(pxPerM, 0.0001);
  return worldPts.map((p) => ({
    x: p.x * scale + offsetX,
    y: p.y * scale + offsetY,
  }));
}

/** Formato de `Konva.Line.points`: `[x1, y1, x2, y2, ...]`. */
export function flattenVec2(pts: ReadonlyArray<Vec2World>): number[] {
  const out: number[] = [];
  for (const p of pts) {
    out.push(p.x, p.y);
  }
  return out;
}

/**
 * Círculo/arco como polilinha; `endAngle - startAngle` em [0, 2π] (2π fecha o loop).
 * 96 segmentos dão ~1 px de erro radial em raios urbanos.
 */
export function discretizeCircle(
  cx: number,
  cy: number,
  r: number,
  startAngle = 0,
  endAngle = Math.PI * 2,
  segments = 96,
): Vec2World[] {
  const out: Vec2World[] = [];
  const span = endAngle - startAngle;
  for (let i = 0; i <= segments; i++) {
    const t = i / segments;
    const a = startAngle + span * t;
    out.push({ x: cx + r * Math.cos(a), y: cy + r * Math.sin(a) });
  }
  return out;
}
