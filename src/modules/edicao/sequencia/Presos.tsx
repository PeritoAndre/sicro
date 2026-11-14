/** Presos ao clipe (§8.2, P1.7): o que tem `pos.ref = "clipe"` (título, terço inferior, imagem, narração), com
 *  "Adicionar" → "Título", "Terço inferior", "Imagem", "Narração". Fase 0: casca, fora do cartão até `PRESOS_PRONTOS`. */

import type { Id } from "@domain/edicao";

export const PRESOS_PRONTOS = false;

export function Presos({ clipe }: { clipe: Id }) {
  void clipe;
  return null;
}
