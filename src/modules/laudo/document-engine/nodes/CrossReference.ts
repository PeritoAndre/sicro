/**
 * CrossReference — node inline atomic que renderiza uma referência a
 * outro elemento numerado (figura/tabela/quesito) do documento.
 *
 * F12.2 — Quando o usuário inserir "ver Figura 3", esse "3" não fica
 * hardcoded — o node armazena o `targetId` (UUID estável da figura
 * referenciada) e o NodeView consulta o map IDs→ordinal mantido pelo
 * plugin AutoNumbering. Se a figura virar a Figura 5 (porque o usuário
 * apagou figuras anteriores), a referência atualiza automaticamente.
 *
 * Atributos:
 *   - `targetId`   : string — UUID do elemento alvo.
 *   - `prefix`     : string — texto antes do número ("Figura", "Tabela", "Quesito").
 *                              Default = label automático baseado no kind.
 */

import { Node, mergeAttributes } from "@tiptap/core";
import {
  AUTO_NUMBERING_PLUGIN_KEY,
  type AutoNumberingState,
} from "../auto-numbering";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    crossReference: {
      insertCrossReference: (targetId: string) => ReturnType;
    };
  }
}

export const CrossReference = Node.create({
  name: "crossReference",
  inline: true,
  group: "inline",
  atom: true,
  selectable: true,
  draggable: false,

  addAttributes() {
    return {
      targetId: { default: null },
      // Prefix opcional pra override do label automático.
      prefix: { default: null },
    };
  },

  parseHTML() {
    return [{ tag: "span[data-cross-ref]" }];
  },

  renderHTML({ node, HTMLAttributes }) {
    // Render estático para HTML export. NodeView fará o render dinâmico.
    const targetId = node.attrs["targetId"] as string | null;
    const prefix = (node.attrs["prefix"] as string | null) ?? "ref";
    return [
      "span",
      mergeAttributes(HTMLAttributes, {
        "data-cross-ref": "true",
        "data-target-id": targetId ?? "",
        class: "sicro-cross-ref",
      }),
      `${prefix} ?`,
    ];
  },

  addNodeView() {
    return ({ node, view, getPos }) => {
      void getPos;
      const span = document.createElement("span");
      span.className = "sicro-cross-ref";
      span.setAttribute("data-cross-ref", "true");
      const targetId = node.attrs["targetId"] as string | null;
      const prefix = (node.attrs["prefix"] as string | null) ?? null;
      if (targetId) span.setAttribute("data-target-id", targetId);

      const update = () => {
        if (!targetId) {
          span.textContent = prefix ? `${prefix} ?` : "ref ?";
          span.classList.add("sicro-cross-ref--missing");
          return;
        }
        const state = AUTO_NUMBERING_PLUGIN_KEY.getState(view.state) as
          | AutoNumberingState
          | undefined;
        const entry = state?.idToOrdinal.get(targetId);
        if (!entry) {
          span.textContent = prefix ? `${prefix} ?` : "ref ?";
          span.classList.add("sicro-cross-ref--missing");
          return;
        }
        span.classList.remove("sicro-cross-ref--missing");
        span.textContent = prefix ? `${prefix} ${entry.ordinal}` : entry.label;
      };

      update();

      return {
        dom: span,
        update: (updatedNode) => {
          if (updatedNode.type.name !== "crossReference") return false;
          // Re-render baseado no map atualizado.
          update();
          return true;
        },
      };
    };
  },

  addCommands() {
    return {
      insertCrossReference:
        (targetId: string) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs: { targetId },
          });
        },
    };
  },
});
