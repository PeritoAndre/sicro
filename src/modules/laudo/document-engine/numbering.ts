/**
 * Walk a TipTap JSON doc and inject sequential numbers into every `table`
 * node (prepending "Tabela N — " to the table `caption` attr).
 *
 * FIGURAS NÃO SÃO MAIS NUMERADAS automaticamente: a legenda da foto/croqui
 * é digitada manualmente pelo perito (inline ou pelo campo de legenda no
 * menu flutuante da figura). O nó `figure` passa INTACTO por aqui — nenhum
 * prefixo "Figura N — " / "Croqui N — " é prependido e o figcaption não é
 * tocado. Apenas as TABELAS continuam recebendo "Tabela N — ".
 *
 * Numbering is computed at render time (not persisted) so reordering blocks
 * in the editor doesn't require a save. Mirrors the live `AutoNumbering`
 * decoration of the editor so the exported PDF/DOCX matches the screen.
 */

import type { JSONContent } from "@tiptap/core";

interface Counters {
  table: number;
}

export function numberFigures(content: JSONContent): JSONContent {
  const counters: Counters = { table: 0 };
  return walk(content, counters);
}

function walk(node: JSONContent, counters: Counters): JSONContent {
  // FIGURA — não numera. Passa o nó intacto (mas continua descendo nos
  // descendentes, abaixo, pra não interromper a recursão em filhos).
  // Nenhum prefixo é prependido; o figcaption fica como o perito digitou.

  // F4 — Tabela: numera "Tabela N — " no attr `caption` (renderizado como
  // `<caption>` pelo SicroTable). Numeração por ORDEM de aparição, igual ao
  // AutoNumbering vivo. NÃO desce nas linhas (counters de tabela aninhada
  // não fazem sentido aqui).
  //
  // EXCEÇÃO: tabelas SEM bordas (`borderStyle: "none"`) são tabelas de
  // LAYOUT (bloco de registro/timbre), não exibições numeradas — não
  // recebem número nem legenda automática.
  //
  // F4.1 — Legenda REMOVIDA (captionVisible=false, estilo Word): também não
  // numera. Em AMBOS os casos o caption é zerado no JSON numerado — cinto
  // extra pra qualquer consumidor deste JSON não vazar texto órfão. (O DOCX
  // NÃO passa por aqui: ele anda o .sicrodoc cru no Rust; a defesa
  // equivalente vive em exporters/docx.rs::render_table.) Espelha
  // SicroTableView.isNumerable.
  if (node.type === "table") {
    if (
      (node.attrs?.borderStyle as string | undefined) === "none" ||
      node.attrs?.captionVisible === false
    ) {
      return {
        ...node,
        attrs: { ...(node.attrs ?? {}), caption: "" },
      };
    }
    counters.table += 1;
    const number = counters.table;
    const existing = String(node.attrs?.caption ?? "");
    const caption = prependCaptionText(existing, `Tabela ${number} — `);
    return {
      ...node,
      attrs: { ...(node.attrs ?? {}), caption },
    };
  }

  if (!node.content) return node;
  return {
    ...node,
    content: node.content.map((child) => walk(child, counters)),
  };
}

/** Regex de prefixo já-presente (evita duplicar "Tabela 1 — Tabela 1 — …"). */
const LABEL_PREFIX_RE = /^(Tabela|Quadro)\s+\d+\s*—/;

/** Versão pra string pura (attr `caption` da tabela). */
function prependCaptionText(existing: string, prefix: string): string {
  if (LABEL_PREFIX_RE.test(existing)) return existing;
  return existing ? `${prefix}${existing}` : prefix.replace(/\s*—\s*$/, "");
}
