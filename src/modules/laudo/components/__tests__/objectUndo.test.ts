/**
 * Pós-laudo Ctrl+Z — Testes da DESFAZIBILIDADE de operações de OBJETO.
 *
 * O perito relatou que tabela / caixa de texto / forma / figura ficavam FORA
 * do Ctrl+Z. A auditoria concluiu que:
 *
 *   1. No nível do ProseMirror, TODA transação que as overlays/ops despacham
 *      no mouseup/clique (mover, redimensionar, rotacionar, recolorir, fundo
 *      de célula, altura de linha, inserir/excluir) é UM step de history
 *      desfazível por padrão (sem `addToHistory:false`). Estes testes provam
 *      esse invariante de forma PURA (EditorState + plugin history, sem DOM):
 *      cada ação = +1 de profundidade e `undo` reverte exatamente ela.
 *
 *   2. O bug real estava no ROTEAMENTO do Ctrl+Z em EditorPage (corrigido lá):
 *      o histórico unificado só observava/desfazia o editor do CORPO, então
 *      objetos no cabeçalho/rodapé escapavam. Isso é integração (precisa de 3
 *      instâncias TipTap reais) e não é coberto aqui — mas a lógica por trás
 *      (1 transação desfazível por ação) é o que estes testes garantem.
 *
 * As transações abaixo são MONTADAS EXATAMENTE como as overlays fazem
 * (`setNodeMarkup` + re-afirma a seleção na MESMA transação), pra refletir o
 * código real.
 */

import { describe, expect, it } from "vitest";
import { getSchema } from "@tiptap/core";
import {
  CellSelection,
} from "@tiptap/pm/tables";
import { EditorState, NodeSelection, type Transaction } from "@tiptap/pm/state";
import { history, undo, redo, undoDepth } from "@tiptap/pm/history";
import type { Node as PMNode } from "@tiptap/pm/model";
import { laudoExtensions } from "../../document-engine/extensions";

const schema = getSchema(laudoExtensions());

/** Aplica uma transação e devolve o novo estado. */
function apply(state: EditorState, tr: Transaction): EditorState {
  return state.apply(tr);
}

/** Desfaz um step; devolve { state, ok }. */
function doUndo(state: EditorState): { state: EditorState; ok: boolean } {
  let next = state;
  let ok = false;
  undo(state, (t) => {
    next = state.apply(t);
    ok = true;
  });
  return { state: next, ok };
}

function doRedo(state: EditorState): { state: EditorState; ok: boolean } {
  let next = state;
  let ok = false;
  redo(state, (t) => {
    next = state.apply(t);
    ok = true;
  });
  return { state: next, ok };
}

/** Acha a posição (before) do primeiro nó com o nome dado. */
function posOf(doc: PMNode, typeName: string): number {
  let pos = -1;
  doc.descendants((node, p) => {
    if (pos === -1 && node.type.name === typeName) pos = p;
    return pos === -1;
  });
  return pos;
}

function stateFrom(json: Record<string, unknown>): EditorState {
  return EditorState.create({
    schema,
    doc: schema.nodeFromJSON(json),
    plugins: [history()],
  });
}

// ---- Fixtures de doc ----

function docWithShape() {
  return {
    type: "doc",
    content: [
      {
        type: "shape",
        attrs: {
          id: "s1",
          kind: "rectangle",
          width_cm: 4,
          height_cm: 3,
          wrap_mode: "in_front",
          wrap_x_cm: 2,
          wrap_y_cm: 2,
          rotation: 0,
          stroke_color: "#d92626",
          stroke_width: 3,
          fill_color: "rgba(255,255,255,0)",
        },
      },
      { type: "paragraph", content: [{ type: "text", text: "x" }] },
    ],
  };
}

function docWithTextBox() {
  return {
    type: "doc",
    content: [
      {
        type: "text_box",
        attrs: {
          id: "t1",
          width_cm: 6,
          height_cm: 2,
          wrap_mode: "in_front",
          wrap_x_cm: 3,
          wrap_y_cm: 3,
          rotation: 0,
          border_enabled: true,
          border_color: "#1f2937",
          border_width: 1,
          border_style: "solid",
          fill_enabled: false,
          fill_color: "#ffffff",
          padding_cm: 0.3,
          text_orientation: "horizontal",
        },
        content: [{ type: "paragraph", content: [{ type: "text", text: "oi" }] }],
      },
    ],
  };
}

