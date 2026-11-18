/**
 * Gestos da linha do tempo sobre o rascunho (dentro de `iniciarGesto`: um arrasto = um passo). Regras em
 * contratos-fase4.md D3 (principal), D5 (livre → `pos`) e D6 (faixas de efeito → `ativo`). Puro. Dono: P4.1.
 */

import type { Draft } from "immer";
import type { Id, LinhaResolvida, ProjetoEdicao } from "@domain/edicao";

export type Ferramenta = "selecionar" | "lamina";

/** Raio do ímã em px da tela. */
export const IMA_PX = 8;

/** O slot mais perto de `q` entre os `alvos` (bordas de clipe, cabeça, marcadores, chaves) a até `raio_q`; senão `q`. */
export function imantar(q: number, alvos: readonly number[], raio_q: number): number {
  void alvos;
  void raio_q;
  return q;
}

/** Bordas de todos os itens, a cabeça e os marcadores: os alvos do ímã (sem os do próprio item arrastado). */
export function alvosDoIma(doc: ProjetoEdicao, linha: LinhaResolvida, cabeca_q: number, exceto: Id | null): number[] {
  void doc;
  void linha;
  void cabeca_q;
  void exceto;
  return [];
}

/** V1: arrastar = inserir o clipe na posição `para` da principal (a mesma operação de `sequencia/acoes.moverPara`). */
export function moverNaPrincipal(d: Draft<ProjetoEdicao>, clipe: Id, para: number): void {
  void d;
  void clipe;
  void para;
}

/** V1: aparar a borda `lado` até o slot `q` (início/fim do trecho pelo quadro exibido; duração dos sem fonte). */
export function aparar(d: Draft<ProjetoEdicao>, linha: LinhaResolvida, clipe: Id, lado: "inicio" | "fim", q: number): void {
  void d;
  void linha;
  void clipe;
  void lado;
  void q;
}

/** Livre (sobreposição ou áudio): começa no slot `de_q` (D5), opcionalmente em outra trilha (`trilha` nova: criada
 *  acima da de cima). `manterAncora` (Alt): fica preso ao mesmo clipe, só muda o deslocamento. */
export function moverLivre(d: Draft<ProjetoEdicao>, linha: LinhaResolvida, livre: Id, de_q: number, op: { trilha?: Id | "nova"; manterAncora?: boolean }): void {
  void d;
  void linha;
  void livre;
  void de_q;
  void op;
}

/** Livre: aparar `lado` até `q` (`duracao_s` passa a valer; "até o fim do clipe" vira duração explícita). */
export function apararLivre(d: Draft<ProjetoEdicao>, linha: LinhaResolvida, livre: Id, lado: "inicio" | "fim", q: number): void {
  void d;
  void linha;
  void livre;
  void lado;
  void q;
}
