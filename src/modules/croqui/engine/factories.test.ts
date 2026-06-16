/**
 * Unit tests for the object factories — MVP 6.
 */

import { describe, expect, it } from "vitest";
import {
  cloneObject,
  makeLine,
  makeMarker,
  makeMeasurement,
  makeText,
  makeVehicle,
} from "./factories";

describe("makeVehicle", () => {
  it("defaults to car body type when omitted", () => {
    const v = makeVehicle({ x: 10, y: 10 });
    expect(v.kind).toBe("vehicle");
    expect(v.body_type).toBe("car");
    expect(v.label).toBe("V1");
    expect(v.visible).toBe(true);
    expect(v.locked).toBe(false);
    expect(v.category).toBe("veiculos");
  });
  it("uses preset dimensions for each subtype", () => {
    const sedan = makeVehicle({ x: 0, y: 0 }, "Sedan A", "sedan");
    const suv = makeVehicle({ x: 0, y: 0 }, "SUV A", "suv");
    const truck = makeVehicle({ x: 0, y: 0 }, "Caminhão", "caminhao");
    const moto = makeVehicle({ x: 0, y: 0 }, "Moto", "moto");
    expect(sedan.width).toBeGreaterThan(0);
    expect(suv.width).toBeGreaterThanOrEqual(sedan.width);
    expect(truck.width).toBeGreaterThan(suv.width);
    expect(moto.width).toBeLessThan(sedan.width);
    expect(sedan.color).not.toBe(truck.color);
  });
});

describe("makeLine", () => {
  it("uses dashed style for R1/R2", () => {
    const r1 = makeLine({ x: 0, y: 0 }, { x: 100, y: 0 }, "r1");
    const r2 = makeLine({ x: 0, y: 0 }, { x: 100, y: 0 }, "r2");
    expect(r1.dashed).toBe(true);
    expect(r2.dashed).toBe(true);
    expect(r1.label).toBe("R1");
    expect(r2.label).toBe("R2");
    expect(r1.color).not.toBe(r2.color);
    expect(r1.category).toBe("referenciais");
    expect(r2.category).toBe("referenciais");
  });
  it("places roads in `vias` category", () => {
    expect(makeLine({ x: 0, y: 0 }, { x: 10, y: 0 }, "road").category).toBe(
      "vias",
    );
    expect(
      makeLine({ x: 0, y: 0 }, { x: 10, y: 0 }, "lane_separator").category,
    ).toBe("vias");
    expect(makeLine({ x: 0, y: 0 }, { x: 10, y: 0 }, "arrow").category).toBe(
      "vias",
    );
  });
});

describe("makeMarker", () => {
  it("uses palette per subtype", () => {
    const x = makeMarker({ x: 0, y: 0 }, "collision_x");
    const brake = makeMarker({ x: 0, y: 0 }, "brake_mark");
    const fluid = makeMarker({ x: 0, y: 0 }, "fluid");
    const body = makeMarker({ x: 0, y: 0 }, "body");
    expect(x.label).toBe("X");
    expect(brake.label).toBe("Frenagem");
    expect(fluid.label).toBe("Fluido");
    expect(body.label).toBe("Vítima");
    expect(brake.size).toBeGreaterThan(x.size);
    expect(x.category).toBe("vestigios");
  });
  it("accepts a label override", () => {
    const m = makeMarker({ x: 0, y: 0 }, "collision_x", "Ponto de impacto");
    expect(m.label).toBe("Ponto de impacto");
  });
});

describe("makeText", () => {
  it("sets the text and defaults", () => {
    const t = makeText({ x: 0, y: 0 }, "Velocidade: 60 km/h");
    expect(t.text).toBe("Velocidade: 60 km/h");
    expect(t.font_size).toBe(16);
    expect(t.category).toBe("anotacoes");
  });
});

describe("makeMeasurement", () => {
  it("stores p1 and p2 and marks category as medidas", () => {
    const m = makeMeasurement({ x: 1, y: 2 }, { x: 10, y: 20 });
    expect(m.p1).toEqual({ x: 1, y: 2 });
    expect(m.p2).toEqual({ x: 10, y: 20 });
    expect(m.category).toBe("medidas");
  });
});

describe("cloneObject", () => {
  it("generates a new id and nudges position for vehicles", () => {
    const v = makeVehicle({ x: 100, y: 100 }, "V1", "sedan");
    const c = cloneObject(v);
    expect(c.id).not.toBe(v.id);
    expect(c.x).toBe(116);
    expect(c.y).toBe(116);
    expect(c.body_type).toBe("sedan");
  });
  it("translates measurement points", () => {
    const m = makeMeasurement({ x: 0, y: 0 }, { x: 100, y: 0 });
    const c = cloneObject(m);
    expect(c.id).not.toBe(m.id);
    expect(c.p1).toEqual({ x: 16, y: 16 });
    expect(c.p2).toEqual({ x: 116, y: 16 });
  });
  it("translates line points", () => {
    const l = makeLine({ x: 0, y: 0 }, { x: 100, y: 0 }, "road");
    const c = cloneObject(l);
    expect(c.id).not.toBe(l.id);
    expect(c.points).toEqual([16, 16, 116, 16]);
  });
});
