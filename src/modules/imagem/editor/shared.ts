/** Helpers compartilhados do editor de imagem. */

import { convertFileSrc } from "@tauri-apps/api/core";

export function assetUrl(
  workspacePath: string,
  relativePath: string,
): string | null {
  if (!workspacePath || !relativePath) return null;
  const sep = workspacePath.includes("\\") ? "\\" : "/";
  const trimmed = workspacePath.replace(/[\\/]+$/, "");
  const normRel = sep === "\\"
    ? relativePath.replace(/\//g, "\\")
    : relativePath.replace(/\\/g, "/");
  try {
    return convertFileSrc(`${trimmed}${sep}${normRel}`);
  } catch {
    return null;
  }
}

export function formatDateTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString("pt-BR");
  } catch {
    return iso;
  }
}

export function prettyBytes(n: number | null | undefined): string {
  if (n == null) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function shortHash(h: string | null | undefined, n = 10): string {
  if (!h) return "—";
  return h.length <= n ? h : `${h.slice(0, n)}…`;
}

/** id do <filter> SVG injetado pelo editor para o mix de canais. */
export const CHANNEL_MIX_FILTER_ID = "sicro-channel-mix";

/** Os canais R/G/B estão todos visíveis? (default true quando ausente) */
export function channelsAllOn(adj: {
  channel_r?: boolean;
  channel_g?: boolean;
  channel_b?: boolean;
}): boolean {
  return (
    (adj.channel_r ?? true) &&
    (adj.channel_g ?? true) &&
    (adj.channel_b ?? true)
  );
}

/** Monta o `filter` CSS do preview ao vivo. Ordem igual ao backend Rust
 * (menos gamma, que o CSS não tem): brightness → contrast → saturate →
 * hue-rotate → mix de canais (url do <filter> SVG) → grayscale → invert. */
export function adjustmentsToCssFilter(adj: {
  brightness: number;
  contrast: number;
  gamma: number;
  saturation: number;
  grayscale: boolean;
  invert: boolean;
  hue?: number;
  channel_r?: boolean;
  channel_g?: boolean;
  channel_b?: boolean;
}): string {
  // Ajustes vêm em % (+50 → 1.5).
  const b = 1 + adj.brightness / 100;
  const c = 1 + adj.contrast / 100;
  const s = 1 + adj.saturation / 100;
  const hue = adj.hue ?? 0;
  // Gamma não existe em CSS: o preview ignora; o backend aplica o pow() real na exportação.
  const parts: string[] = [];
  if (Math.abs(b - 1) > 1e-3) parts.push(`brightness(${b.toFixed(2)})`);
  if (Math.abs(c - 1) > 1e-3) parts.push(`contrast(${c.toFixed(2)})`);
  if (Math.abs(s - 1) > 1e-3) parts.push(`saturate(${s.toFixed(2)})`);
  if (Math.abs(hue) > 0.01) parts.push(`hue-rotate(${hue.toFixed(1)}deg)`);
  if (!channelsAllOn(adj)) parts.push(`url(#${CHANNEL_MIX_FILTER_ID})`);
  if (adj.grayscale) parts.push("grayscale(1)");
  if (adj.invert) parts.push("invert(1)");
  return parts.join(" ");
}
