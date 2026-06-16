/**
 * SicroTableRow / SicroTableCell / SicroTableHeader — estendem os nós nativos
 * do `@tiptap/extension-table-*` com atributos ADITIVOS do overhaul:
 *
 *   - tableRow.rowHeight  — altura mínima da linha em cm (F3). Serializa em
 *     `data-row-height-cm` no `<tr>` (clone estático / export) e em CSS
 *     `min-height` na célula. Aditivo: docs antigos abrem com null.
 *   - tableCell/tableHeader["data-valign"] — alinhamento vertical do conteúdo
 *     (F4 / TablePropertiesDialog). Já era escrito pelo dialog antigo, mas o
 *     attr NÃO estava registrado no schema → era descartado silenciosamente.
 *     Agora persiste de verdade e o CSS/walker DOCX o leem.
 *   - tableCell/tableHeader.backgroundColor — cor de FUNDO da célula (sombreado
 *     estilo Word). null = sem cor (transparente, default). Serializa em
 *     `style="background-color:…"` + `data-cell-bg` no `<td>`/`<th>` (clone
 *     estático/HTML/PDF) e em `CellShading`/`<w:shd>` no DOCX (walker Rust).
 *     Aditivo: docs antigos abrem com null (transparente).
 *
 * Mantêm os NOMES nativos (`tableRow`/`tableCell`/`tableHeader`) pra não
 * quebrar os comandos do prosemirror-tables nem o schema existente.
 */

import { TableRow } from "@tiptap/extension-table-row";
import { TableCell } from "@tiptap/extension-table-cell";
import { TableHeader } from "@tiptap/extension-table-header";

/**
 * Normaliza uma cor de fundo de célula vinda do attr para uma string CSS
 * segura, ou `null` quando "sem cor". Aceita `#RGB`/`#RRGGBB`, `rgb()/rgba()`
 * e nomes simples. Strings vazias/`"transparent"`/`"none"` viram null pra que
 * o default (transparente) NÃO emita nenhum `background-color` — o que mantém
 * docs antigos e tabelas recém-inseridas visualmente neutros.
 */
function normalizeCellBg(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const v = value.trim();
  if (!v || v.toLowerCase() === "transparent" || v.toLowerCase() === "none") {
    return null;
  }
  return v;
}

/**
 * Atributo `backgroundColor` compartilhado por célula e cabeçalho.
 *
 * - parseHTML lê o `data-cell-bg` (clone estático / export) OU o `style`
 *   inline (`background-color`). Sem valor ⇒ null (transparente).
 * - renderHTML emite TANTO `data-cell-bg` (parse robusto) QUANTO o `style`
 *   inline `background-color`. A célula NÃO tem NodeView próprio (só a tabela
 *   tem), então o TipTap aplica este renderHTML direto na `<td>`/`<th>` ao
 *   vivo — e o MESMO caminho roda no clone estático do cabeçalho/rodapé e no
 *   export HTML/PDF (generateHTML). Um único ponto de verdade pro sombreado.
 */
const backgroundColorAttr = {
  backgroundColor: {
    default: null as string | null,
    parseHTML: (el: HTMLElement): string | null => {
      const data = el.getAttribute("data-cell-bg");
      if (data) return normalizeCellBg(data);
      return normalizeCellBg(el.style.backgroundColor);
    },
    renderHTML: (attrs: Record<string, unknown>) => {
      const color = normalizeCellBg(attrs.backgroundColor);
      if (!color) return {};
      return {
        "data-cell-bg": color,
        style: `background-color: ${color};`,
      };
    },
  },
};

export const SicroTableRow = TableRow.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      // F3 — Altura da linha em cm. null = auto (conteúdo define).
      rowHeight: {
        default: null,
        parseHTML: (el: HTMLElement) => {
          const raw = el.getAttribute("data-row-height-cm");
          if (!raw) return null;
          const n = parseFloat(raw);
          return Number.isFinite(n) && n > 0 ? n : null;
        },
        renderHTML: (attrs: { rowHeight?: number | null }) => {
          const h = attrs.rowHeight;
          if (!h || !Number.isFinite(h)) return {};
          // `data-row-height-cm` (parse) + `style` min-height (clone estático).
          return {
            "data-row-height-cm": String(h),
            style: `height: ${h}cm;`,
          };
        },
      },
    };
  },
});

/** Atributo `data-valign` compartilhado por célula e cabeçalho. */
const valignAttr = {
  "data-valign": {
    default: null as string | null,
    parseHTML: (el: HTMLElement) => el.getAttribute("data-valign"),
    renderHTML: (attrs: Record<string, unknown>) => {
      const v = attrs["data-valign"];
      if (v === "middle" || v === "bottom" || v === "top") {
        return { "data-valign": v };
      }
      return {};
    },
  },
};

export const SicroTableCell = TableCell.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      ...valignAttr,
      ...backgroundColorAttr,
    };
  },
});

export const SicroTableHeader = TableHeader.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      ...valignAttr,
      ...backgroundColorAttr,
    };
  },
});
