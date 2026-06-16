/**
 * FieldSuggestion — autocomplete de campos automáticos disparado por `{`.
 *
 * Funciona como o autocomplete de uma IDE: o perito digita `{`, abre um
 * dropdown com TODOS os campos, e conforme digita o filtro reduz. Enter/Tab
 * confirma → o `{query` digitado é substituído por uma PÍLULA (`fieldPlaceholder`),
 * que o NodeView (FieldNodeView) já mostra com o valor resolvido.
 *
 * Implementado sobre `@tiptap/suggestion` (mesma engine do @ menção). O popover
 * é um React root montado num portal ancorado no caret — sem dependência de
 * tippy. Inerte no renderer (`generateHTML` não cria view → o plugin não roda).
 */

import { Extension } from "@tiptap/core";
import Suggestion, { type SuggestionOptions } from "@tiptap/suggestion";
import { createRoot, type Root } from "react-dom/client";
import { createElement } from "react";
import { LAUDO_FIELDS, type LaudoFieldDefinition } from "./catalog";
import {
  FieldSuggestionList,
  type FieldSuggestionListRef,
} from "./FieldSuggestionList";

const norm = (s: string): string =>
  s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase().trim();

/** Filtra o catálogo pelo texto digitado (sem acento; label OU chave). */
function filterFields(query: string): LaudoFieldDefinition[] {
  const q = norm(query);
  if (!q) return [...LAUDO_FIELDS];
  return LAUDO_FIELDS.filter(
    (f) => norm(f.label).includes(q) || norm(f.key).includes(q),
  );
}

export const FieldSuggestion = Extension.create({
  name: "fieldSuggestion",

  addProseMirrorPlugins() {
    const options: SuggestionOptions<LaudoFieldDefinition> = {
      editor: this.editor,
      char: "{",
      // Chaves de campo não têm espaço; o filtro é um token contíguo (igual IDE).
      allowSpaces: false,
      startOfLine: false,
      // Por padrão o plugin só dispara após espaço/início de linha. Liberamos
      // pra QUALQUER posição (ex.: depois de "/" em "{page}/{pages}", ou colado
      // a outro texto). Sem isso, o 2º `{` não abria o menu.
      allowedPrefixes: null,
      items: ({ query }) => filterFields(query),
      command: ({ editor, range, props }) => {
        // Substitui `{query` (o range cobre o `{` + texto digitado) pela pílula.
        editor
          .chain()
          .focus()
          .insertContentAt(range, {
            type: "fieldPlaceholder",
            attrs: { field: props.key },
          })
          .run();
      },
      render: () => {
        let container: HTMLDivElement | null = null;
        let root: Root | null = null;
        const apiRef: { current: FieldSuggestionListRef | null } = {
          current: null,
        };
        let dismissed = false;

        const place = (rect: DOMRect | null | undefined) => {
          if (!container || !rect) return;
          const MENU_W = 280;
          const MAX_H = 320;
          let left = rect.left;
          let top = rect.bottom + 4;
          const vw = window.innerWidth;
          const vh = window.innerHeight;
          if (left + MENU_W > vw - 8) left = Math.max(8, vw - MENU_W - 8);
          // Se não cabe embaixo, abre pra cima.
          if (top + MAX_H > vh - 8 && rect.top - 4 - MAX_H > 8) {
            top = rect.top - 4;
            container.style.transform = "translateY(-100%)";
          } else {
            container.style.transform = "none";
          }
          container.style.left = `${left}px`;
          container.style.top = `${top}px`;
        };

        const draw = (items: LaudoFieldDefinition[], command: (i: LaudoFieldDefinition) => void) => {
          if (!root || dismissed) return;
          root.render(
            createElement(FieldSuggestionList, {
              ref: apiRef,
              items,
              command,
            }),
          );
        };

        return {
          onStart: (props) => {
            dismissed = false;
            container = document.createElement("div");
            container.className = "sicro-field-suggest-portal";
            container.style.position = "fixed";
            container.style.zIndex = "1200";
            document.body.appendChild(container);
            root = createRoot(container);
            draw(props.items, props.command);
            place(props.clientRect?.());
          },
          onUpdate: (props) => {
            if (dismissed) return;
            draw(props.items, props.command);
            place(props.clientRect?.());
          },
          onKeyDown: (props) => {
            if (props.event.key === "Escape") {
              dismissed = true;
              if (container) container.style.display = "none";
              return true;
            }
            if (dismissed) return false;
            return apiRef.current?.onKeyDown(props.event) ?? false;
          },
          onExit: () => {
            // Desmonta no próximo tick — evita "synchronous unmount during
            // render" warning do React quando o exit dispara dentro de um
            // dispatch do ProseMirror.
            const r = root;
            const c = container;
            root = null;
            container = null;
            apiRef.current = null;
            setTimeout(() => {
              try {
                r?.unmount();
              } catch {
                /* noop */
              }
              c?.remove();
            }, 0);
          },
        };
      },
    };

    return [Suggestion(options)];
  },
});
