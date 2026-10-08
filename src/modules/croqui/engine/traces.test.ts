import { describe, expect, it } from "vitest";
import {
  coerceCroquiDoc,
  curveRadiusM,
  frenagemDistM,
  makeTrace,
  tn,
  traceBoundsPx,
  traceGeom,
  traceReadouts,
} from "./index";

const PPM = 10;

describe("traceGeom", () => {
  it("reta: comprimento = corda, meio no meio", () => {
    const g = traceGeom({ x: 0, y: 0 }, { x: 10, y: 0 }, 0);
    expect(g.len).toBeCloseTo(10, 3);
    expect(g.mid).toEqual({ x: 5, y: 0 });
  });
  it("flecha: o meio do traçado passa a `bend` da corda", () => {
    const g = traceGeom({ x: 0, y: 0 }, { x: 10, y: 0 }, 2);
    const meio = g.pts[Math.floor(g.pts.length / 2)]!;
    expect(meio.y).toBeCloseTo(2, 2);
    expect(g.len).toBeGreaterThan(10);
  });
});

describe("makeTrace", () => {
  it("nasce sem rótulo e sem cota, com os padrões do tipo", () => {
    const t = makeTrace("frenagem", { x: 0, y: 0 }, { x: 140, y: 0 }, PPM);
    expect(t.label).toBe("");
    expect(t.show_measure).toBe(false);
    expect(t.category).toBe("vestigios");
    expect(tn(t, "rodas")).toBe(2);
    expect(tn(t, "bitola")).toBe(1.5);
  });
  it("colisão é um ponto só", () => {
    const t = makeTrace("colisao", { x: 5, y: 5 }, { x: 50, y: 50 }, PPM);
    expect(t.p1).toEqual(t.p0);
  });
  it("derrapagem já nasce curva (flecha em px)", () => {
    const t = makeTrace("derrapagem", { x: 0, y: 0 }, { x: 200, y: 0 }, PPM);
    expect(t.bend).toBeCloseTo(-15, 6);
  });
});

describe("contas de referência", () => {
  it("frenagem: v = √(2·μ·g·d)", () => {
    const t = makeTrace("frenagem", { x: 0, y: 0 }, { x: 200, y: 0 }, PPM); // 20 m
    const r = traceReadouts(t, PPM);
    const v = Math.sqrt(2 * 0.7 * 9.81 * 20) * 3.6;
    expect(r.rows.find(([k]) => k.startsWith("Velocidade"))?.[1]).toBe(`${v.toFixed(0)} km/h`);
  });
  it("frenagem com 4 marcas desconta o entre-eixos", () => {
    const t = makeTrace("frenagem", { x: 0, y: 0 }, { x: 200, y: 0 }, PPM);
    t.params.rodas = 4;
    expect(frenagemDistM(t, 20)).toBeCloseTo(17.4, 6);
  });
  it("raio pela corda e flecha", () => {
    expect(curveRadiusM(20, 1)).toBeCloseTo(50.5, 6);
    expect(curveRadiusM(20, 0)).toBe(Infinity);
  });
});

describe("serialização e limites", () => {
  it("o vestígio sobrevive ao coerce do envelope", () => {
    const t = makeTrace("fluido", { x: 0, y: 0 }, { x: 30, y: 0 }, PPM);
    const doc = coerceCroquiDoc({ croqui_id: "c", occurrence_id: "o", objects: [t] });
    expect(doc.objects).toHaveLength(1);
    expect(doc.objects[0]).toMatchObject({ kind: "trace", subtype: "fluido", category: "vestigios" });
  });
  it("AABB cobre a poça do fluido", () => {
    const t = makeTrace("fluido", { x: 0, y: 0 }, { x: 30, y: 0 }, PPM);
    const b = traceBoundsPx(t, PPM);
    expect(b.x + b.width).toBeGreaterThan(30 + 0.9 * PPM);
  });
});
