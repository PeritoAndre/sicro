/**
 * useSelectedTable — detecta a TABELA que CONTÉM o cursor/seleção.
 *
 * Diferente de useSelectedFigure/useSelectedShape/useSelectedTextBox: a
 * tabela NÃO é uma NodeSelection (o cursor fica numa célula, em TextSelection
 * ou CellSelection). Então subimos pelos ANCESTRAIS da seleção do ProseMirror
 * procurando o primeiro nó `table` e expomos { pos, node, domEl }.
 *
 * Reaproveita as defesas dos outros hooks: a cada transaction/selectionUpdate
 * recomputa, reusa `prev.domEl` se `nodeDOM` falhar momentaneamente, e só
 * dispara setState quando algo realmente mudou (pos/attrs/dom).
 *
 * IMPORTANTE: o overlay só funciona enquanto AQUELA região (corpo/cabeçalho/
 * rodapé) é o editor ativo — igual aos outros overlays. No clone estático do
 * cabeçalho/rodapé (fora de edição) não há editor → o hook retorna null.
 */

import { useEffect, useState } from "react";
import type { Editor } from "@tiptap/react";
import type { Node as PMNode } from "@tiptap/pm/model";

export interface SelectedTable {
  /** Posição PM do início do nó table (antes do nó). */
  pos: number;
  /** Snapshot dos attrs do nó table (legenda, bordas, align, padding…). */
  attrs: Record<string, unknown>;
  /** Snapshot do nó table (pra contagem de linhas/colunas se preciso). */
  node: PMNode;
  /** Elemento DOM `<table>` correspondente. */
  domEl: HTMLElement;
}

export function useSelectedTable(editor: Editor | null): SelectedTable | null {
  const [selected, setSelected] = useState<SelectedTable | null>(null);

  useEffect(() => {
    if (!editor) {
      setSelected(null);
      return undefined;
    }

    const compute = (prev: SelectedTable | null): SelectedTable | null => {
      const { selection } = editor.state;
      const $from = selection.$from;
      // Sobe os ancestrais da seleção procurando um nó `table`.
      let tablePos: number | null = null;
      let tableNode: PMNode | null = null;
      for (let d = $from.depth; d >= 0; d--) {
        const n = $from.node(d);
        if (n.type.name === "table") {
          tableNode = n;
          // before(d) = posição imediatamente antes do nó no nível d.
          tablePos = d > 0 ? $from.before(d) : 0;
          break;
        }
      }
      if (tablePos == null || !tableNode) return null;

      // Mapeia pos PM → elemento DOM <table>. O NodeView (SicroTableView)
      // expõe o bloco externo como `dom`; o `nodeDOM` retorna esse bloco —
      // então procuramos a <table> dentro dele.
      let domEl: HTMLElement | null = null;
      try {
        const dom = editor.view.nodeDOM(tablePos);
        if (dom instanceof HTMLElement && dom.isConnected) {
          domEl =
            dom.tagName === "TABLE"
              ? dom
              : (dom.querySelector("table") as HTMLElement | null) ?? dom;
        }
      } catch {
        // ignore — fallback abaixo
      }
      if (!domEl && prev && prev.pos === tablePos && prev.domEl.isConnected) {
        domEl = prev.domEl;
      }
      if (!domEl) return null;

      return {
        pos: tablePos,
        attrs: { ...tableNode.attrs },
        node: tableNode,
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
