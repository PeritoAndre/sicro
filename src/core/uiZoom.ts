/**
 * Zoom da interface inteira via `setZoom` do webview (WebView2 / WebKitGTK).
 * Usa Ctrl+Shift+=/-/0 para não colidir com o zoom de conteúdo dos módulos (Ctrl+=/-/0).
 */

import { getCurrentWebview } from "@tauri-apps/api/webview";

export const UI_ZOOM_DEFAULT = 1;

const UI_ZOOM_LEVELS = [0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];

const EPS = 0.001;
const MIN = UI_ZOOM_LEVELS[0] ?? 0.75;
const MAX = UI_ZOOM_LEVELS[UI_ZOOM_LEVELS.length - 1] ?? 2;

let current = UI_ZOOM_DEFAULT;

/** Valor inválido/corrompido → 100%. */
export function clampUiZoom(z: unknown): number {
  if (typeof z !== "number" || !Number.isFinite(z) || z <= 0) return UI_ZOOM_DEFAULT;
  return Math.min(MAX, Math.max(MIN, Math.round(z * 100) / 100));
}

export function stepUiZoom(z: number, dir: 1 | -1): number {
  if (dir > 0) return UI_ZOOM_LEVELS.find((l) => l > z + EPS) ?? MAX;
  return [...UI_ZOOM_LEVELS].reverse().find((l) => l < z - EPS) ?? MIN;
}

/** Para converter CSS px em px lógicos da janela. */
export function getUiZoom(): number {
  return current;
}

/** Fora do Tauri (testes/navegador) só guarda o valor. */
export function applyUiZoom(z: unknown): void {
  current = clampUiZoom(z);
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  try {
    getCurrentWebview()
      .setZoom(current)
      .catch(() => {});
  } catch {
    /* sem webview (ex.: janela auxiliar) — ignora */
  }
}
