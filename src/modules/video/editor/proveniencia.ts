/**
 * Proveniência da mídia (F6; P1.10): "Formato do gravador", "Observação", "Desvio do relógio (s)" e "Gravador",
 * gravados na análise (`video_proveniencia_salvar`) e copiados para o `MidiaRef` quando a mídia entra numa edição
 * (`store/coleta.ts` chama aqui). Fase 0: nada a copiar.
 */

import type { MidiaRef } from "@domain/edicao";

/** Campos do `MidiaRef` que vêm da proveniência gravada da mídia `mediaId`; null = nada gravado. */
export async function provenienciaDaMidia(ws: string, mediaId: string): Promise<Pick<MidiaRef, "recebido" | "relogio_desvio_s" | "gravador"> | null> {
  void ws;
  void mediaId;
  return null;
}
