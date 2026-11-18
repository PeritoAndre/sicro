/** Onda do som de um item (picos de `edicao_onda`, P1.9) num canvas do tamanho do item. Dono: P4.1 (casca). */

import type { Onda as Picos, Seg } from "@domain/edicao";

export interface OndaProps {
  picos: Picos | null;
  /** Faixa de fonte mostrada no item. */
  de_s: Seg;
  ate_s: Seg;
  largura: number;
  altura: number;
}

export function Onda(props: OndaProps) {
  void props;
  return null;
}
