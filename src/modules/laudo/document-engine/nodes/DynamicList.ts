/**
 * DynamicList — nodes especiais que renderizam listas atualizadas
 * automaticamente: Sumário (TOC), Lista de Figuras, Lista de Tabelas.
 *
 * F12.3 — Antes (F10) os SummaryPanel apenas INSERIA um snapshot
 * estático no doc. O usuário tinha que regenerar manualmente. Agora os
 * nodes especiais leem o doc atual via NodeView e se auto-atualizam a
 * cada update.
 *
 * Tipos:
 *   - `dynamicSummary`     — Sumário dos headings (titulo_1/2/3).
 *   - `dynamicFigureList`  — Lista de figuras numeradas.
 *   - `dynamicTableList`   — Lista de tabelas numeradas.
 *
 * Cada um é block-level atomic. Render via NodeView consulta o doc atual
 * e renderiza UL ou similar.
 */

import { Node, mergeAttributes } from "@tiptap/core";
import { extractOutline, numberOutline } from "../sections";
import { extractFigures, buildFigureList } from "../figures";
import { extractTables, buildTableList } from "../tables";

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    dynamicLists: {
      insertDynamicSummary: () => ReturnType;
      insertDynamicFigureList: () => ReturnType;
      insertDynamicTableList: () => ReturnType;
    };
  }
}

interface JsonNodeLike {
  type?: string;
  attrs?: Record<string, unknown>;
  content?: unknown[];
  text?: string;
  marks?: Array<{ type: string; attrs?: Record<string, unknown> }>;
}

/** Helper para converter PM doc para JSON-ish que os extractors esperam. */
function toJSON(pmDoc: unknown): JsonNodeLike {
  // PM Node.toJSON() retorna JSONContent — assumimos que está disponível.
  const node = pmDoc as { toJSON?: () => JsonNodeLike };
  return node.toJSON ? node.toJSON() : (pmDoc as JsonNodeLike);
}

// ---------------------------------------------------------------------------
// dynamicSummary

export const DynamicSummary = Node.create({
  name: "dynamicSummary",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,

  parseHTML() {
    return [{ tag: "div[data-dynamic-summary]" }];
  },

  // Marcador VAZIO: o índice real é injetado na EXPORTAÇÃO (o renderer do
  // navegador expande este div na lista de títulos; o walker DOCX o converte em
  // TOC nativo). Sem filhos aqui pra o regex de expansão casar simples/robusto.
  // No editor, o NodeView (addNodeView) é que mostra o índice ao vivo.
  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-dynamic-summary": "true",
        class: "sicro-dynamic-list sicro-dynamic-summary",
      }),
    ];
  },

  addNodeView() {
    return ({ view }) => {
      const wrapper = document.createElement("div");
      wrapper.className = "sicro-dynamic-list sicro-dynamic-summary";
      wrapper.setAttribute("data-dynamic-summary", "true");
      wrapper.contentEditable = "false";

      const render = () => {
        const docJson = toJSON(view.state.doc);
        const outline = numberOutline(
          extractOutline(docJson as never),
        );
        wrapper.innerHTML = "";
        const title = document.createElement("h2");
        title.className = "sicro-dynamic-list-title";
        title.textContent = "SUMÁRIO";
        wrapper.appendChild(title);
        if (outline.length === 0) {
          const empty = document.createElement("div");
          empty.className = "sicro-dynamic-list-placeholder";
          empty.textContent = "Nenhum título encontrado.";
          wrapper.appendChild(empty);
          return;
        }
        const ul = document.createElement("ul");
        ul.className = "sicro-dynamic-list-items";
        for (const o of outline) {
          const li = document.createElement("li");
          li.className = `sicro-dynamic-list-item sicro-dynamic-list-item--level-${o.level}`;
          li.textContent = `${o.numero ?? ""} ${o.text}`.trim();
          ul.appendChild(li);
        }
        wrapper.appendChild(ul);
      };

      render();
      return {
        dom: wrapper,
        update: () => {
          render();
          return true;
        },
      };
    };
  },

  addCommands() {
    return {
      insertDynamicSummary:
        () =>
        ({ commands }) => {
          return commands.insertContent({ type: this.name });
        },
    };
  },
});

// ---------------------------------------------------------------------------
// dynamicFigureList

