/**
 * `.sicrodoc` envelope schema.
 *
 * The envelope wraps the TipTap/ProseMirror document JSON (`content`) with
 * SICRO metadata. Versioning is explicit (`schema_version`) so future
 * migrations can be applied at read time without breaking existing files.
 *
 * The TipTap document itself is treated as opaque here — its node set is
 * defined in `./nodes/index.ts`.
 */

import type { JSONContent } from "@tiptap/core";

// F8 — Schema bump 1.0.0 → 1.1.0. Adições NÃO-quebrantes:
//   - `comments` (lista de comentários)
//   - `snapshots` (rolling buffer de revisões)
//   - `status` (rascunho | em_revisao | final)
//   - `finalization` (selo digital quando status === final)
// N — Schema bump 1.1.0 → 1.2.0. Adições NÃO-quebrantes:
//   - `header` (cabeçalho Word-style — região editável separada do body)
//   - `layout.header_height_cm` (altura configurável)
// Docs antigos (1.0.0 / 1.1.0) continuam carregando — campos faltantes
// recebem defaults vazios no `coerceSicroDoc`. Doc legado SEM `header`
// nem `institutional_template` abre com header desabilitado.
export const SCHEMA_VERSION = "1.2.0";

/** Altura padrão do cabeçalho em centímetros (Word default ≈ 1.25cm, mas
 *  laudos institucionais geralmente exigem ~2.5cm pra caber a banda PCA). */
export const DEFAULT_HEADER_HEIGHT_CM = 2.5;

/** Mínimo/máximo do controle de altura. 0cm desliga (mas usa `enabled`). */
export const HEADER_HEIGHT_MIN_CM = 0;
/** Pós-laudo S — Subido de 6 → 10cm pra acomodar cabeçalhos
 *  institucionais maiores (marca + sub-marca + 3-4 linhas de texto). */
export const HEADER_HEIGHT_MAX_CM = 10;

/** W — Rodapé (footer): mesma mecânica do cabeçalho, ancorado na base da
 *  página. Default 2cm (cabe o brasão da PC + 1-2 linhas). */
export const DEFAULT_FOOTER_HEIGHT_CM = 2.0;
export const FOOTER_HEIGHT_MIN_CM = 0;
export const FOOTER_HEIGHT_MAX_CM = 10;

/** Margens de página em string (cm, mm ou pt). MVP 2.1 usa sempre cm. */
export interface SicroDocPageMargins {
  top: string;
  right: string;
  bottom: string;
  left: string;
}

export interface SicroDocPage {
  /** Override das margens definidas pelo institutional_template. Quando
   *  ausente, o template manda. */
  margins?: SicroDocPageMargins;
}

/** Alinhamento horizontal da numeração de página no cabeçalho do PDF. */
export type PageNumberAlign = "left" | "center" | "right";

/**
 * Configuração da numeração de página ("Folha X de Y") impressa no cabeçalho
 * do PDF. O número vem do contador NATIVO do Chromium (placeholders
 * pageNumber/totalPages) — confiável e por página. O perito controla aqui o
 * texto e a aparência.
 *
 * `format` aceita os tokens `{n}` (página atual) e `{total}` (total de
 * páginas). Ex.: "Folha {n} de {total}", "Página {n}/{total}", "{n}".
 */
export interface SicroDocPageNumber {
  format: string;
  font_family: string;
  /** Tamanho em pt (unidade do laudo). */
  size_pt: number;
  /** Cor em hex (#rrggbb). */
  color: string;
  align: PageNumberAlign;
}

export interface SicroDocLayout {
  page_size: "A4";
  orientation: "portrait" | "landscape";
  /** Optional id of an institutional template (header/footer/side-mark).
   *  N — Mantido para resolver `findInstitutionalTemplate` e migrar docs
   *  legados na primeira abertura. NÃO usado mais para renderizar header
   *  hardcoded (agora vive em `document.header.content`). */
  institutional_template?: string;
  /** Override de margens e outras propriedades de página (MVP 2 ajuste). */
  page?: SicroDocPage;
  /** N — Altura do cabeçalho em cm. Aplica-se quando `document.header.enabled`.
   *  Default `DEFAULT_HEADER_HEIGHT_CM`. Limites: HEADER_HEIGHT_MIN_CM ..
   *  HEADER_HEIGHT_MAX_CM. */
  header_height_cm?: number;
  /** W — Altura do rodapé em cm. Aplica-se quando `document.footer.enabled`.
   *  Default `DEFAULT_FOOTER_HEIGHT_CM`. Limites: FOOTER_HEIGHT_MIN_CM ..
   *  FOOTER_HEIGHT_MAX_CM. */
  footer_height_cm?: number;
  /** Numeração de página "Folha X de Y" no cabeçalho do PDF. Aditivo: docs
   *  sem isto usam `DEFAULT_PAGE_NUMBER`. */
  page_number?: SicroDocPageNumber;
}

