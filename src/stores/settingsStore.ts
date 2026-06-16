/**
 * settingsStore — estado global das Configurações do app (o "cofrinho" que
 * vive fora de qualquer `.sicro`). Carregado uma vez no boot (App.tsx) e
 * aplicado ao documento (tema + cor de destaque). A persistência é via
 * `commands.saveAppSettings` no diretório de config do SO.
 */

import { create } from "zustand";
import { commands } from "@core/commands";
import {
  defaultAppSettings,
  type AppSettings,
  type AppearanceSettings,
} from "@domain/app_settings";

const DEFAULT_ACCENT = "#d7a84f";

function hexToRgb(hex: string): [number, number, number] | null {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  const digits = m?.[1];
  if (!digits) return null;
  const n = parseInt(digits, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function clampByte(n: number): number {
  return Math.max(0, Math.min(255, Math.round(n)));
}

function toHex(r: number, g: number, b: number): string {
  return (
    "#" +
    [r, g, b].map((v) => clampByte(v).toString(16).padStart(2, "0")).join("")
  );
}

function lighten([r, g, b]: [number, number, number], amt: number): string {
  return toHex(r + (255 - r) * amt, g + (255 - g) * amt, b + (255 - b) * amt);
}

/**
 * Aplica tema + cor de destaque no `<html>`. Resolve "auto" via
 * prefers-color-scheme. As vars de accent são setadas inline no
 * documentElement (sobrepõem o `:root` do tokens.css).
 */
export function applyAppearance(appearance: AppearanceSettings): void {
  const root = document.documentElement;

  let theme = appearance.theme;
  if (theme === "auto") {
    const prefersLight =
      window.matchMedia?.("(prefers-color-scheme: light)").matches ?? false;
    theme = prefersLight ? "light" : "dark";
  }
  root.dataset.theme = theme; // "light" | "dark"

  const accent = (appearance.accent || DEFAULT_ACCENT).trim();
  const rgb = hexToRgb(accent);
  if (rgb) {
    const [r, g, b] = rgb;
    root.style.setProperty("--sicro-accent", accent);
    root.style.setProperty("--sicro-accent-hover", lighten(rgb, 0.15));
    root.style.setProperty("--sicro-accent-soft", `rgba(${r}, ${g}, ${b}, 0.15)`);
  } else {
    root.style.removeProperty("--sicro-accent");
    root.style.removeProperty("--sicro-accent-hover");
    root.style.removeProperty("--sicro-accent-soft");
  }
}

interface SettingsState {
  settings: AppSettings;
  loaded: boolean;
  /** Lê do disco e aplica a aparência. Seguro chamar mais de uma vez. */
  load: () => Promise<void>;
  /** Aplica a aparência, atualiza o estado e grava no disco. */
  persist: (next: AppSettings) => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set) => ({
  settings: defaultAppSettings(),
  loaded: false,
  async load() {
    try {
      const s = await commands.getAppSettings();
      applyAppearance(s.appearance);
      set({ settings: s, loaded: true });
    } catch {
      // Mantém defaults; o tema escuro padrão do tokens.css já vale.
      set({ loaded: true });
    }
  },
  async persist(next) {
    applyAppearance(next.appearance);
    set({ settings: next });
    await commands.saveAppSettings(next);
  },
}));
