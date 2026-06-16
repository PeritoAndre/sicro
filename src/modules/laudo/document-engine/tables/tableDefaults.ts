/**
 * tableDefaults — geração de larguras de coluna (`colwidth`) padrão.
 *
 * F1.2 — Com `table-layout: fixed` (exigido pelo prosemirror-tables pra o
 * resize de coluna funcionar), uma tabela cujas células não têm `colwidth`
 * explícito colapsa pra largura mínima (`--default-cell-min-width`, ~25px)
 * em vez de distribuir o espaço. Por isso TODA tabela nova — e o "Bloco de
 * registro" 1×3 do cabeçalho — precisa nascer com larguras semeadas.
 *
 * A unidade de `colwidth` é o PIXEL de layout (mesma unidade que o
 * prosemirror-tables usa internamente e serializa em `data-colwidth`/
 * `<colgroup><col style="width:Npx">`). Trabalhamos em px aqui e deixamos
 * a tabela `width: 100%` no CSS escalar visualmente conforme o zoom/página.
 *
 * Lógica PURA (sem dependência de DOM/TipTap) → coberta por testes vitest.
 */

/**
 * Largura útil de conteúdo de uma página A4 retrato com as margens
 * institucionais padrão (esq 3,5cm + dir 2cm ⇒ ~15,5cm) convertida para
 * px de layout a 96dpi (1cm = 37,795px). ~586px. Arredondado pra 600 pra
 * dar uma folga visual; como a tabela é `width: 100%`, o total exato não
 * precisa bater com a página — só a PROPORÇÃO entre colunas importa.
 */
export const DEFAULT_TABLE_CONTENT_WIDTH_PX = 600;

/** Largura mínima de uma coluna semeada (px de layout). Abaixo disto a
 *  célula fica apertada demais pra digitar. */
export const MIN_SEEDED_COL_WIDTH_PX = 48;

/**
 * Distribui `totalWidthPx` igualmente entre `cols` colunas, devolvendo um
 * array de inteiros (px) cujo somatório == `totalWidthPx` (o resto da
 * divisão entra na última coluna pra não perder pixels).
 */
export function seedEqualColWidths(
  cols: number,
  totalWidthPx: number = DEFAULT_TABLE_CONTENT_WIDTH_PX,
): number[] {
  const n = Math.max(1, Math.floor(cols));
  const total = Math.max(n * MIN_SEEDED_COL_WIDTH_PX, Math.floor(totalWidthPx));
  const base = Math.max(MIN_SEEDED_COL_WIDTH_PX, Math.floor(total / n));
  const widths = new Array<number>(n).fill(base);
  // Ajuste do resto na última coluna pra somar exatamente `total`.
  const used = base * n;
  widths[n - 1] = base + (total - used);
  return widths;
}

/**
 * Distribui larguras conforme PESOS relativos (ex.: [3, 2, 1] = coluna larga
 * / média / estreita, estilo Word). Normaliza os pesos e aplica sobre
 * `totalWidthPx`, garantindo o mínimo por coluna e somatório exato.
 */
export function seedWeightedColWidths(
  weights: ReadonlyArray<number>,
  totalWidthPx: number = DEFAULT_TABLE_CONTENT_WIDTH_PX,
): number[] {
  const safe = weights.map((w) => (Number.isFinite(w) && w > 0 ? w : 1));
  const n = safe.length;
  if (n === 0) return [];
  const total = Math.max(n * MIN_SEEDED_COL_WIDTH_PX, Math.floor(totalWidthPx));
  const sum = safe.reduce((a, b) => a + b, 0);
  const widths = safe.map((w) =>
    Math.max(MIN_SEEDED_COL_WIDTH_PX, Math.round((w / sum) * total)),
  );
  // Corrige o erro de arredondamento na última coluna.
  const used = widths.reduce((a, b) => a + b, 0);
  const last = (widths[n - 1] ?? MIN_SEEDED_COL_WIDTH_PX) + (total - used);
  widths[n - 1] = Math.max(MIN_SEEDED_COL_WIDTH_PX, last);
  return widths;
}

/**
 * Pesos do "Bloco de registro" do cabeçalho (tabela 1×3 do timbre Word):
 *   col 1 (dados Registrado/REQ/BO) → larga
 *   col 2 (número do laudo centralizado) → média
 *   col 3 (folha) → estreita
 */
export const REGISTRATION_BLOCK_WEIGHTS: ReadonlyArray<number> = [3, 2, 1.4];

/** Larguras (px) prontas do bloco de registro. */
export function registrationBlockColWidths(
  totalWidthPx: number = DEFAULT_TABLE_CONTENT_WIDTH_PX,
): number[] {
  return seedWeightedColWidths(REGISTRATION_BLOCK_WEIGHTS, totalWidthPx);
}

/** Forma mínima de um nó JSON TipTap (evita acoplar a JSONContent aqui). */
interface JsonNode {
  type: string;
  attrs?: Record<string, unknown>;
  content?: JsonNode[];
}

/**
 * Constrói o JSON de uma tabela N×M com `colwidth` semeado igualmente em
 * cada coluna e (opcional) primeira linha como cabeçalho. Não gera o `id`
 * (deixe o chamador injetar via generateTableId pra manter este módulo PURO,
 * sem dependência do schema/UUID). Usado pelo diálogo "Criar tabela (N×M)".
 */
export function buildSeededTableJson(params: {
  rows: number;
  cols: number;
  withHeaderRow: boolean;
  totalWidthPx?: number;
}): JsonNode {
  const rows = Math.max(1, Math.floor(params.rows));
  const cols = Math.max(1, Math.floor(params.cols));
  const widths = seedEqualColWidths(cols, params.totalWidthPx);
  const rowNodes: JsonNode[] = [];
  for (let r = 0; r < rows; r++) {
    const isHeader = params.withHeaderRow && r === 0;
    const cells: JsonNode[] = [];
    for (let c = 0; c < cols; c++) {
      cells.push({
        type: isHeader ? "tableHeader" : "tableCell",
        attrs: { colspan: 1, rowspan: 1, colwidth: [widths[c]] },
        content: [{ type: "paragraph" }],
      });
    }
    rowNodes.push({ type: "tableRow", content: cells });
  }
  return { type: "table", content: rowNodes };
}
