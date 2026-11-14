/**
 * Texto do `CampoTempo` (§3.1 "Na tela"): mostra `HH:MM:SS,mmm`; aceita `12,5`, `1:02,5`, `1:02:03,250`
 * e `#312` (quadro: resolvido pela tabela na base "fonte", pela taxa da saída na base "programa").
 */

import type { Pts, Racional, Seg, TabelaQuadros } from "@domain/edicao";
import { exibido, fimDaMidia, nQuadros, ptsDaTabela, tempoDoQuadro } from "@modules/edicao/modelo/tempo";

export type BaseTempo = "fonte" | "programa";

export interface RefTempo {
  base: BaseTempo;
  /** Base "fonte": tabela da mídia. */
  tabela?: TabelaQuadros | null;
  /** Base "programa": taxa da saída. */
  fps?: Racional | null;
}

export interface TempoLido {
  t: Seg;
  /** Só quando o quadro veio da tabela. */
  pts: Pts | null;
}

const dois = (v: number) => String(v).padStart(2, "0");

/** `HH:MM:SS,mmm`, arredondado ao milissegundo; horas além de 99 continuam. */
export function formatarTempo(t: Seg): string {
  if (!Number.isFinite(t)) return "";
  const ms = Math.round(Math.abs(t) * 1000);
  const s = Math.floor(ms / 1000);
  const sinal = t < 0 && ms > 0 ? "-" : "";
  return `${sinal}${dois(Math.floor(s / 3600))}:${dois(Math.floor(s / 60) % 60)}:${dois(s % 60)},${String(ms % 1000).padStart(3, "0")}`;
}

/** Quadro (índice na tabela) ou slot do programa no tempo `t`; null sem referência. */
export function quadroDe(t: Seg, ref: RefTempo): number | null {
  if (!Number.isFinite(t)) return null;
  if (ref.base === "fonte") return ref.tabela ? exibido(ref.tabela, t) : null;
  // slot exibido: maior q com q·den/num ≤ t, com folga de 1 µs
  return ref.fps ? Math.max(0, Math.floor((t + 1e-6) * ref.fps.num / ref.fps.den)) : null;
}

/** Tempo do quadro `n` (início dele): pela tabela ou pela taxa da saída. */
export function tempoDe(n: number, ref: RefTempo): TempoLido | null {
  if (!Number.isInteger(n) || n < 0) return null;
  if (ref.base === "fonte") {
    const tab = ref.tabela;
    if (!tab || n >= nQuadros(tab)) return null;
    return { t: tempoDoQuadro(tab, n), pts: ptsDaTabela(tab)[n] as number };
  }
  return ref.fps ? { t: (n * ref.fps.den) / ref.fps.num, pts: null } : null;
}

/** Lê o que foi digitado; null = não entendeu. */
export function lerTempo(texto: string, ref: RefTempo): TempoLido | null {
  const s = texto.trim().replace(/\s+/g, "");
  if (s === "") return null;
  if (s.startsWith("#")) {
    const n = /^#(\d+)$/.exec(s);
    return n ? tempoDe(Number(n[1]), ref) : null;
  }
  const m = /^(-)?(?:(\d+):)?(?:(\d+):)?(\d+(?:[.,]\d*)?|[.,]\d+)$/.exec(s);
  if (!m) return null;
  const [, menos, a, b, seg] = m;
  // com dois ":" as partes são h, min, s; com um, min e s
  const h = b !== undefined ? Number(a) : 0;
  const min = b !== undefined ? Number(b) : a !== undefined ? Number(a) : 0;
  const sec = Number((seg as string).replace(",", "."));
  if (!Number.isFinite(sec)) return null;
  const t = h * 3600 + min * 60 + sec;
  return { t: menos ? -t : t, pts: null };
}

/** Um quadro para frente ou para trás (pela tabela ou pela taxa); sem referência, 1 ms. */
export function passoQuadro(t: Seg, sentido: 1 | -1, ref: RefTempo): TempoLido {
  const tab = ref.base === "fonte" ? ref.tabela : null;
  if (tab && nQuadros(tab) > 0 && Number.isFinite(t)) {
    // no fim da mídia (o "Fim" de um trecho até o fim): ↓ vai ao último quadro, ↑ fica; o quadro exibido ali é o
    // último, e andar a partir dele tirava um quadro com ↑ e dois com ↓
    const fim = fimDaMidia(tab);
    if (t >= fim - tab.tb.num / tab.tb.den / 2) return sentido < 0 ? (tempoDe(nQuadros(tab) - 1, ref) as TempoLido) : { t: Math.max(t, fim), pts: null };
  }
  const q = quadroDe(t, ref);
  if (q !== null) {
    // fora da tabela fica no quadro atual
    return tempoDe(Math.max(0, q + sentido), ref) ?? tempoDe(q, ref) ?? { t, pts: null };
  }
  return { t: t + sentido * 0.001, pts: null };
}
