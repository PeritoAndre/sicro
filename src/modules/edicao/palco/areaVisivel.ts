/**
 * Área do palco que o perito vê agora, em 0..1 da ORIGEM do clipe (vista com zoom e deslocamento e o enquadramento
 * desfeitos): o palco publica (P3.3, ao mudar vista, clipe ou instante); "Adicionar" lê para o destaque, a lupa, o
 * holofote, a velocidade e a ocultação nascerem onde se vê (contratos-fase4.md D9). Dono: pacote "Pessoas na Edição
 * e pequenos" (casca da abertura: sem área publicada, a caixa de sempre).
 */

import { create } from "zustand";
import type { Caixa, Id } from "@domain/edicao";
import type { Afim } from "../modelo/geometria";
import { CAIXA_CENTRO } from "../inspetor/novos";
import type { Tamanho, Vista } from "./vista";

interface AreaVisivel {
  clipe: Id | null;
  /** 0..1 da origem do clipe; null = nada publicado. */
  caixa: Caixa | null;
  publicar(clipe: Id | null, caixa: Caixa | null): void;
}

export const useAreaVisivel = create<AreaVisivel>((set) => ({
  clipe: null,
  caixa: null,
  publicar: (clipe, caixa) => set({ clipe, caixa }),
}));

/** A área que a vista mostra (tela `tela` px, canvas da saída `canvas`), levada à origem pela inversa de `A`. */
export function areaVisivelNaOrigem(vista: Vista, tela: Tamanho, canvas: Tamanho, A: Afim): Caixa {
  void vista;
  void tela;
  void canvas;
  void A;
  return { x: 0, y: 0, w: 1, h: 1 };
}

/** Caixa de um rastreio novo do clipe: o tamanho de `CAIXA_CENTRO` no centro da área visível publicada para ele
 *  (limitada à área); sem área publicada (ou de outro clipe), `CAIXA_CENTRO`. */
export function caixaNova(clipe: Id | null): Caixa {
  void clipe;
  return CAIXA_CENTRO;
}
