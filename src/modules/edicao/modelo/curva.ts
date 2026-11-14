/**
 * Espelho de `src-tauri/src/edicao/curva.rs` (D14): avaliar um `Animavel`.
 * Mesmas contas, na mesma ordem, para dar os mesmos bits (fixtures `curva-*.json`):
 * lerp `a + (b − a)·f`, `f = (t − t0)/(t1 − t0)`, suave `f·f·(3 − 2·f)`.
 */

import type { Animavel, Caixa, Chave, Ponto } from "@domain/edicao";

/** Número, ponto ou caixa: o que uma curva carrega. */
export type Interpolavel = number | Ponto | Caixa;

function lerp(a: number, b: number, f: number): number {
  return a + (b - a) * f;
}

/** Mistura linear por componente (f = 0 dá `a`, f = 1 dá `b`). */
export function misturar<T extends Interpolavel>(a: T, b: T, f: number): T {
  const v: Interpolavel = a;
  if (typeof v === "number") return lerp(v, b as number, f) as T;
  if ("w" in v) {
    const bb = b as Caixa;
    return { x: lerp(v.x, bb.x, f), y: lerp(v.y, bb.y, f), w: lerp(v.w, bb.w, f), h: lerp(v.h, bb.h, f) } as T;
  }
  const bb = b as Ponto;
  return { x: lerp(v.x, bb.x, f), y: lerp(v.y, bb.y, f) } as T;
}

/** Fração da curva "suave" para a fração linear `f`; fora de 0..1 segura a ponta. */
export function suave(f: number): number {
  if (f <= 0) return 0;
  if (f >= 1) return 1;
  return f * f * (3 - 2 * f);
}

/** Quantas chaves têm `t ≤ instante` (busca binária, como o `partition_point` do Rust). */
export function contarAte<T>(ks: readonly Chave<T>[], t: number): number {
  let lo = 0;
  let hi = ks.length;
  while (lo < hi) {
    const meio = (lo + hi) >>> 1;
    if ((ks[meio] as Chave<T>).t <= t) lo = meio + 1;
    else hi = meio;
  }
  return lo;
}

/** Valor entre duas chaves seguidas, pela `interp` da primeira (ausente = linear); fora de [t0, t1) segura a ponta. */
export function entre<T extends Interpolavel>(k0: Chave<T>, k1: Chave<T>, t: number): T {
  if (t >= k1.t) return k1.v;
  if (t <= k0.t) return k0.v;
  const f = (t - k0.t) / (k1.t - k0.t);
  switch (k0.interp ?? "linear") {
    case "suave":
      return misturar(k0.v, k1.v, suave(f));
    case "degrau":
      return k0.v;
    default:
      return misturar(k0.v, k1.v, f);
  }
}

/** Lista de chaves (e não valor fixo). `T` nunca é lista. */
export function ehChaves<T>(a: Animavel<T>): a is Chave<T>[] {
  return Array.isArray(a);
}

/** Valor em `t`; fora das chaves vale a mais próxima; chaves no mesmo instante: vale a última.
 *  Lista vazia é erro de estrutura (o Rust entra em pânico; aqui, exceção). */
export function avaliar<T extends Interpolavel>(a: Animavel<T>, t: number): T {
  if (!ehChaves(a)) return a;
  if (a.length === 0) throw new Error("Animavel sem chaves");
  const n = contarAte(a, t);
  if (n === 0) return (a[0] as Chave<T>).v;
  if (n === a.length) return (a[n - 1] as Chave<T>).v;
  return entre(a[n - 1] as Chave<T>, a[n] as Chave<T>, t);
}

/** Um valor por instante de `tempos`. */
export function amostrar<T extends Interpolavel>(a: Animavel<T>, tempos: readonly number[]): T[] {
  return tempos.map((t) => avaliar(a, t));
}
