/**
 * W (fase 2b) — Testes do RODAPÉ Word-style.
 *
 * Espelha os testes N10/N1 do cabeçalho, mais um caso específico do rodapé:
 *   1. Schema coerção — docs antigos sem `footer` ganham defaults seguros.
 *   2. Schema coerção — clamping de `footer_height_cm`.
 *   3. Renderer HTML — gera `<footer class="sicro-doc-page-footer">`
 *      quando `footer.enabled === true` e há conteúdo.
 *   4. Renderer HTML — vazio quando disabled OU empty stub.
 *   5. Renderer HTML — figura do rodapé (brasão da PC, via relative_path)
 *      tem o `src` inlinado como data URI quando o caller pré-carrega os
 *      assets. Garante que o brasão aparece no PDF/HTML exportado.
 */

import { describe, expect, it } from "vitest";
import {
  clampFooterHeightCm,
  coerceSicroDoc,
  DEFAULT_FOOTER_HEIGHT_CM,
  emptyFooterContent,
  FOOTER_HEIGHT_MAX_CM,
  FOOTER_HEIGHT_MIN_CM,
  renderSicroDocToHtml,
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

describe("W — schema coerção do footer", () => {
  it("doc legado sem `footer` recebe defaults seguros (disabled, vazio)", () => {
    const doc = coerceSicroDoc({ ...BASE });
    expect(doc.footer).toBeDefined();
    expect(doc.footer?.enabled).toBe(false);
    expect(doc.footer?.content).toEqual(emptyFooterContent());
  });

  it("doc legado sem `layout.footer_height_cm` recebe default", () => {
    const doc = coerceSicroDoc({
      ...BASE,
      layout: { page_size: "A4", orientation: "portrait" },
    });
    expect(doc.layout.footer_height_cm).toBe(DEFAULT_FOOTER_HEIGHT_CM);
  });

  it("doc com footer completo é preservado", () => {
    const customContent = {
      type: "doc",
      content: [
        {
          type: "paragraph",
          content: [{ type: "text", text: "Footer custom" }],
        },
      ],
    };
    const doc = coerceSicroDoc({
      ...BASE,
      footer: { enabled: true, content: customContent },
    });
    expect(doc.footer?.enabled).toBe(true);
    expect(doc.footer?.content).toEqual(customContent);
  });

  it("footer_height_cm fora dos limites é clampado", () => {
    expect(clampFooterHeightCm(-5)).toBe(FOOTER_HEIGHT_MIN_CM);
    expect(clampFooterHeightCm(100)).toBe(FOOTER_HEIGHT_MAX_CM);
    expect(clampFooterHeightCm(2)).toBe(2);
    expect(clampFooterHeightCm("invalid")).toBe(DEFAULT_FOOTER_HEIGHT_CM);
    expect(clampFooterHeightCm(NaN)).toBe(DEFAULT_FOOTER_HEIGHT_CM);
  });
});

describe("W — renderer HTML do rodapé dinâmico", () => {
  // Helper: checa apenas o TAG markup do footer dinâmico (a regra CSS está
  // sempre presente no <style>, e o `footer.sicro-doc-footer` institucional
  // é outro elemento).
  const hasFooterTag = (html: string): boolean =>
    /<footer\s+class="sicro-doc-page-footer"/.test(html);

  it("emite faixa quando enabled=true e há conteúdo", () => {
    const doc = coerceSicroDoc({
      ...BASE,
      footer: {
        enabled: true,
        content: {
          type: "doc",
          content: [
            {
              type: "paragraph",
              content: [{ type: "text", text: "Rodapé de prova" }],
            },
          ],
        },
      },
    });
    const html = renderSicroDocToHtml(doc, { fullDocument: true });
    expect(hasFooterTag(html)).toBe(true);
    expect(html).toContain("Rodapé de prova");
    expect(html).toContain("--sicro-footer-height");
  });

  it("não emite faixa quando enabled=false", () => {
    const doc = coerceSicroDoc({
      ...BASE,
      footer: {
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
    expect(hasFooterTag(html)).toBe(false);
    expect(html).not.toContain("Não deve aparecer");
  });

  it("não emite faixa para empty stub (single empty paragraph)", () => {
    const doc = coerceSicroDoc({
      ...BASE,
      footer: { enabled: true, content: emptyFooterContent() },
    });
    const html = renderSicroDocToHtml(doc, { fullDocument: true });
    expect(hasFooterTag(html)).toBe(false);
  });

  it("inlina o brasão do rodapé (relative_path → data URI) no export", () => {
    const relPath = "evidencias/imported/brasao_pc.png";
    const dataUri = "data:image/png;base64,QUJD"; // "ABC"
    const doc = coerceSicroDoc({
      ...BASE,
      footer: {
        enabled: true,
        content: {
          type: "doc",
          content: [
            {
              type: "figure",
              attrs: {
                src: "tauri://localhost/should-be-replaced.png",
                relative_path: relPath,
              },
              content: [{ type: "figcaption" }],
            },
          ],
        },
      },
    });
    const html = renderSicroDocToHtml(doc, {
      fullDocument: true,
      evidenceAssets: { byRelativePath: { [relPath]: dataUri } },
    });
    expect(hasFooterTag(html)).toBe(true);
    // O src convertFileSrc deve ter sido trocado pelo data URI inlinado.
    expect(html).toContain(dataUri);
    expect(html).not.toContain("should-be-replaced.png");
  });
});
