/**
 * FieldPlaceholder — node TipTap inline para campos automáticos `{var}`.
 *
 * Sintaxe: uma chave (`{numero_laudo}`), igual a `{ PAGE }` do Word. Cada
 * placeholder é um nó atômico:
 *
 *   <span data-field="numero_laudo" class="sicro-field">{numero_laudo}</span>
 *
 * No editor o NodeView (FieldNodeView) mostra o VALOR resolvido (não o
 * placeholder textual) — só cai no texto `{numero_laudo}` se o valor estiver
 * vazio (estado "pendente"). Quando o perito edita o campo no painel
 * "{} Campos" ou na Home, todas as pílulas atualizam automaticamente.
 *
 * Persistência: `{ type: "fieldPlaceholder", attrs: { field: "numero_bo" } }`.
 *
 * Renderer HTML/PDF: o `<span data-field="key">{key}</span>` é mantido no
 * HTML; o resolver/walker do export troca pelo valor (ou pelo `counter(page)`
 * via CSS, no caso de `page`/`pages`).
 *
 * Comandos: `insertFieldPlaceholder(key)`.
 */

import { Node, mergeAttributes } from "@tiptap/core";
import { ReactNodeViewRenderer } from "@tiptap/react";
import { FieldNodeView } from "./FieldNodeView";

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    fieldPlaceholder: {
      /** Insere um `{{key}}` na posição atual da seleção. */
      insertFieldPlaceholder: (key: string) => ReturnType;
    };
  }
}

export const FieldPlaceholder = Node.create({
  name: "fieldPlaceholder",
  group: "inline",
  inline: true,
  atom: true,
  // Selecionável mas não editável internamente — comporta-se como uma
  // "pílula" de campo único, similar a uma menção em apps de chat.
  selectable: true,

  addAttributes() {
    return {
      field: {
        default: "",
        parseHTML: (el) => el.getAttribute("data-field") || "",
        renderHTML: (attrs) => {
          if (!attrs["field"]) return {};
          return { "data-field": attrs["field"] };
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'span[data-field]',
      },
      // Compatibilidade futura — `<x-field>` se algum exportador usar.
      { tag: "x-field" },
    ];
  },

  renderHTML({ HTMLAttributes, node }) {
    const key = (node.attrs["field"] as string) || "";
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        class: "sicro-field sicro-field-placeholder",
        "data-field": key,
        // contentEditable=false garante que o cursor pula o nó inteiro.
        contenteditable: "false",
      }),
      `{${key}}`,
    ];
  },

  addCommands() {
    return {
      insertFieldPlaceholder:
        (key: string) =>
        ({ commands }) => {
          if (!key) return false;
          return commands.insertContent({
            type: this.name,
            attrs: { field: key },
          });
        },
    };
  },

  /**
   * NodeView React: a pílula é renderizada pelo `FieldNodeView`, que lê o
   * valor real do laudo (metadata + occurrence) e mostra o conteúdo
   * resolvido em vez do placeholder textual. Reativo aos stores.
   */
  addNodeView() {
    return ReactNodeViewRenderer(FieldNodeView);
  },
});
