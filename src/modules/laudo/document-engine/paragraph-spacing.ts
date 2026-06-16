/**
 * Pós-laudo S — Paragraph Spacing
 *
 * Estende paragraph + heading com 3 atributos numéricos típicos do Word:
 *
 *   - `line_height`      → multiplicador (1.0, 1.15, 1.5, 2.0...). 0 = default.
 *   - `space_before_pt`  → espaço antes do parágrafo em pontos. Default 0.
 *   - `space_after_pt`   → espaço depois do parágrafo em pontos. Default 0.
 *
 * Persistência: tudo via inline style CSS (`line-height`, `margin-top`,
 * `margin-bottom`). Compatível com HTML/PDF/DOCX export — esses são
 * atributos CSS standard que o renderer já entende.
 *
 * Comandos chaináveis:
 *   - `setParagraphLineHeight(multiplier)`
 *   - `setParagraphSpaceBefore(pt)`
 *   - `setParagraphSpaceAfter(pt)`
 *
 * Renomeados de `setLineHeight` etc para evitar colisão com extensões
 * builtin do TipTap (ex: `@tiptap/extension-table` define
 * `setLineHeight(lineHeight: string)`).
 */

import { Extension, type CommandProps } from "@tiptap/core";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    paragraphSpacing: {
      /** Define o entrelinhas (multiplicador unitless: 1.0, 1.5...). */
      setParagraphLineHeight: (multiplier: number) => ReturnType;
      /** Define o espaço antes do parágrafo em pontos. */
      setParagraphSpaceBefore: (pt: number) => ReturnType;
      /** Define o espaço depois do parágrafo em pontos. */
      setParagraphSpaceAfter: (pt: number) => ReturnType;
    };
  }
}

/** Lê um número opcional de uma string CSS tipo "12pt" ou "1.5". */
function parseUnit(raw: string | undefined | null, unit: "" | "pt"): number {
  if (!raw) return 0;
  const pattern = unit === "pt" ? /^(-?\d+(?:\.\d+)?)pt$/ : /^(-?\d+(?:\.\d+)?)$/;
  const m = raw.trim().match(pattern);
  return m && m[1] ? parseFloat(m[1]) : 0;
}

/** Aplica um patch de attrs em todos os parágrafos/headings da seleção. */
function applyBlockAttrs(
  patch: Record<string, number>,
  { tr, state, dispatch }: CommandProps,
): boolean {
  const { from, to } = state.selection;
  let changed = false;
  state.doc.nodesBetween(from, to, (node, pos) => {
    if (node.type.name === "paragraph" || node.type.name === "heading") {
      const newAttrs = { ...node.attrs, ...patch };
      if (dispatch) tr.setNodeMarkup(pos, undefined, newAttrs);
      changed = true;
      return false; // não desce no bloco
    }
    return undefined;
  });
  return changed;
}

export const ParagraphSpacing = Extension.create({
  name: "paragraphSpacing",

  addGlobalAttributes() {
    return [
      {
        types: ["paragraph", "heading"],
        attributes: {
          line_height: {
            default: 0,
            parseHTML: (el: HTMLElement) => {
              const raw = el.style?.lineHeight || "";
              return parseUnit(raw, "");
            },
            renderHTML: (attrs: { line_height?: number }) => {
              const v = Number(attrs.line_height) || 0;
              if (!v || v <= 0) return {};
              return { style: `line-height: ${v}` };
            },
          },
          space_before_pt: {
            default: 0,
            parseHTML: (el: HTMLElement) => {
              const raw = el.style?.marginTop || "";
              return parseUnit(raw, "pt");
            },
            renderHTML: (attrs: { space_before_pt?: number }) => {
              const v = Number(attrs.space_before_pt) || 0;
              if (!v) return {};
              return { style: `margin-top: ${v}pt` };
            },
          },
          space_after_pt: {
            default: 0,
            parseHTML: (el: HTMLElement) => {
              const raw = el.style?.marginBottom || "";
              return parseUnit(raw, "pt");
            },
            renderHTML: (attrs: { space_after_pt?: number }) => {
              const v = Number(attrs.space_after_pt) || 0;
              if (!v) return {};
              return { style: `margin-bottom: ${v}pt` };
            },
          },
        },
      },
    ];
  },

  addCommands() {
    return {
      setParagraphLineHeight:
        (multiplier: number) =>
        (props) =>
          applyBlockAttrs(
            { line_height: Math.max(0, multiplier) },
            props,
          ),
      setParagraphSpaceBefore:
        (pt: number) =>
        (props) =>
          applyBlockAttrs(
            { space_before_pt: Math.max(0, pt) },
            props,
          ),
      setParagraphSpaceAfter:
        (pt: number) =>
        (props) =>
          applyBlockAttrs(
            { space_after_pt: Math.max(0, pt) },
            props,
          ),
    };
  },
});
