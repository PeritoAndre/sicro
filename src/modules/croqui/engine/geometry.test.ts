/** Testes da geometria pura (escala e medições). */

import { describe, expect, it } from "vitest";
import {
  angleDeg,
  computePxPerMeter,
  distancePx,
  formatArea,
  formatMeasurement,
  polygonAreaPx,
  polygonCentroid,
  polygonLabelPoint,
  polygonPerimeterPx,
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

describe("polígono (área)", () => {
  // Retângulo 40 × 20 px; com 10 px/m = 4 × 2 m.
  const ret = [
    { x: 0, y: 0 },
    { x: 40, y: 0 },
    { x: 40, y: 20 },
    { x: 0, y: 20 },
  ];

  it("área pela fórmula do laço, nos dois sentidos", () => {
    expect(polygonAreaPx(ret)).toBe(800);
    expect(polygonAreaPx([...ret].reverse())).toBe(800);
  });

  it("polígono côncavo (L)", () => {
    const l = [
      { x: 0, y: 0 },
      { x: 30, y: 0 },
      { x: 30, y: 10 },
      { x: 10, y: 10 },
      { x: 10, y: 30 },
      { x: 0, y: 30 },
    ];
    expect(polygonAreaPx(l)).toBe(500);
  });

  it("rótulo cai dentro do L, longe da quina interna", () => {
    const l = [
      { x: 0, y: 0 }, { x: 300, y: 0 }, { x: 300, y: 100 },
      { x: 100, y: 100 }, { x: 100, y: 300 }, { x: 0, y: 300 },
    ];
    const p = polygonLabelPoint(l);
    // Meia largura do braço = 50: ponto bem dentro de um dos braços.
    const folga = Math.min(p.x, p.y, Math.max(300 - p.x, 0), Math.max(300 - p.y, 0));
    expect(folga).toBeGreaterThan(40);
    expect(p.x < 100 || p.y < 100).toBe(true);
  });

  it("perímetro e centroide", () => {
    expect(polygonPerimeterPx(ret)).toBe(120);
    expect(polygonCentroid(ret)).toEqual({ x: 20, y: 10 });
  });

  it("formatArea em m² com escala e px² sem", () => {
    expect(formatArea(800, 10)).toBe("8.00 m²");
    expect(formatArea(1_250_000, 10)).toBe("12500.0 m²");
    expect(formatArea(800, null)).toBe("800 px²");
  });
});
