import { describe, expect, it } from "vitest";
import { fatorDaFolha } from "./tamanhos";

describe("fatorDaFolha", () => {
  it("folha pequena vale 1", () => {
    expect(fatorDaFolha({ width_px: 1000, height_px: 700 })).toBe(1);
    expect(fatorDaFolha({ width_px: 400, height_px: 300 })).toBe(1);
  });
  it("cresce com o lado maior e satura em 10", () => {
    expect(fatorDaFolha({ width_px: 1600, height_px: 1000 })).toBeCloseTo(1.6, 2);
    expect(fatorDaFolha({ width_px: 3648, height_px: 5472 })).toBeCloseTo(5.47, 2);
    expect(fatorDaFolha({ width_px: 100000, height_px: 10 })).toBe(10);
  });
});
