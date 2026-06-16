/**
 * Fórmula matemática (Rodada 1) — testes do render HTML/export.
 *
 * Os nós `mathBlock`/`mathInline` exibem em todo lugar via PNG (`render_png`).
 * No export HTML (renderSicroDocToHtml → generateHTML → renderHTML do nó):
 *   1. com `render_png` → emite `<img>` com o data URI + classe + data-latex;
 *   2. sem `render_png` → fallback pro texto LaTeX (auditável, nunca some).
 */

import { describe, expect, it } from "vitest";
import { renderSicroDocToHtml, type SicroDoc } from "../..";

const PNG =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

function docWith(content: unknown[]): SicroDoc {
  return {
    document_id: "doc-1",
    occurrence_id: "occ-1",
    type: "laudo",
    title: "Laudo math",
    template_id: "documento_livre",
    created_at: "2026-06-06T00:00:00Z",
    updated_at: "2026-06-06T00:00:00Z",
    metadata: {},
    content: { type: "doc", content },
  } as unknown as SicroDoc;
}

describe("Fórmula matemática — render HTML", () => {
  it("mathBlock com render_png vira <img> centralizado", () => {
    const doc = docWith([
      {
        type: "mathBlock",
        attrs: {
          latex: "k = \\frac{L}{P}",
          render_png: PNG,
          render_w_cm: 3,
          render_h_cm: 1.2,
        },
      },
    ]);
    const html = renderSicroDocToHtml(doc, { fullDocument: false });
    expect(html).toContain("sicro-math-block");
    expect(html).toContain("<img");
    expect(html).toContain("data:image/png;base64,");
    expect(html).toContain('data-latex="k = \\frac{L}{P}"');
  });

  it("mathInline com render_png vira <img> inline", () => {
    const doc = docWith([
      {
        type: "paragraph",
        content: [
          { type: "text", text: "valor " },
          {
            type: "mathInline",
            attrs: { latex: "\\sqrt{2}", render_png: PNG, render_h_cm: 0.6 },
          },
          { type: "text", text: " px" },
        ],
      },
    ]);
    const html = renderSicroDocToHtml(doc, { fullDocument: false });
    expect(html).toContain("sicro-math-inline");
    expect(html).toContain("<img");
  });

  it("sem render_png → fallback pro texto LaTeX (não some)", () => {
    const doc = docWith([
      {
        type: "mathBlock",
        attrs: { latex: "E = mc^2", render_png: null },
      },
    ]);
    const html = renderSicroDocToHtml(doc, { fullDocument: false });
    expect(html).toContain("E = mc^2");
    expect(html).not.toContain("<img");
  });
});
