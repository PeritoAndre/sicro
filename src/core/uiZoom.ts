/**
 * Zoom da interface inteira (o "tamanho" do SICRO na tela). Aplicado no
 * webview principal via `setZoom` do Tauri (WebView2 / WebKitGTK), então
 * vale para tudo — textos, ícones, painéis — sem mexer no CSS dos módulos.
 *
 * Não confundir com o zoom dos módulos (croqui, imagem, laudo), que usa
 * Ctrl + = / - / 0 e só amplia o conteúdo em edição. O da interface usa
 * Ctrl + Shift + = / - / 0 (escopo "geral" do keymap).
 *
 * §13: preferência de UI — não altera nenhuma lógica forense.
 */

import { getCurrentWebview } from "@tauri-apps/api/webview";

export const UI_ZOOM_DEFAULT = 1;

/** Degraus do zoom (os atalhos andam de um em um, como nos navegadores). */
export const UI_ZOOM_LEVELS = [0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];

const EPS = 0.001;
const MIN = UI_ZOOM_LEVELS[0] ?? 0.75;
const MAX = UI_ZOOM_LEVELS[UI_ZOOM_LEVELS.length - 1] ?? 2;

let current = UI_ZOOM_DEFAULT;

/** Garante um número válido dentro da faixa (valores antigos/corrompidos → 100%). */
export function clampUiZoom(z: unknown): number {
  if (typeof z !== "number" || !Number.isFinite(z) || z <= 0) return UI_ZOOM_DEFAULT;
  return Math.min(MAX, Math.max(MIN, Math.round(z * 100) / 100));
}

/** Próximo degrau acima (`dir = 1`) ou abaixo (`dir = -1`) do zoom `z`. */
export function stepUiZoom(z: number, dir: 1 | -1): number {
  if (dir > 0) return UI_ZOOM_LEVELS.find((l) => l > z + EPS) ?? MAX;
  return [...UI_ZOOM_LEVELS].reverse().find((l) => l < z - EPS) ?? MIN;
}

/** Zoom atualmente aplicado — para converter CSS px em px lógicos da janela. */
export function getUiZoom(): number {
  return current;
}

/** Aplica o zoom no webview principal. Fora do Tauri (testes/navegador) só guarda. */
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
