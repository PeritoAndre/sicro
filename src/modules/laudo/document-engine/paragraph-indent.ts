/**
 * R — Paragraph First-Line Indent
 *
 * Estende paragraph + heading com o atributo `first_line_indent_cm`
 * (recuo da primeira linha estilo Word). Aplicado via inline style
 * `text-indent: Xcm` — afeta APENAS a primeira linha do bloco, exatamente
 * como no Word.
 *
 * Persistência: o style é lido/escrito direto no DOM (style="text-indent: 1.25cm").
 * Compatível com HTML/PDF/DOCX export — text-indent é CSS standard.
 *
 * UI: a HorizontalRuler renderiza um triângulo SUPERIOR draggable que
 * controla este atributo do parágrafo onde o cursor está.
 */

import { Extension } from "@tiptap/core";
import type { EditorState } from "@tiptap/pm/state";
import type { Node as PMNode } from "@tiptap/pm/model";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    firstLineIndent: {
      /** Define o recuo da primeira linha do parágrafo selecionado. */
      setFirstLineIndent: (cm: number) => ReturnType;
      /** Define o recuo à esquerda (do bloco inteiro) do parágrafo. */
      setLeftIndent: (cm: number) => ReturnType;
    };
  }
}

export const ParagraphFirstLineIndent = Extension.create({
  name: "paragraphFirstLineIndent",

  addGlobalAttributes() {
    return [
      {
        types: ["paragraph", "heading"],
        attributes: {
          first_line_indent_cm: {
            default: 0,
            parseHTML: (el: HTMLElement) => {
              const indent = el.style?.textIndent || "";
              const match = indent.match(/^(-?\d+(?:\.\d+)?)cm$/);
              return match && match[1] ? parseFloat(match[1]) : 0;
            },
            renderHTML: (attrs: { first_line_indent_cm?: number }) => {
              const v = Number(attrs.first_line_indent_cm) || 0;
              if (!v) return {};
              return { style: `text-indent: ${v.toFixed(2)}cm` };
            },
          },
          // R2 — Recuo à ESQUERDA do bloco inteiro (margin-left), estilo
          // Word. Independente do recuo de 1ª linha; combinados dão o
          // efeito de "lista pendente" (hanging). Importado do `w:ind`.
          left_indent_cm: {
            default: 0,
            parseHTML: (el: HTMLElement) => {
              const m = (el.style?.marginLeft || "").match(
                /^(-?\d+(?:\.\d+)?)cm$/,
              );
              return m && m[1] ? parseFloat(m[1]) : 0;
            },
            renderHTML: (attrs: { left_indent_cm?: number }) => {
              const v = Number(attrs.left_indent_cm) || 0;
              if (!v) return {};
              return { style: `margin-left: ${v.toFixed(2)}cm` };
            },
          },
        },
      },
    ];
  },

  addCommands() {
    return {
      setFirstLineIndent:
        (cm: number) =>
        ({ tr, state, dispatch }) => {
          const { from, to } = state.selection;
          // Aplica em todos os blocos da seleção (geralmente um só).
          let changed = false;
          state.doc.nodesBetween(from, to, (node, pos) => {
            if (node.type.name === "paragraph" || node.type.name === "heading") {
              const newAttrs = {
                ...node.attrs,
                first_line_indent_cm: cm,
              };
              if (dispatch) {
                tr.setNodeMarkup(pos, undefined, newAttrs);
              }
              changed = true;
              // Não desce nesse nó.
              return false;
            }
            return undefined;
          });
          return changed;
        },
      setLeftIndent:
        (cm: number) =>
        ({ tr, state, dispatch }) => {
          const { from, to } = state.selection;
          let changed = false;
          state.doc.nodesBetween(from, to, (node, pos) => {
            if (node.type.name === "paragraph" || node.type.name === "heading") {
              const newAttrs = {
                ...node.attrs,
                left_indent_cm: Math.max(0, cm),
              };
              if (dispatch) {
                tr.setNodeMarkup(pos, undefined, newAttrs);
              }
              changed = true;
              return false;
            }
            return undefined;
          });
          return changed;
        },
    };
  },

  /**
   * Pós-laudo S — Tab / Shift+Tab gerencia o recuo da primeira linha
   * estilo Word.
   *
   *   - `Tab`        → +1.25 cm (acumulativo, máx ~8 cm)
   *   - `Shift+Tab`  → -1.25 cm (mín 0 cm)
   *
   * Só intercepta se o cursor estiver num parágrafo OU heading top-level
   * (não dentro de lista, tabela, código, blockquote — onde o Tab tem
   * outro papel: indentar item, pular célula, etc.). Pra esses casos
   * deixamos o keymap default do TipTap rolar (retornando false).
   */
  addKeyboardShortcuts() {
    const TAB_STEP_CM = 1.25;
    const MAX_INDENT_CM = 8;

    const isPlainBlock = (
      state: EditorState,
    ): { node: PMNode; pos: number } | null => {
      const { $from } = state.selection;
      // Sobe pelo cursor procurando o primeiro paragraph/heading.
      for (let d = $from.depth; d >= 0; d--) {
        const node = $from.node(d);
        if (node.type.name === "paragraph" || node.type.name === "heading") {
          // Garante que o ancestral imediato seja o doc (top-level),
          // NÃO um list_item / table_cell / blockquote.
          const parentDepth = d - 1;
          if (parentDepth < 0) {
            return { node, pos: $from.before(d) };
          }
          const parentName = $from.node(parentDepth).type.name;
          if (
            parentName === "doc" ||
            parentName === "quesito_list" ||
            parentName === "quesito_item"
          ) {
            return { node, pos: $from.before(d) };
          }
          return null;
        }
      }
      return null;
    };

    // Recuo à esquerda do bloco onde o cursor está (qualquer parágrafo/
    // heading, inclusive item de lista).
    const readLeftIndentCm = (state: EditorState): number => {
      const { $from } = state.selection;
      for (let d = $from.depth; d >= 0; d--) {
        const node = $from.node(d);
        if (node.type.name === "paragraph" || node.type.name === "heading") {
          return Number(node.attrs.left_indent_cm) || 0;
        }
      }
      return 0;
    };

    return {
      Tab: ({ editor }) => {
        const target = isPlainBlock(editor.state);
        if (!target) return false; // deixa lista/tabela tratar
        const current = Number(target.node.attrs.first_line_indent_cm) || 0;
        const next = Math.min(MAX_INDENT_CM, current + TAB_STEP_CM);
        if (next === current) return true; // já no max — bloqueia Tab default
        return editor.chain().focus().setFirstLineIndent(next).run();
      },
      "Shift-Tab": ({ editor }) => {
        const target = isPlainBlock(editor.state);
        if (!target) return false;
        const current = Number(target.node.attrs.first_line_indent_cm) || 0;
        const next = Math.max(0, current - TAB_STEP_CM);
        if (next === current) return true;
        return editor.chain().focus().setFirstLineIndent(next).run();
      },
      // Recuo à ESQUERDA do bloco (estilo Word): Ctrl+] aumenta, Ctrl+[
      // diminui — ±1,25cm no parágrafo onde o cursor está, isolado.
      "Mod-]": ({ editor }) => {
        const cur = readLeftIndentCm(editor.state);
        return editor
          .chain()
          .focus()
          .setLeftIndent(Math.min(MAX_INDENT_CM, cur + TAB_STEP_CM))
          .run();
      },
      "Mod-[": ({ editor }) => {
        const cur = readLeftIndentCm(editor.state);
        return editor
          .chain()
          .focus()
          .setLeftIndent(Math.max(0, cur - TAB_STEP_CM))
          .run();
      },
    };
  },
});
