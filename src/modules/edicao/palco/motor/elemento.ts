/**
 * A porta do palco para o `<video>` (§7.1): único arquivo, fora do `relogioMidia`, que toca no tempo do elemento,
 * e sempre pela conversão dele (alvo no MEIO do quadro, quadro exibido pela tabela). `duration`, `seekable` e
 * `videoWidth` não são lidos em lugar nenhum: faixa e tamanho vêm do Rust. Nunca espera `requestVideoFrameCallback`.
 */

import type { Seg } from "@domain/edicao";
import type { RelogioMidia } from "./relogioMidia";

export interface ElementoBusca {
  /** Pede o quadro j: busca o meio dele. */
  buscar(j: number): void;
  /** Quadro exibido agora (tabela). */
  quadro(): number;
  /** Tempo de fonte agora. */
  agora(): Seg;
  buscando(): boolean;
  aoBuscado(f: () => void): () => void;
  tocar(): Promise<void>;
  pausar(): void;
  pausado(): boolean;
}

export function elementoDe(v: HTMLVideoElement, r: RelogioMidia): ElementoBusca {
  return {
    buscar: (j) => {
      v.currentTime = r.alvo(j);
    },
    quadro: () => r.quadro(v.currentTime),
    agora: () => r.paraFonte(v.currentTime),
    buscando: () => v.seeking,
    aoBuscado: (f) => {
      v.addEventListener("seeked", f);
      return () => v.removeEventListener("seeked", f);
    },
    tocar: () => v.play(),
    pausar: () => v.pause(),
    pausado: () => v.paused,
  };
}
