/**
 * Quadros-chave de qualquer `Animavel` (Fase 4, P4.2; contratos-fase4.md D7): pôr no cursor, mover, trocar a
 * interpolação. Tirar a última chave vira valor fixo (`modelo/projeto.ts::tirarChave`, contratos (f) 7 da Fase 0:
 * lista vazia nunca é gravada). Puro. Dono: P4.2 (casca da abertura).
 */

import type { Animavel, Chave, Interp, Seg } from "@domain/edicao";
import type { Interpolavel } from "../../modelo/curva";

/** Instantes das chaves (vazio = valor fixo). */
export function chavesDe<T>(a: Animavel<T>): Seg[] {
  void a;
  return [];
}

/** Chave em `t` com o valor `v`: valor fixo vira [fixo em t0?, v em t] pela regra de D7; chave já em `t` troca o valor. */
export function chaveNoCursor<T extends Interpolavel>(a: Animavel<T>, t: Seg, v: T, interp?: Interp): Animavel<T> {
  void t;
  void v;
  void interp;
  return a;
}

/** Move a chave `i` para `t`, sem passar das vizinhas (a ordem não muda). */
export function moverChave<T>(ks: readonly Chave<T>[], i: number, t: Seg): Chave<T>[] {
  void i;
  void t;
  return [...ks];
}

/** Interpolação da chave `i` até a seguinte. */
export function trocarInterp<T>(ks: readonly Chave<T>[], i: number, interp: Interp): Chave<T>[] {
  void i;
  void interp;
  return [...ks];
}
