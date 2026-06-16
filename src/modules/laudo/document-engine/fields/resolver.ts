/**
 * resolveFieldValue — resolve o valor de um `{{campo}}` em tempo de
 * renderização, dado o contexto do laudo (occurrence + metadata).
 *
 * F5 — Funciona como single source of truth para:
 *   - o renderer HTML (mostra o valor no PDF/HTML);
 *   - o painel de campos (mostra preview "valor: ____");
 *   - a validação (decide se o campo obrigatório está preenchido);
 *   - eventualmente, o walker DOCX (mesma lógica no Rust ou via render).
 *
 * Função pura, sem efeitos colaterais. Aceita objetos `unknown` para os
 * dados externos — converte para string segura ou devolve null quando
 * o campo não tem valor.
 */

import { findField, type LaudoFieldDefinition } from "./catalog";

export interface FieldResolveContext {
  /** Metadados do laudo (`doc.metadata`). */
  metadata?: Record<string, unknown> | null;
  /** Ocorrência ativa (objeto retornado por `useWorkspaceStore`). */
  occurrence?: Record<string, unknown> | null;
  /**
   * Data "agora" usada para `system.data_hoje` e `data_hora_agora`.
   * Default `new Date()`. Aceita override para testes determinísticos.
   */
  now?: Date;
}

/**
 * Resolve o valor de uma `key` de campo conhecido.
 *
 *   - Retorna a string formatada quando o valor existe.
 *   - Retorna `null` quando o campo não está no catálogo.
 *   - Retorna `""` (string vazia) quando o campo está no catálogo mas
 *     a fonte não trouxe valor — diferencia "campo desconhecido" de
 *     "campo conhecido mas vazio".
 */
export function resolveFieldValue(
  key: string,
  ctx: FieldResolveContext,
): string | null {
  const def = findField(key);
  if (!def) return null;
  return resolveDefinition(def, ctx);
}

/**
 * Variante que recebe a definição direto — útil para iteração no painel.
 *
 * Aplica o OVERRIDE LOCAL do laudo: `metadata[def.key]`, quando preenchido,
 * vence a fonte original. É isso que o painel "Campos" edita — o perito pode
 * mudar o valor de qualquer pílula só naquele laudo, sem tocar na ocorrência
 * (fonte de verdade do caso). Campos computados (data atual, contadores de
 * página) ignoram o override.
 */
export function resolveDefinition(
  def: LaudoFieldDefinition,
  ctx: FieldResolveContext,
): string {
  const source = def.source;
  if (source.kind !== "system" && source.kind !== "page_counter") {
    const override = ctx.metadata?.[def.key];
    if (override != null) {
      const s = formatRaw(override);
      if (s.trim() !== "") return s;
    }
  }
  return resolveFromSource(def, ctx);
}

/**
 * Resolve estritamente a FONTE original do campo (occurrence/metadata/fixed/
 * system/contador), IGNORANDO o override local. O painel usa isto como
 * placeholder ("valor herdado do caso") quando não há override.
 */
export function resolveFromSource(
  def: LaudoFieldDefinition,
  ctx: FieldResolveContext,
): string {
  const source = def.source;
  if (source.kind === "fixed") {
    return source.value;
  }
  if (source.kind === "system") {
    const now = ctx.now ?? new Date();
    if (source.field === "data_hoje") return formatDateBR(now);
    if (source.field === "data_hora_agora") return formatDateTimeBR(now);
    return "";
  }
  if (source.kind === "page_counter") {
    // page/pages NÃO são resolvíveis estaticamente — cada página exporta um
    // valor diferente. O renderer HTML/PDF detecta esses campos e os
    // substitui por um <span> com CSS counter(page)/counter(pages). Aqui só
    // devolvemos string vazia (pro painel/validação) para não conflitar.
    return "";
  }
  const bag =
    source.kind === "metadata"
      ? (ctx.metadata ?? null)
      : source.kind === "occurrence"
        ? (ctx.occurrence ?? null)
        : null;
  if (!bag) return "";
  const raw = bag[source.field];
  return formatRaw(raw);
}

/**
 * Resolve TODOS os campos do catálogo de uma vez. Útil para o painel
 * e para a validação de obrigatórios. Devolve `Map<key, value>`.
 */
export function resolveAllFields(
  ctx: FieldResolveContext,
  fields: ReadonlyArray<LaudoFieldDefinition>,
): Map<string, string> {
  const out = new Map<string, string>();
  for (const def of fields) {
    out.set(def.key, resolveDefinition(def, ctx));
  }
  return out;
}

/**
 * Lista de campos obrigatórios SEM valor. Caller decide o que mostrar
 * (warning amarelo, badge no botão de exportar, etc.).
 */
export function findMissingRequiredFields(
  ctx: FieldResolveContext,
  fields: ReadonlyArray<LaudoFieldDefinition>,
): LaudoFieldDefinition[] {
  return fields.filter((def) => {
    if (def.required !== true) return false;
    return resolveDefinition(def, ctx).trim() === "";
  });
}

/**
 * Substitui as pílulas de campo (`<span data-field="KEY">…</span>`) num HTML
 * estático pelo VALOR resolvido — mesma semântica do `FieldNodeView` do editor:
 *   - valor não-vazio → mostra o valor (pílula "resolvida", verde);
 *   - campo vazio OU contador de página (`page`/`pages`, que só o export numera)
 *     → mantém `{KEY}` (pílula "pendente", laranja).
 * Pura (sem DOM). Usada pelos CLONES estáticos do cabeçalho/rodapé no editor,
 * pra que o que aparece fora do modo de edição seja igual ao valor real (antes
 * só a pílula viva, via NodeView, resolvia — o clone mostrava `{numero_laudo}`).
 */
export function resolveFieldSpansInHtml(
  html: string,
  ctx: FieldResolveContext,
): string {
  if (!html) return html;
  return html.replace(
    /<span\b[^>]*\bdata-field=["']([^"']+)["'][^>]*>[\s\S]*?<\/span>/gi,
    (_match, rawKey: string) => {
      const key = rawKey;
      const def = findField(key);
      const value = def ? resolveDefinition(def, ctx).trim() : "";
      const resolved = value.length > 0;
      const cls = resolved
        ? "sicro-field sicro-field-resolved"
        : "sicro-field sicro-field-placeholder";
      const inner = resolved ? escapeFieldHtml(value) : `{${escapeFieldHtml(key)}}`;
      return (
        `<span class="${cls}" data-field="${escapeFieldHtml(key)}" ` +
        `data-resolved="${resolved}" contenteditable="false">${inner}</span>`
      );
    },
  );
}

/** Escapa texto pra inserção segura como conteúdo/atributo HTML. */
function escapeFieldHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// ---------------------------------------------------------------------------
// Formatadores internos.

function formatRaw(raw: unknown): string {
  if (raw == null) return "";
  if (typeof raw === "string") return raw;
  if (typeof raw === "number" || typeof raw === "boolean") return String(raw);
  if (Array.isArray(raw)) {
    return raw
      .map((item) => formatRaw(item))
      .filter((s) => s.length > 0)
      .join("; ");
  }
  // Object: best-effort. Caller que queira controle fino formata na origem.
  try {
    return JSON.stringify(raw);
  } catch {
    return String(raw);
  }
}

function pad(n: number, w = 2): string {
  return String(n).padStart(w, "0");
}

function formatDateBR(d: Date): string {
  return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()}`;
}

function formatDateTimeBR(d: Date): string {
  return (
    `${formatDateBR(d)} ${pad(d.getHours())}:${pad(d.getMinutes())}`
  );
}
