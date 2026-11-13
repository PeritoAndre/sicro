import { describe, expect, it } from "vitest";
import { makeMeasurement, makeVehicle } from "../engine";
import type { SicroLineObject, SicroObject } from "../engine";
import { ligarAoObjeto, objetoSob, resolverCotasVivas } from "./cotasVivas";

const r1: SicroLineObject = { id: "r1", layer_id: "layer_objects", kind: "line", subtype: "r1", points: [0, 0, 1000, 0], stroke_width: 2, visible: true, locked: false, category: "referenciais" };

describe("cotas vivas", () => {
  it("segue o carro e fica perpendicular ao R1", () => {
    const carro = makeVehicle({ x: 200, y: 300 }, "V1", "sedan", 10);
    const cota = { ...makeMeasurement({ x: 200, y: 0 }, { x: 200, y: 300 }), ref_linha: "r1" as const };
    const ligada = { ...cota, ...ligarAoObjeto(cota, carro, 10) };
    const movido = { ...carro, x: 350, y: 420 };
    const out = resolverCotasVivas([r1, movido, ligada], 10);
    const c = out.find((o) => o.id === cota.id) as typeof cota;
    expect(c.p2).toEqual({ x: 350, y: 420 });
    expect(c.p1).toEqual({ x: 350, y: 0 });
  });
  it("sem mudança devolve a mesma lista", () => {
    const objs: SicroObject[] = [r1, makeMeasurement({ x: 1, y: 0 }, { x: 1, y: 5 })];
    expect(resolverCotasVivas(objs, 10)).toBe(objs);
  });
  it("acha o objeto sob o ponto", () => {
    const carro = makeVehicle({ x: 100, y: 100 }, "V1", "sedan", 10);
    expect(objetoSob([r1, carro], { x: 105, y: 102 }, 10)?.id).toBe(carro.id);
    expect(objetoSob([r1, carro], { x: 500, y: 500 }, 10)).toBeNull();
  });
});
