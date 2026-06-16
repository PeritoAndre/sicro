/**
 * useZoom — testes do clamp + cálculo de fit.
 *
 * O hook usa `useState` do React; testamos só a função de clamp pura
 * + a lógica de fit (que é determinística dado containerPx + pageWidthCm).
 * Componentes integrados ficam cobertos por F3 visual.
 */

import { describe, expect, it } from "vitest";
import { PX_PER_CM_96DPI, ZOOM_MAX, ZOOM_MIN } from "../useZoom";

function clamp(v: number): number {
  if (v < ZOOM_MIN) return ZOOM_MIN;
  if (v > ZOOM_MAX) return ZOOM_MAX;
  return Math.round(v * 100) / 100;
}

describe("useZoom — limites de zoom", () => {
  it("clamp respeita ZOOM_MIN (0.5)", () => {
    expect(clamp(0.4)).toBe(ZOOM_MIN);
    expect(clamp(0)).toBe(ZOOM_MIN);
    expect(clamp(-1)).toBe(ZOOM_MIN);
  });

  it("clamp respeita ZOOM_MAX (5.0)", () => {
    expect(clamp(5.5)).toBe(ZOOM_MAX);
    expect(clamp(100)).toBe(ZOOM_MAX);
  });

  it("valores entre 2.0 e 5.0 (faixa nova) NÃO são clampados", () => {
    expect(clamp(2.5)).toBe(2.5);
    expect(clamp(3.0)).toBe(3.0);
    expect(clamp(5.0)).toBe(5.0);
  });

  it("quantiza para 2 casas decimais (valores dentro do range)", () => {
    expect(clamp(0.654321)).toBe(0.65);
    expect(clamp(0.999999)).toBe(1.0);
    expect(clamp(1.234567)).toBe(1.23);
  });

  it("valores dentro do range são preservados (com clamp 0.01)", () => {
    expect(clamp(0.75)).toBe(0.75);
    expect(clamp(1.5)).toBe(1.5);
    expect(clamp(ZOOM_MIN)).toBe(ZOOM_MIN);
    expect(clamp(ZOOM_MAX)).toBe(ZOOM_MAX);
  });
});

describe("useZoom — constantes de unidade", () => {
  it("PX_PER_CM_96DPI é 37.7952756 (96 dpi)", () => {
    expect(PX_PER_CM_96DPI).toBeCloseTo(37.7952756, 4);
  });

  it("21cm A4 width = ~793 px a 96 DPI", () => {
    expect(21 * PX_PER_CM_96DPI).toBeCloseTo(793.7, 1);
  });

  it("29.7cm A4 height = ~1122 px a 96 DPI", () => {
    expect(29.7 * PX_PER_CM_96DPI).toBeCloseTo(1122.5, 1);
  });
});

describe("useZoom — cálculo de fitWidth", () => {
  // Reproduz a lógica de fitWidth sem instanciar o hook.
  function calcFit(containerPx: number, pageWidthCm: number): number {
    if (containerPx <= 0 || pageWidthCm <= 0) return 1;
    const pageWidthPx = pageWidthCm * PX_PER_CM_96DPI;
    const usablePx = Math.max(containerPx - 40, 100);
    return clamp(usablePx / pageWidthPx);
  }

  it("container amplo → preenche", () => {
    // Container 1200px, página A4 = 21cm → zoom ~ 1.46 → clamp 1.46
    const z = calcFit(1200, 21);
    expect(z).toBeGreaterThan(1.4);
    expect(z).toBeLessThanOrEqual(ZOOM_MAX);
  });

  it("container pequeno → encolhe", () => {
    // Container 400px → bem menor que 21cm A4 (793px) → zoom < 1.
    const z = calcFit(400, 21);
    expect(z).toBeLessThan(1);
    expect(z).toBeGreaterThanOrEqual(ZOOM_MIN);
  });

  it("container 0 → mantém 1 (defensivo)", () => {
    expect(calcFit(0, 21)).toBe(1);
  });

  it("paisagem (29.7cm) → ratio menor que retrato no mesmo container", () => {
    const portrait = calcFit(900, 21);
    const landscape = calcFit(900, 29.7);
    expect(landscape).toBeLessThan(portrait);
  });
});
