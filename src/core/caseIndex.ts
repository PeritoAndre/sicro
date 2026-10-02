/** Índice global de casos (estatísticas gerais e navegador da Home). */

import { commands } from "@core/commands";
import {
  caseCountsFromCounters,
  caseEntryFromOccurrence,
  type CaseIndexEntry,
} from "@domain/case_index";

/** Backfill a partir dos recentes (best-effort); o índice cresce sozinho conforme casos abrem. */
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
        /* backend preserva as contagens anteriores */
      }
      await commands.upsertCaseIndex(entry);
    } catch {
      /* workspace movido/excluído */
    }
  }
  return commands.getCaseIndex();
}
