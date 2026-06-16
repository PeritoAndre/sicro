/**
 * Cabeçalhos oficiais — biblioteca reutilizável (criador de cabeçalho do Laudo).
 *
 * O padrão institucional ("o nosso") é definido AQUI em código (BUILTIN), então
 * está sempre disponível e, quando trocarmos pelo modelo oficial exato (+ os
 * brasões corretos), atualiza pra todos de uma vez. Os demais cabeçalhos (de
 * outra unidade ou até outro órgão) o perito salva e ficam em
 * `app-settings.json` (campo `header_templates`), reutilizáveis entre laudos.
 *
 * ⚠️ PROVISÓRIO (§13): o conteúdo do BUILTIN abaixo é um placeholder honesto —
 * só texto, SEM os brasões — até o perito enviar o cabeçalho-modelo exato e os
 * arquivos de logo oficiais. Trocar `provisionalContent()` quando chegarem.
 */

import type { HeaderTemplate, HeaderTemplateContent } from "@domain/app_settings";
import { commands } from "@core/commands";

export const BUILTIN_HEADER_TEMPLATE_ID = "builtin-pcap";

/** Parágrafo centralizado (com negrito/tamanho opcionais). */
function line(text: string, opts?: { bold?: boolean; size?: string }): unknown {
  const marks: unknown[] = [];
  if (opts?.bold) marks.push({ type: "bold" });
  if (opts?.size) {
    marks.push({ type: "textStyle", attrs: { fontSize: opts.size } });
  }
  return {
    type: "paragraph",
    attrs: { textAlign: "center" },
    content: [
      { type: "text", ...(marks.length ? { marks } : {}), text },
    ],
  };
}

/** Conteúdo PROVISÓRIO do cabeçalho institucional padrão (sem brasão ainda). */
function provisionalContent(): HeaderTemplateContent {
  return {
    type: "doc",
    content: [
      line("GOVERNO DO ESTADO DO AMAPÁ", { bold: true, size: "12pt" }),
      line("POLÍCIA CIENTÍFICA DO AMAPÁ", { bold: true, size: "12pt" }),
      line("Departamento de Criminalística", { size: "10pt" }),
    ],
  };
}

/** Padrão institucional definido em código — sempre disponível na lista. */
export function builtinHeaderTemplate(): HeaderTemplate {
  return {
    id: BUILTIN_HEADER_TEMPLATE_ID,
    name: "Departamento de Criminalística (PCI/AP)",
    content: provisionalContent(),
    header_height_cm: 2.8,
    created_at: "2026-01-01T00:00:00.000Z",
  };
}

export function isBuiltinTemplate(t: HeaderTemplate): boolean {
  return t.id === BUILTIN_HEADER_TEMPLATE_ID;
}

/** Coerção defensiva de um cabeçalho lido do disco. `null` se inválido. */
export function coerceHeaderTemplate(raw: unknown): HeaderTemplate | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as Record<string, unknown>;
  const id = typeof r.id === "string" ? r.id : "";
  const name = typeof r.name === "string" ? r.name : "";
  const content = r.content;
  if (!id || !name || !content || typeof content !== "object") return null;
  return {
    id,
    name,
    content: content as HeaderTemplateContent,
    header_height_cm:
      typeof r.header_height_cm === "number" ? r.header_height_cm : undefined,
    created_at: typeof r.created_at === "string" ? r.created_at : "",
  };
}

/** Ordena: o padrão (builtin) primeiro, depois por nome. */
function sortHeaderTemplates(list: HeaderTemplate[]): HeaderTemplate[] {
  return [...list].sort((a, b) => {
    if (isBuiltinTemplate(a)) return -1;
    if (isBuiltinTemplate(b)) return 1;
    return a.name.localeCompare(b.name, "pt-BR");
  });
}

/**
 * Carrega os cabeçalhos da PASTA dedicada (`<app_config_dir>/cabecalhos/`),
 * garantindo, de forma idempotente:
 *  - MIGRAÇÃO one-shot: se a pasta está vazia e há `legacy` (do antigo
 *    `app-settings.json → header_templates`), move cada um pra pasta;
 *  - o "de fábrica" (builtin) materializado como arquivo — editável/atualizável.
 *
 * Devolve a lista pronta (builtin primeiro). Best-effort: num ambiente sem
 * backend, cai no builtin em memória.
 */
export async function ensureAndLoadHeaderTemplates(
  legacy?: HeaderTemplate[] | null,
): Promise<HeaderTemplate[]> {
  const read = async (): Promise<HeaderTemplate[]> =>
    (await commands.listHeaderTemplates())
      .map(coerceHeaderTemplate)
      .filter((t): t is HeaderTemplate => t !== null);

  let list: HeaderTemplate[];
  try {
    list = await read();
  } catch {
    return [builtinHeaderTemplate()];
  }

  // Migração one-shot: pasta vazia + havia salvos no app-settings antigo.
  if (list.length === 0 && legacy && legacy.length > 0) {
    for (const t of legacy) {
      const ct = coerceHeaderTemplate(t);
      if (ct && !isBuiltinTemplate(ct)) {
        try {
          await commands.saveHeaderTemplate(ct);
        } catch {
          /* best-effort */
        }
      }
    }
  }

  // Builtin: SEMPRE sincroniza com a versão do código. Se ainda não existe,
  // materializa. Se existe mas o conteúdo do disco está desatualizado (nome,
  // texto, etc. — porque o "de fábrica" foi atualizado num novo release),
  // sobrescreve. Quem quer editar livremente deve usar "Salvar atual" e criar
  // uma cópia com outro nome — o builtin é sempre canônico/oficial.
  const builtin = builtinHeaderTemplate();
  const onDisk = list.find(isBuiltinTemplate);
  const isStale =
    !onDisk ||
    onDisk.name !== builtin.name ||
    onDisk.header_height_cm !== builtin.header_height_cm ||
    JSON.stringify(onDisk.content) !== JSON.stringify(builtin.content);
  if (isStale) {
    try {
      await commands.saveHeaderTemplate(builtin);
    } catch {
      /* best-effort */
    }
  }

  try {
    list = await read();
  } catch {
    /* mantém a lista anterior */
  }
  if (list.length === 0) list = [builtin];
  return sortHeaderTemplates(list);
}

/** Id estável para um novo template salvo. */
export function newTemplateId(): string {
  try {
    return crypto.randomUUID();
  } catch {
    return `tpl-${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
  }
}

/** Verdadeiro quando o doc de cabeçalho está vazio (só 1 parágrafo sem texto). */
export function isHeaderContentEmpty(
  content: HeaderTemplateContent | undefined | null,
): boolean {
  const blocks = content?.content;
  if (!Array.isArray(blocks) || blocks.length === 0) return true;
  const hasText = JSON.stringify(blocks).includes('"text"');
  return !hasText;
}
