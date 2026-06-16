/**
 * P25 — Wrapper-âncora das figuras flutuantes (editor == PDF).
 *
 * Cobre dois contratos:
 *
 *  1. SERIALIZAÇÃO (renderHTML via renderSicroDocToHtml → generateHTML):
 *     - figura FLUTUANTE (in_front/behind) emite um <div data-figure-anchor>
 *       `position:relative; height:0` ENVOLVENDO uma <figure> `position:absolute`
 *       com left/top em cm. É esse wrapper que faz o PDF e o editor usarem o
 *       MESMO frame de coordenada (a figura ancora no wrapper, não no
 *       container global de todas as páginas).
 *     - figura INLINE continua no fluxo normal (margin:auto), wrapper inerte.
 *
 *  2. MIGRAÇÃO (coerceSicroDoc / sanitizeFloatingFigures):
 *     - fotos flutuantes legadas com wrap_y_cm no frame global antigo (Y grande)
 *       são rebaseadas pra perto da âncora;
 *     - fotos importadas do Word (Y pequeno) ficam intocadas;
 *     - inline nunca é tocada;
 *     - idempotente.
 */

import { describe, expect, it } from "vitest";
import {
  coerceSicroDoc,
  sanitizeFloatingFigures,
  renderSicroDocToHtml,
  type SicroDoc,
} from "../..";

function docWith(content: unknown[]): SicroDoc {
  return {
    document_id: "doc-1",
    occurrence_id: "occ-1",
    type: "laudo",
    title: "Laudo figura âncora",
    template_id: "documento_livre",
    created_at: "2026-06-06T00:00:00Z",
    updated_at: "2026-06-06T00:00:00Z",
    metadata: {},
    content: { type: "doc", content },
  } as unknown as SicroDoc;
}

function figure(attrs: Record<string, unknown>): unknown {
  return {
    type: "figure",
    attrs: { id: "fig-1", src: "x.png", kind: "image", ...attrs },
    content: [
      {
        type: "figcaption",
        content: [{ type: "text", text: "Legenda." }],
      },
    ],
  };
}

describe("Figure.renderHTML — wrapper-âncora (flutuante)", () => {
  it("figura in_front emite <div data-figure-anchor> position:relative envolvendo <figure> position:absolute", () => {
    const html = renderSicroDocToHtml(
      docWith([
        figure({ wrap_mode: "in_front", wrap_x_cm: 1.07, wrap_y_cm: 0.89 }),
      ]),
      { fullDocument: false, numbering: false },
    );
    // Wrapper presente, position:relative, altura zero (o serializador
    // normaliza `0` → `0px`).
    expect(html).toContain('data-figure-anchor="true"');
    expect(html).toMatch(
      /<div[^>]*data-figure-anchor="true"[^>]*style="[^"]*position: relative;[^"]*height: 0(?:px)?;[^"]*"/,
    );
    // Figura interna absoluta com left/top em cm.
    expect(html).toMatch(/<figure[^>]*position: absolute/);
    expect(html).toContain("left: 1.07cm");
    expect(html).toContain("top: 0.89cm");
    // O <div> wrapper vem ANTES da <figure> (envolve-a).
    const divIdx = html.indexOf("data-figure-anchor");
    const figIdx = html.indexOf("<figure");
    expect(divIdx).toBeGreaterThanOrEqual(0);
    expect(figIdx).toBeGreaterThan(divIdx);
  });

  it("figura behind usa z-index negativo dentro do wrapper", () => {
    const html = renderSicroDocToHtml(
      docWith([figure({ wrap_mode: "behind", wrap_x_cm: 0, wrap_y_cm: 2 })]),
      { fullDocument: false, numbering: false },
    );
    expect(html).toContain('data-figure-anchor="true"');
    expect(html).toMatch(/<figure[^>]*z-index: -1/);
  });

  it("figura inline: wrapper inerte (sem position:relative/absolute), figura no fluxo com margin", () => {
    const html = renderSicroDocToHtml(
      docWith([figure({ wrap_mode: "inline" })]),
      { fullDocument: false, numbering: false },
    );
    // Wrapper existe mas é inerte.
    expect(html).toContain('data-figure-anchor="true"');
    expect(html).not.toMatch(
      /<div[^>]*data-figure-anchor="true"[^>]*position: relative/,
    );
    // Figura inline: margin (centralização), nunca position:absolute. O
    // serializador normaliza `0` → `0px`.
    expect(html).toMatch(/<figure[^>]*margin: 0(?:px)? auto/);
    expect(html).not.toMatch(/<figure[^>]*position: absolute/);
  });
});

