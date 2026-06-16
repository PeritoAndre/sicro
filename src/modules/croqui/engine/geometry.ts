/**
 * Pure geometry helpers — Spike E.
 *
 * No React, no Konva. Just numbers in / numbers out. Easy to unit-test.
 * If the math is wrong here, every measurement on the canvas is wrong.
 */

import type { SicroPoint } from "./schema";

export function distancePx(a: SicroPoint, b: SicroPoint): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  return Math.sqrt(dx * dx + dy * dy);
}

/**
 * Calibrate a scale from two clicked points + the real-world distance the
 * user declares. Returns `pxPerMeter`. Throws when the points coincide.
 */
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

/**
 * Convert a pixel distance to meters using the active scale.
 * Returns null when the scale isn't defined yet — callers decide whether
 * to render "px N" or "—".
 */
export function pxToMeters(
  pxDistance: number,
  pxPerMeter: number | null | undefined,
): number | null {
  if (!pxPerMeter || pxPerMeter <= 0) return null;
  return pxDistance / pxPerMeter;
}

/** Human-friendly label for a measurement. Uses meters when scale exists. */
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

/**
 * Midpoint — used to position the measurement label between the two anchors.
 */
export function midpoint(a: SicroPoint, b: SicroPoint): SicroPoint {
  return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
}

/**
 * Angle (in degrees) of the segment a → b relative to the +x axis.
 * Used to rotate the measurement label so it lays along the segment.
 */
export function angleDeg(a: SicroPoint, b: SicroPoint): number {
  return (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI;
}

/**
 * Fit a rectangle of dimensions `imgW × imgH` into a canvas of
 * `canvasW × canvasH`, preserving aspect ratio and leaving a `margin`
 * fraction of the canvas free on each side.
 *
 * Returns the destination rectangle (top-left + size), centred inside
 * the canvas. Used by MVP 9 Round 5 so a 4K drone photo doesn't blow
 * past the canvas the moment it's inserted as a background. A
 * 5472×3648 photo on a 1600×1000 canvas with the default
 * `margin = 0.1` resolves to roughly 1440×960 centred at (80, 20) —
 * fits comfortably.
 *
 * `margin` is clamped to [0, 0.45] (more than that and there's no
 * room for the image at all).
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
