/**
 * N14 — Testes do cabeçalho Word-style.
 *
 * Cobertura:
 *   1. Schema coerção — docs antigos sem `header` ganham defaults seguros.
 *   2. Schema coerção — clamping de `header_height_cm`.
 *   3. Migração N12 — `seedHeaderContentFromInstitutionalTemplate` gera
 *      ProseMirror válido a partir de um template institucional.
 *   4. Renderer HTML — gera `<header class="sicro-doc-page-header">`
 *      quando `header.enabled === true`, vazio quando `false`.
 *   5. Renderer HTML — empty stub (single empty paragraph) NÃO emite
 *      faixa do header (evita banda em branco no PDF).
 *   6. Compatibilidade — doc sem campo `header` em formato legado é
 *      coerced sem crash.
 */

import { describe, expect, it } from "vitest";
import {
  clampHeaderHeightCm,
  coerceSicroDoc,
  DEFAULT_HEADER_HEIGHT_CM,
  emptyHeaderContent,
  HEADER_HEIGHT_MAX_CM,
  HEADER_HEIGHT_MIN_CM,
  PCA_PADRAO_V1,
  renderSicroDocToHtml,
  seedHeaderContentFromInstitutionalTemplate,
  type SicroDoc,
} from "..";

const BASE: Partial<SicroDoc> = {
  document_id: "doc-1",
  occurrence_id: "occ-1",
  type: "laudo",
  title: "Laudo de teste",
  template_id: "documento_livre",
  created_at: "2026-05-28T00:00:00Z",
  updated_at: "2026-05-28T00:00:00Z",
  metadata: { numero_laudo: "123/2026" },
  content: { type: "doc", content: [{ type: "paragraph" }] },
};

describe("N1 — schema coerção do header", () => {
  it("doc legado sem `header` recebe defaults seguros (disabled, vazio)", () => {
    const doc = coerceSicroDoc({ ...BASE });
    expect(doc.header).toBeDefined();
    expect(doc.header?.enabled).toBe(false);
    expect(doc.header?.content).toEqual(emptyHeaderContent());
  });

  it("doc legado sem `layout.header_height_cm` recebe default", () => {
    const doc = coerceSicroDoc({
      ...BASE,
      layout: { page_size: "A4", orientation: "portrait" },
    });
    expect(doc.layout.header_height_cm).toBe(DEFAULT_HEADER_HEIGHT_CM);
  });

  it("doc com header completo é preservado", () => {
    const customContent = {
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [{ type: "text", text: "Header custom" }],
        },
      ],
    };
    const doc = coerceSicroDoc({
      ...BASE,
      header: { enabled: true, content: customContent },
    });
    expect(doc.header?.enabled).toBe(true);
    expect(doc.header?.content).toEqual(customContent);
  });

  it("header_height_cm fora dos limites é clampado", () => {
    expect(clampHeaderHeightCm(-5)).toBe(HEADER_HEIGHT_MIN_CM);
    expect(clampHeaderHeightCm(100)).toBe(HEADER_HEIGHT_MAX_CM);
    expect(clampHeaderHeightCm(2.5)).toBe(2.5);
    expect(clampHeaderHeightCm("invalid")).toBe(DEFAULT_HEADER_HEIGHT_CM);
    expect(clampHeaderHeightCm(NaN)).toBe(DEFAULT_HEADER_HEIGHT_CM);
  });
});

