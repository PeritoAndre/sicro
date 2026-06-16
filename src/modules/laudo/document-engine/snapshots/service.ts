/**
 * Snapshots service — buffer rolling de revisões históricas do laudo.
 *
 * F8 — Cada save manual (Ctrl+S) ou snapshot explícito empurra uma nova
 * entrada para `doc.snapshots[]`. O buffer máximo é 20 — entradas mais
 * antigas são descartadas (FIFO).
 *
 * Restaurar uma snapshot é apenas substituir `doc.content` pelo conteúdo
 * salvo — feito pelo caller, esta camada é pura.
 */

import type { JSONContent } from "@tiptap/core";
import type { SicroDocSnapshot } from "../schema";

export const MAX_SNAPSHOTS = 20;

function uuid(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return crypto.randomUUID();
  }
  return `snap-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e9).toString(36)}`;
}

/** Cria uma snapshot a partir do conteúdo atual do editor. */
export function createSnapshot(opts: {
  author: string;
  label?: string;
  content: JSONContent;
}): SicroDocSnapshot {
  return {
    id: uuid(),
    author: opts.author,
    label: opts.label,
    created_at: new Date().toISOString(),
    content: deepClone(opts.content),
    stats: computeStats(opts.content),
  };
}

/** Insere uma snapshot, respeitando o buffer máximo (FIFO descarta antigas). */
export function pushSnapshot(
  list: SicroDocSnapshot[] | undefined,
  snap: SicroDocSnapshot,
  maxSize = MAX_SNAPSHOTS,
): SicroDocSnapshot[] {
  const next = [snap, ...(list ?? [])];
  return next.slice(0, maxSize);
}

/** Deleta uma snapshot pelo id. */
export function deleteSnapshot(
  list: SicroDocSnapshot[] | undefined,
  id: string,
): SicroDocSnapshot[] {
  return (list ?? []).filter((s) => s.id !== id);
}

/** Conta palavras (regex simples) no conteúdo do documento. */
function computeStats(content: JSONContent): { words: number; paragraphs: number } {
  let words = 0;
  let paragraphs = 0;
  const visit = (node: JSONContent) => {
    if (node.type === "paragraph") paragraphs += 1;
    if (node.type === "text" && typeof node.text === "string") {
      const trimmed = node.text.trim();
      if (trimmed) words += trimmed.split(/\s+/).length;
    }
    if (Array.isArray(node.content)) {
      for (const child of node.content) visit(child);
    }
  };
  visit(content);
  return { words, paragraphs };
}

function deepClone<T>(x: T): T {
  // Estratégia simples — JSON parse/stringify. O conteúdo TipTap é JSON
  // serializável por design.
  return JSON.parse(JSON.stringify(x)) as T;
}