export const DynamicFigureList = Node.create({
  name: "dynamicFigureList",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,

  parseHTML() {
    return [{ tag: "div[data-dynamic-figure-list]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-dynamic-figure-list": "true",
        class: "sicro-dynamic-list sicro-dynamic-figure-list",
      }),
      ["h2", { class: "sicro-dynamic-list-title" }, "LISTA DE FIGURAS"],
      ["div", { class: "sicro-dynamic-list-placeholder" }, "(gerado automaticamente)"],
    ];
  },

  addNodeView() {
    return ({ view }) => {
      const wrapper = document.createElement("div");
      wrapper.className = "sicro-dynamic-list sicro-dynamic-figure-list";
      wrapper.setAttribute("data-dynamic-figure-list", "true");
      wrapper.contentEditable = "false";

      const render = () => {
        const docJson = toJSON(view.state.doc);
        const figs = buildFigureList(extractFigures(docJson as never));
        wrapper.innerHTML = "";
        const title = document.createElement("h2");
        title.className = "sicro-dynamic-list-title";
        title.textContent = "LISTA DE FIGURAS";
        wrapper.appendChild(title);
        if (figs.length === 0) {
          const empty = document.createElement("div");
          empty.className = "sicro-dynamic-list-placeholder";
          empty.textContent = "Nenhuma figura no documento.";
          wrapper.appendChild(empty);
          return;
        }
        const ul = document.createElement("ul");
        ul.className = "sicro-dynamic-list-items";
        for (const f of figs) {
          const li = document.createElement("li");
          li.className = "sicro-dynamic-list-item";
          li.textContent = `${f.label} — ${f.caption || "(sem legenda)"}`;
          ul.appendChild(li);
        }
        wrapper.appendChild(ul);
      };

      render();
      return {
        dom: wrapper,
        update: () => {
          render();
          return true;
        },
      };
    };
  },

  addCommands() {
    return {
      insertDynamicFigureList:
        () =>
        ({ commands }) => {
          return commands.insertContent({ type: this.name });
        },
    };
  },
});

// ---------------------------------------------------------------------------
// dynamicTableList

export const DynamicTableList = Node.create({
  name: "dynamicTableList",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,

  parseHTML() {
    return [{ tag: "div[data-dynamic-table-list]" }];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      "div",
      mergeAttributes(HTMLAttributes, {
        "data-dynamic-table-list": "true",
        class: "sicro-dynamic-list sicro-dynamic-table-list",
      }),
      ["h2", { class: "sicro-dynamic-list-title" }, "LISTA DE TABELAS"],
      ["div", { class: "sicro-dynamic-list-placeholder" }, "(gerado automaticamente)"],
    ];
  },

  addNodeView() {
    return ({ view }) => {
      const wrapper = document.createElement("div");
      wrapper.className = "sicro-dynamic-list sicro-dynamic-table-list";
      wrapper.setAttribute("data-dynamic-table-list", "true");
      wrapper.contentEditable = "false";

      const render = () => {
        const docJson = toJSON(view.state.doc);
        const tbls = buildTableList(extractTables(docJson as never));
        wrapper.innerHTML = "";
        const title = document.createElement("h2");
        title.className = "sicro-dynamic-list-title";
        title.textContent = "LISTA DE TABELAS";
        wrapper.appendChild(title);
        if (tbls.length === 0) {
          const empty = document.createElement("div");
          empty.className = "sicro-dynamic-list-placeholder";
          empty.textContent = "Nenhuma tabela no documento.";
          wrapper.appendChild(empty);
          return;
        }
        const ul = document.createElement("ul");
        ul.className = "sicro-dynamic-list-items";
        for (const t of tbls) {
          const li = document.createElement("li");
          li.className = "sicro-dynamic-list-item";
          li.textContent = `${t.label} (${t.rowCount}×${t.colCount}) — ${
            t.firstCell || "(sem título)"
          }`;
          ul.appendChild(li);
        }
        wrapper.appendChild(ul);
      };

      render();
      return {
        dom: wrapper,
        update: () => {
          render();
          return true;
        },
      };
    };
  },

  addCommands() {
    return {
      insertDynamicTableList:
        () =>
        ({ commands }) => {
          return commands.insertContent({ type: this.name });
        },
    };
  },
});
