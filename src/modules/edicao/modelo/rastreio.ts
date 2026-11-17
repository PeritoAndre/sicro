/**
 * Espelho de `src-tauri/src/edicao/rastreio/avaliar.rs` (§6.1): caixa de um rastreio num quadro.
 * Ordem: âncora no quadro → amostra densa (ausente = nada) → interpolação entre âncoras → depois da última
 * segura a última (só origem manual) → antes da primeira, nada. Fixtures `rastreio-*.json`.
 */

import type { Draft } from "immer";
import { ESTADO_INCERTO, ESTADO_OK, TIPO_MEDIDO, type ArquivoDenso, type Caixa, type Chave, type ProjetoEdicao, type Pts, type Rastreio, type Seg } from "@domain/edicao";
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

/** Espelho de `rastreio::avaliar::BURACO_S`: rastreio que não é manual não liga âncoras mais distantes que isto (s). */
export const BURACO_S = 4;

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
  // rastreio seguido: âncoras longe uma da outra, sem amostra no meio, não se ligam (nada foi seguido ali)
  if (r.origem.tipo !== "manual" && (ks[n] as Chave<Caixa>).t - (ks[n - 1] as Chave<Caixa>).t > BURACO_S) return null;
  return { caixa: entre(ks[n - 1] as Chave<Caixa>, ks[n] as Chave<Caixa>, em.t), como: "interpolado", incerto: false };
}

/** O instante cai numa ausência aceita? Intervalo fechado. */
export function emAusencia(r: Rastreio, t: Seg): boolean {
  return r.ausencias.some((a) => a.de_s <= t && t <= a.ate_s);
}

// ---- âncoras do programa ("Simplificar") ----

const mesmaCaixa = (a: Caixa, b: Caixa) => a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;
const mesmaAncora = (a: Chave<Caixa>, b: Chave<Caixa>) =>
  a.t === b.t && (a.pts ?? null) === (b.pts ?? null) && (a.interp ?? "linear") === (b.interp ?? "linear") && mesmaCaixa(a.v, b.v);

/** `ancoras_do_programa` depois do "Simplificar": os `t` das âncoras devolvidas que não coincidem com âncora do perito
 *  (mesmo `t` e mesma caixa). Âncora do programa de antes que o perito não mexeu continua do programa. */
export function ancorasDoSimplificar(r: Rastreio, devolvidas: readonly Chave<Caixa>[]): Seg[] {
  const programa = new Set(r.ancoras_do_programa ?? []);
  const doPerito = porT(r.ancoras.filter((k) => !programa.has(k.t)));
  return devolvidas.filter((k) => !(doPerito.get(k.t) ?? []).some((a) => mesmaCaixa(a.v, k.v))).map((k) => k.t);
}

/** Âncoras por `t` (uma passada: a lista do Simplificar de um efeito longo tem milhares). */
function porT(ks: readonly Chave<Caixa>[]): Map<Seg, Chave<Caixa>[]> {
  const m = new Map<Seg, Chave<Caixa>[]>();
  for (const k of ks) {
    const v = m.get(k.t);
    if (v) v.push(k);
    else m.set(k.t, [k]);
  }
  return m;
}

/** Toda mudança do documento passa por aqui (`produzir` da store): âncora do programa acrescentada por cima, movida,
 *  apagada ou ajustada à mão sai de `ancoras_do_programa` (vira marcação do perito). A lista trocada na mesma mudança
 *  (o "Simplificar") fica como veio. Desfazer e refazer levam a lista com o documento. */
export function podarAncorasDoPrograma(
  antes: ProjetoEdicao,
  depois: ProjetoEdicao,
  mudar: (base: ProjetoEdicao, f: (d: Draft<ProjetoEdicao>) => void) => ProjetoEdicao,
): ProjetoEdicao {
  if (antes.rastreios === depois.rastreios) return depois;
  const podas = new Map<string, Seg[]>();
  for (const r of depois.rastreios) {
    const lista = r.ancoras_do_programa;
    if (!lista || lista.length === 0) continue;
    const velho = antes.rastreios.find((x) => x.id === r.id);
    if (!velho || velho.ancoras === r.ancoras || velho.ancoras_do_programa !== lista) continue;
    // por `t` numa passada: a cada passo do arrasto de uma alça (lista por âncora custava segundos com milhares)
    const va = porT(velho.ancoras as Chave<Caixa>[]);
    const vb = porT(r.ancoras as Chave<Caixa>[]);
    const intocada = (t: Seg) => {
      const a = va.get(t);
      const b = vb.get(t);
      return a?.length === 1 && b?.length === 1 && mesmaAncora(a[0] as Chave<Caixa>, b[0] as Chave<Caixa>);
    };
    const fica = lista.filter(intocada);
    if (fica.length !== lista.length) podas.set(r.id, fica);
  }
  if (podas.size === 0) return depois;
  return mudar(depois, (d) => {
    for (const r of d.rastreios) {
      const fica = podas.get(r.id);
      if (!fica) continue;
      if (fica.length > 0) r.ancoras_do_programa = fica;
      else {
        // sem âncora do programa, o registro das corridas dela não diz mais nada
        delete r.ancoras_do_programa;
        delete r.corridas_do_programa;
      }
    }
  });
}
