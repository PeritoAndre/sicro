/**
 * Mirror of Rust structs in src-tauri/src/models/occurrence.rs.
 * Keep these manually in sync until a code-generation strategy is adopted.
 *
 * The wire format is JSON (serde), so field names are snake_case as emitted
 * by serde's default serialization. Do NOT rename without changing the Rust side.
 */

export type OccurrenceStatus =
  | "aberta"
  | "em_andamento"
  | "concluida"
  | "arquivada";

export interface Occurrence {
  /** UUID v4 — also used as workspace_id. */
  id: string;
  /** Nome livre do caso, dado no Início. É o rótulo principal; o resto é complemento. */
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

  // ---- Spike D fields (only populated when the occurrence came from a
  //      .sicroapp import; null for hand-created Spike A occurrences). ----
  import_id?: string | null;
  original_mobile_id?: string | null;
  primary_accuracy_m?: number | null;
  resultado?: string | null;
  raw_case_json?: string | null;
  raw_metadata_json?: string | null;
  raw_location_json?: string | null;
}

/** Payload used when creating a new occurrence. */
export interface NewOccurrenceInput {
  /** Nome do caso — o único campo que o Início pede (e mesmo ele é opcional). */
  titulo?: string | null;
  numero_bo?: string | null;
  protocolo?: string | null;
  /** Nº do ofício da Polícia Civil — distinto do protocolo (nº do laudo, PC). */
  oficio?: string | null;
  tipo_pericia?: string | null;
  municipio?: string | null;
  peritos?: string[];
  /** Where the .sicro folder should be created. If null, defaults to the OS Documents folder. */
  parent_directory?: string | null;
}

/**
 * Patch enviado a `update_occurrence` (cabeçalho do caso editável no Dossiê).
 * O perito é a palavra final; "" vira NULL no backend. A proveniência
 * (import_id, raw_*, etc.) nunca é tocada.
 */
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

/** Entry shown in the "recent occurrences" list on Home. */
export interface RecentOccurrence {
  workspace_id: string;
  workspace_path: string;
  occurrence_label: string;
  tipo_pericia: string | null;
  municipio: string | null;
  status: OccurrenceStatus;
  last_opened_at: string;
}

/** Returned by open/load operations: occurrence + path. */
export interface LoadedOccurrence {
  occurrence: Occurrence;
  workspace_path: string;
}

/**
 * Rótulo do caso — espelho de `build_label` (src-tauri/src/models/occurrence.rs):
 * o nome dado pelo perito; sem nome, "BO — tipo — município" (casos antigos);
 * sem nada, a data de criação. Mudar nos dois.
 */
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

/**
 * Patch completo a partir do caso como está. `update_occurrence` sobrescreve
 * TODOS os campos editáveis (campo ausente vira NULL), então quem muda um só
 * campo parte daqui e troca o que quer — senão apaga o resto sem querer.
 */
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
