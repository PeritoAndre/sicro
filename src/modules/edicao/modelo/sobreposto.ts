/**
 * Entrada e saída dos clipes sobrepostos (título, texto livre, imagem): espelho de `edicao/sobreposto.rs`, pela
 * fixture `sobreposto-transformacao.json` (corte, aparecer e deslizar; contratos-fase4.md D4). A prévia viva (P3.3)
 * pinta os sobrepostos com isto. E a folga que a sombra acrescenta ao sprite (`camada::texto::folga_da_sombra`,
 * fixture `texto-folga-sombra.json`): o palco põe as alças na caixa visível do texto, sem ela.
 * Dono: pacote do texto livre (contratos-fase4.md).
 */

import type { ClipeSobreposto, Seg, SombraTexto, Surgir } from "@domain/edicao";
import { avaliar, suave } from "./curva";

/** `TransformacaoAvaliada` do Rust: centro (0..1 do canvas), escala (1 = largura do canvas), graus, opacidade 0..1. */
export interface TransformacaoAvaliada { x: number; y: number; escala: number; rotacao: number; opacidade: number }

type BaseSob = Omit<ClipeSobreposto, "tipo">;

/** Deslocamento do "deslizar" (fração do canvas no eixo do lado), o `DESLIZE` do Rust. */
export const DESLIZE = 0.08;

/** O que o motor não faz (espelho de `sobreposto::recusa`); null = renderiza. Corte, aparecer e deslizar: tudo. */
export function recusa(b: Pick<BaseSob, "animacao">): string | null {
  void b;
  return null;
}

/** `Transformacao` no tempo local `tau` de um clipe de `dur` s, com a entrada e a saída (mesmas contas e ordem do Rust). */
export function transformacaoEm(b: Pick<BaseSob, "transformacao" | "entrada_s" | "saida_s" | "animacao">, tau: Seg, dur: Seg): TransformacaoAvaliada {
  const t = b.transformacao;
  const entrada: Surgir = b.animacao?.entrada ?? "aparecer";
  const saida: Surgir = b.animacao?.saida ?? "aparecer";
  const fracao = (s: Surgir, v: number, d: number) => (s !== "corte" && d > 0 && Number.isFinite(d) ? Math.min(Math.max(v / d, 0), 1) : 1);
  const ke = fracao(entrada, tau, b.entrada_s);
  const ks = fracao(saida, dur - tau, b.saida_s);
  const op = Math.min(Math.max(avaliar(t.opacidade, tau), 0), 1) * ke * ks;
  let x = avaliar(t.x, tau);
  let y = avaliar(t.y, tau);
  let desloc = 0;
  if (entrada === "deslizar") desloc += (1 - suave(ke)) * DESLIZE;
  if (saida === "deslizar") desloc += (1 - suave(ks)) * DESLIZE;
  // só com deslocamento: sem ele x e y saem como avaliados (−0 inclusive)
  if (desloc !== 0) {
    const lado = b.animacao?.lado ?? "baixo";
    if (lado === "cima") y -= desloc;
    else if (lado === "baixo") y += desloc;
    else if (lado === "esq") x -= desloc;
    else x += desloc;
  }
  return { x, y, escala: avaliar(t.escala, tau), rotacao: avaliar(t.rotacao, tau), opacidade: op };
}

/** `f64::round` do Rust: meio para longe do zero. */
const arredondar = (v: number) => (v < 0 ? -Math.round(-v) : Math.round(v));

/** Raios (px) das 3 passadas de caixa do desfoque de desvio `sigma` px (`camada::texto::raios_do_desfoque`). */
export function raiosDoDesfoque(sigma: number): [number, number, number] {
  if (!(sigma > 0) || !Number.isFinite(sigma)) return [0, 0, 0];
  const ideal = Math.sqrt(4 * sigma * sigma + 1);
  let wl = Math.floor(ideal);
  if (wl % 2 === 0) wl -= 1;
  const wu = wl + 2;
  const m = arredondar((12 * sigma * sigma - 3 * (wl * wl) - 12 * wl - 9) / (-4 * wl - 4));
  const raio = (i: number) => Math.floor(Math.max((i < m ? wl : wu) - 1, 0) / 2);
  return [raio(0), raio(1), raio(2)];
}

/** Quanto o sprite cresce de cada lado (px) para caber a sombra (`camada::texto::folga_da_sombra`). */
export function folgaDaSombra(s: SombraTexto, alturaCanvas: number): { x: number; y: number } {
  const r = raiosDoDesfoque(s.desfoque * alturaCanvas).reduce((a, b) => a + b, 0);
  return { x: Math.abs(arredondar(s.dx * alturaCanvas)) + r, y: Math.abs(arredondar(s.dy * alturaCanvas)) + r };
}
