/**
 * tableOps — operações de tabela compartilhadas entre o TableOverlay
 * (toolbar flutuante) e o menu de contexto (botão direito). Centraliza os
 * comandos nativos do TipTap table + as operações próprias (mover bloco,
 * alinhar, legenda) pra não duplicar lógica.
 *
 * Todas as ops focam o editor antes de agir (o overlay só aparece quando o
 * cursor já está na tabela, mas o foco pode ter ido pra um input da toolbar).
 */

import type { Editor } from "@tiptap/react";
import { TextSelection, type EditorState, type Transaction } from "@tiptap/pm/state";
import type { TableAlign } from "../document-engine";

export interface TableOps {
  addRowBefore: () => void;
  addRowAfter: () => void;
  deleteRow: () => void;
  addColumnBefore: () => void;
  addColumnAfter: () => void;
  deleteColumn: () => void;
  mergeCells: () => void;
  splitCell: () => void;
  toggleHeaderRow: () => void;
  deleteTable: () => void;
  setAlign: (align: TableAlign) => void;
  /** Define a cor de fundo das célula(s) selecionada(s) (attr
   *  `backgroundColor`). `null` limpa (sem cor / transparente). Aplica em
   *  TODAS as células de uma CellSelection (multi-célula). */
  setCellBackground: (color: string | null) => void;
  /** Move o nó tabela pra cima/baixo entre os blocos irmãos (reordenar). */
  moveBlock: (dir: "up" | "down") => void;
  /** Mostra/esconde a legenda "Tabela N — …" (estilo Word: remover apaga o
   *  texto; adicionar cria vazia, com placeholder). */
  toggleCaption: () => void;
  /** A legenda está visível? (default true; tabelas novas nascem com ela) */
  captionVisible: () => boolean;
  /** Legenda se aplica? Falso em cabeçalho/rodapé (tabela institucional) e
   *  em tabelas de layout (sem borda) — nesses casos o menu esconde o item. */
  canToggleCaption: () => boolean;
  canMergeCells: () => boolean;
  canSplitCell: () => boolean;
  canMoveUp: () => boolean;
  canMoveDown: () => boolean;
}

/**
 * PURA — calcula a transação que move o nó `table` em `tablePos` pra ANTES do
 * irmão anterior (up) ou pra DEPOIS do irmão seguinte (down), dentro do mesmo
 * pai. Reposiciona o cursor na tabela movida. Retorna `null` se não houver pra
 * onde mover (já é o primeiro/último irmão) ou se `tablePos` não for tabela.
 *
 * Separada de `moveTableBlock` pra ser testável sem Editor/view (só precisa
 * de um EditorState). Mantém a paginação intacta (é só reordenação de blocos).
 */
export function computeMoveTableTransaction(
  state: EditorState,
  tablePos: number,
  dir: "up" | "down",
): Transaction | null {
  const $pos = state.doc.resolve(tablePos);
  const parent = $pos.parent;
  const indexInParent = $pos.index();
  const tableNode = state.doc.nodeAt(tablePos);
  if (!tableNode || tableNode.type.name !== "table") return null;

  const siblingCount = parent.childCount;
  if (dir === "up" && indexInParent <= 0) return null;
  if (dir === "down" && indexInParent >= siblingCount - 1) return null;

  const tableSize = tableNode.nodeSize;
  let tr = state.tr;
  // Remove a tabela.
  tr = tr.delete(tablePos, tablePos + tableSize);

  // Calcula a posição de inserção no doc MODIFICADO.
  let insertPos: number;
  if (dir === "up") {
    // Antes do irmão anterior: pos do início do irmão anterior.
    insertPos = tablePos - parent.child(indexInParent - 1).nodeSize;
  } else {
    // Depois do irmão seguinte. O irmão seguinte agora começa em `tablePos`
    // (a tabela foi removida); pulamos o tamanho dele.
    const nextSib = parent.child(indexInParent + 1);
    insertPos = tablePos + nextSib.nodeSize;
  }

  tr = tr.insert(insertPos, tableNode);
  // Cursor na primeira célula da tabela movida. +3 entra em table>row>cell;
  // o TextSelection.near ajusta pra a posição editável mais próxima.
  try {
    const sel = TextSelection.near(
      tr.doc.resolve(Math.min(insertPos + 3, tr.doc.content.size)),
    );
    tr = tr.setSelection(sel);
  } catch {
    // ignore — selection best-effort
  }
  return tr.scrollIntoView();
}

