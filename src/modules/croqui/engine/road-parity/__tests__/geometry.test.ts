/**
 * Python Parity Engine — testes de geometria.
 */

import { describe, expect, it } from "vitest";
import {
  buildRoadEdges,
  buildRoadRibbon,
  buildRoadSidewalk,
  buildRoundaboutDiskPolygon,
  buildRoundaboutRings,
  flattenVec2,
  projectWorldPoints,
  resolvePxPerM,
  sampleCubicBezier,
} from "../geometry";
import {
  makeParityRoad,
  makeParityRoadBezier,
  makeParityRoundabout,
  PARITY_DEFAULT_PX_PER_M,
} from "../index";

describe("road-parity / sampleCubicBezier", () => {
  it("reta horizontal: amostras alinhadas ao eixo X", () => {
    const road = makeParityRoad(0, 0, 100, 0);
    const pts = sampleCubicBezier(road, 8);
    expect(pts).toHaveLength(9);
    for (const p of pts) {
      expect(p.y).toBeCloseTo(0, 6);
    }
    expect(pts[0]).toEqual({ x: 0, y: 0 });
    expect(pts[pts.length - 1]).toEqual({ x: 100, y: 0 });
  });

  it("curva em S: pontos próximos do controle 1 deslocam-se em Y", () => {
    // Bezier simétrica em S (P1.y=-50, P2.y=50). No t=0.5 os efeitos
    // dos controles se cancelam (B(0.5).y == 0 matematicamente).
    // Em t=0.25 o controle 1 (y=-50) domina, então y < 0.
    const road = makeParityRoadBezier(0, 0, 30, -50, 70, 50, 100, 0);
    const pts = sampleCubicBezier(road, 16);
    const t025 = pts[4]!; // i=4 de n=16 → t=0.25
    const t075 = pts[12]!; // i=12 → t=0.75
    expect(t025.y).toBeLessThan(-5); // dominado pelo C1
    expect(t075.y).toBeGreaterThan(5); // dominado pelo C2
    // Endpoints sempre = anchors.
    expect(pts[0]).toEqual({ x: 0, y: 0 });
    expect(pts[pts.length - 1]).toEqual({ x: 100, y: 0 });
  });

  it("via degenerada (A = B) → todos os pontos no mesmo lugar", () => {
    const road = makeParityRoad(50, 50, 50, 50);
    const pts = sampleCubicBezier(road, 4);
    for (const p of pts) {
      expect(p.x).toBeCloseTo(50, 6);
      expect(p.y).toBeCloseTo(50, 6);
    }
  });

  it("n+1 pontos sempre", () => {
    expect(sampleCubicBezier(makeParityRoad(0, 0, 10, 0), 0)).toHaveLength(1);
    expect(sampleCubicBezier(makeParityRoad(0, 0, 10, 0), 1)).toHaveLength(2);
    expect(sampleCubicBezier(makeParityRoad(0, 0, 10, 0), 32)).toHaveLength(33);
  });
});

describe("road-parity / buildRoadEdges", () => {
  it("reta horizontal de largura 7m: left/right deslocados ±3.5m em Y", () => {
    const road = makeParityRoad(0, 0, 100, 0, { largura_m: 7 });
    const samples = sampleCubicBezier(road, 8);
    const { left, right } = buildRoadEdges(samples, 3.5);
    expect(left).toHaveLength(samples.length);
    expect(right).toHaveLength(samples.length);
    // Em uma reta horizontal indo de (0,0) a (100,0), perpendicular
    // anti-horário é (-0,1). Left = y - 0 + 3.5*1 = 3.5; right = -3.5.
    // (depende da convenção de sinal — verificamos absoluto):
    for (let i = 0; i < samples.length; i++) {
      const l = left[i]!;
      const r = right[i]!;
      expect(Math.abs(l.y)).toBeCloseTo(3.5, 1);
      expect(Math.abs(r.y)).toBeCloseTo(3.5, 1);
      expect(l.y * r.y).toBeLessThan(0); // sinais opostos
    }
  });

  it("polyline de 1 ponto → bordas vazias", () => {
    const result = buildRoadEdges([{ x: 0, y: 0 }], 3);
    expect(result.left).toEqual([]);
    expect(result.right).toEqual([]);
  });
});

describe("road-parity / buildRoadRibbon", () => {
  it("polígono fechado tem 2N pontos (left + reverse(right))", () => {
    const road = makeParityRoad(0, 0, 100, 0, { largura_m: 7 });
    const samples = sampleCubicBezier(road, 16);
    const ribbon = buildRoadRibbon(samples, 3.5);
    expect(ribbon).toHaveLength(samples.length * 2);
  });

  it("primeiro ponto do polígono é o primeiro ponto da borda esquerda", () => {
    const road = makeParityRoad(0, 0, 100, 0, { largura_m: 7 });
    const samples = sampleCubicBezier(road, 8);
    const ribbon = buildRoadRibbon(samples, 3.5);
    const { left } = buildRoadEdges(samples, 3.5);
    expect(ribbon[0]).toEqual(left[0]);
  });
});

