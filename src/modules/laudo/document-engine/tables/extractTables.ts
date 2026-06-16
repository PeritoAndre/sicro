/**
 * extractTables — varre o documento e devolve a lista de tabelas
 * ordenadas por aparição.
 *
 * F7 — Paralelo ao `extractFigures`/`extractOutline`. Usado pela
 * "Lista de Tabelas" futura (F9) e pelo TablesPanel para mostrar
 * "Tabela 1, 2…" no painel de navegação rápida.
 */

import type { JSONContent } from "@tiptap/core";

export interface TableEntry {
  /** Posição ProseMirror do nó table. */
  pos: number;
  /** Número de linhas (incluindo header). */
  rowCount: number;
  /** Número de colunas estimado pela primeira linha. */
  colCount: number;
  /** Primeira célula do cabeçalho ou primeira célula da primeira linha. */
  firstCell: string;
}

export interface NumberedTableEntry extends TableEntry {
  /** Número formatado: "Tabela 1", "Tabela 2", … */
  label: string;
  /** Apenas o número. */
  ordinal: number;
}

export function extractTables(content: JSONContent): TableEntry[] {
  const out: TableEntry[] = [];
  walk(content, 0, out);
  return out;
}

export function buildTableList(
  tables: ReadonlyArray<TableEntry>,
): NumberedTableEntry[] {
  return tables.map((t, idx) => ({
    ...t,
    ordinal: idx + 1,
    label: `Tabela ${idx + 1}`,
  }));
}

function walk(node: JSONContent, basePos: number, out: TableEntry[]): number {
  const type = node.type;
  if (!type) return basePos;
  if (type === "doc") {
    let p = basePos;
    for (const child of node.content ?? []) {
      p = walk(child, p, out);
    }
    return p;
  }
  if (type === "text") {
    return basePos + (node.text?.length ?? 0);
  }

  const openPos = basePos;
  let inner = basePos + 1;

  if (type === "table") {
    // Tabelas de LAYOUT (bloco de registro/timbre, `borderStyle: "none"`) e
    // com legenda REMOVIDA (captionVisible=false) não entram na lista de
    // tabelas numeradas (espelha numberFigures + AutoNumbering +
    // SicroTableView).
    if (
      (node.attrs?.borderStyle as string | undefined) !== "none" &&
      node.attrs?.captionVisible !== false
    ) {
      out.push(summarizeTable(node, openPos));
    }
  }

  for (const child of node.content ?? []) {
    inner = walk(child, inner, out);
  }
  return inner + 1;
}

function summarizeTable(node: JSONContent, pos: number): TableEntry {
  const rows = (node.content ?? []).filter((c) => c.type === "tableRow");
  const rowCount = rows.length;
  const firstRow = rows[0];
  const cells = (firstRow?.content ?? []).filter(
    (c) => c.type === "tableCell" || c.type === "tableHeader",
  );
  const colCount = cells.length;
  // Texto da primeira célula (usado como "title" no painel).
  let firstCellText = "";
  if (cells[0]) {
    firstCellText = collectText(cells[0]).trim();
  }
  return { pos, rowCount, colCount, firstCell: firstCellText };
}

function collectText(node: JSONContent): string {
  let buf = "";
  const visit = (n: JSONContent) => {
    if (n.type === "text" && typeof n.text === "string") {
      buf += n.text;
    }
    if (n.content) for (const c of n.content) visit(c);
  };
  visit(node);
  return buf;
}
