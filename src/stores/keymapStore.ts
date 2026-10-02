/** Overrides de atalho do usuário (id → combinação), em localStorage por máquina. */
import { create } from "zustand";

import { normalizeBinding } from "@core/keymap";
import { ACTION_BY_ID } from "@core/keymapActions";

const LS_KEY = "sicro.keymap.overrides.v1";

function loadOverrides(): Record<string, string> {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (!raw) return {};
    const obj = JSON.parse(raw) as unknown;
    return obj && typeof obj === "object" ? (obj as Record<string, string>) : {};
  } catch {
    return {};
  }
}

function saveOverrides(o: Record<string, string>): void {
  try {
    localStorage.setItem(LS_KEY, JSON.stringify(o));
  } catch {
    /* localStorage indisponível — segue só em memória */
  }
}

/** Override do usuário ?? padrão do catálogo. */
export function resolveBinding(
  overrides: Record<string, string>,
  id: string,
): string {
  const o = overrides[id];
  if (o !== undefined) return o;
  return ACTION_BY_ID[id]?.defaultBinding ?? "";
}

interface KeymapState {
  overrides: Record<string, string>;
  setBinding: (id: string, binding: string) => void;
  resetBinding: (id: string) => void;
  resetAll: () => void;
  /** Leitura não reativa (handlers); a UI usa o seletor `overrides`. */
  binding: (id: string) => string;
}

export const useKeymapStore = create<KeymapState>((set, get) => ({
  overrides: loadOverrides(),
  setBinding: (id, binding) =>
    set((s) => {
      const next = { ...s.overrides, [id]: normalizeBinding(binding) };
      saveOverrides(next);
      return { overrides: next };
    }),
  resetBinding: (id) =>
    set((s) => {
      if (!(id in s.overrides)) return s;
      const next = { ...s.overrides };
      delete next[id];
      saveOverrides(next);
      return { overrides: next };
    }),
  resetAll: () => {
    saveOverrides({});
    set({ overrides: {} });
  },
  binding: (id) => resolveBinding(get().overrides, id),
}));
