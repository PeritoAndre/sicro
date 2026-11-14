/**
 * Espelho das legendas (P1.6): degravação → `Legenda` (tempo de fonte, origem "ia" não conferida) e legenda no tempo do
 * programa (fonte: segue o trecho, com velocidade e corte), pela fixture `legendas-*.json` que o `motor/legendas.rs`
 * gravar. Fase 0: casca.
 */

import type { Degravacao, Id, Legenda } from "@domain/edicao";

export function legendasDaDegravacao(d: Degravacao, midia: Id): Legenda[] {
  void d;
  void midia;
  throw new Error("nao_suportado: legendas");
}
