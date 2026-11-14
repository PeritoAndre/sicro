/**
 * Contas do transporte no modo clipe (§7.1, §8.2, §8.5): a faixa cobre a mídia inteira; o clipe é a parte
 * destacada, com as bordas presas a fronteiras de quadro. Entrada e saída gravam o pts do quadro exibido no
 * cursor e podem ALARGAR o clipe. Tudo pela tabela (D4), nunca pela taxa declarada.
 */

import type { ClipeTrecho, MidiaInfo, Pts, Seg, TabelaQuadros } from "@domain/edicao";
import { exibido, fimDaMidia, fimDoPar, doPar, nQuadros, passoDe, ptsDaTabela, tempoDoQuadro } from "../modelo/tempo";

export interface Escala {
  de: Seg;
  ate: Seg;
  largura: number;
}

const limitar = (v: number, a: number, b: number) => Math.min(Math.max(v, a), b);

export function xDoTempo(e: Escala, t: Seg): number {
  const d = e.ate - e.de;
  return d > 0 ? ((t - e.de) / d) * e.largura : 0;
}

export function tempoDoX(e: Escala, x: number): Seg {
  return e.largura > 0 ? e.de + (limitar(x, 0, e.largura) / e.largura) * (e.ate - e.de) : e.de;
}

/** Fronteira de quadro mais próxima de `t`: índice 0..n (n = fim da mídia). */
export function fronteira(tab: TabelaQuadros, t: Seg): number {
  const n = nQuadros(tab);
  const j = exibido(tab, t);
  const ini = tempoDoQuadro(tab, j);
  const fim = j + 1 < n ? tempoDoQuadro(tab, j + 1) : fimDaMidia(tab);
  return t - ini <= fim - t ? j : j + 1;
}

/** Tempo e pts de uma fronteira; a do fim da mídia não tem pts. */
export function tempoDaFronteira(tab: TabelaQuadros, j: number): { t: Seg; pts: Pts | null } {
  const n = nQuadros(tab);
  if (j >= n) return { t: fimDaMidia(tab), pts: null };
  const k = Math.max(0, j);
  return { t: tempoDoQuadro(tab, k), pts: ptsDaTabela(tab)[k] as number };
}

/** Meio do quadro j em tempo de fonte: o cursor guarda este. Mesma conta de `relogioMidia.meio`. */
export function meioDoQuadro(tab: TabelaQuadros, j: number): Seg {
  const n = nQuadros(tab);
  const k = limitar(Math.trunc(j), 0, n - 1);
  return Number((2n * BigInt(ptsDaTabela(tab)[k] as number) + BigInt(passoDe(tab, k))) * BigInt(tab.tb.num)) / (2 * tab.tb.den);
}

type Pontas = Pick<ClipeTrecho, "inicio_s" | "inicio_pts" | "fim_s" | "fim_pts">;

/** [j_in, j_out) do clipe nessa tabela. */
export function faixaDoClipe(c: Pontas, tab: TabelaQuadros): [number, number] {
  return [doPar(tab, c.inicio_s, c.inicio_pts), fimDoPar(tab, c.fim_s, c.fim_pts)];
}

/** Campos novos ao levar uma borda à fronteira `j`; null = nada muda ou o trecho ficaria vazio. */
export function aparar(c: Pontas, borda: "inicio" | "fim", j: number, tab: TabelaQuadros): Partial<Pontas> | null {
  const n = nQuadros(tab);
  const [a, b] = faixaDoClipe(c, tab);
  if (borda === "inicio") {
    const k = limitar(j, 0, n - 1);
    if (k >= b || k === a) return null;
    const f = tempoDaFronteira(tab, k);
    return { inicio_s: f.t, inicio_pts: f.pts };
  }
  const k = limitar(j, 1, n);
  if (k <= a || k === b) return null;
  const f = tempoDaFronteira(tab, k);
  return { fim_s: f.t, fim_pts: f.pts };
}

/**
 * Borda arrastada na pista: anda o quanto o ponteiro andou desde que pegou a borda (`t0` → `t`, tempos da pista),
 * presa à fronteira de quadro. Pegar e soltar sem mover não muda nada (numa mídia longa 1 px vale muitos quadros).
 * Ponteiro na ponta da pista = ponta da mídia.
 */
export function bordaArrastada(c0: Pontas, borda: "inicio" | "fim", t0: Seg, t: Seg, tab: TabelaQuadros, pista: readonly [Seg, Seg]): Partial<Pontas> | null {
  if (t === t0) return null;
  const base = borda === "inicio" ? c0.inicio_s : c0.fim_s;
  const alvo = t <= pista[0] ? pista[0] : t >= pista[1] ? pista[1] : base + (t - t0);
  return aparar(c0, borda, fronteira(tab, alvo), tab);
}

/** Tecla de entrada (I): o quadro exibido no cursor passa a ser o primeiro. */
export const entradaNoCursor = (c: Pontas, tab: TabelaQuadros, t: Seg) => aparar(c, "inicio", exibido(tab, t), tab);
/** Tecla de saída (O): o quadro exibido no cursor fica DENTRO; o fim (exclusivo) é o quadro seguinte. No último
 *  quadro da mídia o fim é o fim da mídia (pts nulo). */
export const saidaNoCursor = (c: Pontas, tab: TabelaQuadros, t: Seg) => aparar(c, "fim", exibido(tab, t) + 1, tab);

/** Marcar entrada, saída ou quadro-chave: só com `MidiaInfo.marcavel` (contratos (h) 17), a única regra. */
export function marcarBloqueado(info: Pick<MidiaInfo, "marcavel"> | null, tab: TabelaQuadros | null): boolean {
  return !tab || !info?.marcavel;
}

/** Estado da busca do palco: quadro pedido e ainda não pintado, e o último pintado (`Buscador`). */
export interface EstadoBusca {
  alvo: number | null;
  ultimo: number | null;
}

/**
 * Marca (âncora, chave, região) no quadro mostrado. Clipe com mídia: só marcável (h 17) e com a imagem do quadro `j`
 * já pintada; enquanto o palco busca (troca de clipe, passo), a tela ainda mostra o quadro anterior (h 27).
 */
export function marcaLivre(tipo: string | null, info: Pick<MidiaInfo, "marcavel"> | null, tab: TabelaQuadros | null, busca: EstadoBusca | null, j: number | null): boolean {
  if (!tipo) return false;
  if (tipo !== "trecho" && tipo !== "congelar") return true;
  if (marcarBloqueado(info, tab) || !busca) return false;
  return busca.alvo === null && busca.ultimo !== null && busca.ultimo === j;
}
