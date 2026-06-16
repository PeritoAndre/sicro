/**
 * Page layout resolution.
 *
 * Margins can come from two places:
 *   1. The active institutional template (`InstitutionalTemplate.page.margins`)
 *      — the institutional default.
 *   2. The SicroDoc's own `layout.page.margins` — per-laudo override that
 *      the user set via the Inspector "Página" tab.
 *
 * `resolveEffectiveMargins()` is the single source of truth: the editor,
 * the HTML/PDF renderer and the DOCX walker MUST go through it so the
 * three surfaces always agree on the page geometry.
 */

import type { InstitutionalTemplate } from "./institutional-templates";
import type { SicroDoc, SicroDocPageMargins } from "./schema";

/** SICRO default if nothing else is provided (matches the original CSS). */
export const DEFAULT_PAGE_MARGINS: SicroDocPageMargins = {
  top: "3cm",
  right: "2cm",
  bottom: "2.5cm",
  left: "3.5cm",
};

/** Page dimensions for A4 portrait (always — landscape is out of scope here). */
export const A4_PAGE = {
  widthCm: 21,
  heightCm: 29.7,
} as const;

/** Resolve the margins that actually apply to a given doc + template. */
export function resolveEffectiveMargins(
  doc: SicroDoc | null,
  template: InstitutionalTemplate | null,
): SicroDocPageMargins {
  const docOverride = doc?.layout?.page?.margins;
  if (docOverride && isCompleteMargins(docOverride)) {
    return normalize(docOverride);
  }
  if (template?.page.margins) {
    return normalize(template.page.margins);
  }
  return DEFAULT_PAGE_MARGINS;
}

/** Parse a CSS length string ("3cm", "30mm", "12pt") into centimetres.
 *  Falls back to 0 if the input is unparseable — caller decides what to do. */
export function parseLengthCm(value: string | null | undefined): number {
  if (!value) return 0;
  const trimmed = value.trim().toLowerCase();
  const match = trimmed.match(/^(-?\d+(?:\.\d+)?)\s*(cm|mm|pt|in|px)?$/);
  if (!match) return 0;
  const n = Number(match[1]);
  if (Number.isNaN(n)) return 0;
  const unit = (match[2] ?? "cm") as "cm" | "mm" | "pt" | "in" | "px";
  switch (unit) {
    case "cm":
      return n;
    case "mm":
      return n / 10;
    case "pt":
      return n / 28.3464567;
    case "in":
      return n * 2.54;
    case "px":
      return (n * 2.54) / 96;
  }
}

/** Format a centimetre value back to a normalized CSS string with cm. */
export function formatCm(value: number, decimals = 2): string {
  return `${roundTo(value, decimals)}cm`;
}

function roundTo(value: number, decimals: number): number {
  const factor = Math.pow(10, decimals);
  return Math.round(value * factor) / factor;
}

function isCompleteMargins(m: Partial<SicroDocPageMargins>): m is SicroDocPageMargins {
  return Boolean(m.top && m.right && m.bottom && m.left);
}

function normalize(m: SicroDocPageMargins): SicroDocPageMargins {
  // Pass through; the parser/formatter tolerates whatever shape comes in.
  return {
    top: m.top,
    right: m.right,
    bottom: m.bottom,
    left: m.left,
  };
}

/** Convenience helper for code that needs everything in centimetres at once. */
export function marginsInCm(m: SicroDocPageMargins): {
  top: number;
  right: number;
  bottom: number;
  left: number;
} {
  return {
    top: parseLengthCm(m.top),
    right: parseLengthCm(m.right),
    bottom: parseLengthCm(m.bottom),
    left: parseLengthCm(m.left),
  };
}
