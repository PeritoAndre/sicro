/**
 * Modo guiado da rampa (§7.3, P3.4): o elemento fica parado e o palco pede por busca o quadro que o mapa do clipe
 * (`mapaTrecho`, o espelho do Rust) dá ao slot do relógio de parede. Sem som (§7.4). Contas puras; o laço é do palco.
 */

import type { ClipePrincipal, Racional } from "@domain/edicao";
import type { MapaQuadros } from "../modelo/tempo";

/** O clipe toca guiado: trecho em rampa (`playbackRate` não muda com o vídeo tocando, §7.3). */
export const tocaGuiado = (c: ClipePrincipal | null | undefined): boolean => !!c && c.tipo === "trecho" && c.velocidade.tipo === "rampa";

/** Slot do clipe em que o quadro `j` da fonte aparece primeiro; descartado = o do quadro que o cobre. `campos` = 2 no
 *  desentrelace por campo (o mapa conta campos: o quadro j são as entradas 2j e 2j + 1). */
export function slotDoQuadro(mapa: MapaQuadros, j: number, campos: 1 | 2): number {
  const alvo = campos === 2 ? 2 * j : j;
  const q = mapa.quadro_do_slot;
  let lo = 0;
  let hi = q.length;
  while (lo < hi) {
    const m = (lo + hi) >>> 1;
    if ((q[m] as number) < alvo) lo = m + 1;
    else hi = m;
  }
  return Math.min(lo, Math.max(q.length - 1, 0));
}

/** Quadro da fonte do slot `q0 + decorrido·fps` (null = passou do fim do clipe). */
export function quadroGuiado(mapa: MapaQuadros, campos: 1 | 2, q0: number, decorridoS: number, fps: Racional): number | null {
  const q = q0 + Math.floor((decorridoS * fps.num) / fps.den);
  if (q >= mapa.n_q) return null;
  const j = mapa.quadro_do_slot[Math.max(q, 0)] as number;
  return campos === 2 ? j >> 1 : j;
}
