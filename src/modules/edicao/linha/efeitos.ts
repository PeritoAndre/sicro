/**
 * Faixas de efeito da linha do tempo (contratos-fase4.md D2, D6): cada efeito de clipe é uma barra no intervalo
 * `ativo`, convertido entre a base do clipe (fonte no trecho, local nos demais) e os slots do programa pelo mapa
 * do clipe; os quadros-chave (região do enquadramento, âncoras do rastreio seguido) viram losangos. "Começar no
 * cursor" e "Terminar no cursor" (Inspetor, `MarcacoesSecao`) usam `ativoNoCursor`. Puro. Dono: P4.1.
 */

import type { Desconhecido, Efeito, Id, Intervalo, LinhaResolvida, ProjetoEdicao, Seg } from "@domain/edicao";

/** Chave da faixa do efeito: tipo + rastreio seguido (`destaque`, `lupa`, `ocultar`, `holofote`, velocidade que
 *  acompanha, enquadramento que segue), ou só o tipo. */
export function chaveDaFaixa(e: Efeito): string {
  void e;
  return "";
}

/** `ativo` (base do clipe) em slots do programa `[de_q, ate_q)`; null = fora do clipe. */
export function slotsDoAtivo(doc: ProjetoEdicao, linha: LinhaResolvida, clipe: Id, ativo: Intervalo | null): [number, number] | null {
  void doc;
  void linha;
  void clipe;
  void ativo;
  return null;
}

/** Barra arrastada até os slots `[de_q, ate_q)` → `ativo` na base do clipe (borda do clipe = campo nulo). */
export function ativoDoArrasto(doc: ProjetoEdicao, linha: LinhaResolvida, clipe: Id, de_q: number, ate_q: number): Intervalo {
  void doc;
  void linha;
  void clipe;
  void de_q;
  void ate_q;
  return { de: null, ate: null };
}

/** "Começar no cursor" / "Terminar no cursor": o `ativo` com a borda `lado` no tempo do efeito no cursor (`t`,
 *  `ContextoClipe.tempo()`); a outra borda fica, e nunca cruza (fica 1 quadro antes ou depois). */
export function ativoNoCursor(ativo: Intervalo | null, lado: "de" | "ate", t: Seg, passo_s: Seg): Intervalo {
  void lado;
  void t;
  void passo_s;
  return ativo ?? { de: null, ate: null };
}

/** Instantes (base do clipe) dos quadros-chave do efeito: chaves da região do enquadramento; âncoras do rastreio
 *  que ele segue (destaque, lupa, ocultar, holofote, velocidade que acompanha, enquadramento que segue). */
export function chavesDoEfeito(doc: ProjetoEdicao, e: Efeito | Desconhecido): Seg[] {
  void doc;
  void e;
  return [];
}
