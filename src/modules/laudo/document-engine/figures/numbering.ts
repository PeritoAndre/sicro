/**
 * numberFigures — atribui números automáticos às figuras.
 *
 * F6 — Estratégia padrão (`mode: "by-kind"`): cada `kind` tem seu próprio
 * contador. Convenção pericial brasileira:
 *
 *   - `image` / `photoplate` → "Figura 1", "Figura 2", "Figura 3"…
 *   - `croqui`              → "Croqui 1", "Croqui 2"…
 *   - `video_frame`         → "Frame 1", "Frame 2"…
 *
 * O contador é sequencial por aparição no documento.
 *
 * Modo alternativo (`mode: "unified"`): tudo conta como "Figura N" em
 * série única. Útil para laudos que tratam todos os elementos visuais
 * indistintamente.
 *
 * Função pura — não modifica o documento.
 */

import type { FigureEntry, FigureKind } from "./extractFigures";

export type NumberingMode = "by-kind" | "unified";

export interface NumberedFigureEntry extends FigureEntry {
  /** Número formatado: "Figura 1", "Croqui 2", "Frame 1". */
  label: string;
  /** Apenas o número (para casos onde caller monta o label). */
  ordinal: number;
}

export function buildFigureList(
  figures: ReadonlyArray<FigureEntry>,
  mode: NumberingMode = "by-kind",
): NumberedFigureEntry[] {
  const counters = new Map<string, number>();
  return figures.map((entry) => {
    const bucket = mode === "unified" ? "all" : entry.kind;
    const next = (counters.get(bucket) ?? 0) + 1;
    counters.set(bucket, next);
    const prefix =
      mode === "unified"
        ? "Figura"
        : labelPrefix(entry.kind);
    return {
      ...entry,
      ordinal: next,
      label: `${prefix} ${next}`,
    };
  });
}

function labelPrefix(kind: FigureKind): string {
  switch (kind) {
    case "croqui":
      return "Croqui";
    case "video_frame":
      return "Frame";
    case "image":
    case "photoplate":
    default:
      return "Figura";
  }
}