/**
 * Move o nó `table` (ver `computeMoveTableTransaction`) e despacha a transação
 * no editor. Reposiciona o cursor na tabela movida. No-op (false) se não
 * houver pra onde mover.
 */
export function moveTableBlock(editor: Editor, tablePos: number, dir: "up" | "down"): boolean {
  const tr = computeMoveTableTransaction(editor.state, tablePos, dir);
  if (!tr) return false;
  editor.view.dispatch(tr);
  return true;
}

/** Pode mover pra cima? (há irmão anterior no mesmo pai) */
export function canMoveTableUp(editor: Editor, tablePos: number): boolean {
  try {
    const $pos = editor.state.doc.resolve(tablePos);
    return $pos.index() > 0;
  } catch {
    return false;
  }
}

/** Pode mover pra baixo? (há irmão seguinte no mesmo pai) */
export function canMoveTableDown(editor: Editor, tablePos: number): boolean {
  try {
    const $pos = editor.state.doc.resolve(tablePos);
    return $pos.index() < $pos.parent.childCount - 1;
  } catch {
    return false;
  }
}

/**
 * Constrói o objeto de ops pra um editor + posição de tabela. As ops de
 * linha/coluna/célula usam os comandos NATIVOS do TipTap, que agem sobre a
 * seleção atual — por isso focamos o editor e (defensivo) garantimos que a
 * seleção esteja dentro da tabela antes.
 */
export function buildTableOps(
  editor: Editor,
  tablePos: number,
): TableOps {
  const run = (fn: () => void) => {
    editor.commands.focus();
    fn();
  };
  return {
    addRowBefore: () => run(() => editor.chain().focus().addRowBefore().run()),
    addRowAfter: () => run(() => editor.chain().focus().addRowAfter().run()),
    deleteRow: () => run(() => editor.chain().focus().deleteRow().run()),
    addColumnBefore: () =>
      run(() => editor.chain().focus().addColumnBefore().run()),
    addColumnAfter: () =>
      run(() => editor.chain().focus().addColumnAfter().run()),
    deleteColumn: () => run(() => editor.chain().focus().deleteColumn().run()),
    mergeCells: () => run(() => editor.chain().focus().mergeCells().run()),
    splitCell: () => run(() => editor.chain().focus().splitCell().run()),
    toggleHeaderRow: () =>
      run(() => editor.chain().focus().toggleHeaderRow().run()),
    deleteTable: () => run(() => editor.chain().focus().deleteTable().run()),
    setAlign: (align) =>
      run(() =>
        editor.chain().focus().setTablePresentation({ tableAlign: align }).run(),
      ),
    setCellBackground: (color) =>
      // `setCellAttribute` usa o `setCellAttr` do prosemirror-tables, que
      // aplica em TODAS as células de uma CellSelection (ou na célula única).
      run(() =>
        editor
          .chain()
          .focus()
          .setCellAttribute("backgroundColor", color)
          .run(),
      ),
    moveBlock: (dir) => moveTableBlock(editor, tablePos, dir),
    toggleCaption: () => {
      const node = editor.state.doc.nodeAt(tablePos);
      const visible = node?.attrs.captionVisible !== false;
      run(() =>
        editor.chain().focus().setTableCaptionVisible(!visible).run(),
      );
    },
    captionVisible: () => {
      const node = editor.state.doc.nodeAt(tablePos);
      return node?.attrs.captionVisible !== false;
    },
    canToggleCaption: () => {
      // Cabeçalho/rodapé: tabela institucional, sem legenda (vide
      // SicroTableView.isHeaderFooterRegion). Layout (sem borda): idem.
      const region = editor.view.dom.getAttribute("data-sicro-region");
      if (region === "header" || region === "footer") return false;
      const node = editor.state.doc.nodeAt(tablePos);
      return (node?.attrs.borderStyle as string | null) !== "none";
    },
    canMergeCells: () => editor.can().mergeCells(),
    canSplitCell: () => editor.can().splitCell(),
    canMoveUp: () => canMoveTableUp(editor, tablePos),
    canMoveDown: () => canMoveTableDown(editor, tablePos),
  };
}
