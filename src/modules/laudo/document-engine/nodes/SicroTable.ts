/**
 * SicroTable — Tabela como objeto de primeira classe (overhaul F1.2–F4).
 *
 * Estende o `@tiptap/extension-table` mantendo o NOME `"table"` (pra não
 * quebrar `isActive("table")`, comandos nativos, extractTables, auto-
 * numeração, importador DOCX, etc.) e ADICIONA atributos NÃO-quebrantes:
 *
 *   - `id`          — UUID estável (cross-refs + auto-numeração), como Figure.
 *   - `caption`     — legenda "Tabela N — …" (texto livre; o "Tabela N —" é
 *                     decoração viva do AutoNumbering). Editável inline via
 *                     NodeView e renderizada como `<caption>` no clone
 *                     estático / export.
 *   - `tableAlign`  — esquerda | centro | direita (margin auto).
 *   - `borderStyle` — "all" (grade completa, default) | "none" (sem bordas,
 *                     estilo bloco de registro) — toggle de bordas.
 *   - `borderColor`/`borderWidth` — cor + espessura (px) das bordas.
 *   - `cellPadding` — padding interno das células (px).
 *
 * F1.2: `resizable: true` (arrastar a borda da coluna). As larguras
 * (`colwidth` das células) são semeadas nos pontos de inserção
 * (`tables/tableDefaults.ts`) porque, com `table-layout: fixed` no CSS,
 * colunas sem `colwidth` colapsam.
 *
 * IMPORTANTE (clone estático do cabeçalho/rodapé): tudo VISUAL precisa
 * funcionar via `renderHTML`/`parseHTML`, porque header/footer fora de
 * edição são `generateHTML(headerExtensions())` — o NodeView não roda lá.
 * Por isso emitimos `<caption>` + `data-*` no renderHTML, e o CSS aplica
 * bordas/align/padding a partir desses atributos.
 */

import { Table, createColGroup } from "@tiptap/extension-table";
import { mergeAttributes } from "@tiptap/core";
import { SicroTableView } from "./SicroTableView";

/** Alinhamento horizontal da tabela na página. */
export type TableAlign = "left" | "center" | "right";
/** Modo de borda: grade completa ou sem bordas (bloco de registro). */
export type TableBorderStyle = "all" | "none";

