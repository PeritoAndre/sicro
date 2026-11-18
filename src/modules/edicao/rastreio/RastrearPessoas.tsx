/**
 * "Rastrear pessoas" de dentro da Edição (contratos-fase4.md D8): o MESMO motor e a mesma tabela da análise de Pessoas
 * (`pessoas_rastrear` na mídia do clipe, progresso pelo evento `pessoas-progress`, "Cancelar" por `pessoas_cancelar`);
 * ao terminar, o "Da análise" do `PainelRastreio` lista as trilhas. Só mídia do caso (`MidiaRef.origem = "caso"`).
 * Dono: pacote "Pessoas na Edição e pequenos" (casca da abertura; montada no painel só com `PESSOAS_NA_EDICAO_PRONTO`).
 */

import type { MidiaRef } from "@domain/edicao";

/** O pacote liga ao entregar (regra 3). */
export const PESSOAS_NA_EDICAO_PRONTO = false;

export interface RastrearPessoasProps {
  ws: string;
  midia: MidiaRef;
  /** Terminou (ou foi cancelado): o painel pede a lista do "Da análise" de novo. */
  aoTerminar(): void;
}

export function RastrearPessoas(props: RastrearPessoasProps) {
  void props;
  return null;
}
