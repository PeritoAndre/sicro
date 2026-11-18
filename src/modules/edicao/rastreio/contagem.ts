/**
 * Contador de marcações à mão no painel do efeito (contratos-fase4.md D9): âncoras do perito, fora as que o
 * "Simplificar" escolheu e ele não mexeu (`Rastreio.ancoras_do_programa`). Puro. Dono: pacote "Pessoas na Edição e
 * pequenos" (casca da abertura).
 */

import type { Rastreio } from "@domain/edicao";

export function marcadasAMao(r: Pick<Rastreio, "ancoras" | "ancoras_do_programa">): number {
  void r;
  return 0;
}
