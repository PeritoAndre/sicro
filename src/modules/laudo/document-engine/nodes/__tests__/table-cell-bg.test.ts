/**
 * Cor de fundo da célula (attr `backgroundColor` de tableCell/tableHeader).
 *
 * Cobre a FIDELIDADE NO EXPORT pelo caminho real do renderer
 * (renderSicroDocToHtml → generateHTML → renderHTML do cell), que é o mesmo
 * pipeline do clone estático do cabeçalho/rodapé. Garante que:
 *   1. uma célula COM `backgroundColor` emite `background-color` + `data-cell-bg`
 *      no `<td>`/`<th>`;
 *   2. uma célula SEM cor (null / "transparent") NÃO emite background — o
 *      default é transparente (regra "tabelas recém-inseridas consistentes");
 *   3. docs antigos (sem o attr) abrem sem nenhum background.
 *
 * O parseHTML é exercido indiretamente: o attr é registrado no schema, então
 * `generateHTML` o serializa a partir do JSON.
 */

import { describe, expect, it } from "vitest";
import { renderSicroDocToHtml, type SicroDoc } from "../..";

function docWith(content: unknown[]): SicroDoc {
  return {
    document_id: "doc-1",
    occurrence_id: "occ-1",
    type: "laudo",
    title: "Laudo tabela bg",
    template_id: "documento_livre",
    created_at: "2026-06-06T00:00:00Z",
    updated_at: "2026-06-06T00:00:00Z",
    metadata: {},
    content: { type: "doc", content },
  } as unknown as SicroDoc;
}

function tableWith(cellAttrs: Record<string, unknown>): unknown {
  return {
    type: "table",
    attrs: { id: "tbl-test" },
    content: [
      {
        type: "tableRow",
        content: [
          {
            type: "tableCell",
            attrs: { colspan: 1, rowspan: 1, colwidth: [120], ...cellAttrs },
            content: [{ type: "paragraph", content: [{ type: "text", text: "X" }] }],
          },
        ],
      },
    ],
  };
}

describe("tableCell backgroundColor — serialização HTML/PDF", () => {
  it("célula com backgroundColor emite background-color + data-cell-bg", () => {
    const html = renderSicroDocToHtml(
      docWith([tableWith({ backgroundColor: "#ffcc00" })]),
      { fullDocument: false },
    );
    expect(html).toContain("background-color: #ffcc00");
    expect(html).toContain('data-cell-bg="#ffcc00"');
  });

  it("aceita rgb()/rgba() como valor", () => {
    const html = renderSicroDocToHtml(
      docWith([tableWith({ backgroundColor: "rgb(200, 220, 255)" })]),
      { fullDocument: false },
    );
    expect(html).toContain("background-color: rgb(200, 220, 255)");
  });

  it("célula SEM cor (null) não emite background — default transparente", () => {
    const html = renderSicroDocToHtml(
      docWith([tableWith({ backgroundColor: null })]),
      { fullDocument: false },
    );
    expect(html).not.toContain("data-cell-bg");
    expect(html).not.toContain("background-color");
  });

  it('"transparent"/"none" são tratados como sem cor', () => {
    const a = renderSicroDocToHtml(
      docWith([tableWith({ backgroundColor: "transparent" })]),
      { fullDocument: false },
    );
    const b = renderSicroDocToHtml(
      docWith([tableWith({ backgroundColor: "none" })]),
      { fullDocument: false },
    );
    expect(a).not.toContain("data-cell-bg");
    expect(b).not.toContain("data-cell-bg");
  });

  it("doc legado sem o attr abre sem background (aditivo)", () => {
    const html = renderSicroDocToHtml(docWith([tableWith({})]), {
      fullDocument: false,
    });
    expect(html).not.toContain("data-cell-bg");
  });

  it("tableHeader também serializa a cor de fundo", () => {
    const table = {
      type: "table",
      attrs: { id: "tbl-h" },
      content: [
        {
          type: "tableRow",
          content: [
            {
              type: "tableHeader",
              attrs: {
                colspan: 1,
                rowspan: 1,
                colwidth: [120],
                backgroundColor: "#1a2b3c",
              },
              content: [
                { type: "paragraph", content: [{ type: "text", text: "H" }] },
              ],
            },
          ],
        },
      ],
    };
    const html = renderSicroDocToHtml(docWith([table]), { fullDocument: false });
    expect(html).toContain("background-color: #1a2b3c");
    expect(html).toContain('data-cell-bg="#1a2b3c"');
  });
});
