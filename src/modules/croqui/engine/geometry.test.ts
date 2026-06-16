/**
 * Unit tests for the pure geometry helpers — Spike E.
 * No DOM, no Konva. If the math is wrong here, every measurement on the
 * canvas is wrong.
 */

import { describe, expect, it } from "vitest";
import {
  angleDeg,
  computePxPerMeter,
  distancePx,
  formatMeasurement,
  midpoint,
  pxToMeters,
} from "./geometry";

describe("distancePx", () => {
  it("returns 0 for coincident points", () => {
    expect(distancePx({ x: 10, y: 20 }, { x: 10, y: 20 })).toBe(0);
  });
  it("computes 3-4-5 correctly", () => {
    expect(distancePx({ x: 0, y: 0 }, { x: 3, y: 4 })).toBe(5);
  });
});

describe("computePxPerMeter", () => {
  it("returns positive ratio for valid input", () => {
    const r = computePxPerMeter({ x: 0, y: 0 }, { x: 100, y: 0 }, 10);
    expect(r).toBe(10);
  });
  it("throws when points coincide", () => {
    expect(() =>
      computePxPerMeter({ x: 5, y: 5 }, { x: 5, y: 5 }, 10),
    ).toThrow();
  });
  it("throws when real distance is non-positive or NaN", () => {
    expect(() =>
      computePxPerMeter({ x: 0, y: 0 }, { x: 100, y: 0 }, 0),
    ).toThrow();
    expect(() =>
      computePxPerMeter({ x: 0, y: 0 }, { x: 100, y: 0 }, -3),
    ).toThrow();
    expect(() =>
      computePxPerMeter({ x: 0, y: 0 }, { x: 100, y: 0 }, NaN),
    ).toThrow();
  });
});

describe("pxToMeters", () => {
  it("returns null when scale undefined or zero", () => {
    expect(pxToMeters(100, null)).toBeNull();
    expect(pxToMeters(100, 0)).toBeNull();
    expect(pxToMeters(100, -5)).toBeNull();
  });
  it("divides correctly when scale is valid", () => {
    expect(pxToMeters(100, 50)).toBe(2);
  });
});

describe("formatMeasurement", () => {
  it("falls back to px when scale is missing", () => {
    expect(formatMeasurement(123.4, null)).toBe("123 px");
  });
  it("uses cm under 1 metre", () => {
    expect(formatMeasurement(25, 50)).toBe("50 cm");
  });
  it("uses 2 decimals between 1 and 10 metres", () => {
    expect(formatMeasurement(125, 50)).toBe("2.50 m");
  });
  it("uses 1 decimal above 10 metres", () => {
    expect(formatMeasurement(600, 50)).toBe("12.0 m");
  });
});

describe("midpoint", () => {
  it("returns the average of both coords", () => {
    expect(midpoint({ x: 0, y: 0 }, { x: 10, y: 20 })).toEqual({ x: 5, y: 10 });
  });
});

describe("angleDeg", () => {
  it("returns 0 for a horizontal vector", () => {
    expect(angleDeg({ x: 0, y: 0 }, { x: 10, y: 0 })).toBe(0);
  });
  it("returns 90 for a vertical vector", () => {
    expect(angleDeg({ x: 0, y: 0 }, { x: 0, y: 10 })).toBe(90);
  });
  it("returns 180 going left", () => {
    expect(Math.abs(angleDeg({ x: 0, y: 0 }, { x: -10, y: 0 }))).toBe(180);
  });
});
