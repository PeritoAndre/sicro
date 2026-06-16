/**
 * Catálogo de campos automáticos do laudo pericial.
 *
 * F5 — A fonte de verdade do que pode ser usado como `{{variavel}}` no
 * conteúdo do laudo. Cada campo é uma entrada declarativa com:
 *
 *   - `key`:     identificador estável usado em `{{key}}` e no atributo
 *                `data-field` do node `FieldPlaceholder`;
 *   - `label`:   nome humano exibido no painel/UI;
 *   - `group`:   agrupamento visual (Identificação, Local, Pessoas,
 *                Veículos, Vestígios, Mídia, Sistema);
 *   - `source`:  de onde o resolver tira o valor — pode ser uma
 *                referência a `occurrence.*`, `metadata.*`, `system.*`
 *                ou um valor fixo computado (`date_today`, etc.);
 *   - `required`: se a validação deve cobrar antes de exportar;
 *   - `description`: dica para o perito.
 *
 * O catálogo é a única source-of-truth — o painel de campos, o resolver
 * em runtime e a validação consomem todos a mesma lista.
 *
 * Para adicionar um campo: append em `LAUDO_FIELDS` + (se necessário)
 * extend `resolveFieldValue` para um source novo.
 */

export type LaudoFieldGroup =
  | "identificacao"
  | "local"
  | "pessoas"
  | "veiculos"
  | "vestigios"
  | "midia"
  | "sistema";

export type LaudoFieldSource =
  | { kind: "occurrence"; field: string }
  | { kind: "metadata"; field: string }
  | { kind: "system"; field: "data_hoje" | "data_hora_agora" }
  | { kind: "fixed"; value: string }
  /**
   * Contadores de página — equivalentes a `{ PAGE }` e `{ NUMPAGES }` do Word.
   * NÃO resolvíveis estaticamente: cada página do documento exporta um valor
   * diferente. Resolver retorna `""`; o renderer HTML/PDF substitui pelo
   * `counter(page)`/`counter(pages)` do CSS (Paged Media) na hora do export.
   */
  | { kind: "page_counter"; counter: "page" | "pages" };

export interface LaudoFieldDefinition {
  key: string;
  label: string;
  group: LaudoFieldGroup;
  source: LaudoFieldSource;
  required?: boolean;
  description?: string;
  /** Sufixo de unidade exibido no painel (ex: "km/h", "m"). */
  unit?: string;
}

