import { describe, expect, it } from "vitest";
import { peDaPerpendicular } from "./referenciais";

describe("peDaPerpendicular", () => {
  it("cai no pé dentro do segmento", () => {
    expect(peDaPerpendicular([0, 0, 100, 0], { x: 30, y: 40 })).toEqual({ x: 30, y: 0 });
  });
  it("além da ponta, prolonga a reta (continua perpendicular)", () => {
    expect(peDaPerpendicular([0, 0, 100, 0], { x: 130, y: 40 })).toEqual({ x: 130, y: 0 });
  });
  it("polilinha: escolhe o segmento certo", () => {
    const pe = peDaPerpendicular([0, 0, 100, 0, 100, 100], { x: 90, y: 60 });
    expect(pe.x).toBeCloseTo(100);
    expect(pe.y).toBeCloseTo(60);
  });
  it("linha inclinada", () => {
    const pe = peDaPerpendicular([0, 0, 100, 100], { x: 100, y: 0 });
    expect(pe.x).toBeCloseTo(50);
    expect(pe.y).toBeCloseTo(50);
  });
});
