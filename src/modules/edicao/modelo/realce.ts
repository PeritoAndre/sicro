/**
 * Espelho de `motor::efeitos::realce::tabela` (P1.4): a tabela que o mapa de quadros do clipe usa (a da fonte; no
 * desentrelace por campo, uma entrada por campo). `modelo/tempo.ts` (resolver) pergunta aqui. Fase 0: a da fonte.
 */

import type { Desconhecido, Efeito, TabelaQuadros } from "@domain/edicao";

export function tabelaDoClipe(tab: TabelaQuadros, efeitos: readonly (Efeito | Desconhecido)[]): TabelaQuadros {
  void efeitos;
  return tab;
}
