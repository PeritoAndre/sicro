/**
 * Evidence asset loader (MVP 4).
 *
 * The `.sicrodoc` references evidence images by their workspace-relative
 * path (`figure.attrs.relative_path`, `storyboardItem.attrs.relative_path`).
 * For HTML/PDF export we need to inline those as data URIs because the
 * exported HTML lives in `<iframe srcdoc>` / headless Edge, neither of
 * which can fetch `tauri://localhost` asset URLs reliably.
 *
 * The loader walks a `SicroDoc.content` JSON tree, collects every
 * `relative_path`, and asks the Rust backend for the bytes.
 */

import type { JSONContent } from "@tiptap/core";
import { commands } from "@core/commands";

export interface EvidenceAssetMap {
  /** key = workspace-relative path; value = full data: URI ready to drop in <img src>. */
  byRelativePath: Record<string, string>;
}

/**
 * Walk the doc tree and gather every figure/storyboardItem relative_path
 * referenced. Returns a Set so duplicates are loaded once.
 */
export function collectEvidencePaths(content: JSONContent): Set<string> {
  const out = new Set<string>();
  walk(content, (node) => {
    const rel =
      (node.attrs?.relative_path as string | undefined) ??
      (node.attrs?.["relative_path"] as string | undefined);
    if (typeof rel === "string" && rel.length > 0) {
      out.add(rel);
    }
  });
  return out;
}

/**
 * Resolve every relative_path → data URI via the Tauri backend. Best
 * effort: when a single asset fails (deleted file, permission), we log a
 * warning and leave the entry missing — the renderer will fall back to a
 * placeholder.
 */
export async function loadEvidenceAssets(
  workspacePath: string,
  paths: Iterable<string>,
): Promise<EvidenceAssetMap> {
  const byRelativePath: Record<string, string> = {};
  for (const rel of paths) {
    try {
      const asset = await commands.readEvidenceAsset(workspacePath, rel);
      byRelativePath[rel] = `data:${asset.mime_type};base64,${asset.base64}`;
    } catch (err) {
      // eslint-disable-next-line no-console
      console.warn(
        `[laudo] evidence asset ${rel} unavailable: ${(err as Error).message}`,
      );
    }
  }
  return { byRelativePath };
}

/**
 * Return a NEW content tree with every `figure` / `storyboardItem` whose
 * `relative_path` exists in `assets` having its `src` replaced by the
 * corresponding data URI. Nodes without a `relative_path` (free text,
 * authoring) are left untouched.
 */
export function inlineEvidenceAssets(
  content: JSONContent,
  assets: EvidenceAssetMap,
): JSONContent {
  return rewrite(content, (node) => {
    const rel = node.attrs?.relative_path as string | undefined;
    if (!rel) return node;
    const dataUri = assets.byRelativePath[rel];
    if (!dataUri) return node;
    return {
      ...node,
      attrs: { ...node.attrs, src: dataUri },
    };
  });
}

/**
 * Como `inlineEvidenceAssets`, mas TORNA o nó PORTÁTIL: além de setar `src`
 * para o data URI, REMOVE `relative_path` (e o vínculo de evidência) — a imagem
 * deixa de depender da pasta de UM caso e passa a viver EMBUTIDA no próprio nó.
 *
 * Uso: ao salvar um cabeçalho como MODELO GLOBAL ("Cabeçalhos Salvos"). Assim o
 * brasão viaja junto do modelo e funciona em qualquer laudo / caso / máquina
 * (inclusive de colegas de outros setores). Nós sem `relative_path` resolvido em
 * `assets` ficam intactos. Não muta a árvore original.
 *
 * Por que remover `relative_path`: nós SEM ele são ignorados por
 * `normalizeEvidenceSrcsForSave`/`resolveEvidenceSrcsForEditor`, então o data
 * URI sobrevive ao roundtrip de save/load e é renderizado direto pelo editor,
 * pelo HTML/PDF e pelo DOCX (que decodifica o data URI). Como brasão é "chrome"
 * institucional (não evidência do caso), também zeramos evidence_id/hash pra não
 * confundir o verificador de integridade quando aplicado a outro laudo.
 */
export function embedAssetsAsPortable(
  content: JSONContent,
  assets: EvidenceAssetMap,
): JSONContent {
  return rewrite(content, (node) => {
    const rel = node.attrs?.relative_path as string | undefined;
    if (!rel) return node;
    const dataUri = assets.byRelativePath[rel];
    if (!dataUri) return node;
    const attrs = { ...node.attrs, src: dataUri };
    delete (attrs as Record<string, unknown>)["relative_path"];
    delete (attrs as Record<string, unknown>)["evidence_id"];
    delete (attrs as Record<string, unknown>)["source_hash"];
    return { ...node, attrs };
  });
}

// ---------------------------------------------------------------------------

type Visitor = (node: JSONContent) => void;
function walk(node: JSONContent, visit: Visitor): void {
  visit(node);
  for (const child of node.content ?? []) {
    walk(child, visit);
  }
}

type Rewriter = (node: JSONContent) => JSONContent;
function rewrite(node: JSONContent, fn: Rewriter): JSONContent {
  const next = fn(node);
  if (!next.content) return next;
  return {
    ...next,
    content: next.content.map((c) => rewrite(c, fn)),
  };
}
