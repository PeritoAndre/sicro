/**
 * Frente X — medição de LINHAS visuais de um bloco (parágrafo).
 *
 * A paginação "split" (quebra de parágrafo no meio da página) precisa saber:
 *  (1) quantas linhas o parágrafo tem e a altura de cada uma (pra decidir onde
 *      a página estoura) — `measureLineBoxes`;
 *  (2) a posição ProseMirror do início da linha onde a quebra cai (pra inserir
 *      o spacer ali) — `posAtLineStart`, resolvida SOB DEMANDA (só na quebra).
 *
 * X5.3 — Pressupõe layout natural (o caller zera os spacers de paginação 1× por
 * recompute antes de medir).
 *
 * X5.5 — POSIÇÃO PM OFF-SCREEN-SAFE. Antes usávamos `view.posAtCoords` (baseado
 * em `caretRangeFromPoint`), que só resolve pontos DENTRO da viewport — quando
 * a linha do corte caía fora da tela (comum após delete/mudança de margem, que
 * reflui o conteúdo), retornava null e o spacer ia pro lugar errado (empurrava
 * o parágrafo inteiro / buraco). Agora usamos `view.coordsAtPos` (o inverso,
 * que calcula geometria independente de scroll) numa busca binária pelo início
 * da linha — funciona com a linha fora da tela.
 */

import type { EditorView } from "@tiptap/pm/view";

export interface LineBox {
  /** Topo da linha relativo ao topo do bloco (px, ≥0) — layout natural. */
  topRel: number;
  /** Altura da linha (px) = distância até a próxima (ou fim do bloco). */
  height: number;
  /** Topo da linha em coordenada de viewport (px) — usado por `posAtLineStart`
   *  pra resolver a posição PM da linha sob demanda. */
  topAbs: number;
}

/** Tolerância (px) para agrupar rects na MESMA linha (vêm fragmentadas por
 *  text nodes / marcas diferentes) e para comparar tops. */
const LINE_TOL_PX = 2;

export function measureLineBoxes(blockEl: HTMLElement): LineBox[] {
  const blockH = Math.floor(blockEl.offsetHeight);

  let blockRect: DOMRect;
  const tops: number[] = [];
  try {
    blockRect = blockEl.getBoundingClientRect();
    const range = document.createRange();
    range.selectNodeContents(blockEl);
    const list = range.getClientRects();
    for (let i = 0; i < list.length; i++) {
      const r = list[i];
      if (!r || (r.width === 0 && r.height === 0)) continue;
      if (!tops.some((t) => Math.abs(t - r.top) < LINE_TOL_PX)) tops.push(r.top);
    }
  } catch {
    return [{ topRel: 0, height: blockH, topAbs: 0 }];
  }
  tops.sort((a, b) => a - b);
  if (tops.length <= 1) {
    return [{ topRel: 0, height: blockH, topAbs: tops[0] ?? 0 }];
  }

  const out: LineBox[] = [];
  for (let i = 0; i < tops.length; i++) {
    const topAbs = tops[i]!;
    const topRel = Math.max(0, Math.round(topAbs - blockRect.top));
    const nextTopRel =
      i + 1 < tops.length ? Math.round(tops[i + 1]! - blockRect.top) : blockH;
    // A SOMA das alturas TEM que dar `blockH` — senão a paginação sub-conta a
    // cada parágrafo (o "leading" acima da 1ª linha, maior no entrelinhas 1,5),
    // o spacer da quebra sai alto demais e o texto vaza a margem, acumulando a
    // cada página. Por isso a LINHA 0 começa do topo do bloco (0), absorvendo
    // esse leading; as demais usam o próprio topo. Telescopa → Σ = blockH.
    const startRel = i === 0 ? 0 : topRel;
    const height = Math.max(0, nextTopRel - startRel);
    out.push({ topRel, height, topAbs });
  }
  return out;
}

/**
 * Posição PM do INÍCIO da linha cujo topo (viewport-coord) é `targetTopAbs`,
 * dentro do range [fromPos, toPos] do conteúdo do bloco. Busca binária com
 * `view.coordsAtPos` (off-screen-safe): acha a menor posição cujo topo já está
 * na linha-alvo (ou abaixo). Best-effort: devolve `fromPos` se algo falhar.
 */
export function posAtLineStart(
  view: EditorView,
  fromPos: number,
  toPos: number,
  targetTopAbs: number,
): number {
  let lo = fromPos;
  let hi = Math.max(fromPos, toPos);
  try {
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      const top = view.coordsAtPos(mid).top;
      if (top < targetTopAbs - LINE_TOL_PX) {
        lo = mid + 1; // `mid` ainda está numa linha ACIMA da alvo.
      } else {
        hi = mid; // `mid` está na linha-alvo (ou abaixo) — recua.
      }
    }
  } catch {
    return fromPos;
  }
  return lo;
}
