/**
 * A única porta entre o tempo do elemento `<video>` e o tempo de fonte (§7.1, D23).
 * Ninguém mais lê `currentTime`, `duration`, `seekable` nem `videoWidth`: faixa e tamanho vêm do Rust
 * (`PreviaMidia`, tabela compacta). Busca sempre no MEIO do quadro.
 */

import type { PreviaMidia, Seg, TabelaQuadros } from "@domain/edicao";
import { exibido, nQuadros, passoDe, ptsDaTabela } from "../../modelo/tempo";

export interface RelogioMidia {
  /** Quadros da tabela. */
  readonly n: number;
  /** Faixa da mídia em tempo de fonte. */
  readonly de_s: Seg;
  readonly ate_s: Seg;
  paraElemento(t: Seg): number;
  paraFonte(tElemento: number): Seg;
  /** Meio do quadro n em tempo de fonte. */
  meio(n: number): Seg;
  /** Meio do quadro n em tempo do elemento: o alvo de toda busca. */
  alvo(n: number): number;
  /** Quadro exibido no tempo do elemento. */
  quadro(tElemento: number): number;
}

export function relogioMidia(previa: PreviaMidia, tabela: TabelaQuadros): RelogioMidia {
  const pts = ptsDaTabela(tabela);
  const n = nQuadros(tabela);
  const desloc = previa.desloc_s;
  const num = BigInt(tabela.tb.num);
  const den2 = 2 * tabela.tb.den;
  const limitar = (j: number) => Math.min(Math.max(Math.trunc(j), 0), n - 1);
  // (2·pts + passo)·num / (2·den): inteiros até a única divisão
  const meio = (j: number) => {
    const k = limitar(j);
    return Number((2n * BigInt(pts[k] as number) + BigInt(passoDe(tabela, k))) * num) / den2;
  };
  return {
    n,
    de_s: previa.de_s,
    ate_s: previa.ate_s,
    paraElemento: (t) => t - desloc,
    paraFonte: (tEl) => tEl + desloc,
    meio,
    alvo: (j) => meio(j) - desloc,
    quadro: (tEl) => exibido(tabela, tEl + desloc),
  };
}
