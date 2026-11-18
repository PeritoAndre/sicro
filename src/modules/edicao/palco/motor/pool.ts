/**
 * Elementos `<video>` da prévia viva (§7.2): `tamanhoDoPool` elementos criados uma vez, `preload="auto"`, dentro do
 * palco com 2×2 px e opacidade ≈ 0, ligados ao Web Audio antes do `src`; chave = arquivo (trocar o `src` custa
 * 210 a 410 ms). O seguinte é preparado quando o segmento atual COMEÇA (§7.3). Dono: P3.3 (casca da abertura).
 */

import type { Seg } from "@domain/edicao";
import type { MisturaAudio } from "./audio";

export const POOL_MIN = 4;
export const POOL_MAX = 8;

/** `max(4, 2 × maior número de células)`, até 8. */
export function tamanhoDoPool(celulasMax: number): number {
  void celulasMax;
  return POOL_MIN;
}

export interface ElementoPool {
  el: HTMLVideoElement;
  /** Arquivo que ele tem (null = livre). */
  chave: string | null;
}

export interface Pool {
  /** O elemento do arquivo (reaproveita se já tem; senão o menos usado). */
  pedir(chave: string, src: string): ElementoPool;
  /** Pausado no tempo do elemento `t_el` (meio do quadro de entrada). */
  preparar(chave: string, src: string, t_el: Seg): Promise<void>;
  soltar(chave: string): void;
  destruir(): void;
}

export function criarPool(op: { tamanho: number; host: HTMLElement; mistura: MisturaAudio | null }): Pool {
  void op;
  return {
    pedir: () => {
      throw new Error("prévia viva: pool não entregue");
    },
    preparar: async () => {},
    soltar: () => {},
    destruir: () => {},
  };
}
