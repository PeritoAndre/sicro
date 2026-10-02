/** Espelha `src-tauri/src/models/occurrence.rs` (snake_case = wire do serde; não renomear sem mudar o Rust). */

export type OccurrenceStatus =
  | "aberta"
  | "em_andamento"
  | "concluida"
  | "arquivada";

export interface Occurrence {
  /** Também é o workspace_id. */
  id: string;
  /** Nome livre do caso; é o rótulo principal. */
  titulo: string | null;
  numero_bo: string | null;
  protocolo: string | null;
  requisicao: string | null;
  oficio: string | null;
  delegacia: string | null;
  tipo_pericia: string | null;
  natureza: string | null;
  municipio: string | null;
  bairro: string | null;
  logradouro: string | null;
  referencia: string | null;
  latitude: number | null;
  longitude: number | null;
  /** ISO-8601 timestamps. */
  data_fato: string | null;
  data_acionamento: string | null;
  data_chegada: string | null;
  data_encerramento: string | null;
  peritos: string[];
  status: OccurrenceStatus;
  created_at: string;
  updated_at: string;

  // Preenchidos só quando a ocorrência veio de um .sicroapp.
  import_id?: string | null;
  original_mobile_id?: string | null;
  primary_accuracy_m?: number | null;
  resultado?: string | null;
  raw_case_json?: string | null;
  raw_metadata_json?: string | null;
  raw_location_json?: string | null;
}

export interface NewOccurrenceInput {
  titulo?: string | null;
  numero_bo?: string | null;
  protocolo?: string | null;
  /** Ofício da Polícia Civil; distinto do protocolo (nº do laudo). */
  oficio?: string | null;
  tipo_pericia?: string | null;
  municipio?: string | null;
  peritos?: string[];
  /** null = pasta Documentos do SO. */
  parent_directory?: string | null;
}

/** Patch de `update_occurrence`: "" vira NULL no backend; proveniência (import_id, raw_*) nunca é tocada. */
export interface OccurrenceEdit {
  titulo?: string | null;
  numero_bo?: string | null;
  protocolo?: string | null;
  requisicao?: string | null;
  oficio?: string | null;
  delegacia?: string | null;
  tipo_pericia?: string | null;
  natureza?: string | null;
  resultado?: string | null;
  municipio?: string | null;
  bairro?: string | null;
  logradouro?: string | null;
  referencia?: string | null;
  latitude?: number | null;
  longitude?: number | null;
  status?: OccurrenceStatus | null;
  peritos?: string[];
}

export interface RecentOccurrence {
  workspace_id: string;
  workspace_path: string;
  occurrence_label: string;
  tipo_pericia: string | null;
  municipio: string | null;
  status: OccurrenceStatus;
  last_opened_at: string;
}

export interface LoadedOccurrence {
  occurrence: Occurrence;
  workspace_path: string;
}

/** Espelha `build_label` do Rust (mudar nos dois): título; senão "BO — tipo — município"; senão a data. */
export function occurrenceLabel(
  o: Pick<Occurrence, "titulo" | "numero_bo" | "tipo_pericia" | "municipio" | "created_at">,
): string {
  const t = o.titulo?.trim();
  if (t) return t;
  const parts: string[] = [];
  if (o.numero_bo) parts.push(`BO ${o.numero_bo}`);
  if (o.tipo_pericia) parts.push(o.tipo_pericia);
  if (o.municipio) parts.push(o.municipio);
  if (parts.length) return parts.join(" — ");
  const d = new Date(o.created_at);
  return Number.isNaN(d.getTime())
    ? "Caso sem nome"
    : `Caso de ${d.toLocaleDateString("pt-BR")}`;
}

/** `update_occurrence` zera campo ausente: quem muda um só campo parte deste patch completo. */
export function editFromOccurrence(o: Occurrence): OccurrenceEdit {
  return {
    titulo: o.titulo,
    numero_bo: o.numero_bo,
    protocolo: o.protocolo,
    requisicao: o.requisicao,
    oficio: o.oficio,
    delegacia: o.delegacia,
    tipo_pericia: o.tipo_pericia,
    natureza: o.natureza,
    resultado: o.resultado ?? null,
    municipio: o.municipio,
    bairro: o.bairro,
    logradouro: o.logradouro,
    referencia: o.referencia,
    latitude: o.latitude,
    longitude: o.longitude,
    status: o.status,
    peritos: [...o.peritos],
  };
}
