/**
 * U — Hook que detecta a TextBox atualmente selecionada no editor.
 *
 * Mesmo padrão do useSelectedShape: a cada update (transaction +
 * selectionUpdate), verifica se a seleção atual é uma NodeSelection
 * sobre um `text_box`. Se sim, expõe { pos, attrs, domEl }.
 *
 * Defensive: reusa `prev.domEl` se `editor.view.nodeDOM` falhar
 * momentaneamente entre transactions (race comum quando o NodeView
 * está sendo recriado).
 */

import { useEffect, useState } from "react";
import type { Editor } from "@tiptap/react";
import { NodeSelection } from "@tiptap/pm/state";

export interface SelectedTextBox {
  pos: number;
  attrs: Record<string, unknown>;
  domEl: HTMLElement;
}

export function useSelectedTextBox(
  editor: Editor | null,
): SelectedTextBox | null {
  const [selected, setSelected] = useState<SelectedTextBox | null>(null);

  useEffect(() => {
    if (!editor) {
      setSelected(null);
      return undefined;
    }

    const compute = (
      prev: SelectedTextBox | null,
    ): SelectedTextBox | null => {
      const sel = editor.state.selection;
      if (!(sel instanceof NodeSelection)) return null;
      const node = sel.node;
      if (node.type.name !== "text_box") return null;
      let domEl: HTMLElement | null = null;
      try {
        const dom = editor.view.nodeDOM(sel.from);
        // Pós-laudo U fix — exige `.isConnected`. Quando sai do modo
        // header, o EditorContent é desmontado, e o nodeDOM do header
        // editor fica detached. Sem isso, o overlay segurava um domEl
        // detached e renderizava com rect lixo/cached em posição
        // errada (visualmente "desassociado" do textbox real).
        if (dom instanceof HTMLElement && dom.isConnected) domEl = dom;
      } catch {
        // ignore
      }
      // Fallback pra cached só se o cached AINDA estiver conectado.
      if (!domEl && prev && prev.pos === sel.from && prev.domEl.isConnected) {
        domEl = prev.domEl;
      }
      if (!domEl) return null;
      return {
        pos: sel.from,
        attrs: { ...node.attrs },
        domEl,
      };
    };

    const update = () => {
      setSelected((prev) => {
        const next = compute(prev);
        if (!next && !prev) return prev;
        if (!next || !prev) return next;
        if (
          prev.pos === next.pos &&
          prev.domEl === next.domEl &&
          JSON.stringify(prev.attrs) === JSON.stringify(next.attrs)
        ) {
          return prev;
        }
        return next;
      });
    };

    update();
    editor.on("transaction", update);
    editor.on("selectionUpdate", update);

    return () => {
      editor.off("transaction", update);
      editor.off("selectionUpdate", update);
    };
  }, [editor]);

  return selected;
}
