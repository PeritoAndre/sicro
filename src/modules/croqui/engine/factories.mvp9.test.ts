/**
 * Tests for the MVP 9 additions to the Croqui factories.
 *
 * Covers:
 *   - new vehicle body subtypes (pickup/van/onibus/moto_esportiva/
 *     moto_carga/caminhao_pesado/carreta);
 *   - new marker subtypes (skid_curve/sulcagem/ranhura/impact_area/
 *     rest_position/semaforo/placa_pare/placa_preferencia/poste/
 *     arvore/guia/faixa_pedestre);
 *   - new line subtypes (canteiro/acostamento/trajetoria/callout);
 *   - category routing (mobiliário urbano).
 */

import { describe, expect, it } from "vitest";
import { makeLine, makeMarker, makeVehicle } from "./factories";

describe("MVP 9 — new vehicle body subtypes", () => {
  const newBodies = [
    "pickup",
    "van",
    "onibus",
    "moto_esportiva",
    "moto_carga",
    "caminhao_pesado",
    "carreta",
  ] as const;
  it.each(newBodies)("makeVehicle('%s') has expected preset", (body) => {
    const v = makeVehicle({ x: 0, y: 0 }, "V1", body);
    expect(v.body_type).toBe(body);
    expect(v.width).toBeGreaterThan(0);
    expect(v.height).toBeGreaterThan(0);
    expect(v.category).toBe("veiculos");
  });
  it("carreta is the longest of all new vehicles", () => {
    const lengths = newBodies.map(
      (b) => makeVehicle({ x: 0, y: 0 }, "V", b).width,
    );
    const carretaLength = makeVehicle({ x: 0, y: 0 }, "V", "carreta").width;
    expect(carretaLength).toBe(Math.max(...lengths));
  });
  it("moto_esportiva is narrower than caminhao_pesado", () => {
    const moto = makeVehicle({ x: 0, y: 0 }, "V", "moto_esportiva");
    const truck = makeVehicle({ x: 0, y: 0 }, "V", "caminhao_pesado");
    expect(moto.width).toBeLessThan(truck.width);
    expect(moto.height).toBeLessThan(truck.height);
  });
});

describe("MVP 9 — new marker subtypes", () => {
  const newVestigios = [
    "skid_curve",
    "sulcagem",
    "ranhura",
    "impact_area",
    "rest_position",
  ] as const;
  it.each(newVestigios)("makeMarker('%s') routes to vestigios", (kind) => {
    const m = makeMarker({ x: 0, y: 0 }, kind);
    expect(m.subtype).toBe(kind);
    expect(m.category).toBe("vestigios");
    expect(m.label).toBeTruthy();
    expect(m.size).toBeGreaterThan(0);
  });

  const mobiliario = [
    "semaforo",
    "placa_pare",
    "placa_preferencia",
    "poste",
    "arvore",
    "guia",
    "faixa_pedestre",
  ] as const;
  it.each(mobiliario)(
    "makeMarker('%s') routes to mobiliario_urbano",
    (kind) => {
      const m = makeMarker({ x: 0, y: 0 }, kind);
      expect(m.subtype).toBe(kind);
      expect(m.category).toBe("mobiliario_urbano");
    },
  );

  it("impact_area defaults to a large size", () => {
    const m = makeMarker({ x: 0, y: 0 }, "impact_area");
    expect(m.size).toBeGreaterThanOrEqual(60);
  });
});

describe("MVP 9 — new line subtypes", () => {
  it("canteiro is categorised as vias and uses green palette", () => {
    const c = makeLine({ x: 0, y: 0 }, { x: 1, y: 0 }, "canteiro");
    expect(c.category).toBe("vias");
    expect(c.color).toMatch(/^#22c55e$/i);
    expect(c.stroke_width).toBeGreaterThan(4);
  });
  it("trajetoria is dashed and lives in vias", () => {
    const t = makeLine({ x: 0, y: 0 }, { x: 1, y: 0 }, "trajetoria");
    expect(t.category).toBe("vias");
    expect(t.dashed).toBe(true);
  });
  it("callout goes into anotacoes", () => {
    const c = makeLine({ x: 0, y: 0 }, { x: 1, y: 0 }, "callout");
    expect(c.category).toBe("anotacoes");
    expect(c.dashed).toBe(true);
  });
  it("acostamento is categorised as vias", () => {
    const a = makeLine({ x: 0, y: 0 }, { x: 1, y: 0 }, "acostamento");
    expect(a.category).toBe("vias");
  });
});
