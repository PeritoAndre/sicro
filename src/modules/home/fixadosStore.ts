/**
 * Casos fixados no topo da lista. O índice de casos é gravado pelo backend, que
 * só conhece os campos que ele declara; por isso o "fixado" mora aqui, por máquina.
 */

import { create } from "zustand";

const KEY = "sicro.casosFixados.v1";

function ler(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "[]") as unknown;
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

function gravar(ids: string[]): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(ids));
  } catch {
    /* só não persiste */
  }
}

interface FixadosState {
  ids: string[];
  alternar: (workspaceId: string) => void;
}

export const useFixados = create<FixadosState>((set, get) => ({
  ids: ler(),
  alternar(id) {
    const atual = get().ids;
    const ids = atual.includes(id) ? atual.filter((x) => x !== id) : [...atual, id];
    gravar(ids);
    set({ ids });
  },
}));
