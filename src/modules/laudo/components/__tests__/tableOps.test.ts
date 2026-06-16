/**
 * Testes da reordenação de tabela no fluxo (F3 — "mover bloco").
 *
 * `computeMoveTableTransaction` é pura (recebe EditorState, devolve
 * Transaction|null). Construímos um EditorState real a partir do schema do
 * laudo (getSchema das extensões) pra validar que mover up/down reordena os
 * blocos irmãos corretamente e que os limites (topo/base) viram no-op.
 */

import { describe, expect, it } from "vitest";
import { getSchema } from "@tiptap/core";
import { EditorState } from "@tiptap/pm/state";
import type { Node as PMNode } from "@tiptap/pm/model";
import { laudoExtensions } from "../../document-engine/extensions";
import { computeMoveTableTransaction } from "../tableOps";

const schema = getSchema(laudoExtensions());

/** Doc: [P("A"), TABLE("T"), P("B")] — tabela no meio. */
function buildDoc() {
  return schema.nodeFromJSON({
    type: "doc",
    content: [
      { type: "paragraph", content: [{ type: "text", text: "A" }] },
      {
        type: "table",
        attrs: { id: "tbl-1" },
        content: [
          {
            type: "tableRow",
            content: [
              {
                type: "tableCell",
                attrs: { colspan: 1, rowspan: 1, colwidth: [100] },
                content: [{ type: "paragraph", content: [{ type: "text", text: "T" }] }],
              },
            ],
          },
        ],
      },
      { type: "paragraph", content: [{ type: "text", text: "B" }] },
    ],
  });
}

/** Posição (before) do nó table no doc. */
function tablePosOf(doc: PMNode): number {
  let pos = -1;
  doc.descendants((node, p) => {
    if (node.type.name === "table" && pos === -1) pos = p;
    return false;
  });
  return pos;
}

/** Sequência dos tipos de bloco de topo (com texto pra paragraphs). */
function topLevelOrder(doc: PMNode): string[] {
  const out: string[] = [];
  doc.forEach((child) => {
    if (child.type.name === "table") out.push("table");
    else out.push(`p:${child.textContent}`);
  });
  return out;
}

describe("computeMoveTableTransaction", () => {
  it("ordem inicial é [A, table, B]", () => {
    const doc = buildDoc();
    expect(topLevelOrder(doc)).toEqual(["p:A", "table", "p:B"]);
  });

  it("mover UP troca a tabela com o parágrafo anterior", () => {
    const doc = buildDoc();
    const state = EditorState.create({ schema, doc });
    const tr = computeMoveTableTransaction(state, tablePosOf(doc), "up");
    expect(tr).not.toBeNull();
    expect(topLevelOrder(tr!.doc)).toEqual(["table", "p:A", "p:B"]);
  });

  it("mover DOWN troca a tabela com o parágrafo seguinte", () => {
    const doc = buildDoc();
    const state = EditorState.create({ schema, doc });
    const tr = computeMoveTableTransaction(state, tablePosOf(doc), "down");
    expect(tr).not.toBeNull();
    expect(topLevelOrder(tr!.doc)).toEqual(["p:A", "p:B", "table"]);
  });

  it("preserva o conteúdo da tabela ao mover", () => {
    const doc = buildDoc();
    const state = EditorState.create({ schema, doc });
    const tr = computeMoveTableTransaction(state, tablePosOf(doc), "down")!;
    const movedTablePos = tablePosOf(tr.doc);
    const moved = tr.doc.nodeAt(movedTablePos)!;
    expect(moved.type.name).toBe("table");
    expect(moved.textContent).toBe("T");
    // attrs preservados (id).
    expect(moved.attrs.id).toBe("tbl-1");
  });

  it("mover UP no topo é no-op (null)", () => {
    // Doc: [table, P] — tabela já é o primeiro bloco.
    const doc = schema.nodeFromJSON({
      type: "doc",
      content: [
        {
          type: "table",
          content: [
            {
              type: "tableRow",
              content: [
                {
                  type: "tableCell",
                  attrs: { colspan: 1, rowspan: 1, colwidth: [100] },
                  content: [{ type: "paragraph" }],
                },
              ],
            },
          ],
        },
        { type: "paragraph", content: [{ type: "text", text: "B" }] },
      ],
    });
    const state = EditorState.create({ schema, doc });
    expect(computeMoveTableTransaction(state, tablePosOf(doc), "up")).toBeNull();
  });

  it("mover DOWN na base é no-op (null)", () => {
    // Doc: [P, table] — tabela já é o último bloco.
    const doc = schema.nodeFromJSON({
      type: "doc",
      content: [
        { type: "paragraph", content: [{ type: "text", text: "A" }] },
        {
          type: "table",
          content: [
            {
              type: "tableRow",
              content: [
                {
                  type: "tableCell",
                  attrs: { colspan: 1, rowspan: 1, colwidth: [100] },
                  content: [{ type: "paragraph" }],
                },
              ],
            },
          ],
        },
      ],
    });
    const state = EditorState.create({ schema, doc });
    expect(computeMoveTableTransaction(state, tablePosOf(doc), "down")).toBeNull();
  });
});
