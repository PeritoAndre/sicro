/**
 * Unit tests for road templates — MVP 6.
 *
 * Fase S clean cut — Os templates `via_pro_*` (que dependiam de
 * `makeRoad` + `SicroRoadObject` v1) foram removidos. Os templates
 * remanescentes são todos line-based (emitem `SicroLineObject`).
 */

import { describe, expect, it } from "vitest";
import {
  findTemplate,
  TEMPLATES,
  TOOLBAR_TEMPLATES,
  type TemplateId,
} from "./templates";

describe("TEMPLATES registry", () => {
  it("exposes the canonical MVP 6 templates + MVP 9 extensions", () => {
    const ids = Object.keys(TEMPLATES).sort();
    // MVP 6 baseline — devem continuar existindo.
    const mvp6 = [
      "cruzamento_t",
      "cruzamento_x",
      "curva_simples",
      "mao_dupla",
      "mao_unica",
      "rotatoria_simples",
      "via_reta",
    ];
    for (const id of mvp6) {
      expect(ids).toContain(id);
    }
    // Sanity: MVP 9 introduziu pelo menos um template novo.
    expect(ids.length).toBeGreaterThan(mvp6.length);
  });

  it.each(Object.keys(TEMPLATES) as TemplateId[])(
    "%s — produces editable line objects, no id-collision",
    (id) => {
      const tpl = findTemplate(id)!;
      const objs = tpl.build({ x: 200, y: 200 });
      expect(objs.length).toBeGreaterThan(0);
      const ids = new Set(objs.map((o) => o.id));
      expect(ids.size).toBe(objs.length);
      for (const o of objs) {
        // Fase S — templates emitem apenas `line` (line-based).
        // Vias parity são criadas pela ferramenta Criar Via, não por
        // templates.
        expect(o.kind).toBe("line");
        expect(o.category).toBeDefined();
      }
    },
  );

  it("via_reta yields exactly three lines (two edges + separator)", () => {
    const objs = findTemplate("via_reta")!.build({ x: 100, y: 100 });
    expect(objs.length).toBe(3);
  });

  it("cruzamento_x yields six lines (two roads × 3 lines)", () => {
    const objs = findTemplate("cruzamento_x")!.build({ x: 0, y: 0 });
    expect(objs.length).toBe(6);
  });

  it("mao_unica includes an arrow line", () => {
    const objs = findTemplate("mao_unica")!.build({ x: 0, y: 0 });
    const hasArrow = objs.some(
      (o) => o.kind === "line" && o.subtype === "arrow",
    );
    expect(hasArrow).toBe(true);
  });

  it("rotatoria_simples produces 8 segments", () => {
    const objs = findTemplate("rotatoria_simples")!.build({ x: 0, y: 0 });
    expect(objs.length).toBe(8);
  });

  it("curva_simples produces 16 segments (8 outer + 8 inner)", () => {
    const objs = findTemplate("curva_simples")!.build({ x: 0, y: 0 });
    expect(objs.length).toBe(16);
  });
});

describe("findTemplate", () => {
  it("returns undefined for unknown ids", () => {
    expect(findTemplate("nope" as TemplateId)).toBeUndefined();
  });
});

describe("TOOLBAR_TEMPLATES (Fase S — line-based only)", () => {
  it("every entry resolves through findTemplate", () => {
    for (const id of TOOLBAR_TEMPLATES) {
      expect(findTemplate(id)).toBeDefined();
    }
  });

  it("every TOOLBAR_TEMPLATES entry emits only line objects", () => {
    for (const id of TOOLBAR_TEMPLATES) {
      const tpl = findTemplate(id);
      expect(tpl).toBeDefined();
      const objs = tpl!.build({ x: 100, y: 100 });
      expect(objs.length).toBeGreaterThan(0);
      for (const o of objs) {
        expect(o.kind).toBe("line");
      }
    }
  });

  it("contains the staple presets (via_reta, cruzamento_x, etc.)", () => {
    expect(TOOLBAR_TEMPLATES).toContain("via_reta");
    expect(TOOLBAR_TEMPLATES).toContain("cruzamento_x");
    expect(TOOLBAR_TEMPLATES).toContain("rotatoria_simples");
  });
});
