/**
 * Testes da semeadura de larguras de coluna (F1.2).
 *
 * Lógica pura — sem DOM/TipTap. Cobre:
 *   - seedEqualColWidths: somatório exato, mínimo por coluna, 1 coluna.
 *   - seedWeightedColWidths: proporção, mínimo, somatório, pesos inválidos.
 *   - registrationBlockColWidths: 3 colunas decrescentes (larga/média/estreita).
 *   - buildSeededTableJson: estrutura + colwidth em cada célula + header.
 */

import { describe, expect, it } from "vitest";
import {
  buildSeededTableJson,
  DEFAULT_TABLE_CONTENT_WIDTH_PX,
  MIN_SEEDED_COL_WIDTH_PX,
  registrationBlockColWidths,
  seedEqualColWidths,
  seedWeightedColWidths,
} from "../tableDefaults";

describe("seedEqualColWidths", () => {
  it("divide igualmente e soma exatamente o total", () => {
    const w = seedEqualColWidths(4, 600);
    expect(w).toHaveLength(4);
    expect(w.reduce((a, b) => a + b, 0)).toBe(600);
  });

  it("usa o total padrão quando omitido", () => {
    const w = seedEqualColWidths(3);
    expect(w.reduce((a, b) => a + b, 0)).toBe(DEFAULT_TABLE_CONTENT_WIDTH_PX);
  });

  it("1 coluna recebe a largura inteira", () => {
    expect(seedEqualColWidths(1, 500)).toEqual([500]);
  });

  it("respeita o mínimo por coluna quando há muitas colunas", () => {
    const w = seedEqualColWidths(50, 100);
    for (const v of w) expect(v).toBeGreaterThanOrEqual(MIN_SEEDED_COL_WIDTH_PX);
  });

  it("cols <= 0 vira 1 coluna (defensivo)", () => {
    expect(seedEqualColWidths(0, 300)).toHaveLength(1);
  });
});

describe("seedWeightedColWidths", () => {
  it("distribui conforme os pesos (3:1 ⇒ col0 ~3× col1)", () => {
    const w = seedWeightedColWidths([3, 1], 800);
    expect(w).toHaveLength(2);
    expect(w.reduce((a, b) => a + b, 0)).toBe(800);
    // col0 deve ser claramente maior que col1.
    expect(w[0]!).toBeGreaterThan(w[1]!);
  });

  it("soma exatamente o total", () => {
    const w = seedWeightedColWidths([2, 3, 5], 1000);
    expect(w.reduce((a, b) => a + b, 0)).toBe(1000);
  });

  it("pesos inválidos (0/negativo/NaN) viram 1", () => {
    const w = seedWeightedColWidths([0, -2, Number.NaN], 300);
    expect(w).toHaveLength(3);
    // Como todos viram peso 1, ficam ~iguais.
    expect(Math.max(...w) - Math.min(...w)).toBeLessThanOrEqual(2);
  });

  it("lista vazia → array vazio", () => {
    expect(seedWeightedColWidths([])).toEqual([]);
  });
});

describe("registrationBlockColWidths", () => {
  it("retorna 3 colunas larga > média > estreita", () => {
    const w = registrationBlockColWidths();
    expect(w).toHaveLength(3);
    expect(w[0]!).toBeGreaterThan(w[1]!);
    expect(w[1]!).toBeGreaterThan(w[2]!);
  });
});

describe("buildSeededTableJson", () => {
  it("gera table com rows e colwidth em cada célula", () => {
    const t = buildSeededTableJson({ rows: 2, cols: 3, withHeaderRow: true });
    expect(t.type).toBe("table");
    expect(t.content).toHaveLength(2);
    // primeira linha = cabeçalho
    const firstRow = t.content![0]!;
    expect(firstRow.content![0]!.type).toBe("tableHeader");
    // segunda linha = células normais
    expect(t.content![1]!.content![0]!.type).toBe("tableCell");
    // toda célula tem colwidth array de 1 número
    for (const r of t.content!) {
      for (const c of r.content!) {
        const cw = (c.attrs as { colwidth?: number[] }).colwidth;
        expect(Array.isArray(cw)).toBe(true);
        expect(cw!).toHaveLength(1);
        expect(cw![0]!).toBeGreaterThanOrEqual(MIN_SEEDED_COL_WIDTH_PX);
      }
    }
  });

  it("withHeaderRow=false ⇒ primeira linha é tableCell", () => {
    const t = buildSeededTableJson({ rows: 2, cols: 2, withHeaderRow: false });
    expect(t.content![0]!.content![0]!.type).toBe("tableCell");
  });

  it("colwidths das colunas somam ~a largura padrão na primeira linha", () => {
    const t = buildSeededTableJson({ rows: 1, cols: 4, withHeaderRow: false });
    const sum = (t.content![0]!.content ?? []).reduce(
      (acc, c) => acc + ((c.attrs as { colwidth?: number[] }).colwidth?.[0] ?? 0),
      0,
    );
    expect(sum).toBe(DEFAULT_TABLE_CONTENT_WIDTH_PX);
  });
});
