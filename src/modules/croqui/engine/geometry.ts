/** Geometria pura do canvas (escala, medições, encaixe de imagem). Sem React/Konva. */

import type { SicroPoint } from "./schema";

export function distancePx(a: SicroPoint, b: SicroPoint): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  return Math.sqrt(dx * dx + dy * dy);
}

/** px/m a partir de dois pontos clicados + distância real declarada. Lança se os pontos coincidem. */
export function computePxPerMeter(
  p1: SicroPoint,
  p2: SicroPoint,
  realDistanceM: number,
): number {
  if (realDistanceM <= 0 || !Number.isFinite(realDistanceM)) {
    throw new Error("real distance must be a positive finite number");
  }
  const px = distancePx(p1, p2);
  if (px <= 0) {
    throw new Error("scale calibration requires two distinct points");
  }
  return px / realDistanceM;
}

/** `null` sem escala — o caller decide entre "px N" e "—". */
export function pxToMeters(
  pxDistance: number,
  pxPerMeter: number | null | undefined,
): number | null {
  if (!pxPerMeter || pxPerMeter <= 0) return null;
  return pxDistance / pxPerMeter;
}

/** Rótulo da medição: px sem escala; cm abaixo de 1 m; 2 casas até 10 m; 1 casa acima. */
export function formatMeasurement(
  pxDistance: number,
  pxPerMeter: number | null | undefined,
): string {
  const meters = pxToMeters(pxDistance, pxPerMeter);
  if (meters == null) {
    return `${pxDistance.toFixed(0)} px`;
  }
  if (meters < 1) {
    return `${(meters * 100).toFixed(0)} cm`;
  }
  if (meters < 10) {
    return `${meters.toFixed(2)} m`;
  }
  return `${meters.toFixed(1)} m`;
}

/** Área do polígono (fórmula do laço), em px²; sentido dos pontos não importa. */
export function polygonAreaPx(pts: SicroPoint[]): number {
  let s = 0;
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i]!;
    const b = pts[(i + 1) % pts.length]!;
    s += a.x * b.y - b.x * a.y;
  }
  return Math.abs(s) / 2;
}

export function polygonPerimeterPx(pts: SicroPoint[]): number {
  if (pts.length < 2) return 0;
  let s = 0;
  for (let i = 0; i < pts.length; i++) s += distancePx(pts[i]!, pts[(i + 1) % pts.length]!);
  return s;
}

/** Centroide da superfície; polígono degenerado cai na média dos vértices. */
export function polygonCentroid(pts: SicroPoint[]): SicroPoint {
  let a = 0;
  let cx = 0;
  let cy = 0;
  for (let i = 0; i < pts.length; i++) {
    const p = pts[i]!;
    const q = pts[(i + 1) % pts.length]!;
    const c = p.x * q.y - q.x * p.y;
    a += c;
    cx += (p.x + q.x) * c;
    cy += (p.y + q.y) * c;
  }
  if (Math.abs(a) < 1e-9) {
    const n = Math.max(pts.length, 1);
    return {
      x: pts.reduce((s, p) => s + p.x, 0) / n,
      y: pts.reduce((s, p) => s + p.y, 0) / n,
    };
  }
  return { x: cx / (3 * a), y: cy / (3 * a) };
}

function pointInPolygon(p: SicroPoint, pts: SicroPoint[]): boolean {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const a = pts[i]!;
    const b = pts[j]!;
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}

function distToSegment(p: SicroPoint, a: SicroPoint, b: SicroPoint): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const l2 = dx * dx + dy * dy;
  const t = l2 === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

/**
 * Onde pôr o rótulo da área: ponto de dentro mais longe das bordas (no L o centroide cai na
 * quina ou fora). Busca em grade com refinamento; sem ponto interno, volta ao centroide.
 */
export function polygonLabelPoint(pts: SicroPoint[]): SicroPoint {
  if (pts.length < 3) return polygonCentroid(pts);
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  let x0 = Math.min(...xs);
  let x1 = Math.max(...xs);
  let y0 = Math.min(...ys);
  let y1 = Math.max(...ys);
  const clearance = (p: SicroPoint) => {
    let d = Infinity;
    for (let i = 0; i < pts.length; i++) d = Math.min(d, distToSegment(p, pts[i]!, pts[(i + 1) % pts.length]!));
    return d;
  };
  let best: SicroPoint | null = null;
  let bestD = -1;
  const N = 16;
  for (let round = 0; round < 4; round++) {
    const sx = (x1 - x0) / N;
    const sy = (y1 - y0) / N;
    for (let i = 0; i <= N; i++) {
      for (let j = 0; j <= N; j++) {
        const p = { x: x0 + i * sx, y: y0 + j * sy };
        if (!pointInPolygon(p, pts)) continue;
        const d = clearance(p);
        if (d > bestD) {
          bestD = d;
          best = p;
        }
      }
    }
    if (!best) break;
    x0 = best.x - sx;
    x1 = best.x + sx;
    y0 = best.y - sy;
    y1 = best.y + sy;
  }
  return best ?? polygonCentroid(pts);
}

/** Rótulo da área: px² sem escala; 2 casas até 100 m²; 1 casa acima. */
export function formatArea(areaPx2: number, pxPerMeter: number | null | undefined): string {
  if (!pxPerMeter || pxPerMeter <= 0) return `${areaPx2.toFixed(0)} px²`;
  const m2 = areaPx2 / (pxPerMeter * pxPerMeter);
  return m2 < 100 ? `${m2.toFixed(2)} m²` : `${m2.toFixed(1)} m²`;
}

export function midpoint(a: SicroPoint, b: SicroPoint): SicroPoint {
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}

/** Ângulo em graus de a→b em relação ao eixo +x (rotação do rótulo da medição). */
export function angleDeg(a: SicroPoint, b: SicroPoint): number {
  return (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI;
}

/**
 * Encaixa `imgW × imgH` no canvas preservando proporção, centralizado, com
 * `margin` (fração, clampada a [0, 0.45]) livre em cada lado. Serve para foto
 * 4K de drone não estourar o canvas ao virar fundo.
 */
export function fitImageToCanvas(
  imgW: number,
  imgH: number,
  canvasW: number,
  canvasH: number,
  margin = 0.1,
): { x: number; y: number; width: number; height: number } {
  if (imgW <= 0 || imgH <= 0 || canvasW <= 0 || canvasH <= 0) {
    return { x: 0, y: 0, width: 0, height: 0 };
  }
  const m = Math.min(Math.max(margin, 0), 0.45);
  const usableW = canvasW * (1 - 2 * m);
  const usableH = canvasH * (1 - 2 * m);
  const scale = Math.min(usableW / imgW, usableH / imgH);
  const width = imgW * scale;
  const height = imgH * scale;
  const x = (canvasW - width) / 2;
  const y = (canvasH - height) / 2;
  return { x, y, width, height };
}
