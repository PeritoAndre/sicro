/**
 * CommentMark — marca TipTap que ancora um comentário a um intervalo de texto.
 *
 * F8 — Sistema de comentários. Cada comentário tem:
 *   - id (UUID v4 — gerado pelo `addComment` command);
 *   - corpo (texto + autor + timestamp) — armazenado FORA do mark,
 *     na coleção `doc.comments[]` (vide schema 1.1.0).
 *
 * O mark só carrega o `id`, e o renderer aplica:
 *   - background amarelo translúcido;
 *   - sublinhado pontilhado dourado;
 *   - cursor pointer.
 *
 * Comentários "resolved" recebem a classe `is-resolved` que muda o
 * destaque para verde sutil. Implementado via `data-resolved` no mark
 * (atributo opcional, default false) + CSS condicional.
 *
 * Comandos:
 *   - `addComment(id)`      — aplica o mark no intervalo selecionado.
 *   - `removeComment(id)`   — remove o mark cujo id casa (usado pelo Excluir).
 *   - `toggleCommentResolved(id, resolved)` — atualiza `data-resolved`.
 */

import { Mark, mergeAttributes } from "@tiptap/core";

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    commentMark: {
      addComment: (id: string) => ReturnType;
      removeComment: (id: string) => ReturnType;
    };
  }
}

export interface CommentMarkAttrs {
  id: string;
  resolved: boolean;
}

export const CommentMark = Mark.create({
  name: "commentMark",
  inclusive: false,
  excludes: "",
  spanning: true,

  addAttributes() {
    return {
      id: {
        default: null,
        parseHTML: (el) => el.getAttribute("data-comment-id"),
        renderHTML: (attrs) => {
          if (!attrs["id"]) return {};
          return { "data-comment-id": attrs["id"] };
        },
      },
      resolved: {
        default: false,
        parseHTML: (el) => el.getAttribute("data-resolved") === "true",
        renderHTML: (attrs) => {
          if (!attrs["resolved"]) return {};
          return { "data-resolved": "true" };
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: "span[data-comment-id]",
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { class: "sicro-comment-anchor" }),
      0,
    ];
  },

  addCommands() {
    return {
      addComment:
        (id: string) =>
        ({ commands }) => {
          return commands.setMark(this.name, { id, resolved: false });
        },
      removeComment:
        (id: string) =>
        ({ tr, state, dispatch }) => {
          // Remove o mark cujo `id` casa, mantendo demais marks.
          let found = false;
          state.doc.descendants((node, pos) => {
            if (!node.isText) return;
            const mark = node.marks.find(
              (m) => m.type.name === "commentMark" && m.attrs["id"] === id,
            );
            if (mark) {
              found = true;
              tr.removeMark(pos, pos + node.nodeSize, mark);
            }
          });
          if (found && dispatch) dispatch(tr);
          return found;
        },
    };
  },
});
