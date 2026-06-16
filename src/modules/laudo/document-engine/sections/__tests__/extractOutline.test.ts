/**
 * extractOutline + numberOutline — testes da extração e numeração
 * automática de seções.
 */

import { describe, expect, it } from "vitest";
import type { JSONContent } from "@tiptap/core";
import { extractOutline } from "../extractOutline";
import { numberOutline } from "../numbering";

function doc(...children: JSONContent[]): JSONContent {
  return { type: "doc", content: children };
}

function heading(level: 1 | 2 | 3, text: string, laudoStyle?: string): JSONContent {
  const attrs: Record<string, unknown> = { level };
  if (laudoStyle) attrs["laudoStyle"] = laudoStyle;
  return {
    type: "heading",
    attrs,
    content: [{ type: "text", text }],
  };
}

function paragraph(text: string, laudoStyle?: string): JSONContent {
  const attrs: Record<string, unknown> = {};
  if (laudoStyle) attrs["laudoStyle"] = laudoStyle;
  return {
    type: "paragraph",
    attrs: Object.keys(attrs).length ? attrs : undefined,
    content: [{ type: "text", text }],
  };
}

describe("extractOutline — extração de headings", () => {
  it("documento vazio → outline vazio", () => {
    expect(extractOutline(doc())).toEqual([]);
  });

  it("captura headings nível 1, 2 e 3", () => {
    const out = extractOutline(
      doc(
        heading(1, "Laudo Pericial"),
        heading(2, "Histórico"),
        heading(3, "Detalhe"),
      ),
    );
    expect(out).toHaveLength(3);
    expect(out[0]!.level).toBe(1);
    expect(out[0]!.text).toBe("Laudo Pericial");
    expect(out[1]!.level).toBe(2);
    expect(out[2]!.level).toBe(3);
  });

  it("ignora parágrafos comuns", () => {
    const out = extractOutline(
      doc(
        heading(1, "Título"),
        paragraph("um parágrafo aleatório"),
        heading(2, "Seção"),
      ),
    );
    expect(out).toHaveLength(2);
  });

  it("inclui subtítulo (parágrafo com laudoStyle='subtitulo') como level 0", () => {
    const out = extractOutline(
      doc(
        heading(1, "Laudo Pericial"),
        paragraph("Sinistro de trânsito — Macapá/AP", "subtitulo"),
        heading(2, "Histórico"),
      ),
    );
    expect(out).toHaveLength(3);
    expect(out[1]!.level).toBe(0);
    expect(out[1]!.laudoStyle).toBe("subtitulo");
    expect(out[1]!.text).toBe("Sinistro de trânsito — Macapá/AP");
  });

  it("text é o conteúdo concatenado dos descendentes", () => {
    const out = extractOutline(
      doc({
        type: "heading",
        attrs: { level: 1 },
        content: [
          { type: "text", text: "Laudo " },
          { type: "text", text: "Pericial" },
        ],
      }),
    );
    expect(out[0]!.text).toBe("Laudo Pericial");
  });

  it("preserva atributo laudoStyle dos headings", () => {
    const out = extractOutline(
      doc(heading(1, "Título", "titulo_1"), heading(2, "Seção", "titulo_2")),
    );
    expect(out[0]!.laudoStyle).toBe("titulo_1");
    expect(out[1]!.laudoStyle).toBe("titulo_2");
  });

  it("posições são monótonas crescentes", () => {
    const out = extractOutline(
      doc(
        heading(1, "A"),
        heading(2, "B"),
        heading(3, "C"),
      ),
    );
    expect(out[1]!.pos).toBeGreaterThan(out[0]!.pos);
    expect(out[2]!.pos).toBeGreaterThan(out[1]!.pos);
  });
});

describe("numberOutline — numeração hierárquica automática", () => {
  it("subtítulos NÃO recebem número", () => {
    const numbered = numberOutline([
      { level: 1, text: "T", pos: 1 },
      { level: 0, text: "Sub", pos: 5, laudoStyle: "subtitulo" },
    ]);
    expect(numbered[0]!.numero).toBe("1");
    expect(numbered[1]!.numero).toBeNull();
  });

  it("nível 1 vira inteiro sequencial: 1, 2, 3", () => {
    const numbered = numberOutline([
      { level: 1, text: "A", pos: 1 },
      { level: 1, text: "B", pos: 2 },
      { level: 1, text: "C", pos: 3 },
    ]);
    expect(numbered.map((n) => n.numero)).toEqual(["1", "2", "3"]);
  });

  it("nível 2 segue como N.M sob o último N", () => {
    const numbered = numberOutline([
      { level: 1, text: "A", pos: 1 },
      { level: 2, text: "A1", pos: 2 },
      { level: 2, text: "A2", pos: 3 },
      { level: 1, text: "B", pos: 4 },
      { level: 2, text: "B1", pos: 5 },
    ]);
    expect(numbered.map((n) => n.numero)).toEqual([
      "1",
      "1.1",
      "1.2",
      "2",
      "2.1",
    ]);
  });

  it("nível 3 forma chains de três níveis: 1.1.1, 1.1.2, 1.2.1", () => {
    const numbered = numberOutline([
      { level: 1, text: "A", pos: 1 },
      { level: 2, text: "A1", pos: 2 },
      { level: 3, text: "A1a", pos: 3 },
      { level: 3, text: "A1b", pos: 4 },
      { level: 2, text: "A2", pos: 5 },
      { level: 3, text: "A2a", pos: 6 },
    ]);
    expect(numbered.map((n) => n.numero)).toEqual([
      "1",
      "1.1",
      "1.1.1",
      "1.1.2",
      "1.2",
      "1.2.1",
    ]);
  });

  it("contador de nível filho zera ao subir de nível", () => {
    const numbered = numberOutline([
      { level: 1, text: "A", pos: 1 },
      { level: 2, text: "A1", pos: 2 },
      { level: 2, text: "A2", pos: 3 },
      { level: 1, text: "B", pos: 4 },
      // O contador de level 2 deve voltar a "1" sob B.
      { level: 2, text: "B1", pos: 5 },
    ]);
    expect(numbered[4]!.numero).toBe("2.1");
  });

  it("documento começando em nível 2 numera como 1.1 (convenção pericial)", () => {
    const numbered = numberOutline([
      { level: 2, text: "Histórico", pos: 1 },
      { level: 2, text: "Exames", pos: 2 },
    ]);
    expect(numbered.map((n) => n.numero)).toEqual(["1.1", "1.2"]);
  });

  it("subtítulo intercalado entre headings não afeta a numeração", () => {
    const numbered = numberOutline([
      { level: 1, text: "Título", pos: 1 },
      { level: 0, text: "Subtítulo", pos: 2, laudoStyle: "subtitulo" },
      { level: 2, text: "Seção", pos: 3 },
    ]);
    expect(numbered[0]!.numero).toBe("1");
    expect(numbered[1]!.numero).toBeNull();
    expect(numbered[2]!.numero).toBe("1.1");
  });
});
