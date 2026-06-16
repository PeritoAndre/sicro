/**
 * G12 — Testes para os novos factories e cálculos geométricos.
 */

import { describe, expect, it } from "vitest";
import { makeAngle, makeFreehand, makePolygon } from "./factories";

describe("G12 — new annotation factories", () => {
  it("makePolygon stores points and uses first as anchor", () => {
    const pts = [
      { x: 5, y: 10 },
      { x: 20, y: 10 },
      { x: 15, y: 25 },
    ];
    const ann = makePolygon(pts);
    expect(ann.kind).toBe("polygon");
    expect(ann.x).toBe(5);
    expect(ann.y).toBe(10);
    expect(ann.points).toEqual(pts);
  });

  it("makeAngle stores 3 points with vertex in middle", () => {
    const ann = makeAngle(
      { x: 0, y: 0 },
      { x: 10, y: 10 },
      { x: 20, y: 0 },
    );
    expect(ann.kind).toBe("angle");
    expect(ann.points).toHaveLength(3);
    expect(ann.x).toBe(10);
    expect(ann.y).toBe(10);
  });

  it("makeFreehand keeps the captured points sequence", () => {
    const pts = Array.from({ length: 5 }, (_, i) => ({
      x: i * 2,
      y: i * 3,
    }));
    const ann = makeFreehand(pts);
    expect(ann.kind).toBe("freehand");
    expect(ann.points).toHaveLength(5);
  });

  it("polygon factory tolerates empty array (degenerate)", () => {
    const ann = makePolygon([]);
    expect(ann.x).toBe(0);
    expect(ann.y).toBe(0);
    expect(ann.points).toEqual([]);
  });
});
