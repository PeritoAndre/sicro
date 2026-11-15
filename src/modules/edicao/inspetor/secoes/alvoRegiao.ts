/**
 * Alças do alvo de `ocultar` e `holofote` no palco (§8.3): as da caixa do rastreio (do registro) e, na forma
 * "Polígono", as dos vértices (`alvo.vertices`, `Mudanca` `{ efeito }`). Dono: P2.6 (contratos-fase2.md (c)).
 * Abertura: só as da caixa.
 */

import type { Alca, EfHolofote, EfOcultar, QuadroPalco } from "@domain/edicao";

export function alcasDoAlvo(e: EfOcultar | EfHolofote, q: QuadroPalco, daCaixa: Alca[]): Alca[] {
  void e;
  void q;
  return daCaixa;
}
