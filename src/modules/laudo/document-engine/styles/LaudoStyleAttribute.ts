/**
 * LaudoStyleAttribute — extensão TipTap que adiciona o atributo
 * `data-laudo-style` aos nós `paragraph` e `heading`.
 *
 * Esta é a coluna vertebral do sistema de estilos pericial. Ao invés de
 * criar 14 nodes/marks separados, atacamos o problema com UM atributo
 * declarativo que mapeia para CSS via classe `[data-laudo-style="<id>"]`.
 *
 * Vantagens:
 *   - Trabalha com Heading/Paragraph existentes (StarterKit) — sem migration.
 *   - Persiste no `.sicrodoc` como atributo simples.
 *   - DOCX walker pode ler o atributo e mapear para Style do Word.
 *   - HTML renderer mantém via attribute pass-through.
 *
 * Comandos:
 *   - `setLaudoStyle(id)` — aplica o estilo ao nó atual (ou seleção).
 *   - `unsetLaudoStyle()` — remove (volta ao Normal/heading default).
 */

import { Extension } from "@tiptap/core";
import type { LaudoStyleId } from "./definitions";

export interface LaudoStyleAttributeOptions {
  /** Nós aos quais o atributo se aplica. */
  types: string[];
}

declare module "@tiptap/core" {
  interface Commands<ReturnType> {
    laudoStyle: {
      /**
       * Aplica um estilo do catálogo ao bloco atual (paragraph ou heading).
       * Quando o estilo alvo é heading, também ajusta o `level` do nó.
       */
      setLaudoStyle: (id: LaudoStyleId) => ReturnType;
      /** Remove o atributo `data-laudo-style`. */
      unsetLaudoStyle: () => ReturnType;
    };
  }
}

export const LaudoStyleAttribute = Extension.create<LaudoStyleAttributeOptions>({
  name: "laudoStyleAttribute",

  addOptions() {
    return {
      // Aplicamos a paragraph + heading. StarterKit cobre ambos.
      types: ["paragraph", "heading"],
    };
  },

  addGlobalAttributes() {
    return [
      {
        types: this.options.types,
        attributes: {
          laudoStyle: {
            default: null,
            parseHTML: (element) =>
              element.getAttribute("data-laudo-style") || null,
            renderHTML: (attributes) => {
              const value = attributes["laudoStyle"];
              if (!value) return {};
              // Atributo `data-*` + classe utilitária facilitam tanto
              // o CSS interno do editor quanto a exportação HTML/PDF.
              return {
                "data-laudo-style": value,
                class: `sicro-laudo-style sicro-laudo-style--${value}`,
              };
            },
          },
        },
      },
    ];
  },

  addCommands() {
    return {
      setLaudoStyle:
        (id: LaudoStyleId) =>
        ({ commands, chain, state }) => {
          // Para estilos que vinculam a heading (level específico), o
          // applyStyle.ts cuida da conversão paragraph↔heading. Aqui apenas
          // grudamos o atributo nos nós suportados que JÁ existem na seleção.
          //
          // Estratégia simples: usar `updateAttributes` para cada tipo.
          // Se o nó atual é paragraph e o estilo é heading, o helper
          // applyStyle vai delegar para `setNode("heading", { level })`
          // ANTES de chamar `setLaudoStyle`.
          const types = this.options.types;
          // Encontra o tipo do nó na seleção (assume o primeiro suportado).
          let chosen: string | null = null;
          state.doc.nodesBetween(state.selection.from, state.selection.to, (node) => {
            if (chosen) return false;
            if (types.includes(node.type.name)) {
              chosen = node.type.name;
              return false;
            }
            return true;
          });
          if (!chosen) {
            // Fallback: tenta paragraph como menos restritivo.
            chosen = types[0] ?? "paragraph";
          }
          return chain()
            .updateAttributes(chosen, { laudoStyle: id })
            .run();
          // `commands` é usado para satisfazer o tipo do TipTap; sem isso
          // o lint reclama de unused-import indireto.
          void commands;
        },
      unsetLaudoStyle:
        () =>
        ({ chain, state }) => {
          const types = this.options.types;
          let chosen: string | null = null;
          state.doc.nodesBetween(state.selection.from, state.selection.to, (node) => {
            if (chosen) return false;
            if (types.includes(node.type.name)) {
              chosen = node.type.name;
              return false;
            }
            return true;
          });
          if (!chosen) return false;
          return chain()
            .updateAttributes(chosen, { laudoStyle: null })
            .run();
        },
    };
  },
});