export const LAUDO_FIELDS: ReadonlyArray<LaudoFieldDefinition> = [
  // --- IDENTIFICAÇÃO -------------------------------------------------------
  {
    key: "numero_laudo",
    label: "Número do laudo",
    group: "identificacao",
    source: { kind: "metadata", field: "numero_laudo" },
    required: true,
    description: "Identificador único deste laudo. Ex.: 12345/2026.",
  },
  {
    key: "numero_bo",
    label: "Número do BO",
    group: "identificacao",
    source: { kind: "occurrence", field: "numero_bo" },
    required: true,
    description: "Boletim de Ocorrência que originou o exame pericial.",
  },
  {
    key: "numero_requisicao",
    label: "Número da requisição",
    group: "identificacao",
    source: { kind: "occurrence", field: "requisicao" },
    description: "Requisição da autoridade requisitante.",
  },
  {
    key: "numero_oficio",
    label: "Número do ofício",
    group: "identificacao",
    source: { kind: "occurrence", field: "oficio" },
    description: "Ofício de encaminhamento da requisição.",
  },
  {
    key: "numero_procedimento",
    label: "Número do procedimento",
    group: "identificacao",
    source: { kind: "occurrence", field: "procedimento" },
    description: "Procedimento policial ou administrativo associado.",
  },
  {
    key: "tipo_exame",
    label: "Tipo de exame",
    group: "identificacao",
    source: { kind: "occurrence", field: "tipo_pericia" },
    required: true,
  },
  {
    key: "natureza_ocorrencia",
    label: "Natureza da ocorrência",
    group: "identificacao",
    source: { kind: "occurrence", field: "natureza_ocorrencia" },
  },
  {
    key: "autoridade_requisitante",
    label: "Autoridade requisitante",
    group: "identificacao",
    source: { kind: "occurrence", field: "autoridade_requisitante" },
  },
  {
    key: "setor",
    label: "Setor / departamento",
    group: "identificacao",
    source: { kind: "metadata", field: "setor" },
    description: "Setor da Polícia Científica responsável pelo exame.",
  },
  {
    key: "orgao",
    label: "Órgão",
    group: "identificacao",
    source: { kind: "fixed", value: "POLÍCIA CIENTÍFICA DO AMAPÁ" },
    description: "Órgão emissor (vem do template institucional).",
  },

  // --- LOCAL / DATA / HORA -------------------------------------------------
  {
    key: "data_pericia",
    label: "Data da perícia",
    group: "local",
    source: { kind: "occurrence", field: "data_fato" },
    required: true,
    description: "Data em que o exame foi realizado.",
  },
  {
    key: "hora_pericia",
    label: "Hora da perícia",
    group: "local",
    source: { kind: "occurrence", field: "hora_fato" },
  },
  {
    key: "data_elaboracao",
    label: "Data da elaboração",
    group: "local",
    source: { kind: "system", field: "data_hoje" },
    description: "Data corrente — preenchida automaticamente.",
  },
  {
    key: "local_pericia",
    label: "Local da perícia",
    group: "local",
    source: { kind: "occurrence", field: "endereco" },
    required: true,
  },
  {
    key: "municipio",
    label: "Município",
    group: "local",
    source: { kind: "occurrence", field: "municipio" },
    required: true,
  },
  {
    key: "uf",
    label: "UF",
    group: "local",
    source: { kind: "occurrence", field: "uf" },
  },
  {
    key: "coordenadas",
    label: "Coordenadas geográficas",
    group: "local",
    source: { kind: "occurrence", field: "coordenadas" },
    description: "Latitude/longitude (decimal).",
  },
  {
    key: "condicoes_climaticas",
    label: "Condições climáticas",
    group: "local",
    source: { kind: "occurrence", field: "condicoes_climaticas" },
  },
  {
    key: "condicoes_iluminacao",
    label: "Condições de iluminação",
    group: "local",
    source: { kind: "occurrence", field: "condicoes_iluminacao" },
  },

  // --- PESSOAS -------------------------------------------------------------
  {
    key: "nome_perito",
    label: "Nome do perito",
    group: "pessoas",
    source: { kind: "occurrence", field: "perito" },
    required: true,
  },
  {
    key: "matricula_perito",
    label: "Matrícula do perito",
    group: "pessoas",
    source: { kind: "occurrence", field: "matricula_perito" },
  },
  {
    key: "cargo_perito",
    label: "Cargo do perito",
    group: "pessoas",
    source: { kind: "fixed", value: "Perito Criminal" },
  },
  {
    key: "envolvidos",
    label: "Envolvidos",
    group: "pessoas",
    source: { kind: "occurrence", field: "envolvidos" },
    description: "Lista de pessoas envolvidas na ocorrência.",
  },

  // --- VEÍCULOS ------------------------------------------------------------
  {
    key: "veiculos",
    label: "Veículos examinados",
    group: "veiculos",
    source: { kind: "occurrence", field: "veiculos" },
  },
  {
    key: "placas",
    label: "Placas",
    group: "veiculos",
    source: { kind: "occurrence", field: "placas" },
  },
  {
    key: "chassis",
    label: "Chassis",
    group: "veiculos",
    source: { kind: "occurrence", field: "chassis" },
  },

  // --- VESTÍGIOS / MÍDIA ---------------------------------------------------
  {
    key: "vestigios",
    label: "Vestígios",
    group: "vestigios",
    source: { kind: "occurrence", field: "vestigios" },
  },
  {
    key: "midias",
    label: "Mídias / vídeos",
    group: "midia",
    source: { kind: "occurrence", field: "midias" },
  },
  {
    key: "croquis",
    label: "Croquis",
    group: "midia",
    source: { kind: "occurrence", field: "croquis" },
    description: "Croquis SICRO associados ao laudo.",
  },

  // --- SISTEMA -------------------------------------------------------------
  {
    key: "data_hoje",
    label: "Data atual",
    group: "sistema",
    source: { kind: "system", field: "data_hoje" },
    description: "Sempre a data corrente da máquina.",
  },
  {
    key: "data_hora_agora",
    label: "Data e hora atuais",
    group: "sistema",
    source: { kind: "system", field: "data_hora_agora" },
  },
  {
    key: "page",
    label: "Página atual",
    group: "sistema",
    source: { kind: "page_counter", counter: "page" },
    description:
      "Número da página corrente no documento exportado (equivale a {PAGE} do Word). " +
      "Resolvido pelo CSS Paged Media no PDF/HTML — cada página mostra seu próprio número.",
  },
  {
    key: "pages",
    label: "Total de páginas",
    group: "sistema",
    source: { kind: "page_counter", counter: "pages" },
    description:
      "Total de páginas do documento exportado (equivale a {NUMPAGES} do Word). " +
      "Combine com {page} para escrever frases como “Folha {page} de {pages}”.",
  },
];

/** Lookup rápido por key. */
export const LAUDO_FIELDS_BY_KEY: ReadonlyMap<string, LaudoFieldDefinition> =
  new Map(LAUDO_FIELDS.map((f) => [f.key, f]));

/** True se a key corresponde a um campo conhecido. */
export function isKnownFieldKey(key: string): boolean {
  return LAUDO_FIELDS_BY_KEY.has(key);
}

/** Resolve a definição de um campo, ou null. */
export function findField(
  key: string | null | undefined,
): LaudoFieldDefinition | null {
  if (!key) return null;
  return LAUDO_FIELDS_BY_KEY.get(key) ?? null;
}

/** Lista filtrada por grupo. */
export function fieldsByGroup(group: LaudoFieldGroup): LaudoFieldDefinition[] {
  return LAUDO_FIELDS.filter((f) => f.group === group);
}

/** Subconjunto requerido (para validação). */
export function requiredFields(): LaudoFieldDefinition[] {
  return LAUDO_FIELDS.filter((f) => f.required === true);
}

/** Label humano para um grupo. */
export function groupLabel(group: LaudoFieldGroup): string {
  switch (group) {
    case "identificacao":
      return "Identificação";
    case "local":
      return "Local, data e hora";
    case "pessoas":
      return "Pessoas";
    case "veiculos":
      return "Veículos";
    case "vestigios":
      return "Vestígios";
    case "midia":
      return "Mídia / croquis";
    case "sistema":
      return "Sistema";
  }
}

export const FIELD_GROUPS: ReadonlyArray<LaudoFieldGroup> = [
  "identificacao",
  "local",
  "pessoas",
  "veiculos",
  "vestigios",
  "midia",
  "sistema",
];