function docWithFigure() {
  return {
    type: "doc",
    content: [
      {
        type: "figure",
        attrs: {
          id: "f1",
          src: "data:image/png;base64,AAAA",
          alt: "",
          kind: "image",
          width: "70%",
          align: "center",
          wrap_mode: "inline",
          wrap_x_cm: 0,
          wrap_y_cm: 0,
          rotation: 0,
        },
        content: [
          { type: "figcaption", content: [{ type: "text", text: "Foto" }] },
        ],
      },
    ],
  };
}

function cell(text: string) {
  return {
    type: "tableCell",
    attrs: { colspan: 1, rowspan: 1, colwidth: [120] },
    content: [{ type: "paragraph", content: [{ type: "text", text }] }],
  };
}

function docWithTable() {
  return {
    type: "doc",
    content: [
      {
        type: "table",
        attrs: { id: "tbl-1" },
        content: [
          { type: "tableRow", content: [cell("A"), cell("B")] },
          { type: "tableRow", content: [cell("C"), cell("D")] },
        ],
      },
    ],
  };
}

// ---------------------------------------------------------------------------

describe("objeto desfazível — forma (Shape)", () => {
  it("mover/redimensionar (setNodeMarkup) = 1 step e undo reverte", () => {
    let state = stateFrom(docWithShape());
    expect(undoDepth(state)).toBe(0);
    const pos = posOf(state.doc, "shape");
    const node = state.doc.nodeAt(pos)!;
    // Espelha ShapeOverlay.updateShapeAttrs: setNodeMarkup + re-afirma seleção.
    const tr = state.tr.setNodeMarkup(pos, undefined, {
      ...node.attrs,
      wrap_x_cm: 9,
      wrap_y_cm: 8,
      width_cm: 6,
    });
    tr.setSelection(NodeSelection.create(tr.doc, pos));
    state = apply(state, tr);

    expect(undoDepth(state)).toBe(1);
    expect(state.doc.nodeAt(pos)!.attrs.wrap_x_cm).toBe(9);

    const { state: after, ok } = doUndo(state);
    expect(ok).toBe(true);
    expect(after.doc.nodeAt(pos)!.attrs.wrap_x_cm).toBe(2);
    expect(after.doc.nodeAt(pos)!.attrs.width_cm).toBe(4);
  });

  it("recolorir borda (setNodeMarkup) = 1 step, redo reaplica", () => {
    let state = stateFrom(docWithShape());
    const pos = posOf(state.doc, "shape");
    const node = state.doc.nodeAt(pos)!;
    const tr = state.tr.setNodeMarkup(pos, undefined, {
      ...node.attrs,
      stroke_color: "#0000ff",
    });
    state = apply(state, tr);
    expect(undoDepth(state)).toBe(1);

    const undone = doUndo(state);
    expect(undone.state.doc.nodeAt(pos)!.attrs.stroke_color).toBe("#d92626");
    const redone = doRedo(undone.state);
    expect(redone.ok).toBe(true);
    expect(redone.state.doc.nodeAt(pos)!.attrs.stroke_color).toBe("#0000ff");
  });
});

describe("objeto desfazível — caixa de texto (TextBox)", () => {
  it("rotacionar (setNodeMarkup) = 1 step e undo reverte", () => {
    let state = stateFrom(docWithTextBox());
    const pos = posOf(state.doc, "text_box");
    const node = state.doc.nodeAt(pos)!;
    const tr = state.tr.setNodeMarkup(pos, undefined, {
      ...node.attrs,
      rotation: 45,
    });
    tr.setSelection(NodeSelection.create(tr.doc, pos));
    state = apply(state, tr);
    expect(undoDepth(state)).toBe(1);
    expect(state.doc.nodeAt(pos)!.attrs.rotation).toBe(45);

    const { state: after } = doUndo(state);
    expect(after.doc.nodeAt(pos)!.attrs.rotation).toBe(0);
  });

  it("alternar borda/fill (setNodeMarkup) preserva o conteúdo de texto", () => {
    let state = stateFrom(docWithTextBox());
    const pos = posOf(state.doc, "text_box");
    const node = state.doc.nodeAt(pos)!;
    const tr = state.tr.setNodeMarkup(pos, undefined, {
      ...node.attrs,
      fill_enabled: true,
      fill_color: "#ffeeaa",
    });
    state = apply(state, tr);
    expect(undoDepth(state)).toBe(1);
    expect(state.doc.nodeAt(pos)!.textContent).toBe("oi");
    const { state: after } = doUndo(state);
    expect(after.doc.nodeAt(pos)!.attrs.fill_enabled).toBe(false);
    expect(after.doc.nodeAt(pos)!.textContent).toBe("oi");
  });
});

