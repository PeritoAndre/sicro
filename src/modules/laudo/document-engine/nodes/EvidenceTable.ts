/**
 * EvidenceTable — block-level table generated from Dossiê data (checklist,
 * vestígios, medições). Keeps its own node so the renderer can apply a
 * `data-evidence-table` attribute and the export pipeline can style /
 * walk it consistently.
 *
 * The body uses the existing `table` semantics indirectly: this node
 * carries the data as JSON and renders <table> on the fly. We don't
 * reuse @tiptap/extension-table because we want the data to be
 * immutable (auto-generated from the Dossiê) and we don't need cell
 * editing for the spike — the perito edits in the Dossiê if anything
 * is wrong.
 */

import { Node, mergeAttributes } from "@tiptap/core";

export type EvidenceTableKind = "checklist" | "traces" | "measurements";

export interface EvidenceTableColumn {
  key: string;
  label: string;
}

export interface EvidenceTableRow {
  [k: string]: string | number | null | undefined;
}

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    evidenceTable: {
      insertEvidenceTable: (params: {
        kind: EvidenceTableKind;
        title?: string;
        columns: EvidenceTableColumn[];
        rows: EvidenceTableRow[];
        metadata_json?: string | null;
      }) => ReturnType;
    };
  }
}

export const EvidenceTable = Node.create({
  name: "evidenceTable",
  group: "block",
  atom: true,
  draggable: true,

  addAttributes() {
    return {
      kind: { default: "checklist" },
      title: { default: null },
      columns: { default: [] },
      rows: { default: [] },
      metadata_json: { default: null },
    };
  },

  parseHTML() {
    return [{ tag: "table[data-sicro-evidence-table]" }];
  },

  renderHTML({ node }) {
    const kind = (node.attrs.kind as string) ?? "checklist";
    const cols = (node.attrs.columns as EvidenceTableColumn[]) ?? [];
    const rows = (node.attrs.rows as EvidenceTableRow[]) ?? [];
    const title = node.attrs.title as string | null;

    const caption = title
      ? [["caption", { "data-sicro-evidence-table-title": "true" }, title]]
      : [];
    const head = [
      "thead",
      {},
      ["tr", {}, ...cols.map((c) => ["th", { "data-key": c.key }, c.label])],
    ];
    const bodyRows = rows.map((r) => [
      "tr",
      {},
      ...cols.map((c) => ["td", { "data-key": c.key }, cellText(r[c.key])]),
    ]);
    const body = ["tbody", {}, ...bodyRows];

    return [
      "table",
      mergeAttributes(
        {},
        {
          "data-sicro-evidence-table": "true",
          "data-kind": kind,
          "data-sicro-table": "true", // reuse the existing print CSS
        },
      ),
      ...caption,
      head,
      body,
    ] as never;
  },

  addCommands() {
    return {
      insertEvidenceTable:
        ({ kind, title, columns, rows, metadata_json }) =>
        ({ commands }) => {
          if (columns.length === 0) return false;
          return commands.insertContent({
            type: this.name,
            attrs: {
              kind,
              title: title ?? null,
              columns,
              rows,
              metadata_json: metadata_json ?? null,
            },
          });
        },
    };
  },
});

function cellText(value: string | number | null | undefined): string {
  if (value === null || value === undefined) return "—";
  if (typeof value === "number") {
    if (!Number.isFinite(value)) return "—";
    return Number.isInteger(value) ? String(value) : value.toFixed(2);
  }
  const s = String(value).trim();
  return s.length === 0 ? "—" : s;
}
