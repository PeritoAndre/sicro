/**
 * Python Parity Engine — testes de type guards.
 */

import { describe, expect, it } from "vitest";
import {
  isParityObject,
  isParityRoad,
  isParityRoundabout,
} from "../guards";
import { makeParityRoad, makeParityRoundabout } from "../factories";

describe("road-parity / isParityRoad", () => {
  it("true para SicroRoadObject_parity recém criado", () => {
    expect(isParityRoad(makeParityRoad(0, 0, 10, 0))).toBe(true);
  });

  it("false para SicroRoadObject legado (kind = 'road' sem engine)", () => {
    const legacyRoad = {
      id: "leg",
      kind: "road" as const,
      points: [0, 0, 10, 0],
      width: 80,
    };
    expect(isParityRoad(legacyRoad as never)).toBe(false);
  });

  it("false para SicroRoundaboutObject_parity (kind diferente)", () => {
    expect(isParityRoad(makeParityRoundabout(0, 0, 10) as never)).toBe(false);
  });

  it("false para null / undefined / non-object", () => {
    expect(isParityRoad(null as never)).toBe(false);
    expect(isParityRoad(undefined as never)).toBe(false);
    expect(isParityRoad("string" as never)).toBe(false);
    expect(isParityRoad(42 as never)).toBe(false);
  });

  it("false para objeto sem kind", () => {
    expect(isParityRoad({} as never)).toBe(false);
  });
});

describe("road-parity / isParityRoundabout", () => {
  it("true para rotatória parity", () => {
    expect(isParityRoundabout(makeParityRoundabout(0, 0, 10))).toBe(true);
  });

  it("false para rotatória legada (kind = 'roundabout')", () => {
    const legacyRb = {
      id: "leg",
      kind: "roundabout" as const,
      cx: 0,
      cy: 0,
      r: 10,
      width: 7,
    };
    expect(isParityRoundabout(legacyRb as never)).toBe(false);
  });

  it("false para via parity", () => {
    expect(isParityRoundabout(makeParityRoad(0, 0, 10, 0) as never)).toBe(false);
  });
});

describe("road-parity / isParityObject", () => {
  it("true para qualquer objeto parity", () => {
    expect(isParityObject(makeParityRoad(0, 0, 10, 0))).toBe(true);
    expect(isParityObject(makeParityRoundabout(0, 0, 10))).toBe(true);
  });

  it("false para legados", () => {
    const legacyRoad = {
      id: "leg",
      kind: "road" as const,
      points: [0, 0, 10, 0],
    };
    const legacyRb = {
      id: "leg",
      kind: "roundabout" as const,
      cx: 0,
      cy: 0,
      r: 10,
    };
    expect(isParityObject(legacyRoad as never)).toBe(false);
    expect(isParityObject(legacyRb as never)).toBe(false);
  });

  it("false para tipos não-road/non-roundabout (veículo, marcador, etc.)", () => {
    const vehicle = { id: "v1", kind: "vehicle" as const };
    expect(isParityObject(vehicle as never)).toBe(false);
  });
});

describe("road-parity / type narrowing TypeScript", () => {
  it("dentro do if (isParityRoad), TS sabe que é SicroRoadObject_parity", () => {
    // Type starts as the union { engine?: unknown; kind?: unknown }
    // (what the guard accepts), narrows to SicroRoadObject_parity
    // inside the if branch.
    const obj: { engine?: unknown; kind?: unknown } = makeParityRoad(
      0,
      0,
      10,
      0,
      { label: "TEST" },
    );
    if (isParityRoad(obj)) {
      // Sem cast: TS deve permitir acesso direto aos campos parity.
      const widthM: number = obj.largura_m;
      const label = obj.label;
      expect(typeof widthM).toBe("number");
      expect(label).toBe("TEST");
    } else {
      throw new Error("Expected isParityRoad true");
    }
  });

  it("dentro do if (isParityRoundabout), TS sabe que é SicroRoundaboutObject_parity", () => {
    const obj: { engine?: unknown; kind?: unknown } = makeParityRoundabout(
      0,
      0,
      14,
    );
    if (isParityRoundabout(obj)) {
      const r: number = obj.r_m;
      expect(r).toBe(14);
    } else {
      throw new Error("Expected isParityRoundabout true");
    }
  });
});
