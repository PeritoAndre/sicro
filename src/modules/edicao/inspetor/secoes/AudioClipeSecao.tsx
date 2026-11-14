/** Som do clipe (P1.5): volume animado e "Silenciar trecho" (Tom, Mudo) · "Da degravação…" · "Motivo".
 *  Fase 0: casca, fora das seções até `AUDIO_CLIPE_PRONTO`. */

import type { ClipeTrecho } from "@domain/edicao";
import type { SecaoProps } from "../registro";

export const AUDIO_CLIPE_PRONTO = false;

export function AudioClipeSecao(props: SecaoProps<ClipeTrecho>) {
  void props;
  return null;
}
