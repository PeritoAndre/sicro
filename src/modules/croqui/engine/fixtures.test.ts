import { describe, expect, it } from "vitest";
import {
  coerceCroquiDoc,
  FIXTURE_SPECS,
  FIXTURE_SUBTYPES,
  fixtureBoundsPx,
  fixtureCategory,
  fixtureReadouts,
  fn,
  makeFixture,
} from "./index";

const PPM = 10;

describe("makeFixture", () => {
  it("todo tipo nasce sem rótulo, com padrões e na categoria do grupo", () => {
    for (const sub of FIXTURE_SUBTYPES) {
      const o = makeFixture(sub, { x: 0, y: 0 }, null, PPM);
      expect(o.label).toBe("");
      expect(o.category).toBe(FIXTURE_SPECS[sub].grupo);
      for (const [k, v] of Object.entries(FIXTURE_SPECS[sub].def)) expect(o.params[k]).toBe(v);
    }
  });
  it("ponto sem direção aponta para +x a len_m", () => {
    const o = makeFixture("camera", { x: 0, y: 0 }, null, PPM);
    expect(o.p1).toEqual({ x: 150, y: 0 });
  });
  it("árvore guarda o raio nos parâmetros (p1 = p0)", () => {
    const o = makeFixture("arvore", { x: 5, y: 5 }, { x: 90, y: 5 }, PPM);
    expect(o.p1).toEqual(o.p0);
    expect(fn(o, "raio")).toBe(2.5);
  });
});

describe("categorias e leituras", () => {
  it("sinalização x entorno", () => {
    expect(fixtureCategory("placa")).toBe("sinalizacao");
    expect(fixtureCategory("camera")).toBe("entorno");
  });
  it("faixa mostra a travessia; câmera, alcance e abertura", () => {
    const faixa = makeFixture("faixa_pedestre", { x: 0, y: 0 }, { x: 0, y: 140 }, PPM);
    expect(fixtureReadouts(faixa, PPM).rows[0]).toEqual(["Travessia", "14,0 m"]);
    const cam = makeFixture("camera", { x: 0, y: 0 }, { x: 200, y: 0 }, PPM);
    expect(fixtureReadouts(cam, PPM).rows).toEqual([["Alcance do campo", "20,0 m"], ["Abertura", "60°"]]);
  });
});

describe("serialização e limites", () => {
  it("o elemento sobrevive ao coerce com a categoria", () => {
    const o = makeFixture("semaforo", { x: 0, y: 0 }, { x: 40, y: 0 }, PPM);
    const doc = coerceCroquiDoc({ croqui_id: "c", occurrence_id: "o", objects: [{ ...o, category: undefined }] });
    expect(doc.objects[0]).toMatchObject({ kind: "fixture", subtype: "semaforo", category: "sinalizacao" });
  });
  it("AABB da árvore cobre a copa", () => {
    const o = makeFixture("arvore", { x: 100, y: 100 }, null, PPM);
    const b = fixtureBoundsPx(o, PPM);
    expect(b.x).toBeLessThanOrEqual(100 - 25);
    expect(b.width).toBeGreaterThanOrEqual(50);
  });
});
