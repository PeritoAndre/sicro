/** Clipe · Realce (P1.4): "Estabilizar" · "Suavizar" · "Borda" (Preta, Ampliar) · "Refazer estabilização" · "Lente" ·
 *  "Desentrelaçar" (Quadro, Campo) · "Brilho" · "Contraste" · "Gama" · "Saturação" · "Nitidez" · "Média de quadros".
 *  Intermediário de origem por `edicaoOrigemPedir`. Fase 0: casca; o registro usa `RealceSoLeitura` até `REALCE_PRONTO`. */

import type { SecaoProps } from "../registro";
import type { ClipeQualquer } from "../acoes";

export const REALCE_PRONTO = false;

export function RealceSecao(props: SecaoProps<ClipeQualquer>) {
  void props;
  return null;
}
