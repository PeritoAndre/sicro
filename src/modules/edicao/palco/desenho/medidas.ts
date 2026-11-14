/**
 * Espelho de `camada/medidas.rs` (P1.3): régua de escala e velocidade exibida, pela fixture `primitivas-medidas.json`.
 * Fase 0: `nao_suportado`.
 */

import type { EfRegua, EfVelocidadeExibida, Primitiva, QuadroPalco } from "@domain/edicao";

export function primitivasRegua(e: EfRegua, q: QuadroPalco): Primitiva[] {
  void q;
  throw new Error(`nao_suportado: ${e.tipo}`);
}

export function primitivasVelocidade(e: EfVelocidadeExibida, q: QuadroPalco): Primitiva[] {
  void q;
  throw new Error(`nao_suportado: ${e.tipo}`);
}
