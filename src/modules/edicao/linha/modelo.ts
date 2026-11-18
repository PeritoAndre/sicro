/**
 * Linha do tempo (Fase 4, P4.1): OUTRA VISTA do mesmo `ProjetoEdicao` (contratos-fase4.md D1). Nada aqui é estado do
 * documento: as trilhas e os itens saem do documento e da linha resolvida (`modelo/tempo.ts::resolver`, a mesma da
 * Sequência), em slots do programa. Puro; testado sem tela. Dono: P4.1 (casca da abertura: tipos e assinaturas).
 */

import type { Id, ItemSel, LinhaResolvida, ProjetoEdicao } from "@domain/edicao";

/** De cima para baixo: sobreposições (a de cima primeiro), principal (V1), faixas de efeito, som dos trechos,
 *  trilhas de áudio (na ordem do documento) e legendas (D2). */
export type TipoTrilha = "sobreposicao" | "principal" | "efeito" | "som" | "audio" | "legendas";

/** Ids das trilhas que não existem no documento. */
export const TRILHA_PRINCIPAL = "principal";
export const TRILHA_SOM = "som";
export const TRILHA_LEGENDAS = "legendas";

/** Um item numa trilha, em slots do programa `[de_q, ate_q)`. */
export interface ItemNaLinha {
  /** Id do clipe, livre, clipe de áudio, legenda ou efeito. */
  id: Id;
  k: ItemSel["k"] | "efeito";
  /** Trilha do documento (sobreposição ou áudio) ou uma das fixas. */
  trilha: string;
  de_q: number;
  ate_q: number;
  rotulo: string;
  /** Efeito: o clipe da principal (ou sobreposto) dono dele. */
  clipe?: Id;
  /** Slots dos quadros-chave (losangos), dentro de `[de_q, ate_q)`. */
  chaves?: number[];
  /** Item citado num problema da análise. */
  erro?: boolean;
  /** Item de versão futura (`Desconhecido`): aparece apagado, sem arrasto. */
  apagado?: boolean;
}

/** Faixa de efeito: chave = tipo + rastreio seguido (ou só o tipo), na ordem da primeira aparição (D2). */
export interface TrilhaNaLinha {
  id: string;
  tipo: TipoTrilha;
  /** Nome na cabeça: o da trilha do documento, ou o do tipo de efeito + rastreio ("PP1 · Seta"). */
  nome: string;
  itens: ItemNaLinha[];
  oculta?: boolean;
  muda?: boolean;
}

/** As trilhas da linha, na ordem da tela, do documento e da linha resolvida (lista e linha sempre iguais). */
export function trilhasDaLinha(doc: ProjetoEdicao, linha: LinhaResolvida): TrilhaNaLinha[] {
  void doc;
  void linha;
  return [];
}

/** Slot do programa sob um x da tela (px), com `pxPorS` e a rolagem em s. */
export function slotNoX(x: number, pxPorS: number, rolagem_s: number, fps: { num: number; den: number }): number {
  void x;
  void pxPorS;
  void rolagem_s;
  void fps;
  return 0;
}
