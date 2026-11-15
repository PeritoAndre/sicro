/**
 * Espelho de `src-tauri/src/edicao/rastreio/avaliar.rs` (§6.1): caixa de um rastreio num quadro.
 * Ordem: âncora no quadro → amostra densa (ausente = nada) → interpolação entre âncoras → depois da última
 * segura a última (só origem manual) → antes da primeira, nada. Fixtures `rastreio-*.json`.
 */

import { ESTADO_INCERTO, ESTADO_OK, TIPO_MEDIDO, type ArquivoDenso, type Caixa, type Chave, type Pts, type Rastreio, type Seg } from "@domain/edicao";
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

// CONTRATO: abertura da Fase 2 (contratos-fase2.md (b)): `denso` é o terceiro argumento, opcional.
/** Linha do denso no pts do instante (espelho de `do_denso`): amostra ok ou incerta, `null` = estado ausente ou
 *  desconhecido (nada), `undefined` = sem linha nesse quadro ou instante sem pts (segue pelas âncoras). `como` =
 *  "denso" na linha medida, "interpolado" na de `tipo` interpolado ou estendido; `incerto` = estado incerto. */
function doDenso(d: ArquivoDenso, em: Instante): Avaliado | null | undefined {
  if (em.pts == null) return undefined;
  let a = 0;
  let b = d.pts.length;
  while (a < b) {
    const m = (a + b) >>> 1;
    if ((d.pts[m] as Pts) < em.pts) a = m + 1;
    else b = m;
  }
  if (d.pts[a] !== em.pts) return undefined;
  const estado = d.estado[a];
  if (estado === undefined) return undefined;
  if (estado !== ESTADO_OK && estado !== ESTADO_INCERTO) return null;
  const [x, y, w, h] = [d.x[a], d.y[a], d.w[a], d.h[a]];
  if (x === undefined || y === undefined || w === undefined || h === undefined) return undefined;
  const como = (d.tipo[a] ?? TIPO_MEDIDO) === TIPO_MEDIDO ? "denso" : "interpolado";
  return { caixa: { x, y, w, h }, como, incerto: estado === ESTADO_INCERTO };
}

/** Caixa do rastreio no instante; null = nada. Sem `denso` segue pelas âncoras. */
export function avaliar(r: Rastreio, em: Instante, denso?: ArquivoDenso | null): Avaliado | null {
  const ks = r.ancoras;
  const n = contarAte(ks, em.t);
  const i = ancoraNoQuadro(ks, n, em);
  if (i != null) return { caixa: (ks[i] as Chave<Caixa>).v, como: "ancora", incerto: false };
  if (denso) {
    const v = doDenso(denso, em);
    if (v !== undefined) return v;
  }
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
