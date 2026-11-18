/**
 * Relógio da prévia viva (§7.2, §7.3): o tempo de reprodução vive aqui, FORA do React (`assinar`); segue o
 * `currentTime` do elemento mestre do segmento (nunca o contrário); sem vídeo, relógio de parede. Estados
 * `parado | tocando | guiado | esperando`. O store recebe o cursor ao pausar, ao buscar e a cada `PUBLICAR_MS`.
 * Dono: P3.3 (casca da abertura, contratos-fase4.md B8).
 */

import type { Racional, Seg } from "@domain/edicao";

export type EstadoRelogio = "parado" | "tocando" | "guiado" | "esperando";

/** A cada quanto o store recebe o cursor tocando (ms). */
export const PUBLICAR_MS = 250;

/** O elemento que manda no tempo do segmento: tempo dele já em tempo de fonte, e se está pronto para tocar. */
export interface Mestre {
  tempo(): Seg;
  pronto(): boolean;
}

export interface Relogio {
  estado(): EstadoRelogio;
  q(): number;
  /** Toca a partir do slot atual; `velocidade` fora de 0,25..8 vai ao modo guiado (§7.3). */
  tocar(velocidade?: number): void;
  pausar(): void;
  irPara(q: number): void;
  /** Mestre do segmento que começa (null = relógio de parede): `T = de_q + (tempo − t0)/velocidade · fps`. */
  mestre(m: Mestre | null, seg: { de_q: number; t0: Seg; velocidade: number } | null): void;
  /** Avisa a cada mudança de slot e de estado; devolve o cancelamento. */
  assinar(fn: (q: number, e: EstadoRelogio) => void): () => void;
}

export function criarRelogio(op: { fps: Racional; total_q: number; agora?: () => number }): Relogio {
  void op;
  let q = 0;
  return {
    estado: () => "parado",
    q: () => q,
    tocar: () => {},
    pausar: () => {},
    irPara: (n) => {
      q = n;
    },
    mestre: () => {},
    assinar: () => () => {},
  };
}
