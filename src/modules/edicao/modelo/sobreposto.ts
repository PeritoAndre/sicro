/**
 * Entrada e saída dos clipes sobrepostos (título, texto livre, imagem): espelho de `edicao/sobreposto.rs`, pela
 * fixture `sobreposto-transformacao.json`. A prévia viva (P3.3) pinta os sobrepostos com isto.
 * Dono: pacote do texto livre (contratos-fase4.md); a abertura trouxe o que o motor já fazia (rampa de opacidade).
 */

import type { ClipeSobreposto, Seg, Surgir } from "@domain/edicao";
import { avaliar } from "./curva";

/** `TransformacaoAvaliada` do Rust: centro (0..1 do canvas), escala (1 = largura do canvas), graus, opacidade 0..1. */
export interface TransformacaoAvaliada { x: number; y: number; escala: number; rotacao: number; opacidade: number }

type BaseSob = Omit<ClipeSobreposto, "tipo">;

/** Deslocamento do "deslizar" (fração do canvas no eixo do lado), o `DESLIZE` do Rust. */
export const DESLIZE = 0.08;

/** O que o motor ainda não faz (espelho de `sobreposto::recusa`); null = renderiza. */
export function recusa(b: Pick<BaseSob, "animacao">): string | null {
  const a = b.animacao;
  const ok = (s: Surgir) => s === "aparecer";
  return a && (!ok(a.entrada) || !ok(a.saida)) ? "animacao_sobreposto" : null;
}

/** `Transformacao` no tempo local `tau` de um clipe de `dur` s, com a entrada e a saída na opacidade. */
export function transformacaoEm(b: Pick<BaseSob, "transformacao" | "entrada_s" | "saida_s" | "animacao">, tau: Seg, dur: Seg): TransformacaoAvaliada {
  const t = b.transformacao;
  const rampa = (v: number, d: number) => (d > 0 && Number.isFinite(d) ? Math.min(Math.max(v / d, 0), 1) : 1);
  const op = Math.min(Math.max(avaliar(t.opacidade, tau), 0), 1) * rampa(tau, b.entrada_s) * rampa(dur - tau, b.saida_s);
  return { x: avaliar(t.x, tau), y: avaliar(t.y, tau), escala: avaliar(t.escala, tau), rotacao: avaliar(t.rotacao, tau), opacidade: op };
}
