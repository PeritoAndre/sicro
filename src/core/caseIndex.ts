/**
 * Utilitário do índice global de casos (base das estatísticas gerais e do
 * navegador de ocorrências da Home).
 */

import { commands } from "@core/commands";
import {
  caseCountsFromCounters,
  caseEntryFromOccurrence,
  type CaseIndexEntry,
} from "@domain/case_index";

/**
 * Backfill: lê os casos recentes, re-lê cada ocorrência e atualiza o índice.
 * Devolve o índice já atualizado. Best-effort — recentes inacessíveis são
 * ignorados. (O índice cresce naturalmente conforme casos são abertos; isto
 * só acelera o preenchimento inicial a partir da lista de recentes.)
 */
export async function reindexCaseIndexFromRecents(): Promise<CaseIndexEntry[]> {
  const recents = await commands.listRecentOccurrences();
  for (const r of recents) {
    try {
      const occ = await commands.getOccurrence(r.workspace_path);
      const entry = caseEntryFromOccurrence(occ, r.workspace_path);
      try {
        entry.counts = caseCountsFromCounters(
          await commands.getOccurrenceCounts(r.workspace_path),
        );
      } catch {
        /* contagens best-effort — backend preserva as anteriores */
      }
      await commands.upsertCaseIndex(entry);
    } catch {
      /* recente inacessível (workspace movido/excluído) — ignora */
    }
  }
  return commands.getCaseIndex();
}
