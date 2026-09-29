import { describe, expect, it } from "vitest";

import { UI_ZOOM_DEFAULT, applyUiZoom, clampUiZoom, getUiZoom, stepUiZoom } from "./uiZoom";

describe("uiZoom", () => {
  it("valor inválido ou ausente volta a 100%", () => {
    expect(clampUiZoom(undefined)).toBe(UI_ZOOM_DEFAULT);
    expect(clampUiZoom(Number.NaN)).toBe(UI_ZOOM_DEFAULT);
    expect(clampUiZoom(0)).toBe(UI_ZOOM_DEFAULT);
    expect(clampUiZoom("1.5")).toBe(UI_ZOOM_DEFAULT);
  });

  it("prende na faixa 75%–200%", () => {
    expect(clampUiZoom(0.1)).toBe(0.75);
    expect(clampUiZoom(9)).toBe(2);
    expect(clampUiZoom(1.25)).toBe(1.25);
  });

  it("anda pelos degraus e para nas pontas", () => {
    expect(stepUiZoom(1, 1)).toBe(1.1);
    expect(stepUiZoom(1.1, 1)).toBe(1.25);
    expect(stepUiZoom(1.25, -1)).toBe(1.1);
    expect(stepUiZoom(1.3, -1)).toBe(1.25); // fora dos degraus → vizinho de baixo
    expect(stepUiZoom(2, 1)).toBe(2);
    expect(stepUiZoom(0.75, -1)).toBe(0.75);
  });

  it("fora do Tauri só guarda o valor", () => {
    applyUiZoom(1.5);
    expect(getUiZoom()).toBe(1.5);
    applyUiZoom(UI_ZOOM_DEFAULT);
  });
});