/** UUID estável pra tabelas (paralelo ao generateFigureId do Figure). */
export function generateTableId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return `tbl-${crypto.randomUUID().slice(0, 8)}`;
  }
  return `tbl-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;
}

export const DEFAULT_TABLE_BORDER_COLOR = "#1a1a1a";
export const DEFAULT_TABLE_BORDER_WIDTH = 1;
export const DEFAULT_TABLE_CELL_PADDING = 5;

declare module "@tiptap/core" {
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  interface Commands<ReturnType> {
    sicroTable: {
      /** Atualiza a legenda da tabela que contém o cursor/seleção. */
      setTableCaption: (caption: string) => ReturnType;
      /** Mostra/esconde a legenda da tabela (estilo Word: remover apaga o
       *  texto; re-adicionar cria vazia). */
      setTableCaptionVisible: (visible: boolean) => ReturnType;
      /** Aplica atributos de apresentação (align/bordas/padding) na tabela
       *  que contém o cursor. */
      setTablePresentation: (attrs: {
        tableAlign?: TableAlign;
        borderStyle?: TableBorderStyle;
        borderColor?: string;
        borderWidth?: number;
        cellPadding?: number;
      }) => ReturnType;
    };
  }
}

/**
 * Style string da `<table>` (alinhamento via margin + variáveis CSS de
 * borda/padding pra o clone estático/export herdar nas células). Usado no
 * renderHTML; o NodeView seta as mesmas variáveis via `style.setProperty`.
 */
export function tablePresentationStyle(attrs: Record<string, unknown>): string {
  const align = (attrs.tableAlign as TableAlign | null) ?? "left";
  const margin =
    align === "center"
      ? "margin: 0.3cm auto;"
      : align === "right"
        ? "margin: 0.3cm 0 0.3cm auto;"
        : "margin: 0.3cm 0;";
  const color = (attrs.borderColor as string | null) ?? DEFAULT_TABLE_BORDER_COLOR;
  const width = Number(attrs.borderWidth ?? DEFAULT_TABLE_BORDER_WIDTH);
  const pad = Number(attrs.cellPadding ?? DEFAULT_TABLE_CELL_PADDING);
  return (
    `${margin}` +
    ` --sicro-table-border-color: ${color};` +
    ` --sicro-table-border-width: ${width}px;` +
    ` --sicro-table-cell-padding: ${pad}px;`
  );
}

/**
 * Atributos `data-*` de apresentação emitidos na `<table>`. O CSS
 * (`styles.css` + `renderer.ts`) lê esses atributos pra desenhar bordas,
 * padding e alinhamento — funciona inclusive no clone estático.
 */
export function tablePresentationDataAttrs(
  attrs: Record<string, unknown>,
): Record<string, string> {
  const align = (attrs.tableAlign as TableAlign | null) ?? "left";
  const border = (attrs.borderStyle as TableBorderStyle | null) ?? "all";
  const color = (attrs.borderColor as string | null) ?? DEFAULT_TABLE_BORDER_COLOR;
  const width = Number(attrs.borderWidth ?? DEFAULT_TABLE_BORDER_WIDTH);
  const pad = Number(attrs.cellPadding ?? DEFAULT_TABLE_CELL_PADDING);
  return {
    "data-table-align": align,
    "data-border-style": border,
    "data-border-color": color,
    "data-border-width": String(width),
    "data-cell-padding": String(pad),
  };
}

export const SicroTable = Table.extend({
  addOptions() {
    const parent = this.parent?.();
    return {
      ...parent,
      // F1.2 — Liga o redimensionamento de coluna (arrastar a borda).
      resizable: true,
      handleWidth: 5,
      cellMinWidth: 32,
      lastColumnResizable: true,
      // F4 — NodeView custom (estende TableView): legenda inline + bordas/
      // align/padding. O columnResizing instancia `new View(node, min, view)`.
      View: SicroTableView,
      // Mantém os defaults obrigatórios do TableOptions (renderWrapper /
      // allowTableNodeSelection) explícitos pra satisfazer o tipo.
      renderWrapper: parent?.renderWrapper ?? false,
      allowTableNodeSelection: parent?.allowTableNodeSelection ?? false,
      // Mantém o marcador usado pelo CSS legado/escopo do clone estático.
      HTMLAttributes: { "data-sicro-table": "true" },
    };
  },

  addAttributes() {
    return {
      // Preserva os attrs nativos do Table + adiciona os nossos.
      ...this.parent?.(),
      // F12.1 — ID estável (UUID). Aditivo: docs antigos sem `id` abrem com
      // null (auto-numeração ainda funciona por ordem do documento).
      id: {
        default: null,
        parseHTML: (el: HTMLElement) => el.getAttribute("data-table-id"),
        renderHTML: (attrs: { id?: string | null }) =>
          attrs.id ? { "data-table-id": String(attrs.id) } : {},
      },
      // F4 — Legenda. Só guarda o texto livre; o prefixo "Tabela N — " é
      // decoração do AutoNumbering (vivo no editor) e re-aplicado no
      // renderer/export pela mesma numeração por ordem.
      caption: {
        default: "",
        parseHTML: (el: HTMLElement) => {
          const cap = el.querySelector(":scope > caption");
          if (cap) return cap.textContent ?? "";
          return el.getAttribute("data-caption") ?? "";
        },
        // NÃO renderiza via attr — vira um elemento <caption> no renderHTML.
        renderHTML: () => ({}),
      },
      // F4.1 — Legenda removível (estilo Word). `false` = a linha "Tabela N —
      // descrição" não existe (nem no editor, nem no clone/export) e a tabela
      // NÃO consome número. Recriável via botão direito ("Adicionar legenda").
      // Aditivo: docs antigos (sem o attr) abrem com legenda visível.
      captionVisible: {
        default: true,
        parseHTML: (el: HTMLElement) =>
          el.getAttribute("data-caption-visible") !== "false",
        renderHTML: (attrs: { captionVisible?: boolean }) =>
          attrs.captionVisible === false
            ? { "data-caption-visible": "false" }
            : {},
      },
      // F4 — Apresentação. Todos aditivos com defaults seguros. Os valores
      // são emitidos JUNTOS em renderHTML (via tablePresentationDataAttrs),
      // então o renderHTML individual é no-op pra não duplicar.
      tableAlign: {
        default: "left",
        parseHTML: (el: HTMLElement) =>
          (el.getAttribute("data-table-align") as TableAlign | null) ?? "left",
        renderHTML: () => ({}),
      },
      borderStyle: {
        default: "all",
        parseHTML: (el: HTMLElement) => {
          const v = el.getAttribute("data-border-style");
          if (v === "none") return "none";
          // Bloco de registro importado do .docx: retângulo externo, sem
          // grade interna → equivale a borderStyle "none".
          if (el.hasAttribute("data-sicro-header-table") && v == null) {
            return "none";
          }
          return "all";
        },
        renderHTML: () => ({}),
      },
      borderColor: {
        default: DEFAULT_TABLE_BORDER_COLOR,
        parseHTML: (el: HTMLElement) =>
          el.getAttribute("data-border-color") ?? DEFAULT_TABLE_BORDER_COLOR,
        renderHTML: () => ({}),
      },
      borderWidth: {
        default: DEFAULT_TABLE_BORDER_WIDTH,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-border-width");
          const n = raw ? parseFloat(raw) : NaN;
          return Number.isFinite(n) ? n : DEFAULT_TABLE_BORDER_WIDTH;
        },
        renderHTML: () => ({}),
      },
      cellPadding: {
        default: DEFAULT_TABLE_CELL_PADDING,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-cell-padding");
          const n = raw ? parseFloat(raw) : NaN;
          return Number.isFinite(n) ? n : DEFAULT_TABLE_CELL_PADDING;
        },
        renderHTML: () => ({}),
      },
    };
  },

  /**
   * renderHTML — usado pelo CLONE ESTÁTICO do header/rodapé e por
   * `generateHTML` no renderer (HTML/PDF). Emite a `<table>` com os
   * atributos de apresentação + um `<caption>` (1º filho, como manda o
   * HTML) quando há legenda, e o conteúdo (linhas) no hole `0`.
   *
   * O prosemirror-tables (editor) gera o `<colgroup>` ao vivo; no clone
   * estático/export as larguras viajam via `data-colwidth` nas células
   * (parseHTML/renderHTML do TableCell base) — intacto.
   */
  renderHTML({ node, HTMLAttributes }) {
    // F1.2 — Reaproveita o `createColGroup` do extension-table pra emitir o
    // `<colgroup>` com as larguras (das `colwidth` das células). Sem isto,
    // o clone estático / export ignorariam o resize de coluna.
    const cellMinWidth = (this.options.cellMinWidth as number) ?? 32;
    const { colgroup, tableWidth, tableMinWidth } = createColGroup(
      node,
      cellMinWidth,
    ) as {
      colgroup?: unknown;
      tableWidth?: string;
      tableMinWidth?: string;
    };
    const widthStyle = tableWidth
      ? `width: ${tableWidth};`
      : tableMinWidth
        ? `min-width: ${tableMinWidth};`
        : "";
    const presentation = mergeAttributes(
      this.options.HTMLAttributes as Record<string, unknown>,
      HTMLAttributes,
      tablePresentationDataAttrs(node.attrs),
      { style: `${tablePresentationStyle(node.attrs)} ${widthStyle}`.trim() },
    );
    // Tabelas de cabeçalho/rodapé (headerExtensions configura
    // `data-sicro-header-table`) são institucionais/layout (ex.: bloco de
    // registro) e NÃO recebem legenda numerada — espelha o NodeView
    // (SicroTableView.isHeaderFooterRegion). Sem isto, a legenda "piscava":
    // aparecia ao editar (NodeView) e sumia ao desfocar (este clone).
    const isHeaderFooterTable = Boolean(
      (this.options.HTMLAttributes as Record<string, unknown> | undefined)?.[
        "data-sicro-header-table"
      ],
    );
    // Sem <caption> também pra: legenda removida (captionVisible=false) e
    // tabela de layout (borderStyle none) — espelha o NodeView/syncCaption.
    const captionAllowed =
      !isHeaderFooterTable &&
      node.attrs.captionVisible !== false &&
      (node.attrs.borderStyle as string | null) !== "none";
    const caption = captionAllowed
      ? String(node.attrs.caption ?? "").trim()
      : "";
    const children: unknown[] = [];
    if (caption) {
      children.push([
        "caption",
        { class: "sicro-table-caption", "data-table-caption": "true" },
        caption,
      ]);
    }
    if (colgroup) children.push(colgroup);
    children.push(["tbody", 0]);
    return ["table", presentation, ...children];
  },

  addCommands() {
    const parent = this.parent?.() ?? {};
    return {
      ...parent,
      setTableCaption:
        (caption: string) =>
        ({ commands }) =>
          // Digitar uma legenda implica querê-la visível.
          commands.updateAttributes(this.name, {
            caption,
            captionVisible: true,
          }),
      setTableCaptionVisible:
        (visible: boolean) =>
        ({ commands }) =>
          commands.updateAttributes(
            this.name,
            // Remover (estilo Word) também apaga o texto; re-adicionar cria
            // vazia (o NodeView mostra o placeholder "descreva a tabela…").
            visible
              ? { captionVisible: true }
              : { captionVisible: false, caption: "" },
          ),
      setTablePresentation:
        (attrs) =>
        ({ commands }) =>
          commands.updateAttributes(this.name, attrs),
    };
  },
});
