/**
 * Sobrepostos no palco (texto livre, título, imagem): o que aparece no slot do programa e as alças do selecionado —
 * mover (centro), cantos (tamanho da letra), lados (largura da caixa), giro (rotação); arrastar com chaves grava a
 * chave no cursor (D4, D7). O palco (P3.3) pinta e liga as alças por aqui. Dono: pacote do texto livre (casca).
 */

import type { Alca, Id, LinhaResolvida, Primitiva, ProjetoEdicao, Seg, SobImagem, SobTitulo } from "@domain/edicao";
import type { TransformacaoAvaliada } from "../modelo/sobreposto";
import type { FontesCena } from "./cena";

export interface LivreNoQuadro {
  trilha: Id;
  clipe: SobTitulo | SobImagem;
  /** Tempo local no slot. */
  tau: Seg;
  transformacao: TransformacaoAvaliada;
  /** Primitivas do título (0..1 do canvas, antes da transformação); imagem: vazio (o pintor desenha a mídia). */
  primitivas: Primitiva[];
}

/** Sobrepostos visíveis no slot `q` do programa, na ordem de pintura (trilhas não ocultas, de baixo para cima). */
export function livresNoQuadro(doc: ProjetoEdicao, linha: LinhaResolvida, q: number, fontes: FontesCena): LivreNoQuadro[] {
  void doc;
  void linha;
  void q;
  void fontes;
  return [];
}

/** Alças do sobreposto selecionado (0..1 do canvas); cada uma devolve `{ livre, trilha, sobreposto }`. */
export function alcasDoLivre(doc: ProjetoEdicao, item: LivreNoQuadro, canvas: { w: number; h: number }): Alca[] {
  void doc;
  void item;
  void canvas;
  return [];
}
