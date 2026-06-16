/**
 * Testes do subsistema de tabelas (F7).
 *
 * Cobre:
 *   - Catálogo de templates (10 entradas, todos com build válido).
 *   - findTableTemplate: lookup correto + fallback null.
 *   - extractTables: extrai com posições monotônicas.
 *   - buildTableList: numeração sequencial.
 *   - Estrutura: cada template gera type="table" + tableRow + cells.
 */

import { describe, expect, it } from "vitest";
import type { JSONContent } from "@tiptap/core";
import {
  buildTableList,
  extractTables,
  findTableTemplate,
  TABLE_TEMPLATES,
  TABLE_TEMPLATES_BY_ID,
} from "../index";

describe("TABLE_TEMPLATES catálogo", () => {
  // F7.1: simples_3x3 foi removido em favor do InsertTableDialog (N×M
  // custom). Restaram 9 templates periciais.
  it("contém pelo menos 9 templates periciais", () => {
    expect(TABLE_TEMPLATES.length).toBeGreaterThanOrEqual(9);
  });

  it("todos os IDs são únicos", () => {
    const ids = TABLE_TEMPLATES.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("inclui os templates periciais essenciais", () => {
    const ids = new Set(TABLE_TEMPLATES.map((t) => t.id));
    const expected = [
      "dados_local",
      "dados_veiculos",
      "dados_envolvidos",
      "vestigios",
      "medicoes",
      "cronologia",
      "materiais_examinados",
      "condicoes_ambientais",
      "midias_video",
    ];
    for (const id of expected) {
      expect(ids.has(id)).toBe(true);
    }
  });

  it("não inclui mais o simples_3x3 (substituído por InsertTableDialog)", () => {
    const ids = new Set(TABLE_TEMPLATES.map((t) => t.id));
    expect(ids.has("simples_3x3")).toBe(false);
  });

  it("todos têm label, description, build válidos", () => {
    for (const t of TABLE_TEMPLATES) {
      expect(t.label).toBeTruthy();
      expect(t.description).toBeTruthy();
      expect(typeof t.build).toBe("function");
    }
  });

  it("TABLE_TEMPLATES_BY_ID indexa todos", () => {
    for (const t of TABLE_TEMPLATES) {
      expect(TABLE_TEMPLATES_BY_ID.get(t.id)).toBe(t);
    }
  });
});

describe("findTableTemplate", () => {
  it("retorna o template quando ID existe", () => {
    expect(findTableTemplate("dados_local")?.id).toBe("dados_local");
  });

  it("retorna null para ID inválido", () => {
    expect(findTableTemplate("xpto_nao_existe")).toBeNull();
    expect(findTableTemplate(null)).toBeNull();
    expect(findTableTemplate("")).toBeNull();
  });
});

describe("Templates — estrutura do build()", () => {
  for (const t of TABLE_TEMPLATES) {
    it(`${t.id}: build() retorna table com rows`, () => {
      const json = t.build();
      expect(json.type).toBe("table");
      expect(Array.isArray(json.content)).toBe(true);
      expect(json.content!.length).toBeGreaterThan(0);
      // Primeira linha precisa ter pelo menos 1 célula.
      const firstRow = json.content![0]!;
      expect(firstRow.type).toBe("tableRow");
      expect((firstRow.content ?? []).length).toBeGreaterThan(0);
    });
  }

  it("dados_local tem cabeçalho 'Campo' / 'Valor'", () => {
    const json = findTableTemplate("dados_local")!.build();
    const firstRow = json.content![0]!;
    const cells = firstRow.content ?? [];
    expect(cells[0]?.type).toBe("tableHeader");
    const firstCellText = (cells[0]?.content?.[0]?.content?.[0]?.text) ?? "";
    expect(firstCellText).toBe("Campo");
  });
});

// ---------------------------------------------------------------------------
// extractTables + buildTableList

function doc(...children: JSONContent[]): JSONContent {
  return { type: "doc", content: children };
}

function table(rows: number, cols: number, firstCell = ""): JSONContent {
  return {
    type: "table",
    content: Array.from({ length: rows }, (_, r) => ({
      type: "tableRow",
      content: Array.from({ length: cols }, (_, c) => ({
        type: r === 0 ? "tableHeader" : "tableCell",
        content: [
          {
            type: "paragraph",
            content:
              r === 0 && c === 0 && firstCell
                ? [{ type: "text", text: firstCell }]
                : [],
          },
        ],
      })),
    })),
  };
}

function paragraph(text: string): JSONContent {
  return { type: "paragraph", content: [{ type: "text", text }] };
}

describe("extractTables", () => {
  it("documento vazio → lista vazia", () => {
    expect(extractTables(doc())).toEqual([]);
  });

  it("conta tabelas + linhas + colunas corretamente", () => {
    const out = extractTables(
      doc(table(3, 4, "Campo"), paragraph("texto"), table(5, 2, "Hora")),
    );
    expect(out).toHaveLength(2);
    expect(out[0]!.rowCount).toBe(3);
    expect(out[0]!.colCount).toBe(4);
    expect(out[0]!.firstCell).toBe("Campo");
    expect(out[1]!.rowCount).toBe(5);
    expect(out[1]!.colCount).toBe(2);
    expect(out[1]!.firstCell).toBe("Hora");
  });

  it("posições monotônicas crescentes", () => {
    const out = extractTables(
      doc(table(2, 2, "A"), table(2, 2, "B"), table(2, 2, "C")),
    );
    expect(out[1]!.pos).toBeGreaterThan(out[0]!.pos);
    expect(out[2]!.pos).toBeGreaterThan(out[1]!.pos);
  });
});

describe("buildTableList", () => {
  it("numera sequencialmente", () => {
    const raw = extractTables(
      doc(table(2, 2), table(3, 3), table(2, 4)),
    );
    const list = buildTableList(raw);
    expect(list.map((t) => t.label)).toEqual([
      "Tabela 1",
      "Tabela 2",
      "Tabela 3",
    ]);
    expect(list[0]!.ordinal).toBe(1);
    expect(list[2]!.ordinal).toBe(3);
  });

  it("lista vazia → lista vazia", () => {
    expect(buildTableList([])).toEqual([]);
  });
});
