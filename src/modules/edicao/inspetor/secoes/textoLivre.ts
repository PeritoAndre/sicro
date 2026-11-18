/**
 * Texto livre (Fase 4; contratos-fase4.md D4): `SobTitulo` de modelo "livre" numa trilha de sobreposição, um bloco
 * centrado no `lugar` (0,5; 0,5) do título e posto pela `transformacao` (centro x, y; rotação; escala 1). Sem tela
 * aqui (a Sequência e a linha importam sem puxar o Inspetor). Dono: pacote do texto livre (casca da abertura).
 */

import type { Id, LinhaResolvida, Ponto, ProjetoEdicao, SobTitulo } from "@domain/edicao";

/** O pacote liga ao entregar (regra 3): botão "Texto livre", seção do Inspetor, alças no palco. */
export const TEXTO_LIVRE_PRONTO = false;

/** O sobreposto é um texto livre (título de modelo "livre" com um bloco)? */
export function ehTextoLivre(c: { tipo: string; titulo?: { modelo: string; blocos: unknown[] } }): boolean {
  return c.tipo === "titulo" && c.titulo?.modelo === "livre" && (c.titulo?.blocos.length ?? 0) === 1;
}

/** Texto livre novo no slot `q`, centrado em `centro` (o centro da área visível do palco), preso ao clipe da principal
 *  sob `q` (D5); `trilha` = a de sobreposição de cima, ou null para criar uma. Conteúdo vazio (nenhum texto de exemplo). */
export function novoTextoLivre(doc: ProjetoEdicao, linha: LinhaResolvida, q: number, centro: Ponto): { trilha: Id | null; clipe: SobTitulo } | null {
  void doc;
  void linha;
  void q;
  void centro;
  return null;
}

/** "Texto livre": cria no cursor (um passo de histórico), seleciona e abre o Inspetor no campo "Texto". */
export function adicionarTextoLivre(): Id | null {
  return null;
}
