/**
 * Espelho de `src-tauri/src/edicao/rastreio/avaliar.rs` (§6.1): caixa de um rastreio num quadro.
 * Ordem: âncora no quadro → (denso, Fase 2) → interpolação entre âncoras → depois da última segura a
 * última (só origem manual) → antes da primeira, nada. Fixtures `rastreio-*.json`.
 */

import type { Caixa, Chave, Pts, Rastreio, Seg } from "@domain/edicao";
import { contarAte, entre } from "./curva";

/** Tempo (s) na base do rastreio e, quando há mídia, o pts do quadro. */
export interface Instante {
  t: Seg;
  pts: Pts | null;
}
export type Como = "ancora" | "denso" | "interpolado" | "segurado";
/** `incerto` = amostra densa de estado incerto. */
export interface Avaliado {
  caixa: Caixa;
  como: Como;
  incerto: boolean;
}

/** Âncora "neste quadro": mesmo pts quando os dois têm; senão o mesmo `t`. Procura em volta de `n`; vale a última. */
function ancoraNoQuadro(ks: readonly Chave<Caixa>[], n: number, em: Instante): number | null {
  let achada: number | null = null;
  for (let i = Math.max(0, n - 2); i < Math.min(n + 2, ks.length); i++) {
    const k = ks[i] as Chave<Caixa>;
    const mesmo = em.pts != null && k.pts != null ? em.pts === k.pts : k.t === em.t;
    if (mesmo) achada = i;
  }
  return achada;
}

/** Caixa do rastreio no instante; null = nada. Na Fase 0 não há denso (segue pelas âncoras). */
export function avaliar(r: Rastreio, em: Instante): Avaliado | null {
  const ks = r.ancoras;
  const n = contarAte(ks, em.t);
  const i = ancoraNoQuadro(ks, n, em);
  if (i != null) return { caixa: (ks[i] as Chave<Caixa>).v, como: "ancora", incerto: false };
  if (n === 0) return null;
  if (n === ks.length) {
    return r.origem.tipo === "manual" ? { caixa: (ks[n - 1] as Chave<Caixa>).v, como: "segurado", incerto: false } : null;
  }
  return { caixa: entre(ks[n - 1] as Chave<Caixa>, ks[n] as Chave<Caixa>, em.t), como: "interpolado", incerto: false };
}

/** O instante cai numa ausência aceita? Intervalo fechado. */
export function emAusencia(r: Rastreio, t: Seg): boolean {
  return r.ausencias.some((a) => a.de_s <= t && t <= a.ate_s);
}
