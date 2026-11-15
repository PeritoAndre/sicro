/**
 * "Conferir ocultações" (§5.10): percorre no ARQUIVO exportado os intervalos ocultados (`edicao_conferencia_trechos`),
 * os trechos fracos (incerto, interpolado) primeiro; "Próximo trecho fraco", "Quadro anterior", "Próximo quadro";
 * termina em "Conferido" ou "Concluir sem conferir" (`conferirSaida`). Dono: P2.6.
 * Abertura: casca com `CONFERIR_PRONTO = false`; montada na Fila e nas Saídas ("A conferir") só com a bandeira.
 */

export const CONFERIR_PRONTO = false;

export interface ConferirSaidaProps {
  ws: string;
  saidaId: string;
}

/** Botão "Conferir ocultações" e o diálogo dele. */
export function ConferirSaida(p: ConferirSaidaProps) {
  if (!CONFERIR_PRONTO) return null;
  void p;
  return null;
}
