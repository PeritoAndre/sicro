import { describe, expect, it } from "vitest";
import {
  ARMA_MODELOS,
  TRACE_FLUIDOS,
  armaCornersPx,
  armaLenM,
  coerceCroquiDoc,
  curveRadiusM,
  frenagemDistM,
  makeTrace,
  tn,
  tp,
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

describe("sangue", () => {
  it("nasce como poça, com os padrões, e o fluido não tem mais sangue", () => {
    const t = makeTrace("sangue", { x: 0, y: 0 }, { x: 20, y: 0 }, PPM);
    expect(tp(t, "tipo")).toBe("poca");
    expect(tn(t, "raio")).toBe(0.6);
    expect(tn(t, "tom")).toBe(0.3);
    expect(Object.keys(TRACE_FLUIDOS)).not.toContain("sangue");
  });
  it("AABB cobre a poça e o leque da projeção", () => {
    const poca = makeTrace("sangue", { x: 0, y: 0 }, { x: 30, y: 0 }, PPM);
    poca.params.raio = 1;
    const b = traceBoundsPx(poca, PPM);
    expect(b.x + b.width).toBeGreaterThanOrEqual(30 + 10);
    expect(b.y).toBeLessThanOrEqual(-10);
    const proj = makeTrace("sangue", { x: 0, y: 0 }, { x: 40, y: 0 }, PPM);
    proj.params = { ...proj.params, tipo: "projecao", abert: 90 };
    const pb = traceBoundsPx(proj, PPM);
    expect(pb.y).toBeLessThan(-20);
    expect(pb.y + pb.height).toBeGreaterThan(20);
  });
  it("leituras por padrão: área da poça, gotas, alcance", () => {
    const t = makeTrace("sangue", { x: 0, y: 0 }, { x: 20, y: 0 }, PPM);
    t.bend = 0;
    expect(traceReadouts(t, PPM).rows[0]![0]).toBe("Área da poça ≈");
    t.params = { ...t.params, tipo: "gotas", dens: 10 };
    expect(traceReadouts(t, PPM).rows).toEqual([["Trajeto", "2,0 m"], ["Gotas ≈", "20"]]);
    t.params = { ...t.params, tipo: "projecao", abert: 45 };
    expect(traceReadouts(t, PPM).rows).toEqual([["Alcance", "2,0 m"], ["Abertura", "45°"]]);
  });
  it("fluido 'sangue' de croqui antigo vira poça de sangue ao carregar", () => {
    const old = makeTrace("fluido", { x: 0, y: 0 }, { x: 30, y: 0 }, PPM);
    old.params = { ...old.params, tipo: "sangue", raio: 1.2, trilha: false };
    const doc = coerceCroquiDoc({ croqui_id: "c", occurrence_id: "o", objects: [old] });
    const t = doc.objects[0] as typeof old;
    expect(t.subtype).toBe("sangue");
    expect(t.params).toMatchObject({ tipo: "poca", raio: 1.2, trilha: false });
    expect(t.params).not.toHaveProperty("gotas");
    const oleo = makeTrace("fluido", { x: 0, y: 0 }, { x: 30, y: 0 }, PPM);
    expect((coerceCroquiDoc({ croqui_id: "c", occurrence_id: "o", objects: [oleo] }).objects[0] as typeof oleo).subtype).toBe("fluido");
  });
});

describe("arma", () => {
  it("comprimento desenhado = real × ampliação; eixo p0 → p1 não muda o tamanho", () => {
    const t = makeTrace("arma", { x: 0, y: 0 }, { x: 60, y: 0 }, PPM);
    expect(tp(t, "modelo")).toBe("pistola");
    expect(armaLenM(t)).toBeCloseTo(ARMA_MODELOS.pistola!.len_m, 6);
    t.params = { ...t.params, amp: 3 };
    expect(armaLenM(t)).toBeCloseTo(0.57, 6);
    t.params = { ...t.params, modelo: "espingarda", amp: 1 };
    expect(armaLenM(t)).toBeCloseTo(1.15, 6);
  });
  it("retângulo e AABB centrados entre p0 e p1, girando com o eixo", () => {
    const t = makeTrace("arma", { x: 0, y: 0 }, { x: 0, y: 60 }, PPM);
    t.params = { ...t.params, modelo: "fuzil" };
    const c = armaCornersPx(t, PPM);
    const cx = c.reduce((a, p) => a + p.x, 0) / 4;
    const cy = c.reduce((a, p) => a + p.y, 0) / 4;
    expect(cx).toBeCloseTo(0, 6);
    expect(cy).toBeCloseTo(30, 6);
    // Eixo vertical: o comprimento (0,9 m = 9 px) fica em y.
    expect(Math.max(...c.map((p) => p.y)) - Math.min(...c.map((p) => p.y))).toBeCloseTo(9, 6);
    const b = traceBoundsPx(t, PPM);
    expect(b.x).toBeLessThan(0);
    expect(b.y + b.height).toBeGreaterThan(34.5);
    expect(traceReadouts(t, PPM).rows[0]).toEqual(["Comprimento real", "0,90 m"]);
  });
});
