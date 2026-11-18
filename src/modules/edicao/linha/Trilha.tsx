/** Uma trilha da linha do tempo: cabeça (nome, ver, travar) e a pista com os itens. Dono: P4.1 (casca). */

import type { TrilhaNaLinha } from "./modelo";

export interface TrilhaProps {
  trilha: TrilhaNaLinha;
  pxPorS: number;
  altura: number;
}

export function Trilha(props: TrilhaProps) {
  void props;
  return null;
}
