/**
 * Área do palco que o perito vê agora, em 0..1 da ORIGEM do clipe (vista com zoom e deslocamento e o enquadramento
 * desfeitos): o palco publica (P3.3, ao mudar vista, clipe ou instante); "Adicionar" lê para o destaque, a lupa, o
 * holofote, a velocidade e a ocultação nascerem onde se vê (contratos-fase4.md D12). Dono: pacote "Pessoas na Edição
 * e pequenos".
 */

import { create } from "zustand";
import type { Caixa, Id } from "@domain/edicao";
import { caixa as envolvente, inversa, type Afim } from "../modelo/geometria";
import { CAIXA_CENTRO } from "../inspetor/novos";
import { contido, mostrado, type Tamanho, type Vista } from "./vista";

// CONTRATO: D12 dizia "limitada a ela"; com zoom acima de ~3× a caixa de sempre cobriria a tela toda (alças na borda).
/** A caixa nova ocupa no máximo esta fração da área visível em cada lado: com zoom grande as alças ficam na tela. */
export const FRACAO_DA_AREA = 0.6;
/** Área menor que isto (fração da origem) não serve de lugar: só faixa preta ou fundo na tela. */
const AREA_MINIMA = 1e-4;

const INTEIRA: Caixa = Object.freeze({ x: 0, y: 0, w: 1, h: 1 });

interface AreaVisivel {
  clipe: Id | null;
  /** 0..1 da origem do clipe; null = nada publicado. */
  caixa: Caixa | null;
  publicar(clipe: Id | null, caixa: Caixa | null): void;
}

const mesma = (a: Caixa | null, b: Caixa | null) => a === b || (!!a && !!b && a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h);

export const useAreaVisivel = create<AreaVisivel>((set, get) => ({
  clipe: null,
  caixa: null,
  // o palco publica a cada pintura: igual ao que já está, nada muda
  publicar: (clipe, caixa) => {
    const s = get();
    if (s.clipe !== clipe || !mesma(s.caixa, caixa)) set({ clipe, caixa });
  },
}));

const limitar01 = (v: number) => Math.min(Math.max(v, 0), 1);

/** A área que a vista mostra (tela `tela` px, canvas da saída `canvas`), levada à origem pela inversa de `A`. Fora da
 *  imagem (faixas do "Inteiro", fundo) não conta; sem nada da imagem na tela, uma caixa vazia no ponto mais perto. */
export function areaVisivelNaOrigem(vista: Vista, tela: Tamanho, canvas: Tamanho, A: Afim): Caixa {
  const m = mostrado(vista, contido(tela, canvas));
  if (!(m.w > 0 && m.h > 0)) return { ...INTEIRA };
  // parte do canvas que cabe na caixa do palco, em 0..1 do canvas
  const x0 = limitar01(-m.x / m.w);
  const x1 = limitar01((tela.w - m.x) / m.w);
  const y0 = limitar01(-m.y / m.h);
  const y1 = limitar01((tela.h - m.y) / m.h);
  const inv = inversa(A);
  if (!inv || !(x1 > x0 && y1 > y0)) return { ...INTEIRA };
  // giro em múltiplos de 90°: a envolvente é exata
  const b = envolvente(inv, { x: x0, y: y0, w: x1 - x0, h: y1 - y0 });
  const ox0 = limitar01(b.x);
  const ox1 = limitar01(b.x + b.w);
  const oy0 = limitar01(b.y);
  const oy1 = limitar01(b.y + b.h);
  if (!(ox1 > ox0 && oy1 > oy0)) return { x: limitar01(b.x + b.w / 2), y: limitar01(b.y + b.h / 2), w: 0, h: 0 };
  return { x: ox0, y: oy0, w: ox1 - ox0, h: oy1 - oy0 };
}

/** Caixa de um rastreio novo do clipe: o tamanho de `CAIXA_CENTRO` (reduzido por igual até `FRACAO_DA_AREA` da área)
 *  no centro da área visível publicada para ele; sem área publicada (ou de outro clipe, ou sem imagem na tela),
 *  `CAIXA_CENTRO`. */
export function caixaNova(clipe: Id | null): Caixa {
  const { clipe: publicado, caixa: a } = useAreaVisivel.getState();
  const valida = !!a && [a.x, a.y, a.w, a.h].every(Number.isFinite) && a.w >= AREA_MINIMA && a.h >= AREA_MINIMA;
  if (!clipe || publicado !== clipe || !a || !valida) return { ...CAIXA_CENTRO };
  const k = Math.min(1, (FRACAO_DA_AREA * a.w) / CAIXA_CENTRO.w, (FRACAO_DA_AREA * a.h) / CAIXA_CENTRO.h);
  const w = CAIXA_CENTRO.w * k;
  const h = CAIXA_CENTRO.h * k;
  const x = Math.min(Math.max(a.x + (a.w - w) / 2, a.x), a.x + a.w - w);
  const y = Math.min(Math.max(a.y + (a.h - h) / 2, a.y), a.y + a.h - h);
  return { x, y, w, h };
}
