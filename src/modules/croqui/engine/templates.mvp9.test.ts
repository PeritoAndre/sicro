/**
 * Tests for the MVP 9 advanced road templates.
 */

import { describe, expect, it } from "vitest";
import { findTemplate, TEMPLATES, type TemplateId } from "./templates";

describe("MVP 9 — advanced road templates", () => {
  const newIds: TemplateId[] = [
    "avenida_canteiro",
    "cruzamento_y",
    "curva_esquerda",
    "curva_direita",
    "faixa_pedestre_via",
    "via_acostamento",
  ];

  it.each(newIds)("registry exposes '%s'", (id) => {
    expect(findTemplate(id)).toBeDefined();
    expect(TEMPLATES[id].label).toBeTruthy();
  });

  it("avenida_canteiro inserts at least 7 objects including a canteiro line", () => {
    const objs = findTemplate("avenida_canteiro")!.build({ x: 200, y: 200 });
    expect(objs.length).toBeGreaterThanOrEqual(7);
    expect(
      objs.some(
        (o) => o.kind === "line" && o.subtype === "canteiro",
      ),
    ).toBe(true);
  });

  it("cruzamento_y produces vertical-trunk + two diagonal arms", () => {
    const objs = findTemplate("cruzamento_y")!.build({ x: 0, y: 0 });
    // 3 (vertical) + 2 (left arm) + 2 (right arm) = 7
    expect(objs.length).toBe(7);
  });

  it("curva_esquerda and curva_direita have the same object count", () => {
    const l = findTemplate("curva_esquerda")!.build({ x: 0, y: 0 });
    const r = findTemplate("curva_direita")!.build({ x: 0, y: 0 });
    expect(l.length).toBe(r.length);
    expect(l.length).toBe(16);
  });

  it("faixa_pedestre_via emits the 3 roadlines + 6 stripe lines", () => {
    const objs = findTemplate("faixa_pedestre_via")!.build({ x: 0, y: 0 });
    expect(objs.length).toBe(9);
    const stripes = objs.filter(
      (o) => o.kind === "line" && o.subtype === "sidewalk",
    );
    expect(stripes.length).toBe(6);
  });

  it("via_acostamento includes 2 acostamento lines", () => {
    const objs = findTemplate("via_acostamento")!.build({ x: 0, y: 0 });
    const acost = objs.filter(
      (o) => o.kind === "line" && o.subtype === "acostamento",
    );
    expect(acost.length).toBe(2);
  });

  it("every new template gives unique ids", () => {
    for (const id of newIds) {
      const objs = findTemplate(id)!.build({ x: 0, y: 0 });
      const ids = new Set(objs.map((o) => o.id));
      expect(ids.size).toBe(objs.length);
    }
  });
});
