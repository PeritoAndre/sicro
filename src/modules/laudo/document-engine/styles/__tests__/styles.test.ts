/**
 * Sistema de estilos documentais — testes do catálogo + helpers puros.
 *
 * NÃO testa a extension TipTap (requer DOM real). Os testes integrados
 * de aplicação ficam para a validação manual em F4.
 */

import { describe, expect, it } from "vitest";
import {
  LAUDO_STYLES,
  LAUDO_STYLES_BY_ID,
  findLaudoStyle,
  laudoStylesByCategory,
  type LaudoStyleId,
} from "../definitions";

describe("Sistema de estilos — catálogo", () => {
  it("LAUDO_STYLES tem 14 entradas (6 estrutura + 8 pericial)", () => {
    expect(LAUDO_STYLES.length).toBe(14);
  });

  it("todos os IDs são únicos", () => {
    const ids = LAUDO_STYLES.map((s) => s.id);
    const unique = new Set(ids);
    expect(unique.size).toBe(ids.length);
  });

  it("todos os estilos têm label, description, target e category", () => {
    for (const def of LAUDO_STYLES) {
      expect(def.label).toBeTruthy();
      expect(def.description).toBeTruthy();
      expect(def.target).toBeDefined();
      expect(["estrutura", "pericial"]).toContain(def.category);
    }
  });

  it("LAUDO_STYLES_BY_ID indexa todos os estilos", () => {
    for (const def of LAUDO_STYLES) {
      expect(LAUDO_STYLES_BY_ID.get(def.id)).toBe(def);
    }
  });

  it("estilos de heading têm level 1, 2 ou 3", () => {
    for (const def of LAUDO_STYLES) {
      if (def.target.kind === "heading") {
        expect([1, 2, 3]).toContain(def.target.level);
      }
    }
  });

  it("inclui presets críticos do laudo pericial", () => {
    const ids = new Set(LAUDO_STYLES.map((s) => s.id));
    expect(ids.has("quesito")).toBe(true);
    expect(ids.has("resposta")).toBe(true);
    expect(ids.has("conclusao")).toBe(true);
    expect(ids.has("advertencia")).toBe(true);
    expect(ids.has("legenda")).toBe(true);
    expect(ids.has("citacao")).toBe(true);
    expect(ids.has("observacao")).toBe(true);
    expect(ids.has("assinatura")).toBe(true);
  });
});

describe("findLaudoStyle", () => {
  it("retorna a definição quando id é válido", () => {
    const def = findLaudoStyle("quesito");
    expect(def).not.toBeNull();
    expect(def?.id).toBe("quesito");
    expect(def?.category).toBe("pericial");
  });

  it("retorna null para id desconhecido", () => {
    expect(findLaudoStyle("xpto")).toBeNull();
    expect(findLaudoStyle(null)).toBeNull();
    expect(findLaudoStyle(undefined)).toBeNull();
    expect(findLaudoStyle("")).toBeNull();
  });
});

describe("laudoStylesByCategory", () => {
  it("agrupa estrutura e pericial separadamente", () => {
    const grouped = laudoStylesByCategory();
    expect(grouped.estrutura.length).toBeGreaterThan(0);
    expect(grouped.pericial.length).toBeGreaterThan(0);
    expect(
      grouped.estrutura.length + grouped.pericial.length,
    ).toBe(LAUDO_STYLES.length);
  });

  it("nenhum estilo aparece em duas categorias", () => {
    const grouped = laudoStylesByCategory();
    const ids1 = new Set(grouped.estrutura.map((s) => s.id));
    const ids2 = new Set(grouped.pericial.map((s) => s.id));
    for (const id of ids1) expect(ids2.has(id)).toBe(false);
  });

  it("estrutura inclui Normal + Títulos 1-3 + Subtítulo + Seção técnica", () => {
    const grouped = laudoStylesByCategory();
    const ids = grouped.estrutura.map((s) => s.id);
    const expected: LaudoStyleId[] = [
      "normal",
      "titulo_1",
      "titulo_2",
      "titulo_3",
      "subtitulo",
      "secao_tecnica",
    ];
    for (const id of expected) {
      expect(ids).toContain(id);
    }
  });
});
