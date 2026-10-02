/**
 * Guard global de navegação: o editor com trabalho não salvo registra um
 * `guard()` async; os links chamam `attemptNavigation` e só navegam se ele
 * resolver `true`. Um guard por vez (um editor aberto por vez).
 */

import { create } from "zustand";

type NavGuardCallback = () => Promise<boolean>;

interface NavGuardState {
  guard: NavGuardCallback | null;
  /** Substitui o anterior. */
  register: (cb: NavGuardCallback) => void;
  unregister: () => void;
  /** Sem guard registrado, navega direto. */
  attemptNavigation: (target: () => void) => Promise<void>;
}

export const useNavGuard = create<NavGuardState>((set, get) => ({
  guard: null,
  register(cb) {
    set({ guard: cb });
  },
  unregister() {
    set({ guard: null });
  },
  async attemptNavigation(target) {
    const guard = get().guard;
    if (!guard) {
      target();
      return;
    }
    const proceed = await guard();
    if (proceed) target();
  },
}));
