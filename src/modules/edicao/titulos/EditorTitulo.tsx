/** Editor do título pelos modelos Typst (P1.7): prévia do sprite do modelo, área segura, "Até o fim do clipe".
 *  Fase 0: casca, fora da `TituloSecao` até `EDITOR_TITULO_PRONTO`. */

import type { ClipeTitulo, SobTitulo } from "@domain/edicao";

export const EDITOR_TITULO_PRONTO = false;

export function EditorTitulo({ clipe }: { clipe: ClipeTitulo | SobTitulo }) {
  void clipe;
  return null;
}