describe("N12 — migração de docs legados via institutional_template", () => {
  it("semeia paragraphs com brand_lines em negrito centralizadas", () => {
    const content = seedHeaderContentFromInstitutionalTemplate(
      PCA_PADRAO_V1,
      { numero_laudo: "999/2026" },
      null,
    ) as { type: string; content: unknown[] };
    expect(content.type).toBe("doc");
    expect(Array.isArray(content.content)).toBe(true);
    expect(content.content.length).toBeGreaterThan(0);
    // Primeiro parágrafo deve ter o primeiro brand_line ("GOVERNO DO ESTADO...").
    const firstP = content.content[0] as {
      type: string;
      attrs?: { textAlign?: string };
      content: Array<{ type: string; text?: string; marks?: Array<{ type: string }> }>;
    };
    expect(firstP.type).toBe("paragraph");
    expect(firstP.attrs?.textAlign).toBe("center");
    const textNode = firstP.content[0];
    expect(textNode?.text).toContain("GOVERNO DO ESTADO");
    expect(textNode?.marks?.some((m) => m.type === "bold")).toBe(true);
  });

  it("inclui metadata.numero_laudo na linha de metadados quando disponível", () => {
    const content = seedHeaderContentFromInstitutionalTemplate(
      PCA_PADRAO_V1,
      { numero_laudo: "999/2026" },
      null,
    ) as { content: Array<{ content?: Array<{ text?: string }> }> };
    // Algum parágrafo deve mencionar "Laudo nº: 999/2026".
    const allText = content.content
      .flatMap((p) => p.content ?? [])
      .map((n) => n.text ?? "")
      .join(" ");
    expect(allText).toContain("Laudo nº: 999/2026");
  });

  it("retorna doc vazio quando template não tem nada e sem metadata", () => {
    const emptyTemplate = {
      ...PCA_PADRAO_V1,
      header: {
        ...PCA_PADRAO_V1.header,
        brand_lines: [],
        subtitle: undefined,
        metadata_fields: [],
      },
    };
    const content = seedHeaderContentFromInstitutionalTemplate(
      emptyTemplate,
      {},
      null,
    ) as { content: unknown[] };
    expect(content.content.length).toBe(1); // single empty paragraph
  });
});

describe("N10 — renderer HTML do header dinâmico", () => {
  // Helper: checa apenas o TAG markup do header (não a regra CSS, que
  // está sempre presente no <style>).
  const hasHeaderTag = (html: string): boolean =>
    /<header\s+class="sicro-doc-page-header"/.test(html);

  it("emite faixa quando enabled=true e há conteúdo", () => {
    const doc = coerceSicroDoc({
      ...BASE,
      header: {
        enabled: true,
        content: {
          type: "doc",
          content: [
            {
              type: "paragraph",
              content: [{ type: "text", text: "Cabeçalho de prova" }],
            },
          ],
        },
      },
    });
    const html = renderSicroDocToHtml(doc, { fullDocument: true });
    expect(hasHeaderTag(html)).toBe(true);
    expect(html).toContain("Cabeçalho de prova");
    expect(html).toContain("--sicro-header-height");
  });

  it("não emite faixa quando enabled=false", () => {
    const doc = coerceSicroDoc({
      ...BASE,
      header: {
        enabled: false,
        content: {
          type: "doc",
          content: [
            {
              type: "paragraph",
              content: [{ type: "text", text: "Não deve aparecer" }],
            },
          ],
        },
      },
    });
    const html = renderSicroDocToHtml(doc, { fullDocument: true });
    expect(hasHeaderTag(html)).toBe(false);
    expect(html).not.toContain("Não deve aparecer");
  });

  it("não emite faixa para empty stub (single empty paragraph)", () => {
    const doc = coerceSicroDoc({
      ...BASE,
      header: { enabled: true, content: emptyHeaderContent() },
    });
    const html = renderSicroDocToHtml(doc, { fullDocument: true });
    expect(hasHeaderTag(html)).toBe(false);
  });

  it("documento sem header continua exportando body normalmente", () => {
    const doc = coerceSicroDoc({
      ...BASE,
      content: {
        type: "doc",
        content: [
          {
            type: "paragraph",
            content: [{ type: "text", text: "Corpo do laudo" }],
          },
        ],
      },
    });
    const html = renderSicroDocToHtml(doc, { fullDocument: true });
    expect(html).toContain("Corpo do laudo");
    expect(hasHeaderTag(html)).toBe(false);
  });
});