describe("road-parity / buildRoadSidewalk", () => {
  it("calçada é mais larga que asfalto (offset positivo)", () => {
    const road = makeParityRoad(0, 0, 100, 0, { largura_m: 7 });
    const samples = sampleCubicBezier(road, 8);
    const asphalt = buildRoadRibbon(samples, 3.5);
    const sidewalk = buildRoadSidewalk(samples, 3.5, 2);
    // Cada par de pontos da calçada tem mais offset perpendicular.
    // Verificação simples: máximo |y| da calçada > máximo |y| do asfalto.
    const maxAbsY = (pts: { x: number; y: number }[]) =>
      Math.max(...pts.map((p) => Math.abs(p.y)));
    expect(maxAbsY(sidewalk)).toBeGreaterThan(maxAbsY(asphalt));
  });
});

describe("road-parity / buildRoundaboutRings", () => {
  it("raios escalados pelo pxPerM", () => {
    const rb = makeParityRoundabout(0, 0, 10, { largura_m: 6 });
    const rings = buildRoundaboutRings(rb, 5);
    expect(rings.outer_r_px).toBeCloseTo((10 + 3) * 5, 1);
    expect(rings.inner_r_px).toBeCloseTo((10 - 3) * 5, 1);
    expect(rings.sidewalk_r_px).toBeCloseTo((10 + 3 + 2) * 5, 1);
  });

  it("ilha não fica negativa quando largura excede raio", () => {
    // Factory clamp evita esse caso; mas se construído manual, raio
    // interno deve ser >= 0.
    const rb = makeParityRoundabout(0, 0, 5, { largura_m: 10 });
    const rings = buildRoundaboutRings(rb, 10);
    expect(rings.inner_r_px).toBeGreaterThanOrEqual(0);
  });

  it("centro convertido para pixels", () => {
    const rb = makeParityRoundabout(20, 30, 10);
    const rings = buildRoundaboutRings(rb, 8);
    expect(rings.cx_px).toBe(20 * 8);
    expect(rings.cy_px).toBe(30 * 8);
  });
});

describe("road-parity / buildRoundaboutDiskPolygon", () => {
  it("48 vértices por default", () => {
    const rb = makeParityRoundabout(0, 0, 10);
    expect(buildRoundaboutDiskPolygon(rb)).toHaveLength(48);
  });

  it("todos os vértices estão no raio externo (r + largura/2)", () => {
    const rb = makeParityRoundabout(0, 0, 10, { largura_m: 6 });
    const poly = buildRoundaboutDiskPolygon(rb, 16);
    const targetR = 10 + 3;
    for (const p of poly) {
      expect(Math.hypot(p.x, p.y)).toBeCloseTo(targetR, 5);
    }
  });
});

describe("road-parity / resolvePxPerM", () => {
  it("número positivo válido → ele mesmo", () => {
    expect(resolvePxPerM(15)).toBe(15);
    expect(resolvePxPerM(0.5)).toBe(0.5);
  });

  it("null/undefined/0/NaN/Infinity → default", () => {
    expect(resolvePxPerM(null)).toBe(PARITY_DEFAULT_PX_PER_M);
    expect(resolvePxPerM(undefined)).toBe(PARITY_DEFAULT_PX_PER_M);
    expect(resolvePxPerM(0)).toBe(PARITY_DEFAULT_PX_PER_M);
    expect(resolvePxPerM(-5)).toBe(PARITY_DEFAULT_PX_PER_M);
    expect(resolvePxPerM(NaN)).toBe(PARITY_DEFAULT_PX_PER_M);
    expect(resolvePxPerM(Infinity)).toBe(PARITY_DEFAULT_PX_PER_M);
  });
});

describe("road-parity / projectWorldPoints + flattenVec2", () => {
  it("aplica scale + offset corretamente", () => {
    const projected = projectWorldPoints(
      [{ x: 1, y: 2 }],
      10,
      100,
      50,
    );
    expect(projected[0]).toEqual({ x: 1 * 10 + 100, y: 2 * 10 + 50 });
  });

  it("flatten gera [x1, y1, x2, y2]", () => {
    expect(
      flattenVec2([
        { x: 1, y: 2 },
        { x: 3, y: 4 },
      ]),
    ).toEqual([1, 2, 3, 4]);
  });
});
