/** Um item na pista: corpo arrastável, bordas de aparar, tira (V1), onda (áudio), losangos (efeito). Dono: P4.1 (casca). */

import type { ItemNaLinha } from "./modelo";

export interface ClipeNaLinhaProps {
  item: ItemNaLinha;
  pxPorS: number;
  selecionado: boolean;
}

export function ClipeNaLinha(props: ClipeNaLinhaProps) {
  void props;
  return null;
}
