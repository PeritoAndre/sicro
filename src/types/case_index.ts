/** Espelha o índice global de casos (`case-index.json`, Rust: `commands/case_index_commands.rs`). */

import type { Occurrence } from "./occurrence";
import type { WorkspaceCounters } from "./alpha";

/** Contagens da última abertura do caso. */
export interface CaseCounts {
  laudos: number;
  croquis: number;
  photos: number;
  videos: number;
  image_analyses: number;
  laudo_exports: number;
  image_exports: number;
  counted_at: string | null;
}

export interface CaseIndexEntry {
  workspace_id: string;
  workspace_path: string;
  /** Ausente em índices antigos. */
  titulo?: string | null;
  numero_bo: string | null;
  tipo_pericia: string | null;
  natureza: string | null;
  municipio: string | null;
  bairro: string | null;
  status: string;
  data_fato: string | null;
  data_acionamento: string | null;
  data_chegada: string | null;
  data_encerramento: string | null;
  peritos: string[];
  created_at: string | null;
  /** Definido pelo backend; o front pode enviar "". */
  indexed_at: string;
  /** `null` até o caso ser aberto. */
  counts: CaseCounts | null;
}

export function caseEntryFromOccurrence(
  occurrence: Occurrence,
  workspacePath: string,
): CaseIndexEntry {
  return {
    workspace_id: occurrence.id,
    workspace_path: workspacePath,
    titulo: occurrence.titulo,
    numero_bo: occurrence.numero_bo,
    tipo_pericia: occurrence.tipo_pericia,
    natureza: occurrence.natureza,
    municipio: occurrence.municipio,
    bairro: occurrence.bairro,
    status: occurrence.status,
    data_fato: occurrence.data_fato,
    data_acionamento: occurrence.data_acionamento,
    data_chegada: occurrence.data_chegada,
    data_encerramento: occurrence.data_encerramento,
    peritos: occurrence.peritos ?? [],
    created_at: occurrence.created_at,
    indexed_at: "",
    counts: null,
  };
}

export function caseCountsFromCounters(c: WorkspaceCounters): CaseCounts {
  return {
    laudos: c.laudos,
    croquis: c.croquis,
    photos: c.photos,
    videos: c.videos,
    image_analyses: c.image_analyses,
    laudo_exports: c.laudo_exports,
    image_exports: c.image_exports,
    counted_at: new Date().toISOString(),
  };
}
