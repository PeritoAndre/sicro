/**
 * Espelho de `camada/lupa.rs` (P1.2): primitivas da lupa (`lupa` + moldura + ligação), pela fixture
 * `primitivas-lupa.json`. Fase 0: `nao_suportado`.
 */

import type { EfLupa, Primitiva, QuadroPalco } from "@domain/edicao";
import type { Pintura } from "./pintar";

export function primitivasLupa(e: EfLupa, q: QuadroPalco): Primitiva[] {
  void q;
  throw new Error(`nao_suportado: ${e.tipo}`);
}

/** Pinta a ampliação de uma primitiva `lupa` (origem do `p.video` com `p.A` → destino, recortada na forma); o pintor
 *  chama aqui. Fase 0: nada (nenhuma lupa chega ao pintor). */
export function pintarLupa(p: Pintura, l: Extract<Primitiva, { k: "lupa" }>): void {
  void p;
  void l;
}