/** Valores padrão da numeração de página (laudo institucional). */
export const DEFAULT_PAGE_NUMBER: SicroDocPageNumber = {
  format: "Página {n} de {total}",
  font_family: "Arial",
  size_pt: 10,
  color: "#000000",
  align: "center",
};

/** Fontes oferecidas no seletor de numeração (compatíveis com o laudo/PDF). */
export const PAGE_NUMBER_FONTS: readonly string[] = [
  "Arial",
  "Times New Roman",
  "Calibri",
  "Cambria",
  "Georgia",
  "Verdana",
];

/**
 * Resolve a configuração de numeração efetiva: mescla o que o laudo definiu
 * sobre o default (campos ausentes herdam o padrão). Sanitiza valores fora de
 * faixa para não quebrar o template do PDF.
 */
export function resolvePageNumber(doc: SicroDoc | null): SicroDocPageNumber {
  const raw = doc?.layout?.page_number;
  if (!raw) return { ...DEFAULT_PAGE_NUMBER };
  const align: PageNumberAlign =
    raw.align === "left" || raw.align === "right" || raw.align === "center"
      ? raw.align
      : DEFAULT_PAGE_NUMBER.align;
  const size =
    typeof raw.size_pt === "number" && raw.size_pt >= 5 && raw.size_pt <= 24
      ? raw.size_pt
      : DEFAULT_PAGE_NUMBER.size_pt;
  return {
    format:
      typeof raw.format === "string" && raw.format.trim() !== ""
        ? raw.format
        : DEFAULT_PAGE_NUMBER.format,
    font_family:
      typeof raw.font_family === "string" && raw.font_family.trim() !== ""
        ? raw.font_family
        : DEFAULT_PAGE_NUMBER.font_family,
    size_pt: size,
    color: /^#[0-9a-fA-F]{6}$/.test(raw.color ?? "")
      ? raw.color
      : DEFAULT_PAGE_NUMBER.color,
    align,
  };
}

// ---------------------------------------------------------------------------
// N — Cabeçalho Word-style (região editável separada do corpo).

/**
 * Cabeçalho do documento — vive na própria envelope (`document.header`)
 * e é renderizado replicado em todas as páginas. NÃO é um parágrafo do
 * body; é uma região independente com seu próprio editor TipTap quando
 * em modo de edição (`editingRegion === "header"`).
 *
 * `content` é um ProseMirror doc completo (tipo "doc") — assim podemos
 * usar a mesma máquina TipTap (com um subset reduzido de nodes/marks).
 *
 * `enabled === false` ⇒ não renderiza, não consome altura na paginação,
 * mas mantém `content` salvo (para reativar sem perder edição).
 */
export interface SicroDocHeader {
  /** ProseMirror/TipTap document do cabeçalho. */
  content: JSONContent;
  /** Quando false, header desliga e não consome altura na paginação. */
  enabled: boolean;
}

/**
 * Conteúdo vazio inicial do cabeçalho (single empty paragraph). Mantém
 * compatibilidade com o schema ProseMirror padrão.
 */
export function emptyHeaderContent(): JSONContent {
  return {
    type: "doc",
    content: [{ type: "paragraph" }],
  };
}

/**
 * W — Rodapé do documento. Estrutura idêntica ao cabeçalho (`content` +
 * `enabled`), só muda a âncora (base da página) na renderização/paginação.
 * Traz, por exemplo, o brasão da Polícia Científica que no Word fica no
 * rodapé. Aditivo: docs sem `footer` recebem default desligado.
 */
