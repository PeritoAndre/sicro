/**
 * Quesitos — bloco pericial estruturado: lista de perguntas/respostas que
 * tipicamente fecha o laudo institucional.
 *
 * Estrutura:
 *
 *   quesitoList (block container)
 *     └── quesitoItem (block, 1+) — content: quesitoQuestion quesitoAnswer
 *           ├── quesitoQuestion (block, inline*)
 *           └── quesitoAnswer   (block, inline*)
 *
 * Numeração é calculada em render-time (não persistida) — reordenar quesitos
 * no editor reordena a numeração automaticamente.
 */

import { Node, mergeAttributes } from "@tiptap/core";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    quesito: {
      insertQuesitoList: (initialItems?: number) => ReturnType;
      appendQuesito: () => ReturnType;
    };
  }
}

export const QuesitoList = Node.create({
  name: "quesitoList",
  group: "block",
  content: "quesitoItem+",
  defining: true,

  parseHTML() {
    return [{ tag: "section[data-sicro-quesito-list]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "section",
      mergeAttributes(HTMLAttributes, { "data-sicro-quesito-list": "true" }),
      0,
    ];
  },

  addCommands() {
    return {
      insertQuesitoList:
        (initialItems = 1) =>
        ({ commands }) => {
          const items = Array.from(
            { length: Math.max(1, initialItems) },
            (_, i) => defaultQuesitoItem(i + 1),
          );
          return commands.insertContent({
            type: this.name,
            content: items,
          });
        },
      appendQuesito:
        () =>
        ({ chain, state }) => {
          // Find the nearest enclosing quesitoList at the cursor.
          const { $from } = state.selection;
          let depth = $from.depth;
          while (depth > 0 && $from.node(depth).type.name !== "quesitoList") {
            depth -= 1;
          }
          if (depth === 0) return false;
          const node = $from.node(depth);
          const insertAt = $from.before(depth) + node.nodeSize - 1;
          return chain()
            .insertContentAt(insertAt, defaultQuesitoItem(node.childCount + 1))
            .run();
        },
    };
  },
});

export const QuesitoItem = Node.create({
  name: "quesitoItem",
  content: "quesitoQuestion quesitoAnswer",
  defining: true,
  isolating: true,

  parseHTML() {
    return [{ tag: "article[data-sicro-quesito-item]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "article",
      mergeAttributes(HTMLAttributes, { "data-sicro-quesito-item": "true" }),
      0,
    ];
  },
});

export const QuesitoQuestion = Node.create({
  name: "quesitoQuestion",
  content: "inline*",
  defining: true,
  marks: "_",

  parseHTML() {
    return [{ tag: "div[data-sicro-quesito-question]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-sicro-quesito-question": "true",
      }),
      0,
    ];
  },
});

export const QuesitoAnswer = Node.create({
  name: "quesitoAnswer",
  content: "inline*",
  defining: true,
  marks: "_",

  parseHTML() {
    return [{ tag: "div[data-sicro-quesito-answer]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-sicro-quesito-answer": "true",
      }),
      0,
    ];
  },
});

function defaultQuesitoItem(index: number) {
  return {
    type: "quesitoItem",
    content: [
      {
        type: "quesitoQuestion",
        content: [{ type: "text", text: `Pergunta ${index}?` }],
      },
      {
        type: "quesitoAnswer",
        content: [{ type: "text", text: "Resposta do perito." }],
      },
    ],
  };
}
