/**
 * Relative-src bridging (MVP 4).
 *
 * The `.sicrodoc` keeps evidence references portable by storing only the
 * **workspace-relative path** of each asset. But the WebView's <img>
 * element can't fetch a bare relative path — it needs either an absolute
 * URL or the Tauri `tauri://localhost/...` form produced by
 * `convertFileSrc()`.
 *
 * To bridge these two worlds we run two transforms around the editor:
 *
 *   - `resolveEvidenceSrcsForEditor(content, workspacePath)` — called
 *     when content is about to be handed to TipTap. For every node with
 *     a `relative_path` attribute we set `src` to the convertFileSrc'd
 *     absolute path so the editor renders the actual image.
 *
 *   - `normalizeEvidenceSrcsForSave(content)` — called right before
 *     `save_laudo`. For every node with a `relative_path` attribute we
 *     overwrite `src` with `relative_path` (a plain relative path).
 *     That keeps the `.sicrodoc` portable: same file works in another
 *     machine / another workspace folder.
 *
 * Both functions return NEW trees (no in-place mutation) so they're safe
 * to feed into TipTap or to JSON-stringify for save.
 *
 * Nodes without `relative_path` are left untouched — authoring inserts
 * with a placeholder src still work for spike-time scribbles.
 */

import type { JSONContent } from "@tiptap/core";
import { convertFileSrc } from "@tauri-apps/api/core";

export function resolveEvidenceSrcsForEditor(
  content: JSONContent,
  workspacePath: string,
): JSONContent {
  return rewrite(content, (node) => {
    const rel = node.attrs?.["relative_path"] as string | undefined;
    if (!rel || rel.length === 0) return node;
    const abs = joinWorkspace(workspacePath, rel);
    let src: string;
    try {
      src = convertFileSrc(abs);
    } catch {
      // If conversion fails (host outside Tauri runtime), keep the
      // relative path — the <img> will fail to load but the data is
      // preserved.
      src = rel;
    }
    return {
      ...node,
      attrs: { ...node.attrs, src },
    };
  });
}

export function normalizeEvidenceSrcsForSave(
  content: JSONContent,
): JSONContent {
  return rewrite(content, (node) => {
    const rel = node.attrs?.["relative_path"] as string | undefined;
    if (!rel || rel.length === 0) return node;
    return {
      ...node,
      attrs: { ...node.attrs, src: rel },
    };
  });
}

// ---------------------------------------------------------------------------

/** O — Exportado pra ser reusado pelo pipeline de drag-and-drop de fotos.
 *  Cuidado importante: em Windows mistura de `\` e `/` em path absoluto
 *  quebra o asset protocol do Tauri (`convertFileSrc` aceita mas a
 *  resolução do <img> falha). Esta função normaliza tudo pro separador
 *  do host. */
export function joinWorkspace(workspacePath: string, rel: string): string {
  const isWin = workspacePath.includes("\\");
  const sep = isWin ? "\\" : "/";
  const trimmed = workspacePath.replace(/[\\/]+$/, "");
  const normRel = isWin ? rel.replace(/\//g, "\\") : rel.replace(/\\/g, "/");
  return `${trimmed}${sep}${normRel}`;
}

function rewrite(node: JSONContent, fn: (n: JSONContent) => JSONContent): JSONContent {
  const next = fn(node);
  if (!next.content) return next;
  return {
    ...next,
    content: next.content.map((c) => rewrite(c, fn)),
  };
}