export interface SicroDocFooter {
  /** ProseMirror/TipTap document do rodapé. */
  content: JSONContent;
  /** Quando false, rodapé desliga e não consome altura na paginação. */
  enabled: boolean;
}

/** Conteúdo vazio inicial do rodapé (single empty paragraph). */
export function emptyFooterContent(): JSONContent {
  return {
    type: "doc",
    content: [{ type: "paragraph" }],
  };
}

export interface SicroDocMetadata {
  numero_laudo?: string;
  setor?: string;
  tipo_pericia?: string;
  municipio?: string;
  /** Free-form additional metadata; preserved on write. */
  [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// F8 — Comments + snapshots + status.

/** Status de finalização do laudo. */
export type SicroDocStatus = "rascunho" | "em_revisao" | "final";

/** Um comentário ancorado a um intervalo de texto. O id do comentário
 *  vai no `commentMark.id` do conteúdo TipTap. */
export interface SicroDocComment {
  id: string;
  author: string;
  /** ISO 8601 timestamp. */
  created_at: string;
  /** Texto do comentário (markdown leve, sem renderização rica). */
  body: string;
  /** Quando true, o comentário foi resolvido (cinza, escondido por padrão). */
  resolved: boolean;
  /** Lista opcional de respostas em thread. */
  replies?: SicroDocCommentReply[];
}

export interface SicroDocCommentReply {
  id: string;
  author: string;
  created_at: string;
  body: string;
}

/** Snapshot histórico do documento — buffer rolling de 20. */
export interface SicroDocSnapshot {
  id: string;
  /** ISO 8601 timestamp. */
  created_at: string;
  /** Autor da snapshot (perito que rodou o save). */
  author: string;
  /** Etiqueta opcional ("antes da revisão de Joana", "checkpoint pré-defesa"). */
  label?: string;
  /** Conteúdo TipTap completo no momento da snapshot. */
  content: JSONContent;
  /** Quantos parágrafos / palavras / chars havia (cache pra UI). */
  stats?: {
    words: number;
    paragraphs: number;
  };
}

/**
 * F12.11 / H / I — Assinatura digital do laudo.
 *
 * Cinco fluxos suportados:
 *   - "sigdocs" — Assinatura via SIGDOCS, o sistema de fluxo de
 *                 documentos do Estado do Amapá. Perito exporta PDF,
 *                 abre SIGDOCS (janela secundária ou split), sobe o
 *                 PDF para a pasta institucional dele, assina lá, e
 *                 importa o PDF de volta. SIGDOCS continua sendo o
 *                 sistema de transferência entre perito/secretaria/
 *                 delegacia — esta integração só mantém o fluxo já
 *                 estabelecido no Estado.
 *   - "gov_br"  — Assinatura via portal gov.br (Lei 14.063/2020
 *                 "avançada"). Perito exporta PDF, sobe em
 *                 `assinador.iti.gov.br`, baixa assinado.
 *   - "A1"      — Certificado ICP-Brasil em arquivo (.pfx). NÃO
 *                 implementado em runtime ainda; UI registrada.
 *   - "A3"      — Certificado ICP-Brasil em token/cartão. NÃO
 *                 implementado em runtime ainda; UI registrada.
 *   - "mock"    — Demonstração para o alpha (sem hardware/cert real).
 *
 * Quando `type === "gov_br"` ou `"sigdocs"`, o backend grava o PDF
 * assinado em `laudos/<laudo-id>/assinados/<filename>.pdf` e popula
 * o caminho correspondente (`gov_br_signed_pdf_path` ou
 * `sigdocs_signed_pdf_path`).
 */
export interface SicroDocSignature {
  /** Tipo do fluxo de assinatura. */
  type: "A1" | "A3" | "gov_br" | "sigdocs" | "mock";
  /** Nome do titular como aparece no certificado / no login gov.br. */
  signer_name: string;
  /** CPF mascarado ou outro identificador do titular. */
  signer_id?: string;
  /** Autoridade certificadora emissora (ex: "AC SOLUTI", "ITI - gov.br"). */
  issuer?: string;
  /** Validade do certificado (ISO 8601). Não aplicável a gov.br. */
  valid_until?: string;
  /** ISO 8601 do momento da assinatura. */
  signed_at: string;
  /** SHA-256 do hash assinado (deve bater com finalization.content_hash). */
  signed_hash: string;
  /** Stub do blob da assinatura (base64). No mock fica vazio; em A1/A3
   *  real, é o output do PKCS#7 / CMS. */
  signature_blob?: string;

  // --- H — Campos específicos do fluxo gov.br ---
  /** Caminho RELATIVO ao workspace para o PDF assinado importado de
   *  volta do gov.br. Ex: `laudos/<id>/assinados/laudo-abc.pdf`. */
  gov_br_signed_pdf_path?: string;
  /** URL pública do validador do ITI (`https://validar.iti.gov.br`).
   *  Pode ser apontada pra qualquer dispositivo conferir o PDF. */
  gov_br_verification_url?: string;
  /** SHA-256 do PDF assinado (diferente do `signed_hash` que é do
   *  conteúdo do laudo). Permite detectar tampering do arquivo
   *  assinado depois da importação. */
  gov_br_signed_pdf_hash?: string;
  /** Tamanho do PDF assinado em bytes. */
  gov_br_signed_pdf_size?: number;

  // --- I — Campos específicos do fluxo SIGDOCS ---
  /** Caminho RELATIVO ao workspace para o PDF assinado importado de
   *  volta do SIGDOCS. Ex: `laudos/<id>/assinados/laudo-abc.pdf`. */
  sigdocs_signed_pdf_path?: string;
  /** SHA-256 do PDF assinado pelo SIGDOCS. */
  sigdocs_signed_pdf_hash?: string;
  /** Tamanho do PDF assinado em bytes. */
  sigdocs_signed_pdf_size?: number;
  /** Pasta institucional do perito no SIGDOCS onde o PDF está
   *  arquivado (texto livre — o perito anota qual setor/destino). */
  sigdocs_folder?: string;
  /** Protocolo / número de tramitação do SIGDOCS, quando o perito
   *  informar. Útil para rastreamento institucional. */
  sigdocs_protocol?: string;
}

/** Selo de finalização quando o status muda para "final". */
export interface SicroDocFinalization {
  /** ISO 8601 timestamp do clique em "Finalizar". */
  finalized_at: string;
  /** Quem finalizou. */
  finalized_by: string;
  /** SHA-256 do JSON do documento no momento da finalização (string hex). */
  content_hash: string;
  /** Notas opcionais do perito ao finalizar (ex: "revisado por X em Y"). */
  notes?: string;
  /** F12.11 — Assinatura digital opcional (mock A3/A1). */
  signature?: SicroDocSignature;
}

export interface SicroDoc {
  schema_version: string;
  document_id: string;
  occurrence_id: string;
  type: "laudo";
  title: string;
  template_id: string;
  created_at: string;
  updated_at: string;
  metadata: SicroDocMetadata;
  layout: SicroDocLayout;
  /** ProseMirror/TipTap document (BODY do laudo — N: já não inclui header). */
  content: JSONContent;
  /** N — Cabeçalho Word-style. Aditivo: ausente em docs legados, gerado com
   *  defaults seguros (enabled: false, content vazio) no `coerceSicroDoc`. */
  header?: SicroDocHeader;
  /** W — Rodapé Word-style. Aditivo: ausente em docs legados → default
   *  desligado/vazio no `coerceSicroDoc`. */
  footer?: SicroDocFooter;
  // F8 — Adições não-quebrantes (todas opcionais).
  /** Status do laudo (default "rascunho"). */
  status?: SicroDocStatus;
  /** Comentários ancorados a marcas `commentMark`. */
  comments?: SicroDocComment[];
  /** Snapshots históricos — rolling buffer de até 20. */
  snapshots?: SicroDocSnapshot[];
  /** Selo digital criado quando status muda para "final". */
  finalization?: SicroDocFinalization;
}

/**
 * Produce an empty document — single empty paragraph, valid ProseMirror.
 * Used as the initial content for freshly-created laudos and as the fallback
 * for malformed payloads (renders cleanly instead of crashing the editor).
 */
export function emptyDocContent(): JSONContent {
  return {
    type: "doc",
    content: [{ type: "paragraph" }],
  };
}

/**
 * Read-time sanity check. Returns the same envelope if it looks well-formed,
 * or a coerced one with a safe `content` if it does not. We log to console
 * so misshapes are visible during the spike — proper validation will live
 * in a future spike's import pipeline.
 */
export function coerceSicroDoc(raw: unknown): SicroDoc {
  if (!raw || typeof raw !== "object") {
    throw new Error("invalid sicrodoc: not an object");
  }
  const obj = raw as Partial<SicroDoc> & Record<string, unknown>;

  // Minimum-viable envelope: complain only if the absolutely-required fields
  // are missing. Everything else gets a sensible default.
  if (typeof obj.document_id !== "string" || typeof obj.occurrence_id !== "string") {
    throw new Error("invalid sicrodoc: missing document_id or occurrence_id");
  }

  // N — Resolve layout (com saneamento de header_height_cm).
  const rawLayout = (obj.layout as SicroDocLayout) ?? {
    page_size: "A4" as const,
    orientation: "portrait" as const,
  };
  const layout: SicroDocLayout = {
    ...rawLayout,
    header_height_cm: clampHeaderHeightCm(rawLayout.header_height_cm),
    footer_height_cm: clampFooterHeightCm(rawLayout.footer_height_cm),
  };

  // N — Resolve header.
  //   Caso 1: doc já tem `header` salvo → usa ele (com saneamento).
  //   Caso 2: legado SEM header — começa desligado, content vazio.
  //           A migração que copia `institutional_template` para
  //           `header.content` acontece no `EditorPage` na primeira
  //           abertura (N12), não aqui — coerceSicroDoc fica puro.
  const header: SicroDocHeader = coerceHeader(obj.header);
  // W — Resolve footer (mesma lógica do header; default desligado/vazio).
  const footer: SicroDocFooter = coerceFooter(obj.footer);

  return {
    schema_version: (obj.schema_version as string) ?? SCHEMA_VERSION,
    document_id: obj.document_id,
    occurrence_id: obj.occurrence_id,
    type: "laudo",
    title: (obj.title as string) ?? "Laudo sem título",
    template_id: (obj.template_id as string) ?? "documento_em_branco",
    created_at: (obj.created_at as string) ?? new Date().toISOString(),
    updated_at: (obj.updated_at as string) ?? new Date().toISOString(),
    metadata: (obj.metadata as SicroDocMetadata) ?? {},
    layout,
    content: sanitizeFloatingFigures(
      isJsonContent(obj.content) ? (obj.content as JSONContent) : emptyDocContent(),
    ),
    header,
    footer,
    // F8 — campos opcionais. `undefined` evita engordar `.sicrodoc` antigo.
    status: (obj.status as SicroDocStatus | undefined) ?? undefined,
    comments: Array.isArray(obj.comments)
      ? (obj.comments as SicroDocComment[])
      : undefined,
    snapshots: Array.isArray(obj.snapshots)
      ? (obj.snapshots as SicroDocSnapshot[])
      : undefined,
    finalization: (obj.finalization as SicroDocFinalization | undefined) ?? undefined,
  };
}

/**
 * P25 — Limite (em cm) acima do qual um `wrap_y_cm` de figura flutuante é
 * considerado "frame antigo" (global `.editorWrap`, que abrangia TODAS as
 * páginas). Uma página A4 tem 29.7cm; usamos 28 como margem de segurança.
 */
export const FLOATING_FIGURE_REBASE_THRESHOLD_CM = 28;
/** P25 — Passo de rebase (≈ altura útil de uma página A4). O `wrap_y_cm`
 *  antigo (offset global) é reduzido módulo este passo, trazendo a foto pra
 *  perto da âncora pro perito reposicionar. */
const FLOATING_FIGURE_REBASE_STRIDE_CM = 29.7;

/**
 * P25 — Migração defensiva das fotos flutuantes.
 *
 * Antes da ancoragem por wrapper, fotos flutuantes COLOCADAS NO SICRO
 * guardavam `wrap_y_cm` no frame global do `.editorWrap` (que abrangia
 * todas as páginas) — ex.: ~35cm pra uma foto da página 2. No frame novo
 * (offset a partir da âncora da figura no fluxo) esse número joga a foto
 * pra muito longe. Aqui, fotos com `wrap_mode != inline` e
 * `|wrap_y_cm| > FLOATING_FIGURE_REBASE_THRESHOLD_CM` são rebaseadas
 * (módulo o stride de uma página) pra reaparecerem perto da âncora.
 *
 * Fotos IMPORTADAS do Word (Y pequeno, < 28cm) ficam INTOCADAS — já estão
 * no frame certo. Idempotente: rodar de novo não corrompe (valores já
 * pequenos não disparam o rebase).
 *
 * Retorna uma cópia rasa com a árvore reescrita só onde necessário; se
 * nada mudar, retorna a referência original.
 */
export function sanitizeFloatingFigures(content: JSONContent): JSONContent {
  let changed = false;

  const walk = (node: JSONContent): JSONContent => {
    let nextNode = node;

    if (node.type === "figure" && node.attrs) {
      const attrs = node.attrs as Record<string, unknown>;
      const wrapMode = (attrs.wrap_mode as string | undefined) ?? "inline";
      const rawY = attrs.wrap_y_cm;
      const y = typeof rawY === "number" ? rawY : Number(rawY);
      if (
        wrapMode !== "inline" &&
        Number.isFinite(y) &&
        Math.abs(y) > FLOATING_FIGURE_REBASE_THRESHOLD_CM
      ) {
        // Rebase módulo uma página, preservando o sinal. Resultado fica em
        // (-29.7, 29.7) — perto da âncora. Idempotente: |y| <= 28 não entra.
        const rebased = Number(
          (y % FLOATING_FIGURE_REBASE_STRIDE_CM).toFixed(2),
        );
        nextNode = { ...node, attrs: { ...attrs, wrap_y_cm: rebased } };
        changed = true;
      }
    }

    const children = nextNode.content;
    if (Array.isArray(children)) {
      let childrenChanged = false;
      const nextChildren = children.map((c) => {
        const nc = walk(c);
        if (nc !== c) childrenChanged = true;
        return nc;
      });
      if (childrenChanged) {
        nextNode = { ...nextNode, content: nextChildren };
      }
    }

    return nextNode;
  };

  const result = walk(content);
  return changed ? result : content;
}

/** Sanea altura de cabeçalho. NaN/undefined → default. Fora dos limites → clamp. */
export function clampHeaderHeightCm(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return DEFAULT_HEADER_HEIGHT_CM;
  }
  if (value < HEADER_HEIGHT_MIN_CM) return HEADER_HEIGHT_MIN_CM;
  if (value > HEADER_HEIGHT_MAX_CM) return HEADER_HEIGHT_MAX_CM;
  return value;
}

/** Sanea cabeçalho — aceita parcial, completa com defaults. */
function coerceHeader(raw: unknown): SicroDocHeader {
  if (!raw || typeof raw !== "object") {
    return { content: emptyHeaderContent(), enabled: false };
  }
  const h = raw as Partial<SicroDocHeader>;
  return {
    content: isJsonContent(h.content) ? (h.content as JSONContent) : emptyHeaderContent(),
    enabled: typeof h.enabled === "boolean" ? h.enabled : false,
  };
}

/** W — Sanea altura de rodapé. NaN/undefined → default; fora dos limites → clamp. */
export function clampFooterHeightCm(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return DEFAULT_FOOTER_HEIGHT_CM;
  }
  if (value < FOOTER_HEIGHT_MIN_CM) return FOOTER_HEIGHT_MIN_CM;
  if (value > FOOTER_HEIGHT_MAX_CM) return FOOTER_HEIGHT_MAX_CM;
  return value;
}

/** W — Sanea rodapé — aceita parcial, completa com defaults. */
function coerceFooter(raw: unknown): SicroDocFooter {
  if (!raw || typeof raw !== "object") {
    return { content: emptyFooterContent(), enabled: false };
  }
  const f = raw as Partial<SicroDocFooter>;
  return {
    content: isJsonContent(f.content) ? (f.content as JSONContent) : emptyFooterContent(),
    enabled: typeof f.enabled === "boolean" ? f.enabled : false,
  };
}

function isJsonContent(value: unknown): value is JSONContent {
  return Boolean(
    value &&
      typeof value === "object" &&
      "type" in (value as Record<string, unknown>) &&
      typeof (value as Record<string, unknown>).type === "string",
  );
}
