/**
 * Testes da numeração de blocos no render (numberFigures).
 *
 * IMPORTANTE: FIGURAS NÃO SÃO MAIS NUMERADAS automaticamente — a legenda da
 * foto/croqui é digitada manualmente pelo perito. `numberFigures` deixa o nó
 * `figure` (e seu figcaption) INTACTO. Apenas TABELAS recebem "Tabela N — "
 * no attr `caption`. Estes testes cobrem: tabelas numeradas sequencialmente,
 * idempotência (não duplica prefixo), preservação do texto livre da legenda,
 * e a GARANTIA de que figuras passam intactas.
 */

import { describe, expect, it } from "vitest";
import type { JSONContent } from "@tiptap/core";
import { numberFigures } from "../numbering";

function doc(...children: JSONContent[]): JSONContent {
  return { type: "doc", content: children };
}

function table(caption?: string): JSONContent {
  return {
    type: "table",
    attrs: caption !== undefined ? { caption } : {},
    content: [
      {
        type: "tableRow",
        content: [
          { type: "tableCell", content: [{ type: "paragraph" }] },
        ],
      },
    ],
  };
}

function figure(caption: string, kind = "image"): JSONContent {
  return {
    type: "figure",
    attrs: { kind },
    content: [
      { type: "figcaption", content: [{ type: "text", text: caption }] },
    ],
  };
}

function captionOf(node: JSONContent): string {
  return String((node.attrs as { caption?: string } | undefined)?.caption ?? "");
}

describe("numberFigures — tabelas", () => {
  it("prefixa 'Tabela N — ' sequencialmente nas legendas", () => {
    const out = numberFigures(
      doc(table("Dados do local"), table("Medições")),
    );
    const tables = out.content!.filter((n) => n.type === "table");
    expect(captionOf(tables[0]!)).toBe("Tabela 1 — Dados do local");
    expect(captionOf(tables[1]!)).toBe("Tabela 2 — Medições");
  });

  it("tabela sem legenda recebe só o rótulo 'Tabela N'", () => {
    const out = numberFigures(doc(table()));
    const t = out.content!.find((n) => n.type === "table")!;
    expect(captionOf(t)).toBe("Tabela 1");
  });

  it("não duplica o prefixo se já estiver presente (idempotente)", () => {
    const out = numberFigures(doc(table("Tabela 1 — Já numerada")));
    const t = out.content!.find((n) => n.type === "table")!;
    expect(captionOf(t)).toBe("Tabela 1 — Já numerada");
  });

  it("NÃO numera figuras, mas numera tabelas intercaladas normalmente", () => {
    const out = numberFigures(
      doc(figure("Foto A"), table("Tab A"), figure("Foto B"), table("Tab B")),
    );
    const figs = out.content!.filter((n) => n.type === "figure");
    const tables = out.content!.filter((n) => n.type === "table");
    // Figuras: legenda passa INTACTA — sem prefixo "Figura N — ".
    const figText = (f: JSONContent) =>
      (f.content?.[0]?.content ?? [])
        .map((t) => String(t.text ?? ""))
        .join("");
    expect(figText(figs[0]!)).toBe("Foto A");
    expect(figText(figs[1]!)).toBe("Foto B");
    // Tabelas: continuam numeradas 1, 2 (as figuras não consomem ordinal).
    expect(captionOf(tables[0]!)).toBe("Tabela 1 — Tab A");
    expect(captionOf(tables[1]!)).toBe("Tabela 2 — Tab B");
  });

  it("figura (image ou croqui) passa INTACTA — figcaption não é tocado", () => {
    const out = numberFigures(
      doc(figure("Vista geral"), figure("Esquema", "croqui")),
    );
    const figs = out.content!.filter((n) => n.type === "figure");
    const figText = (f: JSONContent) =>
      (f.content?.[0]?.content ?? [])
        .map((t) => String(t.text ?? ""))
        .join("");
    // Nenhum prefixo "Figura N — " / "Croqui N — " é prependido.
    expect(figText(figs[0]!)).toBe("Vista geral");
    expect(figText(figs[1]!)).toBe("Esquema");
    // O figcaption permanece com exatamente um text node (não foi inserido
    // um node de prefixo na frente).
    expect(figs[0]!.content?.[0]?.content).toHaveLength(1);
    expect(figs[1]!.content?.[0]?.content).toHaveLength(1);
  });

  it("preserva a estrutura das linhas (não desce na tabela)", () => {
    const out = numberFigures(doc(table("X")));
    const t = out.content!.find((n) => n.type === "table")!;
    expect(t.content).toHaveLength(1);
    expect(t.content![0]!.type).toBe("tableRow");
  });

  // F4.1 — legenda removível (estilo Word): captionVisible=false não numera,
  // não consome ordinal, e o caption é ZERADO no JSON exportado (defesa pro
  // DOCX, que lê o attr direto).
  it("tabela com legenda removida (captionVisible=false) não numera nem consome ordinal", () => {
    const hidden: JSONContent = {
      ...table("Texto órfão que não deve exportar"),
      attrs: { caption: "Texto órfão que não deve exportar", captionVisible: false },
    };
    const out = numberFigures(doc(table("Antes"), hidden, table("Depois")));
    const tables = out.content!.filter((n) => n.type === "table");
    expect(captionOf(tables[0]!)).toBe("Tabela 1 — Antes");
    // A escondida sai com caption vazia (nada vaza pro export)…
    expect(captionOf(tables[1]!)).toBe("");
    // …e a seguinte continua a sequência sem pular número.
    expect(captionOf(tables[2]!)).toBe("Tabela 2 — Depois");
  });

  it("captionVisible=true (ou ausente) mantém o comportamento atual", () => {
    const explicit: JSONContent = {
      ...table("Visível"),
      attrs: { caption: "Visível", captionVisible: true },
    };
    const out = numberFigures(doc(explicit));
    const t = out.content!.find((n) => n.type === "table")!;
    expect(captionOf(t)).toBe("Tabela 1 — Visível");
  });

  it("ignora tabelas de layout (borderStyle 'none') na numeração", () => {
    const layout: JSONContent = {
      type: "table",
      attrs: { borderStyle: "none", caption: "" },
      content: [
        {
          type: "tableRow",
          content: [{ type: "tableCell", content: [{ type: "paragraph" }] }],
        },
      ],
    };
    const out = numberFigures(doc(layout, table("Real")));
    const tables = out.content!.filter((n) => n.type === "table");
    // A tabela de layout NÃO recebe legenda automática…
    expect(captionOf(tables[0]!)).toBe("");
    // …e a tabela real começa em 1 (não 2).
    expect(captionOf(tables[1]!)).toBe("Tabela 1 — Real");
  });
});