describe("sanitizeFloatingFigures — migração do frame global antigo", () => {
  it("rebaseia wrap_y_cm grande (frame .editorWrap antigo) pra perto da âncora", () => {
    const content = {
      type: "doc",
      content: [
        figure({ wrap_mode: "in_front", wrap_x_cm: 1.07, wrap_y_cm: 35.4 }),
      ],
    };
    const out = sanitizeFloatingFigures(content as never);
    const fig = (out.content as unknown[])[0] as {
      attrs: { wrap_y_cm: number; wrap_x_cm: number };
    };
    // 35.4 % 29.7 = 5.7 → perto da âncora; X intocado.
    expect(Math.abs(fig.attrs.wrap_y_cm)).toBeLessThan(28);
    expect(fig.attrs.wrap_x_cm).toBe(1.07);
  });

  it("NÃO toca foto importada do Word (Y pequeno)", () => {
    const content = {
      type: "doc",
      content: [
        figure({ wrap_mode: "in_front", wrap_x_cm: 0.01, wrap_y_cm: 0.89 }),
      ],
    };
    const out = sanitizeFloatingFigures(content as never);
    // Sem mudança → mesma referência (otimização) e mesmo valor.
    expect(out).toBe(content);
    const fig = (out.content as unknown[])[0] as {
      attrs: { wrap_y_cm: number };
    };
    expect(fig.attrs.wrap_y_cm).toBe(0.89);
  });

  it("NÃO toca figura inline mesmo com wrap_y_cm grande", () => {
    const content = {
      type: "doc",
      content: [figure({ wrap_mode: "inline", wrap_y_cm: 99 })],
    };
    const out = sanitizeFloatingFigures(content as never);
    expect(out).toBe(content);
  });

  it("é idempotente (rodar 2x não corrompe)", () => {
    const content = {
      type: "doc",
      content: [
        figure({ wrap_mode: "behind", wrap_x_cm: 2, wrap_y_cm: 64 }),
      ],
    };
    const once = sanitizeFloatingFigures(content as never);
    const twice = sanitizeFloatingFigures(once);
    // 2ª passada já tem Y pequeno → sem mudança → mesma referência.
    expect(twice).toBe(once);
    const f1 = (once.content as unknown[])[0] as {
      attrs: { wrap_y_cm: number };
    };
    const f2 = (twice.content as unknown[])[0] as {
      attrs: { wrap_y_cm: number };
    };
    expect(f1.attrs.wrap_y_cm).toBe(f2.attrs.wrap_y_cm);
    expect(Math.abs(f2.attrs.wrap_y_cm)).toBeLessThan(28);
  });

  it("coerceSicroDoc aplica a sanitização no content carregado", () => {
    const raw = {
      document_id: "d1",
      occurrence_id: "o1",
      content: {
        type: "doc",
        content: [
          figure({ wrap_mode: "in_front", wrap_x_cm: 0, wrap_y_cm: 70 }),
        ],
      },
    };
    const doc = coerceSicroDoc(raw);
    const fig = (doc.content.content as unknown[])[0] as {
      attrs: { wrap_y_cm: number };
    };
    expect(Math.abs(fig.attrs.wrap_y_cm)).toBeLessThan(28);
  });
});
