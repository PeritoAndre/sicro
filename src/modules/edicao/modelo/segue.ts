/**
 * Zoom de câmera que acompanha a pessoa: espelho de `edicao/segue.rs`, pelas fixtures `segue-*.json` que o Rust grava.
 * O palco passa todo `aDe` por `efeitosNoInstante` (cena.ts, Palco.tsx): com o enquadramento que segue, a região do
 * instante entra no lugar da gravada. Dono: pacote do zoom que segue (contratos-fase4.md); casca até `SEGUE_PRONTO`.
 */

import type { Caixa, Desconhecido, EfEnquadramento, Efeito, Ponto, ProjetoEdicao, Seg } from "@domain/edicao";

/** O pacote liga ao entregar (regra 3 de contratos-fase4.md). */
export const SEGUE_PRONTO = false;

/** Suavização que "Acompanhar rastreio" grava (s, na base do rastreio); o `SUAVIZAR_PADRAO_S` do Rust. */
export const SUAVIZAR_PADRAO_S = 1;

/** Centro da caixa do rastreio num instante da base dele; null = sem caixa ali. */
export interface AmostraSegue { t: Seg; centro: Ponto | null }

/** O que o motor ainda não faz (espelho de `segue::recusa`); null = renderiza. */
export function recusa(q: Pick<EfEnquadramento, "segue">): string | null {
  return q.segue ? "enquadramento_segue" : null;
}

/** Região (0..1 da origem girada) em cada instante de `em` (espelho de `segue::regiao_seguida`). */
export function regiaoSeguida(e: EfEnquadramento, amostras: readonly AmostraSegue[], em: readonly Seg[]): Caixa[] {
  void e;
  void amostras;
  void em;
  throw new Error("nao_suportado: enquadramento_segue");
}

/** Os efeitos com o enquadramento que segue trocado pela região do instante `t` (base do clipe), valor fixo.
 *  Sem enquadramento que segue (ou antes de `SEGUE_PRONTO`): a MESMA lista, sem cópia. */
export function efeitosNoInstante<L extends readonly (Efeito | Desconhecido)[]>(doc: ProjetoEdicao, efeitos: L, t: Seg): L {
  void doc;
  void t;
  return efeitos;
}
