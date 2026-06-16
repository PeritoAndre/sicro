/**
 * Q — Hook que detecta a shape atualmente selecionada no editor.
 *
 * Funciona por observação do estado do TipTap: a cada update, verifica
 * se `editor.state.selection` é uma NodeSelection sobre um `shape`. Se
 * sim, devolve { pos, attrs, domEl } pro overlay React posicionar
 * handles. Caso contrário, retorna null.
 *
 * Mesmo padrão do useSelectedFigure (P19 defensive: reusa prev.domEl
 * se nodeDOM falhar momentaneamente entre transactions).
 */

import { useEffect, useState } from "react";
import type { Editor } from "@tiptap/react";
import { NodeSelection } from "@tiptap/pm/state";

export interface SelectedShape {
  pos: number;
  attrs: Record<string, unknown>;
  domEl: HTMLElement;
}

export function useSelectedShape(editor: Editor | null): SelectedShape | null {
  const [selected, setSelected] = useState<SelectedShape | null>(null);

  useEffect(() => {
    if (!editor) {
      setSelected(null);
      return undefined;
    }

    const compute = (prev: SelectedShape | null): SelectedShape | null => {
      const sel = editor.state.selection;
      if (!(sel instanceof NodeSelection)) return null;
      const node = sel.node;
      if (node.type.name !== "shape") return null;
      let domEl: HTMLElement | null = null;
      try {
        const dom = editor.view.nodeDOM(sel.from);
        if (dom instanceof HTMLElement) domEl = dom;
      } catch {
        // ignore
      }
      if (!domEl && prev && prev.pos === sel.from) {
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
