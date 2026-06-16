/**
 * FontSize — extensão TipTap para armazenar `font-size` inline via `TextStyle`.
 *
 * Não há `@tiptap/extension-font-size` oficial; esta extensão segue o padrão
 * usado por `@tiptap/extension-color` e `@tiptap/extension-font-family`:
 * estende a marca `textStyle` adicionando o atributo `fontSize` que vira
 * `style="font-size: <valor>"` no HTML renderizado.
 *
 * Comandos:
 *   - `setFontSize(size)`     — define o tamanho (string, ex: "14pt").
 *   - `unsetFontSize()`       — remove o atributo (deixa o tamanho padrão).
 *
 * Valores aceitos: qualquer string CSS válida (`"12pt"`, `"1.5em"`, `"16px"`).
 * O catálogo de tamanhos exposto na toolbar fica em `EditorToolbar`.
 */

import { Extension } from "@tiptap/core";

export interface FontSizeOptions {
  /** Nós aos quais o atributo se aplica. */
  types: string[];
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    fontSize: {
      setFontSize: (size: string) => ReturnType;
      unsetFontSize: () => ReturnType;
    };
  }
}

export const FontSize = Extension.create<FontSizeOptions>({
  name: "fontSize",

  addOptions() {
    return {
      types: ["textStyle"],
    };
  },

  addGlobalAttributes() {
    return [
      {
        types: this.options.types,
        attributes: {
          fontSize: {
            default: null,
            parseHTML: (element) =>
              element.style.fontSize?.replace(/['"]+/g, "") || null,
            renderHTML: (attributes) => {
              const value = attributes["fontSize"];
              if (!value) return {};
              return { style: `font-size: ${value}` };
            },
          },
        },
      },
    ];
  },

  addCommands() {
    return {
      setFontSize:
        (size: string) =>
        ({ chain }) => {
          return chain().setMark("textStyle", { fontSize: size }).run();
        },
      unsetFontSize:
        () =>
        ({ chain }) => {
          return chain()
            .setMark("textStyle", { fontSize: null })
            .removeEmptyTextStyle()
            .run();
        },
    };
  },
});
