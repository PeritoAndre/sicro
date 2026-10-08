import { describe, expect, it } from "vitest";
import {
  coerceCroquiDoc,
  makePerson,
  personApplyPose,
  personBoundsPx,
  personChangePosicao,
  personRig,
  personToLocal,
  personToWorld,
  type SicroPersonObject,
} from "./index";

const front = (o: SicroPersonObject) => (o.posicao === "lat_d" ? 1 : -1);

describe("makePerson", () => {
  it("nasce sem rótulo, 1,75 m, cabeça para o topo", () => {
    const o = makePerson("dorsal", { x: 10, y: 20 });
    expect(o.label).toBe("");
    expect(o.altura_m).toBe(1.75);
    expect(o.category).toBe("pessoas");
    const R = personRig(o);
    expect(R.headC.y).toBeLessThan(0);
  });
  it("membros alcançam o alvo da pose (IK)", () => {
    const o = makePerson("dorsal", { x: 0, y: 0 });
    const R = personRig(o);
    const H = o.altura_m;
    expect(R.legs[0]!.T.x).toBeCloseTo(o.pose.ankle[0].x * H, 3);
    expect(R.legs[0]!.T.y).toBeCloseTo(o.pose.ankle[0].y * H, 3);
  });
});

describe("troca de lateral (bug: ficava torta)", () => {
  it("lat_e → lat_d espelha a pose: mãos e joelhos continuam na frente do corpo", () => {
    const a = { ...makePerson("lat_e", { x: 0, y: 0 }) };
    const b = { ...a, ...personChangePosicao(a, "lat_d") } as SicroPersonObject;
    for (const o of [a, b]) {
      const R = personRig(o);
      const m = front(o);
      for (const arm of R.arms) expect(arm.T.x * m).toBeGreaterThan(0);
      for (const leg of R.legs) expect(leg.E.x * m).toBeGreaterThan(0);
    }
    expect(b.curva).toBe(-a.curva);
  });
  it("ida e volta devolve a mesma pose", () => {
    const a = makePerson("lat_d", { x: 0, y: 0 });
    const b = { ...a, ...personChangePosicao(a, "lat_e") } as SicroPersonObject;
    const c = { ...b, ...personChangePosicao(b, "lat_d") } as SicroPersonObject;
    expect(c.pose).toEqual(a.pose);
  });
  it("de frente → de lado aplica a pose lateral", () => {
    const a = makePerson("dorsal", { x: 0, y: 0 });
    const b = { ...a, ...personChangePosicao(a, "lat_d") } as SicroPersonObject;
    expect(b.curva).toBeGreaterThan(0);
  });
});

describe("poses e coordenadas", () => {
  it("braços abertos de frente abrem para os dois lados", () => {
    const o = makePerson("dorsal", { x: 0, y: 0 });
    const p = { ...o, ...personApplyPose(o, "abertos") } as SicroPersonObject;
    const R = personRig(p);
    expect(R.arms[0]!.T.x).toBeLessThan(0);
    expect(R.arms[1]!.T.x).toBeGreaterThan(0);
  });
  it("local ↔ mundo com rotação", () => {
    const o = { ...makePerson("dorsal", { x: 100, y: 50 }), rotation: 37 };
    const w = personToWorld(o, { x: 0.3, y: -0.2 }, 10);
    const l = personToLocal(o, w, 10);
    expect(l.x).toBeCloseTo(0.3, 6);
    expect(l.y).toBeCloseTo(-0.2, 6);
  });
  it("AABB cobre o corpo e a pessoa sobrevive ao coerce", () => {
    const o = makePerson("dorsal", { x: 0, y: 0 });
    const b = personBoundsPx(o, 10);
    expect(b.height).toBeGreaterThan(17);
    const doc = coerceCroquiDoc({ croqui_id: "c", occurrence_id: "o", objects: [{ ...o, category: undefined }] });
    expect(doc.objects[0]).toMatchObject({ kind: "person", category: "pessoas" });
  });
});
