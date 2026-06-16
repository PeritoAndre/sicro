/**
 * Comments service — operações puras sobre a coleção `SicroDoc.comments`.
 *
 * F8 — Não toca o conteúdo TipTap diretamente; o caller é responsável
 * por aplicar/remover o `commentMark` via `editor.commands.addComment` /
 * `removeComment` em paralelo.
 *
 * Convenção:
 *   - Funções nunca mutam: retornam um novo array.
 *   - IDs são UUID v4 (gerados via crypto.randomUUID quando disponível).
 *   - `created_at` é ISO 8601 (toISOString).
 */

import type { JSONContent } from "@tiptap/core";
import type {
  SicroDocComment,
  SicroDocCommentReply,
} from "../schema";

function uuid(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return crypto.randomUUID();
  }
  // Fallback determinístico-ish para ambientes sem crypto (node antigo / SSR).
  return `cmt-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e9).toString(36)}`;
}

/** Cria um comentário novo (não aplica o mark). */
export function createComment(opts: {
  author: string;
  body: string;
}): SicroDocComment {
  return {
    id: uuid(),
    author: opts.author,
    body: opts.body,
    created_at: new Date().toISOString(),
    resolved: false,
    replies: [],
  };
}

/** Adiciona um comentário à lista (imutável). */
export function addComment(
  list: SicroDocComment[] | undefined,
  comment: SicroDocComment,
): SicroDocComment[] {
  return [...(list ?? []), comment];
}

/** Atualiza o corpo de um comentário pelo id. */
export function updateComment(
  list: SicroDocComment[] | undefined,
  id: string,
  patch: Partial<Pick<SicroDocComment, "body" | "resolved">>,
): SicroDocComment[] {
  return (list ?? []).map((c) => (c.id === id ? { ...c, ...patch } : c));
}

/** Marca um comentário como resolvido. */
export function resolveComment(
  list: SicroDocComment[] | undefined,
  id: string,
): SicroDocComment[] {
  return updateComment(list, id, { resolved: true });
}

/** Desfaz "resolvido". */
export function unresolveComment(
  list: SicroDocComment[] | undefined,
  id: string,
): SicroDocComment[] {
  return updateComment(list, id, { resolved: false });
}

/** Remove um comentário da lista. O caller também deve `removeComment(id)` no editor. */
export function deleteComment(
  list: SicroDocComment[] | undefined,
  id: string,
): SicroDocComment[] {
  return (list ?? []).filter((c) => c.id !== id);
}

/** Adiciona uma resposta a um comentário existente. */
export function addReply(
  list: SicroDocComment[] | undefined,
  id: string,
  reply: { author: string; body: string },
): SicroDocComment[] {
  const newReply: SicroDocCommentReply = {
    id: uuid(),
    author: reply.author,
    body: reply.body,
    created_at: new Date().toISOString(),
  };
  return (list ?? []).map((c) =>
    c.id === id
      ? { ...c, replies: [...(c.replies ?? []), newReply] }
      : c,
  );
}

/** Conta os comentários NÃO resolvidos (badge). */
export function countActiveComments(
  list: SicroDocComment[] | undefined,
): number {
  return (list ?? []).filter((c) => !c.resolved).length;
}

// ---------------------------------------------------------------------------
// Extração de anchors do conteúdo TipTap.

export interface CommentAnchorInfo {
  id: string;
  /** Posição no documento TipTap (útil para `setTextSelection` + scroll). */
  pos: number;
  /** Trecho de texto (limitado a 80 chars) que está coberto pela marca. */
  excerpt: string;
}

/** Varre o doc TipTap e retorna todos os anchors de comentário encontrados. */
export function extractCommentAnchors(
  doc: JSONContent | null | undefined,
): CommentAnchorInfo[] {
  if (!doc) return [];
  const out: CommentAnchorInfo[] = [];
  let pos = 0;

  const visit = (node: JSONContent) => {
    if (node.type === "text" && Array.isArray(node.marks)) {
      const m = node.marks.find((mk) => mk.type === "commentMark");
      if (m && typeof m.attrs?.["id"] === "string") {
        const id = m.attrs["id"] as string;
        const text = typeof node.text === "string" ? node.text : "";
        // Se o mesmo id já existe (cobre múltiplos text-runs), concatena.
        const existing = out.find((o) => o.id === id);
        if (existing) {
          existing.excerpt = (existing.excerpt + text).slice(0, 80);
        } else {
          out.push({
            id,
            pos,
            excerpt: text.slice(0, 80),
          });
        }
      }
    }
    if (node.type === "text" && typeof node.text === "string") {
      pos += node.text.length;
    } else {
      pos += 1;
    }
    if (Array.isArray(node.content)) {
      for (const child of node.content) visit(child);
    }
    if (node.type !== "text") {
      pos += 1;
    }
  };

  visit(doc);
  return out;
}
