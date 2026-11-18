/**
 * Quadros-chave de qualquer `Animavel` (Fase 4, P4.2; contratos-fase4.md D7): pôr no cursor, tirar, mover, trocar a
 * interpolação. Mesma linguagem das chaves do zoom (ZS): a chave nova nasce "suave"; a que já existe no instante troca
 * o valor e deixa a curva dela. Tirar a última volta a valor fixo (lista vazia nunca é gravada, contratos (f) 7 da
 * Fase 0). Puro. Dono: P4.2.
 */

import type { Animavel, Chave, Interp, Seg } from "@domain/edicao";
import { avaliar, ehChaves, type Interpolavel } from "../../modelo/curva";

/** Interpolação da chave nova no sobreposto (a do zoom: entra e sai devagar). O volume nasce linear, sem o campo
 *  (`null`): é o que a chave do som gravava antes da Fase 4, e o projeto feito pela mesma tela sai igual. */
export const INTERP_NOVA: Interp = "suave";
/** Dois instantes da mesma chave (s): o mesmo slot por caminhos de conta diferentes. */
export const MESMO_T = 1e-6;

const mesmo = (a: Seg, b: Seg) => Math.abs(a - b) <= MESMO_T;

/** Instantes das chaves (vazio = valor fixo). */
export function chavesDe<T>(a: Animavel<T>): Seg[] {
  return ehChaves(a) ? a.map((k) => k.t) : [];
}

/** Volume acima de 100 % em algum instante (a interpolação nunca passa da maior chave): a prévia toca até 1 pelo
 *  elemento, o resto só se ouve no render. */
export const passaDeUm = (a: Animavel<number>): boolean => (ehChaves(a) ? a.some((k) => k.v > 1) : a > 1);

/** Índice da chave em `t`; -1 = nenhuma ali (ou valor fixo). */
export function chaveEm<T>(a: Animavel<T>, t: Seg): number {
  return ehChaves(a) ? a.findIndex((k) => mesmo(k.t, t)) : -1;
}

/** Instante da chave do cursor: o de uma chave que já está no quadro de `t` (`quadro`: o quadro na tela de um instante),
 *  senão `t`. O modo Clipe põe o cursor no meio do quadro e o Programa no começo; a chave antiga fica onde nasceu. */
export function instanteNoQuadro<T>(a: Animavel<T>, t: Seg, quadro?: (t: Seg) => number): Seg {
  if (!ehChaves(a) || !quadro || chaveEm(a, t) >= 0) return t;
  const q = quadro(t);
  let achado: Seg | null = null;
  for (const k of a) if (quadro(k.t) === q && (achado === null || Math.abs(k.t - t) < Math.abs(achado - t))) achado = k.t;
  return achado ?? t;
}

/** Chave nova; `interp` null = sem o campo (linear). */
function nova<T>(t: Seg, v: T, interp: Interp | null | undefined): Chave<T> {
  const i = interp === undefined ? INTERP_NOVA : interp;
  return i === null ? { t, v } : { t, v, interp: i };
}

/** "Chave no cursor": valor fixo vira `[{ t, v }]`; com chaves, a de `t` troca o valor (curva e pts ficam) ou entra
 *  uma nova, em ordem, com `interp` (padrão `INTERP_NOVA`; null = linear sem o campo). */
export function chaveNoCursor<T extends Interpolavel>(a: Animavel<T>, t: Seg, v: T, interp?: Interp | null): Animavel<T> {
  if (!ehChaves(a)) return [nova(t, v, interp)];
  const i = chaveEm(a, t);
  if (i >= 0) return a.map((k, j) => (j === i ? { ...k, v } : k));
  const n = nova(t, v, interp);
  const depois = a.findIndex((k) => k.t > t);
  return depois < 0 ? [...a, n] : [...a.slice(0, depois), n, ...a.slice(depois)];
}

/** Valor gravado num instante: com chaves, a chave do instante (`chaveNoCursor`); fixo, o valor novo. É o que o
 *  campo do Inspetor e a alça do palco gravam (D7). */
export function gravarEm<T extends Interpolavel>(a: Animavel<T>, t: Seg, v: T, interp?: Interp | null): Animavel<T> {
  return ehChaves(a) ? chaveNoCursor(a, t, v, interp) : v;
}

/** "Tirar chave": a de `t` sai; a última volta a valor fixo (o dela). Sem chave em `t`, nada muda. */
export function tirarChaveEm<T>(a: Animavel<T>, t: Seg): Animavel<T> {
  const i = chaveEm(a, t);
  if (i < 0 || !ehChaves(a)) return a;
  if (a.length === 1) return (a[0] as Chave<T>).v;
  return a.filter((_, j) => j !== i);
}

/** Move a chave `i` para `t`, sem chegar às vizinhas (a ordem não muda; `passo` = folga mínima até elas, 0 = só não
 *  cruzar). Fora do vão entre as vizinhas, para na beira; vão menor que a folga, fica onde está. O `pts` sai (o quadro
 *  mudou). */
export function moverChave<T>(ks: readonly Chave<T>[], i: number, t: Seg, passo = 0): Chave<T>[] {
  const k = ks[i];
  if (!k || !Number.isFinite(t)) return [...ks];
  const antes = ks[i - 1];
  const depois = ks[i + 1];
  const min = antes ? antes.t + Math.max(passo, MESMO_T) : -Infinity;
  const max = depois ? depois.t - Math.max(passo, MESMO_T) : Infinity;
  if (min > max) return [...ks];
  const novo = Math.min(Math.max(t, min), max);
  if (novo === k.t) return [...ks];
  const { pts: _pts, ...resto } = k;
  void _pts;
  return ks.map((x, j) => (j === i ? { ...resto, t: novo } : x));
}

/** Interpolação da chave `i` até a seguinte. */
export function trocarInterp<T>(ks: readonly Chave<T>[], i: number, interp: Interp): Chave<T>[] {
  if (!ks[i]) return [...ks];
  return ks.map((k, j) => (j === i ? { ...k, interp } : k));
}

/** Interpolação da chave `i` (ausente = linear, como no Rust). */
export const interpDe = <T>(k: Chave<T>): Interp => k.interp ?? "linear";

/** Leva a `outro` o que mudou de `antes` para `depois` (as chaves do mesmo instante andam juntas, a interpolação vai
 *  junto, a que saiu sai): a posição tem `x` e `y`, e o editor da curva mexe nas chaves de uma só. */
export function acompanhar<T>(antes: Animavel<T>, depois: Animavel<T>, outro: Animavel<T>): Animavel<T> {
  if (!ehChaves(antes) || !ehChaves(outro)) return outro;
  const d = ehChaves(depois) ? depois : [];
  let r: Animavel<T> = outro;
  if (d.length === antes.length) {
    antes.forEach((a, i) => {
      const b = d[i] as Chave<T>;
      if (!ehChaves(r)) return;
      const j = chaveEm(r, a.t);
      if (j < 0) return;
      if (!mesmo(a.t, b.t)) r = moverChave(r, j, b.t);
      if (interpDe(a) !== interpDe(b) && ehChaves(r)) r = trocarInterp(r, chaveEm(r, b.t), interpDe(b));
    });
    return r;
  }
  for (const a of antes) if (!d.some((b) => mesmo(a.t, b.t))) r = tirarChaveEm(r, a.t);
  return r;
}

/** Valor no instante (fixo ou pelas chaves). */
export const valorEm = <T extends Interpolavel>(a: Animavel<T>, t: Seg): T => avaliar(a, t);
