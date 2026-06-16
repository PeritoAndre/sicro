/**
 * fitImageToCanvas — unit tests (MVP 9 Round 5).
 *
 * The helper feeds the drone-import and "Importar imagem" pipelines:
 * it has to keep a 4K photo from blowing past the canvas while
 * preserving aspect ratio + leaving a configurable margin. Tests
 * cover the canonical cases (landscape, portrait, square, image
 * smaller than canvas, clamp behaviour).
 */

import { describe, expect, it } from "vitest";
import { fitImageToCanvas } from "./geometry";

describe("fitImageToCanvas", () => {
  it("scales a landscape 4K image into a 1600×1000 canvas with 10% margin", () => {
    const rect = fitImageToCanvas(5472, 3648, 1600, 1000, 0.1);
    // Usable area is 1280 × 800. Scale = min(1280/5472, 800/3648)
    // = min(0.2339, 0.2193) = 0.2193 → width ≈ 1199, height = 800.
    expect(rect.width).toBeCloseTo(800 * (5472 / 3648), 0);
    expect(rect.height).toBeCloseTo(800);
    // Centered in the canvas.
    expect(rect.x + rect.width / 2).toBeCloseTo(800);
    expect(rect.y + rect.height / 2).toBeCloseTo(500);
  });

  it("scales a portrait image so its height fits the usable area", () => {
    const rect = fitImageToCanvas(1080, 1920, 800, 600, 0.1);
    // Usable area is 640 × 480. Scale = min(640/1080, 480/1920) = 0.25.
    expect(rect.width).toBeCloseTo(270);
    expect(rect.height).toBeCloseTo(480);
    expect(rect.x + rect.width / 2).toBeCloseTo(400);
    expect(rect.y + rect.height / 2).toBeCloseTo(300);
  });

  it("keeps a square image square and centered", () => {
    const rect = fitImageToCanvas(1000, 1000, 600, 400, 0.1);
    // Usable 480 × 320, scale = 320/1000 = 0.32 → 320 × 320
    expect(rect.width).toBeCloseTo(320);
    expect(rect.height).toBeCloseTo(320);
    expect(rect.x).toBeCloseTo((600 - 320) / 2);
    expect(rect.y).toBeCloseTo((400 - 320) / 2);
  });

  it("scales down even when the image is smaller than the canvas (it still respects the margin)", () => {
    const rect = fitImageToCanvas(100, 100, 1000, 1000, 0.1);
    // Usable 800 × 800. Image becomes 800 × 800 — scaled UP because
    // the helper is "fit to useable area", not "shrink to fit". This
    // is the expected behaviour for the drone background flow.
    expect(rect.width).toBeCloseTo(800);
    expect(rect.height).toBeCloseTo(800);
  });

  it("clamps oversize margin so an image still fits", () => {
    const rect = fitImageToCanvas(100, 100, 200, 200, 0.9);
    // Margin clamped to 0.45 → usable 20 × 20.
    expect(rect.width).toBeLessThanOrEqual(20.01);
    expect(rect.height).toBeLessThanOrEqual(20.01);
    expect(rect.width).toBeGreaterThan(0);
  });

  it("clamps negative margin to 0", () => {
    const rect = fitImageToCanvas(200, 100, 400, 100, -0.5);
    // Effective margin 0 → usable 400 × 100, scale = 100/100 = 1, width = 200.
    expect(rect.width).toBeCloseTo(200);
    expect(rect.height).toBeCloseTo(100);
  });

  it("returns zeros for degenerate inputs (avoid NaN / Infinity)", () => {
    expect(fitImageToCanvas(0, 100, 200, 200)).toEqual({
      x: 0,
      y: 0,
      width: 0,
      height: 0,
    });
    expect(fitImageToCanvas(100, 100, 0, 0)).toEqual({
      x: 0,
      y: 0,
      width: 0,
      height: 0,
    });
  });
});
