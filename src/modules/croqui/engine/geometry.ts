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
