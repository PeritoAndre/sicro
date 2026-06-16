/**
 * RevisionMark — marca de revisão (track-changes lite, F8).
 *
 * Duas variantes em uma única mark (`type` atributo):
 *   - "insertion": texto adicionado durante revisão → verde sublinhado.
 *   - "deletion":  texto a remover → vermelho com strikethrough.
 *
 * O fluxo MVP é MANUAL — o revisor seleciona o trecho e clica no botão
 * "Marcar como inserção" ou "Marcar como remoção" (a serem expostos pela
 * CommentsPanel/VersionsPanel ou um novo botão de toolbar). Auto-tracking
 * via ProseMirror plugin fica pra spike futuro.
 *
 * Comandos:
 *   - addRevisionInsertion(author, timestamp?) — aplica mark.type="insertion"
 *   - addRevisionDeletion(author, timestamp?)  — aplica mark.type="deletion"
 *   - acceptRevision(id)  — remove o mark; (para deletion: também apaga o texto)
 *   - rejectRevision(id)  — remove o mark; (para insertion: também apaga o texto)
 */

import { Mark, mergeAttributes } from "@tiptap/core";

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    revisionMark: {
      addRevisionInsertion: (author?: string) => ReturnType;
      addRevisionDeletion: (author?: string) => ReturnType;
      acceptRevision: (id: string) => ReturnType;
      rejectRevision: (id: string) => ReturnType;
    };
  }
}

export const RevisionMark = Mark.create({
  name: "revisionMark",
  inclusive: false,

  addAttributes() {
    return {
      id: {
        default: null,
        parseHTML: (el) => el.getAttribute("data-revision-id"),
        renderHTML: (attrs) =>
          attrs["id"] ? { "data-revision-id": attrs["id"] } : {},
      },
      type: {
        default: "insertion",
        parseHTML: (el) =>
          el.getAttribute("data-revision-type") === "deletion"
            ? "deletion"
            : "insertion",
        renderHTML: (attrs) => ({
          "data-revision-type":
            attrs["type"] === "deletion" ? "deletion" : "insertion",
        }),
      },
      author: {
        default: null,
        parseHTML: (el) => el.getAttribute("data-revision-author"),
        renderHTML: (attrs) =>
          attrs["author"] ? { "data-revision-author": attrs["author"] } : {},
      },
      created_at: {
        default: null,
        parseHTML: (el) => el.getAttribute("data-revision-at"),
        renderHTML: (attrs) =>
          attrs["created_at"]
            ? { "data-revision-at": attrs["created_at"] }
            : {},
      },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-revision-id]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      mergeAttributes(HTMLAttributes, { class: "sicro-revision-mark" }),
      0,
    ];
  },

  addCommands() {
    const uuid = () => {
      if (
        typeof crypto !== "undefined" &&
        typeof crypto.randomUUID === "function"
      ) {
        return crypto.randomUUID();
      }
      return `rev-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e9).toString(36)}`;
    };

    return {
      addRevisionInsertion:
        (author?: string) =>
        ({ commands }) => {
          return commands.setMark(this.name, {
            id: uuid(),
            type: "insertion",
            author: author ?? "Perito",
            created_at: new Date().toISOString(),
          });
        },
      addRevisionDeletion:
        (author?: string) =>
        ({ commands }) => {
          return commands.setMark(this.name, {
            id: uuid(),
            type: "deletion",
            author: author ?? "Perito",
            created_at: new Date().toISOString(),
          });
        },
      acceptRevision:
        (id: string) =>
        ({ tr, state, dispatch }) => {
          let changed = false;
          state.doc.descendants((node, pos) => {
            if (!node.isText) return;
            const mark = node.marks.find(
              (m) => m.type.name === "revisionMark" && m.attrs["id"] === id,
            );
            if (!mark) return;
            if (mark.attrs["type"] === "deletion") {
              // Aceitar deleção → apagar o texto.
              tr.delete(pos, pos + node.nodeSize);
            } else {
              // Aceitar inserção → manter texto, remover o mark.
              tr.removeMark(pos, pos + node.nodeSize, mark);
            }
            changed = true;
          });
          if (changed && dispatch) dispatch(tr);
          return changed;
        },
      rejectRevision:
        (id: string) =>
        ({ tr, state, dispatch }) => {
          let changed = false;
          state.doc.descendants((node, pos) => {
            if (!node.isText) return;
            const mark = node.marks.find(
              (m) => m.type.name === "revisionMark" && m.attrs["id"] === id,
            );
            if (!mark) return;
            if (mark.attrs["type"] === "insertion") {
              // Rejeitar inserção → apagar o texto.
              tr.delete(pos, pos + node.nodeSize);
            } else {
              // Rejeitar deleção → manter texto, remover o mark.
              tr.removeMark(pos, pos + node.nodeSize, mark);
            }
            changed = true;
          });
          if (changed && dispatch) dispatch(tr);
          return changed;
        },
    };
  },
});
