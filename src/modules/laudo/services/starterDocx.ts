/**
 * starterDocx — monta o "envelope" inicial de um laudo `.docx` (SICRO 3.0).
 *
 * O laudo deixou de ter editor in-app: ele NASCE como um `.docx`. Quando o
 * perito cria um laudo "novo" (não registra um já existente), o backend
 * (`create_laudo_docx`) precisa de um envelope estilo `.sicrodoc`
 * (`content` + `layout` + `header` + `footer?`) que o mesmo walker do export
 * (`render_doc_to_docx`) transforma num `.docx` editável no Word/LibreOffice.
 *
 * Este builder produz um ESQUELETO simples (MVP):
 *   - um cabeçalho institucional semeado a partir do template;
 *   - alguns parágrafos de identificação que misturam texto + pílulas de campo
 *     (`fieldPlaceholder`), trocadas pelo valor real via `fieldValues`;
 *   - as quatro seções padrão de um laudo (objetivo / histórico / exames /
 *     conclusão).
 *
 * Os valores das pílulas vão no mapa `fieldValues` (mesma lógica do
 * `laudoExport.ts::resolveFieldValuesForExport`): só campos NÃO-vazios entram;
 * os vazios mantêm a pílula `{campo}` no `.docx` pro perito preencher na mão.
 */

import type { JSONContent } from "@tiptap/core";
import {
  DEFAULT_PAGE_MARGINS,
  LAUDO_FIELDS,
  findInstitutionalTemplate,
  resolveAllFields,
  seedHeaderContentFromInstitutionalTemplate,
} from "../document-engine";

export interface StarterEnvelopeOptions {
  /** Id do template institucional escolhido (vai pra `layout.institutional_template`). */
  templateId: string;
  /** Metadados do laudo (`numero_laudo`, `setor`, …). Override local dos campos. */
  metadata: Record<string, unknown>;
  /** Ocorrência ativa — fonte de verdade dos campos do caso (ou `null`). */
  occurrence: Record<string, unknown> | null;
}

export interface StarterEnvelopeResult {
  /** Envelope estilo `.sicrodoc` (sem coerção — JSON puro pro backend). */
  envelope: {
    content: JSONContent;
    layout: Record<string, unknown>;
    header: { enabled: boolean; content: unknown };
    footer?: { enabled: boolean; content: unknown };
  };
  /** Mapa {campo: valor} já resolvido (só não-vazios). */
  fieldValues: Record<string, string>;
}

/** Parágrafo "Rótulo: {campo}" — texto fixo + pílula de campo automático. */
function identificationLine(label: string, field: string): JSONContent {
  return {
    type: "paragraph",
    content: [
      { type: "text", marks: [{ type: "bold" }], text: `${label}: ` },
      { type: "fieldPlaceholder", attrs: { field } },
    ],
  };
}

/** Título de seção (heading nível 1). */
function sectionHeading(text: string): JSONContent {
  return {
    type: "heading",
    attrs: { level: 1 },
    content: [{ type: "text", text }],
  };
}

/** Parágrafo vazio (espaço pro perito escrever a seção no Word). */
function emptyParagraph(): JSONContent {
  return { type: "paragraph" };
}

/**
 * Monta o esqueleto do `content` (ProseMirror doc): bloco de identificação
 * (pílulas de campo) + as 4 seções padrão.
 */
function buildSkeletonContent(): JSONContent {
  return {
    type: "doc",
    content: [
      identificationLine("Laudo nº", "numero_laudo"),
      identificationLine("BO nº", "numero_bo"),
      identificationLine("Tipo de exame", "tipo_exame"),
      identificationLine("Data da perícia", "data_pericia"),
      identificationLine("Local da perícia", "local_pericia"),
      identificationLine("Perito", "nome_perito"),
      emptyParagraph(),
      sectionHeading("1 – OBJETIVO"),
      emptyParagraph(),
      sectionHeading("2 – HISTÓRICO"),
      emptyParagraph(),
      sectionHeading("3 – DOS EXAMES"),
      emptyParagraph(),
      sectionHeading("4 – CONCLUSÃO"),
      emptyParagraph(),
    ],
  };
}

/**
 * Resolve TODOS os campos do catálogo e mantém só os NÃO-vazios — espelha
 * `laudoExport.ts::resolveFieldValuesForExport`. Campos vazios ficam de fora
 * pro walker manter a pílula `{campo}` no `.docx`.
 */
function resolveStarterFieldValues(
  metadata: Record<string, unknown>,
  occurrence: Record<string, unknown> | null,
): Record<string, string> {
  const resolved = resolveAllFields({ metadata, occurrence }, LAUDO_FIELDS);
  const out: Record<string, string> = {};
  for (const [key, value] of resolved) {
    const v = value.trim();
    if (v) out[key] = v;
  }
  return out;
}

/**
 * Constrói o envelope inicial + o mapa de valores de campo de um laudo novo.
 */
export function buildStarterEnvelope(
  opts: StarterEnvelopeOptions,
): StarterEnvelopeResult {
  const { templateId, metadata, occurrence } = opts;
  const template = findInstitutionalTemplate(templateId);

  const headerContent = seedHeaderContentFromInstitutionalTemplate(
    template,
    metadata,
    occurrence,
  );

  const layout: Record<string, unknown> = {
    institutional_template: templateId,
    page_size: "A4",
    orientation: "portrait",
    page: { margins: { ...DEFAULT_PAGE_MARGINS } },
  };

  return {
    envelope: {
      content: buildSkeletonContent(),
      layout,
      header: { enabled: true, content: headerContent },
    },
    fieldValues: resolveStarterFieldValues(metadata, occurrence),
  };
}