describe("objeto desfazível — figura (Figure)", () => {
  it("alinhar/largura (setNodeMarkup) = 1 step e undo reverte", () => {
    let state = stateFrom(docWithFigure());
    const pos = posOf(state.doc, "figure");
    const node = state.doc.nodeAt(pos)!;
    const tr = state.tr.setNodeMarkup(pos, undefined, {
      ...node.attrs,
      align: "left",
      width: "40%",
    });
    tr.setSelection(NodeSelection.create(tr.doc, pos));
    state = apply(state, tr);
    expect(undoDepth(state)).toBe(1);

    const { state: after } = doUndo(state);
    expect(after.doc.nodeAt(pos)!.attrs.align).toBe("center");
    expect(after.doc.nodeAt(pos)!.attrs.width).toBe("70%");
  });
});

describe("objeto desfazível — tabela (cell bg / row height / estrutura)", () => {
  it("fundo de célula (setCellAttr via CellSelection) = 1 step e undo reverte", () => {
    let state = stateFrom(docWithTable());
    const tablePos = posOf(state.doc, "table");
    // Seleciona a 1ª célula e aplica backgroundColor (espelha
    // setCellAttribute do prosemirror-tables usado por ops.setCellBackground).
    const firstCellPos = tablePos + 2; // table>row>cell (pos do conteúdo)
    const $cell = state.doc.resolve(firstCellPos);
    const tr0 = state.tr.setSelection(new CellSelection($cell));
    state = apply(state, tr0);
    const before = state.doc;

    // Aplica a cor em cada célula da CellSelection (1 transação).
    const tr = state.tr;
    const sel = state.selection as CellSelection;
    sel.forEachCell((node, pos) => {
      tr.setNodeMarkup(pos, undefined, {
        ...node.attrs,
        backgroundColor: "#ffeeaa",
      });
    });
    state = apply(state, tr);
    expect(undoDepth(state)).toBe(1);

    const { state: after, ok } = doUndo(state);
    expect(ok).toBe(true);
    // Conteúdo idêntico ao de antes da cor (sem backgroundColor).
    expect(after.doc.eq(before)).toBe(true);
  });

  it("altura de linha (setNodeMarkup no tableRow) = 1 step e undo reverte", () => {
    let state = stateFrom(docWithTable());
    const tablePos = posOf(state.doc, "table");
    const rowPos = tablePos + 1; // 1ª tableRow
    const rowNode = state.doc.nodeAt(rowPos)!;
    expect(rowNode.type.name).toBe("tableRow");
    const tr = state.tr.setNodeMarkup(rowPos, undefined, {
      ...rowNode.attrs,
      rowHeight: 2.5,
    });
    state = apply(state, tr);
    expect(undoDepth(state)).toBe(1);
    expect(state.doc.nodeAt(rowPos)!.attrs.rowHeight).toBe(2.5);

    const { state: after } = doUndo(state);
    expect(after.doc.nodeAt(rowPos)!.attrs.rowHeight ?? null).toBe(null);
  });

  it("excluir tabela (delete) = 1 step e undo restaura a tabela inteira", () => {
    let state = stateFrom(docWithTable());
    const tablePos = posOf(state.doc, "table");
    const tableNode = state.doc.nodeAt(tablePos)!;
    const tr = state.tr.delete(tablePos, tablePos + tableNode.nodeSize);
    state = apply(state, tr);
    expect(undoDepth(state)).toBe(1);
    expect(posOf(state.doc, "table")).toBe(-1); // sumiu

    const { state: after } = doUndo(state);
    const restored = posOf(after.doc, "table");
    expect(restored).toBeGreaterThanOrEqual(0);
    expect(after.doc.nodeAt(restored)!.textContent).toBe("ABCD");
  });
});
