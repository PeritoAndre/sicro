/**
 * Palco durante a corrida (§6.3, §8.2): mostra a miniatura do evento com a caixa e o estado (ícone de incerto,
 * fora de cena, perdido), sem buscar no `<video>`; "Parar". Marcar → seguir: a âncora marcada no palco vira o
 * pedido da corrida. Dono: P2.4. Abertura: casca; montada no `Palco` só com `RASTREIO_PRONTO`.
 */

import { RASTREIO_PRONTO } from "../store/rastreioStore";

export interface FerramentaSeguirProps {
  /** Retângulo do quadro no palco (px da caixa do palco). */
  area: { x: number; y: number; w: number; h: number };
}

export function FerramentaSeguir(p: FerramentaSeguirProps) {
  if (!RASTREIO_PRONTO) return null;
  void p;
  return null;
}
