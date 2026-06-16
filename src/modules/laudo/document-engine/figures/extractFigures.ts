/**
 * extractFigures — varre o documento e devolve a lista de figuras
 * ordenadas por aparição.
 *
 * F6 — Suporta 3 fontes:
 *   - `figure` (kind: image/croqui/video_frame)
 *   - `photoPlate` (cada slot vira uma figura numerada)
 *   - `storyboard` (cada item do storyboard pode ser referenciado)
 *
 * Decisão de design: numeração é GLOBAL por documento, mas o `kind`
 * é preservado para o painel poder filtrar (todas / só croquis / só fotos).
 * A renderização final pode optar por separar séries ("Figura 1, 2, 3"
 * para fotos e "Croqui 1, 2" para croquis) ou unificar — fica a critério
 * do renderer/walker via `numberFigures`.
 */

import type { JSONContent } from "@tiptap/core";

export type FigureKind = "image" | "croqui" | "video_frame" | "photoplate";

export interface FigureEntry {
  /** Tipo da figura (mapeia para `figure.attrs.kind` ou node nativo). */
  kind: FigureKind;
  /** Posição ProseMirror do nó pai (para click-to-jump). */
  pos: number;
  /** Legenda exibida na figura ou inferida do photoplate. */
  caption: string;
  /** Quando vem de PhotoPlate: índice da foto dentro da prancha. */
  cellIndex?: number;
  /**
   * Identificador opcional — `evidence_id` ou índice estável dentro do
   * documento. Usado pelo painel para selecionar linhas únicas.
   */
  id?: string;
}

/**
 * Extrai a lista de figuras do documento. Ordem: por aparição
 * (depth-first traversal do JSONContent).
 */
export function extractFigures(content: JSONContent): FigureEntry[] {
  const out: FigureEntry[] = [];
  walk(content, 0, out);
  return out;
}

function walk(node: JSONContent, basePos: number, out: FigureEntry[]): number {
  const type = node.type;
  if (!type) return basePos;

  if (type === "doc") {
    let p = basePos;
    for (const child of node.content ?? []) {
      p = walk(child, p, out);
    }
    return p;
  }

  if (type === "text") {
    return basePos + (node.text?.length ?? 0);
  }

  const openPos = basePos;
  let inner = basePos + 1;

  if (type === "figure") {
    const kind = normalizeFigureKind(node.attrs?.["kind"]);
    out.push({
      kind,
      pos: openPos,
      caption: extractFigureCaption(node),
      ...(typeof node.attrs?.["evidence_id"] === "string"
        ? { id: String(node.attrs["evidence_id"]) }
        : {}),
    });
  } else if (type === "photoPlate") {
    const photos = (node.attrs?.["photos"] as Array<{
      caption?: string;
      evidence_id?: string | null;
    }>) ?? [];
    photos.forEach((p, idx) => {
      out.push({
        kind: "photoplate",
        pos: openPos,
        caption: p.caption ?? "",
        cellIndex: idx,
        ...(p.evidence_id ? { id: p.evidence_id } : {}),
      });
    });
  }

  for (const child of node.content ?? []) {
    inner = walk(child, inner, out);
  }
  return inner + 1;
}

function normalizeFigureKind(raw: unknown): FigureKind {
  if (raw === "croqui") return "croqui";
  if (raw === "video_frame") return "video_frame";
  return "image";
}

function extractFigureCaption(node: JSONContent): string {
  // figcaption é o filho block-level do figure.
  const figcap = (node.content ?? []).find((c) => c.type === "figcaption");
  if (!figcap) return "";
  let buf = "";
  const visit = (n: JSONContent) => {
    if (n.type === "text" && typeof n.text === "string") {
      buf += n.text;
    }
    if (n.content) for (const c of n.content) visit(c);
  };
  visit(figcap);
  return buf.trim();
}
