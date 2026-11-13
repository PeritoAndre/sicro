import { describe, expect, it } from "vitest";
import { fatorDaFolha } from "./tamanhos";

describe("fatorDaFolha", () => {
  it("folha padrão e menores valem 1", () => {
    expect(fatorDaFolha({ width_px: 1600, height_px: 1000 })).toBe(1);
    expect(fatorDaFolha({ width_px: 400, height_px: 300 })).toBe(1);
  });
  it("cresce com o lado maior e satura em 8", () => {
    expect(fatorDaFolha({ width_px: 3648, height_px: 5472 })).toBeCloseTo(3.42, 2);
    expect(fatorDaFolha({ width_px: 100000, height_px: 10 })).toBe(8);
  });
});
