/**
 * U2 — Testes da direção do texto da TextBox (buildTextBoxContentStyle).
 *
 * Função pura → cobre os 3 estados sem precisar montar editor TipTap.
 * `vertical_up` é a marca lateral institucional (lê de baixo p/ cima):
 * writing-mode vertical-rl + rotate(180deg). `vertical_down` é só
 * vertical-rl. `horizontal` não toca em writing-mode/transform.
 */

import { describe, expect, it } from "vitest";
import { buildTextBoxContentStyle } from "../TextBox";

describe("buildTextBoxContentStyle (U2 — direção do texto)", () => {
  it("horizontal: sem writing-mode nem transform, ocupa 100%", () => {
    const s = buildTextBoxContentStyle("horizontal");
    expect(s).not.toContain("writing-mode");
    expect(s).not.toContain("transform");
    expect(s).toContain("width: 100%");
    expect(s).toContain("height: 100%");
  });

  it("vertical_up: lê de baixo p/ cima (vertical-rl + rotate 180deg)", () => {
    const s = buildTextBoxContentStyle("vertical_up");
    expect(s).toContain("writing-mode: vertical-rl");
    expect(s).toContain("transform: rotate(180deg)");
  });

  it("vertical_down: vertical-rl sem rotação (cima p/ baixo)", () => {
    const s = buildTextBoxContentStyle("vertical_down");
    expect(s).toContain("writing-mode: vertical-rl");
    expect(s).not.toContain("rotate");
  });
});
